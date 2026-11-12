//! Native adapter. Core state owns decisions; UI snapshots are coalesced.
use crate::singleton::Action;
use nubi_core::host::{with, Host, Shared};
use nubi_core::protocol::{BrowserMsg, BrowserReply, HookMsg, Reply};
use nubi_core::state::{now_ms, Alert, Inner, Missed};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender, TrySendError},
    Arc, Mutex,
};
use tokio::sync::oneshot;

#[derive(Clone)]
pub enum UiEvent {
    Activate(Action),
    LauncherReady(Result<String, String>),
    Alert(Alert),
    Update,
}

#[derive(Clone)]
pub struct NativeHost {
    shared: Shared,
    sender: SyncSender<UiEvent>,
    dirty: Arc<AtomicBool>,
    pub updates: Arc<Mutex<crate::updater::Inner>>,
}

impl NativeHost {
    pub fn new() -> (Self, Receiver<UiEvent>) {
        Self::from_settings(nubi_core::settings::load())
    }

    pub fn from_settings(settings: nubi_core::settings::Settings) -> (Self, Receiver<UiEvent>) {
        let (sender, receiver) = mpsc::sync_channel(64);
        let shared = Shared::default();
        shared.0.lock().unwrap().settings = settings;
        (
            Self {
                shared,
                sender,
                dirty: Arc::new(AtomicBool::new(true)),
                updates: Arc::default(),
            },
            receiver,
        )
    }

    pub fn post(&self, event: UiEvent) -> Result<(), &'static str> {
        self.sender.try_send(event).map_err(|error| match error {
            TrySendError::Full(_) => "UI event queue is full",
            TrySendError::Disconnected(_) => "UI event loop stopped",
        })
    }
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::AcqRel)
    }
    pub fn toggle_dnd(&self) {
        with(self, |state| state.dnd_manual = !state.dnd_manual);
        self.emit_state();
    }

    pub fn save_settings(&self, settings: nubi_core::settings::Settings) -> Result<(), String> {
        nubi_core::desktime::validate_settings(&settings)?;
        nubi_core::settings::save(&settings)?;
        with(self, |state| state.settings = settings);
        nubi_core::poller::kick(self);
        self.emit_state();
        Ok(())
    }

    pub fn alert(&self, mut alert: Alert) {
        let (dnd, settings) = with(self, |state| (dnd_active(state), state.settings.clone()));
        alert.queued = dnd;
        with(self, |state| state.push_alert(alert.clone()));
        self.emit_state();
        if dnd {
            return;
        }
        if settings.toast_when_fullscreen && nubi_core::winutil::fullscreen_active() {
            if settings.sounds {
                nubi_core::winutil::beep(&alert.kind);
            }
            let title = format!(
                "{} · {}",
                if alert.provider == "codex" {
                    "Codex"
                } else {
                    "Claude"
                },
                alert.title
            );
            // Installer supplies the production AUMID. Development never registers one.
            if let Err(error) = crate::notifications::show(&title, &alert.body) {
                eprintln!("[nubi] toast: {error}");
            }
        } else {
            let _ = self.post(UiEvent::Alert(alert));
        }
    }
}

pub fn dnd_active(state: &Inner) -> bool {
    if state.dnd_manual {
        return true;
    }
    if let (Some(start), Some(end)) = (state.settings.quiet_start, state.settings.quiet_end) {
        use chrono::Timelike;
        let hour = chrono::Local::now().hour() as u8;
        if if start <= end {
            hour >= start && hour < end
        } else {
            hour >= start || hour < end
        } {
            return true;
        }
    }
    state.settings.follow_quiet_time && nubi_core::winutil::quiet_time_active()
}

impl Host for NativeHost {
    fn shared(&self) -> &Shared {
        &self.shared
    }
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").into()
    }
    fn emit_state(&self) {
        self.dirty.store(true, Ordering::Release);
    }

    fn on_hook(&self, message: &HookMsg) -> bool {
        if message.event == "StatusLine" {
            if let Some(input) = &message.tool_input {
                let windows = nubi_core::usage::parse_statusline(input);
                if !windows.is_empty() {
                    with(self, |state| {
                        state.claude_live_ms = now_ms();
                        state.usage.insert(
                            "claude".into(),
                            nubi_core::state::Usage {
                                provider: "claude".into(),
                                status: "ok".into(),
                                message: None,
                                windows,
                                fetched_ms: Some(now_ms()),
                            },
                        );
                    });
                    self.emit_state();
                }
            }
            return false;
        }
        let terminal = nubi_core::protocol::is_question(
            &message.provider,
            &message.event,
            message.tool_name.as_deref(),
        ) && nubi_core::protocol::question_in_terminal(
            message,
            nubi_core::winutil::pids_focused(&message.pids),
        );
        let (alert, wanted) = with(self, |state| {
            (
                state.on_event(message),
                state.settings.wants(&message.provider),
            )
        });
        let key = format!("{}:{}", message.provider, message.session_id);
        with(self, |state| {
            if let Some(target) = state
                .sessions
                .get(&key)
                .and_then(|session| session.action_target.clone())
            {
                for session in state
                    .sessions
                    .values_mut()
                    .filter(|session| session.key != key)
                {
                    if session.action_target.as_ref() == Some(&target) {
                        session.action_target = None;
                        session.compact_queued = false;
                    }
                }
            }
        });
        if let (Some(alert), true) = (alert, wanted) {
            if terminal {
                with(self, |state| state.push_alert(alert));
            } else {
                self.alert(alert);
            }
        }
        self.emit_state();
        terminal
    }

    fn on_browser(&self, message: BrowserMsg) -> BrowserReply {
        if let Err(error) = nubi_core::protocol::validate_browser_msg(&message) {
            return BrowserReply::result(Err(error));
        }
        // Stage 7 owns the on-demand WebView2 profile and cookie import. Never acknowledge
        // a session that this build cannot apply, or persist cookie values elsewhere.
        BrowserReply::result(Err(
            "DeskTime integration is unavailable in this native build.".into(),
        ))
    }

    fn add_pending(&self, message: &HookMsg, reply: oneshot::Sender<Reply>, ttl_ms: u64) -> u64 {
        let id = with(self, |state| state.add_pending(message, reply, ttl_ms));
        self.emit_state();
        id
    }

    fn pending_tick(&self, id: u64, step_ms: u64, hard_ms: u64) -> bool {
        let now = now_ms();
        let (expired, moved) = with(self, |state| match state.pending.get_mut(&id) {
            Some(pending) => {
                let moved = pending.held && pending.expires_ms < hard_ms;
                if moved {
                    pending.expires_ms = pending.expires_ms.saturating_add(step_ms).min(hard_ms);
                }
                (now >= pending.expires_ms, moved)
            }
            None => (true, false),
        });
        if moved && now % 1000 < step_ms {
            self.emit_state();
        }
        expired
    }

    fn remove_pending(&self, id: u64, timed_out: bool) {
        with(self, |state| {
            if let Some(pending) = state.pending.remove(&id) {
                if timed_out {
                    state.missed.push(Missed {
                        id: pending.id,
                        kind: pending.kind,
                        provider: pending.provider,
                        session_key: pending.session_key,
                        folder: pending.folder,
                        name: pending.name,
                        created_ms: pending.created_ms,
                    });
                }
            }
        });
        self.emit_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_decisions_survive_coalesced_ui_updates_and_timeout_moves_to_terminal() {
        let (host, _events) = NativeHost::from_settings(Default::default());
        let message = HookMsg {
            provider: "claude".into(),
            session_id: "test".into(),
            event: "PermissionRequest".into(),
            ..Default::default()
        };
        let (tx, mut rx) = oneshot::channel();
        let id = host.add_pending(&message, tx, 1000);
        with(&host, |state| {
            state.pending.get_mut(&id).unwrap().held = true
        });
        let expiry = with(&host, |state| state.pending[&id].expires_ms);
        host.pending_tick(id, 250, expiry + 100);
        assert_eq!(
            with(&host, |state| state.pending[&id].expires_ms),
            expiry + 100
        );
        with(&host, |state| {
            state.answer(
                id,
                Reply {
                    decision: "allow".into(),
                    message: None,
                    answers: None,
                },
            )
        })
        .unwrap();
        assert_eq!(rx.try_recv().unwrap().decision, "allow");
        host.remove_pending(id, false);
        let (tx, _rx) = oneshot::channel();
        let id = host.add_pending(&message, tx, 1000);
        host.remove_pending(id, true);
        assert_eq!(with(&host, |state| state.missed[0].id), id);
    }
}

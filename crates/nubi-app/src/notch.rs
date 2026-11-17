//! Live island state and ordered core decisions. UI drafts survive snapshots.
mod actions;
mod models;
#[cfg(test)]
mod tests;
use crate::{
    host::NativeHost, DeskSection, NotchData, NotchProvider, NotchRequest, NotchSession,
    NotchWindow, Theme,
};
use models::*;
use nubi_core::{
    host::{with, Host},
    protocol::Reply,
    state::{now_ms, Alert, Inner},
};
use slint::{ComponentHandle, Model, VecModel};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    rc::Rc,
    time::{Duration, Instant},
};

pub struct Notch {
    host: NativeHost,
    sessions: Rc<VecModel<NotchSession>>,
    providers: Rc<VecModel<NotchProvider>>,
    requests: Rc<VecModel<NotchRequest>>,
    desk: Rc<VecModel<DeskSection>>,
    drafts: BTreeMap<u64, Draft>,
    seen: BTreeSet<u64>,
    pub hovering: bool,
    pub clicked: bool,
    pub dragging: bool,
    fold_since: Instant,
    popup: Option<Instant>,
    alerts: VecDeque<Alert>,
    alert: Option<(Alert, Instant)>,
    alert_gap: Option<Instant>,
    delayed: Vec<(Alert, Instant)>,
    refresh: Option<(Instant, u64)>,
    refresh_error: String,
    pub drag_requested: bool,
}

impl Notch {
    pub fn connect(ui: &NotchWindow, host: NativeHost) -> Rc<RefCell<Self>> {
        let this = Rc::new(RefCell::new(Self {
            host,
            sessions: Rc::default(),
            providers: Rc::default(),
            requests: Rc::default(),
            desk: Rc::default(),
            drafts: BTreeMap::new(),
            seen: BTreeSet::new(),
            hovering: false,
            clicked: false,
            dragging: false,
            fold_since: Instant::now(),
            popup: None,
            alerts: VecDeque::new(),
            alert: None,
            alert_gap: None,
            delayed: Vec::new(),
            refresh: None,
            refresh_error: String::new(),
            drag_requested: false,
        }));
        let data = ui.global::<NotchData>();
        data.set_sessions(this.borrow().sessions.clone().into());
        data.set_providers(this.borrow().providers.clone().into());
        data.set_requests(this.borrow().requests.clone().into());
        data.set_desk_sections(this.borrow().desk.clone().into());
        let owner = this.clone();
        let weak = ui.as_weak();
        data.on_answer(move |id, decision| {
            if let (Ok(id), Some(ui)) = (id.parse(), weak.upgrade()) {
                let mut this = owner.borrow_mut();
                this.answer(id, &decision, None, &ui);
                this.update(&ui);
            }
        });
        let owner = this.clone();
        let weak = ui.as_weak();
        data.on_choose(move |id, index| {
            if let (Ok(id), Some(ui)) = (id.parse(), weak.upgrade()) {
                let mut this = owner.borrow_mut();
                let host = this.host.clone();
                let advance = with(&host, |state| {
                    let Some(pending) = state.pending.get(&id) else {
                        return false;
                    };
                    let draft = this.drafts.entry(id).or_default();
                    let question = draft.question(pending);
                    let Some(label) = question["options"][index.max(0) as usize]["label"].as_str()
                    else {
                        return false;
                    };
                    let multi = question["multiSelect"].as_bool().unwrap_or(false);
                    let key = question["question"].as_str().unwrap_or("").to_owned();
                    let selected = draft.answers.entry(key).or_default();
                    if !multi {
                        selected.clear();
                    }
                    if selected.iter().any(|item| item == label) {
                        selected.retain(|item| item != label);
                    } else {
                        selected.push(label.into());
                    }
                    !multi
                });
                if advance {
                    this.advance(id, &ui);
                }
                this.update(&ui);
            }
        });
        let owner = this.clone();
        let weak = ui.as_weak();
        data.on_custom(move |id, text| {
            if let Ok(id) = id.parse::<u64>() {
                let mut this = owner.borrow_mut();
                let host = this.host.clone();
                with(&host, |state| {
                    if let Some(pending) = state.pending.get(&id) {
                        let draft = this.drafts.entry(id).or_default();
                        let question = draft.question(pending);
                        let key = question["question"].as_str().unwrap_or("").to_owned();
                        let multi = question["multiSelect"].as_bool().unwrap_or(false);
                        let options = question["options"].as_array();
                        let selected = draft.answers.entry(key).or_default();
                        selected.retain(|label| {
                            options.is_some_and(|items| {
                                items
                                    .iter()
                                    .any(|option| option["label"].as_str() == Some(label))
                            })
                        });
                        if !text.trim().is_empty() {
                            if !multi {
                                selected.clear();
                            }
                            selected.push(text.trim().into());
                        }
                        draft.custom = text.into();
                    }
                });
                if let Some(ui) = weak.upgrade() {
                    this.update(&ui);
                }
            }
        });
        let owner = this.clone();
        let weak = ui.as_weak();
        data.on_submit(move |id| {
            if let (Ok(id), Some(ui)) = (id.parse(), weak.upgrade()) {
                let mut this = owner.borrow_mut();
                this.advance(id, &ui);
                this.update(&ui);
            }
        });
        let host = this.borrow().host.clone();
        data.on_hold(move |id, held| {
            if let Ok(id) = id.parse::<u64>() {
                with(&host, |state| {
                    if let Some(pending) = state.pending.get_mut(&id) {
                        pending.held = held;
                    }
                });
                host.emit_state();
            }
        });
        let host = this.borrow().host.clone();
        let weak = ui.as_weak();
        data.on_terminal(move |key| {
            let key = if key.is_empty() {
                with(&host, |state| {
                    state
                        .missed
                        .iter()
                        .find(|item| state.settings.wants(&item.provider))
                        .map(|item| item.session_key.clone())
                        .unwrap_or_default()
                })
            } else {
                key.into()
            };
            if let Err(error) = actions::session_action(&host, &key, "jump") {
                with(&host, |state| {
                    if let Some(session) = state.sessions.get_mut(&key) {
                        session.action_error = Some(error.clone());
                    }
                });
                if let Some(ui) = weak.upgrade() {
                    ui.global::<NotchData>().set_request_error(error.into());
                }
                host.emit_state();
            }
        });
        let host = this.borrow().host.clone();
        data.on_dismiss(move |id| {
            let id = if id.is_empty() {
                None
            } else {
                let Ok(id) = id.parse::<u64>() else {
                    return;
                };
                Some(id)
            };
            with(&host, |state| {
                state.missed.retain(|item| match id {
                    Some(id) => item.id != id,
                    None => !state.settings.wants(&item.provider),
                })
            });
            host.emit_state();
        });
        let weak = ui.as_weak();
        let host = this.borrow().host.clone();
        data.on_session_menu(move |key| {
            if let Some(ui) = weak.upgrade() {
                ui.set_compact_queued(with(&host, |state| {
                    state
                        .sessions
                        .get(key.as_str())
                        .is_some_and(|session| session.compact_queued)
                }));
                ui.set_menu_key(key);
                ui.set_menu_error("".into());
                ui.set_menu_index(0);
                ui.set_confirmation(0);
                crate::ui::focus_notch(&ui);
                ui.invoke_focus_menu();
            }
        });
        let weak = ui.as_weak();
        let host = this.borrow().host.clone();
        data.on_close_menu(move || {
            if let Some(ui) = weak.upgrade() {
                if ui.get_menu_key().is_empty() {
                    return;
                }
                ui.global::<NotchData>()
                    .set_restore_session(ui.get_menu_key());
                ui.set_menu_key("".into());
                ui.set_confirmation(0);
                host.emit_state();
            }
        });
        let weak = ui.as_weak();
        let host = this.borrow().host.clone();
        data.on_session_action(move |key, action| {
            if let Some(ui) = weak.upgrade() {
                ui.set_menu_busy(true);
                let host = host.clone();
                let weak = ui.as_weak();
                std::thread::spawn(move || {
                    let result = actions::session_action(&host, &key, &action);
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        ui.set_menu_busy(false);
                        match result {
                            Ok(()) => ui.set_menu_key("".into()),
                            Err(error) => ui.set_menu_error(error.into()),
                        }
                    });
                });
            }
        });
        let host = this.borrow().host.clone();
        let weak = ui.as_weak();
        data.on_desk_action(move |action| {
            if action == "extension_download" {
                let _ = std::process::Command::new("explorer").arg("https://github.com/hitraj2003/nubi/releases/latest/download/nubi-desktime-extension.zip").spawn();
            } else {
                host.desktime.lock().unwrap_or_else(|error| error.into_inner()).error = Some("DeskTime integration is unavailable in this native build.".into());
                if let Some(ui) = weak.upgrade() { ui.global::<NotchData>().set_desk_message("DeskTime integration is unavailable in this native build.".into()); }
                host.emit_state();
            }
        });
        let owner = this.clone();
        ui.on_drag_start(move || owner.borrow_mut().drag_requested = true);
        let owner = this.clone();
        let weak = ui.as_weak();
        ui.on_pill_clicked(move || {
            let mut this = owner.borrow_mut();
            if !this.dragging {
                this.clicked = true;
                this.hovering = true;
                if let Some(ui) = weak.upgrade() {
                    crate::ui::focus_notch(&ui);
                    ui.invoke_focus_requests();
                }
            }
        });
        let owner = this.clone();
        let weak = ui.as_weak();
        ui.on_tab_selected(move |_| {
            if let Some(ui) = weak.upgrade() {
                owner.borrow_mut().release_holds();
                ui.global::<NotchData>().set_focused_request("".into());
                ui.global::<NotchData>().set_input_focused(false);
                ui.global::<NotchData>().set_detail_tip("".into());
                ui.set_menu_key("".into());
                owner.borrow_mut().update(&ui);
            }
        });
        let owner = this.clone();
        let weak = ui.as_weak();
        ui.on_refresh_usage(move || {
            if let Some(ui) = weak.upgrade() {
                owner.borrow_mut().refresh(&ui);
            }
        });
        this.borrow_mut().update(ui);
        this
    }

    fn answer(
        &mut self,
        id: u64,
        decision: &str,
        answers: Option<serde_json::Map<String, serde_json::Value>>,
        ui: &NotchWindow,
    ) {
        ui.global::<NotchData>().set_request_error("".into());
        let result = with(&self.host, |state| {
            state.answer(
                id,
                Reply {
                    decision: decision.into(),
                    message: (decision == "deny").then(|| "Denied from Nubi".into()),
                    answers: if decision == "skip" {
                        Some(Default::default())
                    } else {
                        answers
                    },
                },
            )
        });
        if let Err(error) = result {
            ui.global::<NotchData>().set_request_error(error.into());
        }
        self.host.emit_state();
    }
    fn advance(&mut self, id: u64, ui: &NotchWindow) {
        let host = self.host.clone();
        let answers = with(&host, |state| {
            let pending = state.pending.get(&id)?;
            let draft = self.drafts.entry(id).or_default();
            if draft.selected(pending).is_empty() {
                return None;
            }
            let questions = pending.tool_input.as_ref()?.get("questions")?.as_array()?;
            if draft.step + 1 < questions.len() {
                draft.step += 1;
                draft.custom.clear();
                return None;
            }
            Some(
                draft
                    .answers
                    .iter()
                    .map(|(key, labels)| {
                        (key.clone(), serde_json::Value::String(labels.join(", ")))
                    })
                    .collect(),
            )
        });
        if let Some(answers) = answers {
            self.answer(id, "answer", Some(answers), ui);
        }
    }
    pub fn release_holds(&self) {
        with(&self.host, |state| {
            for pending in state.pending.values_mut() {
                pending.held = false;
            }
        });
        self.host.emit_state();
    }
    pub fn pointer(&mut self, inside: bool, ui: &NotchWindow) {
        self.hovering = inside;
        if inside {
            self.popup = None;
            if with(&self.host, |state| {
                state
                    .pending
                    .values()
                    .any(|pending| state.settings.wants(&pending.provider))
            }) {
                ui.set_tab(0);
            }
        } else {
            self.clicked = false;
            ui.global::<NotchData>().set_detail_tip("".into());
            self.popup = None;
            self.fold_since = Instant::now();
            if ui.global::<NotchData>().get_focused_request().is_empty() {
                self.release_holds();
            }
        }
    }
    pub fn alert(&mut self, alert: Alert) {
        if self.delayed.len() + self.alerts.len() >= 50 {
            return;
        }
        if matches!(alert.kind.as_str(), "permission" | "question") {
            self.delayed.push((alert, Instant::now()));
        } else {
            self.alerts.push_back(alert);
        }
    }
    fn newest(state: &Inner) -> u64 {
        state
            .usage
            .values()
            .filter(|usage| state.settings.wants(&usage.provider))
            .filter_map(|usage| usage.fetched_ms)
            .max()
            .unwrap_or(0)
    }
    fn refresh(&mut self, ui: &NotchWindow) {
        if ui.get_tab() == 3 {
            self.host
                .desktime
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .error = Some("DeskTime integration is unavailable in this native build.".into());
            self.update(ui);
            return;
        }
        if self.refresh.is_none() {
            self.refresh = Some((
                Instant::now(),
                with(&self.host, |state| Self::newest(state)),
            ));
            self.refresh_error.clear();
            nubi_core::poller::force(&self.host);
        }
    }
}

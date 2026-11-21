//! Update offers share the Waiting pane, after permission and question prompts.
use super::*;
use crate::{updater, NotchUpdate};

pub(crate) fn connect_update(ui: &NotchWindow, host: NativeHost, runtime: tokio::runtime::Handle) {
    let data = ui.global::<NotchData>();
    let owner = host.clone();
    data.on_update_dismiss(move |id| {
        if let Ok(id) = id.parse() {
            updater::dismiss_update(owner.clone(), id);
        }
    });
    let owner = host.clone();
    data.on_update_release(move |id| {
        if let Some(notice) =
            updater::get_update_notice(owner.clone()).filter(|notice| id == notice.id.to_string())
        {
            let _ = std::process::Command::new("explorer")
                .arg(release_url(&notice.version))
                .spawn();
        }
    });
    let weak = ui.as_weak();
    data.on_update_install(move |id| {
        let Ok(id) = id.parse::<u64>() else { return };
        let host = host.clone();
        let weak = weak.clone();
        runtime.spawn(async move {
            let _ = updater::install_update(host, id).await;
            let _ = weak.upgrade_in_event_loop(move |ui| {
                let data = ui.global::<NotchData>();
                if data.get_update().id == id.to_string() {
                    data.set_update_submitting(false);
                }
            });
        });
    });
}

fn release_url(version: &str) -> String {
    format!("https://github.com/hitraj2003/nubi/releases/tag/v{version}")
}

impl Notch {
    #[cfg(test)]
    pub(crate) fn acknowledge_update_for_test(&mut self, ui: &NotchWindow) {
        self.update_shown = Some(Instant::now() - Duration::from_millis(251));
        self.update_notice(ui);
    }

    pub(crate) fn update_notice(&mut self, ui: &NotchWindow) {
        let data = ui.global::<NotchData>();
        let Some(notice) = updater::get_update_notice(self.host.clone()) else {
            if self.update_id.take().is_some() {
                data.set_update(NotchUpdate::default());
                data.set_update_submitting(false);
                self.update_shown = None;
                if data.get_focused_request().starts_with("update:") {
                    data.set_focused_request("".into());
                }
            }
            return;
        };
        let fresh = self.update_id != Some(notice.id);
        if fresh {
            self.update_id = Some(notice.id);
            self.update_shown = None;
            data.set_update_submitting(false);
            // Select the request pane without changing native or keyboard focus.
            ui.set_tab(0);
        }
        let shown = !fresh && ui.window().is_visible() && ui.get_update_card_shown();
        // A card covered by higher-priority prompts must not expire unseen.
        let visible = ui.window().is_visible() && ui.get_update_card_visible();
        let held = ui.get_update_card_held() || (!visible && notice.expires_ms != 0);
        if held != notice.held {
            updater::hold_update(self.host.clone(), notice.id, held);
        }
        let countdown = if notice.automatic || notice.phase != "available" || notice.expires_ms == 0
        {
            String::new()
        } else if held {
            "Paused".into()
        } else {
            let seconds = notice.expires_ms.saturating_sub(now_ms()).div_ceil(1000);
            format!("Dismisses in {seconds}s")
        };
        let row = NotchUpdate {
            id: notice.id.to_string().into(),
            version: notice.version.into(),
            phase: notice.phase.into(),
            message: notice.message.into(),
            countdown: countdown.into(),
            automatic: notice.automatic,
        };
        if data.get_update().phase != row.phase {
            data.set_update_submitting(false);
        }
        if data.get_update() != row {
            data.set_update(row);
        }
        if shown
            && !notice.automatic
            && notice.expires_ms == 0
            && data.get_update().phase == "available"
        {
            if self.update_shown.get_or_insert_with(Instant::now).elapsed()
                >= Duration::from_millis(250)
            {
                updater::update_notice_ready(self.host.clone(), notice.id);
                self.update_shown = None;
            }
        } else {
            self.update_shown = None;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn release_link_uses_the_offered_version() {
        assert_eq!(
            super::release_url("9.9.9-dev"),
            "https://github.com/hitraj2003/nubi/releases/tag/v9.9.9-dev"
        );
    }
}

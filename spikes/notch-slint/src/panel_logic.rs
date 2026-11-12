use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::rc::Rc;

pub fn connect(panel: &super::ParityPanel) {
    let weak = panel.as_weak();
    panel.on_filtered_sessions(move |filter| {
        let Some(panel) = weak.upgrade() else {
            return ModelRc::default();
        };
        let sessions = panel.global::<super::Fixture>().get_sessions();
        let rows = sessions
            .iter()
            .filter(|row| {
                filter == 0 || row.provider == if filter == 1 { "claude" } else { "codex" }
            })
            .collect::<Vec<_>>();
        ModelRc::from(Rc::new(VecModel::from(rows)))
    });
    panel.on_hotkey_parts(|value| {
        ModelRc::from(Rc::new(VecModel::from(
            value
                .split('+')
                .filter(|part| !part.is_empty())
                .map(SharedString::from)
                .collect::<Vec<_>>(),
        )))
    });
    let weak = panel.as_weak();
    panel.on_capture_shortcut(move |text, control, alt, shift| {
        let Some(panel) = weak.upgrade() else {
            return;
        };
        let backspace: SharedString = slint::platform::Key::Backspace.into();
        if text == backspace {
            panel.set_hotkey("".into());
            panel.set_dirty(true);
            return;
        }
        if text.is_empty()
            || text
                .chars()
                .any(|c| c.is_control() || ('\u{e000}'..='\u{f8ff}').contains(&c))
        {
            return;
        }
        let mut parts = Vec::new();
        if control {
            parts.push("Ctrl".to_string());
        }
        if alt {
            parts.push("Alt".to_string());
        }
        if shift {
            parts.push("Shift".to_string());
        }
        parts.push(text.to_uppercase());
        panel.set_hotkey(parts.join("+").into());
        panel.set_dirty(true);
    });
}

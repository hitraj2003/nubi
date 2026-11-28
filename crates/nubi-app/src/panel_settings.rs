//! Panel drafts are field patches, so a save cannot overwrite unrelated host changes.
use crate::{FieldChoice, PanelField};
use nubi_core::settings::Settings;
use serde_json::{json, Value};
use slint::{ModelRc, SharedString, VecModel};
use std::{collections::BTreeMap, rc::Rc};

#[derive(Clone)]
pub struct Draft {
    baseline: Value,
    edits: BTreeMap<String, Value>,
}
impl Draft {
    pub fn new(settings: &Settings) -> Self {
        Self {
            baseline: serde_json::to_value(settings).unwrap(),
            edits: BTreeMap::new(),
        }
    }
    pub fn dirty(&self) -> bool {
        !self.edits.is_empty()
    }
    pub fn value(&self, key: &str) -> Value {
        self.edits.get(key).unwrap_or(&self.baseline[key]).clone()
    }
    pub fn edit(&mut self, key: &str, value: Value) {
        if self.baseline[key] == value {
            self.edits.remove(key);
        } else {
            self.edits.insert(key.into(), value);
        }
    }
    pub fn rebase(&mut self, current: &Settings) {
        self.baseline = serde_json::to_value(current).unwrap();
        self.edits.retain(|key, value| self.baseline[key] != *value);
    }
    pub fn merged(&self, current: &Settings) -> Result<Settings, String> {
        let mut value = serde_json::to_value(current).unwrap();
        for (key, edit) in &self.edits {
            value[key] = edit.clone();
        }
        let settings: Settings = serde_json::from_value(value).map_err(|e| e.to_string())?;
        validate(&settings)?;
        Ok(settings)
    }
    pub fn edit_text(&mut self, key: &str, text: &str) -> Result<(), String> {
        let value = match key {
            "takeover_secs" | "request_popup_secs" | "monitor" | "quiet_start_min" | "quiet_end_min" => {
                json!(text.parse::<u32>().map_err(|_| "Enter a whole number")?)
            }
            "quiet_start" | "quiet_end" => {
                if text.is_empty() {
                    Value::Null
                } else {
                    json!(text.parse::<u8>().map_err(|_| "Choose an hour")?)
                }
            }
            _ if self.baseline[key].is_boolean() => json!(text == "true"),
            _ => json!(text),
        };
        self.edit(key, value);
        Ok(())
    }
    pub fn choice(&mut self, key: &str, item: &str) -> Result<(), String> {
        let mut values = self.value(key).as_array().cloned().unwrap_or_default();
        let item = if key == "work_days" {
            json!(item.parse::<u8>().map_err(|_| "Invalid weekday")?)
        } else {
            json!(item)
        };
        if values.contains(&item) {
            values.retain(|v| *v != item);
        } else {
            values.push(item);
        }
        if key == "dt_views" && values.is_empty() {
            return Err("Pick at least one DeskTime section".into());
        }
        self.edit(key, json!(values));
        Ok(())
    }
    pub fn date(&mut self, key: &str, date: &str, remove: bool) -> Result<(), String> {
        if !["holidays", "half_days"].contains(&key) {
            return Err("Invalid date list".into());
        }
        chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| "Choose a valid date")?;
        let mut values = self.value(key).as_array().cloned().unwrap_or_default();
        if remove {
            values.retain(|v| v.as_str() != Some(date));
        } else {
            if values.iter().any(|v| v.as_str() == Some(date)) {
                return Err(format!("{date} is already in the list"));
            }
            if values.len() >= 366 {
                return Err("The list is full (366 days)".into());
            }
            values.push(json!(date));
            values.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
        }
        self.edit(key, json!(values));
        Ok(())
    }
}

pub fn validate(settings: &Settings) -> Result<(), String> {
    if !(3..=95).contains(&settings.takeover_secs) {
        return Err("Move to terminal after must be 3–95 seconds".into());
    }
    if settings.request_popup_secs > 120 {
        return Err("Keep cards open must be 0–120 seconds".into());
    }
    if settings.quiet_start.is_some_and(|v| v > 23) || settings.quiet_end.is_some_and(|v| v > 23) {
        return Err("Quiet hours must be 00:00–23:00".into());
    }
    crate::tray::parse_shortcuts(settings)?;
    nubi_core::desktime::validate_settings(settings)
}

pub struct Spec {
    pub pane: i32,
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub section: &'static str,
    pub kind: &'static str,
    pub options: &'static [(&'static str, &'static str)],
}
macro_rules! field {
    ($p:expr,$k:expr,$l:expr,$d:expr,$s:expr,$t:expr,$o:expr) => {
        Spec {
            pane: $p,
            key: $k,
            label: $l,
            description: $d,
            section: $s,
            kind: $t,
            options: $o,
        }
    };
}
const THEMES: &[(&str, &str)] = &[("light", "Light"), ("dark", "Dark"), ("system", "System")];
const ACCENTS: &[(&str, &str)] = &[
    ("yellow", "Yellow"),
    ("terracotta", "Terracotta"),
    ("blue", "Blue"),
    ("green", "Green"),
    ("violet", "Violet"),
    ("black", "Ink"),
];
const PERIOD: &[(&str, &str)] = &[
    ("target", "Target"),
    ("todate", "To date"),
    ("balance", "Balance"),
    ("eod", "At end of day"),
    ("late", "Late arrival"),
];
pub const SPECS: &[Spec] = &[
    field!(0,"providers","Show sessions from","Which coding agents Nubi keeps an eye on","","select", &[("both","Both"),("claude","Claude"),("codex","Codex")]),
    field!(0,"auto_start","Start with Windows","Opens Nubi when you sign in","","bool", &[]),
    field!(0,"auto_update","Update automatically","Downloads updates in the background, then asks when to install. When off, asks before downloading.","","bool", &[]),
    field!(0,"launcher_enabled","Open Claude or Codex from Explorer","Press the shortcut in a File Explorer window to open a terminal in that folder","Launcher","bool", &[]),
    field!(0,"launch_hotkey","Launcher shortcut","Select Record, then press a key combination.","","shortcut", &[]),
    field!(1,"monitor","Monitor","Choose which display shows the notch.","Notch placement","monitor", &[]),
    field!(1,"edge","Edge","Where the notch sits on the screen","","select", &[("top","Top"),("right","Right"),("left","Left"),("bottom","Bottom")]),
    field!(1,"notch_on_hover","Shrink when not in use","The notch becomes a thin line until you point at it","","bool", &[]),
    field!(1,"theme","Theme","","Panel","theme", THEMES),
    field!(1,"accent","Accent","Color of buttons, switches and the selected item","","accent", ACCENTS),
    field!(1,"notch_theme","Theme","","Notch & notifications","theme", THEMES),
    field!(1,"notch_accent","Accent","Color of buttons, switches and the selected item","","accent", ACCENTS),
    field!(1,"reduce_motion","Reduce motion","Simple fades instead of bouncy animations. Also on when Windows asks for less motion.","","bool", &[]),
    field!(4,"sounds","Sounds","Play a short sound for each alert","","bool", &[]),
    field!(4,"notifications_inside_notch","Keep notifications inside the notch","Show toasts and request cards inside the notch on every edge. Overrides Windows notifications in full screen.","","bool", &[]),
    field!(4,"toast_when_fullscreen","Use Windows notifications in full screen","Applies when Keep notifications inside the notch is off","","bool", &[]),
    field!(4,"dnd_hotkey","Keyboard shortcut","Turns Do not disturb on or off.","Do not disturb","shortcut", &[]),
    field!(4,"quiet_start","Quiet hours from","Stay quiet between these hours","","hour", &[]),
    field!(4,"quiet_end","Quiet hours to","Quiet hours can cross midnight","","hour", &[]),
    field!(4,"follow_quiet_time","Follow Windows Focus","Stay quiet while Windows Focus or Do not disturb is on","","bool", &[]),
    field!(4,"questions_mode","Agent questions","Choices only shows the prompt's options. Full also lets you type an answer.","Questions & approvals","select", &[("off","Off"),("system","Choices only"),("full","Full")]),
    field!(4,"request_layout","Request cards","How multiple requests appear","","select", &[("stack","Stacked"),("list","Side by side")]),
    field!(4,"takeover_secs","Move to terminal after","Time to answer in Nubi before the terminal asks instead","","number", &[]),
    field!(4,"request_popup_secs","Keep cards open","0 keeps cards open until answered","","number", &[]),
    field!(5,"dt_views","Sections","Which cards appear in the notch's DeskTime tab","Notch tab","multi", &[("day","Today"),("week","Week"),("month","Month")]),
    field!(5,"dt_rows_day","Today rows","Values shown on the Today card","","multi", &[("done","Done"),("finish","Finish at"),("target","Target"),("late","Late arrival"),("nonprod","Non-productive")]),
    field!(5,"dt_rows_week","Week rows","Values shown on the Week card","","multi", PERIOD),
    field!(5,"dt_rows_month","Month rows","Values shown on the Month card","","multi", PERIOD),
    field!(5,"dt_notes","Descriptions","Small grey line under each value, like “48% of target”","","bool", &[]),
    field!(5,"dt_glance","Show in the closed notch","A short DeskTime line next to the rings, at the top edge","","bool", &[]),
    field!(5,"dt_glance_kind","Closed notch shows","","","select", &[("left","Time left today"),("done","Done / target")]),
    field!(5,"dt_alert_target","Target reached alert","A notification when today's productive target is reached","","bool", &[]),
    field!(5,"daily_target","Daily target","Hours to work on a normal day","Work schedule","time", &[]),
    field!(5,"end_of_day","End of day","Used to project where you will end up today","","time", &[]),
    field!(5,"start_of_day","Start of day","Optional. Lets the month view show a latest arrival time.","","time", &[]),
    field!(5,"work_days","Work days","Days that count towards your target","","multi", &[("1","Mon"),("2","Tue"),("3","Wed"),("4","Thu"),("5","Fri"),("6","Sat"),("0","Sun")]),
    field!(5,"holidays","Holidays","These days do not count towards your target","Days off","dates", &[]),
    field!(5,"half_days","Half days","These days count for half your normal target","","dates", &[]),
];
pub fn text(value: &Value) -> String {
    value.as_str().map(str::to_owned).unwrap_or_else(|| {
        if value.is_null() {
            String::new()
        } else {
            value.to_string()
        }
    })
}
pub fn model<T: Clone + 'static>(rows: Vec<T>) -> ModelRc<T> {
    Rc::new(VecModel::from(rows)).into()
}
pub fn row(spec: &Spec, draft: &Draft, monitors: &[String]) -> PanelField {
    let value = draft.value(spec.key);
    let string = text(&value);
    let mut options: Vec<_> = spec
        .options
        .iter()
        .map(|(_, label)| SharedString::from(*label))
        .collect();
    let mut selected = spec
        .options
        .iter()
        .position(|(key, _)| *key == string)
        .unwrap_or(0) as i32;
    if spec.kind == "hour" {
        options = std::iter::once("Off".into())
            .chain((0..24).map(|h| format!("{h:02}:00").into()))
            .collect();
        selected = value.as_i64().map_or(0, |v| v as i32 + 1);
    } else if spec.kind == "monitor" {
        options = monitors.iter().map(|s| s.as_str().into()).collect();
        selected = value.as_i64().unwrap_or(0) as i32;
    }
    let choices = if spec.kind == "dates" {
        value
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| FieldChoice {
                value: text(v).into(),
                label: text(v).into(),
                selected: true,
            })
            .collect()
    } else {
        spec.options
            .iter()
            .map(|(key, label)| FieldChoice {
                value: (*key).into(),
                label: (*label).into(),
                selected: value
                    .as_array()
                    .is_some_and(|list| list.iter().any(|v| text(v) == *key)),
            })
            .collect()
    };
    let (hour, minute) = if spec.kind == "hour" {
        (
            value.as_i64().map_or(0, |v| v as i32 + 1),
            draft.value(&format!("{}_min", spec.key)).as_i64().unwrap_or(0) as i32,
        )
    } else { string
        .split_once(':')
        .and_then(|(h, m)| Some((h.parse().ok()?, m.parse().ok()?)))
        .unwrap_or((0, 0)) };
    let mut minutes: Vec<_> = (0..60).step_by(5).collect();
    if !minutes.contains(&minute) {
        minutes.push(minute);
        minutes.sort();
    }
    let minute_index = minutes.iter().position(|v| *v == minute).unwrap_or(0) as i32;
    PanelField {
        key: spec.key.into(),
        label: spec.label.into(),
        description: spec.description.into(),
        section: spec.section.into(),
        kind: spec.kind.into(),
        value: string.clone().into(),
        checked: value.as_bool().unwrap_or(false),
        selected,
        options: model(options),
        choices: model(choices),
        minimum: if spec.key == "takeover_secs" { 3 } else { 0 },
        maximum: if spec.key == "takeover_secs" { 95 } else { 120 },
        number: value.as_i64().unwrap_or(0) as i32,
        hour,
        minute,
        enabled: match spec.key {
            "launch_hotkey" => draft.value("launcher_enabled") == true,
            "dt_glance_kind" => draft.value("dt_glance") == true,
            _ => true,
        },
        error: SharedString::default(),
        first: false,
        last: false,
        hours: model(
            (spec.kind == "hour")
                .then(|| "Off".into())
                .into_iter()
                .chain((0..24).map(|h| format!("{h:02}").into()))
                .collect(),
        ),
        minutes: model(
            minutes
                .into_iter()
                .map(|m| format!("{m:02}").into())
                .collect(),
        ),
        minute_index,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn draft_patches_preserve_untouched_and_concurrent_host_updates() {
        let original: Settings = serde_json::from_str("{}").unwrap();
        assert!(!original.notifications_inside_notch);
        let mut draft = Draft::new(&original);
        draft.edit("sounds", json!(false));
        draft.edit_text("notifications_inside_notch", "true").unwrap();
        let mut concurrent = original.clone();
        concurrent.notch_along = 0.73;
        concurrent.day_targets.insert(1, "06:25".into());
        concurrent.desktime_enabled = true;
        concurrent.launch_hotkey = "ctrl+shift+y".into();
        let merged = draft.merged(&concurrent).unwrap();
        assert!(!merged.sounds);
        assert!(merged.notifications_inside_notch);
        assert_eq!(merged.notch_along, 0.73);
        assert_eq!(merged.day_targets[&1], "06:25");
        assert!(merged.desktime_enabled);
        assert_eq!(merged.launch_hotkey, "ctrl+shift+y");
        draft.rebase(&concurrent);
        assert_eq!(draft.value("launch_hotkey"), "ctrl+shift+y");
        assert!(draft.dirty());
    }
    #[test]
    fn validates_time_dates_shortcuts_and_last_desktime_section() {
        let mut draft = Draft::new(&Settings::default());
        draft.edit_text("daily_target", "00:00").unwrap();
        assert!(draft.merged(&Settings::default()).is_err());
        draft.edit_text("daily_target", "07:35").unwrap();
        draft.edit_text("launch_hotkey", "ctrl+alt+d").unwrap();
        assert!(draft.merged(&Settings::default()).is_err());
        draft.edit_text("launch_hotkey", "ctrl+shift+y").unwrap();
        assert!(draft.merged(&Settings::default()).is_ok());
        assert!(draft.date("holidays", "2027-02-29", false).is_err());
        draft.date("holidays", "2028-02-29", false).unwrap();
        assert!(draft.date("holidays", "2028-02-29", false).is_err());
        draft.choice("dt_views", "day").unwrap();
        draft.choice("dt_views", "week").unwrap();
        assert!(draft.choice("dt_views", "month").is_err());
        assert_eq!(draft.value("dt_views"), json!(["month"]));
    }
}

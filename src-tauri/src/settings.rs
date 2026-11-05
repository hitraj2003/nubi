//! User settings, stored in %APPDATA%\nubi\settings.json (moved from %APPDATA%\claude-overlay).
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    /// "claude" | "codex" | "both"
    pub providers: String,
    /// "island" | "ring" | "taskbar" (old "notch" is read as "island")
    pub display_mode: String,
    /// Index into the monitor list.
    pub monitor: usize,
    /// Edge for notch mode: "top" | "bottom" | "left" | "right"
    pub edge: String,
    /// Where the notch sits along its edge, 0..1 (centre = 0.5).
    pub notch_along: f64,
    /// Fold the notch to a small tab until the pointer reaches it.
    pub notch_on_hover: bool,
    /// Waiting approvals/questions in the notification island: "stack" (deck) | "list" (side by side).
    pub request_layout: String,
    /// Seconds a request card stays open before folding away (0 = until answered).
    pub request_popup_secs: u32,
    /// Seconds an approval or question waits in the overlay before the terminal asks instead.
    pub takeover_secs: u32,
    /// Claude questions in the overlay: "off" | "system" (the prompt's own options only) | "full".
    pub questions_mode: String,
    /// Floating ring position (physical px); None = default spot.
    pub ring_pos: Option<(i32, i32)>,
    /// Taskbar mode: "left" | "right"
    pub taskbar_side: String,
    pub dnd_hotkey: String,
    /// Opens the Claude/Codex launcher for the focused File Explorer folder. Empty = off.
    pub launch_hotkey: String,
    pub launcher_enabled: bool,
    /// Quiet hours, local hour 0-23. Both set = active.
    pub quiet_start: Option<u8>,
    pub quiet_end: Option<u8>,
    pub follow_quiet_time: bool,
    pub sounds: bool,
    pub toast_when_fullscreen: bool,
    /// Install at idle startup; later checks offer updates. Off still checks and notifies.
    pub auto_update: bool,
    /// Start Nubi at Windows sign-in (HKCU Run key).
    pub auto_start: bool,
    /// Panel theme: "system" | "dark" | "light"
    pub theme: String,
    /// Panel accent: "yellow" | "terracotta" | "blue" | "green" | "violet" | "black"
    pub accent: String,
    /// Theme for the notch, toast, ring, ring tip and taskbar strip: "system" | "dark" | "light"
    pub notch_theme: String,
    /// Accent for the same surfaces (same values as `accent`).
    pub notch_accent: String,
    /// Fades only, no springs (also on when Windows asks for less motion).
    pub reduce_motion: bool,
    pub desktime_enabled: bool,
    /// Shows the DeskTime tab in the notch and runs the dashboard reads.
    pub desktime_tab: bool,
    /// HH:MM targets and clocks; an empty start disables arrival clocks.
    pub daily_target: String,
    pub end_of_day: String,
    pub start_of_day: String,
    /// Sunday = 0, Monday = 1.
    pub work_days: Vec<u8>,
    pub day_targets: std::collections::BTreeMap<u8, String>,
    pub holidays: Vec<String>,
    pub half_days: Vec<String>,
    /// DeskTime sections shown in the notch tab: any of "day", "week", "month" (at least one).
    pub dt_views: Vec<String>,
    /// Small DeskTime text in the collapsed notch.
    pub dt_glance: bool,
    /// Glance content: "left" | "done".
    pub dt_glance_kind: String,
    /// Notification when the productive target for today is reached.
    pub dt_alert_target: bool,
    /// Rows shown on each DeskTime card, by row key (see desktime::DAY_ROWS / PERIOD_ROWS).
    pub dt_rows_day: Vec<String>,
    pub dt_rows_week: Vec<String>,
    pub dt_rows_month: Vec<String>,
    /// Small grey description under each value.
    pub dt_notes: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            providers: "both".into(),
            display_mode: "island".into(),
            monitor: 0,
            edge: "top".into(),
            notch_along: 0.5,
            notch_on_hover: true,
            request_layout: "stack".into(),
            request_popup_secs: 8,
            questions_mode: "system".into(),
            takeover_secs: 10,
            ring_pos: None,
            taskbar_side: "right".into(),
            dnd_hotkey: "ctrl+alt+d".into(),
            launch_hotkey: "ctrl+shift+t".into(),
            launcher_enabled: true,
            quiet_start: None,
            quiet_end: None,
            follow_quiet_time: true,
            sounds: true,
            toast_when_fullscreen: true,
            auto_update: true,
            auto_start: false,
            theme: "system".into(),
            accent: "yellow".into(),
            notch_theme: "dark".into(),
            notch_accent: "yellow".into(),
            reduce_motion: false,
            desktime_enabled: false,
            desktime_tab: true,
            daily_target: "08:30".into(),
            end_of_day: "18:30".into(),
            start_of_day: String::new(),
            work_days: vec![1, 2, 3, 4, 5],
            day_targets: Default::default(),
            holidays: Vec::new(),
            half_days: Vec::new(),
            dt_views: vec!["day".into(), "week".into(), "month".into()],
            dt_glance: false,
            dt_glance_kind: "left".into(),
            dt_alert_target: true,
            dt_rows_day: vec!["done".into(), "finish".into()],
            dt_rows_week: vec!["balance".into(), "late".into()],
            dt_rows_month: vec!["balance".into(), "late".into()],
            dt_notes: false,
        }
    }
}

impl Settings {
    pub fn wants(&self, provider: &str) -> bool {
        self.providers == "both" || self.providers == provider
    }
}

pub fn data_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(std::env::temp_dir).join("nubi")
}

fn path() -> PathBuf {
    data_dir().join("settings.json")
}

pub fn load() -> Settings {
    // Before the rename to Nubi, settings lived in %APPDATA%\claude-overlay.
    let legacy = dirs::config_dir().map(|d| d.join("claude-overlay").join("settings.json"));
    std::fs::read_to_string(path())
        .ok()
        .or_else(|| legacy.and_then(|p| std::fs::read_to_string(p).ok()))
        .and_then(|t| serde_json::from_str::<Settings>(&t).ok())
        .map(|mut s| {
            // The edge notch became the island (same edges, new shape).
            if s.display_mode == "notch" {
                s.display_mode = "island".into();
            }
            // The leave-at glance was removed (DeskTime math now follows the v5 extension).
            if s.dt_glance_kind == "leave" {
                s.dt_glance_kind = "left".into();
            }
            s
        })
        .unwrap_or_default()
}

/// Add or remove the HKCU Run entry that starts Nubi at sign-in.
pub fn apply_auto_start(on: bool) -> Result<(), String> {
    use windows_sys::Win32::System::Registry::{RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
    let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Run");
    let name = wide("Nubi");
    if on {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let cmd = wide(&format!("\"{}\"", exe.display()));
        let rc = unsafe {
            RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), REG_SZ, cmd.as_ptr() as *const _, (cmd.len() * 2) as u32)
        };
        if rc != 0 {
            return Err(format!("could not set auto-start (error {rc})"));
        }
    } else {
        // 2 = ERROR_FILE_NOT_FOUND: already off.
        let rc = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) };
        if rc != 0 && rc != 2 {
            return Err(format!("could not remove auto-start (error {rc})"));
        }
    }
    Ok(())
}

pub fn save(s: &Settings) -> Result<(), String> {
    std::fs::create_dir_all(data_dir()).map_err(|e| e.to_string())?;
    let tmp = path().with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(s).unwrap()).map_err(|e| e.to_string())?;
    std::fs::rename(tmp, path()).map_err(|e| e.to_string())
}

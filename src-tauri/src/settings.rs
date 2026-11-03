//! User settings, stored in %APPDATA%\claude-overlay\settings.json.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    /// "claude" | "codex" | "both"
    pub providers: String,
    /// "notch" | "ring" | "taskbar"
    pub display_mode: String,
    /// Index into the monitor list.
    pub monitor: usize,
    /// Edge for notch mode: "top" | "bottom" | "left" | "right"
    pub edge: String,
    /// Where the notch sits along its edge, 0..1 (centre = 0.5).
    pub notch_along: f64,
    /// Fold the notch to a small tab until the pointer reaches it.
    pub notch_on_hover: bool,
    /// Waiting approvals/questions beside the notch: "stack" | "side".
    pub notch_requests_layout: String,
    /// Floating ring position (physical px); None = default spot.
    pub ring_pos: Option<(i32, i32)>,
    /// Taskbar mode: "left" | "right"
    pub taskbar_side: String,
    pub dnd_hotkey: String,
    /// Quiet hours, local hour 0-23. Both set = active.
    pub quiet_start: Option<u8>,
    pub quiet_end: Option<u8>,
    pub follow_quiet_time: bool,
    pub sounds: bool,
    pub toast_when_fullscreen: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            providers: "both".into(),
            display_mode: "notch".into(),
            monitor: 0,
            edge: "right".into(),
            notch_along: 0.5,
            notch_on_hover: true,
            notch_requests_layout: "stack".into(),
            ring_pos: None,
            taskbar_side: "right".into(),
            dnd_hotkey: "ctrl+alt+d".into(),
            quiet_start: None,
            quiet_end: None,
            follow_quiet_time: true,
            sounds: true,
            toast_when_fullscreen: true,
        }
    }
}

impl Settings {
    pub fn wants(&self, provider: &str) -> bool {
        self.providers == "both" || self.providers == provider
    }
}

pub fn data_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(std::env::temp_dir).join("claude-overlay")
}

fn path() -> PathBuf {
    data_dir().join("settings.json")
}

pub fn load() -> Settings {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(s: &Settings) -> Result<(), String> {
    std::fs::create_dir_all(data_dir()).map_err(|e| e.to_string())?;
    let tmp = path().with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(s).unwrap()).map_err(|e| e.to_string())?;
    std::fs::rename(tmp, path()).map_err(|e| e.to_string())
}

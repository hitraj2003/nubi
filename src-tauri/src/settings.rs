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
    /// Quiet hours, local hour 0-23. Both set = active.
    pub quiet_start: Option<u8>,
    pub quiet_end: Option<u8>,
    pub follow_quiet_time: bool,
    pub sounds: bool,
    pub toast_when_fullscreen: bool,
    /// Install new versions from GitHub releases automatically (release builds only).
    pub auto_update: bool,
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
            quiet_start: None,
            quiet_end: None,
            follow_quiet_time: true,
            sounds: true,
            toast_when_fullscreen: true,
            auto_update: true,
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
            s
        })
        .unwrap_or_default()
}

pub fn save(s: &Settings) -> Result<(), String> {
    std::fs::create_dir_all(data_dir()).map_err(|e| e.to_string())?;
    let tmp = path().with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(s).unwrap()).map_err(|e| e.to_string())?;
    std::fs::rename(tmp, path()).map_err(|e| e.to_string())
}

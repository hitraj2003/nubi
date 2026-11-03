//! Enable/disable our hook entries in Claude Code (`~/.claude/settings.json`) and
//! Codex (`$CODEX_HOME/hooks.json`). Our entries are found by the hook exe name in the
//! command, so no extra marker field is written. Unrelated hooks are kept. Writes are atomic.
use crate::protocol::HOOK_TIMEOUT_SECS;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub const HOOK_EXE: &str = "claude-overlay-hook.exe";

struct EventSpec {
    name: &'static str,
    matcher: Option<&'static str>,
    timeout: Option<u64>,
}

const CLAUDE_EVENTS: &[EventSpec] = &[
    EventSpec { name: "SessionStart", matcher: None, timeout: Some(10) },
    EventSpec { name: "SessionEnd", matcher: None, timeout: Some(10) },
    EventSpec { name: "UserPromptSubmit", matcher: None, timeout: Some(10) },
    EventSpec { name: "PreToolUse", matcher: Some("*"), timeout: Some(10) },
    EventSpec { name: "Notification", matcher: None, timeout: Some(10) },
    EventSpec { name: "Stop", matcher: None, timeout: Some(10) },
    EventSpec { name: "SubagentStop", matcher: None, timeout: Some(10) },
    EventSpec { name: "PermissionRequest", matcher: Some("*"), timeout: Some(HOOK_TIMEOUT_SECS) },
];

const CODEX_EVENTS: &[EventSpec] = &[
    EventSpec { name: "SessionStart", matcher: None, timeout: Some(10) },
    EventSpec { name: "SessionEnd", matcher: None, timeout: Some(3) },
    EventSpec { name: "UserPromptSubmit", matcher: None, timeout: Some(10) },
    EventSpec { name: "PreToolUse", matcher: None, timeout: Some(10) },
    EventSpec { name: "Stop", matcher: None, timeout: Some(10) },
    EventSpec { name: "SubagentStart", matcher: None, timeout: Some(10) },
    EventSpec { name: "SubagentStop", matcher: None, timeout: Some(10) },
    EventSpec { name: "PermissionRequest", matcher: None, timeout: Some(HOOK_TIMEOUT_SECS) },
];

pub fn claude_settings_path() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("CLAUDE_CONFIG_DIR") {
        return Some(PathBuf::from(d).join("settings.json"));
    }
    Some(dirs::home_dir()?.join(".claude").join("settings.json"))
}

pub fn codex_home() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("CODEX_HOME") {
        return Some(PathBuf::from(d));
    }
    Some(dirs::home_dir()?.join(".codex"))
}

pub fn codex_hooks_path() -> Option<PathBuf> {
    Some(codex_home()?.join("hooks.json"))
}

/// Stable copy of our exe that hooks call, so moving the app does not break hooks.
pub fn shim_path() -> Option<PathBuf> {
    Some(dirs::data_local_dir()?.join("claude-overlay").join(HOOK_EXE))
}

/// Copy the running exe to the shim path. A locked shim (hook running) is left as is.
pub fn install_shim() -> std::io::Result<PathBuf> {
    let dst = shim_path().ok_or_else(|| std::io::Error::other("no local app data dir"))?;
    std::fs::create_dir_all(dst.parent().unwrap())?;
    let src = std::env::current_exe()?;
    match std::fs::copy(&src, &dst) {
        Ok(_) => Ok(dst),
        Err(e) if dst.exists() => {
            eprintln!("[claude-overlay] shim copy skipped: {e}");
            Ok(dst)
        }
        Err(e) => Err(e),
    }
}

fn command_for(shim: &Path, provider: &str) -> String {
    format!("\"{}\" --hook {}", shim.display().to_string().replace('\\', "/"), provider)
}

fn is_ours(h: &Value) -> bool {
    ["command", "commandWindows"]
        .iter()
        .any(|k| h.get(*k).and_then(|c| c.as_str()).map_or(false, |c| c.contains(HOOK_EXE)))
}

/// Remove our entries. Returns true when anything was removed.
pub fn strip(root: &mut Value) -> bool {
    let Some(hooks) = root.get_mut("hooks").and_then(|h| h.as_object_mut()) else { return false };
    let mut changed = false;
    let events: Vec<String> = hooks.keys().cloned().collect();
    for ev in events {
        let Some(groups) = hooks.get_mut(&ev).and_then(|g| g.as_array_mut()) else { continue };
        for g in groups.iter_mut() {
            if let Some(list) = g.get_mut("hooks").and_then(|l| l.as_array_mut()) {
                let before = list.len();
                list.retain(|h| !is_ours(h));
                changed |= list.len() != before;
            }
        }
        groups.retain(|g| g.get("hooks").and_then(|l| l.as_array()).map_or(true, |l| !l.is_empty()));
        if groups.is_empty() {
            hooks.remove(&ev);
        }
    }
    if hooks.is_empty() {
        root.as_object_mut().map(|o| o.remove("hooks"));
    }
    changed
}

fn add(root: &mut Value, events: &[EventSpec], command: &str, codex: bool) {
    if !root.is_object() {
        *root = Value::Object(Map::new());
    }
    let obj = root.as_object_mut().unwrap();
    let hooks = obj.entry("hooks").or_insert_with(|| json!({}));
    if !hooks.is_object() {
        *hooks = json!({});
    }
    let hooks = hooks.as_object_mut().unwrap();
    for e in events {
        let mut h = json!({ "type": "command", "command": command });
        if codex {
            h["commandWindows"] = json!(command);
        }
        if let Some(t) = e.timeout {
            h["timeout"] = json!(t);
        }
        let mut group = json!({ "hooks": [h] });
        if let Some(m) = e.matcher {
            group["matcher"] = json!(m);
        }
        let list = hooks.entry(e.name).or_insert_with(|| json!([]));
        if !list.is_array() {
            *list = json!([]);
        }
        list.as_array_mut().unwrap().push(group);
    }
}

pub fn has_ours(root: &Value) -> bool {
    root.get("hooks")
        .and_then(|h| h.as_object())
        .map_or(false, |h| {
            h.values().filter_map(|g| g.as_array()).flatten().any(|g| {
                g.get("hooks").and_then(|l| l.as_array()).map_or(false, |l| l.iter().any(is_ours))
            })
        })
}

fn read_json(path: &Path) -> Result<Value, String> {
    match std::fs::read_to_string(path) {
        Ok(t) if t.trim().is_empty() => Ok(json!({})),
        Ok(t) => serde_json::from_str(&t).map_err(|e| format!("{} is not valid JSON: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e.to_string()),
    }
}

fn write_atomic(path: &Path, v: &Value) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let bak = path.with_extension("json.claude-overlay.bak");
    if path.exists() && !bak.exists() {
        let _ = std::fs::copy(path, &bak);
    }
    let tmp = path.with_extension("json.claude-overlay.tmp");
    let text = serde_json::to_string_pretty(v).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text + "\n").map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn path_for(provider: &str) -> Option<PathBuf> {
    if provider == "codex" {
        codex_hooks_path()
    } else {
        claude_settings_path()
    }
}

pub fn is_enabled(provider: &str) -> bool {
    path_for(provider).and_then(|p| read_json(&p).ok()).map_or(false, |v| has_ours(&v))
}

pub fn set_enabled(provider: &str, enable: bool) -> Result<(), String> {
    let path = path_for(provider).ok_or("no home directory")?;
    let mut root = read_json(&path)?;
    strip(&mut root);
    if enable {
        let shim = install_shim().map_err(|e| e.to_string())?;
        let cmd = command_for(&shim, provider);
        if provider == "codex" {
            add(&mut root, CODEX_EVENTS, &cmd, true);
        } else {
            add(&mut root, CLAUDE_EVENTS, &cmd, false);
        }
    }
    write_atomic(&path, &root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_then_strip_keeps_foreign_hooks() {
        let mut root = json!({
            "model": "x",
            "hooks": { "SessionStart": [ { "hooks": [ { "type": "command", "command": "other.exe" } ] } ] }
        });
        add(&mut root, CLAUDE_EVENTS, "\"C:/x/claude-overlay-hook.exe\" --hook claude", false);
        assert!(has_ours(&root));
        assert_eq!(root["hooks"]["SessionStart"].as_array().unwrap().len(), 2);
        assert!(strip(&mut root));
        assert!(!has_ours(&root));
        assert_eq!(root["hooks"]["SessionStart"][0]["hooks"][0]["command"], "other.exe");
        assert!(root["hooks"].get("Stop").is_none());
        assert_eq!(root["model"], "x");
    }

    #[test]
    fn strip_removes_empty_hooks_key() {
        let mut root = json!({});
        add(&mut root, CODEX_EVENTS, "\"C:/x/claude-overlay-hook.exe\" --hook codex", true);
        assert_eq!(root["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"], HOOK_TIMEOUT_SECS);
        assert!(root["hooks"]["Stop"][0]["hooks"][0]["commandWindows"].is_string());
        strip(&mut root);
        assert!(root.get("hooks").is_none());
    }

    #[test]
    fn enabling_twice_does_not_duplicate() {
        let mut root = json!({});
        for _ in 0..2 {
            strip(&mut root);
            add(&mut root, CLAUDE_EVENTS, "\"C:/x/claude-overlay-hook.exe\" --hook claude", false);
        }
        assert_eq!(root["hooks"]["Stop"].as_array().unwrap().len(), 1);
    }
}

//! Enable/disable our hook entries in Claude Code (`~/.claude/settings.json`) and
//! Codex (`$CODEX_HOME/hooks.json`). Our entries are found by the hook exe name in the
//! command, so no extra marker field is written. Unrelated hooks are kept. Writes are atomic.
use crate::protocol::{HOOK_TIMEOUT_SECS, QUESTION_HOOK_TIMEOUT_SECS};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub const HOOK_EXE: &str = "nubi-hook.exe";
/// Hook exe name used before the rename to Nubi. Entries with it are treated as ours, so they are
/// replaced on the next hook refresh.
const LEGACY_HOOK_EXE: &str = "claude-overlay-hook.exe";

struct EventSpec {
    name: &'static str,
    matcher: Option<&'static str>,
    timeout: Option<u64>,
}

const CLAUDE_EVENTS: &[EventSpec] = &[
    EventSpec {
        name: "SessionStart",
        matcher: None,
        timeout: Some(10),
    },
    EventSpec {
        name: "SessionEnd",
        matcher: None,
        timeout: Some(10),
    },
    EventSpec {
        name: "UserPromptSubmit",
        matcher: None,
        timeout: Some(10),
    },
    EventSpec {
        name: "PreToolUse",
        matcher: Some("*"),
        timeout: Some(QUESTION_HOOK_TIMEOUT_SECS),
    },
    EventSpec {
        name: "PostToolUse",
        matcher: Some("*"),
        timeout: Some(10),
    },
    EventSpec {
        name: "Notification",
        matcher: None,
        timeout: Some(10),
    },
    EventSpec {
        name: "Stop",
        matcher: None,
        timeout: Some(10),
    },
    EventSpec {
        name: "SubagentStop",
        matcher: None,
        timeout: Some(10),
    },
    EventSpec {
        name: "PermissionRequest",
        matcher: Some("*"),
        timeout: Some(HOOK_TIMEOUT_SECS),
    },
];

const CODEX_EVENTS: &[EventSpec] = &[
    EventSpec {
        name: "SessionStart",
        matcher: None,
        timeout: Some(10),
    },
    EventSpec {
        name: "SessionEnd",
        matcher: None,
        timeout: Some(3),
    },
    EventSpec {
        name: "UserPromptSubmit",
        matcher: None,
        timeout: Some(10),
    },
    EventSpec {
        name: "PreToolUse",
        matcher: None,
        timeout: Some(10),
    },
    EventSpec {
        name: "PostToolUse",
        matcher: None,
        timeout: Some(10),
    },
    EventSpec {
        name: "Interrupt",
        matcher: None,
        timeout: Some(10),
    },
    EventSpec {
        name: "PreModelRequest",
        matcher: None,
        timeout: Some(10),
    },
    EventSpec {
        name: "Stop",
        matcher: None,
        timeout: Some(10),
    },
    EventSpec {
        name: "SubagentStart",
        matcher: None,
        timeout: Some(10),
    },
    EventSpec {
        name: "SubagentStop",
        matcher: None,
        timeout: Some(10),
    },
    EventSpec {
        name: "PermissionRequest",
        matcher: None,
        timeout: Some(HOOK_TIMEOUT_SECS),
    },
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

/// Stable helper path that hooks call, so moving the app does not break hooks.
pub fn shim_path() -> Option<PathBuf> {
    Some(dirs::data_local_dir()?.join("nubi").join(HOOK_EXE))
}

/// Copy the separate helper to the stable path. A locked shim is left as is.
pub fn install_shim() -> std::io::Result<PathBuf> {
    if crate::isolation::development() {
        return Err(std::io::Error::other(
            "Hook installation is disabled in development mode",
        ));
    }
    let dst = shim_path().ok_or_else(|| std::io::Error::other("no local app data dir"))?;
    install_shim_from(&std::env::current_exe()?, dst)
}

fn install_shim_from(app_exe: &Path, dst: PathBuf) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dst.parent().unwrap())?;
    // The default install folder and stable shim folder differ only in case on Windows.
    // Keep the packaged source in a subfolder so installers never overwrite a held hook.
    let packaged = app_exe.parent().unwrap().join("helpers").join(HOOK_EXE);
    let src = if packaged.is_file() {
        packaged
    } else {
        app_exe.with_file_name(HOOK_EXE)
    };
    if src == dst && dst.is_file() {
        return Ok(dst);
    }
    match std::fs::copy(&src, &dst) {
        Ok(_) => Ok(dst),
        Err(e) if dst.exists() => {
            eprintln!("[nubi] shim copy skipped: {e}");
            Ok(dst)
        }
        Err(e) => Err(e),
    }
}

fn command_for(shim: &Path, provider: &str) -> String {
    format!(
        "\"{}\" --hook {}",
        shim.display().to_string().replace('\\', "/"),
        provider
    )
}

fn windows_command_for(shim: &Path, provider: &str) -> String {
    let path = shim
        .display()
        .to_string()
        .replace('\\', "/")
        .replace('\'', "''");
    // Invoke explicitly so both PowerShell and cmd hosts handle quoted executable paths.
    format!("powershell.exe -NoProfile -NonInteractive -Command \"& '{path}' --hook {provider}\"")
}

fn is_ours(h: &Value) -> bool {
    ["command", "commandWindows"].iter().any(|k| {
        h.get(*k)
            .and_then(|c| c.as_str())
            .is_some_and(|c| c.contains(HOOK_EXE) || c.contains(LEGACY_HOOK_EXE))
    })
}

/// Remove our entries. Returns true when anything was removed.
pub fn strip(root: &mut Value) -> bool {
    let Some(hooks) = root.get_mut("hooks").and_then(|h| h.as_object_mut()) else {
        return false;
    };
    let mut changed = false;
    let events: Vec<String> = hooks.keys().cloned().collect();
    for ev in events {
        let Some(groups) = hooks.get_mut(&ev).and_then(|g| g.as_array_mut()) else {
            continue;
        };
        for g in groups.iter_mut() {
            if let Some(list) = g.get_mut("hooks").and_then(|l| l.as_array_mut()) {
                let before = list.len();
                list.retain(|h| !is_ours(h));
                changed |= list.len() != before;
            }
        }
        groups.retain(|g| {
            g.get("hooks")
                .and_then(|l| l.as_array())
                .is_none_or(|l| !l.is_empty())
        });
        if groups.is_empty() {
            hooks.remove(&ev);
        }
    }
    if hooks.is_empty() {
        root.as_object_mut().map(|o| o.remove("hooks"));
    }
    changed
}

fn add(root: &mut Value, events: &[EventSpec], command: &str, windows_command: Option<&str>) {
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
        if let Some(windows_command) = windows_command {
            h["commandWindows"] = json!(windows_command);
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
        .is_some_and(|h| {
            h.values().filter_map(|g| g.as_array()).flatten().any(|g| {
                g.get("hooks")
                    .and_then(|l| l.as_array())
                    .is_some_and(|l| l.iter().any(is_ours))
            })
        })
}

fn read_json(path: &Path) -> Result<Value, String> {
    match std::fs::read_to_string(path) {
        Ok(t) if t.trim().is_empty() => Ok(json!({})),
        Ok(t) => serde_json::from_str(&t)
            .map_err(|e| format!("{} is not valid JSON: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e.to_string()),
    }
}

fn write_atomic(path: &Path, v: &Value) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let bak = path.with_extension("json.nubi.bak");
    if path.exists() && !bak.exists() {
        let _ = std::fs::copy(path, &bak);
    }
    let tmp = path.with_extension("json.nubi.tmp");
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
    path_for(provider)
        .and_then(|p| read_json(&p).ok())
        .is_some_and(|v| has_ours(&v))
}

// ---- Claude Code status line ----
// Nubi takes the `statusLine` slot so it gets Claude's usage numbers without calling the API.
// The user's previous status line is saved in %APPDATA%\nubi\statusline_prev.json and still
// runs (Nubi prints its output), and comes back when Claude hooks are turned off.

fn prev_statusline_path() -> PathBuf {
    crate::settings::data_dir().join("statusline_prev.json")
}

fn statusline_is_ours(v: &Value) -> bool {
    v.get("command")
        .and_then(|c| c.as_str())
        .is_some_and(|c| c.contains(HOOK_EXE) || c.contains(LEGACY_HOOK_EXE))
}

/// Command of the status line Nubi replaced, if there was one.
pub fn previous_statusline() -> Option<String> {
    let v: Value =
        serde_json::from_str(&std::fs::read_to_string(prev_statusline_path()).ok()?).ok()?;
    v.get("command")
        .and_then(|c| c.as_str())
        .map(String::from)
        .filter(|c| !c.trim().is_empty())
}

fn take_statusline(root: &mut Value, shim: &Path) {
    let cur = root.get("statusLine").cloned().unwrap_or(Value::Null);
    if !statusline_is_ours(&cur) {
        let _ = std::fs::create_dir_all(crate::settings::data_dir());
        let _ = std::fs::write(
            prev_statusline_path(),
            serde_json::to_string_pretty(&cur).unwrap_or_default(),
        );
    }
    let padding = cur.get("padding").cloned().unwrap_or(json!(0));
    let cmd = format!(
        "\"{}\" --statusline",
        shim.display().to_string().replace('\\', "/")
    );
    root["statusLine"] = json!({ "type": "command", "command": cmd, "padding": padding });
}

fn give_back_statusline(root: &mut Value) {
    let ours = root.get("statusLine").is_some_and(statusline_is_ours);
    if !ours {
        return;
    }
    let prev = std::fs::read_to_string(prev_statusline_path())
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok());
    match prev {
        Some(v) if v.is_object() => root["statusLine"] = v,
        _ => {
            root.as_object_mut().map(|o| o.remove("statusLine"));
        }
    }
    let _ = std::fs::remove_file(prev_statusline_path());
}

pub fn set_enabled(provider: &str, enable: bool) -> Result<(), String> {
    if crate::isolation::development() {
        return Err("Hook changes are disabled in development mode".into());
    }
    let path = path_for(provider).ok_or("no home directory")?;
    let mut root = read_json(&path)?;
    strip(&mut root);
    if enable {
        let shim = install_shim().map_err(|e| e.to_string())?;
        let cmd = command_for(&shim, provider);
        if provider == "codex" {
            let windows_cmd = windows_command_for(&shim, provider);
            add(&mut root, CODEX_EVENTS, &cmd, Some(&windows_cmd));
        } else {
            add(&mut root, CLAUDE_EVENTS, &cmd, None);
            take_statusline(&mut root, &shim);
        }
    } else if provider == "claude" {
        give_back_statusline(&mut root);
    }
    write_atomic(&path, &root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shim_uses_separate_helper_and_keeps_locked_copy() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = std::env::temp_dir().join(format!("nubi-shim-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let app = dir.join("nubi.exe");
        let helper = dir.join("helpers").join(HOOK_EXE);
        let dst = dir.join(HOOK_EXE);
        std::fs::create_dir_all(helper.parent().unwrap()).unwrap();
        std::fs::write(&app, b"large GUI payload").unwrap();
        std::fs::write(&helper, b"hook").unwrap();
        assert_eq!(install_shim_from(&app, dst.clone()).unwrap(), dst);
        assert_eq!(std::fs::read(&dst).unwrap(), b"hook");
        let locked = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&dst)
            .unwrap();
        std::fs::write(&helper, b"new hook").unwrap();
        assert_eq!(install_shim_from(&app, dst.clone()).unwrap(), dst);
        assert_eq!(std::fs::read(&dst).unwrap(), b"hook");
        drop(locked);
        install_shim_from(&app, dst.clone()).unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"new hook");
        std::fs::remove_file(&helper).unwrap();
        assert_eq!(install_shim_from(&app, dst.clone()).unwrap(), dst);
        assert_eq!(std::fs::read(&dst).unwrap(), b"new hook");
        std::fs::remove_file(&dst).unwrap();
        assert!(install_shim_from(&app, dst).is_err());
        std::fs::remove_file(&app).unwrap();
        std::fs::remove_dir(helper.parent().unwrap()).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn portable_helper_is_required_and_installs_separately() {
        let dir =
            std::env::temp_dir().join(format!("nubi-missing-hook-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let app = dir.join("nubi.exe");
        let dst = dir.join("stable").join(HOOK_EXE);
        std::fs::write(&app, b"GUI payload").unwrap();
        assert!(install_shim_from(&app, dst.clone()).is_err());
        assert!(!dst.exists());
        let helper = dir.join(HOOK_EXE);
        std::fs::write(&helper, b"portable hook").unwrap();
        assert_eq!(install_shim_from(&app, dst.clone()).unwrap(), dst);
        assert_eq!(std::fs::read(&dst).unwrap(), b"portable hook");
        std::fs::remove_file(helper).unwrap();
        std::fs::remove_file(&dst).unwrap();
        std::fs::remove_file(app).unwrap();
        std::fs::remove_dir(dst.parent().unwrap()).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn add_then_strip_keeps_foreign_hooks() {
        let mut root = json!({
            "model": "x",
            "hooks": { "SessionStart": [ { "hooks": [ { "type": "command", "command": "other.exe" } ] } ] }
        });
        add(
            &mut root,
            CLAUDE_EVENTS,
            "\"C:/x/nubi-hook.exe\" --hook claude",
            None,
        );
        assert!(has_ours(&root));
        assert_eq!(root["hooks"]["SessionStart"].as_array().unwrap().len(), 2);
        assert!(strip(&mut root));
        assert!(!has_ours(&root));
        assert_eq!(
            root["hooks"]["SessionStart"][0]["hooks"][0]["command"],
            "other.exe"
        );
        assert!(root["hooks"].get("Stop").is_none());
        assert_eq!(root["model"], "x");
    }

    #[test]
    fn strip_removes_empty_hooks_key() {
        let mut root = json!({});
        let windows_cmd = windows_command_for(Path::new("C:/x/nubi-hook.exe"), "codex");
        add(
            &mut root,
            CODEX_EVENTS,
            "\"C:/x/nubi-hook.exe\" --hook codex",
            Some(&windows_cmd),
        );
        assert_eq!(
            root["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"],
            HOOK_TIMEOUT_SECS
        );
        assert!(root["hooks"]["PostToolUse"][0]["hooks"][0]["commandWindows"].is_string());
        assert!(root["hooks"]["Interrupt"][0]["hooks"][0]["commandWindows"].is_string());
        assert!(root["hooks"]["PreModelRequest"][0]["hooks"][0]["commandWindows"].is_string());
        assert!(root["hooks"]["Stop"][0]["hooks"][0]["commandWindows"].is_string());
        assert_eq!(
            root["hooks"]["Stop"][0]["hooks"][0]["commandWindows"],
            windows_cmd
        );
        strip(&mut root);
        assert!(root.get("hooks").is_none());
    }

    #[test]
    fn enabling_twice_does_not_duplicate() {
        let mut root = json!({});
        for _ in 0..2 {
            strip(&mut root);
            add(
                &mut root,
                CLAUDE_EVENTS,
                "\"C:/x/nubi-hook.exe\" --hook claude",
                None,
            );
        }
        assert_eq!(root["hooks"]["Stop"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn windows_command_quotes_spaces_and_apostrophes() {
        assert_eq!(
            windows_command_for(Path::new("C:\\Users\\O'Brien Smith\\nubi-hook.exe"), "codex"),
            "powershell.exe -NoProfile -NonInteractive -Command \"& 'C:/Users/O''Brien Smith/nubi-hook.exe' --hook codex\""
        );
    }
}

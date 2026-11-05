//! App glue: shared state, alerts (pill / toast / queue), DND, commands, tray, hotkey.
use crate::protocol::{HookMsg, Reply};
use crate::settings::{self, Settings};
use crate::state::{now_ms, Alert, Inner};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::oneshot;

pub type App = AppHandle;

pub struct Shared(pub Mutex<Inner>);

pub fn with<R>(app: &App, f: impl FnOnce(&mut Inner) -> R) -> R {
    let st = app.state::<Shared>();
    let mut g = st.0.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut g)
}

pub fn dnd_active(i: &Inner) -> bool {
    if i.dnd_manual {
        return true;
    }
    if let (Some(a), Some(b)) = (i.settings.quiet_start, i.settings.quiet_end) {
        use chrono::Timelike;
        let h = chrono::Local::now().hour() as u8;
        let inside = if a <= b { h >= a && h < b } else { h >= a || h < b };
        if inside {
            return true;
        }
    }
    i.settings.follow_quiet_time && crate::winutil::quiet_time_active()
}

pub fn emit_state(app: &App) {
    let snap = with(app, |i| {
        let d = dnd_active(i);
        i.snapshot(d)
    });
    let _ = app.emit("state", snap);
}

pub fn set_pipe_error(app: &App, e: String) {
    eprintln!("[nubi] {e}");
    with(app, |i| i.pipe_error = Some(e));
    emit_state(app);
}

/// Returns true when a question must be released directly to the focused terminal.
pub fn on_hook(app: &App, m: &HookMsg) -> bool {
    // Usage numbers from Claude Code's status line (see hook_client::statusline).
    if m.event == "StatusLine" {
        if let Some(rl) = &m.tool_input {
            let windows = crate::usage::parse_statusline(rl);
            if !windows.is_empty() {
                with(app, |i| {
                    i.claude_live_ms = crate::state::now_ms();
                    i.usage.insert(
                        "claude".into(),
                        crate::state::Usage {
                            provider: "claude".into(),
                            status: "ok".into(),
                            message: None,
                            windows,
                            fetched_ms: Some(crate::state::now_ms()),
                        },
                    );
                });
                emit_state(app);
            }
        }
        return false;
    }
    let terminal_question = crate::protocol::is_question(&m.provider, &m.event, m.tool_name.as_deref())
        && crate::protocol::question_in_terminal(m, crate::winutil::pids_focused(&m.pids));
    let (alert, wanted) = with(app, |i| (i.on_event(m), i.settings.wants(&m.provider)));
    // A CLI can replace its logical conversation without changing its process. Old rows must
    // not retain permission to close or compact the new conversation in that process.
    let key = format!("{}:{}", m.provider, m.session_id);
    with(app, |i| {
        let target = i.sessions.get(&key).and_then(|s| s.action_target.clone());
        if let Some(target) = target {
            for s in i.sessions.values_mut().filter(|s| s.key != key) {
                if s.action_target.as_ref() == Some(&target) {
                    s.action_target = None;
                    s.compact_queued = false;
                }
            }
        }
    });
    if let (Some(a), true) = (alert, wanted) {
        if terminal_question {
            with(app, |i| i.push_alert(a));
        } else {
            show_alert(app, a);
        }
    }
    emit_state(app);
    terminal_question
}

pub fn island_mode(app: &App) -> bool {
    with(app, |i| i.settings.display_mode == "island" && i.settings.edge == "top")
}

pub fn takeover_secs(app: &App) -> u64 {
    with(app, |i| i.settings.takeover_secs as u64)
}

pub fn questions_mode(app: &App) -> String {
    with(app, |i| i.settings.questions_mode.clone())
}

/// Show the notification island (it hides itself when it has nothing left to show).
pub fn show_island(app: &App) {
    if island_mode(app) {
        return; // the top island is always shown and draws requests itself
    }
    if let Some(w) = app.get_webview_window("overlay") {
        let s = with(app, |i| i.settings.clone());
        place_overlay(app, &s);
        let _ = w.show();
    }
}

pub fn add_pending(app: &App, m: &HookMsg, tx: oneshot::Sender<Reply>, ttl_ms: u64) -> u64 {
    let id = with(app, |i| i.add_pending(m, tx, ttl_ms));
    if !with(app, |i| dnd_active(i)) {
        show_island(app);
    }
    emit_state(app);
    id
}

/// `timed_out`: the takeover time ran out with no answer, so the request moves to `missed`.
pub fn remove_pending(app: &App, id: u64, timed_out: bool) {
    with(app, |i| {
        if let Some(p) = i.pending.remove(&id) {
            if timed_out {
                i.missed.push(crate::state::Missed {
                    id: p.id,
                    kind: p.kind,
                    provider: p.provider,
                    session_key: p.session_key,
                    folder: p.folder,
                    name: p.name,
                    created_ms: p.created_ms,
                });
            }
        }
    });
    emit_state(app);
}

/// Pause (pointer over the card) or resume a request's countdown.
#[tauri::command]
pub fn hold_pending(app: App, id: u64, on: bool) {
    with(&app, |i| {
        if let Some(p) = i.pending.get_mut(&id) {
            p.held = on;
        }
    });
    emit_state(&app);
}

/// One tick of a request's countdown. While held, the deadline moves forward by `step_ms`, but
/// never past `hard_ms` (the hook's own deadline). True when the time is up.
pub fn pending_tick(app: &App, id: u64, step_ms: u64, hard_ms: u64) -> bool {
    let now = now_ms();
    let (up, moved) = with(app, |i| match i.pending.get_mut(&id) {
        Some(p) => {
            let mut moved = false;
            if p.held && p.expires_ms < hard_ms {
                p.expires_ms = (p.expires_ms + step_ms).min(hard_ms);
                moved = true;
            }
            (now >= p.expires_ms, moved)
        }
        None => (true, false),
    });
    if moved && now % 1000 < step_ms {
        emit_state(app); // about once a second, so cards show the paused time
    }
    up
}

/// Dismiss one missed request, or all of them when `id` is None.
#[tauri::command]
pub fn dismiss_missed(app: App, id: Option<u64>) {
    with(&app, |i| i.missed.retain(|x| id.is_some_and(|id| x.id != id)));
    emit_state(&app);
}

/// Pill normally; Windows toast when a full-screen app is in front; queue only during DND.
pub fn show_alert(app: &App, mut a: Alert) {
    let (dnd, s) = with(app, |i| (dnd_active(i), i.settings.clone()));
    a.queued = dnd;
    with(app, |i| i.push_alert(a.clone()));
    if dnd {
        return;
    }
    if s.toast_when_fullscreen && crate::winutil::fullscreen_active() {
        if s.sounds {
            crate::winutil::beep(&a.kind);
        }
        let _ = app
            .notification()
            .builder()
            .title(format!("{} · {}", provider_name(&a.provider), a.title))
            .body(&a.body)
            .show();
        return;
    }
    if s.display_mode == "island" && s.edge == "top" {
        if let Some(w) = app.get_webview_window("island") {
            let mut payload = serde_json::to_value(&a).unwrap_or_default();
            payload["sound"] = serde_json::json!(s.sounds);
            let _ = w.emit_to(w.label(), "pill", payload);
        }
        return;
    }
    if let Some(w) = app.get_webview_window("overlay") {
        place_overlay(app, &s);
        let _ = w.show();
        let mut payload = serde_json::to_value(&a).unwrap_or_default();
        payload["sound"] = serde_json::json!(s.sounds);
        let _ = w.emit_to(w.label(), "pill", payload);
    }
}

fn provider_name(p: &str) -> &'static str {
    if p == "codex" {
        "Codex"
    } else {
        "Claude"
    }
}

/// Alert island placement on the chosen monitor.
pub fn place_overlay<R: Runtime>(app: &AppHandle<R>, s: &Settings) {
    let Some(w) = app.get_webview_window("overlay") else { return };
    let mons = app.available_monitors().unwrap_or_default();
    let Some(m) = mons.get(s.monitor).or(mons.first()) else { return };
    let (mp, ms) = (m.position(), m.size());
    let ws = w.outer_size().unwrap_or_default();
    let (mw, mh, ww, wh) = (ms.width as i32, ms.height as i32, ws.width as i32, ws.height as i32);
    // Navi: the alert island sits at the top centre of the screen, whatever the display mode.
    let _ = (mh, wh);
    let (x, y) = ((mw - ww) / 2, 0);
    let _ = w.set_position(PhysicalPosition::new(mp.x + x, mp.y + y));
}

/// Place every display surface for the current settings.
pub fn place_all(app: &App, s: &Settings) {
    place_overlay(app, s);
    crate::island::place(app, s);
    crate::ring::place(app);
    crate::taskbar::place_now(app);
}

pub fn open_panel(app: &App) {
    if let Some(w) = app.get_webview_window("panel") {
        let _ = w.show();
        let _ = w.unminimize();
        // Windows may refuse focus to a background app; a topmost flip still brings it to the front.
        let _ = w.set_always_on_top(true);
        let _ = w.set_focus();
        let _ = w.set_always_on_top(false);
    }
    emit_state(app);
}

pub fn toggle_dnd(app: &App) {
    let on = with(app, |i| {
        i.dnd_manual = !i.dnd_manual;
        i.dnd_manual
    });
    let _ = on; // queued alerts stay in the panel list
    emit_state(app);
    refresh_menu(app);
}

static LAUNCH_SHORTCUT: Mutex<Option<Shortcut>> = Mutex::new(None);

fn parse_hotkey(combo: &str) -> Result<Option<Shortcut>, String> {
    if combo.trim().is_empty() {
        return Ok(None);
    }
    combo.parse().map(Some).map_err(|e| format!("bad hotkey '{combo}': {e}"))
}

pub fn register_hotkey(app: &App, s: &Settings) -> Result<(), String> {
    let dnd = parse_hotkey(&s.dnd_hotkey)?;
    let launch = if s.launcher_enabled { parse_hotkey(&s.launch_hotkey)? } else { None };
    if dnd.is_some() && dnd == launch {
        return Err("The two keyboard shortcuts must differ.".into());
    }
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    *LAUNCH_SHORTCUT.lock().unwrap() = launch;
    for sc in [dnd, launch].into_iter().flatten() {
        gs.register(sc).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn refresh_hooks(app: &App) {
    let map: HashMap<String, bool> = ["claude", "codex"]
        .iter()
        .map(|p| (p.to_string(), crate::hooks_config::is_enabled(p)))
        .collect();
    with(app, |i| i.hooks = map);
}

// ---------- commands (called from the panel) ----------

#[tauri::command]
pub fn get_state(app: App) -> serde_json::Value {
    with(&app, |i| {
        let d = dnd_active(i);
        i.snapshot(d)
    })
}

#[tauri::command]
pub fn answer_permission(app: App, id: u64, decision: String) -> Result<(), String> {
    if decision != "allow" && decision != "deny" {
        return Err("decision must be allow or deny".into());
    }
    let message = (decision == "deny").then(|| "Denied from Nubi".to_string());
    let r = with(&app, |i| i.answer(id, Reply { decision, message, answers: None }));
    emit_state(&app);
    r
}

/// Answer a Claude AskUserQuestion. Empty answers = "answer in terminal".
#[tauri::command]
pub fn answer_question(app: App, id: u64, answers: serde_json::Map<String, serde_json::Value>) -> Result<(), String> {
    let decision = if answers.is_empty() { "skip" } else { "answer" };
    let r = with(&app, |i| i.answer(id, Reply { decision: decision.into(), message: None, answers: Some(answers) }));
    emit_state(&app);
    r
}

#[tauri::command]
pub fn jump_session(app: App, key: String) -> Result<bool, String> {
    let pids = with(&app, |i| i.sessions.get(&key).map(|s| s.pids.clone())).ok_or("unknown session")?;
    Ok(crate::winutil::focus_pids(&pids))
}

#[tauri::command]
pub fn close_session(app: App, key: String) -> Result<(), String> {
    let target = with(&app, |i| i.sessions.get(&key).and_then(|s| s.action_target.clone()))
        .ok_or("Cannot identify this agent process. Use Jump to close it in the terminal.")?;
    target.close()?;
    with(&app, |i| {
        i.sessions.remove(&key);
        i.pending.retain(|_, p| p.session_key != key);
        i.missed.retain(|p| p.session_key != key);
    });
    emit_state(&app);
    Ok(())
}

#[tauri::command]
pub fn new_session_terminal(app: App, key: String) -> Result<(), String> {
    let cwd = with(&app, |i| i.sessions.get(&key).and_then(|s| s.cwd.clone()))
        .ok_or("This session has no recorded folder.")?;
    crate::session_actions::new_terminal(&cwd)
}

#[tauri::command]
pub fn queue_session_compact(app: App, key: String, queued: bool) -> Result<(), String> {
    let target = with(&app, |i| {
        let s = i.sessions.get_mut(&key).ok_or("Unknown session")?;
        if !queued {
            s.compact_queued = false;
            return Ok(None);
        }
        if s.compact_queued { return Ok(None); }
        let target = s.action_target.clone().ok_or("Cannot identify this agent process. Use Jump and run /compact in the terminal.")?;
        s.compact_queued = true;
        s.action_error = None;
        Ok::<_, String>(Some(target))
    })?;
    emit_state(&app);
    let Some(target) = target else { return Ok(()) };
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            if !target.available() {
                with(&app, |i| {
                    if let Some(s) = i.sessions.get_mut(&key) {
                        if s.action_target.as_ref() == Some(&target) && s.compact_queued {
                            s.compact_queued = false;
                            s.action_error = Some("Queued compact cancelled: the session process has exited.".into());
                        }
                    }
                });
                emit_state(&app);
                return;
            }
            let ready = with(&app, |i| {
                let pending = i.pending.values().any(|p| p.session_key == key);
                let Some(s) = i.sessions.get_mut(&key) else { return None };
                if !s.compact_queued || s.action_target.as_ref() != Some(&target) { return None; }
                if !pending && compact_ready(s, now_ms()) {
                    s.compact_queued = false;
                    Some(true)
                } else { Some(false) }
            });
            match ready {
                None => return,
                Some(false) => continue,
                Some(true) => break,
            }
        }
        let result = target.compact();
        with(&app, |i| {
            if let Some(s) = i.sessions.get_mut(&key) {
                if s.action_target.as_ref() == Some(&target) {
                    s.action_error = result.err();
                }
            }
        });
        emit_state(&app);
    });
    Ok(())
}

fn compact_ready(s: &crate::state::Session, now: u64) -> bool {
    matches!(s.state.as_str(), "idle" | "done")
        && now.saturating_sub(s.last_event_ms) >= 500
}

#[cfg(test)]
mod session_action_tests {
    use super::*;
    #[test]
    fn compact_waits_through_work_and_permissions_for_both_providers() {
        for provider in ["claude", "codex"] {
            let mut i = Inner::default();
            let mut m = HookMsg { provider: provider.into(), session_id: "queued".into(), event: "SessionStart".into(), ..Default::default() };
            let key = format!("{provider}:queued");
            i.on_event(&m);
            i.sessions.get_mut(&key).unwrap().compact_queued = true;
            for event in ["UserPromptSubmit", "PermissionRequest", "PostToolUse", "Stop"] {
                m.event = event.into();
                i.on_event(&m);
                let s = &i.sessions[&key];
                assert!(s.compact_queued, "Queue lost on {event}");
                assert!(!compact_ready(s, s.last_event_ms + 499));
                assert_eq!(compact_ready(s, s.last_event_ms + 500), event == "Stop");
            }
            m.event = "SessionEnd".into();
            i.on_event(&m);
            assert!(!i.sessions.contains_key(&key));
        }
    }
}

#[tauri::command]
pub fn save_settings(app: App, settings: Settings) -> Result<(), String> {
    crate::desktime::validate_settings(&settings)?;
    register_hotkey(&app, &settings)?;
    settings::apply_auto_start(settings.auto_start)?;
    settings::save(&settings)?;
    with(&app, |i| i.settings = settings.clone());
    place_all(&app, &settings);
    crate::poller::kick(&app);
    crate::desktime::emit(&app);
    emit_state(&app);
    refresh_menu(&app);
    Ok(())
}

#[tauri::command]
pub fn set_dnd(app: App, on: bool) {
    with(&app, |i| i.dnd_manual = on);
    emit_state(&app);
    refresh_menu(&app);
}

#[tauri::command]
pub fn set_hooks(app: App, provider: String, enable: bool) -> Result<(), String> {
    if provider != "claude" && provider != "codex" {
        return Err("unknown provider".into());
    }
    crate::hooks_config::set_enabled(&provider, enable)?;
    refresh_hooks(&app);
    emit_state(&app);
    refresh_menu(&app);
    Ok(())
}

#[tauri::command]
pub fn test_alert(app: App) {
    show_alert(
        &app,
        Alert {
            ts: now_ms(),
            provider: "claude".into(),
            kind: "done".into(),
            title: "Task complete".into(),
            body: "test notification".into(),
            session_key: None,
            queued: false,
        },
    );
    emit_state(&app);
}

#[tauri::command]
pub fn refresh_usage(app: App) {
    crate::poller::force(&app);
}

/// Re-open request cards in the island (from the notch, ring or strip).
#[tauri::command]
pub fn show_requests(app: App) {
    if island_mode(&app) {
        if let Some(w) = app.get_webview_window("island") {
            let _ = w.emit_to(w.label(), "show_requests", ());
        }
        return;
    }
    show_island(&app);
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.emit_to(w.label(), "show_requests", ());
    }
}

/// Open the GitHub bug report form (.github/ISSUE_TEMPLATE/bug_report.yml) in the browser, with
/// the Nubi and Windows versions filled in through the form's field ids.
#[tauri::command]
pub fn report_issue(app: App) {
    use std::os::windows::process::CommandExt;
    let ver = app.package_info().version.to_string();
    let os = std::process::Command::new("cmd")
        .args(["/C", "ver"])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let enc = |t: &str| -> String {
        t.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
                _ => format!("%{b:02X}"),
            })
            .collect()
    };
    let url = format!(
        "https://github.com/hitraj2003/nubi/issues/new?template=bug_report.yml&nubi-version={}&windows={}",
        enc(&ver),
        enc(&os)
    );
    // explorer opens a URL in the default browser without a console window.
    let _ = std::process::Command::new("explorer").arg(url).spawn();
}

/// After an update, say so once: an alert pill "Updated to X" (the panel's About has the notes).
/// The last version that ran is kept in %APPDATA%\nubi\last_version. A missing file with an
/// existing settings.json means the user came from a version before this check (0.1.1 or older).
fn announce_update(app: &App) {
    let dir = settings::data_dir();
    let file = dir.join("last_version");
    let ver = app.package_info().version.to_string();
    let last = std::fs::read_to_string(&file).ok().map(|t| t.trim().to_string());
    let updated = match &last {
        Some(v) => v != &ver,
        None => dir.join("settings.json").exists(),
    };
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(&file, &ver);
    if !updated {
        return;
    }
    let a = app.clone();
    std::thread::spawn(move || {
        // Let the windows load first.
        std::thread::sleep(std::time::Duration::from_secs(4));
        show_alert(
            &a,
            Alert {
                ts: now_ms(),
                provider: "claude".into(),
                kind: "done".into(),
                title: format!("Nubi updated to {ver}"),
                body: "See what's new in Settings > About".into(),
                session_key: None,
                queued: false,
            },
        );
        emit_state(&a);
    });
}

/// About > What's new: the GitHub release notes for this version.
#[tauri::command]
pub fn open_release_notes(app: App) {
    let url = format!("https://github.com/hitraj2003/nubi/releases/tag/v{}", app.package_info().version);
    let _ = std::process::Command::new("explorer").arg(url).spawn();
}

/// Page errors end up in the app log.
#[tauri::command]
pub fn log_js(msg: String) {
    eprintln!("[page] {}", msg.chars().take(500).collect::<String>());
}

#[tauri::command]
pub fn hide_overlay(app: App) {
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.hide();
    }
}

#[tauri::command]
pub fn show_panel(app: App) {
    open_panel(&app);
}

#[tauri::command]
pub fn monitors(app: App) -> Vec<String> {
    let names = crate::winutil::monitor_names();
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(n, m)| {
            let gdi = m.name().cloned().unwrap_or_default();
            let friendly = names.get(&gdi).cloned().unwrap_or_else(|| format!("{}x{}", m.size().width, m.size().height));
            format!("Display {}: {}", n + 1, friendly)
        })
        .collect()
}

// ---------- tray menu: the common settings, one click away ----------

/// (setting key, submenu title, [(value, label)])
const CHOICES: &[(&str, &str, &[(&str, &str)])] = &[
    ("display_mode", "Display", &[("island", "Notch"), ("ring", "Floating ring"), ("taskbar", "Taskbar strip")]),
    ("providers", "Providers", &[("both", "Claude and Codex"), ("claude", "Claude"), ("codex", "Codex")]),
    ("questions_mode", "Answer Claude's questions in Nubi", &[("off", "Off, answer in terminal"), ("system", "Choices only"), ("full", "Full")]),
    ("request_layout", "Waiting cards", &[("stack", "Stacked"), ("list", "Side by side")]),
    ("edge", "Notch edge", &[("right", "Right"), ("left", "Left"), ("top", "Top"), ("bottom", "Bottom")]),
    ("taskbar_side", "Taskbar side", &[("right", "Right, beside the tray"), ("left", "Left")]),
];

fn setting_value(s: &Settings, key: &str) -> String {
    serde_json::to_value(s).ok().and_then(|v| v.get(key).and_then(|x| x.as_str()).map(String::from)).unwrap_or_default()
}

pub fn build_menu(app: &App) -> tauri::Result<Menu<tauri::Wry>> {
    let (s, dnd) = with(app, |i| (i.settings.clone(), i.dnd_manual));
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(app, "open", "Open settings", true, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    let (key, _, opts) = CHOICES[0];
    let cur = setting_value(&s, key);
    let style = Submenu::new(app, "Change style", true)?;
    for (val, label) in opts {
        style.append(&CheckMenuItem::with_id(app, format!("set:{key}:{val}"), *label, true, cur == *val, None::<&str>)?)?;
    }
    menu.append(&style)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&CheckMenuItem::with_id(app, "dnd", "Do not disturb", true, dnd, None::<&str>)?)?;
    menu.append(&CheckMenuItem::with_id(app, "sounds", "Sounds", true, s.sounds, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "refresh", "Refresh usage", true, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?)?;
    Ok(menu)
}

/// Rebuild the tray menu so its check marks follow the current settings.
pub fn refresh_menu(app: &App) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id("main"), build_menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn apply_settings(app: &App, s: Settings) {
    if let Err(e) = settings::save(&s) {
        eprintln!("[nubi] save settings: {e}");
    }
    with(app, |i| i.settings = s.clone());
    place_all(app, &s);
    crate::poller::kick(app);
    emit_state(app);
    refresh_menu(app);
}

fn on_menu(app: &App, id: &str) {
    if let Some(rest) = id.strip_prefix("set:") {
        let Some((key, val)) = rest.split_once(':') else { return };
        let s = with(app, |i| i.settings.clone());
        let Ok(mut v) = serde_json::to_value(&s) else { return };
        v[key] = serde_json::json!(val);
        if let Ok(ns) = serde_json::from_value::<Settings>(v) {
            apply_settings(app, ns);
        }
        return;
    }
    if let Some(n) = id.strip_prefix("takeover:").and_then(|n| n.parse::<u32>().ok()) {
        let mut s = with(app, |i| i.settings.clone());
        s.takeover_secs = n;
        apply_settings(app, s);
        return;
    }
    if let Some(p) = id.strip_prefix("hooks:") {
        let on = with(app, |i| i.hooks.get(p).copied().unwrap_or(false));
        if let Err(e) = crate::hooks_config::set_enabled(p, !on) {
            eprintln!("[nubi] hooks {p}: {e}");
        }
        refresh_hooks(app);
        emit_state(app);
        refresh_menu(app);
        return;
    }
    match id {
        "open" => open_panel(app),
        "dnd" => toggle_dnd(app),
        "sounds" => {
            let mut s = with(app, |i| i.settings.clone());
            s.sounds = !s.sounds;
            apply_settings(app, s);
        }
        "fold" => {
            let mut s = with(app, |i| i.settings.clone());
            s.notch_on_hover = !s.notch_on_hover;
            apply_settings(app, s);
        }
        "refresh" => crate::poller::force(app),
        "test" => test_alert(app.clone()),
        "quit" => app.exit(0),
        _ => {}
    }
}

// ---------- setup ----------

pub fn setup(app: &App) -> tauri::Result<()> {
    let s = settings::load();
    with(app, |i| i.settings = s.clone());
    refresh_hooks(app);
    // Hooks that are on get rewritten with this build's shim and event list, so an update
    // (new events, longer question timeout) reaches settings that were written by an older build.
    for p in ["claude", "codex"] {
        if with(app, |i| i.hooks.get(p).copied().unwrap_or(false)) {
            if let Err(e) = crate::hooks_config::set_enabled(p, true) {
                eprintln!("[nubi] refresh {p} hooks: {e}");
            }
        }
    }

    let menu = build_menu(app)?;
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("Nubi")
        .menu(&menu)
        .on_menu_event(|app, ev| on_menu(app, ev.id.as_ref()))
        .build(app)?;

    if let Err(e) = register_hotkey(app, &s) {
        eprintln!("[nubi] {e}");
    }
    for l in ["island", "overlay", "ring", "ringtip", "taskbar"] {
        if let Some(w) = app.get_webview_window(l) {
            let _ = w.set_shadow(false);
        }
    }
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.set_ignore_cursor_events(true);
    }
    place_all(app, &s);
    announce_update(app);
    crate::notch::start_watch(app.clone());
    #[cfg(debug_assertions)]
    crate::pipe::start_sim(app.clone());
    crate::taskbar::start(app.clone());
    if let Some(p) = app.get_webview_window("panel") {
        let p2 = p.clone();
        p.on_window_event(move |e| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = e {
                api.prevent_close();
                let _ = p2.hide();
            }
        });
    }
    let a = app.clone();
    tauri::async_runtime::spawn(async move { crate::pipe::serve(a).await });
    crate::poller::start(app.clone());
    crate::desktime::start(app.clone());
    std::thread::spawn(crate::native_host::register);
    Ok(())
}

pub fn shortcut_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, sc, ev| {
            if ev.state() != ShortcutState::Pressed {
                return;
            }
            if Some(*sc) == *LAUNCH_SHORTCUT.lock().unwrap() {
                crate::launcher::trigger(app);
            } else {
                toggle_dnd(app);
            }
        })
        .build()
}

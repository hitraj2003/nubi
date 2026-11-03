//! App glue: shared state, alerts (pill / toast / queue), DND, commands, tray, hotkey.
use crate::protocol::{HookMsg, Reply};
use crate::settings::{self, Settings};
use crate::state::{now_ms, Alert, Inner};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::oneshot;

pub type App = AppHandle;

pub struct Shared(pub Mutex<Inner>);

fn with<R>(app: &App, f: impl FnOnce(&mut Inner) -> R) -> R {
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
    eprintln!("[claude-overlay] {e}");
    with(app, |i| i.pipe_error = Some(e));
    emit_state(app);
}

pub fn on_hook(app: &App, m: &HookMsg) {
    let (alert, wanted) = with(app, |i| (i.on_event(m), i.settings.wants(&m.provider)));
    if let (Some(a), true) = (alert, wanted) {
        show_alert(app, a);
    }
    emit_state(app);
}

pub fn add_pending(app: &App, m: &HookMsg, tx: oneshot::Sender<Reply>, ttl_ms: u64) -> u64 {
    let id = with(app, |i| i.add_pending(m, tx, ttl_ms));
    let s = with(app, |i| i.settings.clone());
    crate::notch::place(app, &s);
    emit_state(app);
    id
}

pub fn remove_pending(app: &App, id: u64) {
    with(app, |i| i.pending.remove(&id));
    let s = with(app, |i| i.settings.clone());
    crate::notch::place(app, &s);
    emit_state(app);
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
    if let Some(w) = app.get_webview_window("overlay") {
        place_overlay(app, &s);
        let _ = w.show();
        let mut payload = serde_json::to_value(&a).unwrap_or_default();
        payload["sound"] = serde_json::json!(s.sounds);
        let _ = w.emit("pill", payload);
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
    crate::notch::place(app, s);
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
    if !on {
        // Leaving DND: flush nothing loud, the queued alerts stay in the panel list.
    }
    emit_state(app);
}

pub fn register_hotkey(app: &App, combo: &str) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    if combo.trim().is_empty() {
        return Ok(());
    }
    let sc: Shortcut = combo.parse().map_err(|e| format!("bad hotkey '{combo}': {e}"))?;
    gs.register(sc).map_err(|e| e.to_string())
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
    let message = (decision == "deny").then(|| "Denied from claude-overlay".to_string());
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
pub fn save_settings(app: App, settings: Settings) -> Result<(), String> {
    register_hotkey(&app, &settings.dnd_hotkey)?;
    settings::save(&settings)?;
    with(&app, |i| i.settings = settings.clone());
    place_all(&app, &settings);
    crate::poller::kick(&app);
    emit_state(&app);
    Ok(())
}

#[tauri::command]
pub fn set_dnd(app: App, on: bool) {
    with(&app, |i| i.dnd_manual = on);
    emit_state(&app);
}

#[tauri::command]
pub fn set_hooks(app: App, provider: String, enable: bool) -> Result<(), String> {
    if provider != "claude" && provider != "codex" {
        return Err("unknown provider".into());
    }
    crate::hooks_config::set_enabled(&provider, enable)?;
    refresh_hooks(&app);
    emit_state(&app);
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
    crate::poller::kick(&app);
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
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(n, m)| format!("{}: {} ({}x{})", n, m.name().cloned().unwrap_or_default(), m.size().width, m.size().height))
        .collect()
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
                eprintln!("[claude-overlay] refresh {p} hooks: {e}");
            }
        }
    }

    let open = MenuItem::with_id(app, "open", "Open panel", true, None::<&str>)?;
    let dnd = MenuItem::with_id(app, "dnd", "Toggle Do Not Disturb", true, None::<&str>)?;
    let test = MenuItem::with_id(app, "test", "Test notification", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &dnd, &test, &sep, &quit])?;
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("claude-overlay")
        .menu(&menu)
        .on_menu_event(|app, ev| match ev.id.as_ref() {
            "open" => open_panel(app),
            "dnd" => toggle_dnd(app),
            "test" => test_alert(app.clone()),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    if let Err(e) = register_hotkey(app, &s.dnd_hotkey) {
        eprintln!("[claude-overlay] {e}");
    }
    place_all(app, &s);
    crate::notch::start_watch(app.clone());
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
    Ok(())
}

pub fn shortcut_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _sc, ev| {
            if ev.state() == ShortcutState::Pressed {
                toggle_dnd(app);
            }
        })
        .build()
}

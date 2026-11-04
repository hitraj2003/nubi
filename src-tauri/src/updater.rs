//! Signed updates with release notes and cancellable notifications.
use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};
use tauri_plugin_updater::{Update, UpdaterExt};

const STARTUP_DELAY_SECS: u64 = 3;
const CHECK_INTERVAL_SECS: u64 = 6 * 3600;
const OFFER_MS: u64 = 10_000;
const STARTUP_INSTALL_MS: u64 = 5_000;

#[derive(Clone, Serialize)]
pub struct Notice {
    id: u64, version: String, notes: String, phase: String,
    automatic: bool, expires_ms: u64, held: bool, message: String,
}

#[derive(Default)]
pub struct UpdateState(Mutex<Inner>);

#[derive(Default)]
struct Inner {
    candidate: Option<Update>, notice: Option<Notice>, next_id: u64,
    checking: bool, installing: bool, dismissed_version: Option<String>,
}

#[derive(Debug, PartialEq)]
enum Tick { Stop, Wait, Dismiss, Install }

impl Inner {
    fn tick(&mut self, id: u64, now: u64, elapsed: u64) -> Tick {
        let Some(n) = self.notice.as_mut().filter(|n| n.id == id) else { return Tick::Stop; };
        if n.phase == "error" { return Tick::Stop; }
        if n.held {
            if n.expires_ms > 0 { n.expires_ms = n.expires_ms.saturating_add(elapsed); }
            return Tick::Wait;
        }
        // Zero means the page has not acknowledged displaying the release notes yet.
        if n.phase != "available" || n.expires_ms == 0 || now < n.expires_ms { return Tick::Wait; }
        if n.automatic { Tick::Install } else { Tick::Dismiss }
    }

    fn dismiss(&mut self, id: u64) {
        if let Some(n) = self.notice.as_ref().filter(|n| n.id == id && n.phase != "installing") {
            self.dismissed_version = Some(n.version.clone()); self.notice = None;
        }
    }

    fn download_current(&self, id: u64) -> bool {
        self.notice.as_ref().is_some_and(|n| n.id == id && n.phase == "downloading")
    }

    fn offer_instead(&mut self, id: u64, now: u64) {
        self.installing = false;
        if let Some(n) = self.notice.as_mut().filter(|n| n.id == id) {
            n.automatic = false; n.phase = "available".into();
            n.expires_ms = now + OFFER_MS;
            n.message = "Update when you're ready. Your work can continue.".into();
        }
    }
}

fn with<T>(app: &AppHandle, f: impl FnOnce(&mut Inner) -> T) -> T {
    let state = app.state::<UpdateState>();
    let mut inner = state.0.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut inner)
}

fn activity(app: &AppHandle) -> (bool, bool) {
    crate::app::with(app, |i| (i.settings.auto_update,
        !i.pending.is_empty() || !i.missed.is_empty()
        || i.sessions.values().any(|s| matches!(s.state.as_str(), "working" | "waiting"))))
}

fn automatic_startup(startup: bool, enabled: bool, busy: bool) -> bool {
    startup && enabled && !busy
}

fn emit(app: &AppHandle) {
    let notice = with(app, |i| i.notice.clone());
    let _ = app.emit("update_notice", &notice);
    if notice.is_none() {
        if let Some(window) = app.get_webview_window("update") { let _ = window.hide(); }
    }
}

fn show(app: &AppHandle) -> Result<(), String> {
    let window = app.get_webview_window("update").ok_or("Update notification is unavailable")?;
    let monitors = app.available_monitors().map_err(|e| e.to_string())?;
    let selected = crate::app::with(app, |i| i.settings.monitor);
    if let Some(monitor) = monitors.get(selected).or(monitors.first()) {
        let position = monitor.position();
        let size = monitor.size();
        let window_size = window.outer_size().map_err(|e| e.to_string())?;
        let margin = (16.0 * monitor.scale_factor()) as i32;
        let x = position.x + (size.width as i32 - window_size.width as i32 - margin).max(0);
        let _ = window.set_position(PhysicalPosition::new(x, position.y + margin));
    }
    window.show().map_err(|e| e.to_string())
}

pub fn start(app: AppHandle) {
    if cfg!(debug_assertions) { return; }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(STARTUP_DELAY_SECS)).await;
        let mut startup = true;
        loop {
            if let Err(error) = check(&app, startup, false).await { eprintln!("[nubi] update check: {error}"); }
            startup = false;
            tokio::time::sleep(Duration::from_secs(CHECK_INTERVAL_SECS)).await;
        }
    });
}

async fn check(app: &AppHandle, startup: bool, explicit: bool) -> Result<String, String> {
    if !with(app, |i| {
        if i.checking || i.installing { return false; }
        i.checking = true; true
    }) { return Ok("An update check or download is already in progress".into()); }
    let result = async {
        app.updater_builder().timeout(Duration::from_secs(30)).build()
            .map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())
    }.await;
    with(app, |i| i.checking = false);
    let Some(update) = result? else { return Ok("You have the latest version".into()); };
    let (enabled, busy) = activity(app);
    let automatic = automatic_startup(startup, enabled, busy);
    let id = with(app, |i| {
        if i.installing { return None; }
        if !explicit && (i.dismissed_version.as_deref() == Some(&update.version)
            || i.notice.as_ref().is_some_and(|n| n.version == update.version)) { return None; }
        i.next_id += 1;
        let id = i.next_id;
        i.notice = Some(Notice {
            id, version: update.version.clone(), notes: update.body.clone().unwrap_or_default(),
            phase: "available".into(), automatic,
            expires_ms: 0,
            held: false, message: String::new(),
        });
        i.candidate = Some(update); Some(id)
    });
    if let Some(id) = id {
        emit(app);
        if let Err(error) = show(app) { dismiss(app, id); return Err(error); }
        watch_notice(app.clone(), id);
    }
    Ok("Update available — see the notification".into())
}

fn watch_notice(app: AppHandle, id: u64) {
    tauri::async_runtime::spawn(async move {
        let mut last = crate::state::now_ms();
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let now = crate::state::now_ms();
            let action = with(&app, |i| i.tick(id, now, now.saturating_sub(last)));
            last = now;
            match action {
                Tick::Stop => return,
                Tick::Dismiss => { dismiss(&app, id); return; }
                Tick::Install => { let _ = install(&app, id, true).await; }
                Tick::Wait => {}
            }
        }
    });
}

fn offer_instead(app: &AppHandle, id: u64) {
    with(app, |i| i.offer_instead(id, crate::state::now_ms()));
    emit(app);
}

async fn install(app: &AppHandle, id: u64, automatic: bool) -> Result<(), String> {
    if cfg!(debug_assertions) { return Err("Updates only work in release builds".into()); }
    let update = with(app, |i| {
        if i.installing { return Err("An update is already in progress"); }
        let n = i.notice.as_mut().filter(|n| n.id == id).ok_or("Update offer has expired")?;
        if !matches!(n.phase.as_str(), "available" | "error") { return Err("Update is already in progress"); }
        let update = i.candidate.clone().ok_or("Check for updates again")?;
        i.installing = true; n.phase = "downloading".into(); n.expires_ms = 0;
        n.message = "Downloading update… You can cancel before installation.".into();
        Ok(update)
    }).map_err(str::to_string)?;
    let (enabled, busy) = activity(app);
    if automatic && (!enabled || busy) { offer_instead(app, id); return Ok(()); }
    emit(app);
    let downloaded = update.download(|_, _| {}, || {}).await.map_err(|e| e.to_string());
    // Dismissal invalidates the notice even if its network download is still finishing.
    if !with(app, |i| i.download_current(id)) {
        with(app, |i| i.installing = false); return Ok(());
    }
    let result = match downloaded {
        Ok(bytes) => {
            let (enabled, busy) = activity(app);
            if automatic && (!enabled || busy) { offer_instead(app, id); return Ok(()); }
            let valid = with(app, |i| {
                let Some(n) = i.notice.as_mut().filter(|n| n.id == id) else { return false; };
                n.phase = "installing".into(); n.message = "Installing update and restarting Nubi…".into(); true
            });
            if !valid { with(app, |i| i.installing = false); return Ok(()); }
            emit(app);
            update.install(bytes).map_err(|e| e.to_string())
        }
        Err(error) => Err(error),
    };
    if let Err(error) = result {
        with(app, |i| {
            i.installing = false;
            if let Some(n) = i.notice.as_mut().filter(|n| n.id == id) {
                n.phase = "error".into(); n.automatic = false; n.expires_ms = 0;
                n.message = format!("Update failed: {error}");
            }
        });
        emit(app); return Err(error);
    }
    app.restart();
}

fn dismiss(app: &AppHandle, id: u64) {
    with(app, |i| i.dismiss(id));
    emit(app);
}

#[tauri::command]
pub fn get_update_notice(app: AppHandle) -> Option<Notice> { with(&app, |i| i.notice.clone()) }

#[tauri::command]
pub fn update_notice_ready(app: AppHandle, id: u64) {
    with(&app, |i| {
        if let Some(n) = i.notice.as_mut().filter(|n| n.id == id && n.phase == "available" && n.expires_ms == 0) {
            n.expires_ms = crate::state::now_ms() + if n.automatic { STARTUP_INSTALL_MS } else { OFFER_MS };
        }
    });
    emit(&app);
}

#[tauri::command]
pub fn dismiss_update(app: AppHandle, id: u64) { dismiss(&app, id); }

#[tauri::command]
pub fn hold_update(app: AppHandle, id: u64, on: bool) {
    with(&app, |i| { if let Some(n) = i.notice.as_mut().filter(|n| n.id == id) { n.held = on; } });
    emit(&app);
}

#[tauri::command]
pub async fn install_update(app: AppHandle, id: u64) -> Result<(), String> { install(&app, id, false).await }

#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<String, String> {
    if cfg!(debug_assertions) { return Ok("Updates only work in release builds".into()); }
    check(&app, false, true).await
}

#[tauri::command]
pub fn app_version() -> String { env!("CARGO_PKG_VERSION").to_string() }

#[cfg(test)]
mod tests {
    use super::*;

    fn state(automatic: bool, phase: &str, expires_ms: u64) -> Inner {
        Inner { notice: Some(Notice {
            id: 1, version: "0.2.0".into(), notes: "Release notes".into(), phase: phase.into(),
            automatic, expires_ms, held: false, message: String::new(),
        }), ..Default::default() }
    }

    #[test]
    fn startup_policy_protects_work_and_manual_mode() {
        assert!(automatic_startup(true, true, false));
        assert!(!automatic_startup(true, true, true));
        assert!(!automatic_startup(true, false, false));
        assert!(!automatic_startup(false, true, false));
        assert!(!automatic_startup(false, false, true));
    }

    #[test]
    fn timeout_dismisses_offers_and_only_auto_startup_installs() {
        let mut manual = state(false, "available", 1_000);
        assert_eq!(manual.tick(1, 999, 250), Tick::Wait);
        assert_eq!(manual.tick(1, 1_000, 1), Tick::Dismiss);
        manual.dismiss(1);
        assert!(manual.notice.is_none());
        assert_eq!(manual.dismissed_version.as_deref(), Some("0.2.0"));
        assert_eq!(state(true, "available", 1_000).tick(1, 1_000, 250), Tick::Install);
    }

    #[test]
    fn notes_must_be_displayed_and_reading_pauses_countdown() {
        assert_eq!(state(true, "available", 0).tick(1, 50_000, 250), Tick::Wait);
        let mut reading = state(true, "available", 1_000);
        reading.notice.as_mut().unwrap().held = true;
        assert_eq!(reading.tick(1, 1_000, 250), Tick::Wait);
        assert_eq!(reading.notice.as_ref().unwrap().expires_ms, 1_250);
    }

    #[test]
    fn cancelled_download_cannot_install_or_affect_a_new_offer() {
        let mut downloading = state(false, "downloading", 0);
        assert!(downloading.download_current(1));
        downloading.dismiss(1);
        assert!(!downloading.download_current(1));
        downloading.notice = state(false, "available", 1_000).notice;
        downloading.notice.as_mut().unwrap().id = 2;
        downloading.dismiss(1);
        assert!(downloading.notice.is_some());
        assert_eq!(downloading.tick(1, 2_000, 250), Tick::Stop);
    }

    #[test]
    fn download_errors_do_not_retry_automatically_and_install_cannot_be_cancelled() {
        assert_eq!(state(true, "error", 0).tick(1, 50_000, 250), Tick::Stop);
        let mut installing = state(false, "installing", 0);
        installing.dismiss(1);
        assert!(installing.notice.is_some());
        assert_eq!(installing.tick(1, 50_000, 250), Tick::Wait);
    }

    #[test]
    fn activity_or_manual_setting_change_converts_download_to_a_cancellable_offer() {
        let mut downloading = state(true, "downloading", 0);
        downloading.installing = true;
        downloading.offer_instead(1, 5_000);
        assert!(!downloading.installing);
        assert!(!downloading.download_current(1));
        assert!(!downloading.notice.as_ref().unwrap().automatic);
        assert_eq!(downloading.tick(1, 15_000, 250), Tick::Dismiss);
    }
}

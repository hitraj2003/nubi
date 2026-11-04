//! DeskTime's server-rendered dashboard, read through an isolated WebView2 profile.
//! A hidden window loads today / week / month in turn; an injected script reports the card numbers
//! through one validated command. The session comes from the owner's browser through the Nubi
//! extension; the WebView2 profile keeps it. Nubi never handles a password.
use crate::app::{self, App};
use crate::protocol::{desktime_domain, validate_browser_msg, BrowserCookie, BrowserMsg, BrowserReply};
use crate::settings::{self, Settings};
use chrono::{Datelike, Duration, Local, NaiveDate, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Mutex;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// Weekly early-leave reserve; month surplus beyond it becomes late-arrival allowance.
const EARLY_LEAVE_WEEKLY_MIN: f64 = 90.0;
const PAGE_TIMEOUT_SECS: u64 = 60;
const VIEWS: [&str; 3] = ["day", "week", "month"];

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Report {
    view: String,
    productive_min: Option<u32>,
    at_work_min: Option<u32>,
    range_start: String,
    range_end: String,
    holidays: Vec<String>,
    logged_out: bool,
}

#[derive(Default)]
struct Inner {
    /// Last good report per view with its fetch time (ms). Persisted so a restart shows it at once.
    reports: BTreeMap<String, (Report, i64)>,
    /// View being loaded and when the load started (ms).
    pending: Option<(String, i64)>,
    /// Progress of the load: 0 opening, 1 today, 2 week, 3 month.
    step: u8,
    logged_out: bool,
    error: Option<String>,
}
#[derive(Default)]
pub struct DeskTime(Mutex<Inner>);

#[derive(Serialize)]
pub struct Row {
    key: String,
    label: String,
    value: String,
    note: String,
    /// "pos" | "neg" | "".
    tone: String,
}
#[derive(Serialize)]
pub struct Section {
    view: String,
    title: String,
    /// Day: the big number. Week/month: unused.
    big: String,
    caption: String,
    over: bool,
    /// Day: done as a share of the target, 0..=100.
    percent: f64,
    /// Week/month: working days in range.
    note: String,
    rows: Vec<Row>,
    fetched_ms: i64,
}
#[derive(Serialize)]
pub struct Snapshot {
    show: bool,
    enabled: bool,
    logged_out: bool,
    loading: bool,
    step: u8,
    error: Option<String>,
    updated_ms: i64,
    sections: Vec<Section>,
    /// Folder to load as an unpacked extension (Settings shows it).
    extension_dir: String,
}

fn date(s: &str) -> Result<NaiveDate, String> {
    if s.len() != 10 { return Err("Dates must use YYYY-MM-DD".into()); }
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| "Invalid date".into())
}
fn clock(s: &str) -> Result<i64, String> {
    let bytes = s.as_bytes();
    if bytes.len() != 5 || bytes[2] != b':' || ![bytes[0], bytes[1], bytes[3], bytes[4]].iter().all(u8::is_ascii_digit) {
        return Err("Times must use HH:MM".into());
    }
    let h: i64 = s[..2].parse().map_err(|_| "Invalid time")?;
    let m: i64 = s[3..].parse().map_err(|_| "Invalid time")?;
    if h > 23 || m > 59 { return Err("Time is out of range".into()); }
    Ok(h * 60 + m)
}
pub fn validate_settings(s: &Settings) -> Result<(), String> {
    if clock(&s.daily_target)? == 0 { return Err("Daily target must be greater than zero".into()); }
    clock(&s.end_of_day)?;
    if !s.start_of_day.is_empty() { clock(&s.start_of_day)?; }
    if s.work_days.len() > 7 || s.work_days.iter().any(|d| *d > 6) || s.day_targets.len() > 7 {
        return Err("Invalid work days".into());
    }
    for (d, target) in &s.day_targets {
        if *d > 6 { return Err("Invalid target weekday".into()); }
        clock(target)?;
    }
    for list in [&s.holidays, &s.half_days] {
        if list.len() > 366 { return Err("Date lists are limited to 366 entries".into()); }
        for d in list { date(d)?; }
    }
    Ok(())
}

fn range(view: &str, today: NaiveDate) -> Result<(NaiveDate, NaiveDate), String> {
    match view {
        "day" => Ok((today, today)),
        "week" => {
            let start = today - Duration::days(today.weekday().num_days_from_monday() as i64);
            Ok((start, start + Duration::days(6)))
        }
        "month" => {
            let start = today.with_day(1).ok_or("Invalid date")?;
            let next = (start + Duration::days(32)).with_day(1).ok_or("Invalid date")?;
            Ok((start, next - Duration::days(1)))
        }
        _ => Err("Unknown DeskTime view".into()),
    }
}
fn validate_report(r: &Report, today: NaiveDate) -> Result<(), String> {
    if r.holidays.len() > 366 { return Err("Too many holidays".into()); }
    if r.logged_out {
        // A login-page report carries no numbers; only its shape is checked.
        if r.view.len() > 16 || r.productive_min.is_some() || r.at_work_min.is_some() || !r.holidays.is_empty() {
            return Err("Invalid logged-out report".into());
        }
        return Ok(());
    }
    let expected = range(&r.view, today)?;
    let start = date(&r.range_start)?;
    let end = date(&r.range_end)?;
    if (start, end) != expected { return Err("Unexpected DeskTime date range".into()); }
    let maximum = ((end - start).num_days() + 1) as u32 * 1440;
    if r.productive_min.is_some_and(|n| n > maximum) || r.at_work_min.is_some_and(|n| n > maximum)
        || matches!((r.productive_min, r.at_work_min), (Some(p), Some(a)) if p > a) {
        return Err("Invalid DeskTime duration".into());
    }
    for holiday in &r.holidays {
        let d = date(holiday)?;
        if d < start || d > end { return Err("Holiday outside range".into()); }
    }
    Ok(())
}

// ---------- target math ----------
fn target(s: &Settings, d: NaiveDate, holidays: &[String]) -> f64 {
    let key = d.to_string();
    let weekday = d.weekday().num_days_from_sunday() as u8;
    if !s.work_days.contains(&weekday) || s.holidays.contains(&key) || holidays.contains(&key) { return 0.0; }
    let minutes = s.day_targets.get(&weekday).and_then(|v| clock(v).ok())
        .unwrap_or_else(|| clock(&s.daily_target).unwrap_or(510)) as f64;
    minutes * if s.half_days.contains(&key) { 0.5 } else { 1.0 }
}
fn span(s: &Settings, start: NaiveDate, end: NaiveDate, holidays: &[String]) -> f64 {
    start.iter_days().take_while(|d| *d <= end).map(|d| target(s, d, holidays)).sum()
}
fn work_days(s: &Settings, start: NaiveDate, end: NaiveDate, holidays: &[String]) -> usize {
    start.iter_days().take_while(|d| *d <= end).filter(|d| target(s, *d, holidays) > 0.0).count()
}
fn duration(minutes: f64) -> String {
    let m = minutes.abs().round() as i64;
    if m < 60 { format!("{m}m") } else { format!("{}h {:02}m", m / 60, m % 60) }
}
fn signed(minutes: f64) -> String { format!("{}{}", if minutes.round() >= 0.0 { "+" } else { "-" }, duration(minutes)) }
fn tone(minutes: f64) -> &'static str { if minutes.round() > 0.0 { "pos" } else if minutes.round() < 0.0 { "neg" } else { "" } }
fn row(key: &str, label: &str, value: String, note: String, tone: &str) -> Row {
    Row { key: key.into(), label: label.into(), value, note, tone: tone.into() }
}

fn section(r: &Report, fetched_ms: i64, s: &Settings, now: chrono::DateTime<Local>) -> Section {
    let today = now.date_naive();
    let (Ok(start), Ok(end)) = (date(&r.range_start), date(&r.range_end)) else { unreachable!("validated report") };
    let productive = r.productive_min.unwrap_or(0) as f64;
    let total = span(s, start, end, &r.holidays);
    let remaining = total - productive;
    let mut out = Section { view: r.view.clone(), title: String::new(), big: String::new(), caption: String::new(), over: false,
        percent: 0.0, note: String::new(), rows: Vec::new(), fetched_ms };
    if r.view == "day" {
        let share = if total > 0.0 { productive / total * 100.0 } else { 100.0 };
        out.title = "Today".into();
        out.over = remaining <= 0.0 && total > 0.0;
        out.big = if remaining > 0.0 { duration(remaining) } else if total > 0.0 { signed(-remaining) } else { "0m".into() };
        out.caption = if total == 0.0 { "no target today".into() } else if out.over { "overtime today".into() } else { "productive time left today".into() };
        out.percent = share.clamp(0.0, 100.0);
        let finish = if remaining > 0.0 && start == today { (now + Duration::minutes(remaining.ceil() as i64)).format("%H:%M").to_string() } else { "done".into() };
        out.rows.push(row("done", "Done", duration(productive), format!("{}% of target", share.round()), ""));
        out.rows.push(row("finish", "Finish at", finish, String::new(), ""));
        out.rows.push(row("target", "Target", duration(total), if s.half_days.contains(&start.to_string()) && total > 0.0 { "Half day".into() } else { String::new() }, ""));
        if let Some(at_work) = r.at_work_min {
            let pct = if at_work > 0 { (productive / at_work as f64 * 100.0).round() } else { 100.0 };
            out.rows.push(row("nonprod", "Non-productive", duration(at_work as f64 - productive), format!("{pct}% productive"), ""));
        }
        return out;
    }
    let days = work_days(s, start, end, &r.holidays);
    let required = span(s, start, today.min(end), &r.holidays);
    let eod = clock(&s.end_of_day).unwrap_or(1110);
    let minute = now.hour() as i64 * 60 + now.minute() as i64;
    let available = if today >= start && today <= end && target(s, today, &r.holidays) > 0.0 { (eod - minute).max(0) as f64 } else { 0.0 };
    let balance = productive - required;
    out.title = if r.view == "week" { "This week" } else { "This month" }.into();
    out.note = format!("{days} day{}", if days == 1 { "" } else { "s" });
    out.rows.push(row("target", "Target", duration(total), String::new(), ""));
    out.rows.push(row("todate", "To date", duration(productive), format!("of {}", duration(required)), ""));
    out.rows.push(row("balance", "Balance", signed(balance), String::new(), tone(balance)));
    out.rows.push(row("eod", "By end of day", signed(balance + available), String::new(), tone(balance + available)));
    if r.view == "month" {
        let allowance = (balance + available - EARLY_LEAVE_WEEKLY_MIN).round().max(0.0);
        let note = match clock(&s.start_of_day) {
            Ok(start_clock) if allowance > 0.0 => {
                let arrival = start_clock + allowance as i64;
                format!("by {:02}:{:02}{}", arrival / 60 % 24, arrival % 60, if arrival >= 1440 { " +1d" } else { "" })
            }
            _ => String::new(),
        };
        out.rows.push(row("late", "Late arrival", if allowance > 0.0 { duration(allowance) } else { "None".into() }, note, ""));
    }
    out
}

// ---------- state ----------
fn cache_path() -> std::path::PathBuf { settings::data_dir().join("desktime-cache.json") }
fn save_cache(reports: &BTreeMap<String, (Report, i64)>) {
    let result = serde_json::to_string(reports).map_err(|e| e.to_string())
        .and_then(|json| std::fs::create_dir_all(settings::data_dir()).and_then(|_| std::fs::write(cache_path(), json)).map_err(|e| e.to_string()));
    if let Err(error) = result { eprintln!("[desktime] cache not saved: {error}"); }
}
fn load_cache() -> BTreeMap<String, (Report, i64)> {
    let Ok(text) = std::fs::read_to_string(cache_path()) else { return BTreeMap::new() };
    if text.len() > 256 * 1024 { return BTreeMap::new(); }
    let mut reports: BTreeMap<String, (Report, i64)> = serde_json::from_str(&text).unwrap_or_default();
    // A cache is only kept for views the report validator accepts (right shape, sane numbers).
    let today = Local::now().date_naive();
    reports.retain(|view, (r, _)| VIEWS.contains(&view.as_str()) && r.view == *view && !r.logged_out && r.productive_min.is_some() && validate_report(r, today).is_ok());
    reports
}

fn snapshot(app: &App) -> Snapshot {
    let settings = app::with(app, |i| i.settings.clone());
    let state = app.state::<DeskTime>();
    let inner = state.0.lock().unwrap_or_else(|e| e.into_inner());
    let now = Local::now();
    let sections: Vec<Section> = VIEWS.iter().filter_map(|view| inner.reports.get(*view))
        .filter(|(r, _)| validate_report(r, now.date_naive()).is_ok())
        .map(|(r, time)| section(r, *time, &settings, now)).collect();
    Snapshot { show: settings.desktime_tab, enabled: settings.desktime_enabled, logged_out: inner.logged_out, loading: inner.pending.is_some(), step: inner.step,
        error: inner.error.clone(), updated_ms: sections.iter().map(|s| s.fetched_ms).max().unwrap_or(0), sections,
        extension_dir: crate::native_host::extension_dir().display().to_string() }
}
#[tauri::command]
pub fn desktime_state(app: App) -> Snapshot { snapshot(&app) }
pub fn emit(app: &App) {
    // Send only to bundled windows; remote pages cannot subscribe to application events.
    let snap = snapshot(app);
    for label in ["island", "panel"] { let _ = app.emit_to(label, "desktime", &snap); }
}

// ---------- scraping ----------
fn allowed(url: &tauri::Url) -> bool {
    url.scheme() == "https" && url.host_str().is_some_and(|h| h == "desktime.com" || h.ends_with(".desktime.com"))
}
fn dashboard(view: &str) -> tauri::Url {
    let base = "https://desktime.com/app/my";
    let url = if view == "day" { base.to_string() } else { format!("{base}/{}/{view}", Local::now().date_naive()) };
    url.parse().expect("static DeskTime URL")
}
fn profile() -> std::path::PathBuf { settings::data_dir().join("desktime-profile") }
fn step_for(view: &str) -> u8 { match view { "week" => 2, "month" => 3, _ => 0 } }
fn set_error(app: &App, message: &str) {
    let state = app.state::<DeskTime>();
    let mut inner = state.0.lock().unwrap_or_else(|e| e.into_inner());
    inner.pending = None;
    inner.error = Some(message.into());
}
/// The hidden scraper window, created on `url` when missing. about:blank is allowed so cookies
/// can be set before the first DeskTime page loads.
fn scraper(app: &App, url: tauri::Url) -> Result<WebviewWindow, String> {
    if let Some(w) = app.get_webview_window("desktime") { return Ok(w); }
    WebviewWindowBuilder::new(app, "desktime", WebviewUrl::External(url))
        .title("DeskTime").visible(false).data_directory(profile())
        .initialization_script(include_str!("desktime-scraper.js"))
        .on_navigation(|url| allowed(url) || url.as_str() == "about:blank")
        .on_page_load(|w, payload| {
            // The first page is up: the script is now reading today's numbers.
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                let app = w.app_handle();
                let moved = {
                    let state = app.state::<DeskTime>();
                    let mut inner = state.0.lock().unwrap_or_else(|e| e.into_inner());
                    let day = inner.pending.as_ref().is_some_and(|p| p.0 == "day");
                    if day && inner.step == 0 { inner.step = 1; }
                    day
                };
                if moved { emit(app); }
            }
        })
        .build().map_err(|e| e.to_string())
}
/// Starts loading one view in the hidden window. Runs on the main thread.
fn load(app: &App, view: &str) -> Result<(), String> {
    let started = Local::now().timestamp_millis();
    {
        let state = app.state::<DeskTime>();
        let mut inner = state.0.lock().unwrap_or_else(|e| e.into_inner());
        inner.pending = Some((view.into(), started));
        inner.step = step_for(view);
    }
    let result = match app.get_webview_window("desktime") {
        Some(w) => w.navigate(dashboard(view)).map_err(|e| e.to_string()),
        None => scraper(app, dashboard(view)).map(|_| ()),
    };
    match &result {
        Err(error) => set_error(app, error),
        Ok(()) => {
            // Watchdog: a page that never reports must not leave the spinner running.
            let a = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(PAGE_TIMEOUT_SECS));
                let stuck = a.state::<DeskTime>().0.lock().unwrap_or_else(|e| e.into_inner()).pending.as_ref().is_some_and(|p| p.1 == started);
                if stuck { set_error(&a, "DeskTime did not respond"); emit(&a); }
            });
        }
    }
    result
}
/// Refreshes all views unless a load is already running.
pub fn refresh(app: &App) {
    if !app::with(app, |i| i.settings.desktime_enabled && i.settings.desktime_tab) { return; }
    {
        let state = app.state::<DeskTime>();
        let inner = state.0.lock().unwrap_or_else(|e| e.into_inner());
        if inner.pending.as_ref().is_some_and(|p| Local::now().timestamp_millis() - p.1 < (PAGE_TIMEOUT_SECS as i64 + 5) * 1000) { return; }
    }
    let a = app.clone();
    let _ = app.run_on_main_thread(move || { let _ = load(&a, "day"); emit(&a); });
}
pub fn start(app: App) {
    *app.state::<DeskTime>().0.lock().unwrap_or_else(|e| e.into_inner()) = Inner { reports: load_cache(), ..Inner::default() };
    std::thread::spawn(move || loop {
        refresh(&app);
        std::thread::sleep(std::time::Duration::from_secs(300));
    });
}
#[tauri::command]
pub fn desktime_refresh(app: App) { refresh(&app); }

fn save_enabled(app: &App, on: bool) -> Result<(), String> {
    let mut s = app::with(app, |i| i.settings.clone());
    s.desktime_enabled = on;
    settings::save(&s)?;
    app::with(app, |i| i.settings = s);
    Ok(())
}
// ---------- browser session (Nubi browser extension, see native_host.rs) ----------
// Login happens in the owner's own browser. The extension sends the DeskTime cookies; they go
// into the hidden window's WebView2 profile. Nubi never sees or stores a password.
fn blank() -> tauri::Url { "about:blank".parse().expect("static URL") }
/// Set-Cookie form, so the cookie crate parses the expiry (no direct `time` dependency).
fn to_cookie(c: &BrowserCookie) -> Result<tauri::webview::Cookie<'static>, String> {
    let mut text = format!("{}={}; Domain={}; Path={}", c.name, c.value, c.domain, c.path);
    if c.secure { text.push_str("; Secure"); }
    if c.http_only { text.push_str("; HttpOnly"); }
    if let Some(at) = c.expires.and_then(|e| chrono::DateTime::from_timestamp(e as i64, 0)) {
        text.push_str(&at.format("; Expires=%a, %d %b %Y %H:%M:%S GMT").to_string());
    }
    tauri::webview::Cookie::parse(text).map_err(|_| "Invalid cookie".to_string())
}
/// Mirrors the browser's DeskTime cookies into the scraper profile. Runs off the main thread.
fn apply_session(app: &App, cookies: &[BrowserCookie]) -> Result<(), String> {
    let window = scraper(app, blank())?;
    // Cookies the browser no longer has are dropped, so a browser logout also ends Nubi's session.
    for old in window.cookies().map_err(|e| e.to_string())? {
        if old.domain().is_some_and(desktime_domain) && !cookies.iter().any(|c| c.name == old.name()) {
            let _ = window.delete_cookie(old);
        }
    }
    for c in cookies { window.set_cookie(to_cookie(c)?).map_err(|e| e.to_string())?; }
    let was_enabled = app::with(app, |i| i.settings.desktime_enabled);
    if !was_enabled { save_enabled(app, true)?; }
    let resume = {
        let state = app.state::<DeskTime>();
        let mut inner = state.0.lock().unwrap_or_else(|e| e.into_inner());
        let resume = !was_enabled || inner.logged_out;
        inner.logged_out = false;
        inner.error = None;
        resume
    };
    app::emit_state(app);
    emit(app);
    // A running session refreshes on its own timer; a new or restored one reads now.
    if resume { refresh(app); }
    Ok(())
}
/// Clears the scraper profile and turns DeskTime off. Used by Log out and by the extension's Remove access.
fn revoke(app: &App) -> Result<(), String> {
    let window = scraper(app, blank())?;
    if let Ok(cookies) = window.cookies() {
        for c in cookies { let _ = window.delete_cookie(c); }
    }
    window.clear_all_browsing_data().map_err(|e| e.to_string())?;
    let _ = window.close();
    save_enabled(app, false)?;
    *app.state::<DeskTime>().0.lock().unwrap_or_else(|e| e.into_inner()) = Inner::default();
    let _ = std::fs::remove_file(cache_path());
    app::emit_state(app);
    emit(app);
    Ok(())
}
/// A browser message from the pipe. Blocking: call from a worker thread, never the main thread.
pub fn on_browser(app: &App, msg: BrowserMsg) -> BrowserReply {
    if let Err(e) = validate_browser_msg(&msg) { return BrowserReply::result(Err(e)); }
    match msg {
        BrowserMsg::DesktimeRevoke => BrowserReply::result(revoke(app)),
        // A background re-send after Log out in Nubi must not reconnect: only Allow access does.
        BrowserMsg::DesktimeSession { sync: true, .. } if !app::with(app, |i| i.settings.desktime_enabled) => BrowserReply {
            ok: false, error: Some("Access was removed in Nubi. Click Allow access to connect again.".into()), revoked: true,
        },
        BrowserMsg::DesktimeSession { cookies, .. } => BrowserReply::result(apply_session(app, &cookies)),
    }
}
#[tauri::command]
pub async fn desktime_logout(app: App) -> Result<(), String> { revoke(&app) }
/// "site": DeskTime's login page in the default browser. "extension": the unpacked extension folder.
#[tauri::command]
pub fn desktime_open(target: String) -> Result<(), String> {
    let arg = match target.as_str() {
        "site" => "https://desktime.com/login".to_string(),
        "extension" => {
            crate::native_host::register();
            crate::native_host::extension_dir().display().to_string()
        }
        _ => return Err("Unknown target".into()),
    };
    std::process::Command::new("explorer").arg(arg).spawn().map(|_| ()).map_err(|e| e.to_string())
}
/// The only command a remote DeskTime page may call (see capabilities/desktime.json).
#[tauri::command]
pub fn desktime_report(app: App, webview: WebviewWindow, report: Report) -> Result<(), String> {
    if webview.label() != "desktime" || !allowed(&webview.url().map_err(|e| e.to_string())?) {
        return Err("Unexpected report source".into());
    }
    validate_report(&report, Local::now().date_naive())?;
    if !app::with(&app, |i| i.settings.desktime_enabled) { return Err("DeskTime is disabled".into()); }
    let state = app.state::<DeskTime>();
    let mut next = None;
    {
        let mut inner = state.0.lock().unwrap_or_else(|e| e.into_inner());
        if !report.logged_out && inner.pending.as_ref().map(|p| p.0.as_str()) != Some(report.view.as_str()) { return Err("Unexpected report view".into()); }
        inner.pending = None;
        inner.logged_out = report.logged_out;
        if report.logged_out {
            inner.reports.clear();
            inner.error = None;
            let _ = std::fs::remove_file(cache_path());
        } else if report.productive_min.is_some() {
            next = match report.view.as_str() { "day" => Some("week"), "week" => Some("month"), _ => None };
            inner.error = None;
            let view = report.view.clone();
            inner.reports.insert(view, (report, Local::now().timestamp_millis()));
            save_cache(&inner.reports);
        } else {
            // Cards missing: keep the last good data and say so; the cache stays as it is.
            inner.error = Some("Could not read DeskTime cards".into());
        }
    }
    emit(&app);
    if let Some(view) = next {
        let a = app.clone();
        let _ = app.run_on_main_thread(move || { let _ = load(&a, view); emit(&a); });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn day(s: &str) -> NaiveDate { date(s).unwrap() }
    fn report() -> Report { Report { view: "week".into(), productive_min: Some(500), at_work_min: Some(600), range_start: "2026-11-02".into(), range_end: "2026-11-08".into(), holidays: vec![], logged_out: false } }
    fn at(s: &str) -> chrono::DateTime<Local> {
        use chrono::TimeZone;
        Local.from_local_datetime(&chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()).unwrap()
    }
    #[test]
    fn defaults_and_weighted_targets() {
        let mut s = Settings::default();
        assert_eq!(span(&s, day("2026-11-02"), day("2026-11-08"), &[]), 2550.0);
        s.holidays.push("2026-11-03".into());
        s.half_days.push("2026-11-04".into());
        s.day_targets.insert(5, "06:00".into());
        assert_eq!(span(&s, day("2026-11-02"), day("2026-11-08"), &[]), 1635.0);
        assert_eq!(target(&s, day("2026-11-07"), &[]), 0.0);
        assert_eq!(target(&s, day("2026-11-04"), &["2026-11-04".into()]), 0.0);
        assert_eq!(work_days(&s, day("2026-11-02"), day("2026-11-08"), &[]), 4);
    }
    #[test]
    fn rejects_untrusted_payloads() {
        let mut r = report();
        assert!(validate_report(&r, day("2026-11-04")).is_ok());
        r.productive_min = Some(10081);
        assert!(validate_report(&r, day("2026-11-04")).is_err());
        r = report(); r.range_end = "2026-11-09".into();
        assert!(validate_report(&r, day("2026-11-04")).is_err());
        r = report(); r.holidays = vec!["2026-11-01".into()];
        assert!(validate_report(&r, day("2026-11-04")).is_err());
        r = report(); r.holidays = vec!["2026-11-04".into(); 367];
        assert!(validate_report(&r, day("2026-11-04")).is_err());
        r = report(); r.view = "unknown".into();
        assert!(validate_report(&r, day("2026-11-04")).is_err());
        r = report(); r.productive_min = Some(601);
        assert!(validate_report(&r, day("2026-11-04")).is_err());
        r = report(); r.logged_out = true; r.productive_min = None; r.at_work_min = None;
        assert!(validate_report(&r, day("2026-11-04")).is_ok());
        r.productive_min = Some(1);
        assert!(validate_report(&r, day("2026-11-04")).is_err());
        assert!(serde_json::from_str::<Report>(r#"{"view":"day","extra":1}"#).is_err());
    }
    #[test]
    fn calendar_boundaries_and_settings() {
        assert_eq!(range("week", day("2027-01-01")).unwrap(), (day("2026-12-28"), day("2027-01-03")));
        assert_eq!(range("month", day("2028-02-15")).unwrap().1, day("2028-02-29"));
        let mut s = Settings::default();
        assert!(validate_settings(&s).is_ok());
        s.end_of_day = "24:00".into(); assert!(validate_settings(&s).is_err());
        assert!(clock("ab:00").is_err());
        assert!(date("2026-02-30").is_err());
    }
    #[test]
    fn day_section_remaining_and_overtime() {
        let s = Settings::default();
        let mut r = Report { view: "day".into(), productive_min: Some(376), at_work_min: Some(414), range_start: "2026-11-04".into(), range_end: "2026-11-04".into(), holidays: vec![], logged_out: false };
        let sec = section(&r, 1, &s, at("2026-11-04 11:00"));
        assert_eq!(sec.big, "2h 14m");
        assert!(!sec.over);
        assert_eq!(sec.rows[0].note, "74% of target");
        assert_eq!(sec.rows[1].value, "13:14");
        assert_eq!(sec.rows[3].value, "38m");
        r.productive_min = Some(532);
        let sec = section(&r, 1, &s, at("2026-11-04 17:00"));
        assert_eq!(sec.big, "+22m");
        assert!(sec.over);
        assert_eq!(sec.percent, 100.0);
        assert_eq!(sec.rows[1].value, "done");
    }
    #[test]
    fn period_balance_projection_and_late_arrival() {
        let mut s = Settings::default();
        s.start_of_day = "09:00".into();
        // Wed 2026-11-04 17:30: required Mon-Wed = 3 x 510; 60 min left until 18:30.
        let r = Report { view: "week".into(), productive_min: Some(1530), at_work_min: None, range_start: "2026-11-02".into(), range_end: "2026-11-08".into(), holidays: vec![], logged_out: false };
        let sec = section(&r, 1, &s, at("2026-11-04 17:30"));
        assert_eq!(sec.note, "5 days");
        assert_eq!(sec.rows[1].note, "of 25h 30m");
        assert_eq!(sec.rows[2].value, "+0m");
        assert_eq!(sec.rows[3].value, "+1h 00m");
        let m = Report { view: "month".into(), productive_min: Some(1530 + 400), at_work_min: None, range_start: "2026-11-01".into(), range_end: "2026-11-30".into(), holidays: vec![], logged_out: false };
        let sec = section(&m, 1, &s, at("2026-11-04 17:30"));
        // Required Nov 2-4 = 1530; balance 400 + 60 available - 90 reserve = 370 minutes late allowance.
        assert_eq!(sec.rows[4].value, "6h 10m");
        assert_eq!(sec.rows[4].note, "by 15:10");
    }
}

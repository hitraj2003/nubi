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
/// Row keys the owner can pick per card.
pub const DAY_ROWS: [&str; 5] = ["done", "finish", "target", "late", "nonprod"];
pub const PERIOD_ROWS: [&str; 5] = ["target", "todate", "balance", "eod", "late"];

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
    /// Date of the last "target reached" notification, so it fires once a day.
    alerted: Option<NaiveDate>,
    /// When the browser extension last handed over the session (ms), since Nubi started.
    synced_ms: i64,
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
    /// Remaining time, or overtime with a leading +.
    big: String,
    caption: String,
    over: bool,
    /// Done as a share of the target, 0..=100.
    percent: f64,
    /// Week/month: share of the target due by end of today, 0..=100.
    pace: Option<f64>,
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
    /// Oldest fetch time among the shown sections (ms).
    updated_ms: i64,
    sections: Vec<Section>,
    /// Sections the owner chose to show.
    views: Vec<String>,
    /// Newest successful read of any view (ms), for Settings.
    read_ms: i64,
    /// Last session handover from the browser extension (ms); 0 when none since start.
    synced_ms: i64,
    /// Views read on each refresh, in order (the loading steps).
    fetch: Vec<String>,
    /// Collapsed-notch text; empty when the glance is off or there is no data.
    glance: String,
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
    if s.dt_views.is_empty() || s.dt_views.len() > 3 || s.dt_views.iter().any(|v| !VIEWS.contains(&v.as_str()))
        || s.dt_views.iter().enumerate().any(|(i, v)| s.dt_views[..i].contains(v)) {
        return Err("Pick at least one DeskTime section".into());
    }
    if !["left", "done"].contains(&s.dt_glance_kind.as_str()) { return Err("Invalid glance".into()); }
    for (rows, allowed) in [(&s.dt_rows_day, &DAY_ROWS[..]), (&s.dt_rows_week, &PERIOD_ROWS[..]), (&s.dt_rows_month, &PERIOD_ROWS[..])] {
        if rows.len() > allowed.len() || rows.iter().any(|r| !allowed.contains(&r.as_str())) { return Err("Invalid DeskTime rows".into()); }
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

// ---------- target math (same rules as the DeskTime Remaining v5 extension) ----------
/// Day weight: 0 off day or holiday, 0.5 half day, 1 otherwise.
fn weight(s: &Settings, d: NaiveDate, holidays: &[String]) -> f64 {
    let key = d.to_string();
    let weekday = d.weekday().num_days_from_sunday() as u8;
    if !s.work_days.contains(&weekday) || s.holidays.contains(&key) || holidays.contains(&key) { return 0.0; }
    if s.half_days.contains(&key) { 0.5 } else { 1.0 }
}
/// Full-day target for the weekday (per-weekday override, else the daily target).
fn day_target(s: &Settings, d: NaiveDate) -> f64 {
    let weekday = d.weekday().num_days_from_sunday() as u8;
    s.day_targets.get(&weekday).and_then(|v| clock(v).ok()).unwrap_or_else(|| clock(&s.daily_target).unwrap_or(510)) as f64
}
fn target(s: &Settings, d: NaiveDate, holidays: &[String]) -> f64 { weight(s, d, holidays) * day_target(s, d) }
/// (working days, target minutes) from `start` to `end`, inclusive.
fn span(s: &Settings, start: NaiveDate, end: NaiveDate, holidays: &[String]) -> (f64, f64) {
    start.iter_days().take_while(|d| *d <= end).fold((0.0, 0.0), |(n, m), d| (n + weight(s, d, holidays), m + target(s, d, holidays)))
}
fn duration(minutes: f64) -> String {
    let m = minutes.abs().round() as i64;
    if m < 60 { format!("{m}m") } else { format!("{}h {:02}m", m / 60, m % 60) }
}
fn signed(minutes: f64) -> String { format!("{}{}", if minutes.round() >= 0.0 { "+" } else { "-" }, duration(minutes)) }
fn days_text(n: f64) -> String {
    let v = if n.fract() == 0.0 { format!("{n:.0}") } else { format!("{n}") };
    format!("{v} day{}", if n == 1.0 { "" } else { "s" })
}
fn hhmm(minutes: i64) -> String {
    let d = minutes.div_euclid(1440);
    let m = minutes.rem_euclid(1440);
    format!("{:02}:{:02}{}", m / 60, m % 60, if d > 0 { format!(" +{d}d") } else { String::new() })
}
fn row(key: &str, label: &str, value: String, note: String, tone: &str) -> Row {
    Row { key: key.into(), label: label.into(), value, note, tone: tone.into() }
}

/// Views to read: the shown ones, plus today for the glance and alert, plus the month for the
/// late-arrival allowance that Today and This week also show.
fn fetch_views(s: &Settings) -> Vec<&'static str> {
    let shown = |v: &str| s.dt_views.iter().any(|x| x == v);
    let day = shown("day") || s.dt_glance || s.dt_alert_target;
    VIEWS.into_iter().filter(|v| match *v { "day" => day, "week" => shown("week"), _ => !s.dt_views.is_empty() }).collect()
}
/// Minutes left until end of day when today is a working day, else 0.
fn available(s: &Settings, now: chrono::DateTime<Local>, holidays: &[String]) -> f64 {
    if weight(s, now.date_naive(), holidays) == 0.0 { return 0.0; }
    let eod = clock(&s.end_of_day).unwrap_or(1110);
    (eod - (now.hour() as i64 * 60 + now.minute() as i64)).max(0) as f64
}
/// Late-arrival row from the month report: month surplus projected to end of today, minus the
/// weekly early-leave reserve. None without a month report covering today.
fn late_row(month: Option<&Report>, s: &Settings, now: chrono::DateTime<Local>) -> Option<Row> {
    let m = month?;
    let today = now.date_naive();
    let (Ok(start), Ok(end)) = (date(&m.range_start), date(&m.range_end)) else { return None };
    if today < start || today > end { return None; }
    let required = span(s, start, today, &m.holidays).1;
    let surplus = (m.productive_min? as f64 + available(s, now, &m.holidays) - required).round();
    let allowance = (surplus - EARLY_LEAVE_WEEKLY_MIN).max(0.0);
    let mut notes = Vec::new();
    if allowance <= 0.0 {
        notes.push(format!("{} {}", duration(surplus), if surplus >= 0.0 { "ahead" } else { "behind" }));
        notes.push(format!("need over {}", duration(EARLY_LEAVE_WEEKLY_MIN)));
    } else if let Ok(arrive) = clock(&s.start_of_day) {
        notes.push(format!("arrive by {}", hhmm(arrive + allowance as i64)));
    }
    let value = if allowance > 0.0 { duration(allowance) } else { "None".into() };
    Some(row("late", "Late arrival", value, notes.join(" · "), if allowance > 0.0 { "pos" } else { "" }))
}
fn section(r: &Report, fetched_ms: i64, s: &Settings, now: chrono::DateTime<Local>, month: Option<&Report>) -> Section {
    let today = now.date_naive();
    let (Ok(start), Ok(end)) = (date(&r.range_start), date(&r.range_end)) else { unreachable!("validated report") };
    let productive = r.productive_min.unwrap_or(0) as f64;
    let (days, total) = span(s, start, end, &r.holidays);
    let remaining = total - productive;
    let share = if total > 0.0 { productive / total * 100.0 } else { 100.0 };
    let over = remaining <= 0.0 && total > 0.0;
    let big = if remaining > 0.0 { duration(remaining) } else if total > 0.0 { signed(-remaining) } else { "0m".into() };
    let mut out = Section { view: r.view.clone(), title: String::new(), big, caption: String::new(), over,
        percent: share.clamp(0.0, 100.0), pace: None, note: String::new(), rows: Vec::new(), fetched_ms };
    if r.view == "day" {
        out.title = "Today".into();
        out.caption = if total == 0.0 { "no target today".into() } else if over { "overtime".into() } else { "left".into() };
        out.rows.push(row("done", "Done", duration(productive), format!("{}% of target", share.round()), ""));
        if remaining > 0.0 && start == today {
            out.rows.push(row("finish", "Finish at", (now + Duration::minutes(remaining.ceil() as i64)).format("%H:%M").to_string(), String::new(), ""));
        }
        let half = s.half_days.contains(&start.to_string()) && total > 0.0;
        out.rows.push(row("target", "Target", duration(total), if half { "half day".into() } else { String::new() }, ""));
        if start == today { out.rows.extend(late_row(month, s, now)); }
        if let Some(at_work) = r.at_work_min.map(f64::from).filter(|a| *a > productive) {
            out.rows.push(row("nonprod", "Non-productive", duration(at_work - productive), format!("{}% productive", (productive / at_work * 100.0).round()), ""));
        }
        return out;
    }
    out.title = if r.view == "week" { "This week" } else { "This month" }.into();
    out.caption = if over { "overtime".into() } else { "left".into() };
    out.note = days_text(days);
    let in_range = today >= start && today <= end;
    let (done_days, required) = if in_range { span(s, start, today, &r.holidays) } else if today > end { (days, total) } else { (0.0, 0.0) };
    if in_range && total > 0.0 { out.pace = Some((required / total * 100.0).clamp(0.0, 100.0)); }
    let daily = clock(&s.daily_target).unwrap_or(510) as f64;
    let per_day = if (days * daily - total).abs() < 0.5 { format!(" × {}", duration(daily)) } else { String::new() };
    out.rows.push(row("target", "Target", duration(total), format!("{}{per_day}", days_text(days)), ""));
    let eod_label = hhmm(clock(&s.end_of_day).unwrap_or(1110));
    let avail = if in_range { available(s, now, &r.holidays) } else { 0.0 };
    if in_range {
        let diff = productive - required;
        let mut notes = vec![if diff >= 0.0 { "ahead".to_string() } else { "behind".to_string() }];
        if avail > 0.0 {
            let proj = (diff + avail).round();
            notes.push(format!("{} {} at {eod_label}", signed(proj), if proj >= 0.0 { "ahead" } else { "behind" }));
        }
        out.rows.push(row("todate", "To date", duration(productive), format!("of {} · {}", duration(required), days_text(done_days)), ""));
        out.rows.push(row("balance", "Balance", signed(diff), notes.join(" · "), if diff >= 0.0 { "pos" } else { "neg" }));
    }
    if avail > 0.0 {
        let left = remaining - avail;
        let rest_days = (span(s, today, end, &r.holidays).0 - weight(s, today, &r.holidays)).max(0.0);
        let label = format!("At {eod_label}");
        out.rows.push(if left <= 0.0 { row("eod", &label, signed(-left), "overtime".into(), "pos") }
            else if rest_days > 0.0 { row("eod", &label, format!("{} left", duration(left)), format!("{}/day for {}", duration(left / rest_days), days_text(rest_days)), "") }
            else { row("eod", &label, format!("{} left", duration(left)), "no working days left".into(), "neg") });
    }
    if in_range { out.rows.extend(late_row(if r.view == "month" { Some(r) } else { month }, s, now)); }
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
    let valid = |view: &str| inner.reports.get(view).filter(|(r, _)| validate_report(r, now.date_naive()).is_ok());
    let month = valid("month").map(|(r, _)| r);
    let sections: Vec<Section> = VIEWS.iter().filter(|v| settings.dt_views.iter().any(|x| x == *v)).filter_map(|v| valid(v))
        .map(|(r, time)| section(r, *time, &settings, now, month)).collect();
    let glance = if settings.dt_glance && settings.desktime_enabled && settings.desktime_tab && !inner.logged_out {
        valid("day").map(|(r, _)| glance(&section(r, 0, &settings, now, month), &settings.dt_glance_kind)).unwrap_or_default()
    } else { String::new() };
    let fetch = fetch_views(&settings).into_iter().map(String::from).collect();
    Snapshot { fetch, glance, views: settings.dt_views.clone(),
        read_ms: inner.reports.values().map(|(_, t)| *t).max().unwrap_or(0), synced_ms: inner.synced_ms, show: settings.desktime_tab, enabled: settings.desktime_enabled, logged_out: inner.logged_out, loading: inner.pending.is_some(), step: inner.step,
        error: inner.error.clone(), updated_ms: sections.iter().map(|s| s.fetched_ms).min().unwrap_or(0), sections,
        extension_dir: crate::native_host::extension_dir().display().to_string() }
}
fn glance(day: &Section, kind: &str) -> String {
    let cell = |key: &str| day.rows.iter().find(|r| r.key == key);
    match kind {
        "done" => match (cell("done"), cell("target")) { (Some(d), Some(t)) => format!("{} / {}", d.value, t.value), _ => String::new() },
        _ => if day.over { day.big.clone() } else if day.big == "0m" { String::new() } else { format!("{} left", day.big) },
    }
}
/// Productive minutes still missing today, or None without a target today.
fn day_remaining(r: &Report, s: &Settings) -> Option<f64> {
    let d = date(&r.range_start).ok()?;
    let total = target(s, d, &r.holidays);
    (total > 0.0).then(|| total - r.productive_min.unwrap_or(0) as f64)
}
fn notify_target(app: &App) {
    use tauri_plugin_notification::NotificationExt;
    let _ = app.notification().builder().title("DeskTime · Target reached")
        .body("You reached today's productive target.").show();
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
    eprintln!("[desktime] {message}");
    inner.error = Some(message.into());
}
/// The hidden scraper window, created on `url` when missing. about:blank is allowed so cookies
/// can be set before the first DeskTime page loads.
fn scraper(app: &App, url: tauri::Url) -> Result<WebviewWindow, String> {
    if let Some(w) = app.get_webview_window("desktime") { return Ok(w); }
    WebviewWindowBuilder::new(app, "desktime", WebviewUrl::External(url))
        .title("DeskTime").visible(false).inner_size(1440.0, 900.0).data_directory(profile())
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
        .and_then(|w| { block_extras(&w)?; Ok(w) })
}
/// Request kinds the scraper needs. Everything else (images, fonts, media, stylesheets, any
/// non-DeskTime host such as analytics or reCAPTCHA) is refused before it leaves the machine.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind { Document, Script, Xhr, Fetch, Other }
fn keep_resource(uri: &str, kind: Kind) -> bool {
    kind != Kind::Other && uri.parse::<tauri::Url>().is_ok_and(|u| allowed(&u))
}
/// Installs the WebView2 request filter on the hidden window only.
fn block_extras(window: &WebviewWindow) -> Result<(), String> {
    window.with_webview(|wv| unsafe {
        use webview2_com::Microsoft::Web::WebView2::Win32::*;
        use webview2_com::{take_pwstr, WebResourceRequestedEventHandler};
        let Ok(core) = wv.controller().CoreWebView2() else { return };
        let Ok(env) = windows_core::Interface::cast::<ICoreWebView2_2>(&core).and_then(|c| c.Environment()) else {
            eprintln!("[desktime] request filter unavailable");
            return;
        };
        let handler = WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let mut ctx = COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL;
            args.ResourceContext(&mut ctx)?;
            let kind = match ctx {
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT => Kind::Document,
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_SCRIPT => Kind::Script,
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_XML_HTTP_REQUEST => Kind::Xhr,
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FETCH => Kind::Fetch,
                _ => Kind::Other,
            };
            let mut uri = windows_core::PWSTR::null();
            args.Request()?.Uri(&mut uri)?;
            if !keep_resource(&take_pwstr(uri), kind) {
                let blocked = env.CreateWebResourceResponse(
                    None::<&windows::Win32::System::Com::IStream>, 403,
                    windows_core::w!("Blocked"), windows_core::w!(""))?;
                args.SetResponse(&blocked)?;
            }
            Ok(())
        }));
        let mut token = 0i64;
        let added = core.AddWebResourceRequestedFilter(windows_core::w!("*"), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)
            .and_then(|_| core.add_WebResourceRequested(&handler, &mut token));
        if let Err(e) = added { eprintln!("[desktime] request filter not installed: {e}"); }
    }).map_err(|e| e.to_string())
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
    let first = app::with(app, |i| (i.settings.desktime_enabled && i.settings.desktime_tab).then(|| fetch_views(&i.settings).first().copied()));
    let Some(Some(first)) = first else { return };
    {
        let state = app.state::<DeskTime>();
        let inner = state.0.lock().unwrap_or_else(|e| e.into_inner());
        if inner.pending.as_ref().is_some_and(|p| Local::now().timestamp_millis() - p.1 < (PAGE_TIMEOUT_SECS as i64 + 5) * 1000) { return; }
    }
    let a = app.clone();
    let _ = app.run_on_main_thread(move || { let _ = load(&a, first); emit(&a); });
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
/// Deletes `stale` (name, domain, path) and writes `cookies` through the WebView2 cookie manager.
/// Written directly, not through tauri's cookie type: that type drops the leading dot of a domain
/// cookie, which would turn every cookie host-only. The browser's domain string is kept as is.
fn write_cookies(window: &WebviewWindow, stale: Vec<(String, String, String)>, cookies: Vec<BrowserCookie>) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    window.with_webview(move |wv| {
        let result = (|| -> windows_core::Result<()> { unsafe {
            use webview2_com::Microsoft::Web::WebView2::Win32::*;
            use windows_core::{Interface, HSTRING};
            let manager = wv.controller().CoreWebView2()?.cast::<ICoreWebView2_2>()?.CookieManager()?;
            for (name, domain, path) in &stale {
                for d in [domain.clone(), format!(".{domain}")] {
                    manager.DeleteCookiesWithDomainAndPath(&HSTRING::from(name), &HSTRING::from(d), &HSTRING::from(path))?;
                }
            }
            for c in &cookies {
                let cookie = manager.CreateCookie(&HSTRING::from(&c.name), &HSTRING::from(&c.value), &HSTRING::from(&c.domain), &HSTRING::from(&c.path))?;
                cookie.SetIsSecure(c.secure.into())?;
                cookie.SetIsHttpOnly(c.http_only.into())?;
                match c.same_site.as_deref() {
                    Some("strict") => cookie.SetSameSite(COREWEBVIEW2_COOKIE_SAME_SITE_KIND_STRICT)?,
                    Some("lax") => cookie.SetSameSite(COREWEBVIEW2_COOKIE_SAME_SITE_KIND_LAX)?,
                    Some("no_restriction") => cookie.SetSameSite(COREWEBVIEW2_COOKIE_SAME_SITE_KIND_NONE)?,
                    _ => {}
                }
                if let Some(at) = c.expires { cookie.SetExpires(at)?; }
                manager.AddOrUpdateCookie(&cookie)?;
            }
            Ok(())
        } })();
        let _ = tx.send(result.map_err(|e| e.to_string()));
    }).map_err(|e| e.to_string())?;
    rx.recv_timeout(std::time::Duration::from_secs(10)).map_err(|_| "Cookie update timed out".to_string())?
}
/// Mirrors the browser's DeskTime cookies into the scraper profile. Runs off the main thread.
fn apply_session(app: &App, cookies: &[BrowserCookie]) -> Result<(), String> {
    let window = scraper(app, blank())?;
    // Cookies the browser no longer has are dropped, so a browser logout also ends Nubi's session.
    // A cookie counts as kept only when name, domain and path all match.
    let stale: Vec<(String, String, String)> = window.cookies().map_err(|e| e.to_string())?.into_iter()
        .filter(|old| old.domain().is_some_and(desktime_domain))
        .map(|old| (old.name().to_string(), old.domain().unwrap_or_default().to_string(), old.path().unwrap_or("/").to_string()))
        .filter(|(n, d, p)| !cookies.iter().any(|c| &c.name == n && c.domain.trim_start_matches('.') == d && &c.path == p))
        .collect();
    write_cookies(&window, stale, cookies.to_vec())?;
    let was_enabled = app::with(app, |i| i.settings.desktime_enabled);
    if !was_enabled { save_enabled(app, true)?; }
    let resume = {
        let state = app.state::<DeskTime>();
        let mut inner = state.0.lock().unwrap_or_else(|e| e.into_inner());
        let resume = !was_enabled || inner.logged_out;
        inner.synced_ms = Local::now().timestamp_millis();
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

// ---------- in-app login (alternative to the extension) ----------
// A visible window shows DeskTime's own login page on the scraper's WebView2 profile, so the
// session cookies land where the hidden window reads them. The owner types email and password
// into DeskTime's page; Nubi never reads them. No scraper script and no IPC in this window.
/// True once DeskTime has moved past its login page to the signed-in app.
fn signed_in(url: &tauri::Url) -> bool { allowed(url) && url.path().starts_with("/app") }
/// Sign-in pages may hop to an identity provider (Google, Microsoft); any https page is allowed.
fn login_nav(url: &tauri::Url) -> bool { url.scheme() == "https" }
/// Called once the login window reaches the DeskTime app: turn DeskTime on and read now.
fn finish_login(app: &App) -> Result<(), String> {
    if !app::with(app, |i| i.settings.desktime_enabled) { save_enabled(app, true)?; }
    {
        let state = app.state::<DeskTime>();
        let mut inner = state.0.lock().unwrap_or_else(|e| e.into_inner());
        inner.synced_ms = Local::now().timestamp_millis();
        inner.logged_out = false;
        inner.error = None;
    }
    app::emit_state(app);
    emit(app);
    refresh(app);
    Ok(())
}
#[tauri::command]
pub async fn desktime_login(app: App) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("desktime-login") { let _ = w.set_focus(); return Ok(()); }
    let url: tauri::Url = "https://desktime.com/login".parse().expect("static URL");
    WebviewWindowBuilder::new(&app, "desktime-login", WebviewUrl::External(url))
        .title("Log in to DeskTime").inner_size(480.0, 720.0).center().data_directory(profile())
        .on_navigation(login_nav)
        .on_page_load(|w, payload| {
            if !matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) || !signed_in(payload.url()) { return; }
            let app = w.app_handle().clone();
            let _ = w.close();
            std::thread::spawn(move || {
                if let Err(e) = finish_login(&app) { set_error(&app, &e); emit(&app); }
            });
        })
        .build().map(|_| ()).map_err(|e| e.to_string())
}
/// "site": DeskTime's login page in the default browser. "extension": the unpacked extension folder.
#[tauri::command]
pub fn desktime_open(target: String) -> Result<(), String> {
    let arg = match target.as_str() {
        "site" => "https://desktime.com/login".to_string(),
        "dashboard" => "https://desktime.com/app/my".to_string(),
        "extension_download" => "https://github.com/hitraj2003/nubi/releases/latest/download/nubi-desktime-extension.zip".to_string(),
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
    validate_report(&report, Local::now().date_naive()).inspect_err(|e| eprintln!("[desktime] report rejected: {e}"))?;
    if !app::with(&app, |i| i.settings.desktime_enabled) { return Err("DeskTime is disabled".into()); }
    let settings = app::with(&app, |i| i.settings.clone());
    let state = app.state::<DeskTime>();
    let mut next = None;
    let mut reached = false;
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
            let order = fetch_views(&settings);
            next = order.iter().position(|v| *v == report.view).and_then(|i| order.get(i + 1).copied());
            if report.view == "day" && settings.dt_alert_target {
                // Fires once a day, only when a read crosses the target (not on the first read).
                let same_day = |r: &Report| r.range_start == report.range_start;
                let before = inner.reports.get("day").filter(|(r, _)| same_day(r)).and_then(|(r, _)| day_remaining(r, &settings));
                let today = date(&report.range_start).ok();
                if before.is_some_and(|b| b > 0.0) && day_remaining(&report, &settings).is_some_and(|n| n <= 0.0) && inner.alerted != today {
                    inner.alerted = today;
                    reached = true;
                }
            }
            inner.error = None;
            let view = report.view.clone();
            inner.reports.insert(view, (report, Local::now().timestamp_millis()));
            save_cache(&inner.reports);
        } else {
            // Cards missing: keep the last good data and say so; the cache stays as it is.
            eprintln!("[desktime] cards missing on {} view", report.view);
            inner.error = Some("Could not read DeskTime cards".into());
        }
    }
    emit(&app);
    if reached { notify_target(&app); }
    if let Some(view) = next {
        let a = app.clone();
        let _ = app.run_on_main_thread(move || { let _ = load(&a, view); emit(&a); });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_filter_keeps_only_desktime_pages_scripts_and_data() {
        assert!(keep_resource("https://desktime.com/app/my", Kind::Document));
        assert!(keep_resource("https://static.desktime.com/app.js", Kind::Script));
        assert!(keep_resource("https://desktime.com/api/x", Kind::Xhr));
        assert!(keep_resource("https://desktime.com/api/x", Kind::Fetch));
        assert!(!keep_resource("https://desktime.com/logo.png", Kind::Other));
        assert!(!keep_resource("https://js-agent.newrelic.com/a.js", Kind::Script));
        assert!(!keep_resource("https://www.google.com/recaptcha/api.js", Kind::Script));
        assert!(!keep_resource("https://desktime.com.evil.net/a.js", Kind::Script));
        assert!(!keep_resource("http://desktime.com/app/my", Kind::Document));
        assert!(!keep_resource("not a url", Kind::Document));
    }
    fn day(s: &str) -> NaiveDate { date(s).unwrap() }
    fn report() -> Report { Report { view: "week".into(), productive_min: Some(500), at_work_min: Some(600), range_start: "2026-11-02".into(), range_end: "2026-11-08".into(), holidays: vec![], logged_out: false } }
    fn at(s: &str) -> chrono::DateTime<Local> {
        use chrono::TimeZone;
        Local.from_local_datetime(&chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()).unwrap()
    }
    #[test]
    fn defaults_and_weighted_targets() {
        let mut s = Settings::default();
        assert_eq!(span(&s, day("2026-11-02"), day("2026-11-08"), &[]), (5.0, 2550.0));
        s.holidays.push("2026-11-03".into());
        s.half_days.push("2026-11-04".into());
        s.day_targets.insert(5, "06:00".into());
        assert_eq!(span(&s, day("2026-11-02"), day("2026-11-08"), &[]), (3.5, 1635.0));
        assert_eq!(target(&s, day("2026-11-07"), &[]), 0.0);
        assert_eq!(target(&s, day("2026-11-04"), &["2026-11-04".into()]), 0.0);
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
        let sec = section(&r, 1, &s, at("2026-11-04 11:00"), None);
        assert_eq!(sec.big, "2h 14m");
        assert!(!sec.over);
        assert_eq!(sec.rows[0].note, "74% of target");
        assert_eq!(sec.rows[1].value, "13:14");
        assert_eq!(sec.rows[3].value, "38m");
        r.productive_min = Some(532);
        r.at_work_min = Some(532);
        let sec = section(&r, 1, &s, at("2026-11-04 17:00"), None);
        assert_eq!(sec.big, "+22m");
        assert!(sec.over);
        assert_eq!(sec.percent, 100.0);
        // v5: Finish at disappears once the target is met; no Non-productive row without idle time.
        assert!(!sec.rows.iter().any(|r| r.key == "finish" || r.key == "nonprod"));
    }
    #[test]
    fn period_balance_projection_and_late_arrival() {
        let mut s = Settings::default();
        s.start_of_day = "09:00".into();
        // Wed 2026-11-04 17:30: required Mon-Wed = 3 x 510; 60 min left until 18:30.
        let r = Report { view: "week".into(), productive_min: Some(1530), at_work_min: None, range_start: "2026-11-02".into(), range_end: "2026-11-08".into(), holidays: vec![], logged_out: false };
        let now = at("2026-11-04 17:30");
        let sec = section(&r, 1, &s, now, None);
        assert_eq!(sec.note, "5 days");
        assert_eq!(sec.big, "17h 00m");
        assert_eq!(sec.pace, Some(60.0));
        assert_eq!(sec.rows[0].note, "5 days × 8h 30m");
        assert_eq!(sec.rows[1].note, "of 25h 30m · 3 days");
        assert_eq!(sec.rows[2].value, "+0m");
        assert_eq!(sec.rows[2].note, "ahead · +1h 00m ahead at 18:30");
        assert_eq!((sec.rows[3].label.as_str(), sec.rows[3].value.as_str(), sec.rows[3].note.as_str()), ("At 18:30", "16h 00m left", "8h 00m/day for 2 days"));
        assert_eq!(sec.rows.len(), 4);
        let m = Report { view: "month".into(), productive_min: Some(1530 + 400), at_work_min: None, range_start: "2026-11-01".into(), range_end: "2026-11-30".into(), holidays: vec![], logged_out: false };
        let sec = section(&m, 1, &s, now, None);
        // Required Nov 2-4 = 1530; balance 400 + 60 available - 90 reserve = 370 minutes late allowance.
        assert_eq!(sec.rows[4].value, "6h 10m");
        assert_eq!(sec.rows[4].note, "arrive by 15:10");
        // Week and Today show the same allowance from the month report.
        assert_eq!(section(&r, 1, &s, now, Some(&m)).rows[4].value, "6h 10m");
        let d = Report { view: "day".into(), productive_min: Some(500), at_work_min: None, range_start: "2026-11-04".into(), range_end: "2026-11-04".into(), holidays: vec![], logged_out: false };
        assert!(section(&d, 1, &s, now, Some(&m)).rows.iter().any(|r| r.key == "late" && r.value == "6h 10m"));
        let behind = Report { productive_min: Some(1000), ..m.clone() };
        let late = late_row(Some(&behind), &s, now).unwrap();
        assert_eq!((late.value.as_str(), late.note.as_str()), ("None", "7h 50m behind · need over 1h 30m"));
    }
    #[test]
    fn half_days_count_as_half() {
        let mut s = Settings::default();
        s.half_days.push("2026-11-06".into());
        let r = Report { view: "week".into(), productive_min: Some(0), at_work_min: None, range_start: "2026-11-02".into(), range_end: "2026-11-08".into(), holidays: vec![], logged_out: false };
        let sec = section(&r, 1, &s, at("2026-11-02 10:00"), None);
        assert_eq!(sec.note, "4.5 days");
        assert_eq!(sec.rows[0].note, "4.5 days × 8h 30m");
    }
    #[test]
    fn glance_and_fetch_order() {
        let mut s = Settings::default();
        let day = Report { view: "day".into(), productive_min: Some(300), at_work_min: None, range_start: "2026-11-04".into(), range_end: "2026-11-04".into(), holidays: vec![], logged_out: false };
        let sec = section(&day, 1, &s, at("2026-11-04 11:00"), None);
        assert_eq!(glance(&sec, "left"), "3h 30m left");
        assert_eq!(glance(&sec, "done"), "5h 00m / 8h 30m");
        assert_eq!(fetch_views(&s), vec!["day", "week", "month"]);
        s.dt_views = vec!["month".into()];
        s.dt_alert_target = false;
        assert_eq!(fetch_views(&s), vec!["month"]);
        s.dt_glance = true;
        assert_eq!(fetch_views(&s), vec!["day", "month"]);
        s.dt_glance_kind = "leave".into();
        assert!(validate_settings(&s).is_err());
        s.dt_glance_kind = "left".into();
        s.dt_views = vec![];
        assert!(validate_settings(&s).is_err());
        s.dt_views = vec!["week".into(), "week".into()];
        assert!(validate_settings(&s).is_err());
    }
}

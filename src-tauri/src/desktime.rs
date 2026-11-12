//! DeskTime's server-rendered dashboard, read through an isolated WebView2 profile.
//! A hidden window loads today / week / month in turn; an injected script reports the card numbers
//! through one validated command. The session comes from the owner's browser through the Nubi
//! extension; the WebView2 profile keeps it. Nubi never handles a password.
use crate::app::{self, App};
use crate::protocol::{desktime_domain, validate_browser_msg, BrowserCookie, BrowserMsg, BrowserReply};
use crate::settings;
use chrono::{Local, NaiveDate};
use std::collections::BTreeMap;
use std::sync::Mutex;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

pub use nubi_core::desktime::*;

const PAGE_TIMEOUT_SECS: u64 = 60;

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
                cookie.SetIsSecure(c.secure)?;
                cookie.SetIsHttpOnly(c.http_only)?;
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
}

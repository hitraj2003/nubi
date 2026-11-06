//! Usage limits for Claude and Codex. Credentials are read only, never refreshed,
//! never logged. Poll floor 5 minutes; HTTP 429 backs off (persisted across restarts).
use crate::state::{now_ms, Usage, Window};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

const POLL_SECS: i64 = 300;
const USER_AGENT: &str = concat!("nubi/", env!("CARGO_PKG_VERSION"));
const CLAUDE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const CODEX_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

fn now_s() -> i64 {
    (now_ms() / 1000) as i64
}

fn backoff_path() -> PathBuf {
    crate::settings::data_dir().join("usage_backoff.json")
}

pub fn load_backoff() -> HashMap<String, i64> {
    std::fs::read_to_string(backoff_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_backoff(b: &HashMap<String, i64>) {
    let _ = std::fs::create_dir_all(crate::settings::data_dir());
    let _ = std::fs::write(backoff_path(), serde_json::to_string(b).unwrap_or_default());
}

pub enum Outcome {
    Ok(Vec<Window>),
    NeedsAuth(String),
    Unavailable(String),
    RateLimited(i64),
    Error(String),
}

fn label_for_secs(secs: f64) -> String {
    let h = secs / 3600.0;
    if (h - 5.0).abs() < 0.1 {
        "5h".into()
    } else if (h - 168.0).abs() < 1.0 {
        "7d".into()
    } else if h >= 24.0 {
        format!("{:.0}d", h / 24.0)
    } else {
        format!("{h:.0}h")
    }
}

fn parse_time(v: &Value) -> Option<i64> {
    if let Some(n) = v.as_f64() {
        return Some(if n > 1e12 { (n / 1000.0) as i64 } else { n as i64 });
    }
    chrono::DateTime::parse_from_rfc3339(v.as_str()?).ok().map(|d| d.timestamp())
}

fn get(url: &str, headers: &[(&str, &str)]) -> Result<Value, Outcome> {
    let mut req = ureq::get(url).set("User-Agent", USER_AGENT).timeout(Duration::from_secs(15));
    for (k, v) in headers {
        req = req.set(k, v);
    }
    match req.call() {
        Ok(r) => r.into_json::<Value>().map_err(|e| Outcome::Error(format!("bad reply: {e}"))),
        Err(ureq::Error::Status(429, r)) => {
            let after = r.header("retry-after").and_then(|s| s.parse::<i64>().ok()).unwrap_or(900);
            Err(Outcome::RateLimited(after.clamp(60, 3600)))
        }
        Err(ureq::Error::Status(401 | 403, _)) => Err(Outcome::NeedsAuth("sign-in expired; open the CLI to renew".into())),
        Err(ureq::Error::Status(c, _)) => Err(Outcome::Error(format!("HTTP {c}"))),
        Err(e) => Err(Outcome::Error(format!("network: {}", e.kind()))),
    }
}

/// Windows from the `rate_limits` object Claude Code gives its status line command.
pub fn parse_statusline(v: &Value) -> Vec<Window> {
    let mut out = vec![];
    for (key, label) in [("five_hour", "5h"), ("seven_day", "7d")] {
        if let Some(u) = v[key]["used_percentage"].as_f64() {
            out.push(Window { id: key.into(), label: label.into(), used_percent: u, resets_at: parse_time(&v[key]["resets_at"]) });
        }
    }
    out
}

pub fn parse_claude(v: &Value) -> Vec<Window> {
    let mut out: Vec<Window> = vec![];
    for (key, label) in [
        ("five_hour", "5h"),
        ("seven_day", "7d"),
        ("seven_day_opus", "7d Opus"),
        ("seven_day_sonnet", "7d Sonnet"),
    ] {
        let w = &v[key];
        if let Some(u) = w["utilization"].as_f64() {
            out.push(Window { id: key.into(), label: label.into(), used_percent: u, resets_at: parse_time(&w["resets_at"]) });
        }
    }
    if let Some(list) = v["limits"].as_array() {
        for l in list {
            let kind = l["kind"].as_str().unwrap_or("");
            let id = match kind {
                "session" => "five_hour",
                "weekly_all" | "weekly" => "seven_day",
                k => k,
            };
            if id.is_empty() || out.iter().any(|w| w.id == id) {
                continue;
            }
            if let Some(p) = l["percent"].as_f64() {
                out.push(Window { id: id.into(), label: kind.into(), used_percent: p, resets_at: parse_time(&l["resets_at"]) });
            }
        }
    }
    out
}

pub fn parse_codex(v: &Value) -> Vec<Window> {
    let rl = &v["rate_limit"];
    let now = now_s();
    let mut out = vec![];
    for (id, key) in [("primary", "primary_window"), ("secondary", "secondary_window")] {
        let w = &rl[key];
        let Some(p) = w["used_percent"].as_f64() else { continue };
        let secs = w["limit_window_seconds"].as_f64();
        let reset = parse_time(&w["reset_at"]).or_else(|| w["reset_after_seconds"].as_i64().map(|s| now + s));
        out.push(Window {
            id: id.into(),
            label: secs.map(label_for_secs).unwrap_or_else(|| id.into()),
            used_percent: p,
            resets_at: reset,
        });
    }
    out
}

fn fetch_claude() -> Outcome {
    let dir = std::env::var("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .ok()
        .or_else(|| dirs::home_dir().map(|h| h.join(".claude")));
    let Some(dir) = dir else { return Outcome::Unavailable("no home directory".into()) };
    let text = [".credentials.json", "credentials.json"]
        .iter()
        .find_map(|n| std::fs::read_to_string(dir.join(n)).ok());
    let Some(text) = text else { return Outcome::Unavailable("Claude Code not signed in".into()) };
    let Ok(v) = serde_json::from_str::<Value>(&text) else { return Outcome::Error("unreadable credentials".into()) };
    let o = v.get("claudeAiOauth").unwrap_or(&v);
    let Some(tok) = o["accessToken"].as_str().filter(|t| !t.trim().is_empty()) else {
        return Outcome::Unavailable("Claude Code not signed in (API key mode has no limits)".into());
    };
    let expired_days = o["expiresAt"]
        .as_f64()
        .map(|ms| (now_ms() as f64 - ms) / 86_400_000.0)
        .filter(|d| *d > 0.0);
    let bearer = format!("Bearer {tok}");
    match get(CLAUDE_URL, &[("Authorization", &bearer), ("anthropic-beta", "oauth-2025-04-20")]) {
        Ok(v) => Outcome::Ok(parse_claude(&v)),
        Err(Outcome::NeedsAuth(_)) if expired_days.is_some() => Outcome::NeedsAuth(format!(
            "Claude Code's saved sign-in expired {:.0} day(s) ago; start `claude` once to renew it",
            expired_days.unwrap().max(1.0)
        )),
        Err(o) => o,
    }
}

fn fetch_codex() -> Outcome {
    let Some(home) = crate::hooks_config::codex_home() else { return Outcome::Unavailable("no home directory".into()) };
    let Ok(text) = std::fs::read_to_string(home.join("auth.json")) else {
        return Outcome::Unavailable("no auth.json (not signed in, or keyring storage: not supported yet)".into());
    };
    let Ok(v) = serde_json::from_str::<Value>(&text) else { return Outcome::Error("unreadable auth.json".into()) };
    let t = &v["tokens"];
    let (Some(tok), Some(acct)) = (t["access_token"].as_str(), t["account_id"].as_str()) else {
        return Outcome::Unavailable("Codex uses an API key or has no ChatGPT sign-in".into());
    };
    let bearer = format!("Bearer {tok}");
    match get(CODEX_URL, &[("Authorization", &bearer), ("ChatGPT-Account-Id", acct)]) {
        Ok(v) => Outcome::Ok(parse_codex(&v)),
        Err(o) => o,
    }
}

pub fn fetch(provider: &str) -> Outcome {
    if provider == "codex" {
        fetch_codex()
    } else {
        fetch_claude()
    }
}

/// Apply an outcome to the shown usage; returns the next poll time (unix seconds).
pub fn apply(provider: &str, out: Outcome, prev: Option<&Usage>) -> (Usage, i64) {
    let now = now_s();
    let keep = prev.map(|p| p.windows.clone()).unwrap_or_default();
    let fetched_ms = prev.filter(|p| !p.windows.is_empty()).and_then(|p| p.fetched_ms);
    let mk = |status: &str, msg: Option<String>, windows: Vec<Window>| Usage {
        provider: provider.into(),
        status: status.into(),
        message: msg,
        windows,
        // An unsuccessful attempt must not make cached numbers look fresh.
        fetched_ms: if status == "ok" { Some(now_ms()) } else { fetched_ms },
    };
    match out {
        Outcome::Ok(w) if w.is_empty() => (mk("unavailable", Some("no limit windows returned".into()), keep), now + POLL_SECS),
        Outcome::Ok(w) => (mk("ok", None, w), now + POLL_SECS),
        Outcome::NeedsAuth(m) => (mk("needs_auth", Some(m), keep), now + POLL_SECS),
        Outcome::Unavailable(m) => (mk("unavailable", Some(m), keep), now + POLL_SECS),
        Outcome::RateLimited(s) => (mk("backoff", Some(format!("rate limited, retry in {} min", s / 60)), keep), now + s),
        Outcome::Error(m) => (mk("error", Some(m), keep), now + POLL_SECS),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_fetches_keep_last_successful_values_and_time() {
        let (mut cached, _) = apply("codex", Outcome::Ok(parse_codex(&json!({
            "rate_limit":{"primary_window":{"used_percent":25,"limit_window_seconds":18000}}
        }))), None);
        cached.fetched_ms = Some(1_000);
        for outcome in [Outcome::RateLimited(900), Outcome::Error("offline".into()),
            Outcome::NeedsAuth("expired".into()), Outcome::Unavailable("no login".into()), Outcome::Ok(vec![])] {
            let before = now_s();
            let (stale, next) = apply("codex", outcome, Some(&cached));
            assert_ne!(stale.status, "ok");
            assert_eq!(stale.windows[0].used_percent, 25.0);
            assert_eq!(stale.fetched_ms, Some(1_000));
            assert!(next >= before + if stale.status == "backoff" { 900 } else { POLL_SECS });
            cached = stale;
        }
        let (fresh, _) = apply("codex", Outcome::Ok(vec![Window { used_percent: 35.0, ..Window::default() }]), Some(&cached));
        assert_eq!(fresh.status, "ok");
        assert_eq!(fresh.windows[0].used_percent, 35.0);
        assert!(fresh.fetched_ms.unwrap() > 1_000);
    }

    #[test]
    fn failed_first_fetch_has_no_success_timestamp() {
        let (usage, _) = apply("claude", Outcome::RateLimited(60), None);
        assert!(usage.windows.is_empty());
        assert_eq!(usage.fetched_ms, None);
    }

    #[test]
    fn usage_request_identifies_app_version() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                assert_eq!(stream.read(&mut byte).unwrap(), 1);
                request.push(byte[0]);
            }
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").unwrap();
            String::from_utf8(request).unwrap()
        });
        assert!(get(&format!("http://{addr}/usage"), &[]).is_ok());
        let request = server.join().unwrap().to_ascii_lowercase();
        assert!(request.lines().any(|line| line == format!("user-agent: nubi/{}", env!("CARGO_PKG_VERSION"))));
    }

    #[test]
    fn statusline_rate_limits() {
        let w = parse_statusline(&json!({"five_hour":{"used_percentage":42.5,"resets_at":1800000000},"seven_day":{"used_percentage":7,"resets_at":1800500000}}));
        assert_eq!(w.len(), 2);
        assert_eq!((w[0].label.as_str(), w[0].used_percent, w[0].resets_at), ("5h", 42.5, Some(1800000000)));
        assert_eq!(w[1].label, "7d");
        assert!(parse_statusline(&json!({})).is_empty());
    }
    use serde_json::json;

    #[test]
    fn claude_named_windows() {
        let v = json!({"five_hour":{"utilization":27.0,"resets_at":"2026-11-03T12:00:00+00:00"},"seven_day":{"utilization":4.0,"resets_at":null},"seven_day_opus":null});
        let w = parse_claude(&v);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].label, "5h");
        assert!(w[0].resets_at.is_some());
        assert!(w[1].resets_at.is_none());
    }

    #[test]
    fn missing_is_not_zero() {
        assert!(parse_claude(&json!({})).is_empty());
        assert!(parse_codex(&json!({"rate_limit":{"primary_window":{}}})).is_empty());
    }

    #[test]
    fn codex_windows() {
        let v = json!({"rate_limit":{"primary_window":{"used_percent":25,"limit_window_seconds":18000,"reset_at":1800001000},"secondary_window":{"used_percent":3,"limit_window_seconds":604800,"reset_after_seconds":60}}});
        let w = parse_codex(&v);
        assert_eq!(w[0].label, "5h");
        assert_eq!(w[0].resets_at, Some(1800001000));
        assert_eq!(w[1].label, "7d");
    }
}

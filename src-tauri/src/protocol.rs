//! Wire format between the `--hook` client and the GUI, plus hook stdout output.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Largest hook stdin we accept. Larger input: the hook exits silently and the
/// provider shows its own prompt.
pub const MAX_STDIN: usize = 1024 * 1024;
/// Largest frame on the pipe.
pub const MAX_FRAME: usize = 512 * 1024;
/// Tool input above this size is not shown and cannot be approved from the overlay.
pub const MAX_TOOL_INPUT: usize = 32 * 1024;
/// Hook timeout written into Claude/Codex config for PermissionRequest.
pub const HOOK_TIMEOUT_SECS: u64 = 120;
/// Client gives up before the provider kills the hook.
pub const PERMISSION_DEADLINE_SECS: u64 = 100;
/// Hook timeout for PreToolUse (covers AskUserQuestion, which waits for an answer).
pub const QUESTION_HOOK_TIMEOUT_SECS: u64 = 300;
/// How long a question waits for an answer in the overlay before the terminal asks instead.
pub const QUESTION_DEADLINE_SECS: u64 = 240;

pub fn pipe_name() -> String {
    let user: String = std::env::var("USERNAME")
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let user = if user.is_empty() { "default".to_string() } else { user };
    format!(r"\\.\pipe\nubi-{user}")
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct HookMsg {
    pub v: u32,
    pub provider: String,
    pub event: String,
    pub session_id: String,
    pub cwd: Option<String>,
    pub transcript_path: Option<String>,
    pub tool_name: Option<String>,
    pub tool_input: Option<Value>,
    /// Tool input existed but was too large to forward.
    pub tool_input_oversize: bool,
    pub message: Option<String>,
    pub notification_type: Option<String>,
    pub pids: Vec<u32>,
    pub wants_reply: bool,
    pub deadline_secs: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Reply {
    /// "allow" | "deny" | "answer"
    pub decision: String,
    pub message: Option<String>,
    /// For "answer": question text -> chosen label(s), multi-select joined with ", ".
    #[serde(default)]
    pub answers: Option<serde_json::Map<String, Value>>,
}

// ---------- DeskTime browser connector ----------
// The Nubi browser extension sends these through Chrome native messaging (`nubi.exe
// chrome-extension://<id>/`), which forwards them to the GUI as `{"browser": ...}` pipe frames.

/// Largest native message accepted from the browser.
pub const MAX_BROWSER_MSG: usize = 64 * 1024;
const MAX_COOKIES: usize = 64;

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    /// Unix seconds; None for a browser-session cookie.
    pub expires: Option<f64>,
}
// Cookie values are secrets: Debug never prints them.
impl std::fmt::Debug for BrowserCookie {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BrowserCookie({} @ {}{})", self.name, self.domain, self.path)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum BrowserMsg {
    /// The DeskTime cookies of the browser. `sync` marks a background re-send after a cookie change.
    DesktimeSession {
        cookies: Vec<BrowserCookie>,
        #[serde(default)]
        sync: bool,
    },
    DesktimeRevoke,
}

/// Pipe frame for a browser message; a HookMsg never has a `browser` key.
#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct BrowserFrame {
    pub browser: BrowserMsg,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Default)]
pub struct BrowserReply {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Access was removed in Nubi; the extension stops re-sending until allowed again.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub revoked: bool,
}
impl BrowserReply {
    pub fn result(r: Result<(), String>) -> Self {
        match r {
            Ok(()) => Self { ok: true, ..Default::default() },
            Err(e) => Self { error: Some(e), ..Default::default() },
        }
    }
}

/// desktime.com or a subdomain, with or without the leading dot cookies use for subdomains.
pub fn desktime_domain(domain: &str) -> bool {
    let d = domain.strip_prefix('.').unwrap_or(domain).to_ascii_lowercase();
    d.len() <= 253
        && d.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        && !d.starts_with('.')
        && !d.contains("..")
        && (d == "desktime.com" || d.ends_with(".desktime.com"))
}

fn cookie_ok(c: &BrowserCookie) -> Result<(), &'static str> {
    // RFC 6265 token: visible ASCII without separators.
    let token = |b: u8| b.is_ascii_graphic() && !b"()<>@,;:\\\"/[]?={}".contains(&b);
    if c.name.is_empty() || c.name.len() > 256 || !c.name.bytes().all(token) {
        return Err("Invalid cookie name");
    }
    // Visible ASCII without `;`, so a value can never add cookie attributes.
    if c.value.len() > 4096 || !c.value.bytes().all(|b| (b.is_ascii_graphic() && b != b';') || b == b' ') {
        return Err("Invalid cookie value");
    }
    if !desktime_domain(&c.domain) {
        return Err("Cookie is not for desktime.com");
    }
    if !c.path.starts_with('/') || c.path.len() > 1024 || !c.path.bytes().all(|b| b.is_ascii_graphic() && b != b';') {
        return Err("Invalid cookie path");
    }
    if c.expires.is_some_and(|e| !e.is_finite() || !(0.0..1e11).contains(&e)) {
        return Err("Invalid cookie expiry");
    }
    Ok(())
}

pub fn validate_browser_msg(m: &BrowserMsg) -> Result<(), String> {
    if let BrowserMsg::DesktimeSession { cookies, .. } = m {
        if cookies.len() > MAX_COOKIES {
            return Err("Too many cookies".into());
        }
        for c in cookies {
            cookie_ok(c)?;
        }
    }
    Ok(())
}

/// Parse and validate one native message body from the browser.
pub fn parse_browser_msg(body: &[u8]) -> Result<BrowserMsg, String> {
    if body.len() > MAX_BROWSER_MSG {
        return Err("Message is too large".into());
    }
    // serde errors can quote input, which may hold cookie values: report a fixed message.
    let msg: BrowserMsg = serde_json::from_slice(body).map_err(|_| "Unexpected message".to_string())?;
    validate_browser_msg(&msg)?;
    Ok(msg)
}

/// True for a Claude Code AskUserQuestion the overlay can answer.
pub fn is_question(provider: &str, event: &str, tool: Option<&str>) -> bool {
    provider == "claude" && event == "PreToolUse" && tool == Some("AskUserQuestion")
}

/// Only native Claude questions bypass the popup when their terminal host is focused.
pub fn question_in_terminal(msg: &HookMsg, host_focused: bool) -> bool {
    host_focused && is_question(&msg.provider, &msg.event, msg.tool_name.as_deref())
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(|x| x.to_string())
}

/// Build the allowlisted message from raw hook stdin. None = nothing to send.
pub fn build_msg(provider: &str, raw: &Value, pids: Vec<u32>) -> Option<HookMsg> {
    let event = s(raw, "hook_event_name")?;
    let session_id = s(raw, "session_id")?;
    let mut tool_input = raw.get("tool_input").cloned();
    let mut oversize = false;
    if let Some(ti) = &tool_input {
        if serde_json::to_string(ti).map(|t| t.len()).unwrap_or(usize::MAX) > MAX_TOOL_INPUT {
            tool_input = None;
            oversize = true;
        }
    }
    let tool = s(raw, "tool_name");
    let question = is_question(provider, &event, tool.as_deref());
    let wants_reply = (event == "PermissionRequest" || question) && !oversize;
    Some(HookMsg {
        v: 1,
        provider: provider.to_string(),
        event,
        session_id,
        cwd: s(raw, "cwd"),
        transcript_path: s(raw, "transcript_path"),
        tool_name: s(raw, "tool_name"),
        tool_input,
        tool_input_oversize: oversize,
        message: s(raw, "message"),
        notification_type: s(raw, "notification_type"),
        pids,
        wants_reply,
        deadline_secs: if question { QUESTION_DEADLINE_SECS } else { PERMISSION_DEADLINE_SECS },
    })
}

/// Hook stdout for a PermissionRequest reply. None = print nothing (native prompt).
pub fn decision_json(r: &Reply) -> Option<String> {
    let decision = match r.decision.as_str() {
        "allow" => json!({ "behavior": "allow" }),
        "deny" => {
            let mut d = json!({ "behavior": "deny" });
            if let Some(m) = &r.message {
                d["message"] = json!(m);
            }
            d
        }
        _ => return None,
    };
    Some(
        json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": decision } })
            .to_string(),
    )
}

/// Hook stdout for an answered AskUserQuestion: allow + the original questions + answers.
/// None = print nothing (the terminal asks).
pub fn question_json(tool_input: &Value, r: &Reply) -> Option<String> {
    if r.decision != "answer" {
        return None;
    }
    let answers = r.answers.as_ref().filter(|a| !a.is_empty())?;
    let questions = tool_input.get("questions")?.as_array()?;
    // Every answer must name a question that was asked.
    let asked: Vec<&str> = questions.iter().filter_map(|q| q["question"].as_str()).collect();
    if !answers.keys().all(|k| asked.contains(&k.as_str())) {
        return None;
    }
    Some(
        json!({ "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow",
            "updatedInput": { "questions": questions, "answers": answers }
        }})
        .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_json_matches_contract() {
        let out = decision_json(&Reply { decision: "allow".into(), message: None, answers: None }).unwrap();
        assert_eq!(
            out,
            r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}"#
        );
    }

    #[test]
    fn deny_carries_message() {
        let out = decision_json(&Reply { decision: "deny".into(), message: Some("no".into()), answers: None }).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["hookSpecificOutput"]["decision"]["behavior"], "deny");
        assert_eq!(v["hookSpecificOutput"]["decision"]["message"], "no");
    }

    #[test]
    fn unknown_decision_prints_nothing() {
        assert!(decision_json(&Reply { decision: "maybe".into(), message: None, answers: None }).is_none());
    }

    #[test]
    fn oversize_tool_input_is_not_approvable() {
        let big = "x".repeat(MAX_TOOL_INPUT + 10);
        let raw = json!({"hook_event_name":"PermissionRequest","session_id":"s","tool_name":"Bash","tool_input":{"command": big}});
        let m = build_msg("claude", &raw, vec![]).unwrap();
        assert!(m.tool_input.is_none());
        assert!(m.tool_input_oversize);
        assert!(!m.wants_reply);
    }

    #[test]
    fn only_permission_request_waits() {
        let raw = json!({"hook_event_name":"Stop","session_id":"s"});
        assert!(!build_msg("codex", &raw, vec![]).unwrap().wants_reply);
        let raw = json!({"hook_event_name":"PermissionRequest","session_id":"s","tool_input":{"command":"ls"}});
        assert!(build_msg("codex", &raw, vec![]).unwrap().wants_reply);
    }

    #[test]
    fn question_answer_echoes_questions() {
        let ti = json!({"questions":[{"question":"Which framework?","header":"F","options":[{"label":"React"},{"label":"Vue"}],"multiSelect":false}]});
        let mut a = serde_json::Map::new();
        a.insert("Which framework?".into(), json!("React"));
        let out = question_json(&ti, &Reply { decision: "answer".into(), message: None, answers: Some(a) }).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "allow");
        assert_eq!(v["hookSpecificOutput"]["updatedInput"]["answers"]["Which framework?"], "React");
        assert_eq!(v["hookSpecificOutput"]["updatedInput"]["questions"][0]["header"], "F");
    }

    #[test]
    fn unknown_question_key_prints_nothing() {
        let ti = json!({"questions":[{"question":"Q1","options":[]}]});
        let mut a = serde_json::Map::new();
        a.insert("Other".into(), json!("x"));
        assert!(question_json(&ti, &Reply { decision: "answer".into(), message: None, answers: Some(a) }).is_none());
        assert!(question_json(&ti, &Reply { decision: "skip".into(), message: None, answers: None }).is_none());
    }

    #[test]
    fn only_claude_questions_wait() {
        let raw = json!({"hook_event_name":"PreToolUse","session_id":"s","tool_name":"AskUserQuestion","tool_input":{"questions":[]}});
        assert!(build_msg("claude", &raw, vec![]).unwrap().wants_reply);
        assert_eq!(build_msg("claude", &raw, vec![]).unwrap().deadline_secs, QUESTION_DEADLINE_SECS);
        assert!(!build_msg("codex", &raw, vec![]).unwrap().wants_reply);
        let raw = json!({"hook_event_name":"PreToolUse","session_id":"s","tool_name":"Bash"});
        assert!(!build_msg("claude", &raw, vec![]).unwrap().wants_reply);
    }

    #[test]
    fn missing_session_is_dropped() {
        assert!(build_msg("claude", &json!({"hook_event_name":"Stop"}), vec![]).is_none());
    }

    fn cookie() -> Value {
        json!({"name":"desktime_session","value":"abc=def","domain":".desktime.com","path":"/","secure":true,"httpOnly":true,"expires":1893456000.5})
    }

    #[test]
    fn browser_session_parses_and_validates() {
        let body = json!({"type":"desktime_session","cookies":[cookie()]}).to_string();
        let BrowserMsg::DesktimeSession { cookies, sync } = parse_browser_msg(body.as_bytes()).unwrap() else { panic!() };
        assert!(!sync);
        assert_eq!(cookies[0].name, "desktime_session");
        assert!(cookies[0].http_only);
        let body = json!({"type":"desktime_session","cookies":[],"sync":true}).to_string();
        assert_eq!(parse_browser_msg(body.as_bytes()).unwrap(), BrowserMsg::DesktimeSession { cookies: vec![], sync: true });
        assert_eq!(parse_browser_msg(br#"{"type":"desktime_revoke"}"#).unwrap(), BrowserMsg::DesktimeRevoke);
        // Session cookies carry no expiry.
        let mut c = cookie();
        c["expires"] = Value::Null;
        assert!(parse_browser_msg(json!({"type":"desktime_session","cookies":[c]}).to_string().as_bytes()).is_ok());
    }

    #[test]
    fn browser_message_shape_is_strict() {
        for body in [
            r#"{"type":"other"}"#,
            r#"{"type":"desktime_session"}"#,
            r#"{"type":"desktime_session","cookies":[],"extra":1}"#,
            r#"not json"#,
        ] {
            assert!(parse_browser_msg(body.as_bytes()).is_err(), "{body}");
        }
        let mut c = cookie();
        c["sameSite"] = json!("lax");
        assert!(parse_browser_msg(json!({"type":"desktime_session","cookies":[c]}).to_string().as_bytes()).is_err());
        let many: Vec<Value> = (0..65).map(|_| cookie()).collect();
        assert!(parse_browser_msg(json!({"type":"desktime_session","cookies":many}).to_string().as_bytes()).is_err());
        assert!(parse_browser_msg(&vec![b' '; MAX_BROWSER_MSG + 1]).is_err());
    }

    #[test]
    fn cookie_errors_never_echo_values() {
        let mut c = cookie();
        c["value"] = json!("secret;Domain=evil.com");
        let err = parse_browser_msg(json!({"type":"desktime_session","cookies":[c]}).to_string().as_bytes()).unwrap_err();
        assert!(!err.contains("secret"));
        let err = parse_browser_msg(br#"{"type":"desktime_session","cookies":[{"value":"secret"}]}"#).unwrap_err();
        assert!(!err.contains("secret"));
    }

    #[test]
    fn cookie_fields_are_checked() {
        let bad = |k: &str, v: Value| {
            let mut c = cookie();
            c[k] = v;
            parse_browser_msg(json!({"type":"desktime_session","cookies":[c]}).to_string().as_bytes()).is_err()
        };
        for d in ["evil.com", "desktime.com.evil.com", "evildesktime.com", "..desktime.com", "a..desktime.com", "desk time.com"] {
            assert!(bad("domain", json!(d)), "{d}");
        }
        for d in ["desktime.com", ".desktime.com", "app.desktime.com", "DeskTime.com"] {
            assert!(!bad("domain", json!(d)), "{d}");
        }
        assert!(bad("name", json!("")));
        assert!(bad("name", json!("a=b")));
        assert!(bad("name", json!("x".repeat(257))));
        assert!(bad("value", json!("a\nb")));
        assert!(bad("value", json!("x".repeat(4097))));
        assert!(bad("path", json!("app")));
        assert!(bad("path", json!("/a;b")));
        assert!(bad("expires", json!(-1.0)));
        assert!(bad("expires", json!(1e12)));
        assert!(format!("{:?}", serde_json::from_value::<BrowserCookie>(cookie()).unwrap()).find("abc").is_none());
    }

    #[test]
    fn browser_frames_and_hook_frames_do_not_mix() {
        let frame = serde_json::to_string(&BrowserFrame { browser: BrowserMsg::DesktimeRevoke }).unwrap();
        assert_eq!(frame, r#"{"browser":{"type":"desktime_revoke"}}"#);
        assert!(serde_json::from_str::<HookMsg>(&frame).is_err());
        let hook = serde_json::to_string(&HookMsg { v: 1, provider: "claude".into(), ..Default::default() }).unwrap();
        assert!(serde_json::from_str::<BrowserFrame>(&hook).is_err());
        assert_eq!(serde_json::to_string(&BrowserReply::result(Ok(()))).unwrap(), r#"{"ok":true}"#);
        assert_eq!(serde_json::to_string(&BrowserReply::result(Err("x".into()))).unwrap(), r#"{"ok":false,"error":"x"}"#);
    }

    #[test]
    fn focused_questions_use_terminal_but_permissions_keep_their_route() {
        let question = json!({"hook_event_name":"PreToolUse","session_id":"s","tool_name":"AskUserQuestion"});
        let msg = build_msg("claude", &question, vec![]).unwrap();
        assert!(question_in_terminal(&msg, true));
        assert!(!question_in_terminal(&msg, false));
        assert!(!question_in_terminal(&build_msg("codex", &question, vec![]).unwrap(), true));
        let permission = json!({"hook_event_name":"PermissionRequest","session_id":"s","tool_name":"Bash"});
        for provider in ["claude", "codex"] {
            assert!(!question_in_terminal(&build_msg(provider, &permission, vec![]).unwrap(), true));
        }
    }
}

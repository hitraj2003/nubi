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

pub fn pipe_name() -> String {
    let user: String = std::env::var("USERNAME")
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let user = if user.is_empty() { "default".to_string() } else { user };
    format!(r"\\.\pipe\claude-overlay-{user}")
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
    /// "allow" | "deny"
    pub decision: String,
    pub message: Option<String>,
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
    let wants_reply = event == "PermissionRequest" && !oversize;
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
        deadline_secs: PERMISSION_DEADLINE_SECS,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_json_matches_contract() {
        let out = decision_json(&Reply { decision: "allow".into(), message: None }).unwrap();
        assert_eq!(
            out,
            r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}"#
        );
    }

    #[test]
    fn deny_carries_message() {
        let out = decision_json(&Reply { decision: "deny".into(), message: Some("no".into()) }).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["hookSpecificOutput"]["decision"]["behavior"], "deny");
        assert_eq!(v["hookSpecificOutput"]["decision"]["message"], "no");
    }

    #[test]
    fn unknown_decision_prints_nothing() {
        assert!(decision_json(&Reply { decision: "maybe".into(), message: None }).is_none());
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
    fn missing_session_is_dropped() {
        assert!(build_msg("claude", &json!({"hook_event_name":"Stop"}), vec![]).is_none());
    }
}

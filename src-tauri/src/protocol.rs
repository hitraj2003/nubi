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
    /// "allow" | "deny" | "answer"
    pub decision: String,
    pub message: Option<String>,
    /// For "answer": question text -> chosen label(s), multi-select joined with ", ".
    #[serde(default)]
    pub answers: Option<serde_json::Map<String, Value>>,
}

/// True for a Claude Code AskUserQuestion the overlay can answer.
pub fn is_question(provider: &str, event: &str, tool: Option<&str>) -> bool {
    provider == "claude" && event == "PreToolUse" && tool == Some("AskUserQuestion")
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
}

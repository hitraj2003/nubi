//! In-memory app state: sessions, pending approvals, alerts, usage.
use crate::protocol::{HookMsg, Reply};
use crate::settings::Settings;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::{Read, Seek, SeekFrom};
use tokio::sync::oneshot;

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

const STALE_MS: u64 = 2 * 3600 * 1000;
const DROP_MS: u64 = 12 * 3600 * 1000;
const MAX_ALERTS: usize = 50;

#[derive(Serialize, Clone, Debug)]
pub struct Session {
    pub key: String,
    pub provider: String,
    pub session_id: String,
    pub cwd: Option<String>,
    pub folder: String,
    /// Session title: Claude `/rename` or its auto title, Codex thread name. Empty until known.
    pub name: String,
    #[serde(skip)]
    pub name_checked_ms: u64,
    /// "idle" | "working" | "waiting" | "done" | "stale"
    pub state: String,
    pub attention: bool,
    pub last_event: String,
    pub last_event_ms: u64,
    pub subagents: i32,
    pub context_tokens: Option<u64>,
    #[serde(skip)]
    pub pids: Vec<u32>,
    #[serde(skip)]
    pub action_target: Option<crate::session_actions::Target>,
    pub compact_queued: bool,
    pub action_error: Option<String>,
    #[serde(skip)]
    pub transcript_path: Option<String>,
}

pub struct Pending {
    pub id: u64,
    /// "permission" | "question"
    pub kind: String,
    pub provider: String,
    pub session_key: String,
    pub folder: String,
    pub name: String,
    pub tool_name: Option<String>,
    pub tool_input: Option<Value>,
    pub created_ms: u64,
    pub expires_ms: u64,
    /// The pointer is over the card: the countdown is paused.
    pub held: bool,
    pub tx: Option<oneshot::Sender<Reply>>,
}

/// A request the notch held until its takeover time ran out; it now waits in the terminal.
/// Kept so the notch can say so until the user dismisses it or the session moves on.
#[derive(Serialize, Clone, Debug)]
pub struct Missed {
    pub id: u64,
    pub kind: String,
    pub provider: String,
    pub session_key: String,
    pub folder: String,
    pub name: String,
    pub created_ms: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct Alert {
    pub ts: u64,
    pub provider: String,
    /// "done" | "permission" | "idle" | "info"
    pub kind: String,
    pub title: String,
    pub body: String,
    pub session_key: Option<String>,
    /// Shown while DND was on (queued, no pill/sound).
    pub queued: bool,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Window {
    pub id: String,
    pub label: String,
    pub used_percent: f64,
    /// Unix seconds.
    pub resets_at: Option<i64>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Usage {
    pub provider: String,
    /// "ok" | "needs_auth" | "unavailable" | "error" | "backoff" | "disabled"
    pub status: String,
    pub message: Option<String>,
    pub windows: Vec<Window>,
    /// Time of the last successful nonempty usage update, not the last attempt.
    pub fetched_ms: Option<u64>,
}

#[derive(Default)]
pub struct Inner {
    pub settings: Settings,
    /// Last time Claude usage came from the status line (ms). While recent, the API is not polled.
    pub claude_live_ms: u64,
    pub sessions: BTreeMap<String, Session>,
    pub pending: HashMap<u64, Pending>,
    pub missed: Vec<Missed>,
    pub alerts: VecDeque<Alert>,
    pub usage: BTreeMap<String, Usage>,
    pub dnd_manual: bool,
    pub next_id: u64,
    pub pipe_error: Option<String>,
    pub hooks: HashMap<String, bool>,
}

/// Title of a session, if it has one. Claude: last `custom-title` (from `/rename`) in the
/// transcript, else the last `ai-title`. Codex: last `thread_name` for the id in
/// `~/.codex/session_index.jsonl`.
fn session_name(provider: &str, session_id: &str, transcript: Option<&str>) -> Option<String> {
    let pick = |v: &Value, k: &str| v.get(k).and_then(|x| x.as_str()).map(str::trim).filter(|x| !x.is_empty()).map(String::from);
    if provider == "codex" {
        let path = dirs::home_dir()?.join(".codex").join("session_index.jsonl");
        let text = std::fs::read_to_string(path).ok()?;
        return text.lines().rev().filter(|l| l.contains(session_id)).find_map(|l| {
            let v: Value = serde_json::from_str(l).ok()?;
            if v.get("id")?.as_str()? != session_id { return None; }
            pick(&v, "thread_name")
        });
    }
    let text = std::fs::read_to_string(transcript?).ok()?;
    let (mut custom, mut ai) = (None, None);
    for l in text.lines() {
        if l.contains("\"type\":\"custom-title\"") {
            if let Ok(v) = serde_json::from_str::<Value>(l) { custom = pick(&v, "customTitle").or(custom); }
        } else if l.contains("\"type\":\"ai-title\"") {
            if let Ok(v) = serde_json::from_str::<Value>(l) { ai = pick(&v, "aiTitle").or(ai); }
        }
    }
    custom.or(ai)
}

fn folder_of(cwd: &Option<String>) -> String {
    cwd.as_deref()
        .map(|c| c.trim_end_matches(['/', '\\']))
        .and_then(|c| c.rsplit(['/', '\\']).next())
        .filter(|s| !s.is_empty())
        .unwrap_or("session")
        .to_string()
}

impl Inner {
    /// Update session state from a hook event. Returns an alert to show, if any.
    pub fn on_event(&mut self, m: &HookMsg) -> Option<Alert> {
        let key = format!("{}:{}", m.provider, m.session_id);
        // Clear requests on session progress, not unrelated subagent activity or
        // a permission notification that means the terminal is still waiting.
        if matches!(m.event.as_str(), "SessionStart" | "SessionEnd" | "UserPromptSubmit" | "PreToolUse" | "PostToolUse" | "PreModelRequest" | "Stop" | "Interrupt")
            || (m.event == "Notification" && m.notification_type.as_deref() == Some("idle_prompt")) {
            self.missed.retain(|x| x.session_key != key);
            self.pending.retain(|_, p| p.session_key != key);
        }
        if m.event == "SessionEnd" {
            self.sessions.remove(&key);
            return None;
        }
        let now = now_ms();
        let s = self.sessions.entry(key.clone()).or_insert_with(|| Session {
            key: key.clone(),
            provider: m.provider.clone(),
            session_id: m.session_id.clone(),
            cwd: None,
            folder: String::new(),
            name: String::new(),
            name_checked_ms: 0,
            state: "idle".into(),
            attention: false,
            last_event: String::new(),
            last_event_ms: now,
            subagents: 0,
            context_tokens: None,
            pids: vec![],
            action_target: None,
            compact_queued: false,
            action_error: None,
            transcript_path: None,
        });
        if m.cwd.is_some() {
            s.cwd = m.cwd.clone();
        }
        s.folder = folder_of(&s.cwd);
        if !m.pids.is_empty() {
            s.pids = m.pids.clone();
            if s.action_target.is_none() {
                s.action_target = crate::session_actions::find_target(&s.provider, &m.pids);
            }
        }
        if m.transcript_path.is_some() {
            s.transcript_path = m.transcript_path.clone();
        }
        s.last_event = m.event.clone();
        s.last_event_ms = now;
        // Re-read the title at most every 20 s (titles appear after the first prompt and can be renamed).
        if now.saturating_sub(s.name_checked_ms) > 20_000 {
            s.name_checked_ms = now;
            if let Some(n) = session_name(&s.provider, &s.session_id, s.transcript_path.as_deref()) {
                s.name = n;
            }
        }
        let label = if s.name.is_empty() { s.folder.clone() } else { s.name.clone() };
        let mk = |kind: &str, title: &str, body: String| Alert {
            ts: now,
            provider: m.provider.clone(),
            kind: kind.into(),
            title: title.into(),
            body,
            session_key: Some(key.clone()),
            queued: false,
        };
        match m.event.as_str() {
            "SessionStart" => {
                s.state = "idle".into();
                s.attention = false;
                None
            }
            "PreToolUse" if crate::protocol::is_question(&m.provider, &m.event, m.tool_name.as_deref()) => {
                s.state = "waiting".into();
                s.attention = true;
                let q = m.tool_input.as_ref()
                    .and_then(|t| t["questions"][0]["question"].as_str())
                    .unwrap_or("Claude has a question")
                    .to_string();
                Some(mk("question", "Question for you", format!("{label} · {q}")))
            }
            "UserPromptSubmit" | "PreToolUse" | "PostToolUse" | "PreModelRequest" => {
                s.state = "working".into();
                s.attention = false;
                None
            }
            "SubagentStart" => {
                s.subagents += 1;
                s.state = "working".into();
                None
            }
            "SubagentStop" => {
                s.subagents = (s.subagents - 1).max(0);
                None
            }
            "PermissionRequest" => {
                s.state = "waiting".into();
                s.attention = true;
                let tool = m.tool_name.clone().unwrap_or_else(|| "tool".into());
                let note = if m.tool_input_oversize { " (too large: answer in terminal)" } else { "" };
                Some(mk("permission", "Permission needed", format!("{label} · {tool}{note}")))
            }
            "Interrupt" => {
                s.state = "idle".into();
                s.attention = false;
                None
            }
            "Notification" => {
                let nt = m.notification_type.clone().unwrap_or_default();
                let msg = m.message.clone().unwrap_or_default();
                if nt == "idle_prompt" || msg.contains("waiting for your input") {
                    s.state = "idle".into();
                    s.attention = true;
                    Some(mk("idle", "Waiting for you", label.clone()))
                } else if nt == "permission_prompt" {
                    s.state = "waiting".into();
                    s.attention = true;
                    None
                } else {
                    None
                }
            }
            "Stop" => {
                s.state = "done".into();
                s.attention = true;
                if let Some(p) = &s.transcript_path {
                    s.context_tokens = context_tokens(&m.provider, p).or(s.context_tokens);
                }
                Some(mk("done", "Task complete", label.clone()))
            }
            _ => None,
        }
    }

    pub fn push_alert(&mut self, a: Alert) {
        self.alerts.push_front(a);
        self.alerts.truncate(MAX_ALERTS);
    }

    pub fn add_pending(&mut self, m: &HookMsg, tx: oneshot::Sender<Reply>, ttl_ms: u64) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        let key = format!("{}:{}", m.provider, m.session_id);
        let folder = self.sessions.get(&key).map(|s| s.folder.clone()).unwrap_or_default();
        let name = self.sessions.get(&key).map(|s| s.name.clone()).unwrap_or_default();
        let now = now_ms();
        self.pending.insert(
            id,
            Pending {
                id,
                kind: if m.event == "PermissionRequest" { "permission".into() } else { "question".into() },
                provider: m.provider.clone(),
                session_key: key,
                folder,
                name,
                tool_name: m.tool_name.clone(),
                tool_input: m.tool_input.clone(),
                created_ms: now,
                expires_ms: now + ttl_ms,
                held: false,
                tx: Some(tx),
            },
        );
        id
    }

    /// Send a decision. Err when the request already expired or was answered.
    pub fn answer(&mut self, id: u64, reply: Reply) -> Result<(), String> {
        let p = self.pending.remove(&id).ok_or("request expired or already answered")?;
        if now_ms() > p.expires_ms {
            return Err("request expired".into());
        }
        if let Some(s) = self.sessions.get_mut(&p.session_key) {
            s.state = if reply.decision == "deny" { "idle".into() } else { "working".into() };
            s.attention = false;
        }
        p.tx.ok_or("no waiter")?.send(reply).map_err(|_| "hook already gone".to_string())
    }

    pub fn sweep(&mut self) {
        let now = now_ms();
        self.sessions.retain(|_, s| now.saturating_sub(s.last_event_ms) < DROP_MS);
        for s in self.sessions.values_mut() {
            if now.saturating_sub(s.last_event_ms) > STALE_MS && s.state != "waiting" {
                s.state = "stale".into();
            }
        }
        // The pipe countdown owns expiry and moves requests to the terminal list.
        // Removing them here races that handoff and loses the Waiting entry.
    }

    pub fn snapshot(&self, dnd: bool) -> Value {
        let mut pending: Vec<&Pending> = self.pending.values().collect();
        pending.sort_by_key(|p| p.created_ms);
        let wants = |p: &str| self.settings.wants(p);
        json!({
            "settings": self.settings,
            "dnd": dnd,
            "dnd_manual": self.dnd_manual,
            "pipe_error": self.pipe_error,
            "hooks": self.hooks,
            "sessions": self.sessions.values().filter(|s| wants(&s.provider)).collect::<Vec<_>>(),
            "pending": pending.iter().filter(|p| wants(&p.provider)).map(|p| json!({
                "id": p.id,
                "kind": p.kind,
                "questions": if p.kind == "question" { p.tool_input.as_ref().map(|t| t["questions"].clone()) } else { None },
                "provider": p.provider,
                "session_key": p.session_key,
                "folder": p.folder,
                "name": p.name,
                "tool_name": p.tool_name,
                "tool_input": p.tool_input.as_ref().map(|v| serde_json::to_string_pretty(v).unwrap_or_default()),
                "expires_ms": p.expires_ms,
                "held": p.held,
                "created_ms": p.created_ms,
            })).collect::<Vec<_>>(),
            "missed": self.missed.iter().filter(|x| wants(&x.provider)).collect::<Vec<_>>(),
            "alerts": self.alerts,
            "usage": self.usage.values().filter(|u| wants(&u.provider)).collect::<Vec<_>>(),
        })
    }
}

/// Current context size from the transcript tail. Claude: last assistant usage.
/// Codex: last token_count event. Best effort.
pub fn context_tokens(provider: &str, path: &str) -> Option<u64> {
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let start = len.saturating_sub(256 * 1024);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = String::new();
    f.take(256 * 1024).read_to_string(&mut buf).ok()?;
    for line in buf.lines().rev() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if provider == "codex" {
            let p = &v["payload"];
            if p["type"] == "token_count" {
                if let Some(n) = p["info"]["last_token_usage"]["input_tokens"].as_u64() {
                    return Some(n);
                }
            }
        } else if let Some(u) = v["message"].get("usage") {
            let n = ["input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens"]
                .iter()
                .filter_map(|k| u[*k].as_u64())
                .sum::<u64>();
            if n > 0 {
                return Some(n);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(event: &str) -> HookMsg {
        HookMsg {
            provider: "claude".into(),
            event: event.into(),
            session_id: "s1".into(),
            cwd: Some("C:\\work\\proj".into()),
            ..Default::default()
        }
    }

    #[test]
    fn lifecycle() {
        let mut st = Inner::default();
        assert!(st.on_event(&msg("SessionStart")).is_none());
        assert_eq!(st.sessions["claude:s1"].folder, "proj");
        st.on_event(&msg("PreToolUse"));
        assert_eq!(st.sessions["claude:s1"].state, "working");
        let a = st.on_event(&msg("Stop")).unwrap();
        assert_eq!(a.kind, "done");
        st.on_event(&msg("SessionEnd"));
        assert!(st.sessions.is_empty());
    }

    #[test]
    fn answer_routes_once() {
        let mut st = Inner::default();
        let m = msg("PermissionRequest");
        st.on_event(&m);
        let (tx, mut rx) = oneshot::channel();
        let id = st.add_pending(&m, tx, 60_000);
        st.answer(id, Reply { decision: "allow".into(), message: None, answers: None }).unwrap();
        assert_eq!(rx.try_recv().unwrap().decision, "allow");
        assert!(st.answer(id, Reply { decision: "deny".into(), message: None, answers: None }).is_err());
    }

    #[test]
    fn native_question_remains_waiting_until_terminal_answer() {
        let mut st = Inner::default();
        let mut m = msg("PreToolUse");
        m.tool_name = Some("AskUserQuestion".into());
        m.tool_input = Some(json!({"questions":[{"question":"Which option?"}]}));
        assert_eq!(st.on_event(&m).unwrap().kind, "question");
        assert_eq!(st.sessions["claude:s1"].state, "waiting");
        assert!(st.pending.is_empty());
        m.event = "PostToolUse".into();
        st.on_event(&m);
        assert_eq!(st.sessions["claude:s1"].state, "working");
        assert!(!st.sessions["claude:s1"].attention);
    }

    #[test]
    fn terminal_answers_clear_waiting_for_both_providers() {
        for provider in ["claude", "codex"] {
            for event in ["PostToolUse", "PreToolUse", "PreModelRequest", "UserPromptSubmit", "Stop", "Interrupt", "SessionEnd"] {
                let mut st = Inner::default();
                let mut m = msg("PermissionRequest");
                m.provider = provider.into();
                st.on_event(&m);
                let (tx, mut rx) = oneshot::channel();
                let id = st.add_pending(&m, tx, 60_000);
                let key = format!("{provider}:s1");
                st.missed.push(Missed { id: id + 1, kind: "permission".into(), provider: provider.into(),
                    session_key: key.clone(), folder: "proj".into(), name: String::new(), created_ms: now_ms() });
                let mut other = m.clone();
                other.session_id = "other".into();
                let (other_tx, _other_rx) = oneshot::channel();
                let other_id = st.add_pending(&other, other_tx, 60_000);
                m.event = event.into();
                st.on_event(&m);
                assert!(!st.pending.contains_key(&id), "{provider}: {event}");
                assert!(st.pending.contains_key(&other_id));
                assert!(st.missed.is_empty(), "{provider}: {event}");
                assert!(matches!(rx.try_recv(), Err(oneshot::error::TryRecvError::Closed)));
                if event == "Interrupt" { assert_eq!(st.sessions[&key].state, "idle"); }
            }
        }
    }

    #[test]
    fn unrelated_activity_keeps_terminal_requests_visible() {
        let mut st = Inner::default();
        let mut m = msg("PermissionRequest");
        st.on_event(&m);
        let (tx, _rx) = oneshot::channel();
        let id = st.add_pending(&m, tx, 60_000);
        st.missed.push(Missed { id: id + 1, kind: "permission".into(), provider: "claude".into(),
            session_key: "claude:s1".into(), folder: "proj".into(), name: String::new(), created_ms: now_ms() });
        for event in ["SubagentStart", "SubagentStop", "Notification"] {
            m.event = event.into();
            m.notification_type = Some("permission_prompt".into());
            st.on_event(&m);
            assert!(st.pending.contains_key(&id));
            assert_eq!(st.missed.len(), 1);
        }
        m.notification_type = Some("idle_prompt".into());
        st.on_event(&m);
        assert!(st.pending.is_empty());
        assert!(st.missed.is_empty());
    }

    #[test]
    fn sweep_leaves_expiry_to_pipe_handoff() {
        let mut st = Inner::default();
        let (tx, _rx) = oneshot::channel();
        let id = st.add_pending(&msg("PermissionRequest"), tx, 0);
        st.sweep();
        assert!(st.pending.contains_key(&id));
    }

    #[test]
    fn expired_answer_rejected() {
        let mut st = Inner::default();
        let m = msg("PermissionRequest");
        let (tx, _rx) = oneshot::channel();
        let id = st.add_pending(&m, tx, 0);
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert!(st.answer(id, Reply { decision: "allow".into(), message: None, answers: None }).is_err());
    }
}

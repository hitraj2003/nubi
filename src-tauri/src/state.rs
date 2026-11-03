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
    pub transcript_path: Option<String>,
}

pub struct Pending {
    pub id: u64,
    /// "permission" | "question"
    pub kind: String,
    pub provider: String,
    pub session_key: String,
    pub folder: String,
    pub tool_name: Option<String>,
    pub tool_input: Option<Value>,
    pub created_ms: u64,
    pub expires_ms: u64,
    pub tx: Option<oneshot::Sender<Reply>>,
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
    pub fetched_ms: Option<u64>,
}

#[derive(Default)]
pub struct Inner {
    pub settings: Settings,
    pub sessions: BTreeMap<String, Session>,
    pub pending: HashMap<u64, Pending>,
    pub alerts: VecDeque<Alert>,
    pub usage: BTreeMap<String, Usage>,
    pub dnd_manual: bool,
    pub next_id: u64,
    pub pipe_error: Option<String>,
    pub hooks: HashMap<String, bool>,
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
            state: "idle".into(),
            attention: false,
            last_event: String::new(),
            last_event_ms: now,
            subagents: 0,
            context_tokens: None,
            pids: vec![],
            transcript_path: None,
        });
        if m.cwd.is_some() {
            s.cwd = m.cwd.clone();
        }
        s.folder = folder_of(&s.cwd);
        if !m.pids.is_empty() {
            s.pids = m.pids.clone();
        }
        if m.transcript_path.is_some() {
            s.transcript_path = m.transcript_path.clone();
        }
        s.last_event = m.event.clone();
        s.last_event_ms = now;
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
                Some(mk("question", "Question for you", format!("{} · {q}", s.folder)))
            }
            "UserPromptSubmit" | "PreToolUse" | "PostToolUse" => {
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
                Some(mk("permission", "Permission needed", format!("{} · {tool}{note}", s.folder)))
            }
            "Notification" => {
                let nt = m.notification_type.clone().unwrap_or_default();
                let msg = m.message.clone().unwrap_or_default();
                if nt == "idle_prompt" || msg.contains("waiting for your input") {
                    s.state = "idle".into();
                    s.attention = true;
                    Some(mk("idle", "Waiting for you", s.folder.clone()))
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
                Some(mk("done", "Task complete", s.folder.clone()))
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
        let now = now_ms();
        self.pending.insert(
            id,
            Pending {
                id,
                kind: if m.event == "PermissionRequest" { "permission".into() } else { "question".into() },
                provider: m.provider.clone(),
                session_key: key,
                folder,
                tool_name: m.tool_name.clone(),
                tool_input: m.tool_input.clone(),
                created_ms: now,
                expires_ms: now + ttl_ms,
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
        self.pending.retain(|_, p| p.expires_ms > now);
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
                "tool_name": p.tool_name,
                "tool_input": p.tool_input.as_ref().map(|v| serde_json::to_string_pretty(v).unwrap_or_default()),
                "expires_ms": p.expires_ms,
            })).collect::<Vec<_>>(),
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
    fn expired_answer_rejected() {
        let mut st = Inner::default();
        let m = msg("PermissionRequest");
        let (tx, _rx) = oneshot::channel();
        let id = st.add_pending(&m, tx, 0);
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert!(st.answer(id, Reply { decision: "allow".into(), message: None, answers: None }).is_err());
    }
}

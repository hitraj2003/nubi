//! `--hook <provider>` mode: read hook JSON from stdin, forward it to the GUI over the pipe.
//! For PermissionRequest, wait for a decision and print it. Any failure prints nothing
//! and exits 0, so the provider falls back to its own prompt.
use crate::protocol::*;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::mpsc;
use std::time::Duration;

pub fn run(provider: &str) -> i32 {
    let provider = if provider == "codex" { "codex" } else { "claude" };
    let mut buf = Vec::new();
    let _ = std::io::stdin().take(MAX_STDIN as u64 + 1).read_to_end(&mut buf);
    if buf.len() > MAX_STDIN {
        return 0;
    }
    let Ok(raw) = serde_json::from_slice::<serde_json::Value>(&buf) else { return 0 };
    let Some(msg) = build_msg(provider, &raw, crate::winutil::ancestors()) else { return 0 };
    let question = is_question(provider, &msg.event, msg.tool_name.as_deref());
    let tool_input = raw.get("tool_input").cloned().unwrap_or_default();
    let wants = msg.wants_reply;
    let wait = if wants { msg.deadline_secs } else { 3 };
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(exchange(&msg));
    });
    if let Ok(Some(reply)) = rx.recv_timeout(Duration::from_secs(wait)) {
        if wants {
            let out = if question { question_json(&tool_input, &reply) } else { decision_json(&reply) };
            if let Some(out) = out {
                let mut so = std::io::stdout();
                let _ = so.write_all(out.as_bytes());
                let _ = so.flush();
            }
        }
    }
    0
}

fn exchange(msg: &HookMsg) -> Option<Reply> {
    let name = pipe_name();
    let mut pipe = None;
    for _ in 0..20 {
        match OpenOptions::new().read(true).write(true).open(&name) {
            Ok(f) => {
                pipe = Some(f);
                break;
            }
            // 231 = all pipe instances busy; anything else (2 = GUI not running) gives up.
            Err(e) if e.raw_os_error() == Some(231) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => {
                if std::env::var_os("CLAUDE_OVERLAY_DEBUG").is_some() {
                    eprintln!("[claude-overlay hook] open {name}: {e}");
                }
                return None;
            }
        }
    }
    let mut f = pipe?;
    let mut line = serde_json::to_string(msg).ok()?;
    line.push('\n');
    f.write_all(line.as_bytes()).ok()?;
    f.flush().ok()?;
    if !msg.wants_reply {
        return None;
    }
    let mut resp = String::new();
    BufReader::new(f).take(4096).read_line(&mut resp).ok()?;
    serde_json::from_str(resp.trim()).ok()
}

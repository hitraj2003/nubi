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

/// Connect to the GUI's pipe; None when the GUI is not running or stays busy.
pub fn open_pipe() -> Option<std::fs::File> {
    let name = pipe_name();
    for _ in 0..20 {
        match OpenOptions::new().read(true).write(true).open(&name) {
            Ok(f) => return Some(f),
            // 231 = all pipe instances busy; anything else (2 = GUI not running) gives up.
            Err(e) if e.raw_os_error() == Some(231) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => {
                if std::env::var_os("NUBI_DEBUG").is_some() {
                    eprintln!("[nubi hook] open {name}: {e}");
                }
                return None;
            }
        }
    }
    None
}

fn exchange(msg: &HookMsg) -> Option<Reply> {
    let mut f = open_pipe()?;
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

/// `--statusline` mode: Claude Code's status line command. Claude Code passes a JSON blob on
/// stdin that includes `rate_limits` (5-hour and weekly usage) for subscribers. We forward that
/// to the GUI (no network call, so no rate limit), then run the user's previous status line
/// command with the same stdin and print its output, so their status line looks as before.
pub fn statusline() -> i32 {
    let mut buf = Vec::new();
    let _ = std::io::stdin().take(MAX_STDIN as u64 + 1).read_to_end(&mut buf);
    if let Ok(raw) = serde_json::from_slice::<serde_json::Value>(&buf) {
        if let Some(rl) = raw.get("rate_limits").filter(|v| v.is_object()) {
            let msg = HookMsg {
                v: 1,
                provider: "claude".into(),
                event: "StatusLine".into(),
                session_id: raw.get("session_id").and_then(|s| s.as_str()).unwrap_or_default().into(),
                tool_input: Some(rl.clone()),
                ..Default::default()
            };
            // Never hold up the status line: give the GUI 300 ms at most.
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = exchange(&msg);
                let _ = tx.send(());
            });
            let _ = rx.recv_timeout(Duration::from_millis(300));
        }
    }
    if let Some(cmd) = crate::hooks_config::previous_statusline() {
        if let Some(out) = run_shell(&cmd, &buf) {
            let mut so = std::io::stdout();
            let _ = so.write_all(&out);
            let _ = so.flush();
        }
    }
    0
}

/// Run a status line command the way Claude Code would: Git Bash when present, else cmd.
fn run_shell(cmd: &str, stdin: &[u8]) -> Option<Vec<u8>> {
    use std::process::{Command, Stdio};
    let bash = std::env::var_os("CLAUDE_CODE_GIT_BASH_PATH")
        .map(std::path::PathBuf::from)
        .filter(|p| p.exists())
        .or_else(|| {
            ["C:/Program Files/Git/bin/bash.exe", "C:/Program Files (x86)/Git/bin/bash.exe"]
                .iter()
                .map(std::path::PathBuf::from)
                .find(|p| p.exists())
        });
    let mut c = match bash {
        Some(b) => {
            let mut c = Command::new(b);
            c.arg("-c").arg(cmd);
            c
        }
        None => {
            use std::os::windows::process::CommandExt;
            let mut c = Command::new("cmd");
            c.arg("/C").raw_arg(cmd);
            c
        }
    };
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    if let Some(mut si) = child.stdin.take() {
        let _ = si.write_all(stdin);
    }
    child.wait_with_output().ok().map(|o| o.stdout)
}

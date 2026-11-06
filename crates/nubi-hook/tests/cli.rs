//! Exercise the real executable against isolated pipes, never the production GUI.
use nubi_core::protocol::{decision_json, question_json, HookMsg, Reply, MAX_STDIN};
use serde_json::{json, Value};
use std::io::Write;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    user: String,
    dir: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let user = format!(
            "NubiHookTest{}x{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let dir = std::env::temp_dir().join(&user);
        std::fs::create_dir_all(dir.join("nubi")).unwrap();
        Self { user, dir }
    }

    fn pipe(&self) -> NamedPipeServer {
        ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .max_instances(1)
            .in_buffer_size(4096)
            .create(format!(r"\\.\pipe\nubi-{}", self.user))
            .unwrap()
    }

    fn launch(&self, args: &[&str], input: &[u8]) -> Child {
        let mut child = Command::new(env!("CARGO_BIN_EXE_nubi-hook"))
            .args(args)
            .env("USERNAME", &self.user)
            .env("NUBI_SIM_DATA_DIR", self.dir.join("nubi"))
            .creation_flags(0x0800_0000)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        child
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Only this fixture's uniquely named directory is created or removed.
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn output(mut child: Child, deadline: Duration) -> Output {
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("Hook did not honor its deadline");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    out
}

async fn exchange(input: &Value, provider: &str, reply: Option<&Reply>) -> (HookMsg, Vec<u8>) {
    let fixture = Fixture::new();
    let pipe = fixture.pipe();
    let child = fixture.launch(&["--hook", provider], &serde_json::to_vec(input).unwrap());
    tokio::time::timeout(Duration::from_secs(5), pipe.connect())
        .await
        .unwrap()
        .unwrap();
    let (read, mut write) = tokio::io::split(pipe);
    let mut line = String::new();
    tokio::time::timeout(
        Duration::from_secs(5),
        BufReader::new(read).take(512 * 1024).read_line(&mut line),
    )
    .await
    .unwrap()
    .unwrap();
    let message: HookMsg = serde_json::from_str(&line).unwrap();
    if let Some(reply) = reply {
        let mut line = serde_json::to_vec(reply).unwrap();
        line.push(b'\n');
        write.write_all(&line).await.unwrap();
        write.flush().await.unwrap();
    }
    let out = output(child, Duration::from_secs(5)).await;
    (message, out.stdout)
}

#[tokio::test]
async fn malformed_oversized_and_offline_calls_fail_open() {
    let fixture = Fixture::new();
    let permission = br#"{"hook_event_name":"PermissionRequest","session_id":"offline"}"#;
    for input in [
        b"invalid".as_slice(),
        b"{}",
        permission,
        &vec![b' '; MAX_STDIN + 1],
    ] {
        let out = output(
            fixture.launch(&["--hook", "claude"], input),
            Duration::from_secs(5),
        )
        .await;
        assert!(out.stdout.is_empty());
    }
    let out = output(
        fixture.launch(&["--unsupported"], b""),
        Duration::from_secs(2),
    )
    .await;
    assert!(out.stdout.is_empty());
}

#[tokio::test]
async fn permissions_preserve_replies_ancestry_and_deadline() {
    for provider in ["claude", "codex"] {
        for decision in ["allow", "deny"] {
            let reply = Reply {
                decision: decision.into(),
                message: Some("test decision".into()),
                answers: None,
            };
            let (message, stdout) = exchange(&json!({
                "hook_event_name": "PermissionRequest", "session_id": "session", "tool_name": "Bash",
                "tool_input": { "command": "echo test" }
            }), provider, Some(&reply)).await;
            assert_eq!(message.provider, provider);
            assert!(message.wants_reply);
            assert_eq!(message.deadline_secs, 100);
            assert!(message.pids.contains(&std::process::id()));
            assert_eq!(
                String::from_utf8(stdout).unwrap(),
                decision_json(&reply).unwrap()
            );
        }
    }
}

#[tokio::test]
async fn questions_preserve_answers_and_provider_routing() {
    let tool_input = json!({"questions": [{"question": "Proceed?", "header": "Choice", "options": [{"label": "Yes", "description": "Continue"}], "multiSelect": false}]});
    let input = json!({"hook_event_name": "PreToolUse", "session_id": "question", "tool_name": "AskUserQuestion", "tool_input": tool_input});
    let reply = Reply {
        decision: "answer".into(),
        message: None,
        answers: Some(serde_json::from_value(json!({"Proceed?": "Yes"})).unwrap()),
    };
    let (message, stdout) = exchange(&input, "claude", Some(&reply)).await;
    assert!(message.wants_reply);
    assert_eq!(message.deadline_secs, 240);
    assert_eq!(
        String::from_utf8(stdout).unwrap(),
        question_json(&tool_input, &reply).unwrap()
    );
    let (message, stdout) = exchange(&input, "codex", None).await;
    assert!(!message.wants_reply);
    assert!(stdout.is_empty());
}

#[tokio::test]
#[cfg(debug_assertions)]
async fn statusline_preserves_previous_command_stdin_and_stdout() {
    let fixture = Fixture::new();
    let command =
        r#"powershell.exe -NoProfile -NonInteractive -Command "[Console]::In.ReadToEnd()""#;
    std::fs::write(
        fixture.dir.join("nubi/statusline_prev.json"),
        json!({"command": command}).to_string(),
    )
    .unwrap();
    let input =
        br#"{"session_id":"statusline","rate_limits":{"five_hour":{"used_percentage":25}}}"#;
    let out = output(
        fixture.launch(&["--statusline"], input),
        Duration::from_secs(10),
    )
    .await;
    assert_eq!(
        String::from_utf8(out.stdout).unwrap().trim(),
        std::str::from_utf8(input).unwrap()
    );
}

#[tokio::test]
#[cfg(debug_assertions)]
async fn blocked_statusline_pipe_keeps_300ms_deadline() {
    let fixture = Fixture::new();
    let pipe = fixture.pipe();
    let input =
        json!({"session_id": "statusline", "rate_limits": {"blocked": "x".repeat(256 * 1024)}});
    let start = Instant::now();
    let child = fixture.launch(&["--statusline"], &serde_json::to_vec(&input).unwrap());
    tokio::time::timeout(Duration::from_secs(5), pipe.connect())
        .await
        .unwrap()
        .unwrap();
    // Hold the connection without reading, so the worker blocks writing its frame.
    let out = output(child, Duration::from_secs(2)).await;
    assert!(out.stdout.is_empty());
    assert!(start.elapsed() >= Duration::from_millis(250));
    assert!(start.elapsed() < Duration::from_secs(2));
    drop(pipe);
}

#[tokio::test]
async fn blocked_notification_pipe_keeps_three_second_deadline() {
    let fixture = Fixture::new();
    let pipe = fixture.pipe();
    let input = json!({"hook_event_name": "Stop", "session_id": "notification", "cwd": "x".repeat(256 * 1024)});
    let start = Instant::now();
    let child = fixture.launch(&["--hook", "claude"], &serde_json::to_vec(&input).unwrap());
    tokio::time::timeout(Duration::from_secs(5), pipe.connect())
        .await
        .unwrap()
        .unwrap();
    let out = output(child, Duration::from_secs(5)).await;
    assert!(out.stdout.is_empty());
    assert!(start.elapsed() >= Duration::from_millis(2900));
    assert!(start.elapsed() < Duration::from_secs(5));
    drop(pipe);
}

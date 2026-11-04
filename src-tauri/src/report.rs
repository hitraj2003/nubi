//! About page: agent-drafted bug reports and the release list (version history).
use crate::app::{with, App};
use serde_json::{json, Map, Value};
use std::io::Write;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::Duration;

const REPO: &str = "hitraj2003/nubi";
const NO_WINDOW: u32 = 0x0800_0000;
/// GitHub drops very long "new issue" URLs; stay under this.
const MAX_URL: usize = 7_500;

fn run(cmd: &str) -> String {
    Command::new("cmd")
        .args(["/C", cmd])
        .creation_flags(NO_WINDOW)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn enc(t: &str) -> String {
    t.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Facts the report form needs. Never tokens or file contents.
#[tauri::command]
pub async fn report_context(app: App) -> Value {
    let ver = app.package_info().version.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let (s, hooks, usage) = with(&app, |i| {
            let usage: Vec<String> = i.usage.values().map(|u| format!("{}: {}", u.provider, u.status)).collect();
            (i.settings.clone(), i.hooks.clone(), usage)
        });
        let mut hooks: Vec<String> = hooks.iter().map(|(k, v)| format!("{k}: {}", if *v { "on" } else { "off" })).collect();
        hooks.sort();
        let agents = [("Claude Code", run("claude --version")), ("Codex", run("codex --version"))]
            .into_iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(n, v)| format!("{n} {v}"))
            .collect::<Vec<_>>()
            .join(", ");
        let setup = format!(
            "Display mode: {}, edge: {}\nProviders: {}\nQuestions: {}\nHooks: {}\nUsage: {}",
            s.display_mode,
            s.edge,
            s.providers,
            s.questions_mode,
            hooks.join(", "),
            usage.join(", ")
        );
        json!({ "nubi-version": ver, "windows": run("ver"), "agent-versions": agents, "setup": setup })
    })
    .await
    .unwrap_or(Value::Null)
}

/// Ask the local Claude or Codex CLI to turn the user's sentence into the bug form fields.
/// Runs in an empty temp folder with no tools, so the agent sees only the prompt.
#[tauri::command]
pub async fn draft_report(agent: String, text: String, context: Value) -> Result<Value, String> {
    if agent != "claude" && agent != "codex" {
        return Err("agent must be claude or codex".into());
    }
    tauri::async_runtime::spawn_blocking(move || draft(&agent, &text, &context))
        .await
        .map_err(|e| e.to_string())?
}

fn draft(agent: &str, text: &str, context: &Value) -> Result<Value, String> {
    let prompt = format!(
        "You fill in a bug report form for Nubi, a Windows app that shows Claude Code and Codex \
         sessions, approvals and usage in a notch, floating ring or taskbar strip.\n\
         User's description:\n{text}\n\nApp facts:\n{context}\n\n\
         Reply with only one JSON object, no code fence, with these string keys:\n\
         \"what-happened\", \"expected\", \"steps\" (numbered lines), \
         \"area\" (one of: Notch; Toast (notification pill and request cards); Floating ring or its tooltip; \
         Taskbar strip; Settings panel; Approvals (Allow / Deny); Claude questions; Usage numbers; Sounds; \
         Updates or installer; Hooks or status line in Claude Code / Codex; Other), \
         \"provider\" (Claude Code, Codex, Both or Not related to an agent), \
         \"frequency\" (Every time, Sometimes or Once). Write plain, short sentences. \
         Do not invent details the user did not give; write \"Not given\" instead."
    );
    let dir = std::env::temp_dir().join(format!("nubi-report-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let out_file = dir.join("out.txt");
    let cmd = if agent == "claude" {
        "claude -p --tools \"\" --no-session-persistence".to_string()
    } else {
        format!("codex exec --ephemeral --skip-git-repo-check -o \"{}\" -", out_file.display())
    };
    let mut child = Command::new("cmd")
        .args(["/C", &cmd])
        .current_dir(&dir)
        .creation_flags(NO_WINDOW)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start {agent}: {e}"))?;
    {
        let mut stdin = child.stdin.take().ok_or("no stdin")?;
        stdin.write_all(prompt.as_bytes()).map_err(|e| e.to_string())?;
    } // stdin closes here so the CLI starts.
    // Read output on threads so a full pipe never blocks the child; wait at most 2 minutes.
    let mut so = child.stdout.take().ok_or("no stdout")?;
    let mut se = child.stderr.take().ok_or("no stderr")?;
    let t_out = std::thread::spawn(move || { let mut b = Vec::new(); let _ = std::io::Read::read_to_end(&mut so, &mut b); b });
    let t_err = std::thread::spawn(move || { let mut b = Vec::new(); let _ = std::io::Read::read_to_end(&mut se, &mut b); b });
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if started.elapsed() > Duration::from_secs(120) => {
                let _ = child.kill();
                let _ = std::fs::remove_dir_all(&dir);
                return Err(format!("{agent} took too long"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let stdout = t_out.join().unwrap_or_default();
    let stderr = t_err.join().unwrap_or_default();
    let text = if agent == "codex" {
        std::fs::read_to_string(&out_file).unwrap_or_default()
    } else {
        String::from_utf8_lossy(&stdout).to_string()
    };
    let _ = std::fs::remove_dir_all(&dir);
    if !status.success() && text.trim().is_empty() {
        let err = String::from_utf8_lossy(&stderr);
        return Err(format!("{agent} failed: {}", err.lines().last().unwrap_or("no output")));
    }
    let (Some(a), Some(b)) = (text.find('{'), text.rfind('}')) else {
        return Err(format!("{agent} did not return the form"));
    };
    let v: Value = serde_json::from_str(&text[a..=b]).map_err(|_| format!("{agent} did not return the form"))?;
    let mut m = Map::new();
    for k in ["what-happened", "expected", "steps", "area", "provider", "frequency"] {
        m.insert(k.into(), Value::String(v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()));
    }
    Ok(Value::Object(m))
}

/// Open the GitHub bug form with every field filled in. Long text fields are cut to fit.
#[tauri::command]
pub fn open_report(fields: Map<String, Value>) {
    let mut url = format!("https://github.com/{REPO}/issues/new?template=bug_report.yml");
    // Short fields first so cutting never drops them.
    let order = ["nubi-version", "windows", "agent-versions", "area", "provider", "frequency", "what-happened", "expected", "steps", "setup"];
    for k in order {
        let Some(v) = fields.get(k).and_then(|v| v.as_str()) else { continue };
        if v.trim().is_empty() {
            continue;
        }
        let mut val = v.to_string();
        loop {
            let part = format!("&{k}={}", enc(&val));
            if url.len() + part.len() <= MAX_URL {
                url.push_str(&part);
                break;
            }
            if val.len() < 40 {
                break;
            }
            let mut cut = val.len() * 3 / 4;
            while !val.is_char_boundary(cut) {
                cut -= 1;
            }
            val.truncate(cut);
            val.push('…');
        }
    }
    let _ = Command::new("explorer").arg(url).spawn();
}

/// Every published release, newest first, with its Windows installer.
#[tauri::command]
pub async fn list_releases(app: App) -> Result<Value, String> {
    let current = app.package_info().version.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let r = ureq::get(&format!("https://api.github.com/repos/{REPO}/releases?per_page=50"))
            .set("User-Agent", "Nubi")
            .set("Accept", "application/vnd.github+json")
            .timeout(Duration::from_secs(15))
            .call()
            .map_err(|e| format!("could not load releases: {e}"))?;
        let list: Vec<Value> = r.into_json().map_err(|e| e.to_string())?;
        let out: Vec<Value> = list
            .iter()
            .filter(|r| !r["draft"].as_bool().unwrap_or(false))
            .map(|r| {
                let ver = r["tag_name"].as_str().unwrap_or("").trim_start_matches('v').to_string();
                let asset = r["assets"]
                    .as_array()
                    .and_then(|a| a.iter().find(|x| x["name"].as_str().is_some_and(|n| n.ends_with("-setup.exe"))));
                json!({
                    "version": ver,
                    "date": r["published_at"].as_str().unwrap_or("").get(..10).unwrap_or(""),
                    "prerelease": r["prerelease"].as_bool().unwrap_or(false),
                    "current": ver == current,
                    "notes": r["html_url"],
                    "download": asset.map(|a| a["browser_download_url"].clone()),
                })
            })
            .collect();
        Ok(Value::Array(out))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Open a release page or installer link in the browser. Only this repo's GitHub links.
#[tauri::command]
pub fn open_release_link(url: String) -> Result<(), String> {
    if !url.starts_with(&format!("https://github.com/{REPO}/releases/")) {
        return Err("not a Nubi release link".into());
    }
    Command::new("explorer").arg(url).spawn().map(|_| ()).map_err(|e| e.to_string())
}

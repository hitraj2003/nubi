//! Named-pipe server. One instance per connection; each connection carries one hook message.
//! Security: per-user pipe name, DACL for the current user + SYSTEM only, remote clients
//! rejected, first instance flag against squatting, client exe path must be ours.
use crate::app::{self, App};
use crate::protocol::*;
use std::ffi::c_void;
use std::os::windows::io::AsRawHandle;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::{NamedPipeServer, PipeMode, ServerOptions};
use tokio::sync::oneshot;
use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{GetTokenInformation, TokenUser, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER};
use windows_sys::Win32::System::Pipes::GetNamedPipeClientProcessId;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn current_user_sid() -> Option<String> {
    unsafe {
        let mut tok: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut tok) == 0 {
            return None;
        }
        let mut len = 0u32;
        GetTokenInformation(tok, TokenUser, std::ptr::null_mut(), 0, &mut len);
        let mut buf = vec![0u8; len as usize];
        let ok = GetTokenInformation(tok, TokenUser, buf.as_mut_ptr() as *mut c_void, len, &mut len);
        CloseHandle(tok);
        if ok == 0 {
            return None;
        }
        let tu = &*(buf.as_ptr() as *const TOKEN_USER);
        let mut p: *mut u16 = std::ptr::null_mut();
        if ConvertSidToStringSidW(tu.User.Sid, &mut p) == 0 {
            return None;
        }
        let mut n = 0;
        while *p.add(n) != 0 {
            n += 1;
        }
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, n));
        LocalFree(p as *mut c_void);
        Some(s)
    }
}

/// Security descriptor kept for the process lifetime (address as usize so futures stay Send).
fn security_descriptor() -> Result<usize, String> {
    let sid = current_user_sid().ok_or("cannot read current user SID")?;
    let sddl = wide(&format!("D:P(A;;GA;;;{sid})(A;;GA;;;SY)"));
    let mut sd: *mut c_void = std::ptr::null_mut();
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut sd, std::ptr::null_mut())
    };
    if ok == 0 {
        return Err("cannot build pipe security descriptor".into());
    }
    Ok(sd as usize)
}

fn create(name: &str, sd: usize, first: bool) -> std::io::Result<NamedPipeServer> {
    let mut sa = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd as *mut c_void,
        bInheritHandle: 0,
    };
    unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .pipe_mode(PipeMode::Byte)
            .max_instances(64)
            .create_with_security_attributes_raw(name, &mut sa as *mut _ as *mut c_void)
    }
}

/// Paths a legitimate client can run from: this exe or the hook shim.
fn client_allowed(pipe: &NamedPipeServer) -> bool {
    let mut pid = 0u32;
    if unsafe { GetNamedPipeClientProcessId(pipe.as_raw_handle() as HANDLE, &mut pid) } == 0 {
        return false;
    }
    let Some(img) = crate::winutil::process_image(pid) else { return false };
    let norm = |p: std::path::PathBuf| {
        std::fs::canonicalize(&p).unwrap_or(p).display().to_string().to_lowercase()
    };
    let img = norm(img.into());
    let mut allowed = vec![];
    if let Ok(p) = std::env::current_exe() {
        allowed.push(norm(p));
    }
    if let Some(p) = crate::hooks_config::shim_path() {
        allowed.push(norm(p));
    }
    let ok = allowed.iter().any(|a| *a == img);
    if !ok {
        eprintln!("[nubi] rejected pipe client: {img}");
    }
    ok
}

pub async fn serve(app: App) {
    let name = pipe_name();
    let sd = match security_descriptor() {
        Ok(sd) => sd,
        Err(e) => return app::set_pipe_error(&app, e),
    };
    let mut first = true;
    loop {
        let server = match create(&name, sd, first) {
            Ok(s) => s,
            Err(e) => {
                let msg = if first {
                    format!("pipe {name} already in use (another copy running?): {e}")
                } else {
                    format!("pipe create failed: {e}")
                };
                return app::set_pipe_error(&app, msg);
            }
        };
        first = false;
        if server.connect().await.is_err() {
            continue;
        }
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move { handle(app2, server).await });
    }
}

async fn handle(app: App, pipe: NamedPipeServer) {
    if !client_allowed(&pipe) {
        return;
    }
    let (rd, mut wr) = tokio::io::split(pipe);
    let mut reader = BufReader::new(rd);
    let mut buf = Vec::new();
    let read = tokio::time::timeout(
        Duration::from_secs(5),
        (&mut reader).take(MAX_FRAME as u64).read_until(b'\n', &mut buf),
    )
    .await;
    if !matches!(read, Ok(Ok(n)) if n > 0) {
        eprintln!("[nubi] read failed: {read:?}");
        return;
    }
    let msg = match serde_json::from_slice::<HookMsg>(&buf) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[nubi] bad frame: {e}");
            return;
        }
    };
    if msg.provider != "claude" && msg.provider != "codex" {
        return;
    }
    app::on_hook(&app, &msg);
    let question = is_question(&msg.provider, &msg.event, msg.tool_name.as_deref());
    if !(msg.wants_reply && (msg.event == "PermissionRequest" || question)) {
        return;
    }
    if question && app::questions_mode(&app) == "off" {
        return; // connection closes, the hook prints nothing, Claude asks in the terminal
    }
    let cap = if question { QUESTION_DEADLINE_SECS } else { PERMISSION_DEADLINE_SECS };
    // The user's takeover time, kept inside the hook's own deadline.
    let takeover = app::takeover_secs(&app).max(3);
    let wait = Duration::from_secs(takeover.min(msg.deadline_secs.min(cap).saturating_sub(5)));
    let (tx, rx) = oneshot::channel();
    let id = app::add_pending(&app, &msg, tx, wait.as_millis() as u64);
    let mut probe = [0u8; 1];
    let reply = tokio::select! {
        r = rx => r.ok(),
        _ = tokio::time::sleep(wait) => None,
        _ = reader.read(&mut probe) => None, // provider moved on
    };
    app::remove_pending(&app, id);
    if let Some(rep) = reply {
        if let Ok(mut line) = serde_json::to_string(&rep) {
            line.push('\n');
            let _ = wr.write_all(line.as_bytes()).await;
            let _ = wr.flush().await;
        }
    }
}

/// Debug builds only: a test inbox. Each `<name>.json` file dropped into the folder named by
/// `NUBI_SIM_DIR` is read as `{"provider": "...", "hook": {hook JSON}}` and handled
/// like a hook call; for requests, the answer is written to `<name>.reply`. Lets a sandboxed
/// test script drive the UI without the named pipe. Not compiled into release builds.
#[cfg(debug_assertions)]
pub fn start_sim(app: App) {
    let Some(dir) = std::env::var_os("NUBI_SIM_DIR").map(std::path::PathBuf::from) else { return };
    let _ = std::fs::create_dir_all(&dir);
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(150)).await;
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) != Some("json") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&p) else { continue };
                let _ = std::fs::remove_file(&p);
                let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
                let provider = v["provider"].as_str().unwrap_or("claude").to_string();
                let Some(msg) = build_msg(&provider, &v["hook"], vec![]) else { continue };
                let app = app.clone();
                let out = p.with_extension("reply");
                tauri::async_runtime::spawn(async move {
                    app::on_hook(&app, &msg);
                    let question = is_question(&msg.provider, &msg.event, msg.tool_name.as_deref());
                    if !msg.wants_reply || (question && app::questions_mode(&app) == "off") {
                        return;
                    }
                    let wait = Duration::from_secs(app::takeover_secs(&app).max(3));
                    let (tx, rx) = oneshot::channel();
                    let id = app::add_pending(&app, &msg, tx, wait.as_millis() as u64);
                    let reply = tokio::select! { r = rx => r.ok(), _ = tokio::time::sleep(wait) => None };
                    app::remove_pending(&app, id);
                    let text = reply.and_then(|r| serde_json::to_string(&r).ok()).unwrap_or_else(|| "timeout".into());
                    let _ = std::fs::write(out, text);
                });
            }
        }
    });
}

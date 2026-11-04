//! Launcher hotkey: open Claude or Codex in the folder of the focused File Explorer window.
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow};

const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Folder chosen when the hotkey fired; launcher_run reads it.
static TARGET: Mutex<Option<String>> = Mutex::new(None);

fn class_of(hwnd: windows_sys::Win32::Foundation::HWND) -> String {
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Asks the Shell for the folder shown in the Explorer window `hwnd`.
/// With several Explorer tabs in one window, the Shell reports the first tab it lists.
fn explorer_folder(hwnd: isize) -> Option<String> {
    let script = format!(
        "[Console]::OutputEncoding=[Text.Encoding]::UTF8;\
         foreach($w in (New-Object -ComObject Shell.Application).Windows()){{\
         if($w.HWND -eq {hwnd}){{$w.Document.Folder.Self.Path;break}}}}"
    );
    let ps = std::path::PathBuf::from(std::env::var_os("SystemRoot")?)
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let out = Command::new(ps)
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    std::path::Path::new(&path).is_dir().then_some(path)
}

fn focused_folder() -> Result<String, String> {
    let hwnd = unsafe { GetForegroundWindow() };
    match class_of(hwnd).as_str() {
        "CabinetWClass" | "ExploreWClass" => explorer_folder(hwnd as isize)
            .ok_or_else(|| "This Explorer window does not show a file system folder.".into()),
        "Progman" | "WorkerW" => dirs::desktop_dir()
            .map(|d| d.to_string_lossy().into_owned())
            .ok_or_else(|| "The desktop folder is unavailable.".into()),
        _ => Err("Focus a File Explorer window first.".into()),
    }
}

/// Hotkey handler. Reads the foreground window before the launcher takes focus.
pub fn trigger(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let found = focused_folder();
        *TARGET.lock().unwrap() = found.as_ref().ok().cloned();
        let payload = match &found {
            Ok(dir) => serde_json::json!({ "dir": dir }),
            Err(e) => serde_json::json!({ "error": e }),
        };
        if let Some(w) = app.get_webview_window("launcher") {
            let _ = w.emit("launcher", payload);
            let _ = w.center();
            let _ = w.show();
            let _ = w.set_always_on_top(true);
            let _ = w.set_focus();
        }
    });
}

fn hide(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("launcher") {
        let _ = w.hide();
    }
}

#[tauri::command]
pub fn launcher_run(app: AppHandle, cli: String) -> Result<(), String> {
    if cli != "claude" && cli != "codex" {
        return Err("Unknown CLI.".into());
    }
    let dir = TARGET.lock().unwrap().take().ok_or("No folder selected.")?;
    // Hide only on success so an error stays readable in the card.
    crate::session_actions::open_terminal(&dir, Some(&cli))?;
    hide(&app);
    Ok(())
}

#[tauri::command]
pub fn launcher_cancel(app: AppHandle) {
    TARGET.lock().unwrap().take();
    hide(&app);
}

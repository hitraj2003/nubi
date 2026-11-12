use std::os::windows::process::CommandExt;
use std::process::Command;
use windows_sys::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow};

const CREATE_NO_WINDOW: u32 = 0x08000000;

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
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    std::path::Path::new(&path).is_dir().then_some(path)
}

pub fn focused_folder() -> Result<String, String> {
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

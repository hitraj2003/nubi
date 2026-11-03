//! Small Win32 helpers: process tree, window focus, notification state, beeps.
use std::collections::HashMap;
use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HWND, INVALID_HANDLE_VALUE, LPARAM};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
use windows_sys::Win32::UI::Shell::SHQueryUserNotificationState;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindow, GetWindowTextLengthW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    SetForegroundWindow, ShowWindow, GW_OWNER, MB_ICONASTERISK, MB_ICONEXCLAMATION, SW_RESTORE,
};

/// Parent chain of the current process, nearest first (excludes self).
pub fn ancestors() -> Vec<u32> {
    let mut parent: HashMap<u32, u32> = HashMap::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return vec![];
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snap, &mut e) != 0 {
            loop {
                parent.insert(e.th32ProcessID, e.th32ParentProcessID);
                if Process32NextW(snap, &mut e) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
    }
    let mut out = vec![];
    let mut cur = unsafe { GetCurrentProcessId() };
    for _ in 0..16 {
        match parent.get(&cur) {
            Some(&p) if p != 0 && !out.contains(&p) => {
                out.push(p);
                cur = p;
            }
            _ => break,
        }
    }
    out
}

pub fn process_image(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        if ok == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

unsafe extern "system" fn collect(hwnd: HWND, lp: LPARAM) -> BOOL {
    let list = &mut *(lp as *mut Vec<(HWND, u32)>);
    if IsWindowVisible(hwnd) != 0 && GetWindow(hwnd, GW_OWNER).is_null() && GetWindowTextLengthW(hwnd) > 0 {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        list.push((hwnd, pid));
    }
    1
}

/// Bring the nearest ancestor window to the front. Best effort: whole window, not tab.
pub fn focus_pids(pids: &[u32]) -> bool {
    let mut list: Vec<(HWND, u32)> = vec![];
    unsafe {
        EnumWindows(Some(collect), &mut list as *mut _ as LPARAM);
    }
    for pid in pids {
        if let Some((hwnd, _)) = list.iter().find(|(_, p)| p == pid) {
            unsafe {
                if IsIconic(*hwnd) != 0 {
                    ShowWindow(*hwnd, SW_RESTORE);
                }
                return SetForegroundWindow(*hwnd) != 0;
            }
        }
    }
    false
}

/// QUERY_USER_NOTIFICATION_STATE, or 0 when unknown.
fn notification_state() -> i32 {
    let mut st: i32 = 0;
    let hr = unsafe { SHQueryUserNotificationState(&mut st) };
    if hr < 0 {
        0
    } else {
        st
    }
}

/// Full-screen app, presentation mode or busy (QUNS_BUSY / RUNNING_D3D_FULL_SCREEN / PRESENTATION_MODE).
pub fn fullscreen_active() -> bool {
    matches!(notification_state(), 2 | 3 | 4)
}

/// QUNS_QUIET_TIME. Focus Assist has no public API; this is the nearest documented signal.
pub fn quiet_time_active() -> bool {
    notification_state() == 6
}

pub fn beep(kind: &str) {
    let t = if kind == "permission" { MB_ICONEXCLAMATION } else { MB_ICONASTERISK };
    unsafe {
        MessageBeep(t);
    }
}

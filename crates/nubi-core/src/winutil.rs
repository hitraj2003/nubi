//! Small Win32 helpers: process tree, window focus, notification state, beeps.
use std::collections::HashMap;
use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HWND, INVALID_HANDLE_VALUE, LPARAM};
use windows_sys::Win32::System::Console::GetConsoleWindow;
use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Shell::SHQueryUserNotificationState;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindow, GetWindowTextLengthW, GetWindowThreadProcessId,
    IsIconic, IsWindow, IsWindowVisible, SetForegroundWindow, ShowWindow, GW_OWNER,
    MB_ICONASTERISK, MB_ICONEXCLAMATION, SW_RESTORE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetAncestor, GA_ROOTOWNER};

/// Session window candidates: visible console owner first, then parents nearest first.
/// Excludes self and the desktop shell; a hidden console owner is a final fallback.
pub fn ancestors() -> Vec<u32> {
    let mut parent: HashMap<u32, u32> = HashMap::new();
    // explorer.exe: the shell. Its windows are File Explorer folders, never the terminal.
    let mut shell: Vec<u32> = vec![];
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
                let len = e
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(e.szExeFile.len());
                if String::from_utf16_lossy(&e.szExeFile[..len])
                    .eq_ignore_ascii_case("explorer.exe")
                {
                    shell.push(e.th32ProcessID);
                }
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
            Some(&p) if p != 0 && !out.contains(&p) && !shell.contains(&p) => {
                out.push(p);
                cur = p;
            }
            _ => break,
        }
    }
    // A new console can inherit ancestors from an unrelated terminal. Prefer its visible
    // root owner. Pseudoconsole message windows are hidden; keep ancestor fallback for
    // Windows Terminal and integrated terminals instead of treating them as visible hosts.
    unsafe {
        let con = GetConsoleWindow();
        if !con.is_null() {
            let top = GetAncestor(con, GA_ROOTOWNER);
            let top = if top.is_null() { con } else { top };
            let mut pid = 0u32;
            GetWindowThreadProcessId(top, &mut pid);
            if !shell.contains(&pid) {
                add_console_host(&mut out, pid, IsWindowVisible(top) != 0);
            }
        }
    }
    out
}

/// Visible top-level window that owns this process's console, or 0 when hidden or absent.
/// Windows Terminal owns its pseudoconsole windows, so this names the exact terminal window
/// even when one WindowsTerminal.exe process hosts several windows.
pub fn console_window() -> u64 {
    unsafe {
        let con = GetConsoleWindow();
        if con.is_null() {
            return 0;
        }
        let top = GetAncestor(con, GA_ROOTOWNER);
        let top = if top.is_null() { con } else { top };
        if IsWindowVisible(top) != 0 {
            top as u64
        } else {
            0
        }
    }
}

/// The bound console window when it still exists and is visible.
fn bound_window(window: u64) -> Option<HWND> {
    let hwnd = window as usize as HWND;
    (window != 0 && unsafe { IsWindow(hwnd) != 0 && IsWindowVisible(hwnd) != 0 }).then_some(hwnd)
}

fn add_console_host(pids: &mut Vec<u32>, pid: u32, visible: bool) {
    if pid == 0 {
        return;
    }
    if visible {
        pids.retain(|candidate| *candidate != pid);
        pids.insert(0, pid);
    } else if !pids.contains(&pid) {
        pids.push(pid);
    }
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
    if IsWindowVisible(hwnd) != 0
        && GetWindow(hwnd, GW_OWNER).is_null()
        && GetWindowTextLengthW(hwnd) > 0
    {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        list.push((hwnd, pid));
    }
    1
}

/// Bring the first unambiguous session host to the front. Best effort: whole window, not tab.
/// A bound console window wins over PID matching.
pub fn focus_pids(window: u64, pids: &[u32]) -> bool {
    let mut list: Vec<(HWND, u32)> = vec![];
    unsafe {
        EnumWindows(Some(collect), &mut list as *mut _ as LPARAM);
    }
    if let Some(hwnd) = bound_window(window).or_else(|| nearest_host(pids, &list)) {
        unsafe {
            if IsIconic(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            }
            return SetForegroundWindow(hwnd) != 0;
        }
    }
    false
}

fn nearest_host<T: Copy>(pids: &[u32], windows: &[(T, u32)]) -> Option<T> {
    for pid in pids {
        let mut hosts = windows.iter().filter(|(_, owner)| owner == pid);
        if let Some((window, _)) = hosts.next() {
            // A shared process with several windows does not identify this session's window.
            // Do not guess one or continue to a farther ancestor from another session.
            return hosts.next().is_none().then_some(*window);
        }
    }
    None
}

fn focused_host(pids: &[u32], windows: &[(usize, u32)], foreground: usize) -> bool {
    foreground != 0 && nearest_host(pids, windows) == Some(foreground)
}

fn terminal_host(image: &str) -> bool {
    let name = image.rsplit(['\\', '/']).next().unwrap_or("");
    [
        "WindowsTerminal.exe",
        "WindowsTerminalPreview.exe",
        "OpenConsole.exe",
        "conhost.exe",
        "cmd.exe",
        "powershell.exe",
        "pwsh.exe",
        "claude.exe",
    ]
    .iter()
    .any(|terminal| name.eq_ignore_ascii_case(terminal))
}

/// Whether the nearest identifiable session host is foreground. Unknown/ambiguous hosts
/// keep popup handling. Win32 identifies windows here, not tabs or panes inside a host.
pub fn pids_focused(window: u64, pids: &[u32]) -> bool {
    if let Some(bound) = bound_window(window) {
        return unsafe { GetForegroundWindow() } == bound;
    }
    if pids.is_empty() {
        return false;
    }
    let mut windows: Vec<(HWND, u32)> = vec![];
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground.is_null() {
            return false;
        }
        let mut owner = 0;
        GetWindowThreadProcessId(foreground, &mut owner);
        // An editor's foreground window does not prove its integrated terminal has focus.
        if !process_image(owner).as_deref().is_some_and(terminal_host) {
            return false;
        }
        EnumWindows(Some(collect), &mut windows as *mut _ as LPARAM);
        let windows: Vec<_> = windows
            .into_iter()
            .map(|(hwnd, pid)| (hwnd as usize, pid))
            .collect();
        focused_host(pids, &windows, foreground as usize)
    }
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
    matches!(notification_state(), 2..=4)
}

/// QUNS_QUIET_TIME. Focus Assist has no public API; this is the nearest documented signal.
pub fn quiet_time_active() -> bool {
    notification_state() == 6
}

pub fn beep(kind: &str) {
    let t = if kind == "permission" {
        MB_ICONEXCLAMATION
    } else {
        MB_ICONASTERISK
    };
    unsafe {
        MessageBeep(t);
    }
}

/// Friendly monitor names keyed by GDI device name (`\.\DISPLAY1`), as Windows Settings shows
/// them: the EDID name ("Acer ET241Y"), or "Internal Display" for a laptop panel.
pub fn monitor_names() -> HashMap<String, String> {
    use windows_sys::Win32::Devices::Display::*;
    let mut out = HashMap::new();
    unsafe {
        let (mut np, mut nm) = (0u32, 0u32);
        if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut np, &mut nm) != 0 {
            return out;
        }
        let mut paths: Vec<DISPLAYCONFIG_PATH_INFO> = vec![std::mem::zeroed(); np as usize];
        let mut modes: Vec<DISPLAYCONFIG_MODE_INFO> = vec![std::mem::zeroed(); nm as usize];
        if QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut np,
            paths.as_mut_ptr(),
            &mut nm,
            modes.as_mut_ptr(),
            std::ptr::null_mut(),
        ) != 0
        {
            return out;
        }
        for p in &paths[..np as usize] {
            let mut src: DISPLAYCONFIG_SOURCE_DEVICE_NAME = std::mem::zeroed();
            src.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
            src.header.size = std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
            src.header.adapterId = p.sourceInfo.adapterId;
            src.header.id = p.sourceInfo.id;
            if DisplayConfigGetDeviceInfo(&mut src.header) != 0 {
                continue;
            }
            let mut tgt: DISPLAYCONFIG_TARGET_DEVICE_NAME = std::mem::zeroed();
            tgt.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
            tgt.header.size = std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
            tgt.header.adapterId = p.targetInfo.adapterId;
            tgt.header.id = p.targetInfo.id;
            if DisplayConfigGetDeviceInfo(&mut tgt.header) != 0 {
                continue;
            }
            let gdi = String::from_utf16_lossy(&src.viewGdiDeviceName)
                .trim_end_matches('\0')
                .to_string();
            let tech = tgt.outputTechnology;
            let internal = tech == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL
                || tech == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED
                || tech == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED;
            let name = String::from_utf16_lossy(&tgt.monitorFriendlyDeviceName)
                .trim_end_matches('\0')
                .to_string();
            let name = if internal {
                "Internal Display".to_string()
            } else if name.is_empty() {
                continue;
            } else {
                name
            };
            out.insert(gdi, name);
        }
    }
    out
}

#[cfg(test)]
mod focus_tests {
    use super::{add_console_host, focused_host, nearest_host, terminal_host};

    #[test]
    fn bound_visible_console_precedes_unrelated_ancestor_terminal() {
        let mut pids = vec![25184, 200, 11128];
        let windows = [(100, 11128), (200, 13036)];
        add_console_host(&mut pids, 13036, true);
        assert_eq!(pids, [13036, 25184, 200, 11128]);
        assert_eq!(nearest_host(&pids, &windows), Some(200));
        assert!(focused_host(&pids, &windows, 200));
        assert!(!focused_host(&pids, &windows, 100));
    }

    #[test]
    fn visible_console_owner_already_in_ancestry_is_promoted_once() {
        let mut pids = vec![1, 20, 30];
        add_console_host(&mut pids, 30, true);
        add_console_host(&mut pids, 30, true);
        assert_eq!(pids, [30, 1, 20]);
    }

    #[test]
    fn hidden_console_preserves_terminal_and_editor_ancestor_fallback() {
        for host in [20, 30] {
            let mut pids = vec![1, host, 40];
            add_console_host(&mut pids, 50, false);
            add_console_host(&mut pids, host, false);
            add_console_host(&mut pids, 0, true);
            assert_eq!(pids, [1, host, 40, 50]);
            assert_eq!(nearest_host(&pids, &[(100, host), (200, 40)]), Some(100));
        }
    }

    #[test]
    fn ambiguous_host_never_selects_a_window_or_farther_ancestor() {
        let windows = [(100, 10), (200, 10), (300, 30)];
        assert_eq!(nearest_host(&[1, 10, 30], &windows), None);
        assert!(!focused_host(&[1, 10, 30], &windows, 100));
        assert!(!focused_host(&[1, 10, 30], &windows, 300));
        assert_eq!(nearest_host(&[30, 10], &windows), Some(300));
        assert_eq!(nearest_host(&[], &windows), None);
        assert_eq!(nearest_host::<usize>(&[10], &[]), None);
    }

    #[test]
    fn editor_window_is_not_a_focused_terminal() {
        assert!(terminal_host("C:/Windows/System32/conhost.exe"));
        assert!(terminal_host(
            "C:/Program Files/WindowsApps/WindowsTerminal.exe"
        ));
        assert!(!terminal_host("C:/Code.exe"));
        assert!(!terminal_host("C:/explorer.exe"));
    }

    #[test]
    fn only_the_nearest_session_host_counts_as_focused() {
        let windows = [(100, 10), (200, 20), (300, 30)];
        assert!(focused_host(&[1, 10, 20], &windows, 100));
        assert!(!focused_host(&[1, 10, 20], &windows, 200));
        assert!(!focused_host(&[1, 10, 20], &windows, 300));
    }

    #[test]
    fn unknown_or_ambiguous_focus_keeps_popup_handling() {
        assert!(!focused_host(&[], &[(100, 10)], 100));
        assert!(!focused_host(&[10], &[], 100));
        assert!(!focused_host(&[10], &[(100, 10)], 0));
        assert!(!focused_host(&[10], &[(100, 10), (200, 10)], 100));
    }
}

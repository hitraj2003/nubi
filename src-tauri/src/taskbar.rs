//! Taskbar strip placement, following CodeZeno/Claude-Code-Usage-Monitor (MIT, commit dea72c9):
//! find `Shell_TrayWnd` / `Shell_SecondaryTrayWnd`, sit flush left of `TrayNotifyWnd`, bottom-aligned
//! to the tray, re-checked every 2 s, hidden while a full-screen app covers that monitor.
//! Like CodeZeno (`embed_as_child`), the strip is re-parented into the taskbar window: Windows 11 draws
//! the taskbar above ordinary topmost windows, so an overlay would sit underneath it.
//! Known gap: if Explorer restarts, the strip window goes with the old taskbar until the app restarts.
use crate::app::{App, Shared};
use std::time::Duration;
use tauri::{Emitter, Manager};
use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, GetClassNameW, GetCursorPos, GetForegroundWindow, GetParent, GetWindowLongW, GetWindowRect,
    IsWindowVisible, SetParent, SetWindowLongW, SetWindowPos, ShowWindow, GWL_EXSTYLE, GWL_STYLE, HWND_TOP,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, WS_CHILD, WS_CLIPSIBLINGS, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

/// Logical size of the strip (CodeZeno classic theme: 46 tall; width grows with our provider glyph).
pub const STRIP_W: f64 = 233.0;
pub const STRIP_H: f64 = 46.0;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn class_of(h: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(h, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn rect_of(h: HWND) -> Option<RECT> {
    let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    (unsafe { GetWindowRect(h, &mut r) } != 0).then_some(r)
}

unsafe extern "system" fn collect_bars(h: HWND, lp: LPARAM) -> BOOL {
    let list = &mut *(lp as *mut Vec<HWND>);
    let c = class_of(h);
    if c == "Shell_TrayWnd" || c == "Shell_SecondaryTrayWnd" {
        list.push(h);
    }
    1
}

fn overlap(a: &RECT, b: (i32, i32, i32, i32)) -> i64 {
    let (x, y, w, h) = b;
    let l = a.left.max(x);
    let t = a.top.max(y);
    let r = a.right.min(x + w);
    let btm = a.bottom.min(y + h);
    ((r - l).max(0) as i64) * ((btm - t).max(0) as i64)
}

/// (taskbar rect, tray rect) for the monitor at `mon` (x, y, w, h).
fn taskbar_for(mon: (i32, i32, i32, i32)) -> Option<(HWND, RECT, RECT)> {
    let mut bars: Vec<HWND> = vec![];
    unsafe {
        EnumWindows(Some(collect_bars), &mut bars as *mut _ as LPARAM);
    }
    let bar = bars
        .into_iter()
        .filter_map(|h| rect_of(h).map(|r| (h, r)))
        .max_by_key(|(_, r)| overlap(r, mon))
        .filter(|(_, r)| overlap(r, mon) > 0)?;
    let tray_class = wide("TrayNotifyWnd");
    let tray = unsafe { FindWindowExW(bar.0, std::ptr::null_mut(), tray_class.as_ptr(), std::ptr::null()) };
    let tray_rect = if tray.is_null() { None } else { rect_of(tray) };
    // No tray on this bar: a zero-width reference at the bar's right end (CodeZeno system_tray_reference).
    let tr = tray_rect.unwrap_or(RECT { left: bar.1.right, top: bar.1.top, right: bar.1.right, bottom: bar.1.bottom });
    Some((bar.0, bar.1, tr))
}

/// CodeZeno `embed_as_child`: child style, tool + no-activate, not topmost, SetParent, and a
/// layered-style toggle so Windows 11 does not leave it under the taskbar's composition visual.
fn embed(hwnd: HWND, parent: HWND) {
    unsafe {
        if GetParent(hwnd) == parent {
            return;
        }
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
        SetWindowLongW(hwnd, GWL_EXSTYLE, (ex | WS_EX_TOOLWINDOW as i32 | WS_EX_NOACTIVATE as i32) & !(WS_EX_TOPMOST as i32));
        let st = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        SetWindowLongW(hwnd, GWL_STYLE, ((st & !WS_POPUP) | WS_CHILD | WS_CLIPSIBLINGS) as i32);
        SetParent(hwnd, parent);
        if GetParent(hwnd) == parent {
            let ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
            if ex & WS_EX_LAYERED as i32 != 0 {
                SetWindowLongW(hwnd, GWL_EXSTYLE, ex & !(WS_EX_LAYERED as i32));
                SetWindowLongW(hwnd, GWL_EXSTYLE, ex);
            }
        }
    }
}

/// Full screen on that monitor: the foreground window covers it within 2 px (CodeZeno rule),
/// ignoring the desktop and taskbar classes.
fn fullscreen_on(mon: (i32, i32, i32, i32)) -> bool {
    let fg = unsafe { GetForegroundWindow() };
    if fg.is_null() || unsafe { IsWindowVisible(fg) } == 0 {
        return false;
    }
    let c = class_of(fg);
    if ["Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd"].contains(&c.as_str()) {
        return false;
    }
    let Some(r) = rect_of(fg) else { return false };
    let (x, y, w, h) = mon;
    r.left <= x + 2 && r.top <= y + 2 && r.right >= x + w - 2 && r.bottom >= y + h - 2
}

/// Windows app theme: light taskbar when SystemUsesLightTheme = 1.
pub fn taskbar_is_light() -> bool {
    let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let val = wide("SystemUsesLightTheme");
    let mut data: u32 = 0;
    let mut size = 4u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            val.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            &mut data as *mut u32 as *mut _,
            &mut size,
        )
    };
    rc == 0 && data == 1
}

/// One placement at a time. Without it the 2 s watchdog could read the old mode, then show the
/// strip again right after a mode switch hid it.
static PLACE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn place_once(app: &App) {
    let _guard = PLACE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some(w) = app.get_webview_window("taskbar") else { return };
    let s = {
        let st = app.state::<Shared>();
        let g = st.0.lock().unwrap_or_else(|e| e.into_inner());
        g.settings.clone()
    };
    if s.display_mode != "taskbar" {
        // The strip is a child of Explorer's taskbar: hide it natively so it does not linger there.
        if let Some(hwnd) = w.hwnd().ok().map(|h| h.0 as HWND) {
            unsafe { ShowWindow(hwnd, SW_HIDE) };
        }
        let _ = w.hide();
        return;
    }
    let mons = app.available_monitors().unwrap_or_default();
    let Some(m) = mons.get(s.monitor).or(mons.first()) else { return };
    let mon = (m.position().x, m.position().y, m.size().width as i32, m.size().height as i32);
    let scale = m.scale_factor();
    let (ww, wh) = ((STRIP_W * scale).round() as i32, (STRIP_H * scale).round() as i32);
    let Some(hwnd) = w.hwnd().ok().map(|h| h.0 as HWND) else { return };
    let Some((bar_hwnd, bar, tray)) = taskbar_for(mon) else {
        unsafe { ShowWindow(hwnd, SW_HIDE) };
        return;
    };
    if fullscreen_on(mon) {
        unsafe { ShowWindow(hwnd, SW_HIDE) };
        return;
    }
    let pad = (8.0 * scale).round() as i32;
    let x = if s.taskbar_side == "left" { bar.left + pad } else { tray.left - ww };
    // Bottom-aligned to the tray, which fills the bar's height (CodeZeno aligned_origin).
    let y = tray.bottom - wh;
    embed(hwnd, bar_hwnd);
    // Child coordinates are relative to the taskbar window.
    unsafe {
        SetWindowPos(hwnd, HWND_TOP, x - bar.left, y - bar.top, ww, wh, SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_FRAMECHANGED);
    }
    let _ = w.emit_to(w.label(), "taskbar_theme", if taskbar_is_light() { "light" } else { "dark" });
}

/// Watchdog every 2 s (CodeZeno TASKBAR_WATCH_INTERVAL_SECS).
pub fn start(app: App) {
    let a2 = app.clone();
    std::thread::spawn(move || loop {
        place_once(&app);
        std::thread::sleep(Duration::from_secs(2));
    });
    // A web view inside Explorer's taskbar does not receive clicks, so watch the button natively:
    // a click on the strip opens the panel.
    std::thread::spawn(move || {
        let mut was_down = false;
        loop {
            std::thread::sleep(Duration::from_millis(40));
            let Some(w) = a2.get_webview_window("taskbar") else { continue };
            if !w.is_visible().unwrap_or(false) {
                continue;
            }
            let Some(hwnd) = w.hwnd().ok().map(|h| h.0 as HWND) else { continue };
            let down = unsafe { (GetAsyncKeyState(VK_LBUTTON as i32) as u16 & 0x8000) != 0 || (GetAsyncKeyState(VK_RBUTTON as i32) as u16 & 0x8000) != 0 };
            if down && !was_down {
                let mut p = POINT { x: 0, y: 0 };
                if unsafe { GetCursorPos(&mut p) } != 0 {
                    if let Some(r) = rect_of(hwnd) {
                        if p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom {
                            let waiting = {
                                let st = a2.state::<Shared>();
                                let g = st.0.lock().unwrap_or_else(|e| e.into_inner());
                                !g.pending.is_empty()
                            };
                            if waiting { crate::app::show_requests(a2.clone()) } else { crate::app::open_panel(&a2) }
                        }
                    }
                }
            }
            was_down = down;
        }
    });
}

pub fn place_now(app: &App) {
    place_once(app);
}

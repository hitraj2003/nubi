//! Input protection for the ordinary alert window; independent of notch shape state.
use std::{cell::Cell, ffi::c_void};
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT},
    UI::WindowsAndMessaging::*,
};
type Procedure = unsafe extern "system" fn(HWND, u32, usize, isize, usize, usize) -> isize;
#[link(name = "comctl32")]
unsafe extern "system" {
    fn SetWindowSubclass(window: HWND, callback: Option<Procedure>, id: usize, data: usize) -> i32;
    fn RemoveWindowSubclass(window: HWND, callback: Option<Procedure>, id: usize) -> i32;
    fn DefSubclassProc(window: HWND, message: u32, w: usize, l: isize) -> isize;
}
#[link(name = "user32")]
unsafe extern "system" {
    fn GetDpiForWindow(window: HWND) -> u32;
}
const ID: usize = 0x414c4552;
const NO_ACTIVATE: isize = 0x08000000;
struct Area {
    rect: Cell<[f64; 4]>,
    alive: Cell<bool>,
}
impl Area {
    fn contains(&self, window: HWND, x: i32, y: i32) -> bool {
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        // SAFETY: live window and writable stack RECT.
        if unsafe { GetWindowRect(window, &mut rect) } == 0 {
            return false;
        }
        // SAFETY: query only; HWND is owned by the Slint window.
        let scale = f64::from(unsafe { GetDpiForWindow(window) }.max(96)) / 96.;
        let [left, top, width, height] = self.rect.get();
        let (x, y) = (
            f64::from(x - rect.left) / scale,
            f64::from(y - rect.top) / scale,
        );
        x >= left && y >= top && x < left + width && y < top + height
    }
}
unsafe extern "system" fn procedure(
    window: HWND,
    message: u32,
    w: usize,
    l: isize,
    _: usize,
    data: usize,
) -> isize {
    if message == WM_MOUSEACTIVATE {
        return 3;
    }
    if message == WM_STYLECHANGING && w as i32 == GWL_EXSTYLE {
        // SAFETY: Windows provides a writable STYLESTRUCT for WM_STYLECHANGING.
        let styles = unsafe { &mut *(l as *mut STYLESTRUCT) };
        styles.styleNew = (styles.styleNew | NO_ACTIVATE as u32 | WS_EX_TOOLWINDOW | WS_EX_LAYERED)
            & !WS_EX_APPWINDOW;
    }
    if message == WM_NCDESTROY {
        // SAFETY: callback data remains owned until Popup drops on this UI thread.
        unsafe {
            (&*(data as *const Area)).alive.set(false);
            RemoveWindowSubclass(window, Some(procedure), ID);
        }
    }
    if message == WM_NCHITTEST {
        // SAFETY: boxed Area lives until this subclass is removed on the UI thread.
        let area = unsafe { &*(data as *const Area) };
        let (x, y) = ((l as u16 as i16) as i32, ((l >> 16) as u16 as i16) as i32);
        return if area.contains(window, x, y) { 1 } else { -1 };
    }
    // SAFETY: forward original parameters through the native subclass chain.
    unsafe { DefSubclassProc(window, message, w, l) }
}

pub struct Popup {
    window: HWND,
    area: Box<Area>,
}
impl Popup {
    pub fn new(window: isize) -> Result<Self, String> {
        let window = window as *mut c_void;
        let area = Box::new(Area {
            rect: Cell::new([0.; 4]),
            alive: Cell::new(true),
        });
        // SAFETY: UI thread owns the HWND; these style changes do not activate it.
        unsafe {
            let style = GetWindowLongPtrW(window, GWL_EXSTYLE);
            SetWindowLongPtrW(
                window,
                GWL_EXSTYLE,
                (style
                    | NO_ACTIVATE
                    | WS_EX_TOOLWINDOW as isize
                    | WS_EX_TRANSPARENT as isize
                    | WS_EX_LAYERED as isize)
                    & !(WS_EX_APPWINDOW as isize),
            );
            if SetWindowSubclass(
                window,
                Some(procedure),
                ID,
                (&*area as *const Area) as usize,
            ) == 0
            {
                return Err(std::io::Error::last_os_error().to_string());
            }
        }
        Ok(Self { window, area })
    }
    pub fn place(&self, index: usize) -> Result<(u32, u32, f32), String> {
        use windows_sys::Win32::Graphics::Gdi::*;
        unsafe extern "system" fn collect(
            monitor: HMONITOR,
            _: HDC,
            _: *mut RECT,
            data: isize,
        ) -> i32 {
            // SAFETY: EnumDisplayMonitors synchronously receives this live Vec pointer.
            unsafe {
                (&mut *(data as *mut Vec<HMONITOR>)).push(monitor);
            }
            1
        }
        let mut monitors = Vec::<HMONITOR>::new();
        // SAFETY: synchronous enumeration, no clip rectangle, pointer lives for the call.
        unsafe {
            EnumDisplayMonitors(
                std::ptr::null_mut(),
                std::ptr::null(),
                Some(collect),
                (&mut monitors as *mut Vec<HMONITOR>) as isize,
            );
        }
        let monitor = monitors
            .get(index)
            .or(monitors.first())
            .copied()
            .ok_or("No display monitor available")?;
        let empty = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            rcMonitor: empty,
            rcWork: empty,
            dwFlags: 0,
        };
        // SAFETY: monitor from enumeration and initialized output structure.
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        #[link(name = "shcore")]
        unsafe extern "system" {
            fn GetDpiForMonitor(monitor: HMONITOR, kind: i32, x: *mut u32, y: *mut u32) -> i32;
        }
        // SAFETY: query only, window is live.
        let mut x = unsafe { GetDpiForWindow(self.window) }.max(96);
        let mut y = x;
        // SAFETY: enumerated monitor and valid output pointers; fallback DPI retained on failure.
        unsafe {
            GetDpiForMonitor(monitor, 0, &mut x, &mut y);
        }
        let scale = f64::from(x.max(96)) / 96.;
        let width = ((720. * scale).round() as i32)
            .min(info.rcMonitor.right - info.rcMonitor.left)
            .max(1);
        let height = ((160. * scale).round() as i32)
            .min(info.rcMonitor.bottom - info.rcMonitor.top)
            .max(1);
        let left = info.rcMonitor.left + (info.rcMonitor.right - info.rcMonitor.left - width) / 2;
        // SAFETY: live window, original top-centre placement, no focus transfer.
        if unsafe {
            SetWindowPos(
                self.window,
                HWND_TOPMOST,
                left,
                info.rcMonitor.top,
                width,
                height,
                SWP_NOACTIVATE | SWP_FRAMECHANGED,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok((width as u32, height as u32, scale as f32))
    }
    pub fn update(&self, rect: [f64; 4]) {
        self.area.rect.set(rect);
        let mut cursor = POINT { x: 0, y: 0 };
        // SAFETY: stack POINT and live HWND; the subclass prevents focus stealing.
        unsafe {
            let inside = GetCursorPos(&mut cursor) != 0
                && self.area.contains(self.window, cursor.x, cursor.y);
            let style = GetWindowLongPtrW(self.window, GWL_EXSTYLE);
            let next = if inside {
                style & !(WS_EX_TRANSPARENT as isize)
            } else {
                style | WS_EX_TRANSPARENT as isize
            };
            if next != style {
                SetWindowLongPtrW(self.window, GWL_EXSTYLE, next);
            }
        }
    }
}
impl Drop for Popup {
    fn drop(&mut self) {
        // SAFETY: removed on the UI thread before Area or its Slint HWND is released.
        unsafe {
            if self.area.alive.get() {
                RemoveWindowSubclass(self.window, Some(procedure), ID);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn popup_preserves_layered_nonactivating_style_and_handles_window_destruction() {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        // SAFETY: built-in class and null-terminated title; this test owns the hidden HWND.
        let window = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                WS_POPUP,
                0,
                0,
                720,
                160,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        assert!(!window.is_null());
        let popup = Popup::new(window as isize).unwrap();
        // SAFETY: query/mutation on the test-owned HWND; subclass guards style changes.
        unsafe {
            SetWindowLongPtrW(window, GWL_EXSTYLE, 0);
            let style = GetWindowLongPtrW(window, GWL_EXSTYLE);
            assert_ne!(style & NO_ACTIVATE, 0);
            assert_ne!(style & WS_EX_TOOLWINDOW as isize, 0);
            assert_ne!(style & WS_EX_LAYERED as isize, 0);
            assert_eq!(style & WS_EX_APPWINDOW as isize, 0);
            assert_eq!(SendMessageW(window, WM_MOUSEACTIVATE, 0, 0), 3);
            assert_ne!(DestroyWindow(window), 0);
        }
        assert!(!popup.area.alive.get());
        drop(popup);
    }
}

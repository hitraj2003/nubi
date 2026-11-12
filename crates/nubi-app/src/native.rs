//! Native notch placement, non-activation and curved hit testing.
use crate::shape::{Shape, WINDOW_HEIGHT, WINDOW_WIDTH};
use std::cell::Cell;
use std::ffi::c_void;
use std::io;

type Hwnd = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
#[repr(C)]
#[derive(Default)]
struct Point {
    x: i32,
    y: i32,
}
#[repr(C)]
struct MonitorInfo {
    size: u32,
    monitor: Rect,
    work: Rect,
    flags: u32,
}

#[repr(C)]
struct StyleChange {
    old: u32,
    new: u32,
}

const GWL_EXSTYLE: i32 = -20;
const WS_EX_NOACTIVATE: isize = 0x0800_0000;
const WS_EX_TOOLWINDOW: isize = 0x0000_0080;
const WS_EX_APPWINDOW: isize = 0x0004_0000;
const WS_EX_TRANSPARENT: isize = 0x20;
const WS_EX_LAYERED: isize = 0x0008_0000;
const SWP_NOACTIVATE: u32 = 0x10;
const SWP_FRAMECHANGED: u32 = 0x20;
const SUBCLASS_ID: usize = 0x534c_494e;
type SubclassProc = unsafe extern "system" fn(Hwnd, u32, usize, isize, usize, usize) -> isize;

#[link(name = "user32")]
unsafe extern "system" {
    fn SetProcessDpiAwarenessContext(context: isize) -> i32;
    fn GetDpiForWindow(window: Hwnd) -> u32;
    fn GetWindowLongPtrW(window: Hwnd, index: i32) -> isize;
    fn SetWindowLongPtrW(window: Hwnd, index: i32, value: isize) -> isize;
    fn SetWindowPos(window: Hwnd, after: Hwnd, x: i32, y: i32, cx: i32, cy: i32, flags: u32)
        -> i32;
    fn GetWindowRect(window: Hwnd, rect: *mut Rect) -> i32;
    fn GetCursorPos(point: *mut Point) -> i32;
    fn MonitorFromWindow(window: Hwnd, flags: u32) -> *mut c_void;
    fn GetMonitorInfoW(monitor: *mut c_void, info: *mut MonitorInfo) -> i32;
    fn SystemParametersInfoW(action: u32, param: u32, value: *mut c_void, flags: u32) -> i32;
}

#[link(name = "comctl32")]
unsafe extern "system" {
    fn SetWindowSubclass(
        window: Hwnd,
        callback: Option<SubclassProc>,
        id: usize,
        data: usize,
    ) -> i32;
    fn RemoveWindowSubclass(window: Hwnd, callback: Option<SubclassProc>, id: usize) -> i32;
    fn DefSubclassProc(window: Hwnd, message: u32, wparam: usize, lparam: isize) -> isize;
}

thread_local! {
    static SHAPE: Cell<Shape> = const { Cell::new(Shape(crate::shape::CLOSED)) };
    static PLACE_DIRTY: Cell<bool> = const { Cell::new(true) };
}

/// Call before Slint/winit creates any windows. winit also establishes DPI awareness.
pub fn enable_dpi_awareness() {
    // DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2. An already-set context is harmless.
    unsafe {
        SetProcessDpiAwarenessContext(-4);
    }
}

pub fn system_reduced_motion() -> bool {
    let mut enabled = 1_i32;
    // SPI_GETCLIENTAREAANIMATION.
    let success = unsafe { SystemParametersInfoW(0x1042, 0, (&mut enabled as *mut i32).cast(), 0) };
    success != 0 && enabled == 0
}

pub fn set_shape(shape: Shape) {
    SHAPE.with(|value| value.set(shape));
}

fn contains_screen(window: Hwnd, x: i32, y: i32) -> bool {
    let mut rect = Rect::default();
    if unsafe { GetWindowRect(window, &mut rect) } == 0 {
        return false;
    }
    let scale = f64::from(unsafe { GetDpiForWindow(window) }.max(96)) / 96.0;
    SHAPE.with(|shape| {
        shape.get().contains(
            f64::from(x - rect.left) / scale,
            f64::from(y - rect.top) / scale,
        )
    })
}

unsafe extern "system" fn procedure(
    window: Hwnd,
    message: u32,
    wparam: usize,
    lparam: isize,
    _: usize,
    _: usize,
) -> isize {
    match message {
        0x0021 => return 3, // WM_MOUSEACTIVATE: MA_NOACTIVATE; retain the click.
        0x0084 => {
            // WM_NCHITTEST. Signed coordinates support negative-origin monitors.
            let x = (lparam as u16) as i16 as i32;
            let y = ((lparam >> 16) as u16) as i16 as i32;
            return if contains_screen(window, x, y) { 1 } else { -1 }; // HTCLIENT / HTTRANSPARENT.
        }
        0x02e0 | 0x007e | 0x001a => PLACE_DIRTY.with(|dirty| dirty.set(true)), // DPI/display/settings.
        0x007c if wparam as i32 == GWL_EXSTYLE => {
            // WM_STYLECHANGING: winit may rewrite the style after DPI/theme changes.
            let style = unsafe { &mut *(lparam as *mut StyleChange) };
            style.new = (style.new | WS_EX_NOACTIVATE as u32 | WS_EX_TOOLWINDOW as u32)
                & !(WS_EX_APPWINDOW as u32);
        }
        0x0082 => {
            // WM_NCDESTROY.
            unsafe {
                RemoveWindowSubclass(window, Some(procedure), SUBCLASS_ID);
            }
        }
        _ => {}
    }
    unsafe { DefSubclassProc(window, message, wparam, lparam) }
}

pub struct NativeWindow {
    window: Hwnd,
    transparent: bool,
    base_layered: bool,
}

impl NativeWindow {
    /// Call on the UI thread after native creation, before revealing notch content.
    /// The caller keeps the Slint/raw-window-handle owner alive for this object's lifetime.
    pub fn new(handle: isize) -> io::Result<Self> {
        let window = handle as Hwnd;
        if window.is_null() {
            return Err(io::Error::other("missing Win32 window"));
        }
        let old = unsafe { GetWindowLongPtrW(window, GWL_EXSTYLE) };
        let style = (old | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_TRANSPARENT)
            & !WS_EX_APPWINDOW;
        unsafe {
            SetWindowLongPtrW(window, GWL_EXSTYLE, style);
        }
        if unsafe { SetWindowSubclass(window, Some(procedure), SUBCLASS_ID, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let native = Self {
            window,
            transparent: true,
            base_layered: old & WS_EX_LAYERED != 0,
        };
        native.place()?;
        Ok(native)
    }

    pub fn pointer_inside(&self) -> Option<bool> {
        let mut cursor = Point::default();
        if unsafe { GetCursorPos(&mut cursor) } == 0 {
            return None;
        }
        Some(contains_screen(self.window, cursor.x, cursor.y))
    }

    pub fn click_through(&mut self, transparent: bool) {
        if self.transparent == transparent {
            return;
        }
        let old = unsafe { GetWindowLongPtrW(self.window, GWL_EXSTYLE) };
        let style = if transparent {
            old | WS_EX_TRANSPARENT | WS_EX_LAYERED
        } else if self.base_layered {
            old & !WS_EX_TRANSPARENT
        } else {
            old & !(WS_EX_TRANSPARENT | WS_EX_LAYERED)
        };
        unsafe {
            SetWindowLongPtrW(self.window, GWL_EXSTYLE, style);
        }
        self.transparent = transparent;
    }

    pub fn refresh_placement(&self) -> io::Result<()> {
        if PLACE_DIRTY.with(|dirty| dirty.replace(false)) {
            self.place()?;
        }
        Ok(())
    }

    fn place(&self) -> io::Result<()> {
        let mut info = MonitorInfo {
            size: std::mem::size_of::<MonitorInfo>() as u32,
            monitor: Rect::default(),
            work: Rect::default(),
            flags: 0,
        };
        let monitor = unsafe { MonitorFromWindow(self.window, 2) }; // MONITOR_DEFAULTTONEAREST.
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let scale = f64::from(unsafe { GetDpiForWindow(self.window) }.max(96)) / 96.0;
        let width = (WINDOW_WIDTH * scale).round() as i32;
        let height = (WINDOW_HEIGHT * scale).round() as i32;
        let x = info.monitor.left + (info.monitor.right - info.monitor.left - width) / 2;
        let topmost = -1_isize as Hwnd; // HWND_TOPMOST.
        if unsafe {
            SetWindowPos(
                self.window,
                topmost,
                x,
                info.monitor.top,
                width,
                height,
                SWP_NOACTIVATE | SWP_FRAMECHANGED,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

impl Drop for NativeWindow {
    fn drop(&mut self) {
        unsafe {
            RemoveWindowSubclass(self.window, Some(procedure), SUBCLASS_ID);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn CreateWindowExW(
            ex: u32,
            class: *const u16,
            title: *const u16,
            style: u32,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            parent: Hwnd,
            menu: Hwnd,
            instance: Hwnd,
            parameter: Hwnd,
        ) -> Hwnd;
        fn SendMessageW(window: Hwnd, message: u32, wparam: usize, lparam: isize) -> isize;
        fn DestroyWindow(window: Hwnd) -> i32;
    }

    #[test]
    fn win32_styles_noactivate_and_curved_hit_test() {
        enable_dpi_awareness();
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let handle = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                0x8000_0000,
                0,
                0,
                720,
                700,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert!(!handle.is_null(), "{}", io::Error::last_os_error());
        {
            let mut window = NativeWindow::new(handle as isize).unwrap();
            set_shape(Shape(crate::shape::OPENED));
            let style = unsafe { GetWindowLongPtrW(handle, GWL_EXSTYLE) };
            assert_ne!(style & 0x8, 0); // WS_EX_TOPMOST.
            assert_ne!(style & WS_EX_NOACTIVATE, 0);
            assert_ne!(style & WS_EX_TOOLWINDOW, 0);
            unsafe {
                SetWindowLongPtrW(handle, GWL_EXSTYLE, 0);
            }
            assert_ne!(
                unsafe { GetWindowLongPtrW(handle, GWL_EXSTYLE) } & WS_EX_NOACTIVATE,
                0
            );
            window.transparent = false;
            window.click_through(true);
            assert_eq!(unsafe { SendMessageW(handle, 0x0021, 0, 0) }, 3);
            window.click_through(false);
            assert_eq!(
                unsafe { GetWindowLongPtrW(handle, GWL_EXSTYLE) } & WS_EX_TRANSPARENT,
                0
            );
            window.click_through(true);
            assert_ne!(
                unsafe { GetWindowLongPtrW(handle, GWL_EXSTYLE) } & WS_EX_TRANSPARENT,
                0
            );
            let mut rect = Rect::default();
            assert_ne!(unsafe { GetWindowRect(handle, &mut rect) }, 0);
            let scale = f64::from(unsafe { GetDpiForWindow(handle) }) / 96.0;
            let at = |x: f64, y: f64| {
                let x = rect.left + (x * scale).round() as i32;
                let y = rect.top + (y * scale).round() as i32;
                let packed = (x as u16 as isize) | ((y as u16 as isize) << 16);
                unsafe { SendMessageW(handle, 0x0084, 0, packed) }
            };
            assert_eq!(at(360.0, 100.0), 1);
            assert_eq!(at(51.0, 187.0), -1);
            assert_eq!(at(0.0, 0.0), -1);
        }
        assert_ne!(unsafe { DestroyWindow(handle) }, 0);
    }
}

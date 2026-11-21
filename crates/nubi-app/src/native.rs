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
    static EXPANSION_OFFSET: Cell<f64> = const { Cell::new(0.) };
    static SHAPE: Cell<Shape> = const { Cell::new(Shape(crate::shape::CLOSED)) };
    static PLACE_DIRTY: Cell<bool> = const { Cell::new(true) };
    static SCENE: Cell<crate::shape::Scene> = const { Cell::new(crate::shape::Scene { edge: 0, offset: 0., width: WINDOW_WIDTH, height: WINDOW_HEIGHT }) };
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

/// Ordinary panel windows need the same button-coordinate ordering as the notch,
/// without the notch's hit testing or non-activation styles.
pub fn repair_panel_pointer(handle: isize) -> io::Result<()> {
    if unsafe {
        SetWindowSubclass(
            handle as Hwnd,
            Some(panel_pointer_procedure),
            SUBCLASS_ID + 1,
            0,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
unsafe extern "system" fn panel_pointer_procedure(
    window: Hwnd,
    message: u32,
    wparam: usize,
    lparam: isize,
    _: usize,
    _: usize,
) -> isize {
    if matches!(message,0x0201..=0x0209 | 0x020b..=0x020d) {
        unsafe {
            DefSubclassProc(window, 0x0200, wparam & 0xffff, lparam);
        }
    }
    if message == 0x0082 {
        unsafe {
            RemoveWindowSubclass(window, Some(panel_pointer_procedure), SUBCLASS_ID + 1);
        }
    }
    unsafe { DefSubclassProc(window, message, wparam, lparam) }
}

pub fn set_shape(shape: Shape) {
    SHAPE.with(|value| value.set(shape));
}

pub fn set_expansion_offset(offset: f64) {
    EXPANSION_OFFSET.with(|value| value.set(offset));
}

fn contains_screen(window: Hwnd, x: i32, y: i32) -> bool {
    let mut rect = Rect::default();
    if unsafe { GetWindowRect(window, &mut rect) } == 0 {
        return false;
    }
    let scale = f64::from(unsafe { GetDpiForWindow(window) }.max(96)) / 96.0;
    SHAPE.with(|shape| {
        SCENE.with(|scene| {
            let mut geometry = scene.get();
            geometry.offset += EXPANSION_OFFSET.with(Cell::get);
            geometry.contains(
                shape.get(),
                f64::from(x - rect.left) / scale,
                f64::from(y - rect.top) / scale,
            )
        })
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
        0x0201..=0x0209 | 0x020b..=0x020d => {
            // Windows can deliver a button message before its coalesced WM_MOUSEMOVE.
            // winit's MouseInput has no coordinates, so refresh its cached position
            // from this button's client coordinates before forwarding the button.
            // DefSubclassProc keeps normal Slint capture, focus and click handling.
            unsafe {
                DefSubclassProc(window, 0x0200, wparam & 0xffff, lparam);
            }
        }
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
    edge: i32,
    ratio: f64,
    monitor: usize,
    drag_start: Option<(i32, i32)>,
    dragged: bool,
}

pub struct Placement {
    pub width: u32,
    pub height: u32,
    pub scale: f64,
    pub offset: f64,
    pub preferences_changed: bool,
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
            edge: 0,
            ratio: 0.5,
            monitor: 0,
            drag_start: None,
            dragged: false,
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

    pub fn focused(&self) -> bool {
        unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() == self.window }
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

    pub fn configure(&mut self, settings: &nubi_core::settings::Settings) -> io::Result<Placement> {
        let edge = match settings.edge.as_str() {
            "top" => 0,
            "bottom" => 1,
            "left" => 2,
            _ => 3,
        };
        let changed = self.edge != edge
            || self.ratio != settings.notch_along
            || self.monitor != settings.monitor;
        self.edge = edge;
        self.ratio = settings.notch_along;
        self.monitor = settings.monitor;
        let dirty = PLACE_DIRTY.with(|dirty| dirty.replace(false));
        if changed || dirty {
            self.place()?;
        }
        let scene = SCENE.with(Cell::get);
        let scale = f64::from(unsafe { GetDpiForWindow(self.window) }.max(96)) / 96.;
        Ok(Placement {
            width: (scene.width * scale).round() as u32,
            height: (scene.height * scale).round() as u32,
            scale,
            offset: scene.offset,
            preferences_changed: dirty,
        })
    }

    pub fn start_drag(&mut self) {
        let mut point = Point::default();
        if unsafe { GetCursorPos(&mut point) } != 0 {
            self.drag_start = Some((point.x, point.y));
            self.dragged = false;
        }
    }
    pub fn drag(&mut self) -> Option<(f64, bool, bool)> {
        let start = self.drag_start?;
        let mut point = Point::default();
        if unsafe { GetCursorPos(&mut point) } == 0 {
            return None;
        }
        let down =
            unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(1) } < 0;
        if !self.dragged && f64::from(point.x - start.0).hypot(f64::from(point.y - start.1)) >= 5. {
            self.dragged = true;
        }
        if !down {
            self.drag_start = None;
        }
        let (_, info) = self.monitor_info().ok()?;
        let area = if self.edge == 0 {
            info.monitor
        } else {
            info.work
        };
        let ratio = if self.edge >= 2 {
            f64::from(point.y - area.top) / f64::from((area.bottom - area.top).max(1))
        } else {
            f64::from(point.x - area.left) / f64::from((area.right - area.left).max(1))
        };
        Some((ratio.clamp(0., 1.), !down, self.dragged))
    }

    fn monitor_info(&self) -> io::Result<(Hwnd, MonitorInfo)> {
        unsafe extern "system" fn collect(
            monitor: *mut c_void,
            _: *mut c_void,
            _: *mut Rect,
            data: isize,
        ) -> i32 {
            unsafe { &mut *(data as *mut Vec<*mut c_void>) }.push(monitor);
            1
        }
        #[link(name = "user32")]
        unsafe extern "system" {
            fn EnumDisplayMonitors(
                dc: *mut c_void,
                clip: *const Rect,
                callback: unsafe extern "system" fn(
                    *mut c_void,
                    *mut c_void,
                    *mut Rect,
                    isize,
                ) -> i32,
                data: isize,
            ) -> i32;
        }
        let mut monitors: Vec<*mut c_void> = Vec::new();
        unsafe {
            EnumDisplayMonitors(
                std::ptr::null_mut(),
                std::ptr::null(),
                collect,
                (&mut monitors as *mut Vec<*mut c_void>) as isize,
            );
        }
        let monitor = monitors
            .get(self.monitor)
            .or(monitors.first())
            .copied()
            .unwrap_or_else(|| unsafe { MonitorFromWindow(self.window, 2) });
        let mut info = MonitorInfo {
            size: std::mem::size_of::<MonitorInfo>() as u32,
            monitor: Rect::default(),
            work: Rect::default(),
            flags: 0,
        };
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((monitor, info))
    }

    fn place(&self) -> io::Result<()> {
        let (monitor, info) = self.monitor_info()?;
        #[link(name = "shcore")]
        unsafe extern "system" {
            fn GetDpiForMonitor(monitor: Hwnd, kind: i32, x: *mut u32, y: *mut u32) -> i32;
        }
        let mut dpi_x = unsafe { GetDpiForWindow(self.window) }.max(96);
        let mut dpi_y = dpi_x;
        unsafe {
            GetDpiForMonitor(monitor, 0, &mut dpi_x, &mut dpi_y);
        }
        let scale = f64::from(dpi_x.max(96)) / 96.;
        let area = if self.edge == 0 {
            info.monitor
        } else {
            info.work
        };
        let (logical_width, logical_height): (f64, f64) = if self.edge >= 2 {
            (420., 860.)
        } else {
            (WINDOW_WIDTH, WINDOW_HEIGHT)
        };
        let width = ((logical_width * scale).round() as i32).min((area.right - area.left).max(1));
        let height = if self.edge >= 2 {
            // Let downward-first expansion use the complete monitor work area.
            (area.bottom - area.top).max(1)
        } else {
            ((logical_height * scale).round() as i32).min((area.bottom - area.top).max(1))
        };
        let logical_width = f64::from(width) / scale;
        let logical_height = f64::from(height) / scale;
        let along = |span: i32, size: i32| {
            ((f64::from(span) * self.ratio - f64::from(size) / 2.).round() as i32)
                .clamp(0, (span - size).max(0))
        };
        let x = if self.edge == 2 {
            area.left
        } else if self.edge == 3 {
            area.right - width
        } else {
            area.left + along(area.right - area.left, width)
        };
        let y = if self.edge >= 2 {
            area.top + along(area.bottom - area.top, height)
        } else if self.edge == 0 {
            area.top
        } else {
            area.bottom - height
        };
        let offset = if self.edge >= 2 {
            (f64::from(area.top) + f64::from(area.bottom - area.top) * self.ratio
                - f64::from(y + height / 2))
                / scale
        } else {
            (f64::from(area.left) + f64::from(area.right - area.left) * self.ratio
                - f64::from(x + width / 2))
                / scale
        };
        SCENE.with(|scene| {
            scene.set(crate::shape::Scene {
                edge: self.edge,
                offset,
                width: logical_width,
                height: logical_height,
            })
        });
        let topmost = -1_isize as Hwnd; // HWND_TOPMOST.
        if unsafe {
            SetWindowPos(
                self.window,
                topmost,
                x,
                y,
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
    fn win32_button_coordinates_precede_input_without_a_mouse_move() {
        use std::cell::RefCell;
        type Messages = RefCell<Vec<(u32, usize, isize)>>;
        unsafe extern "system" fn record(
            window: Hwnd,
            message: u32,
            wparam: usize,
            lparam: isize,
            _: usize,
            data: usize,
        ) -> isize {
            if (0x0200..=0x020d).contains(&message) {
                unsafe { &*(data as *const Messages) }
                    .borrow_mut()
                    .push((message, wparam, lparam));
            }
            unsafe { DefSubclassProc(window, message, wparam, lparam) }
        }
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
        let messages: Messages = RefCell::default();
        assert_ne!(
            unsafe {
                SetWindowSubclass(
                    handle,
                    Some(record),
                    99,
                    &messages as *const Messages as usize,
                )
            },
            0
        );
        {
            let _native = NativeWindow::new(handle as isize).unwrap();
            for message in [
                0x0201, 0x0202, 0x0203, 0x0204, 0x0205, 0x0206, 0x0207, 0x0208, 0x0209, 0x020b,
                0x020c, 0x020d,
            ] {
                messages.borrow_mut().clear();
                // Signed captured coordinates and X-button metadata must survive.
                let coordinates = (-17_i16 as u16 as isize) | (102_isize << 16);
                let flags = 0x0001_0001;
                unsafe {
                    SendMessageW(handle, message, flags, coordinates);
                }
                assert_eq!(
                    *messages.borrow(),
                    [
                        (0x0200, flags & 0xffff, coordinates),
                        (message, flags, coordinates),
                    ],
                    "move must reach the backend before button {message:#x}"
                );
            }
        }
        unsafe {
            RemoveWindowSubclass(handle, Some(record), 99);
        }
        assert_ne!(unsafe { DestroyWindow(handle) }, 0);
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
                // WS_POPUP | WS_VISIBLE. Windows can drop HWND_TOPMOST from a hidden window,
                // which made this test depend on the desktop; the real notch is visible.
                0x9000_0000,
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

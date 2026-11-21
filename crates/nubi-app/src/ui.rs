//! UI-thread ownership of Slint surfaces and native tray/keyboard resources.
use crate::{
    Fixture, NativeLauncher, NotchWindow, ParityPanel, Session, Theme,
    host::{NativeHost, UiEvent},
    motion::Motion,
    native,
    notch::Notch,
    singleton::Action,
    tray::{Shortcuts, Tray},
};
use nubi_core::host::with;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::winit_030::winit::platform::windows::WindowAttributesExtWindows;
use slint::winit_030::{EventResult, WinitWindowAccessor, winit::event::WindowEvent};
use slint::{ComponentHandle, Model, Timer, TimerMode, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::mpsc::Receiver,
    time::{Duration, Instant},
};

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod runtime_tests;
#[cfg(test)]
use crate::updater;

pub(crate) fn hwnd(window: &slint::Window) -> Option<isize> {
    let owner = window.window_handle();
    let handle = owner.window_handle().ok()?;
    if let RawWindowHandle::Win32(handle) = handle.as_raw() {
        Some(handle.hwnd.get())
    } else {
        None
    }
}

// Show requests can precede Winit's HWND creation. Once it exists, request a
// native redraw as well: Slint's coalesced request can retain the pre-show frame.
fn present_settings(weak: slint::Weak<ParityPanel>, retries: u8) {
    Timer::single_shot(Duration::from_millis(16), move || {
        let Some(panel) = weak.upgrade() else {
            return;
        };
        if !panel.window().is_visible() {
            return;
        }
        if hwnd(panel.window()).is_none() {
            if retries > 0 {
                present_settings(weak, retries - 1);
            }
            return;
        }
        foreground(panel.window());
        if let Some(handle) = hwnd(panel.window()) {
            if let Err(error) = native::repair_panel_pointer(handle) {
                eprintln!("[nubi] panel pointer: {error}");
            }
        }
        panel.window().request_redraw();
        panel
            .window()
            .with_winit_window(|window| window.request_redraw());
    });
}

/// Bounded, development-only animation-update measurements. These are event gaps,
/// not compositor presentation times or a claim of visual parity.
#[derive(Default)]
struct FrameTrace {
    last: Option<Instant>,
    gaps: Vec<f64>,
}
impl FrameTrace {
    fn frame(&mut self, now: Instant, active: bool) {
        if let Some(last) = self.last {
            self.gaps
                .push(now.duration_since(last).as_secs_f64() * 1000.);
        }
        self.last = active.then_some(now);
        if (!active || self.gaps.len() == 240) && !self.gaps.is_empty() {
            self.gaps.sort_by(f64::total_cmp);
            let count = self.gaps.len();
            eprintln!(
                "[nubi dev] animation update gaps: n={count}, median={:.2}ms, p95={:.2}ms, max={:.2}ms",
                self.gaps[count / 2],
                self.gaps[(count - 1) * 95 / 100],
                self.gaps[count - 1]
            );
            self.gaps.clear();
        }
    }
}

fn foreground(window: &slint::Window) {
    if let Some(handle) = hwnd(window) {
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::*;
            let handle = handle as _;
            ShowWindow(handle, SW_RESTORE);
            SetWindowPos(handle, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
            SetForegroundWindow(handle);
            SetWindowPos(handle, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
        }
    }
}

// HWND creation may follow show(). Position and activate once native creation finishes.
// Keep this separate from panel foreground(), which deliberately clears topmost.
fn present_runtime<T: ComponentHandle + 'static>(weak: slint::Weak<T>, retries: u8) {
    Timer::single_shot(Duration::from_millis(16), move || {
        let Some(ui) = weak.upgrade() else { return };
        let window = ui.window();
        if !window.is_visible() {
            return;
        }
        if hwnd(window).is_none() {
            if retries > 0 {
                present_runtime(weak, retries - 1);
            }
            return;
        }
        window.with_winit_window(|native| {
            use slint::winit_030::winit::{dpi, window::WindowLevel};
            let monitor = native
                .current_monitor()
                .or_else(|| native.primary_monitor());
            if let Some(monitor) = monitor {
                let scale = monitor.scale_factor();
                let origin = monitor.position();
                let size = monitor.size();
                let (width, height) = (340., 176.);
                let _ = native.request_inner_size(dpi::LogicalSize::new(width, height));
                let width = (width * scale).round() as i32;
                let height = (height * scale).round() as i32;
                let (x, y) = (
                    (size.width as i32 - width) / 2,
                    (size.height as i32 - height) / 2,
                );
                native.set_outer_position(dpi::PhysicalPosition::new(origin.x + x, origin.y + y));
            }
            native.set_window_level(WindowLevel::AlwaysOnTop);
            native.focus_window();
            native.request_redraw();
        });
        if let Some(handle) = hwnd(window) {
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow(handle as _);
            }
        }
        window.request_redraw();
    });
}

pub(crate) fn focus_notch(ui: &NotchWindow) {
    if let Some(handle) = hwnd(ui.window()) {
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow(handle as _);
        }
    }
}

pub(crate) fn apply_theme(theme: Theme<'_>, preference: &str) {
    // Offscreen fixtures select their own palette independently of the desktop.
    if cfg!(test) {
        theme.set_dark(dark(preference));
        return;
    }

    use windows_sys::Win32::Graphics::Gdi::{
        COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_WINDOW, COLOR_WINDOWTEXT, GetSysColor,
    };
    use windows_sys::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
    use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETHIGHCONTRAST, SystemParametersInfoW};

    let size = std::mem::size_of::<HIGHCONTRASTW>() as u32;
    let mut settings = HIGHCONTRASTW {
        cbSize: size,
        dwFlags: 0,
        lpszDefaultScheme: std::ptr::null_mut(),
    };
    let high_contrast = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            size,
            (&mut settings as *mut HIGHCONTRASTW).cast(),
            0,
        ) != 0
    } && settings.dwFlags & HCF_HIGHCONTRASTON != 0;
    theme.set_high_contrast(high_contrast);
    if high_contrast {
        let system_color = |index| {
            let rgb = unsafe { GetSysColor(index) };
            slint::Color::from_rgb_u8(rgb as u8, (rgb >> 8) as u8, (rgb >> 16) as u8)
        };
        let background = system_color(COLOR_WINDOW);
        theme.set_contrast_bg(background);
        theme.set_contrast_fg(system_color(COLOR_WINDOWTEXT));
        theme.set_contrast_accent(system_color(COLOR_HIGHLIGHT));
        theme.set_contrast_accent_ink(system_color(COLOR_HIGHLIGHTTEXT));
        let brightness = 299 * u32::from(background.red())
            + 587 * u32::from(background.green())
            + 114 * u32::from(background.blue());
        theme.set_dark(brightness < 128_000);
    } else {
        theme.set_dark(dark(preference));
    }
}

pub(crate) fn dark(theme: &str) -> bool {
    if theme != "system" {
        return theme != "light";
    }
    use windows_sys::Win32::System::Registry::*;
    let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
        .encode_utf16()
        .collect();
    let name: Vec<u16> = "AppsUseLightTheme\0".encode_utf16().collect();
    let mut value = 0u32;
    let mut size = 4;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut value as *mut u32).cast(),
            &mut size,
        );
    }
    value == 0
}

fn models<C: ComponentHandle>(component: &C, host: &NativeHost)
where
    for<'a> Fixture<'a>: slint::Global<'a, C>,
{
    let (sessions, percent, week, usage_text, resets, times, lines) = with(host, |state| {
        let mut rows: Vec<_> = state
            .sessions
            .values()
            .filter(|session| state.settings.wants(&session.provider))
            .collect();
        rows.sort_by_key(|session| std::cmp::Reverse(session.last_event_ms));
        let rows = rows
            .into_iter()
            .take(8)
            .map(|session| {
                let seconds = ((nubi_core::state::now_ms().saturating_sub(session.last_event_ms)
                    as f64)
                    / 1000.)
                    .round();
                let ago = if seconds < 60. {
                    format!("{seconds:.0}s")
                } else if seconds < 3600. {
                    format!("{:.0}m", (seconds / 60.).round())
                } else {
                    format!("{:.0}h", (seconds / 3600.).round())
                };
                let state = if session.state == "working" {
                    "Working".into()
                } else if session.attention {
                    "Needs you".into()
                } else {
                    let mut chars = session.state.chars();
                    chars
                        .next()
                        .map(|first| first.to_uppercase().to_string() + chars.as_str())
                        .unwrap_or_default()
                };
                Session {
                    key: session.key.as_str().into(),
                    name: if session.name.is_empty() {
                        &session.folder
                    } else {
                        &session.name
                    }
                    .as_str()
                    .into(),
                    provider: session.provider.as_str().into(),
                    state: state.into(),
                    context: session
                        .context_tokens
                        .filter(|tokens| *tokens > 0)
                        .map(|tokens| format!("{:.0}k", (tokens as f64 / 1000.).round()))
                        .unwrap_or_default()
                        .into(),
                    ago: ago.into(),
                }
            })
            .collect::<Vec<_>>();
        let mut percent = vec![0.; 2];
        let mut week = percent.clone();
        let mut usage_text = vec![slint::SharedString::from("—"); 2];
        let mut resets = vec![slint::SharedString::default(); 4];
        for (index, provider) in ["claude", "codex"].iter().enumerate() {
            if let Some(usage) = state.usage.get(*provider) {
                for (window_index, window) in usage.windows.iter().take(2).enumerate() {
                    let value = window.used_percent.clamp(0., 100.) as f32;
                    if window_index == 0 {
                        percent[index] = value;
                        usage_text[index] = format!("{value:.0}%").into();
                    } else {
                        week[index] = value;
                    }
                    if let Some(reset) = window.resets_at {
                        let seconds = reset
                            .saturating_sub(chrono::Local::now().timestamp())
                            .max(0);
                        resets[index * 2 + window_index] =
                            format!("resets in {}m", (seconds + 59) / 60).into();
                    }
                }
            }
        }
        let times = state
            .alerts
            .iter()
            .map(|alert| {
                chrono::DateTime::from_timestamp_millis(alert.ts as i64)
                    .map(|time| {
                        time.with_timezone(&chrono::Local)
                            .format("%I:%M %p")
                            .to_string()
                    })
                    .unwrap_or_default()
                    .into()
            })
            .collect::<Vec<slint::SharedString>>();
        let lines = state
            .alerts
            .iter()
            .map(|alert| format!("{} · {}", alert.title, alert.body).into())
            .collect::<Vec<slint::SharedString>>();
        (rows, percent, week, usage_text, resets, times, lines)
    });
    let fixture = component.global::<Fixture>();
    fixture.on_usage_color(|percent, soft| crate::usage_color::color(f64::from(percent), soft));
    fixture.set_sessions(Rc::new(VecModel::from(sessions)).into());
    fixture.set_session_percent(Rc::new(VecModel::from(percent)).into());
    fixture.set_week_percent(Rc::new(VecModel::from(week)).into());
    fixture.set_usage_text(Rc::new(VecModel::from(usage_text)).into());
    fixture.set_reset_labels(Rc::new(VecModel::from(resets)).into());
    fixture.set_alert_times(Rc::new(VecModel::from(times)).into());
    fixture.set_alert_lines(Rc::new(VecModel::from(lines)).into());
}

struct Surfaces {
    host: NativeHost,
    runtime: tokio::runtime::Handle,
    shortcuts: Rc<RefCell<Shortcuts>>,
    panel: Option<ParityPanel>,
    launcher: Option<NativeLauncher>,
    panel_hidden: Option<Instant>,
    panel_state: Option<Rc<RefCell<crate::panel_logic::Panel>>>,
    panel_notch: Rc<RefCell<Notch>>,
    panel_notch_ui: slint::Weak<NotchWindow>,
    alerts: crate::alerts::Alerts,
}

impl Surfaces {
    fn activate(&mut self, action: Action) -> Result<(), Box<dyn std::error::Error>> {
        match action {
            Action::Settings => self.settings()?,
            Action::Launcher => {
                // Read the foreground window before showing or focusing any Nubi surface.
                let host = self.host.clone();
                std::thread::spawn(move || {
                    let _ = host.post(UiEvent::LauncherReady(crate::launcher::focused_folder()));
                });
            }
        }
        Ok(())
    }

    fn settings(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.panel.is_none() {
            let panel = ParityPanel::new()?;
            let state = self
                .panel_state
                .get_or_insert_with(|| {
                    crate::panel_logic::Panel::new(&with(&self.host, |s| s.settings.clone()))
                })
                .clone();
            let services = crate::panel_services::Services {
                host: self.host.clone(),
                runtime: self.runtime.clone(),
                register: {
                    let shortcuts = self.shortcuts.clone();
                    Rc::new(move |settings| shortcuts.borrow_mut().apply(settings))
                },
                notch: self.panel_notch.clone(),
                notch_ui: self.panel_notch_ui.clone(),
            };
            crate::panel_logic::Panel::bind(&panel, state, services);
            self.panel = Some(panel);
        }
        let panel = self.panel.as_ref().unwrap();
        panel.show()?;
        panel.invoke_take_focus();
        if let Some(state) = &self.panel_state {
            let names = Rc::new(RefCell::new(Vec::new()));
            let collected = names.clone();
            panel.window().with_winit_window(move |window| {
                let friendly = nubi_core::winutil::monitor_names();
                *collected.borrow_mut() = window
                    .available_monitors()
                    .enumerate()
                    .map(|(i, monitor)| {
                        let name = monitor
                            .name()
                            .unwrap_or_else(|| format!("Display {}", i + 1));
                        friendly.get(&name).cloned().unwrap_or(name)
                    })
                    .collect();
            });
            state
                .borrow_mut()
                .set_monitors(panel, names.borrow().clone());
        }
        foreground(panel.window());
        present_settings(panel.as_weak(), 60);
        Ok(())
    }

    fn release_hidden(&mut self) {
        if let Some(panel) = &self.panel {
            if panel.window().is_visible() {
                self.panel_hidden = None;
            } else {
                let since = self.panel_hidden.get_or_insert_with(Instant::now);
                if since.elapsed() >= Duration::from_secs(60) {
                    if let Some(state) = &self.panel_state {
                        state.borrow_mut().retain(panel);
                    }
                    self.panel_notch.borrow_mut().release_panel_holds();
                    self.panel = None;
                    self.panel_hidden = None;
                }
            }
        }
        if self
            .launcher
            .as_ref()
            .is_some_and(|launcher| !launcher.window().is_visible())
        {
            self.launcher = None;
        }
    }

    fn launcher(
        &mut self,
        found: Result<String, String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let ui = NativeLauncher::new()?;
        let settings = with(&self.host, |state| state.settings.clone());
        apply_theme(ui.global::<Theme>(), &settings.notch_theme);
        ui.global::<Theme>()
            .set_notch_accent(settings.notch_accent.into());
        ui.global::<Theme>()
            .set_reduced(settings.reduce_motion || native::system_reduced_motion());
        let previous =
            std::fs::read_to_string(nubi_core::settings::data_dir().join("launcher-cli"))
                .unwrap_or_default();
        ui.set_selected(i32::from(previous == "codex"));
        match found {
            Ok(dir) => ui.set_dir(dir.into()),
            Err(error) => ui.set_error(error.into()),
        }
        let weak = ui.as_weak();
        ui.on_cancel(move || {
            if let Some(ui) = weak.upgrade() {
                let _ = ui.hide();
            }
        });
        let weak = ui.as_weak();
        ui.on_run(move |cli| {
            if let Some(ui) = weak.upgrade() {
                if !matches!(cli.as_str(), "claude" | "codex") || ui.get_dir().is_empty() {
                    return;
                }
                // The web launcher remembers an attempted choice even if opening fails.
                let _ = std::fs::create_dir_all(nubi_core::settings::data_dir());
                let _ = std::fs::write(
                    nubi_core::settings::data_dir().join("launcher-cli"),
                    cli.as_bytes(),
                );
                match nubi_core::session_actions::open_terminal(&ui.get_dir(), Some(&cli)) {
                    Ok(()) => {
                        let _ = ui.hide();
                    }
                    Err(error) => {
                        ui.set_dir("".into());
                        ui.set_error(error.into());
                        ui.invoke_take_focus();
                    }
                }
            }
        });
        let weak = ui.as_weak();
        let mut activated = false;
        ui.window().on_winit_window_event(move |_, event| {
            match event {
                WindowEvent::Focused(true) => activated = true,
                WindowEvent::Focused(false) if activated => {
                    if let Some(ui) = weak.upgrade() {
                        let _ = ui.hide();
                    }
                }
                _ => {}
            }
            EventResult::Propagate
        });
        // Shown Slint windows retain an internal reference until explicitly hidden.
        if let Some(previous) = self.launcher.take() {
            previous.hide()?;
        }
        ui.show()?;
        ui.invoke_take_focus();
        present_runtime(ui.as_weak(), 8);
        self.launcher = Some(ui);
        Ok(())
    }
}

pub fn run(
    host: NativeHost,
    events: Receiver<UiEvent>,
    runtime: tokio::runtime::Handle,
    action: Option<Action>,
    exit_after: Option<Duration>,
) -> Result<(), Box<dyn std::error::Error>> {
    native::enable_dpi_awareness();
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_window_attributes_hook(|attributes| {
            if attributes.title == "Nubi notch" {
                attributes
                    .with_transparent(true)
                    .with_decorations(false)
                    .with_active(false)
                    .with_skip_taskbar(true)
                    .with_undecorated_shadow(false)
                    .with_drag_and_drop(false)
            } else if attributes.title == "Nubi alert" {
                attributes
                    .with_active(false)
                    .with_transparent(true)
                    .with_decorations(false)
                    .with_skip_taskbar(true)
                    .with_undecorated_shadow(false)
                    .with_drag_and_drop(false)
            } else {
                attributes.with_skip_taskbar(true).with_drag_and_drop(false)
            }
        })
        .select()?;
    let ui = NotchWindow::new()?;
    let settings = with(&host, |state| state.settings.clone());
    apply_theme(ui.global::<Theme>(), &settings.notch_theme);
    let reduced = settings.reduce_motion || native::system_reduced_motion();
    ui.global::<Theme>().set_reduced(reduced);
    models(&ui, &host);
    let notch = Notch::connect(&ui, host.clone());
    let weak = ui.as_weak();
    ui.global::<crate::NotchData>().on_focus_input(move || {
        if let Some(ui) = weak.upgrade() {
            focus_notch(&ui);
        }
    });
    crate::notch::connect_update(&ui, host.clone(), runtime.clone());
    let motion = Rc::new(RefCell::new(Motion::new()));
    motion.borrow().draw(&ui);
    let frame_motion = motion.clone();
    let weak = ui.as_weak();
    ui.on_tab_geometry(move |x, width| {
        if let Some(ui) = weak.upgrade() {
            frame_motion.borrow_mut().tab_target(
                x.into(),
                width.into(),
                ui.global::<Theme>().get_reduced(),
            );
            ui.window().request_redraw();
        }
    });
    let animated = Rc::new(Cell::new(false));
    let frame_animated = animated.clone();
    let frame_motion = motion.clone();
    let weak = ui.as_weak();
    let animation_start = Instant::now();
    let trace =
        nubi_core::isolation::development() && std::env::var_os("NUBI_TRACE_FRAMES").is_some();
    let mut frame_trace = FrameTrace::default();
    let frame_timer = Timer::default();
    let mut next_frame = Instant::now();
    let mut traced_monitor = false;
    ui.window().on_winit_window_event(move |_, event| {
        if trace && matches!(event, WindowEvent::MouseInput { .. }) {
            eprintln!("[nubi dev] pointer: {event:?}");
        }
        if matches!(event, WindowEvent::RedrawRequested) {
            let Some(ui) = weak.upgrade() else {
                return EventResult::Propagate;
            };
            let now = Instant::now();
            let refresh = ui
                .window()
                .with_winit_window(|window| {
                    window
                        .current_monitor()
                        .and_then(|monitor| monitor.refresh_rate_millihertz())
                })
                .flatten()
                .filter(|rate| *rate > 0)
                .unwrap_or(60_000);
            let interval = Duration::from_secs_f64(1000. / f64::from(refresh));
            if trace && !traced_monitor {
                eprintln!("[nubi dev] monitor refresh: {refresh}mHz");
                traced_monitor = true;
            }
            let due = now >= next_frame;
            let mut motion = frame_motion.borrow_mut();
            let loops = frame_animated.get() && !ui.global::<Theme>().get_reduced();
            if due {
                let following = next_frame + interval;
                next_frame = if following > now {
                    following
                } else {
                    now + interval
                };
                if motion.tick(now) {
                    motion.draw(&ui);
                }
                if loops {
                    let elapsed = animation_start.elapsed().as_secs_f64();
                    ui.set_phase((elapsed % 1.1 / 1.1) as f32);
                    ui.set_pulse_phase((elapsed % 1.4 / 1.4) as f32);
                    ui.set_refresh_phase((elapsed % 0.7 / 0.7) as f32);
                    ui.set_equalizer_phase((elapsed % 1.) as f32);
                }
            }
            let active = motion.active() || loops;
            if trace && (due || !active) {
                frame_trace.frame(now, active);
            }
            if active && !frame_timer.running() {
                // Software presentation can generate extra redraw events.
                // Limit spring updates to the monitor rate and schedule one
                // wakeup, without adding Slint's second redraw throttle.
                let weak = weak.clone();
                // Round up for Slint's millisecond timer clock. Retaining the
                // absolute deadline avoids accumulating a 17 ms / 9 ms drift.
                let delay = next_frame.saturating_duration_since(now);
                let delay = Duration::from_millis(delay.as_micros().div_ceil(1000).max(1) as u64);
                frame_timer.start(TimerMode::SingleShot, delay, move || {
                    if let Some(ui) = weak.upgrade() {
                        ui.window()
                            .with_winit_window(|window| window.request_redraw());
                    }
                });
            } else if !active {
                frame_timer.stop();
            }
        }
        EventResult::Propagate
    });
    let shortcuts = Rc::new(RefCell::new(Shortcuts::new()?));
    if let Err(error) = shortcuts.borrow_mut().apply(&settings) {
        eprintln!("[nubi] shortcuts: {error}");
    }
    let surfaces = Rc::new(RefCell::new(Surfaces {
        host: host.clone(),
        runtime,
        shortcuts: shortcuts.clone(),
        panel: None,
        launcher: None,
        panel_hidden: None,
        panel_state: None,
        panel_notch: notch.clone(),
        panel_notch_ui: ui.as_weak(),
        alerts: crate::alerts::Alerts::new(host.clone()),
    }));
    let surfaces_callback = surfaces.clone();
    ui.on_open_panel(move || {
        let mut surfaces = surfaces_callback.borrow_mut();
        if let Err(error) = surfaces.settings() {
            eprintln!("[nubi] settings: {error}");
        }
        if let Some(panel) = &surfaces.panel {
            panel.invoke_navigate(0);
        }
    });
    let callback_host = host.clone();
    ui.on_toggle_dnd(move || callback_host.toggle_dnd());
    ui.window()
        .on_close_requested(|| slint::CloseRequestResponse::KeepWindowShown);
    ui.show()?;
    let tray = Tray::new(&host)?;
    let mut native_window: Option<native::NativeWindow> = None;
    let mut inside = false;
    let mut focused = false;
    let mut hover = None;
    let mut last_refresh = Instant::now();
    let start = Instant::now();
    let weak = ui.as_weak();
    let timer = Timer::default();
    let mut initial = action;
    timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
        let Some(ui) = weak.upgrade() else {
            return;
        };
        if native_window.is_none() {
            if let Some(handle) = hwnd(ui.window()) {
                match native::NativeWindow::new(handle) {
                    Ok(window) => {
                        native_window = Some(window);
                        ui.set_ready(true);
                    }
                    Err(error) => {
                        eprintln!("[nubi] native window: {error}");
                        let _ = slint::quit_event_loop();
                        return;
                    }
                }
            }
        }
        if let Some(action) = initial.take() {
            let _ = surfaces.borrow_mut().activate(action);
        }
        for event in events.try_iter().take(64) {
            let result = match event {
                UiEvent::Activate(action) => surfaces.borrow_mut().activate(action),
                UiEvent::LauncherReady(found) => surfaces.borrow_mut().launcher(found),
                UiEvent::Alert(alert) => {
                    if with(&host, |state| state.settings.sounds) {
                        crate::sounds::chime(&alert.kind);
                    }
                    notch.borrow_mut().alert(alert);
                    Ok(())
                }
                UiEvent::OverlayAlert(alert) => {
                    if with(&host, |state| state.settings.sounds) {
                        crate::sounds::overlay_chime(&alert.kind);
                    }
                    surfaces.borrow_mut().alerts.push(alert);
                    Ok(())
                }
                UiEvent::Update => {
                    notch.borrow_mut().update_notice(&ui);
                    Ok(())
                }
            };
            if let Err(error) = result {
                eprintln!("[nubi] UI: {error}");
            }
        }
        while let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            match event.id.0.as_str() {
                "open" => {
                    let _ = surfaces.borrow_mut().settings();
                }
                "quit" => {
                    let _ = slint::quit_event_loop();
                }
                "refresh" => nubi_core::poller::force(&host),
                "dnd" => host.toggle_dnd(),
                "sounds" => {
                    let mut settings = with(&host, |state| state.settings.clone());
                    settings.sounds = !settings.sounds;
                    if let Err(error) = host.save_settings(settings) {
                        eprintln!("[nubi] settings: {error}");
                    }
                }
                _ => {}
            }
        }
        while let Ok(event) = tray_icon::TrayIconEvent::receiver().try_recv() {
            if matches!(
                event,
                tray_icon::TrayIconEvent::Click {
                    button: tray_icon::MouseButton::Left,
                    button_state: tray_icon::MouseButtonState::Up,
                    ..
                }
            ) {
                let _ = surfaces.borrow_mut().settings();
            }
        }
        while let Ok(event) = global_hotkey::GlobalHotKeyEvent::receiver().try_recv() {
            if event.state == global_hotkey::HotKeyState::Pressed {
                if Some(event.id) == shortcuts.borrow().launch {
                    let _ = surfaces.borrow_mut().activate(Action::Launcher);
                } else {
                    host.toggle_dnd();
                }
            }
        }
        let now = Instant::now();
        if let Some(native) = &mut native_window {
            let active = native.focused();
            if focused && !active {
                ui.invoke_focus_requests();
                ui.global::<crate::NotchData>()
                    .set_focused_request("".into());
                ui.global::<crate::NotchData>().set_input_focused(false);
                ui.set_menu_key("".into());
                notch.borrow().release_holds();
            }
            focused = active;
            let settings = with(&host, |state| state.settings.clone());
            if let Ok(place) = native.configure(&settings) {
                if place.preferences_changed {
                    apply_theme(ui.global::<Theme>(), &settings.notch_theme);
                    ui.global::<Theme>()
                        .set_reduced(settings.reduce_motion || native::system_reduced_motion());
                }
                ui.set_offset(place.offset as f32);
                ui.global::<Theme>().set_scale(place.scale as f32);
                let size = slint::PhysicalSize::new(place.width, place.height);
                if ui.window().size() != size {
                    ui.window().set_size(size);
                    motion.borrow().draw(&ui);
                }
            }
            if notch.borrow_mut().drag_requested {
                notch.borrow_mut().drag_requested = false;
                native.start_drag();
            }
            if let Some((ratio, ended, moved)) = native.drag() {
                notch.borrow_mut().dragging = moved && !ended;
                if moved {
                    with(&host, |state| state.settings.notch_along = ratio);
                    if ended {
                        let settings = with(&host, |state| state.settings.clone());
                        let _ = nubi_core::settings::save(&settings);
                    }
                }
            }
            if let Some(current) = native.pointer_inside() {
                native.click_through(!current);
                if current != inside {
                    inside = current;
                    hover = Some((now, current));
                }
            }
        }
        let mut motion = motion.borrow_mut();
        if let Some((since, entering)) = hover {
            if now.duration_since(since) >= Duration::from_millis(if entering { 40 } else { 140 }) {
                notch.borrow_mut().pointer(entering, &ui);
                hover = None;
            }
        }
        let (view, target) = notch.borrow_mut().view(&ui);
        let was_active = motion.active();
        motion.target(view, target, ui.global::<Theme>().get_reduced());
        if motion.active() && !was_active {
            ui.window().request_redraw();
        }
        drop(motion);
        let loops = ui
            .global::<crate::NotchData>()
            .get_providers()
            .iter()
            .any(|provider| !provider.activity.is_empty())
            || ui.global::<crate::NotchData>().get_refreshing()
            || ui.global::<crate::NotchData>().get_desk_loading();
        let was_animated = animated.replace(loops && !ui.global::<Theme>().get_reduced());
        if animated.get() && !was_animated {
            ui.window().request_redraw();
        }
        let dirty = host.take_dirty();
        if dirty
            || ((ui.get_opened()
                || surfaces
                    .borrow()
                    .panel
                    .as_ref()
                    .is_some_and(|p| p.window().is_visible()))
                && last_refresh.elapsed() >= Duration::from_secs(1))
        {
            last_refresh = now;
            models(&ui, &host);
            notch.borrow_mut().update(&ui);
            let view = surfaces.borrow();
            if let Some(panel) = &view.panel {
                if let Some(state) = &view.panel_state {
                    state.borrow_mut().tick(panel, &host, &notch);
                }
            }
            tray.refresh(&host);
            let settings = with(&host, |state| state.settings.clone());
            if let Err(error) = shortcuts.borrow_mut().apply(&settings) {
                eprintln!("[nubi] shortcuts: {error}");
            }
            drop(view);
        }
        // Notices are replaceable state: recover coalesced wakeups and track card visibility.
        notch.borrow_mut().update_notice(&ui);
        if let Err(error) = surfaces.borrow_mut().alerts.tick() {
            eprintln!("[nubi] alert overlay: {error}");
        }
        surfaces.borrow_mut().release_hidden();
        if exit_after.is_some_and(|duration| start.elapsed() >= duration) {
            let _ = slint::quit_event_loop();
        }
    });
    slint::run_event_loop()?;
    timer.stop();
    drop(timer);
    Ok(())
}

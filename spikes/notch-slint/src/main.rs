#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(target_os = "windows"))]
compile_error!("This spike evaluates Windows notch behavior; build on Windows.");
#[cfg(not(any(feature = "femtovg", feature = "software")))]
compile_error!("Enable femtovg or software.");

mod native;
mod shape;
mod spring;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::winit_030::winit::platform::windows::WindowAttributesExtWindows;
use slint::{ComponentHandle, ModelRc, Timer, TimerMode, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

slint::include_modules!();

#[derive(Default)]
struct Options {
    renderer: Option<String>,
    theme: Option<String>,
    state: Option<bool>,
    reduced: bool,
    exit_after: Option<Duration>,
}

fn options() -> Result<Options, Box<dyn std::error::Error>> {
    let mut options = Options::default();
    for arg in std::env::args().skip(1) {
        if let Some(renderer) = arg.strip_prefix("--renderer=") {
            match renderer {
                "femtovg" if cfg!(feature = "femtovg") => {}
                "software" if cfg!(feature = "software") => {}
                _ => return Err(format!("renderer unavailable in this build: {renderer}").into()),
            }
            options.renderer = Some(renderer.to_string());
        } else if let Some(theme) = arg.strip_prefix("--theme=") {
            if !matches!(theme, "light" | "dark") {
                return Err("theme must be light or dark".into());
            }
            options.theme = Some(theme.to_string());
        } else if let Some(state) = arg.strip_prefix("--state=") {
            options.state = Some(match state {
                "open" => true,
                "closed" => false,
                _ => return Err("state must be open or closed".into()),
            });
        } else if arg == "--reduce-motion" {
            options.reduced = true;
        } else if let Some(seconds) = arg.strip_prefix("--exit-after=") {
            options.exit_after = Some(Duration::from_secs(seconds.parse()?));
        } else {
            return Err(format!("unknown argument: {arg}").into());
        }
    }
    Ok(options)
}

struct Motion {
    springs: [spring::Spring; 4],
    opened: bool,
    moving: bool,
    last: Instant,
}

impl Motion {
    fn new(opened: bool) -> Self {
        let values = if opened { shape::OPENED } else { shape::CLOSED };
        Self {
            springs: values.map(spring::Spring::new),
            opened,
            moving: false,
            last: Instant::now(),
        }
    }

    fn target(&mut self, opened: bool, reduced: bool) {
        if opened == self.opened {
            return;
        }
        let target = if opened { shape::OPENED } else { shape::CLOSED };
        let growing = target[0] * target[1] > self.springs[0].target * self.springs[1].target + 1.0;
        let preset = if growing { spring::OPEN } else { spring::CLOSE };
        for (spring, value) in self.springs.iter_mut().zip(target) {
            spring.to(value, preset);
            if reduced {
                spring.jump();
            }
        }
        // motion.js kick() preserves the running frame clock during a retarget.
        if !self.moving {
            self.last = Instant::now();
        }
        self.opened = opened;
        self.moving = true;
    }

    fn tick(&mut self, now: Instant) -> bool {
        if !self.moving {
            return false;
        }
        let dt = now.saturating_duration_since(self.last).as_secs_f64();
        self.last = now;
        self.moving = false;
        for spring in &mut self.springs {
            self.moving |= spring.step(dt);
        }
        true
    }

    fn draw(&self, ui: &NotchWindow) {
        let values = self.springs.map(|s| s.x);
        ui.set_along(values[0] as f32);
        ui.set_depth(values[1] as f32);
        ui.set_corner(values[2] as f32);
        ui.set_ear(values[3] as f32);
        ui.set_opened(self.opened);
        native::set_shape(shape::Shape(values));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = options()?;
    native::enable_dpi_awareness();
    let renderer = options.renderer.unwrap_or_else(|| {
        if cfg!(feature = "femtovg") {
            "femtovg"
        } else {
            "software"
        }
        .into()
    });
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name(renderer)
        .with_winit_window_attributes_hook(|attributes| {
            attributes
                .with_transparent(true)
                .with_decorations(false)
                .with_active(false)
                .with_skip_taskbar(true)
                .with_undecorated_shadow(false)
                .with_drag_and_drop(false)
        })
        .select()?;

    let ui = NotchWindow::new()?;
    ui.global::<Theme>()
        .set_dark(options.theme.as_deref() != Some("light"));
    let rows = vec![
        Session {
            name: "G · native notch".into(),
            provider: "claude".into(),
            state: "Done".into(),
            context: "48k".into(),
            ago: "2m".into(),
        },
        Session {
            name: "G · toolkit spike".into(),
            provider: "codex".into(),
            state: "Idle".into(),
            context: "32k".into(),
            ago: "5m".into(),
        },
        Session {
            name: "design-system".into(),
            provider: "claude".into(),
            state: "Done".into(),
            context: "16k".into(),
            ago: "12m".into(),
        },
    ];
    ui.set_sessions(ModelRc::from(Rc::new(VecModel::from(rows))));

    let motion = Rc::new(RefCell::new(Motion::new(options.state.unwrap_or(false))));
    motion.borrow().draw(&ui);
    let reduced = options.reduced || native::system_reduced_motion();
    let forced_state = options.state;
    {
        let motion = motion.clone();
        ui.on_pill_clicked(move || {
            if forced_state.is_none() {
                motion.borrow_mut().target(true, reduced);
            }
        });
    }
    ui.show()?;

    let timer = Timer::default();
    let ui_weak = ui.as_weak();
    // winit 0.30 can defer native window creation until the event loop starts.
    // Keep content transparent until native placement/styles have been installed.
    let mut native_window: Option<native::NativeWindow> = None;
    let fatal_error = Rc::new(RefCell::new(None));
    let tick_error = fatal_error.clone();
    let tick_motion = motion.clone();
    let mut inside = false;
    let mut hover = None;
    let mut placement_error_reported = false;
    // Same 16ms cursor cadence and 40/140ms dwell as today's notch watcher.
    // No property invalidation or repaint once the springs settle.
    timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
        let Some(ui) = ui_weak.upgrade() else {
            return;
        };
        let now = Instant::now();
        if native_window.is_none() {
            let owner = ui.window().window_handle();
            let Ok(handle) = owner.window_handle() else {
                return;
            };
            let RawWindowHandle::Win32(handle) = handle.as_raw() else {
                return;
            };
            match native::NativeWindow::new(handle.hwnd.get()) {
                Ok(window) => {
                    native_window = Some(window);
                    ui.set_ready(true);
                }
                Err(error) => {
                    *tick_error.borrow_mut() = Some(error);
                    let _ = slint::quit_event_loop();
                    return;
                }
            }
        }
        let native = native_window.as_mut().unwrap();
        if let Err(error) = native.refresh_placement()
            && !placement_error_reported
        {
            eprintln!("notch placement failed: {error}");
            placement_error_reported = true;
        }
        if let Some(current) = native.pointer_inside() {
            native.click_through(!current);
            if current != inside {
                inside = current;
                hover = Some((now, current));
            }
        }
        let mut motion = tick_motion.borrow_mut();
        if let Some((start, entering)) = hover {
            let dwell = Duration::from_millis(if entering { 40 } else { 140 });
            if now.duration_since(start) >= dwell {
                if forced_state.is_none() {
                    motion.target(entering, reduced);
                }
                hover = None;
            }
        }
        if motion.tick(now) {
            motion.draw(&ui);
        }
    });
    let exit_timer = Timer::default();
    if let Some(duration) = options.exit_after {
        exit_timer.start(TimerMode::SingleShot, duration, || {
            let _ = slint::quit_event_loop();
        });
    }
    let result = slint::run_event_loop();
    timer.stop();
    drop(timer);
    result?;
    if let Some(error) = fatal_error.borrow_mut().take() {
        return Err(error.into());
    }
    Ok(())
}

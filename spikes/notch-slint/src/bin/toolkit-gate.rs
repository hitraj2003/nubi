#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[path = "../native.rs"]
mod native;
#[path = "../shape.rs"]
#[allow(dead_code)] // The gate shares hit testing but holds the notch closed.
mod shape;
#[cfg(test)]
#[path = "../toolkit_tests.rs"]
mod toolkit_tests;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::winit_030::{WinitWindowAccessor, winit::platform::windows::WindowAttributesExtWindows};
use slint::{ComponentHandle, Timer, TimerMode};
use std::{cell::RefCell, rc::Rc, time::Duration};

slint::include_modules!();

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if !bytes
        .iter()
        .enumerate()
        .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return false;
    }
    let year: u32 = value[..4].parse().unwrap_or(0);
    let month: u32 = value[5..7].parse().unwrap_or(0);
    let day: u32 = value[8..].parse().unwrap_or(0);
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let max = match month {
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => 0,
    };
    year > 0 && day > 0 && day <= max
}

fn valid_time(value: &str) -> bool {
    let b = value.as_bytes();
    b.len() == 5
        && b[2] == b':'
        && [b[0], b[1], b[3], b[4]].iter().all(u8::is_ascii_digit)
        && value[..2].parse::<u32>().is_ok_and(|h| h < 24)
        && value[3..].parse::<u32>().is_ok_and(|m| m < 60)
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetForegroundWindow() -> *mut std::ffi::c_void;
    fn SetForegroundWindow(window: *mut std::ffi::c_void) -> i32;
    fn IsWindow(window: *mut std::ffi::c_void) -> i32;
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut surface = String::from("panel");
    let mut dark = true;
    let mut exit_after = None;
    for arg in std::env::args().skip(1) {
        if let Some(value) = arg.strip_prefix("--surface=") {
            if !matches!(value, "panel" | "approval" | "about") {
                return Err("surface must be panel, approval, or about".into());
            }
            surface = value.into();
        } else if let Some(value) = arg.strip_prefix("--theme=") {
            dark = match value {
                "dark" => true,
                "light" => false,
                _ => return Err("theme must be dark or light".into()),
            };
        } else if let Some(value) = arg.strip_prefix("--exit-after=") {
            exit_after = Some(Duration::from_secs(value.parse()?));
        } else {
            return Err(format!("unknown argument: {arg}").into());
        }
    }
    native::enable_dpi_awareness();
    // No new motion; querying the setting also keeps the shared native boundary exercised.
    let _reduced_motion = native::system_reduced_motion();
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_window_attributes_hook(|attributes| {
            let notch = attributes.title.contains("Slint spike");
            let attributes = attributes.with_active(false).with_drag_and_drop(false);
            if notch {
                attributes
                    .with_transparent(true)
                    .with_decorations(false)
                    .with_skip_taskbar(true)
                    .with_undecorated_shadow(false)
            } else {
                attributes
            }
        })
        .select()?;

    let previous_foreground = Rc::new(RefCell::new(unsafe { GetForegroundWindow() }));
    let notch = NotchWindow::new()?;
    let panel = ToolkitWindow::new()?;
    let approval = ApprovalWindow::new()?;
    for theme in [
        notch.global::<Theme>(),
        panel.global::<Theme>(),
        approval.global::<Theme>(),
    ] {
        theme.set_dark(dark);
    }
    panel.set_page(i32::from(surface == "about"));
    panel.invoke_set_theme(dark);
    panel.on_validate_date(|v| valid_date(v.as_str()));
    panel.on_validate_time(|v| valid_time(v.as_str()));
    {
        let notch = notch.as_weak();
        let approval = approval.as_weak();
        panel.on_theme_changed(move |dark| {
            if let Some(ui) = notch.upgrade() {
                ui.global::<Theme>().set_dark(dark);
            }
            if let Some(ui) = approval.upgrade() {
                ui.global::<Theme>().set_dark(dark);
            }
        });
    }
    let focus_pending = Rc::new(RefCell::new(Some(surface == "approval")));
    {
        let weak = approval.as_weak();
        let pending = focus_pending.clone();
        let previous = previous_foreground.clone();
        panel.on_approval(move || {
            *previous.borrow_mut() = unsafe { GetForegroundWindow() };
            if let Some(ui) = weak.upgrade() {
                ui.set_status("Waiting for your decision".into());
                if ui.show().is_ok() {
                    *pending.borrow_mut() = Some(true);
                }
            }
        });
    }
    {
        let card = approval.as_weak();
        let owner = panel.as_weak();
        let previous = previous_foreground.clone();
        approval.on_decision(move |decision| {
            if let Some(ui) = card.upgrade() {
                let _ = ui.hide();
            }
            if let Some(ui) = owner.upgrade() {
                ui.set_status(format!("Approval: {decision} (fake request)").into());
                if ui.window().is_visible() {
                    ui.window()
                        .with_winit_window(|window| window.focus_window());
                    ui.invoke_restore_approval_focus();
                    return;
                }
            }
            let window = *previous.borrow();
            // Restore only a still-live window; never activate the passive notch.
            if unsafe { IsWindow(window) } != 0 {
                unsafe {
                    SetForegroundWindow(window);
                }
            }
        });
    }
    notch.show()?;
    if surface == "approval" {
        approval.show()?;
    } else {
        panel.show()?;
    }
    // Closing the last interactive surface must also release the passive notch.
    panel.window().on_close_requested(|| {
        let _ = slint::quit_event_loop();
        slint::CloseRequestResponse::HideWindow
    });
    // Closing an approval follows the same denial and focus-restore path as Escape.
    {
        let weak = approval.as_weak();
        approval.window().on_close_requested(move || {
            if let Some(ui) = weak.upgrade() {
                ui.invoke_decision("deny".into());
            }
            slint::CloseRequestResponse::HideWindow
        });
    }

    let timer = Timer::default();
    let weak_notch = notch.as_weak();
    let weak_panel = panel.as_weak();
    let weak_approval = approval.as_weak();
    let failure = Rc::new(RefCell::new(None));
    let tick_failure = failure.clone();
    let mut passive: Option<native::NativeWindow> = None;
    native::set_shape(shape::Shape(shape::CLOSED));
    timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
        let Some(ui) = weak_notch.upgrade() else {
            return;
        };
        if passive.is_none() {
            let owner = ui.window().window_handle();
            let Ok(handle) = owner.window_handle() else {
                return;
            };
            let RawWindowHandle::Win32(handle) = handle.as_raw() else {
                return;
            };
            match native::NativeWindow::new(handle.hwnd.get()) {
                Ok(window) => {
                    passive = Some(window);
                    ui.set_ready(true);
                }
                Err(error) => {
                    *tick_failure.borrow_mut() = Some(error);
                    let _ = slint::quit_event_loop();
                    return;
                }
            }
        }
        let window = passive.as_mut().unwrap();
        if let Err(error) = window.refresh_placement() {
            *tick_failure.borrow_mut() = Some(error);
            let _ = slint::quit_event_loop();
            return;
        }
        if let Some(inside) = window.pointer_inside() {
            window.click_through(!inside);
        }
        if native::take_exit_requested() {
            let _ = slint::quit_event_loop();
        }
        let pending = *focus_pending.borrow();
        if let Some(card) = pending {
            let ready = if card {
                weak_approval.upgrade().is_some_and(|ui| {
                    let ready = ui
                        .window()
                        .with_winit_window(|window| window.focus_window())
                        .is_some();
                    if ready {
                        ui.invoke_take_focus();
                    }
                    ready
                })
            } else {
                weak_panel.upgrade().is_some_and(|ui| {
                    let ready = ui
                        .window()
                        .with_winit_window(|window| window.focus_window())
                        .is_some();
                    if ready {
                        ui.invoke_take_focus();
                    }
                    ready
                })
            };
            if ready {
                *focus_pending.borrow_mut() = None;
            }
        }
    });
    let exit_timer = Timer::default();
    if let Some(duration) = exit_after {
        exit_timer.start(TimerMode::SingleShot, duration, || {
            let _ = slint::quit_event_loop();
        });
    }
    slint::run_event_loop()?;
    timer.stop();
    if let Some(error) = failure.borrow_mut().take() {
        return Err(error.into());
    }
    Ok(())
}

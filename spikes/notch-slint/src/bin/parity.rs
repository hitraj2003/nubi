#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{Platform, PlatformError, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, Timer, TimerMode};
use std::{cell::RefCell, path::Path, rc::Rc, time::Duration};

slint::include_modules!();
#[path = "../overlay_logic.rs"]
mod overlay_logic;
#[path = "../panel_logic.rs"]
mod panel_logic;

type AdapterSlot = Rc<RefCell<Option<Rc<MinimalSoftwareWindow>>>>;
struct Offscreen(AdapterSlot);
impl Platform for Offscreen {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let adapter = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        *self.0.borrow_mut() = Some(adapter.clone());
        Ok(adapter)
    }
}

fn bitmap(
    path: &Path,
    pixels: &[PremultipliedRgbaColor],
    width: usize,
    height: usize,
    background: u16,
) -> std::io::Result<()> {
    let stride = (width * 3 + 3) & !3;
    let mut bytes = vec![0; 54 + stride * height];
    bytes[..2].copy_from_slice(b"BM");
    let length = bytes.len() as u32;
    bytes[2..6].copy_from_slice(&length.to_le_bytes());
    bytes[10..14].copy_from_slice(&54_u32.to_le_bytes());
    bytes[14..18].copy_from_slice(&40_u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&(width as i32).to_le_bytes());
    bytes[22..26].copy_from_slice(&(height as i32).to_le_bytes());
    bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&24_u16.to_le_bytes());
    for (index, pixel) in pixels.iter().enumerate() {
        let offset = 54 + (height - 1 - index / width) * stride + (index % width) * 3;
        let backdrop = (255 - u16::from(pixel.alpha)) * background / 255;
        bytes[offset..offset + 3].copy_from_slice(&[
            (u16::from(pixel.blue) + backdrop) as u8,
            (u16::from(pixel.green) + backdrop) as u8,
            (u16::from(pixel.red) + backdrop) as u8,
        ]);
    }
    std::fs::write(path, bytes)
}

fn render(
    adapter: &MinimalSoftwareWindow,
    window: &slint::Window,
    name: &str,
    scale: f32,
    size: (u32, u32),
    dark: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    window.show()?;
    adapter.dispatch_event(WindowEvent::ScaleFactorChanged {
        scale_factor: scale,
    });
    let width = (size.0 as f32 * scale).round() as usize;
    let height = (size.1 as f32 * scale).round() as usize;
    adapter.set_size(slint::PhysicalSize::new(width as u32, height as u32));
    // Settle declared colour/position transitions without requiring a desktop.
    slint::platform::update_timers_and_animations();
    let mut pixels = vec![PremultipliedRgbaColor::default(); width * height];
    adapter.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, width);
    });
    let theme = if dark { "dark" } else { "light" };
    let filename = format!("target/parity/{name}-{theme}-{}.bmp", (scale * 100.) as u32);
    bitmap(
        Path::new(&filename),
        &pixels,
        width,
        height,
        if dark {
            18
        } else if name.starts_with("overlay") {
            96
        } else {
            255
        },
    )?;
    window.hide()?;
    Ok(())
}

fn render_all(overlays_only: bool) -> Result<(), Box<dyn std::error::Error>> {
    let slot = AdapterSlot::default();
    slint::platform::set_platform(Box::new(Offscreen(slot.clone())))?;
    std::fs::create_dir_all("target/parity")?;
    for dark in [true, false] {
        for scale in [1.0, 1.5] {
            for (name, fixture) in [
                ("overlay-approval", "approval"),
                ("overlay-parameters", "parameters"),
                ("overlay-expanded", "parameters"),
                ("overlay-question", "question"),
                ("overlay-multi", "question"),
                ("overlay-custom", "question"),
            ] {
                let ui = ParityOverlay::new()?;
                ui.global::<Theme>().set_dark(dark);
                ui.global::<Theme>().set_reduced(true);
                ui.set_frozen(true);
                overlay_logic::connect(&ui, fixture);
                ui.set_expanded(name == "overlay-expanded");
                if name == "overlay-multi" {
                    ui.invoke_choose(0);
                    ui.invoke_choose(0);
                }
                if name == "overlay-custom" {
                    ui.set_custom("Claude Code".into());
                    ui.invoke_typed("Claude Code".into());
                }
                ui.invoke_settle_focus();
                let adapter = slot.borrow().as_ref().unwrap().clone();
                render(&adapter, ui.window(), name, scale, (720, 700), dark)?;
            }
            if overlays_only {
                continue;
            }
            for (name, opened, tab, depth) in [
                ("notch-closed", false, 1, 44.),
                ("notch-sessions", true, 1, 188.),
                ("notch-usage", true, 2, 180.),
            ] {
                let ui = NotchWindow::new()?;
                ui.global::<Theme>().set_dark(dark);
                ui.set_ready(true);
                ui.set_opened(opened);
                ui.set_tab(tab);
                ui.set_along(if opened { 620. } else { 145. });
                ui.set_depth(depth);
                ui.set_corner(if opened { 32. } else { 16. });
                ui.set_ear(if opened { 20. } else { 8. });
                let adapter = slot.borrow().as_ref().unwrap().clone();
                render(&adapter, ui.window(), name, scale, (720, 700), dark)?;
            }
            for (name, page, pane) in [
                ("panel-inbox", 0, 0),
                ("panel-sessions", 1, 0),
                ("panel-usage", 2, 0),
                ("panel-alerts", 3, 0),
                ("settings-general", 4, 0),
                ("settings-about", 4, 6),
            ] {
                let ui = ParityPanel::new()?;
                panel_logic::connect(&ui);
                ui.global::<Theme>().set_dark(dark);
                ui.set_page(page);
                ui.set_pane(pane);
                let adapter = slot.borrow().as_ref().unwrap().clone();
                render(&adapter, ui.window(), name, scale, (1100, 760), dark)?;
            }
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments.iter().any(|arg| arg == "--render") {
        return render_all(arguments.iter().any(|arg| arg == "--overlays-only"));
    }
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .select()?;
    if let Some(fixture) = arguments
        .iter()
        .find_map(|arg| arg.strip_prefix("--overlay="))
    {
        let ui = ParityOverlay::new()?;
        overlay_logic::connect(&ui, fixture);
        ui.global::<Theme>()
            .set_dark(!arguments.iter().any(|arg| arg == "--theme=light"));
        ui.global::<Theme>()
            .set_reduced(arguments.iter().any(|arg| arg == "--reduce-motion"));
        ui.show()?;
        ui.invoke_take_focus();
        let weak = ui.as_weak();
        ui.window().on_close_requested(move || {
            if let Some(ui) = weak.upgrade() {
                ui.invoke_decide(
                    if ui.get_question() {
                        "terminal"
                    } else {
                        "deny"
                    }
                    .into(),
                );
            }
            slint::CloseRequestResponse::HideWindow
        });
        return Ok(slint::run_event_loop()?);
    }
    let ui = ParityPanel::new()?;
    panel_logic::connect(&ui);
    ui.global::<Theme>()
        .set_dark(!arguments.iter().any(|arg| arg == "--theme=light"));
    for argument in &arguments {
        if let Some(page) = argument.strip_prefix("--page=") {
            ui.set_page(match page {
                "inbox" => 0,
                "sessions" => 1,
                "usage" => 2,
                "alerts" => 3,
                "general" => 4,
                "about" => {
                    ui.set_pane(6);
                    4
                }
                _ => return Err(format!("unknown page: {page}").into()),
            });
        }
    }
    let timer = Timer::default();
    if let Some(seconds) = arguments
        .iter()
        .find_map(|arg| arg.strip_prefix("--exit-after="))
    {
        timer.start(
            TimerMode::SingleShot,
            Duration::from_secs(seconds.parse()?),
            || {
                let _ = slint::quit_event_loop();
            },
        );
    }
    ui.run()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(window: &slint::Window, text: impl Into<slint::SharedString>) {
        let text = text.into();
        window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        window.dispatch_event(WindowEvent::KeyReleased { text });
    }

    #[test]
    fn overlay_keyboard_questions_and_clock() {
        use slint::Model;
        use slint::platform::Key;
        let slot = AdapterSlot::default();
        slint::platform::set_platform(Box::new(Offscreen(slot.clone()))).unwrap();
        let draw = |ui: &ParityOverlay| {
            ui.show().unwrap();
            ui.window()
                .dispatch_event(WindowEvent::WindowActiveChanged(true));
            let adapter = slot.borrow().as_ref().unwrap().clone();
            adapter.set_size(slint::PhysicalSize::new(720, 700));
            let mut pixels = vec![PremultipliedRgbaColor::default(); 720 * 700];
            adapter.draw_if_needed(|renderer| {
                renderer.render(&mut pixels, 720);
            });
        };
        for dark in [true, false] {
            let ui = ParityOverlay::new().unwrap();
            ui.global::<Theme>().set_dark(dark);
            ui.global::<Theme>().set_reduced(true);
            ui.set_frozen(true);
            overlay_logic::connect(&ui, "parameters");
            draw(&ui);
            ui.invoke_take_focus();
            assert_eq!(ui.get_focus_index(), 2, "Initial Deny focus");
            key(ui.window(), Key::Tab);
            assert_eq!(ui.get_focus_index(), 3);
            key(ui.window(), Key::Tab);
            assert_eq!(ui.get_focus_index(), 0, "Tab wraps");
            key(ui.window(), Key::Return);
            assert!(ui.get_expanded(), "Enter expands parameters");
            key(ui.window(), Key::Backtab);
            assert_eq!(ui.get_focus_index(), 3, "Reverse Tab wraps");
            key(ui.window(), Key::Return);
            assert!(ui.get_terminal_requested());
            assert_eq!(ui.get_outcome(), "");
            key(ui.window(), Key::Escape);
            assert_eq!(ui.get_outcome(), "deny");

            let ui = ParityOverlay::new().unwrap();
            ui.set_frozen(true);
            overlay_logic::connect(&ui, "question");
            draw(&ui);
            ui.invoke_take_focus();
            key(ui.window(), Key::Return);
            assert_eq!(ui.get_step(), 1, "Single choice advances immediately");
            draw(&ui);
            assert_eq!(ui.get_focus_index(), 0);
            key(ui.window(), Key::Return);
            assert_eq!(ui.get_outcome(), "", "Multi choice waits for Done");
            assert!(
                ui.get_current_question()
                    .options
                    .row_data(0)
                    .unwrap()
                    .selected
            );
            key(ui.window(), Key::Return);
            assert!(
                !ui.get_current_question()
                    .options
                    .row_data(0)
                    .unwrap()
                    .selected
            );
            for _ in 0..3 {
                key(ui.window(), Key::Tab);
            }
            assert_eq!(ui.get_focus_index(), 3);
            key(ui.window(), "Custom answer");
            assert_eq!(ui.get_custom(), "Custom answer");
            key(ui.window(), Key::Tab);
            assert_eq!(ui.get_focus_index(), 4);
            key(ui.window(), Key::Return);
            assert_eq!(ui.get_outcome(), "answered");
            assert_eq!(ui.get_answers().row_data(0).unwrap(), "Both");
            assert_eq!(ui.get_answers().row_data(1).unwrap(), "Custom answer");

            let ui = ParityOverlay::new().unwrap();
            ui.set_frozen(true);
            overlay_logic::connect(&ui, "question");
            draw(&ui);
            ui.invoke_take_focus();
            for _ in 0..3 {
                key(ui.window(), Key::Tab);
            }
            key(ui.window(), "  λ答  ");
            key(ui.window(), Key::Return);
            assert_eq!(
                ui.get_step(),
                1,
                "Custom Enter advances, preserving Unicode"
            );
            assert_eq!(ui.get_custom(), "");
            ui.invoke_advance();
            assert_eq!(ui.get_outcome(), "", "Empty Done does not submit");
            ui.invoke_choose(0);
            ui.invoke_choose(1);
            ui.set_custom("   ".into());
            ui.invoke_custom_advance();
            assert_eq!(
                ui.get_outcome(),
                "",
                "Blank custom Enter cannot submit selected options"
            );
            ui.invoke_advance();
            assert_eq!(ui.get_answers().row_data(0).unwrap(), "λ答");
            assert_eq!(ui.get_answers().row_data(1).unwrap(), "Claude Code, Codex");

            let ui = ParityOverlay::new().unwrap();
            ui.set_frozen(true);
            overlay_logic::connect(&ui, "approval");
            draw(&ui);
            ui.invoke_take_focus();
            key(ui.window(), Key::Backtab);
            key(ui.window(), Key::Return);
            assert_eq!(ui.get_outcome(), "allow");
            ui.invoke_decide("deny".into());
            assert_eq!(
                ui.get_outcome(),
                "allow",
                "Resolved requests cannot be answered twice"
            );
        }
        let ui = ParityOverlay::new().unwrap();
        ui.set_frozen(true);
        overlay_logic::connect(&ui, "question");
        draw(&ui);
        for event in [
            WindowEvent::PointerPressed {
                position: slint::LogicalPosition::new(300., 127.),
                button: slint::platform::PointerEventButton::Left,
            },
            WindowEvent::PointerReleased {
                position: slint::LogicalPosition::new(300., 127.),
                button: slint::platform::PointerEventButton::Left,
            },
        ] {
            ui.window().dispatch_event(event);
        }
        assert_eq!(ui.get_step(), 1, "Real option click advances single choice");
        ui.hide().unwrap();
        let ui = ParityOverlay::new().unwrap();
        ui.set_frozen(true);
        overlay_logic::connect(&ui, "approval");
        draw(&ui);
        ui.window().dispatch_event(WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(210., 25.),
        });
        assert!(ui.get_held(), "Real pointer entering card holds countdown");
        ui.window().dispatch_event(WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(300., 155.),
        });
        assert!(ui.get_held(), "Hovering an action also holds countdown");
        let remaining = ui.get_remaining();
        std::thread::sleep(Duration::from_millis(110));
        ui.invoke_tick();
        assert_eq!(ui.get_remaining(), remaining);
        ui.window().dispatch_event(WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(20., 500.),
        });
        ui.invoke_tick();
        std::thread::sleep(Duration::from_millis(110));
        ui.invoke_tick();
        assert!(ui.get_remaining() < remaining);
        ui.set_external_held(true);
        ui.invoke_tick();
        let remaining = ui.get_remaining();
        std::thread::sleep(Duration::from_millis(110));
        ui.invoke_tick();
        assert_eq!(
            ui.get_remaining(),
            remaining,
            "External holds also freeze the countdown"
        );
        assert_eq!(overlay_logic::clock(95.), "1:35");
        assert_eq!(overlay_logic::clock(60.), "1:00");
        assert_eq!(overlay_logic::clock(59.01), "1:00");
        assert_eq!(overlay_logic::clock(59.), "59s");
    }
    #[test]
    fn draft_navigation_and_real_pointer_toggle() {
        let slot = AdapterSlot::default();
        slint::platform::set_platform(Box::new(Offscreen(slot.clone()))).unwrap();
        let panel = ParityPanel::new().unwrap();
        panel_logic::connect(&panel);
        panel.set_provider_filter(2);
        assert_eq!(slint::Model::row_count(&panel.get_session_rows()), 1);
        assert_eq!(
            slint::Model::row_data(&panel.get_session_rows(), 0)
                .unwrap()
                .provider,
            "codex"
        );
        panel.set_provider_filter(0);
        panel.set_page(4);
        panel.show().unwrap();
        let adapter = slot.borrow().as_ref().unwrap().clone();
        adapter.set_size(slint::PhysicalSize::new(1100, 760));
        let mut pixels = vec![PremultipliedRgbaColor::default(); 1100 * 760];
        adapter.draw_if_needed(|renderer| {
            renderer.render(&mut pixels, 1100);
        });
        for event in [
            WindowEvent::PointerPressed {
                position: slint::LogicalPosition::new(1010., 173.),
                button: slint::platform::PointerEventButton::Left,
            },
            WindowEvent::PointerReleased {
                position: slint::LogicalPosition::new(1010., 173.),
                button: slint::platform::PointerEventButton::Left,
            },
        ] {
            panel.window().dispatch_event(event);
        }
        assert!(
            !panel.get_auto_start(),
            "The source-positioned switch responds to real pointer input"
        );
        assert!(panel.get_dirty());
        panel.invoke_navigate(2);
        assert_eq!(panel.get_page(), 4);
        assert!(panel.get_leave_open());
        panel.invoke_discard();
        assert!(panel.get_auto_start());
        assert!(!panel.get_dirty());
        panel.set_auto_update(false);
        panel.set_dirty(true);
        panel.invoke_save();
        panel.set_auto_update(true);
        panel.invoke_discard();
        assert!(!panel.get_auto_update());
        panel.invoke_capture_shortcut("k".into(), true, true, false);
        assert_eq!(panel.get_hotkey(), "Ctrl+Alt+K");
        panel.invoke_discard();
        assert_eq!(panel.get_hotkey(), "Ctrl+Shift+T");
    }
}

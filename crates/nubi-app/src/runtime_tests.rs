//! Launcher and notch-update checks on compiled Slint surfaces, without a desktop.
use super::*;
use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{Key, Platform, PlatformError, WindowAdapter, WindowEvent};

struct Offscreen(Rc<MinimalSoftwareWindow>);
impl Platform for Offscreen {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.0.clone())
    }
}

fn platform() -> Rc<MinimalSoftwareWindow> {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window.clone()))).unwrap();
    window
}

fn key(window: &MinimalSoftwareWindow, text: slint::SharedString) {
    window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text });
}

fn draw(window: &MinimalSoftwareWindow, width: u32, height: u32) -> Vec<u8> {
    window.set_size(slint::PhysicalSize::new(width, height));
    slint::platform::update_timers_and_animations();
    window.request_redraw();
    let mut pixels = vec![PremultipliedRgbaColor::default(); (width * height) as usize];
    assert!(window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, width as usize);
    }));
    pixels
        .into_iter()
        .flat_map(|pixel| [pixel.red, pixel.green, pixel.blue, pixel.alpha])
        .collect()
}

#[test]
fn launcher_selection_focus_shortcuts_and_error_dismissal() {
    let window = platform();
    let ui = NativeLauncher::new().unwrap();
    let choices = Rc::new(RefCell::new(Vec::new()));
    let ran = choices.clone();
    ui.on_run(move |cli| ran.borrow_mut().push(cli.to_string()));
    let cancelled = Rc::new(Cell::new(0));
    let count = cancelled.clone();
    ui.on_cancel(move || count.set(count.get() + 1));
    ui.set_dir("C:/projects/long folder".into());
    ui.set_selected(1);
    ui.show().unwrap();
    ui.invoke_take_focus();
    draw(&window, 340, 176);
    key(&window, "\n".into());
    assert_eq!(choices.borrow().as_slice(), ["codex"]);
    for arrow in [
        Key::Tab,
        Key::LeftArrow,
        Key::RightArrow,
        Key::UpArrow,
        Key::DownArrow,
    ] {
        let previous = ui.get_selected();
        key(&window, arrow.into());
        assert_eq!(ui.get_selected(), 1 - previous);
        key(&window, "\n".into());
        assert_eq!(
            choices.borrow().last().unwrap(),
            if ui.get_selected() == 0 {
                "claude"
            } else {
                "codex"
            }
        );
    }
    window.dispatch_event(WindowEvent::KeyPressed {
        text: Key::Shift.into(),
    });
    let previous = ui.get_selected();
    key(&window, Key::Tab.into());
    window.dispatch_event(WindowEvent::KeyReleased {
        text: Key::Shift.into(),
    });
    assert_eq!(ui.get_selected(), 1 - previous);
    for shortcut in ["c", "X", "C", "x"] {
        key(&window, shortcut.into());
    }
    assert_eq!(
        &choices.borrow()[choices.borrow().len() - 4..],
        ["claude", "codex", "claude", "codex"]
    );
    ui.set_error("Focus a File Explorer window first.".into());
    ui.invoke_take_focus();
    draw(&window, 340, 176);
    let before = choices.borrow().len();
    for shortcut in ["c", "x", "\n"] {
        key(&window, shortcut.into());
    }
    assert_eq!(choices.borrow().len(), before);
    key(&window, Key::Escape.into());
    assert_eq!(cancelled.get(), 1);
    ui.hide().unwrap();
}

fn update_card() -> NotchWindow {
    let ui = NotchWindow::new().unwrap();
    ui.global::<Theme>().set_reduced(true);
    ui.set_ready(true);
    ui.set_opened(true);
    ui.set_view_mode(3);
    ui.set_tab(0);
    ui.set_along(620.);
    ui.set_depth(320.);
    ui.set_open_height(320.);
    ui.global::<crate::NotchData>()
        .set_update(crate::NotchUpdate {
            id: "1".into(),
            version: "0.2.0".into(),
            phase: "available".into(),
            countdown: "Dismisses in 10s".into(),
            ..Default::default()
        });
    ui
}

fn phase(ui: &NotchWindow, phase: &str, message: &str) {
    let data = ui.global::<crate::NotchData>();
    let mut notice = data.get_update();
    notice.phase = phase.into();
    notice.automatic = phase == "ready";
    notice.message = message.into();
    notice.countdown = if phase == "available" {
        "Dismisses in 10s"
    } else {
        ""
    }
    .into();
    data.set_update(notice);
    data.set_update_submitting(false);
}

#[test]
fn notch_update_keyboard_actions_pause_and_disabled_phases() {
    let window = platform();
    let ui = update_card();
    let data = ui.global::<crate::NotchData>();
    let installed = Rc::new(RefCell::new(Vec::new()));
    let cancelled = Rc::new(RefCell::new(Vec::new()));
    let released = Rc::new(RefCell::new(Vec::new()));
    let calls = installed.clone();
    data.on_update_install(move |id| calls.borrow_mut().push(id.to_string()));
    let calls = cancelled.clone();
    data.on_update_dismiss(move |id| calls.borrow_mut().push(id.to_string()));
    let calls = released.clone();
    data.on_update_release(move |id| calls.borrow_mut().push(id.to_string()));
    ui.show().unwrap();
    ui.invoke_focus_requests();
    draw(&window, 720, 360);
    assert!(ui.get_update_card_shown());
    assert!(
        data.get_focused_request().is_empty(),
        "appearance does not focus card"
    );
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(100., 80.),
    });
    assert!(ui.get_update_card_held(), "hover anywhere on card pauses");
    window.dispatch_event(WindowEvent::PointerExited);
    assert!(!ui.get_update_card_held());
    for _ in 0..20 {
        key(&window, Key::Tab.into());
        if data.get_focused_request() == "update:1" {
            break;
        }
    }
    assert_eq!(data.get_focused_request(), "update:1");
    assert!(ui.get_update_card_held(), "keyboard focus pauses");
    key(&window, "\n".into());
    assert_eq!(
        released.borrow().as_slice(),
        ["1"],
        "first card control opens release page"
    );
    key(&window, Key::Tab.into());
    key(&window, "\n".into());
    key(&window, "\n".into());
    assert_eq!(
        installed.borrow().as_slice(),
        ["1"],
        "submission disables immediately"
    );
    data.set_update_submitting(false);
    key(&window, Key::Tab.into());
    key(&window, "\n".into());
    assert_eq!(
        cancelled.borrow().as_slice(),
        ["1"],
        "Later dismisses offered notice"
    );
    phase(
        &ui,
        "downloading",
        "Downloading update… You can cancel before installation.",
    );
    draw(&window, 720, 360);
    // The focused secondary action remains Cancel during download.
    key(&window, "\n".into());
    assert_eq!(cancelled.borrow().as_slice(), ["1", "1"]);
    phase(&ui, "installing", "Installing update and restarting Nubi…");
    draw(&window, 720, 360);
    key(&window, "\n".into());
    assert_eq!(installed.borrow().len(), 1);
    assert_eq!(
        cancelled.borrow().len(),
        2,
        "installation has no Cancel action"
    );
    phase(&ui, "error", "Update failed: test error");
    ui.invoke_focus_requests();
    draw(&window, 720, 360);
    data.set_focused_request("".into());
    for _ in 0..20 {
        key(&window, Key::Tab.into());
        if data.get_focused_request() == "update:1" {
            break;
        }
    }
    key(&window, "\n".into());
    assert_eq!(installed.borrow().len(), 2, "error exposes Retry");
    ui.hide().unwrap();
}

#[test]
fn notch_update_follows_prompts_and_requires_visible_card() {
    let window = platform();
    let ui = update_card();
    ui.global::<crate::NotchData>().set_requests(
        Rc::new(VecModel::from(vec![
            crate::NotchRequest {
                id: "42".into(),
                title: "Permission".into(),
                height: 320.,
                ..Default::default()
            },
            crate::NotchRequest {
                id: "43".into(),
                title: "Question".into(),
                question: true,
                prompt: "Choose an option".into(),
                ..Default::default()
            },
        ]))
        .into(),
    );
    ui.show().unwrap();
    draw(&window, 720, 360);
    assert!(ui.get_requests_content_height() > 500.);
    assert!(
        !ui.get_update_card_shown(),
        "permission/question prompts precede update"
    );
    let position = slint::LogicalPosition::new(360., 180.);
    window.dispatch_event(WindowEvent::PointerMoved { position });
    window.dispatch_event(WindowEvent::PointerScrolled {
        position,
        delta_x: 0.,
        delta_y: -1000.,
    });
    draw(&window, 720, 360);
    assert!(
        ui.get_update_card_shown(),
        "scroll reveals update below prompts"
    );
    ui.set_tab(1);
    draw(&window, 720, 360);
    assert!(!ui.get_update_card_shown());
    ui.set_tab(0);
    ui.set_content_opacity(0.5);
    assert!(
        !ui.get_update_card_shown(),
        "opening animation must finish before acknowledgment"
    );
    ui.set_content_opacity(1.);
    ui.set_depth(44.);
    assert!(
        !ui.get_update_card_shown(),
        "collapsed shape must not acknowledge hidden card"
    );
    ui.hide().unwrap();
}

#[test]
fn notch_update_acknowledgment_hold_and_dismiss_use_live_updater() {
    let window = platform();
    let ui = update_card();
    let (host, _events) = NativeHost::from_settings(nubi_core::settings::Settings::default());
    let notch = Notch::connect(&ui, host.clone());
    updater::seed_notice(&host, false);
    notch.borrow_mut().update_notice(&ui);
    assert_eq!(
        notch.borrow_mut().view(&ui).0,
        3,
        "offer opens request pane"
    );
    assert_eq!(
        updater::get_update_notice(host.clone()).unwrap().expires_ms,
        0
    );
    ui.show().unwrap();
    ui.set_ready(false);
    draw(&window, 720, 360);
    notch.borrow_mut().update_notice(&ui);
    assert_eq!(
        updater::get_update_notice(host.clone()).unwrap().expires_ms,
        0
    );
    ui.set_ready(true);
    notch.borrow_mut().view(&ui);
    draw(&window, 720, 360);
    assert!(
        ui.get_update_card_shown(),
        "height={}, content={}",
        ui.get_open_height(),
        ui.get_requests_content_height()
    );
    notch.borrow_mut().update_notice(&ui);
    assert_eq!(
        updater::get_update_notice(host.clone()).unwrap().expires_ms,
        0,
        "first visible frame does not acknowledge early"
    );
    // Complete the existing 250 ms presentation grace without sleeping in the test.
    // Visibility is validated by the real compiled Slint card.
    notch.borrow_mut().acknowledge_update_for_test(&ui);
    let notice = updater::get_update_notice(host.clone()).unwrap();
    let remaining = notice.expires_ms.saturating_sub(nubi_core::state::now_ms());
    assert!((9_900..=10_000).contains(&remaining));
    ui.set_depth(90.);
    assert!(
        ui.get_update_card_visible(),
        "opening shape still shows part of card"
    );
    assert!(
        !ui.get_update_card_shown(),
        "partial card cannot acknowledge appearance"
    );
    notch.borrow_mut().update_notice(&ui);
    assert!(
        !updater::get_update_notice(host.clone()).unwrap().held,
        "animation settling must not hold an already acknowledged visible card"
    );
    ui.set_depth(44.);
    notch.borrow_mut().update_notice(&ui);
    assert!(
        updater::get_update_notice(host.clone()).unwrap().held,
        "actually clipped card pauses"
    );
    ui.set_depth(320.);
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(100., 80.),
    });
    notch.borrow_mut().update_notice(&ui);
    assert!(updater::get_update_notice(host.clone()).unwrap().held);
    window.dispatch_event(WindowEvent::PointerExited);
    ui.set_tab(1);
    draw(&window, 720, 360);
    notch.borrow_mut().update_notice(&ui);
    assert!(
        updater::get_update_notice(host.clone()).unwrap().held,
        "hidden card pauses after acknowledgment"
    );
    ui.set_tab(0);
    // The countdown row appears after acknowledgment; run the live sizing pass.
    notch.borrow_mut().view(&ui);
    draw(&window, 720, 360);
    notch.borrow_mut().update_notice(&ui);
    assert!(!updater::get_update_notice(host.clone()).unwrap().held);
    updater::dismiss_update(host.clone(), notice.id);
    notch.borrow_mut().update_notice(&ui);
    assert!(ui.global::<crate::NotchData>().get_update().id.is_empty());
    updater::seed_notice(&host, true);
    notch.borrow_mut().update_notice(&ui);
    draw(&window, 720, 360);
    notch.borrow_mut().acknowledge_update_for_test(&ui);
    let notice = updater::get_update_notice(host.clone()).unwrap();
    assert_eq!(notice.phase, "ready");
    assert_eq!(notice.expires_ms, 0, "ready card never starts a countdown");
    assert!(ui
        .global::<crate::NotchData>()
        .get_update()
        .countdown
        .is_empty());
    ui.hide().unwrap();
}

#[test]
fn notch_update_card_renders_light_dark_at_1x_and_125x() {
    let window = platform();
    let folder = std::env::temp_dir()
        .join("nubi-native-20261120-update-row")
        .join("renders");
    std::fs::create_dir_all(&folder).unwrap();
    for dark in [false, true] {
        for percent in [100, 125, 150, 175] {
            let scale = percent as f32 / 100.;
            window.dispatch_event(WindowEvent::ScaleFactorChanged {
                scale_factor: scale,
            });
            let ui = update_card();
            ui.global::<Theme>().set_scale(scale);
            let (host, _events) =
                NativeHost::from_settings(nubi_core::settings::Settings::default());
            let notch = Notch::connect(&ui, host);
            ui.global::<Theme>().set_dark(dark);
            let mut motion = Motion::new();
            ui.show().unwrap();
            draw(&window, (720. * scale) as u32, (360. * scale) as u32);
            for (name, state, message) in [
                ("available-off", "available", ""),
                ("ready-on", "ready", ""),
                ("downloading", "downloading", ""),
                ("installing", "installing", ""),
                ("error", "error", "Update failed: the download could not complete. Check your connection and retry."),
            ] {
                phase(&ui, state, message);
                let (view, target) = notch.borrow_mut().view(&ui);
                motion.target(view, target, true);
                motion.draw(&ui);
                assert_eq!(ui.get_requests_content_height(), 64.);
                assert_eq!(ui.get_open_height(), 132.);
                save(&window, &folder, &format!("notch-update-{name}"), dark, percent, 720, 360);
                assert!(ui.get_update_card_shown(), "complete {name} card fits at {percent}%");
            }
            ui.hide().unwrap();
        }
    }
}

#[test]
fn update_mouse_actions_use_live_card_geometry_without_notice_watchers() {
    let window = platform();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for automatic in [false, true] {
        for scale in [1., 1.25] {
            window.dispatch_event(WindowEvent::ScaleFactorChanged {
                scale_factor: scale,
            });
            for edge in 0..4 {
                let ui = update_card();
                ui.global::<Theme>().set_scale(scale);
                let settings = nubi_core::settings::Settings {
                    edge: ["top", "bottom", "left", "right"][edge].into(),
                    ..Default::default()
                };
                let (host, _events) = NativeHost::from_settings(settings);
                let notch = Notch::connect(&ui, host.clone());
                crate::notch::connect_update(&ui, host.clone(), runtime.handle().clone());
                // No watch_notice task or UI-event consumption. Ready offers have no
                // countdown watcher; the live notice synchronization must suffice.
                updater::seed_notice(&host, automatic);
                ui.show().unwrap();
                let mut motion = Motion::new();
                let mut settle = || {
                    for _ in 0..4 {
                        notch.borrow_mut().update_notice(&ui);
                        let (view, target) = notch.borrow_mut().view(&ui);
                        motion.target(view, target, true);
                        motion.draw(&ui);
                        draw(&window, (720. * scale) as u32, (360. * scale) as u32);
                    }
                };
                let click = |x, y| {
                    let position = slint::LogicalPosition::new(x, y);
                    let shape = crate::shape::Shape([
                        f64::from(ui.get_along()),
                        f64::from(ui.get_depth()),
                        f64::from(ui.get_corner()),
                        f64::from(ui.get_ear()),
                    ]);
                    let scene = crate::shape::Scene {
                        edge: edge as i32,
                        offset: f64::from(ui.get_expansion_offset()),
                        width: 720.,
                        height: 360.,
                    };
                    assert!(scene.contains(shape, f64::from(x), f64::from(y)),
                        "native hit test contains button: automatic={automatic}, edge={edge}, scale={scale}");
                    window.dispatch_event(WindowEvent::PointerMoved { position });
                    window.dispatch_event(WindowEvent::PointerPressed {
                        position,
                        button: slint::platform::PointerEventButton::Left,
                    });
                    window.dispatch_event(WindowEvent::PointerReleased {
                        position,
                        button: slint::platform::PointerEventButton::Left,
                    });
                };
                settle();
                assert!(ui.get_update_card_shown());
                let left = match edge {
                    2 => 0.,
                    3 => 720. - ui.get_depth(),
                    _ => (720. - ui.get_along()) / 2.,
                };
                let top = match edge {
                    0 => 0.,
                    1 => 360. - ui.get_depth(),
                    _ => (360. - ui.get_along()) / 2. + ui.get_expansion_offset(),
                };
                let card_right = left + ui.get_open_width() - 12.;
                let button_y = top + 52. + 18. + 14.;
                click(card_right - 118., button_y);
                runtime.block_on(async { tokio::task::yield_now().await });
                settle();
                assert_eq!(ui.global::<crate::NotchData>().get_update().phase, "error");
                assert_eq!(
                    ui.global::<crate::NotchData>().get_update().automatic,
                    automatic
                );
                assert_eq!(
                    ui.global::<crate::NotchData>().get_update().message,
                    "Update failed: Updates only work in release builds"
                );
                click(card_right - 38., button_y);
                settle();
                assert!(updater::get_update_notice(host).is_none());
                assert!(ui.global::<crate::NotchData>().get_update().id.is_empty());
                ui.hide().unwrap();
            }
        }
    }
}

#[test]
fn unhovered_update_stays_visible_and_unheld_at_its_live_height() {
    let window = platform();
    let ui = update_card();
    let (host, _events) = NativeHost::from_settings(nubi_core::settings::Settings::default());
    let notch = Notch::connect(&ui, host.clone());
    updater::seed_notice(&host, false);
    ui.show().unwrap();
    let mut motion = Motion::new();
    for frame in 0..12 {
        notch.borrow_mut().update_notice(&ui);
        let (view, target) = notch.borrow_mut().view(&ui);
        assert_eq!(view, 3, "unhovered update keeps the pane open");
        motion.target(view, target, true);
        motion.draw(&ui);
        draw(&window, 720, 360);
        if frame == 3 {
            notch.borrow_mut().acknowledge_update_for_test(&ui);
        }
        if frame > 6 {
            assert!(
                ui.get_update_card_shown(),
                "countdown card remains fully visible"
            );
            assert!(!updater::get_update_notice(host.clone()).unwrap().held);
        }
    }
    assert_ne!(updater::get_update_notice(host).unwrap().expires_ms, 0);
    ui.hide().unwrap();
}

#[test]
fn ready_card_actions_and_optional_rows_use_compact_layout() {
    let window = platform();
    let ui = update_card();
    let data = ui.global::<crate::NotchData>();
    let installed = Rc::new(Cell::new(0));
    let dismissed = Rc::new(Cell::new(0));
    let count = installed.clone();
    data.on_update_install(move |_| count.set(count.get() + 1));
    let count = dismissed.clone();
    data.on_update_dismiss(move |_| count.set(count.get() + 1));
    ui.show().unwrap();
    draw(&window, 720, 360);
    let available_height = ui.get_requests_content_height();
    phase(&ui, "ready", "");
    draw(&window, 720, 360);
    let ready_height = ui.get_requests_content_height();
    assert!(
        (available_height - ready_height).abs() < 0.01,
        "countdown metadata does not move controls or change card height"
    );
    assert_eq!(ready_height, 64., "tight shared permission/update row");
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(100., 80.),
    });
    draw(&window, 720, 360);
    assert_eq!(
        ui.get_requests_content_height(),
        ready_height,
        "hover does not create countdown"
    );
    window.dispatch_event(WindowEvent::PointerExited);
    ui.invoke_focus_requests();
    for _ in 0..20 {
        key(&window, Key::Tab.into());
        if data.get_focused_request() == "update:1" {
            break;
        }
    }
    key(&window, Key::Tab.into());
    key(&window, "\n".into());
    key(&window, "\n".into());
    assert_eq!(installed.get(), 1, "ready card installs once");
    data.set_update_submitting(false);
    key(&window, Key::Tab.into());
    key(&window, "\n".into());
    assert_eq!(dismissed.get(), 1, "ready card exposes Later");
    phase(&ui, "downloading", "");
    draw(&window, 720, 360);
    let progress_height = ui.get_requests_content_height();
    phase(&ui, "downloading", "Downloading…");
    draw(&window, 720, 360);
    assert!(
        (ui.get_requests_content_height() - progress_height).abs() < 5.,
        "message replaces phase label"
    );
    ui.hide().unwrap();
}

#[test]
fn runtime_surfaces_render_both_themes_at_supported_dpi() {
    let window = platform();
    let folder = std::env::temp_dir()
        .join("nubi-native-20261120-stage5")
        .join("renders");
    std::fs::create_dir_all(&folder).unwrap();
    for dark in [false, true] {
        for percent in [100, 125, 150, 175] {
            let scale = percent as f32 / 100.;
            window.dispatch_event(WindowEvent::ScaleFactorChanged {
                scale_factor: scale,
            });
            let launcher = NativeLauncher::new().unwrap();
            launcher.global::<Theme>().set_dark(dark);
            launcher.global::<Theme>().set_scale(scale);
            launcher.set_dir("C:/projects/a very long directory with spaces/project".into());
            launcher.show().unwrap();
            launcher.invoke_take_focus();
            save(&window, &folder, "launcher", dark, percent, 340, 176);
            launcher.hide().unwrap();
            drop(launcher);
        }
    }
}

fn save(
    window: &MinimalSoftwareWindow,
    folder: &std::path::Path,
    surface: &str,
    dark: bool,
    percent: u32,
    width: u32,
    height: u32,
) {
    let width = width * percent / 100;
    let height = height * percent / 100;
    let pixels = draw(window, width, height);
    assert!(pixels.as_chunks::<4>().0.iter().any(|pixel| pixel[3] != 0));
    image::save_buffer(
        folder.join(format!(
            "{surface}-{}-{percent}.png",
            if dark { "dark" } else { "light" }
        )),
        &pixels,
        width,
        height,
        image::ColorType::Rgba8,
    )
    .unwrap();
}

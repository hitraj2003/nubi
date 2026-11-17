//! Compiled-surface renders and core-connected interaction checks. No desktop,
//! hooks, pipe server, settings writes or registrations are needed.
use super::*;
use image::ImageEncoder;
use nubi_core::{protocol::HookMsg, settings::Settings};
use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{Platform, PlatformError, WindowAdapter, WindowEvent};

fn save(image: &image::RgbImage, path: impl AsRef<std::path::Path>) {
    let encoder = image::codecs::png::PngEncoder::new_with_quality(
        std::fs::File::create(path).unwrap(),
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::NoFilter,
    );
    encoder
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
}
struct Offscreen(Rc<MinimalSoftwareWindow>);
impl Platform for Offscreen {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.0.clone())
    }
}
fn capture(window: &MinimalSoftwareWindow, ui: &NotchWindow, name: &str, scale: f32, side: bool) {
    window.dispatch_event(WindowEvent::ScaleFactorChanged {
        scale_factor: scale,
    });
    ui.global::<Theme>().set_scale(scale);
    let (w, h) = if side { (420., 860.) } else { (720., 700.) };
    let (width, height) = ((w * scale) as u32, (h * scale) as u32);
    window.set_size(slint::PhysicalSize::new(width, height));
    let mut pixels = vec![PremultipliedRgbaColor::default(); width as usize * height as usize];
    window.request_redraw();
    assert!(window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, width as usize);
    }));
    let mut image = image::RgbImage::new(width, height);
    let mut opaque = 0;
    for (index, pixel) in pixels.iter().enumerate() {
        if pixel.alpha == 255 {
            opaque += 1;
        }
        let background = ((255 - u16::from(pixel.alpha)) * 96 / 255) as u8;
        image.put_pixel(
            index as u32 % width,
            index as u32 / width,
            image::Rgb([
                pixel.red.saturating_add(background),
                pixel.green.saturating_add(background),
                pixel.blue.saturating_add(background),
            ]),
        );
    }
    assert!(
        opaque > (if ui.get_view_mode() == 0 { 100. } else { 1000. } * scale * scale) as usize,
        "empty rendered notch: {name}"
    );
    // The software renderer's buffer has the target physical size, never a
    // resized 100% bitmap. Separate nearest-neighbor zooms expose AA and borders.
    let folder =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/stage3/renders");
    std::fs::create_dir_all(&folder).unwrap();
    save(&image, folder.join(format!("{name}.png")));
    if !name.contains("-125-top-usage")
        && !name.contains("-175-right-sessions")
        && !name.contains("-125-top-question")
    {
        return;
    }
    let crop = if side {
        image::imageops::crop_imm(
            &image,
            0,
            (height / 2).saturating_sub((ui.get_along() * scale / 2.) as u32 + 24),
            width,
            (ui.get_along() * scale + 48.).min(height as f32) as u32,
        )
        .to_image()
    } else {
        let y = if ui.get_edge() == 1 {
            height.saturating_sub((ui.get_depth() * scale + 24.) as u32)
        } else {
            0
        };
        image::imageops::crop_imm(
            &image,
            0,
            y,
            width,
            (ui.get_depth() * scale + 24.).min(height as f32) as u32,
        )
        .to_image()
    };
    let zoom = image::imageops::resize(
        &crop,
        crop.width() * 3,
        crop.height() * 3,
        image::imageops::FilterType::Nearest,
    );
    save(&zoom, folder.join(format!("{name}-zoom.png")));
}
#[test]
fn live_notch_decisions_folding_edges_and_physical_renders() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window.clone()))).unwrap();
    let settings = Settings {
        desktime_tab: true,
        desktime_enabled: true,
        ..Default::default()
    };
    let (host, _events) = NativeHost::from_settings(settings);
    let stamp = now_ms();
    with(&host, |state| {
        for (provider, name) in [("claude", "Nubi · native notch"), ("codex", "Stage 3")] {
            state.on_event(&HookMsg {
                provider: provider.into(),
                session_id: "render".into(),
                event: "SessionStart".into(),
                ..Default::default()
            });
            let session = state
                .sessions
                .get_mut(&format!("{provider}:render"))
                .unwrap();
            session.name = name.into();
            session.state = if provider == "claude" {
                "working"
            } else {
                "done"
            }
            .into();
            session.context_tokens = Some(48000);
            state.usage.insert(
                provider.into(),
                nubi_core::state::Usage {
                    provider: provider.into(),
                    status: "ok".into(),
                    fetched_ms: Some(stamp),
                    message: None,
                    windows: vec![
                        nubi_core::state::Window {
                            id: "5h".into(),
                            label: "5h".into(),
                            used_percent: 42.,
                            resets_at: Some((stamp / 1000) as i64 + 3660),
                        },
                        nubi_core::state::Window {
                            id: "7d".into(),
                            label: "7d".into(),
                            used_percent: 68.,
                            resets_at: Some((stamp / 1000) as i64 + 7200),
                        },
                    ],
                },
            );
        }
    });
    let ui = NotchWindow::new().unwrap();
    let notch = Notch::connect(&ui, host.clone());
    ui.set_ready(true);
    ui.show().unwrap();
    let message = HookMsg {
        provider: "claude".into(),
        session_id: "render".into(),
        event: "PermissionRequest".into(),
        tool_name: Some("Bash".into()),
        tool_input: Some(serde_json::json!({"command":"cargo test --offline --workspace"})),
        ..Default::default()
    };
    let (tx, mut rx) = tokio::sync::oneshot::channel();
    let id = host.add_pending(&message, tx, 600000);
    notch.borrow_mut().update(&ui);
    assert_eq!(ui.get_tab(), 0);
    assert_eq!(notch.borrow_mut().view(&ui).0, 3);
    ui.global::<NotchData>()
        .invoke_hold(id.to_string().into(), true);
    assert!(with(&host, |state| state.pending[&id].held));
    ui.set_view_mode(3);
    ui.set_opened(true);
    ui.set_along(620.);
    ui.set_depth(112.);
    ui.set_open_height(112.);
    window.dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(520., 74.),
    });
    assert!(
        with(&host, |state| state.pending[&id].held),
        "request hover includes Allow/Deny buttons"
    );
    let expiry = with(&host, |state| state.pending[&id].expires_ms);
    host.pending_tick(id, 250, expiry + 1000);
    assert_eq!(
        with(&host, |state| state.pending[&id].expires_ms),
        expiry + 250
    );
    ui.global::<NotchData>()
        .invoke_answer(id.to_string().into(), "deny".into());
    let decision = rx.try_recv().unwrap();
    assert_eq!(decision.decision, "deny");
    assert_eq!(decision.message.as_deref(), Some("Denied from Nubi"));
    let question = HookMsg {
        tool_name: Some("AskUserQuestion".into()),
        event: "PreToolUse".into(),
        tool_input: Some(serde_json::json!({"questions":[
        {"question":"Which language?","options":[{"label":"Rust","description":"Native runtime"},{"label":"日本語"}],"multiSelect":false},
        {"question":"Which checks?","options":[{"label":"Build"},{"label":"Tests"}],"multiSelect":true}]})),
        ..message.clone()
    };
    let (tx, mut answers) = tokio::sync::oneshot::channel();
    let qid = host.add_pending(&question, tx, 600000);
    notch.borrow_mut().update(&ui);
    ui.global::<NotchData>()
        .invoke_choose(qid.to_string().into(), 1);
    assert!(
        ui.global::<NotchData>()
            .get_requests()
            .row_data(0)
            .unwrap()
            .multi
    );
    ui.global::<NotchData>()
        .invoke_submit(qid.to_string().into());
    assert!(answers.try_recv().is_err());
    ui.global::<NotchData>()
        .invoke_choose(qid.to_string().into(), 0);
    ui.global::<NotchData>()
        .invoke_custom(qid.to_string().into(), "  Unicode ✓  ".into());
    ui.set_tab(1);
    ui.invoke_tab_selected(1);
    ui.set_tab(0);
    ui.invoke_tab_selected(0);
    notch.borrow_mut().update(&ui);
    assert_eq!(
        ui.global::<NotchData>()
            .get_requests()
            .row_data(0)
            .unwrap()
            .custom,
        "  Unicode ✓  "
    );
    ui.global::<NotchData>()
        .set_focused_request(qid.to_string().into());
    ui.global::<NotchData>().set_input_focused(true);
    ui.global::<NotchData>()
        .invoke_submit(qid.to_string().into());
    assert!(ui.global::<NotchData>().get_focused_request().is_empty());
    assert!(!ui.global::<NotchData>().get_input_focused());
    let reply = answers.try_recv().unwrap().answers.unwrap();
    assert_eq!(reply["Which language?"], "日本語");
    assert_eq!(reply["Which checks?"], "Build, Unicode ✓");
    // Real keyboard events exercise tab focus and confirmation trapping.
    ui.set_tab(0);
    ui.invoke_focus_requests();
    window.dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::Tab.into(),
    });
    window.dispatch_event(WindowEvent::KeyReleased {
        text: slint::platform::Key::Tab.into(),
    });
    window.dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::RightArrow.into(),
    });
    assert_eq!(ui.get_tab(), 1);
    window.dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::End.into(),
    });
    assert_eq!(ui.get_tab(), 3);
    ui.global::<NotchData>()
        .invoke_session_menu("claude:render".into());
    window.dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::End.into(),
    });
    assert_eq!(ui.get_menu_index(), 3);
    window.dispatch_event(WindowEvent::KeyPressed { text: "\n".into() });
    assert_eq!(ui.get_confirmation(), 1);
    window.dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::Tab.into(),
    });
    assert_eq!(ui.get_menu_index(), 1);
    window.dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::Escape.into(),
    });
    assert!(ui.get_menu_key().is_empty());
    ui.set_tab(0);
    let (tx, _rx) = tokio::sync::oneshot::channel();
    let missed = host.add_pending(&message, tx, 600000);
    host.remove_pending(missed, true);
    notch.borrow_mut().update(&ui);
    assert!(
        ui.global::<NotchData>()
            .get_requests()
            .row_data(0)
            .unwrap()
            .missed
    );
    ui.global::<NotchData>()
        .invoke_dismiss(missed.to_string().into());
    assert!(with(&host, |state| state.missed.is_empty()));
    let (tx, _rx) = tokio::sync::oneshot::channel();
    let hidden = host.add_pending(
        &HookMsg {
            provider: "codex".into(),
            ..message.clone()
        },
        tx,
        600000,
    );
    host.remove_pending(hidden, true);
    let (tx, _rx) = tokio::sync::oneshot::channel();
    let visible = host.add_pending(&message, tx, 600000);
    host.remove_pending(visible, true);
    with(&host, |state| state.settings.providers = "claude".into());
    notch.borrow_mut().update(&ui);
    assert_eq!(ui.global::<NotchData>().get_missed_count(), 1);
    ui.global::<NotchData>().invoke_dismiss("".into());
    assert_eq!(
        with(&host, |state| state
            .missed
            .iter()
            .map(|item| item.id)
            .collect::<Vec<_>>()),
        vec![hidden]
    );
    with(&host, |state| {
        state.settings.providers = "both".into();
        state.missed.clear();
    });
    use chrono::Datelike;
    let today = chrono::Local::now().date_naive();
    let monday = today - chrono::Duration::days(today.weekday().num_days_from_monday().into());
    let month_start = today.with_day(1).unwrap();
    let month_end = (month_start + chrono::Duration::days(32))
        .with_day(1)
        .unwrap()
        - chrono::Duration::days(1);
    for (view, start, end, productive) in [
        ("day", today, today, 310),
        ("week", monday, monday + chrono::Duration::days(6), 1530),
        ("month", month_start, month_end, 4000),
    ] {
        host.desktime.lock().unwrap().reports.insert(
            view.into(),
            (
                nubi_core::desktime::Report {
                    view: view.into(),
                    productive_min: Some(productive),
                    at_work_min: Some(productive + 40),
                    range_start: start.to_string(),
                    range_end: end.to_string(),
                    holidays: vec![],
                    logged_out: false,
                },
                stamp as i64,
            ),
        );
    }
    let mut renders = 0;
    for theme in ["dark", "light"] {
        with(&host, |state| state.settings.notch_theme = theme.into());
        for percent in [100, 125, 150, 175] {
            let scale = percent as f32 / 100.;
            for edge in ["top", "bottom", "left", "right"] {
                with(&host, |state| state.settings.edge = edge.into());
                notch.borrow_mut().update(&ui);
                assert_eq!(ui.global::<Theme>().get_dark(), theme == "dark");
                for (state, tab, view) in [
                    ("compact", 1, 1),
                    ("sessions", 1, 3),
                    ("usage", 2, 3),
                    ("desktime", 3, 3),
                    ("folded", 1, 0),
                ] {
                    ui.set_tab(tab);
                    notch.borrow_mut().hovering = view == 3;
                    notch.borrow_mut().clicked = view == 3;
                    let (_, mut values) = notch.borrow_mut().view(&ui);
                    if view == 0 {
                        values = [56., 6., 3., 4.];
                    }
                    if view == 1 {
                        values = if ui.get_edge() >= 2 {
                            [122., 46., 18., 8.]
                        } else {
                            [145., 44., 16., 8.]
                        };
                    }
                    ui.set_view_mode(view);
                    ui.set_opened(view == 3);
                    ui.set_along(values[0] as f32);
                    ui.set_depth(values[1] as f32);
                    ui.set_corner(values[2] as f32);
                    ui.set_ear(values[3] as f32);
                    capture(
                        &window,
                        &ui,
                        &format!("{theme}-{percent}-{edge}-{state}"),
                        scale,
                        ui.get_edge() >= 2,
                    );
                    renders += 1;
                }
            }
            with(&host, |state| {
                state.settings.edge = "top".into();
                state.settings.questions_mode = "full".into();
            });
            for (state, message) in [
                ("approval", message.clone()),
                ("question", question.clone()),
                ("slim", message.clone()),
            ] {
                with(&host, |core| {
                    core.settings.questions_mode =
                        if state == "slim" { "system" } else { "full" }.into()
                });
                let (tx, _rx) = tokio::sync::oneshot::channel();
                let id = host.add_pending(&message, tx, 600000);
                notch.borrow_mut().update(&ui);
                ui.set_tab(0);
                let (_, values) = notch.borrow_mut().view(&ui);
                ui.set_view_mode(3);
                ui.set_opened(true);
                ui.set_along(values[0] as f32);
                ui.set_depth(values[1] as f32);
                ui.set_corner(values[2] as f32);
                ui.set_ear(values[3] as f32);
                capture(
                    &window,
                    &ui,
                    &format!("{theme}-{percent}-top-{state}"),
                    scale,
                    false,
                );
                renders += 1;
                host.remove_pending(id, false);
            }
        }
    }
    assert_eq!(renders, 184);
    // All edges keep permission and missed-card actions inside the narrow card.
    for edge in ["left", "right", "bottom"] {
        with(&host, |state| state.settings.edge = edge.into());
        for (name, msg, mode) in [
            ("slim", message.clone(), "system"),
            ("question", question.clone(), "full"),
        ] {
            with(&host, |state| state.settings.questions_mode = mode.into());
            let (tx, _rx) = tokio::sync::oneshot::channel();
            let id = host.add_pending(&msg, tx, 600000);
            notch.borrow_mut().update(&ui);
            ui.set_tab(0);
            let (_, values) = notch.borrow_mut().view(&ui);
            ui.set_view_mode(3);
            ui.set_opened(true);
            ui.set_along(values[0] as f32);
            ui.set_depth(values[1] as f32);
            capture(
                &window,
                &ui,
                &format!("light-125-{edge}-{name}"),
                1.25,
                ui.get_side(),
            );
            host.remove_pending(id, false);
        }
    }
    // DeskTime errors never leak into Usage or survive a successful refresh.
    ui.set_tab(3);
    host.desktime.lock().unwrap().error = Some("offline".into());
    notch.borrow_mut().update(&ui);
    assert!(ui
        .global::<NotchData>()
        .get_refresh_error()
        .starts_with("Couldn't refresh"));
    ui.set_tab(2);
    ui.invoke_tab_selected(2);
    assert!(ui.global::<NotchData>().get_refresh_error().is_empty());
    ui.set_tab(3);
    host.desktime.lock().unwrap().error = None;
    notch.borrow_mut().update(&ui);
    assert!(ui.global::<NotchData>().get_refresh_error().is_empty());
    for (enabled, logged_out) in [(false, false), (true, true)] {
        with(&host, |state| state.settings.desktime_enabled = enabled);
        host.desktime.lock().unwrap().logged_out = logged_out;
        notch.borrow_mut().update(&ui);
        assert_eq!(ui.global::<NotchData>().get_desk_sections().row_count(), 0);
    }
    with(&host, |state| {
        state.settings.notch_accent = "blue".into();
        state.settings.reduce_motion = true;
    });
    notch.borrow_mut().update(&ui);
    assert_eq!(ui.global::<Theme>().get_notch_accent(), "blue");
    assert!(ui.global::<Theme>().get_reduced());
    with(&host, |state| {
        state.settings.notch_on_hover = true;
        state.pending.clear();
        state.missed.clear();
    });
    notch.borrow_mut().update(&ui);
    notch.borrow_mut().pointer(false, &ui);
    notch.borrow_mut().fold_since = Instant::now() - Duration::from_millis(451);
    assert_eq!(notch.borrow_mut().view(&ui).0, 0);
    notch.borrow_mut().pointer(true, &ui);
    assert_eq!(notch.borrow_mut().view(&ui).0, 1);
    notch.borrow_mut().clicked = true;
    assert_eq!(notch.borrow_mut().view(&ui).0, 3);
    ui.hide().unwrap();
}

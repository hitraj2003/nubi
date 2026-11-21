//! Compiled panel controls, real core replies and offscreen software renders.
use super::*;
use crate::{NotchWindow, panel_services::Services};
use nubi_core::{
    host::Host,
    protocol::HookMsg,
    state::{Alert, Usage, Window},
};
use slint::platform::{
    Key, Platform, PlatformError, PointerEventButton, WindowAdapter, WindowEvent,
    software_renderer::{MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType},
};
use std::cell::Cell;

struct Offscreen(Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>);
impl Platform for Offscreen {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        self.0.borrow_mut().push(window.clone());
        Ok(window)
    }
}
fn key(window: &MinimalSoftwareWindow, text: slint::SharedString) {
    window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text });
    slint::platform::update_timers_and_animations();
}
fn click(window: &MinimalSoftwareWindow, x: f32, y: f32) {
    let position = slint::LogicalPosition::new(x, y);
    window.dispatch_event(WindowEvent::PointerMoved { position });
    window.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
    slint::platform::update_timers_and_animations();
}
fn render(
    window: &MinimalSoftwareWindow,
    scale: f32,
    width: u32,
    height: u32,
    path: Option<&std::path::Path>,
) {
    window.dispatch_event(WindowEvent::ScaleFactorChanged {
        scale_factor: scale,
    });
    let (w, h) = (
        (width as f32 * scale).round() as u32,
        (height as f32 * scale).round() as u32,
    );
    window.set_size(slint::PhysicalSize::new(w, h));
    slint::platform::update_timers_and_animations();
    window.request_redraw();
    let mut pixels = vec![PremultipliedRgbaColor::default(); (w * h) as usize];
    assert!(window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, w as usize);
    }));
    if let Some(path) = path {
        let rgba: Vec<u8> = pixels
            .into_iter()
            .flat_map(|p| [p.red, p.green, p.blue, p.alpha])
            .collect();
        image::save_buffer(path, &rgba, w, h, image::ColorType::Rgba8).unwrap();
    }
}
fn fixture() -> (
    NativeHost,
    tokio::sync::oneshot::Receiver<nubi_core::protocol::Reply>,
    u64,
) {
    let settings = Settings {
        sounds: false,
        toast_when_fullscreen: false,
        follow_quiet_time: false,
        questions_mode: "full".into(),
        quiet_start: Some(22),
        quiet_end: None,
        ..Default::default()
    };
    let (host, _) = NativeHost::from_settings(settings);
    with(&host, |s| {
        for i in 0..14 {
            s.on_event(&HookMsg {
                provider: if i % 2 == 0 { "claude" } else { "codex" }.into(),
                session_id: format!("session-{i}"),
                event: "UserPromptSubmit".into(),
                cwd: Some(format!("C:/projects/folder-{i}")),
                ..Default::default()
            });
        }
        for provider in ["claude", "codex"] {
            s.usage.insert(
                provider.into(),
                Usage {
                    provider: provider.into(),
                    status: "ok".into(),
                    fetched_ms: Some(now_ms() - 80000),
                    windows: [
                        ("5h", 42.),
                        ("7d", 64.),
                        ("7d Opus", 78.),
                        ("7d Sonnet", 12.),
                    ]
                    .into_iter()
                    .map(|(label, used_percent)| Window {
                        id: label.into(),
                        label: label.into(),
                        used_percent,
                        resets_at: Some(chrono::Local::now().timestamp() + 7200),
                    })
                    .collect(),
                    message: None,
                },
            );
        }
        s.alerts.clear();
        for queued in [false, false, true] {
            s.push_alert(Alert {
                ts: now_ms(),
                provider: if queued { "codex" } else { "claude" }.into(),
                kind: "done".into(),
                title: "Task complete".into(),
                body: "Review ready".into(),
                session_key: None,
                queued,
            });
        }
    });
    let message = HookMsg {
        provider: "claude".into(),
        session_id: "session-0".into(),
        event: "PermissionRequest".into(),
        tool_name: Some("Bash".into()),
        tool_input: Some(json!({"command":"cargo test"})),
        ..Default::default()
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    let id = host.add_pending(&message, tx, 60000);
    (host, rx, id)
}
#[test]
fn compiled_panel_workflows_drafts_replies_and_render_matrix() {
    compiled_workflows(true);
}

#[test]
fn compiled_panel_remaining_workflows_and_render_matrix() {
    // Independent fixture lets remaining checks execute even if the Escape regression fails.
    compiled_workflows(false);
}

fn compiled_workflows(check_escape: bool) {
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(Offscreen(windows.clone()))).unwrap();
    eprintln!("panel: fixture");
    let (host, mut permission, permission_id) = fixture();
    let notch_ui = NotchWindow::new().unwrap();
    let notch = Notch::connect(&notch_ui, host.clone());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let registrations = Rc::new(Cell::new(0));
    let called = registrations.clone();
    let services = Services {
        host: host.clone(),
        runtime: runtime.handle().clone(),
        register: Rc::new(move |_| {
            called.set(called.get() + 1);
            Ok(())
        }),
        notch: notch.clone(),
        notch_ui: notch_ui.as_weak(),
    };
    eprintln!("panel: create");
    let panel = ParityPanel::new().unwrap();
    let state = Panel::new(&with(&host, |s| s.settings.clone()));
    eprintln!("panel: bind");
    Panel::bind(&panel, state.clone(), services.clone());
    assert!(
        panel.global::<PanelData>().get_release_loading(),
        "version loading starts at panel startup"
    );
    eprintln!("panel: bound");
    panel.show().unwrap();
    panel.invoke_take_focus();
    eprintln!("panel: shown");
    let window = windows.borrow().last().unwrap().clone();
    panel.global::<Theme>().set_reduced(true);
    panel.set_page(0);
    eprintln!("panel: first render");
    render(&window, 1., 1100, 760, None);
    eprintln!("panel: first rendered");
    assert_eq!(panel.global::<NotchData>().get_pending_count(), 1);
    click(&window, 146., 190.);
    assert_eq!(
        permission.try_recv().unwrap().decision,
        "allow",
        "compiled Allow returns real hook reply"
    );
    assert!(!with(&host, |s| s.pending.contains_key(&permission_id)));
    // Escape routes through the compiled shell and cannot close a dirty draft.
    panel.set_page(4);
    panel
        .global::<PanelData>()
        .invoke_edit("auto_update".into(), "false".into());
    if check_escape {
        eprintln!(
            "Escape diagnosis: dirty={} focus={:?}",
            panel.get_dirty(),
            slint::private_unstable_api::re_exports::WindowInner::from_pub(panel.window())
                .focus_item
                .borrow()
                .upgrade()
        );
        let opener = slint::private_unstable_api::re_exports::WindowInner::from_pub(panel.window())
            .focus_item
            .borrow()
            .clone();
        key(&window, Key::Escape.into());
        eprintln!(
            "Escape diagnosis after dispatch: dirty={} focus={:?}",
            panel.get_dirty(),
            slint::private_unstable_api::re_exports::WindowInner::from_pub(panel.window())
                .focus_item
                .borrow()
                .upgrade()
        );
        assert!(panel.get_leave_open());
        // Popup traversal wraps inside its three actions, in both directions.
        key(&window, Key::Tab.into());
        let first = slint::private_unstable_api::re_exports::WindowInner::from_pub(panel.window())
            .focus_item
            .borrow()
            .clone();
        for _ in 0..3 {
            key(&window, Key::Tab.into());
        }
        assert_eq!(
            first.upgrade(),
            slint::private_unstable_api::re_exports::WindowInner::from_pub(panel.window())
                .focus_item
                .borrow()
                .upgrade()
        );
        window.dispatch_event(WindowEvent::KeyPressed {
            text: Key::Shift.into(),
        });
        for _ in 0..3 {
            key(&window, Key::Tab.into());
        }
        window.dispatch_event(WindowEvent::KeyReleased {
            text: Key::Shift.into(),
        });
        assert_eq!(
            first.upgrade(),
            slint::private_unstable_api::re_exports::WindowInner::from_pub(panel.window())
                .focus_item
                .borrow()
                .upgrade()
        );
        key(&window, Key::Escape.into());
        assert!(!panel.get_leave_open());
        assert_eq!(
            opener.upgrade(),
            slint::private_unstable_api::re_exports::WindowInner::from_pub(panel.window())
                .focus_item
                .borrow()
                .upgrade(),
            "cancel restores opener"
        );
    }
    panel.invoke_discard();

    // Actual capture callback, including parser validation. No global registration.
    for (text, ctrl, alt, shift, meta, expected) in [
        ("y".into(), true, false, true, false, Some("Ctrl+Shift+Y")),
        (
            " ".into(),
            true,
            false,
            false,
            true,
            Some("Ctrl+Super+Space"),
        ),
        (
            Key::LeftArrow.into(),
            false,
            true,
            false,
            false,
            Some("Alt+ArrowLeft"),
        ),
        (
            Key::Escape.into(),
            true,
            false,
            false,
            false,
            Some("Ctrl+Escape"),
        ),
        (
            Key::F8.into(),
            true,
            true,
            false,
            false,
            Some("Ctrl+Alt+F8"),
        ),
        ("z".into(), false, false, false, false, None),
        (Key::Control.into(), true, false, false, false, None),
        (Key::Meta.into(), false, false, false, true, None),
        (Key::Tab.into(), true, false, false, false, None),
        (Key::Backspace.into(), false, false, false, false, Some("")),
        (Key::Delete.into(), true, false, false, false, Some("")),
    ] {
        let before = state.borrow().draft.value("launch_hotkey");
        panel.global::<PanelData>().invoke_capture(
            "launch_hotkey".into(),
            text,
            ctrl,
            alt,
            shift,
            meta,
        );
        let value = state.borrow().draft.value("launch_hotkey");
        assert_eq!(value, expected.map_or(before, |value| json!(value)));
        let merged = state
            .borrow()
            .draft
            .merged(&with(&host, |s| s.settings.clone()))
            .unwrap();
        crate::tray::parse_shortcuts(&merged).unwrap();
    }
    panel.invoke_discard();

    // Report editing resolves @index values and seeds every displayed default.
    panel
        .global::<PanelData>()
        .invoke_action("manual-report".into(), "".into());
    assert_eq!(state.borrow().report["provider"], "Claude Code");
    assert_eq!(state.borrow().report["frequency"], "Every time");
    panel
        .global::<PanelData>()
        .invoke_edit("report:area".into(), "@2".into());
    panel
        .global::<PanelData>()
        .invoke_edit("report:provider".into(), "@1".into());
    assert_eq!(state.borrow().report["area"], "Settings panel");
    assert_eq!(state.borrow().report["provider"], "Codex");
    assert!(
        !serde_json::to_string(&state.borrow().report)
            .unwrap()
            .contains("@1")
    );

    // The compiled calendar's emitted year-month-day shape is accepted and normalized.
    panel.set_page(4);
    panel.set_pane(5);
    panel
        .global::<PanelData>()
        .invoke_picked_date("holidays".into(), 2027, 1, 9);
    assert_eq!(
        state.borrow().draft.value("holidays"),
        json!(["2027-01-09"])
    );
    panel
        .global::<PanelData>()
        .invoke_edit("start_of_day".into(), "08:17".into());
    panel
        .global::<PanelData>()
        .invoke_edit("start_of_day".into(), "m:23".into());
    assert_eq!(state.borrow().draft.value("start_of_day"), "08:23");
    panel
        .global::<PanelData>()
        .invoke_edit("start_of_day".into(), "".into());
    assert_eq!(state.borrow().draft.value("start_of_day"), "");
    panel
        .global::<PanelData>()
        .invoke_edit("daily_target".into(), "00:00".into());
    assert!(!panel.invoke_persist());
    assert_eq!(registrations.get(), 0);
    panel
        .global::<PanelData>()
        .invoke_edit("daily_target".into(), "08:35".into());
    panel.set_pane(0);
    panel
        .global::<PanelData>()
        .invoke_edit("launch_hotkey".into(), "Ctrl+Shift+Y".into());
    with(&host, |s| {
        s.settings.notch_along = 0.77;
        s.settings.day_targets.insert(3, "06:17".into());
        s.settings.sounds = false;
    });
    state.borrow_mut().tick(&panel, &host, &notch);
    assert_eq!(
        state
            .borrow()
            .draft
            .merged(&with(&host, |s| s.settings.clone()))
            .unwrap()
            .day_targets[&3],
        "06:17"
    );
    panel.invoke_navigate(2);
    assert!(panel.get_leave_open());
    assert_eq!(panel.get_page(), 4);
    panel.invoke_cancel_modal();
    assert!(panel.get_dirty());
    panel.invoke_request_close();
    assert!(panel.get_leave_open());
    panel.invoke_cancel_modal();
    panel.invoke_discard();
    assert!(!panel.get_dirty());
    assert_eq!(state.borrow().draft.value("notch_along"), 0.77);

    // Focus and text survive model updates and destruction of the hidden panel surface.
    let questions = HookMsg {
        provider: "claude".into(),
        session_id: "session-0".into(),
        event: "PreToolUse".into(),
        tool_name: Some("AskUserQuestion".into()),
        tool_input: Some(
            json!({"questions":[{"question":"Pick tools","multiSelect":true,"options":[{"label":"Rust"},{"label":"Node"}]},{"question":"Next step","options":[{"label":"Test"}]}]}),
        ),
        ..Default::default()
    };
    let (tx, mut reply) = tokio::sync::oneshot::channel();
    let id = host.add_pending(&questions, tx, 60000);
    let id: slint::SharedString = id.to_string().into();
    notch.borrow_mut().update(&notch_ui);
    state.borrow_mut().tick(&panel, &host, &notch);
    panel.set_page(0);
    panel.global::<NotchData>().invoke_choose(id.clone(), 0);
    panel
        .global::<NotchData>()
        .invoke_custom(id.clone(), "Unicode ✓".into());
    panel.global::<NotchData>().invoke_hold(id.clone(), true);
    notch_ui.global::<NotchData>().invoke_hold(id.clone(), true);
    panel.global::<NotchData>().invoke_hold(id.clone(), false);
    assert!(
        with(&host, |s| s.pending[&id.parse::<u64>().unwrap()].held),
        "panel release preserves notch hold"
    );
    panel.global::<NotchData>().invoke_hold(id.clone(), true);
    notch.borrow().release_holds();
    assert!(with(&host, |s| s.pending[&id.parse::<u64>().unwrap()].held));
    let rows = panel.global::<NotchData>().get_requests();
    let row = rows.row_data(0).unwrap();
    assert!(row.question);
    let options = row.options;
    state.borrow_mut().tick(&panel, &host, &notch);
    assert!(Rc::ptr_eq(
        &rows
            .as_any()
            .downcast_ref::<VecModel<NotchRequest>>()
            .map(|_| state.borrow().requests.clone())
            .unwrap(),
        &state.borrow().requests
    ));
    assert_eq!(rows.row_data(0).unwrap().custom, "Unicode ✓");
    assert!(options.row_data(0).unwrap().selected);
    panel.set_page(4);
    panel.set_pane(4);
    panel
        .global::<PanelData>()
        .invoke_edit("dnd_hotkey".into(), "Ctrl+Alt+Y".into());
    state.borrow_mut().retain(&panel);
    panel.hide().unwrap();
    notch.borrow_mut().release_panel_holds();
    drop(panel);
    eprintln!("panel: create");
    let panel = ParityPanel::new().unwrap();
    Panel::bind(&panel, state.clone(), services);
    panel.show().unwrap();
    panel.invoke_take_focus();
    eprintln!("panel: shown");
    assert!(panel.get_dirty());
    assert_eq!(panel.get_pane(), 4);
    assert_eq!(state.borrow().draft.value("dnd_hotkey"), "Ctrl+Alt+Y");
    panel.set_page(1);
    assert_eq!(panel.get_session_rows().row_count(), 14);
    panel.set_provider_filter(2);
    panel.invoke_filter_changed(2);
    assert_eq!(panel.get_session_rows().row_count(), 7);
    assert_eq!(
        panel
            .global::<NotchData>()
            .get_requests()
            .row_data(0)
            .unwrap()
            .custom,
        "Unicode ✓"
    );
    panel.global::<NotchData>().invoke_submit(id.clone());
    panel.global::<NotchData>().invoke_choose(id, 0);
    let reply = reply.try_recv().unwrap();
    assert_eq!(reply.decision, "answer");
    let answers = reply.answers.unwrap();
    assert_eq!(answers["Pick tools"], "Rust, Unicode ✓");
    assert_eq!(answers["Next step"], "Test");
    panel.invoke_discard();
    host.take_dirty();
    notch.borrow_mut().release_panel_holds();
    state.borrow_mut().tick(&panel, &host, &notch);
    state.borrow_mut().tick(&panel, &host, &notch);
    assert!(
        !host.take_dirty(),
        "empty panel holds must not create idle refresh loop"
    );
    assert!(
        panel
            .global::<NotchData>()
            .get_providers()
            .row_data(0)
            .unwrap()
            .status
            .is_empty()
    );
    assert_eq!(
        panel.global::<PanelData>().get_usage_age(),
        "Updated 1 min ago"
    );
    assert_eq!(
        panel
            .global::<NotchData>()
            .get_providers()
            .row_data(0)
            .unwrap()
            .windows
            .row_count(),
        4
    );
    assert_eq!(panel.global::<PanelData>().get_alerts().row_count(), 2);

    panel.hide().unwrap();
}

#[test]
fn compiled_panel_dialog_traversal_and_opener_restore() {
    use slint::private_unstable_api::re_exports::WindowInner;
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(Offscreen(windows.clone()))).unwrap();
    let panel = ParityPanel::new().unwrap();
    panel.show().unwrap();
    let window = windows.borrow().last().unwrap().clone();
    render(&window, 1., 1100, 760, None);
    click(&window, 44., 712.);
    let focus = || {
        WindowInner::from_pub(panel.window())
            .focus_item
            .borrow()
            .upgrade()
    };
    let opener = focus();
    assert!(opener.is_some());
    panel.set_dirty(true);
    panel.invoke_request_close();
    assert!(panel.get_leave_open());
    key(&window, Key::Tab.into());
    let first = focus();
    assert!(first.is_some());
    for _ in 0..3 {
        key(&window, Key::Tab.into());
    }
    assert_eq!(focus(), first, "Tab stays inside three-action dialog");
    window.dispatch_event(WindowEvent::KeyPressed {
        text: Key::Shift.into(),
    });
    for _ in 0..3 {
        key(&window, Key::Tab.into());
    }
    window.dispatch_event(WindowEvent::KeyReleased {
        text: Key::Shift.into(),
    });
    assert_eq!(focus(), first, "Shift+Tab stays inside dialog");
    key(&window, Key::Escape.into());
    assert!(!panel.get_leave_open());
    assert_eq!(focus(), opener, "cancel restores opener");
    panel.hide().unwrap();
}

#[test]
fn session_copy_preserves_actual_directory() {
    let (host, _reply, _) = fixture();
    assert_eq!(
        crate::panel_services::session_path(&host, "claude:session-0").as_deref(),
        Some("C:/projects/folder-0")
    );
}

#[test]
fn compiled_panel_independent_render_matrix() {
    let windows = Rc::new(RefCell::new(Vec::new()));
    slint::platform::set_platform(Box::new(Offscreen(windows.clone()))).unwrap();
    let (host, _reply, _) = fixture();
    let notch_ui = NotchWindow::new().unwrap();
    let notch = Notch::connect(&notch_ui, host.clone());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let panel = ParityPanel::new().unwrap();
    let state = Panel::new(&with(&host, |s| s.settings.clone()));
    Panel::bind(
        &panel,
        state.clone(),
        Services {
            host: host.clone(),
            runtime: runtime.handle().clone(),
            register: Rc::new(|_| Ok(())),
            notch: notch.clone(),
            notch_ui: notch_ui.as_weak(),
        },
    );
    panel.show().unwrap();
    let window = windows.borrow().last().unwrap().clone();
    panel
        .global::<PanelData>()
        .invoke_action("manual-report".into(), "".into());
    let questions = HookMsg {
        provider: "claude".into(),
        session_id: "session-1".into(),
        event: "PreToolUse".into(),
        tool_name: Some("AskUserQuestion".into()),
        tool_input: Some(
            json!({"questions":[{"question":"What should happen next?","multiSelect":true,"options":[{"label":"Run tests"},{"label":"Review changes"}]}]}),
        ),
        ..Default::default()
    };
    render_matrix(&panel, &state, &host, &notch, &window, &questions);
}

fn render_matrix(
    panel: &ParityPanel,
    state: &Rc<RefCell<Panel>>,
    host: &NativeHost,
    notch: &Rc<RefCell<Notch>>,
    window: &MinimalSoftwareWindow,
    questions: &HookMsg,
) {
    let (tx, _render_reply) = tokio::sync::oneshot::channel();
    host.add_pending(
        &HookMsg {
            provider: "claude".into(),
            session_id: "session-0".into(),
            event: "PermissionRequest".into(),
            tool_name: Some("Bash".into()),
            tool_input: Some(json!({"command":"cargo test -p nubi-app"})),
            ..Default::default()
        },
        tx,
        60000,
    );
    let (tx, _question_reply) = tokio::sync::oneshot::channel();
    host.add_pending(questions, tx, 60000);
    panel.set_page(0);
    state.borrow_mut().tick(panel, host, notch);
    let requests = panel.global::<NotchData>().get_requests();
    assert!((0..requests.row_count()).any(|index| requests.row_data(index).unwrap().question));
    assert!((0..requests.row_count()).any(|index| !requests.row_data(index).unwrap().question));
    panel.global::<PanelData>().set_release_loading(false);
    panel.global::<PanelData>().set_release_status("".into());
    panel.global::<PanelData>().set_releases(settings::model(crate::panel_services::release_rows(&json!([
        {"version":"0.1.3","current":true,"date":"2026-11-20","notes":"https://github.com/hitraj2003/nubi/releases/tag/v0.1.3"},
        {"version":"0.1.2","current":false,"date":"2026-11-19","notes":"https://github.com/hitraj2003/nubi/releases/tag/v0.1.2","download":"https://github.com/hitraj2003/nubi/releases/download/v0.1.2/nubi.exe"}
    ]))));
    let folder = std::env::var_os("NUBI_PANEL_RENDER_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("nubi-native-20261120-stage6-panel/renders"));
    std::fs::create_dir_all(&folder).unwrap();
    let focused = std::env::var_os("NUBI_PANEL_RENDER_FOCUSED").is_some();
    let scales: &[f32] = if focused {
        &[1.]
    } else {
        &[1., 1.25, 1.5, 1.75]
    };
    let sizes: &[(u32, u32)] = if focused {
        &[(1100, 760)]
    } else {
        &[(1100, 760), (900, 580), (1400, 920)]
    };
    let mut count = 0;
    for dark in [false, true] {
        with(host, |s| {
            s.settings.theme = if dark { "dark" } else { "light" }.into()
        });
        for &scale in scales {
            for &(width, height) in sizes {
                for view in (0..12).filter(|view| *view != 9) {
                    let (page, pane) = if view < 4 { (view, 0) } else { (4, view - 4) };
                    panel.set_page(page);
                    panel.set_pane(pane);
                    state.borrow_mut().tick(panel, host, notch);
                    panel.global::<Theme>().set_reduced(true);
                    let path = folder.join(format!(
                        "panel-{}-{view}-{}-{width}x{height}.png",
                        if dark { "dark" } else { "light" },
                        (scale * 100.) as i32
                    ));
                    render(window, scale, width, height, Some(&path));
                    count += 1;
                }
            }
        }
    }
    panel.global::<PanelData>().set_report_open(true);
    slint::platform::update_timers_and_animations();
    render(window, 1., 1100, 760, Some(&folder.join("report-form.png")));
    panel.invoke_cancel_modal();
    panel.hide().unwrap();
    assert_eq!(count, if focused { 22 } else { 264 });
}

#[test]
fn save_transaction_rolls_back_registration_and_startup_without_publishing() {
    let before = Settings::default();
    let next = Settings {
        auto_start: !before.auto_start,
        ..before.clone()
    };
    let events = RefCell::new(Vec::new());
    let result = transaction(
        &before,
        &next,
        |s| {
            events.borrow_mut().push(format!("keys:{}", s.auto_start));
            Ok(())
        },
        |on| {
            events.borrow_mut().push(format!("start:{on}"));
            Ok(())
        },
        |_| Err("disk full".into()),
    );
    assert_eq!(result.unwrap_err(), "disk full");
    assert_eq!(
        *events.borrow(),
        ["keys:true", "start:true", "keys:false", "start:false"]
    );
    events.borrow_mut().clear();
    let result = transaction(
        &before,
        &next,
        |s| {
            events.borrow_mut().push(s.auto_start.to_string());
            if s.auto_start {
                Err("hotkey busy".into())
            } else {
                Ok(())
            }
        },
        |_| panic!("startup after rejected registration"),
        |_| panic!("published failed registration"),
    );
    assert_eq!(result.unwrap_err(), "hotkey busy");
    assert_eq!(*events.borrow(), ["true", "false"]);
}

#[test]
fn legacy_one_sided_quiet_hours_and_shortcut_capture_remain_valid() {
    let settings = Settings {
        quiet_start: Some(23),
        quiet_end: None,
        ..Default::default()
    };
    let mut draft = Draft::new(&settings);
    draft.edit("sounds", json!(false));
    assert!(draft.merged(&settings).is_ok());
    assert_eq!(
        capture("y", true, false, true, false).as_deref(),
        Some("Ctrl+Shift+Y")
    );
    assert_eq!(
        capture(
            &slint::SharedString::from(Key::F8),
            true,
            true,
            false,
            false
        )
        .as_deref(),
        Some("Ctrl+Alt+F8")
    );
    assert_eq!(
        capture(
            &slint::SharedString::from(Key::Backspace),
            false,
            false,
            false,
            false
        )
        .as_deref(),
        Some("")
    );
    for (key, label) in [
        (Key::F13, "F13"),
        (Key::F14, "F14"),
        (Key::F15, "F15"),
        (Key::F16, "F16"),
        (Key::F17, "F17"),
        (Key::F18, "F18"),
    ] {
        assert_eq!(
            capture(&slint::SharedString::from(key), true, false, false, false),
            Some(format!("Ctrl+{label}"))
        );
    }
    for key in [Key::F19, Key::F24] {
        assert!(capture(&slint::SharedString::from(key), true, false, false, false).is_none());
    }
    for (plain, shifted, label) in [
        ("`", "~", "Backquote"),
        ("\\", "|", "Backslash"),
        ("[", "{", "BracketLeft"),
        ("]", "}", "BracketRight"),
        (",", "<", "Comma"),
        ("=", "+", "Equal"),
        ("-", "_", "Minus"),
        (".", ">", "Period"),
        ("'", "\"", "Quote"),
        (";", ":", "Semicolon"),
        ("/", "?", "Slash"),
    ] {
        assert_eq!(
            capture(plain, true, false, false, false),
            Some(format!("Ctrl+{label}"))
        );
        assert_eq!(
            capture(shifted, true, false, true, false),
            Some(format!("Ctrl+Shift+{label}"))
        );
    }
    assert!(
        capture(
            &slint::SharedString::from(Key::Control),
            true,
            false,
            false,
            false
        )
        .is_none()
    );
}

#[test]
#[ignore = "explicit isolated Win32 message test; no physical input"]
fn ordinary_panel_win32_buttons_use_their_own_coordinates() {
    use slint::winit_030::{
        WinitWindowAccessor, winit::platform::windows::EventLoopBuilderExtWindows,
    };
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_event_loop_builder({
            let mut builder = slint::winit_030::winit::event_loop::EventLoop::with_user_event();
            builder.with_any_thread(true);
            builder
        })
        .with_winit_window_attributes_hook(|attributes| attributes.with_active(false))
        .select()
        .unwrap();
    let panel = ParityPanel::new().unwrap();
    panel.show().unwrap();
    panel.invoke_take_focus();
    eprintln!("panel: shown");
    fn check_step(weak: slint::Weak<ParityPanel>, step: usize) {
        let panel = weak.upgrade().unwrap();
        if step == 3 {
            panel
                .window()
                .with_winit_window(|window| window.set_visible(false));
            panel.hide().unwrap();
            slint::quit_event_loop().unwrap();
            return;
        }
        let handle = crate::ui::hwnd(panel.window()).expect("panel HWND");
        crate::native::repair_panel_pointer(handle).unwrap();
        let scale = panel.window().scale_factor();
        let packed = |x: f32, y: f32| {
            ((x * scale).round() as i16 as u16 as isize)
                | (((y * scale).round() as i16 as u16 as isize) << 16)
        };
        let (page, y) = [(1, 144.), (2, 188.), (3, 232.)][step];
        panel.set_page(4);
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;
            SendMessageW(handle as _, 0x0200, 0, packed(700., 500.));
            SendMessageW(handle as _, 0x0201, 1, packed(44., y));
            SendMessageW(handle as _, 0x0202, 0, packed(44., y));
        }
        // Winit queues native input. Let its event loop deliver the messages
        // before checking Slint callbacks or sending another sequence.
        slint::Timer::single_shot(Duration::from_millis(60), move || {
            assert_eq!(
                weak.upgrade().unwrap().get_page(),
                page,
                "stale backend position must not redirect panel button"
            );
            check_step(weak, step + 1);
        });
    }
    let weak = panel.as_weak();
    slint::Timer::single_shot(Duration::from_millis(120), move || check_step(weak, 0));
    slint::run_event_loop().unwrap();
}

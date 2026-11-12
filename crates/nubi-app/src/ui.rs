//! UI-thread ownership of Slint surfaces and native tray/keyboard resources.
use crate::{
    host::{NativeHost, UiEvent},
    motion::Motion,
    native,
    singleton::Action,
    tray::{Shortcuts, Tray},
    updater, Fixture, NativeLauncher, NativeUpdate, NotchWindow, ParityPanel, Session, Theme,
};
use nubi_core::host::with;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::winit_030::winit::platform::windows::WindowAttributesExtWindows;
use slint::{ComponentHandle, Timer, TimerMode, VecModel};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::mpsc::Receiver,
    time::{Duration, Instant},
};

fn hwnd(window: &slint::Window) -> Option<isize> {
    let owner = window.window_handle();
    let handle = owner.window_handle().ok()?;
    if let RawWindowHandle::Win32(handle) = handle.as_raw() {
        Some(handle.hwnd.get())
    } else {
        None
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

fn dark(theme: &str) -> bool {
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
    update: Option<NativeUpdate>,
    update_id: Option<u64>,
    update_shown: Option<Instant>,
    panel_hidden: Option<Instant>,
    draft: Option<PanelDraft>,
}

// Preserve editable values and navigation while releasing the hidden native surface.
struct PanelDraft {
    start: bool,
    update: bool,
    launcher: bool,
    providers: i32,
    hotkey: slint::SharedString,
    dirty: bool,
    page: i32,
    pane: i32,
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
            crate::panel_logic::connect(&panel);
            let settings = with(&self.host, |state| state.settings.clone());
            panel.global::<Theme>().set_dark(dark(&settings.theme));
            panel
                .global::<Theme>()
                .set_reduced(settings.reduce_motion || native::system_reduced_motion());
            panel.set_auto_start(settings.auto_start);
            panel.set_saved_start(settings.auto_start);
            panel.set_auto_update(settings.auto_update);
            panel.set_saved_update(settings.auto_update);
            panel.set_launcher(settings.launcher_enabled);
            panel.set_saved_launcher(settings.launcher_enabled);
            let providers = match settings.providers.as_str() {
                "claude" => 1,
                "codex" => 2,
                _ => 0,
            };
            panel.set_providers(providers);
            panel.set_saved_providers(providers);
            panel.set_hotkey(settings.launch_hotkey.as_str().into());
            panel.set_saved_hotkey(settings.launch_hotkey.into());
            panel.set_page(4);
            panel.set_app_version(env!("CARGO_PKG_VERSION").into());
            models(&panel, &self.host);
            if let Some(draft) = self.draft.take() {
                panel.set_auto_start(draft.start);
                panel.set_auto_update(draft.update);
                panel.set_launcher(draft.launcher);
                panel.set_providers(draft.providers);
                panel.set_hotkey(draft.hotkey);
                panel.set_dirty(draft.dirty);
                panel.set_page(draft.page);
                panel.set_pane(draft.pane);
            }
            let weak = panel.as_weak();
            let host = self.host.clone();
            let shortcuts = self.shortcuts.clone();
            panel.on_persist(move || {
                let Some(panel) = weak.upgrade() else {
                    return;
                };
                let previous = with(&host, |state| state.settings.clone());
                let mut settings = previous.clone();
                let previous_start = settings.auto_start;
                settings.auto_start = panel.get_auto_start();
                settings.auto_update = panel.get_auto_update();
                settings.launcher_enabled = panel.get_launcher();
                settings.launch_hotkey = panel.get_hotkey().to_string();
                settings.providers = match panel.get_providers() {
                    1 => "claude",
                    2 => "codex",
                    _ => "both",
                }
                .into();
                let result = shortcuts
                    .borrow_mut()
                    .apply(&settings)
                    .and_then(|_| {
                        if previous_start != settings.auto_start {
                            nubi_core::settings::apply_auto_start(settings.auto_start)
                        } else {
                            Ok(())
                        }
                    })
                    .and_then(|_| host.save_settings(settings.clone()));
                if result.is_err() {
                    let _ = shortcuts.borrow_mut().apply(&previous);
                    if previous_start != settings.auto_start {
                        let _ = nubi_core::settings::apply_auto_start(previous_start);
                    }
                }
                panel.set_status(result.err().unwrap_or_else(|| "Saved".into()).into());
            });
            let host = self.host.clone();
            panel.on_refresh(move || nubi_core::poller::force(&host));
            let host = self.host.clone();
            panel.on_toggle_dnd(move || host.toggle_dnd());
            let host = self.host.clone();
            panel.on_test_alert(move || {
                host.alert(nubi_core::state::Alert {
                    ts: nubi_core::state::now_ms(),
                    provider: "claude".into(),
                    kind: "done".into(),
                    title: "Task complete".into(),
                    body: "test notification".into(),
                    session_key: None,
                    queued: false,
                })
            });
            let host = self.host.clone();
            let runtime = self.runtime.clone();
            let weak = panel.as_weak();
            panel.on_check_update(move || {
                let host = host.clone();
                let weak = weak.clone();
                runtime.spawn(async move {
                    let message = updater::check_update(host)
                        .await
                        .unwrap_or_else(|error| error);
                    let _ =
                        weak.upgrade_in_event_loop(move |panel| panel.set_status(message.into()));
                });
            });
            panel.on_release_notes(|| {
                let _ = std::process::Command::new("explorer")
                    .arg(format!(
                        "https://github.com/hitraj2003/nubi/releases/tag/v{}",
                        env!("CARGO_PKG_VERSION")
                    ))
                    .spawn();
            });
            let host = self.host.clone();
            panel.on_terminal(move |key| jump(&host, &key));
            panel
                .window()
                .on_close_requested(|| slint::CloseRequestResponse::HideWindow);
            self.panel = Some(panel);
        }
        let panel = self.panel.as_ref().unwrap();
        panel.show()?;
        foreground(panel.window());
        Ok(())
    }

    fn release_hidden(&mut self) {
        if let Some(panel) = &self.panel {
            if panel.window().is_visible() {
                self.panel_hidden = None;
            } else {
                let since = self.panel_hidden.get_or_insert_with(Instant::now);
                if since.elapsed() >= Duration::from_secs(60) {
                    self.draft = Some(PanelDraft {
                        start: panel.get_auto_start(),
                        update: panel.get_auto_update(),
                        launcher: panel.get_launcher(),
                        providers: panel.get_providers(),
                        hotkey: panel.get_hotkey(),
                        dirty: panel.get_dirty(),
                        page: panel.get_page(),
                        pane: panel.get_pane(),
                    });
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
        ui.global::<Theme>().set_dark(dark(&settings.notch_theme));
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
                match nubi_core::session_actions::open_terminal(&ui.get_dir(), Some(&cli)) {
                    Ok(()) => {
                        let _ = std::fs::create_dir_all(nubi_core::settings::data_dir());
                        let _ = std::fs::write(
                            nubi_core::settings::data_dir().join("launcher-cli"),
                            cli.as_bytes(),
                        );
                        let _ = ui.hide();
                    }
                    Err(error) => ui.set_error(error.into()),
                }
            }
        });
        ui.show()?;
        foreground(ui.window());
        ui.invoke_take_focus();
        self.launcher = Some(ui);
        Ok(())
    }

    fn update(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(notice) = updater::get_update_notice(self.host.clone()) else {
            self.update = None;
            self.update_id = None;
            self.update_shown = None;
            return Ok(());
        };
        if self.update_id != Some(notice.id) {
            let ui = NativeUpdate::new()?;
            ui.global::<Theme>()
                .set_dark(dark(&with(&self.host, |state| {
                    state.settings.notch_theme.clone()
                })));
            let id = notice.id;
            let host = self.host.clone();
            ui.on_cancel(move || updater::dismiss_update(host.clone(), id));
            let host = self.host.clone();
            ui.on_hold(move |on| updater::hold_update(host.clone(), id, on));
            let host = self.host.clone();
            let runtime = self.runtime.clone();
            ui.on_install(move || {
                let host = host.clone();
                runtime.spawn(async move {
                    if let Err(error) = updater::install_update(host, id).await {
                        eprintln!("[nubi] update: {error}");
                    }
                });
            });
            let version = notice.version.clone();
            ui.on_release_notes(move || {
                let _ = std::process::Command::new("explorer")
                    .arg(format!(
                        "https://github.com/hitraj2003/nubi/releases/tag/v{version}"
                    ))
                    .spawn();
            });
            let host = self.host.clone();
            ui.window().on_close_requested(move || {
                updater::dismiss_update(host.clone(), id);
                slint::CloseRequestResponse::KeepWindowShown
            });
            self.update = Some(ui);
            self.update_id = Some(id);
            self.update_shown = Some(Instant::now());
        }
        let ui = self.update.as_ref().unwrap();
        ui.set_version(notice.version.into());
        ui.set_notes(if notice.notes.trim().is_empty() {
            "Release notes are available on the release page.".into()
        } else {
            notice.notes.into()
        });
        ui.set_phase(notice.phase.into());
        ui.set_message(notice.message.into());
        let seconds = notice
            .expires_ms
            .saturating_sub(nubi_core::state::now_ms())
            .div_ceil(1000);
        ui.set_countdown(if notice.expires_ms == 0 {
            "".into()
        } else if notice.held {
            "Paused while you read".into()
        } else {
            format!(
                "{} in {seconds}s",
                if notice.automatic {
                    "Updates"
                } else {
                    "Dismisses"
                }
            )
            .into()
        });
        ui.show()?;
        ui.set_focused(hwnd(ui.window()).is_some_and(|handle| unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() as isize == handle
        }));
        if self
            .update_shown
            .is_some_and(|shown| shown.elapsed() >= Duration::from_millis(250))
            && hwnd(ui.window()).is_some()
        {
            self.update_shown = None;
            updater::update_notice_ready(self.host.clone(), notice.id);
        }
        Ok(())
    }
}

fn jump(host: &NativeHost, key: &str) {
    let pids = with(host, |state| {
        state.sessions.get(key).map(|session| session.pids.clone())
    });
    if let Some(pids) = pids {
        nubi_core::winutil::focus_pids(&pids);
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
            } else if attributes.title == "Nubi update" {
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
    ui.global::<Theme>().set_dark(dark(&settings.notch_theme));
    let reduced = settings.reduce_motion || native::system_reduced_motion();
    ui.global::<Theme>().set_reduced(reduced);
    models(&ui, &host);
    ui.set_sessions(ui.global::<Fixture>().get_sessions());
    let motion = Rc::new(RefCell::new(Motion::new(false)));
    motion.borrow().draw(&ui);
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
        update: None,
        update_id: None,
        update_shown: None,
        panel_hidden: None,
        draft: None,
    }));
    let surfaces_callback = surfaces.clone();
    ui.on_open_panel(move || {
        if let Err(error) = surfaces_callback.borrow_mut().settings() {
            eprintln!("[nubi] settings: {error}");
        }
    });
    let callback_host = host.clone();
    ui.on_toggle_dnd(move || callback_host.toggle_dnd());
    let callback_host = host.clone();
    ui.on_refresh_usage(move || nubi_core::poller::force(&callback_host));
    let callback_host = host.clone();
    ui.on_session_clicked(move |key| jump(&callback_host, &key));
    let callback_motion = motion.clone();
    ui.on_tab_selected(move |tab| callback_motion.borrow_mut().tab(tab, reduced));
    let callback_motion = motion.clone();
    ui.on_pill_clicked(move || callback_motion.borrow_mut().target(true, reduced));
    ui.window()
        .on_close_requested(|| slint::CloseRequestResponse::KeepWindowShown);
    ui.show()?;
    let tray = Tray::new(&host)?;
    let mut native_window: Option<native::NativeWindow> = None;
    let mut inside = false;
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
                        nubi_core::winutil::beep(&alert.kind);
                    }
                    Ok(())
                }
                UiEvent::Update => surfaces.borrow_mut().update(),
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
                "sounds" | "island" | "ring" | "taskbar" => {
                    let mut settings = with(&host, |state| state.settings.clone());
                    if event.id.0 == "sounds" {
                        settings.sounds = !settings.sounds;
                    } else {
                        settings.display_mode = event.id.0.clone();
                    }
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
            let _ = native.refresh_placement();
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
                motion.target(entering, reduced);
                hover = None;
            }
        }
        if motion.tick(now) {
            motion.draw(&ui);
        }
        let dirty = host.take_dirty();
        if dirty || (ui.get_opened() && last_refresh.elapsed() >= Duration::from_secs(1)) {
            last_refresh = now;
            models(&ui, &host);
            ui.set_sessions(ui.global::<Fixture>().get_sessions());
            let view = surfaces.borrow();
            if let Some(panel) = &view.panel {
                models(panel, &host);
                panel.set_dnd(with(&host, |state| crate::host::dnd_active(state)));
            }
            tray.refresh(&host);
            let settings = with(&host, |state| state.settings.clone());
            if let Err(error) = shortcuts.borrow_mut().apply(&settings) {
                eprintln!("[nubi] shortcuts: {error}");
            }
            drop(view);
            // Update notifications are replaceable state. Recover even if their wakeup
            // was coalesced while ordered activation events filled the channel.
            let _ = surfaces.borrow_mut().update();
        }
        if surfaces.borrow().update.is_some() {
            let _ = surfaces.borrow_mut().update();
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

//! Native panel controller. Stable models and drafts outlive the hidden surface.
#[cfg(test)]
#[path = "panel_tests.rs"]
mod tests;
use crate::{
    CalendarDay, NotchData, NotchProvider, NotchRequest, NotchSession, PanelAlert, PanelData, PanelField,
    ParityPanel, Theme,
    host::NativeHost,
    notch::Notch,
    panel_settings::{self as settings, Draft},
};
use nubi_core::{host::with, settings::Settings, state::now_ms};
use serde_json::{Map, Value, json};
use slint::{ComponentHandle, Model, VecModel};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    time::{Duration, Instant},
};

// Slint 1.18 drops focus when a request repeater removes its focused control.
// Restore the shell only when no live control retains focus.
fn ensure_focus(panel: &ParityPanel) {
    use slint::private_unstable_api::re_exports::WindowInner;
    let inner = WindowInner::from_pub(panel.window());
    if inner
        .focus_item
        .borrow()
        .upgrade()
        .is_none_or(|item| !item.is_visible())
        && !panel.get_leave_open()
        && !panel.global::<PanelData>().get_report_open()
        && panel.get_confirmation_key().is_empty()
    {
        panel.invoke_take_focus();
    }
}

pub fn connect(panel: &ParityPanel) {
    panel
        .global::<crate::Fixture>()
        .on_usage_color(|percent, soft| crate::usage_color::color(f64::from(percent), soft));
}
pub struct Panel {
    pub draft: Draft,
    fields: Vec<Rc<VecModel<PanelField>>>,
    requests: Rc<VecModel<NotchRequest>>,
    sessions: Rc<VecModel<NotchSession>>,
    providers: Rc<VecModel<NotchProvider>>,
    alerts: Rc<VecModel<PanelAlert>>,
    pub report: Map<String, Value>,
    report_rows: Rc<VecModel<PanelField>>,
    errors: BTreeMap<String, String>,
    monitors: Vec<String>,
    refresh: Option<(Instant, u64)>,
    pub page: i32,
    pub pane: i32,
    pub filter: i32,
    pub report_text: slint::SharedString,
    pub report_fields: bool,
}
impl Panel {
    pub fn new(current: &Settings) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            draft: Draft::new(current),
            fields: (0..8).map(|_| Rc::default()).collect(),
            requests: Rc::default(),
            sessions: Rc::default(),
            providers: Rc::default(),
            alerts: Rc::default(),
            report: Map::new(),
            report_rows: Rc::default(),
            errors: BTreeMap::new(),
            monitors: vec!["Display 1".into()],
            refresh: None,
            page: 4,
            pane: 0,
            filter: 0,
            report_text: Default::default(),
            report_fields: false,
        }))
    }
    pub fn retain(&mut self, panel: &ParityPanel) {
        self.page = panel.get_page();
        self.pane = panel.get_pane();
        self.filter = panel.get_provider_filter();
        self.report_text = panel.global::<PanelData>().get_report_text();
        self.report_fields = panel.global::<PanelData>().get_report_fields();
    }
    pub fn bind(
        panel: &ParityPanel,
        owner: Rc<RefCell<Self>>,
        services: crate::panel_services::Services,
    ) {
        connect(panel);
        let data = panel.global::<PanelData>();
        // LOCALE_ITIME reads the current user's Windows 12/24-hour override.
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetLocaleInfoEx(locale: *const u16, kind: u32, buffer: *mut u16, length: i32) -> i32;
        }
        let mut clock = [0u16; 2];
        let read = unsafe { GetLocaleInfoEx(std::ptr::null(), 0x23, clock.as_mut_ptr(), 2) };
        data.set_clock_24(read > 0 && clock[0] == u16::from(b'1'));
        data.set_development(nubi_core::isolation::development());
        panel.set_app_version(env!("CARGO_PKG_VERSION").into());
        {
            let this = owner.borrow();
            panel.set_page(this.page);
            panel.set_pane(this.pane);
            panel.set_provider_filter(this.filter);
            panel.set_session_rows(this.sessions.clone().into());
            data.set_alerts(this.alerts.clone().into());
            data.set_report_rows(this.report_rows.clone().into());
            data.set_report_text(this.report_text.clone());
            data.set_report_fields(this.report_fields);
            panel
                .global::<NotchData>()
                .set_requests(this.requests.clone().into());
            panel
                .global::<NotchData>()
                .set_providers(this.providers.clone().into());
        }
        {
            use chrono::Datelike;
            let month = std::rc::Rc::new(std::cell::Cell::new(0i32));
            let weak = panel.as_weak();
            let shown = month.clone();
            data.on_calendar_move(move |delta| {
                let Some(panel) = weak.upgrade() else { return };
                let today = chrono::Local::now().date_naive();
                let current = today.year() * 12 + today.month0() as i32;
                shown.set(if delta == 0 { current } else { shown.get() + delta });
                let (year, month0) = (shown.get().div_euclid(12), shown.get().rem_euclid(12) as u32);
                let Some(first) = chrono::NaiveDate::from_ymd_opt(year, month0 + 1, 1) else { return };
                let lead = first.weekday().num_days_from_monday() as i64;
                let days: Vec<CalendarDay> = (0..42)
                    .map(|cell| {
                        let date = first + chrono::Duration::days(cell - lead);
                        if date.month0() == month0 {
                            CalendarDay {
                                label: date.day().to_string().into(),
                                date: date.format("%Y-%m-%d").to_string().into(),
                            }
                        } else {
                            CalendarDay::default()
                        }
                    })
                    .collect();
                let data = panel.global::<PanelData>();
                data.set_calendar(std::rc::Rc::new(VecModel::from(days)).into());
                data.set_calendar_title(first.format("%B %Y").to_string().into());
            });
            let weak = panel.as_weak();
            data.on_calendar_pick(move |date| {
                if let Some(panel) = weak.upgrade() {
                    let data = panel.global::<PanelData>();
                    data.invoke_edit(data.get_date_field(), date);
                }
            });
        }
        let weak = panel.as_weak();
        data.on_picked_date(move |key, year, month, day| {
            if let Some(panel) = weak.upgrade() {
                panel
                    .global::<PanelData>()
                    .invoke_edit(key, format!("{year:04}-{month:02}-{day:02}").into());
            }
        });
        macro_rules! edit {
            ($method:ident, $body:expr) => {{
                let owner = owner.clone();
                let weak = panel.as_weak();
                data.$method(move |key, value| {
                    if let Some(panel) = weak.upgrade() {
                        let mut this = owner.borrow_mut();
                        let result = ($body)(&mut *this, key.as_str(), value.as_str());
                        this.result(&panel, &key, result);
                    }
                });
            }};
        }
        edit!(on_edit, |this: &mut Self, key: &str, value: &str| this
            .edit(key, value));
        edit!(on_choice, |this: &mut Self, key: &str, value: &str| this
            .draft
            .choice(key, value));
        edit!(on_remove_date, |this: &mut Self, key: &str, value: &str| {
            this.draft.date(key, value, true)
        });
        let weak = panel.as_weak();
        let captured = owner.clone();
        data.on_capture(move |key, text, control, alt, shift, meta| {
            if let Some(panel) = weak.upgrade() {
                if let Some(shortcut) = capture(&text, control, alt, shift, meta) {
                    let mut this = captured.borrow_mut();
                    let result = this.edit(&key, &shortcut);
                    this.result(&panel, &key, result);
                    return true;
                }
            }
            false
        });
        let weak = panel.as_weak();
        let panes = owner.clone();
        panel.on_pane_changed(move |pane| {
            if let Some(panel) = weak.upgrade() {
                panel.global::<PanelData>().set_fields(
                    panes.borrow().fields[pane.clamp(0, 7) as usize]
                        .clone()
                        .into(),
                );
            }
        });
        let weak = panel.as_weak();
        let state = owner.clone();
        let host = services.host.clone();
        panel.on_filter_changed(move |_| {
            if let Some(panel) = weak.upgrade() {
                state.borrow_mut().live(&panel, &host);
            }
        });
        let weak = panel.as_weak();
        let state = owner.clone();
        let host = services.host.clone();
        panel.on_discard(move || {
            if let Some(panel) = weak.upgrade() {
                let mut this = state.borrow_mut();
                this.draft = Draft::new(&with(&host, |s| s.settings.clone()));
                this.errors.clear();
                this.sync_fields(&panel);
                panel.set_status("".into());
            }
        });
        let weak = panel.as_weak();
        let state = owner.clone();
        let saved = services.clone();
        panel.on_persist(move || {
            let Some(panel) = weak.upgrade() else {
                return false;
            };
            let mut this = state.borrow_mut();
            let result = this.save(&saved);
            match result {
                Ok(()) => {
                    this.sync_fields(&panel);
                    panel.set_status("Saved".into());
                    let weak = panel.as_weak();
                    slint::Timer::single_shot(std::time::Duration::from_secs(2), move || {
                        if let Some(panel) = weak.upgrade() {
                            if panel.get_status() == "Saved" {
                                panel.set_status("".into());
                            }
                        }
                    });
                    true
                }
                Err(error) => {
                    panel.set_status(format!("Not saved: {error}").into());
                    false
                }
            }
        });
        let weak = panel.as_weak();
        let notch = services.notch.clone();
        panel.on_hide_panel(move || {
            notch.borrow_mut().release_panel_holds();
            if let Some(panel) = weak.upgrade() {
                let _ = panel.hide();
            }
        });
        let weak = panel.as_weak();
        panel.window().on_close_requested(move || {
            if let Some(panel) = weak.upgrade() {
                panel.invoke_request_close();
            }
            slint::CloseRequestResponse::KeepWindowShown
        });
        crate::panel_services::connect(panel, owner.clone(), services.clone());
        bridge(panel, owner.clone(), &services);
        owner
            .borrow_mut()
            .tick(panel, &services.host, &services.notch);
        // Match the original panel startup, independently of About navigation.
        data.invoke_action("releases".into(), "".into());
    }
    fn edit(&mut self, key: &str, value: &str) -> Result<(), String> {
        if key.starts_with("report:") {
            let value = if let Some(index) = value.strip_prefix('@') {
                self.report_rows
                    .iter()
                    .find(|row| row.key == key)
                    .and_then(|row| row.options.row_data(index.parse().unwrap_or(0)))
                    .map(|v| v.to_string())
                    .ok_or("Invalid selection")?
            } else {
                value.to_string()
            };
            self.report
                .insert(key.trim_start_matches("report:").into(), json!(value));
            self.sync_report();
            return Ok(());
        }
        let spec = settings::SPECS
            .iter()
            .find(|s| s.key == key)
            .ok_or("Unknown setting")?;
        if spec.kind == "readonly" {
            return Ok(());
        }
        if spec.kind == "dates" {
            let date = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|_| "Choose a valid date")?
                .format("%Y-%m-%d")
                .to_string();
            return self.draft.date(key, &date, false);
        }
        if spec.kind == "hour" {
            if let Some(time) = value.strip_prefix("time=") {
                let (h,m) = time.split_once(':').ok_or("Enter a time as HH:MM")?;
                let parse = |part: &str| part.len() <= 2 && !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
                if !parse(h) || !parse(m) { return Err("Enter a time as HH:MM".into()); }
                let hour = h.parse::<u8>().unwrap();
                let minute = m.parse::<u8>().unwrap();
                if hour > 24 || minute > 59 || (hour == 24 && minute != 0) {
                    return Err("Enter 00:00 through 23:59, or 24:00 for midnight".into());
                }
                self.draft.edit_text(key, &(hour % 24).to_string())?;
                return self.draft.edit_text(&format!("{key}_min"), &minute.to_string());
            }
            if let Some(minute) = value.strip_prefix("m:") {
                return self.draft.edit_text(&format!("{key}_min"), minute.trim_start_matches('0').parse::<u8>().unwrap_or(0).to_string().as_str());
            }
            if let Some(index) = value.strip_prefix("h:") {
                let index = index.parse::<usize>().map_err(|_| "Invalid hour")?;
                let hour = if index == 0 { String::new() } else { (index - 1).to_string() };
                return self.draft.edit_text(key, &hour);
            }
        }
        let mut value = value.to_string();
        if let Some(index) = value.strip_prefix('@') {
            let index = index.parse::<usize>().map_err(|_| "Invalid selection")?;
            value = match spec.kind {
                "hour" => {
                    if index == 0 {
                        String::new()
                    } else {
                        (index - 1).to_string()
                    }
                }
                "monitor" => index.to_string(),
                _ => spec.options.get(index).ok_or("Invalid selection")?.0.into(),
            };
        } else if spec.kind == "time" && (value.starts_with("h:") || value.starts_with("m:")) {
            let previous = settings::text(&self.draft.value(key));
            let (h, m) = previous.split_once(':').unwrap_or(("0", "0"));
            let h = value.strip_prefix("h:").unwrap_or(h);
            let m = value.strip_prefix("m:").unwrap_or(m);
            value = format!(
                "{:02}:{:02}",
                h.parse::<u8>().map_err(|_| "Invalid hour")?,
                m.parse::<u8>().map_err(|_| "Invalid minute")?
            );
        }
        self.draft.edit_text(key, &value)
    }
    fn result(&mut self, panel: &ParityPanel, key: &str, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.errors.remove(key);
                panel.set_status("".into());
            }
            Err(error) => {
                self.errors.insert(key.into(), error.clone());
                panel.set_status(error.into());
            }
        }
        self.sync_fields(panel);
    }
    fn sync_fields(&mut self, panel: &ParityPanel) {
        for pane in 0..8 {
            let mut rows: Vec<_> = settings::SPECS
                .iter()
                .filter(|s| s.pane == pane)
                .map(|spec| {
                    let mut row = settings::row(spec, &self.draft, &self.monitors);
                    row.error = self
                        .errors
                        .get(spec.key)
                        .cloned()
                        .unwrap_or_default()
                        .into();
                    row
                })
                .collect();
            for index in 0..rows.len() {
                rows[index].first = index == 0 || !rows[index].section.is_empty();
                rows[index].last = index + 1 == rows.len() || !rows[index + 1].section.is_empty();
            }
            sync_fields(&self.fields[pane as usize], rows);
        }
        panel.global::<PanelData>().set_fields(
            self.fields[panel.get_pane().clamp(0, 7) as usize]
                .clone()
                .into(),
        );
        panel.set_dirty(self.draft.dirty() || !self.errors.is_empty());
        crate::ui::apply_theme(
            panel.global::<Theme>(),
            &settings::text(&self.draft.value("theme")),
        );
        panel
            .global::<Theme>()
            .set_notch_accent(settings::text(&self.draft.value("accent")).into());
        panel
            .global::<Theme>()
            .set_panel_accent(settings::text(&self.draft.value("accent")).into());
        panel.global::<Theme>().set_reduced(
            self.draft.value("reduce_motion") == true || crate::native::system_reduced_motion(),
        );
    }
    pub fn tick(&mut self, panel: &ParityPanel, host: &NativeHost, notch: &Rc<RefCell<Notch>>) {
        self.draft
            .rebase(&with(host, |state| state.settings.clone()));
        self.sync_fields(panel);
        self.live(panel, host);
        let requests = notch.borrow_mut().panel_requests();
        crate::notch::models::sync_requests(&self.requests, requests);
        panel
            .global::<NotchData>()
            .set_pending_count(self.requests.row_count() as i32);
        ensure_focus(panel);
        let weak = panel.as_weak();
        slint::Timer::single_shot(Duration::ZERO, move || {
            if let Some(panel) = weak.upgrade() {
                ensure_focus(&panel);
            }
        });
        if !panel.window().is_visible() || panel.get_page() > 1 {
            notch.borrow_mut().release_panel_holds();
        }
    }
    fn live(&mut self, panel: &ParityPanel, host: &NativeHost) {
        let data = panel.global::<PanelData>();
        with(host, |state| {
            panel.set_dnd(crate::host::dnd_active(state));
            data.set_pipe_error(state.pipe_error.clone().unwrap_or_default().into());
            data.set_claude_hooks(*state.hooks.get("claude").unwrap_or(&false));
            data.set_codex_hooks(*state.hooks.get("codex").unwrap_or(&false));
            data.set_desktime_tab(state.settings.desktime_tab);
            let mut sessions: Vec<_> = state
                .sessions
                .values()
                .filter(|s| state.settings.wants(&s.provider))
                .collect();
            sessions.sort_by_key(|s| std::cmp::Reverse(s.last_event_ms));
            panel.set_session_count(sessions.len() as i32);
            crate::notch::models::sync(
                &self.sessions,
                sessions
                    .into_iter()
                    .filter(|s| {
                        panel.get_provider_filter() == 0
                            || s.provider
                                == if panel.get_provider_filter() == 1 {
                                    "claude"
                                } else {
                                    "codex"
                                }
                    })
                    .map(|s| NotchSession {
                        key: s.key.as_str().into(),
                        provider: s.provider.as_str().into(),
                        name: if s.name.is_empty() {
                            &s.folder
                        } else {
                            &s.name
                        }
                        .as_str()
                        .into(),
                        context: format!(
                            "{}{}{}",
                            s.folder,
                            s.context_tokens
                                .filter(|v| *v > 0)
                                .map(|v| format!(" · {}k context", v / 1000))
                                .unwrap_or_default(),
                            if s.subagents > 0 {
                                format!(" · {} agents", s.subagents)
                            } else {
                                String::new()
                            }
                        )
                        .into(),
                        state: format!(
                            "{}{}",
                            if s.attention {
                                "Needs you"
                            } else {
                                match s.state.as_str() {
                                    "working" => "Working",
                                    "waiting" => "Waiting",
                                    "done" => "Done",
                                    _ => "Idle",
                                }
                            },
                            if s.compact_queued {
                                " · Compact queued"
                            } else {
                                ""
                            }
                        )
                        .into(),
                        ago: age(now_ms().saturating_sub(s.last_event_ms)).into(),
                        working: s.state == "working",
                        attention: s.attention,
                        queued: s.compact_queued,
                        error: s.action_error.clone().unwrap_or_default().into(),
                    })
                    .collect(),
            );
            crate::notch::models::sync(
                &self.providers,
                ["claude", "codex"]
                    .into_iter()
                    .filter(|p| state.settings.wants(p))
                    .map(|p| provider(state, p))
                    .collect(),
            );
            let oldest = state
                .usage
                .values()
                .filter(|u| state.settings.wants(&u.provider) && !u.windows.is_empty())
                .filter_map(|u| u.fetched_ms)
                .min();
            data.set_usage_age(
                oldest
                    .map(|at| usage_age(at, now_ms()))
                    .unwrap_or_default()
                    .into(),
            );
            if let Some((start, stamp)) = self.refresh {
                let newest = state
                    .usage
                    .values()
                    .filter_map(|u| u.fetched_ms)
                    .max()
                    .unwrap_or(0);
                if newest > stamp || start.elapsed() >= Duration::from_secs(15) {
                    panel.global::<NotchData>().set_refresh_error(
                        if newest > stamp {
                            ""
                        } else {
                            "Could not update usage. Try Refresh again."
                        }
                        .into(),
                    );
                    self.refresh = None;
                }
            }
            panel
                .global::<NotchData>()
                .set_refreshing(self.refresh.is_some());
            let mut alerts: Vec<PanelAlert> = Vec::new();
            for a in &state.alerts {
                let text = format!("{} · {}", a.title, a.body);
                if let Some(last) = alerts.last_mut().filter(|last| {
                    !last.queued && !a.queued && last.text == text && last.provider == a.provider
                }) {
                    last.count += 1;
                    continue;
                }
                alerts.push(PanelAlert {
                    time: chrono::DateTime::from_timestamp_millis(a.ts as i64)
                        .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
                        .unwrap_or_default()
                        .into(),
                    provider: a.provider.as_str().into(),
                    text: text.into(),
                    queued: a.queued,
                    count: 1,
                });
            }
            crate::notch::models::sync(&self.alerts, alerts);
        });
    }
    pub fn start_refresh(&mut self, host: &NativeHost) {
        self.refresh = Some((
            Instant::now(),
            with(host, |s| {
                s.usage
                    .values()
                    .filter_map(|u| u.fetched_ms)
                    .max()
                    .unwrap_or(0)
            }),
        ));
        nubi_core::poller::force(host);
    }
    pub fn set_monitors(&mut self, panel: &ParityPanel, monitors: Vec<String>) {
        if !monitors.is_empty() {
            self.monitors = monitors;
            self.sync_fields(panel);
        }
    }
    fn save(&mut self, services: &crate::panel_services::Services) -> Result<(), String> {
        if let Some(error) = self.errors.values().next() {
            return Err(error.clone());
        }
        // Merge, registration and publication share the host lock. A concurrent
        // tray/drag update cannot land between the merge and the final assignment.
        let next = with(&services.host, |state| {
            let previous = state.settings.clone();
            let next = self.draft.merged(&previous)?;
            transaction(
                &previous,
                &next,
                |s| (services.register)(s),
                nubi_core::settings::apply_auto_start,
                |s| {
                    nubi_core::settings::save(s)?;
                    state.settings = s.clone();
                    Ok(())
                },
            )?;
            Ok::<_, String>(next)
        })?;
        nubi_core::poller::kick(&services.host);
        nubi_core::host::Host::emit_state(&services.host);
        self.draft = Draft::new(&next);
        Ok(())
    }
    pub fn sync_report(&mut self) {
        let rows = crate::panel_services::report_rows(&self.report);
        for row in &rows {
            if row.kind != "select" {
                continue;
            }
            let value = if row.kind == "select" {
                row.options
                    .row_data(row.selected as usize)
                    .unwrap_or_default()
            } else {
                row.value.clone()
            };
            self.report.insert(
                row.key.trim_start_matches("report:").into(),
                json!(value.as_str()),
            );
        }
        sync_fields(&self.report_rows, rows);
    }
}
fn sync_fields(target: &VecModel<PanelField>, rows: Vec<PanelField>) {
    for (index, mut row) in rows.into_iter().enumerate() {
        if let Some(previous) = target.row_data(index).filter(|old| old.key == row.key) {
            if previous.hours.iter().eq(row.hours.iter()) {
                row.hours = previous.hours.clone();
            }
            if previous.minutes.iter().eq(row.minutes.iter()) {
                row.minutes = previous.minutes.clone();
            }
            if previous.options.iter().eq(row.options.iter()) {
                row.options = previous.options.clone();
            }
            if let Some(choices) = previous
                .choices
                .as_any()
                .downcast_ref::<VecModel<crate::FieldChoice>>()
            {
                crate::notch::models::sync(choices, row.choices.iter().collect());
                row.choices = previous.choices.clone();
            }
            if previous == row {
                continue;
            }
            target.set_row_data(index, row);
        } else if index < target.row_count() {
            target.set_row_data(index, row);
        } else {
            target.push(row);
        }
    }
}
pub(crate) fn transaction(
    previous: &Settings,
    next: &Settings,
    mut shortcuts: impl FnMut(&Settings) -> Result<(), String>,
    mut startup: impl FnMut(bool) -> Result<(), String>,
    mut persist: impl FnMut(&Settings) -> Result<(), String>,
) -> Result<(), String> {
    let keys_changed = crate::tray::parse_shortcuts(previous).ok() != Some(crate::tray::parse_shortcuts(next)?);
    if keys_changed {
        shortcuts(next)?;
    }
    let result = (if next.auto_start != previous.auto_start {
        startup(next.auto_start)
    } else {
        Ok(())
    })
    .and_then(|_| persist(next));
    if let Err(error) = result {
        let restore_shortcuts = if keys_changed { shortcuts(previous) } else { Ok(()) };
        let restore_start = if next.auto_start != previous.auto_start {
            startup(previous.auto_start)
        } else {
            Ok(())
        };
        return Err(format!(
            "{error}{}{}",
            restore_shortcuts
                .err()
                .map(|e| format!("; shortcut rollback failed: {e}"))
                .unwrap_or_default(),
            restore_start
                .err()
                .map(|e| format!("; startup rollback failed: {e}"))
                .unwrap_or_default()
        ));
    }
    Ok(())
}
pub fn age(ms: u64) -> String {
    let seconds = (ms as f64 / 1000.).round();
    if seconds < 60. {
        format!("{seconds:.0}s")
    } else if seconds < 3600. {
        format!("{:.0}m", (seconds / 60.).round())
    } else {
        format!("{:.0}h", (seconds / 3600.).round())
    }
}
fn usage_age(stamp: u64, now: u64) -> String {
    let minutes = now.saturating_sub(stamp) / 60000;
    if minutes == 0 {
        "Updated just now".into()
    } else {
        format!("Updated {minutes} min ago")
    }
}
fn provider(state: &nubi_core::state::Inner, name: &str) -> NotchProvider {
    let mut row = crate::notch::models::provider_row(state, name, now_ms());
    let usage = state.usage.get(name);
    row.status = String::new().into();
    if let Some(usage) = usage {
        row.windows = settings::model(
            usage
                .windows
                .iter()
                .map(|w| crate::UsageWindow {
                    percent: w.used_percent.clamp(0., 100.) as f32,
                    label: match w.label.as_str() {
                        "5h" => "Last 5 hours",
                        "7d" => "This week",
                        "7d Opus" => "This week, Opus",
                        "7d Sonnet" => "This week, Sonnet",
                        s => s,
                    }
                    .into(),
                    reset: reset(w.resets_at).into(),
                })
                .collect(),
        );
        // Original panel only warns for missing or stale data; cached successful
        // windows keep their last update time even if a subsequent read fails.
        row.message = String::new().into();
        if usage.windows.is_empty() {
            row.status = match usage.status.as_str() {
                "loading" => "Loading usage...",
                "needs_auth" => "Sign in again",
                "no_auth" => "Sign in to see usage",
                "backoff" => "Waiting to try again",
                "error" => "Could not update usage",
                _ => "Usage unavailable",
            }
            .into();
        } else if usage
            .fetched_ms
            .is_none_or(|stamp| now_ms().saturating_sub(stamp) > 15 * 60000)
        {
            row.status = usage
                .fetched_ms
                .map(|stamp| usage_age(stamp, now_ms()))
                .unwrap_or_else(|| "Not updated yet".into())
                .into();
        }
    } else {
        row.status = "Loading usage...".into();
    }

    row
}
fn reset(at: Option<i64>) -> String {
    let Some(at) = at.filter(|v| *v != 0) else {
        return String::new();
    };
    let minutes = ((at - chrono::Local::now().timestamp()) as f64 / 60.).round() as i64;
    if minutes <= 0 {
        "resetting".into()
    } else if minutes < 60 {
        format!("resets in {minutes}m")
    } else if minutes < 1440 {
        format!("resets in {}h {}m", minutes / 60, minutes % 60)
    } else {
        format!("resets in {}d", (minutes as f64 / 1440.).round())
    }
}
fn capture(text: &str, control: bool, alt: bool, shift: bool, meta: bool) -> Option<String> {
    use slint::platform::Key;
    let is = |key| slint::SharedString::from(key) == text;
    if is(Key::Tab) || is(Key::Backtab) {
        return None;
    }
    if is(Key::Backspace) || is(Key::Delete) {
        return Some(String::new());
    }
    if !(control || alt || shift || meta) {
        return None;
    }
    let special = [
        (Key::Escape, "Escape"),
        (Key::UpArrow, "ArrowUp"),
        (Key::DownArrow, "ArrowDown"),
        (Key::LeftArrow, "ArrowLeft"),
        (Key::RightArrow, "ArrowRight"),
        (Key::Return, "Enter"),
        (Key::Home, "Home"),
        (Key::End, "End"),
        (Key::PageUp, "PageUp"),
        (Key::PageDown, "PageDown"),
        (Key::Insert, "Insert"),
        (Key::F1, "F1"),
        (Key::F2, "F2"),
        (Key::F3, "F3"),
        (Key::F4, "F4"),
        (Key::F5, "F5"),
        (Key::F6, "F6"),
        (Key::F7, "F7"),
        (Key::F8, "F8"),
        (Key::F9, "F9"),
        (Key::F10, "F10"),
        (Key::F11, "F11"),
        (Key::F12, "F12"),
        (Key::F13, "F13"),
        (Key::F14, "F14"),
        (Key::F15, "F15"),
        (Key::F16, "F16"),
        (Key::F17, "F17"),
        (Key::F18, "F18"),
    ];
    let key = if text == " " {
        "Space".to_string()
    } else {
        special
            .iter()
            .find(|(key, _)| is(*key))
            .map(|(_, label)| label.to_string())
            .or_else(|| {
                // Slint can deliver the shifted character. Use the Windows
                // virtual-key name expected by the shortcut registrar.
                let punctuation = match text {
                    "`" | "~" => "Backquote",
                    "\\" | "|" => "Backslash",
                    "[" | "{" => "BracketLeft",
                    "]" | "}" => "BracketRight",
                    "," | "<" => "Comma",
                    "=" | "+" => "Equal",
                    "-" | "_" => "Minus",
                    "." | ">" => "Period",
                    "'" | "\"" => "Quote",
                    ";" | ":" => "Semicolon",
                    "/" | "?" => "Slash",
                    _ if text.len() == 1 && text.as_bytes()[0].is_ascii_alphanumeric() => {
                        return Some(text.to_ascii_uppercase());
                    }
                    _ => return None,
                };
                Some(punctuation.into())
            })?
    };
    let mut parts = Vec::new();
    if control {
        parts.push("Ctrl".into());
    }
    if alt {
        parts.push("Alt".into());
    }
    if shift {
        parts.push("Shift".into());
    }
    if meta {
        parts.push("Super".into());
    }
    parts.push(key);
    let shortcut = parts.join("+");
    shortcut
        .parse::<global_hotkey::hotkey::HotKey>()
        .ok()
        .map(|_| shortcut)
}

fn bridge(
    panel: &ParityPanel,
    owner: Rc<RefCell<Panel>>,
    services: &crate::panel_services::Services,
) {
    let data = panel.global::<NotchData>();
    macro_rules! forward {($name:ident,$invoke:ident,($($arg:ident),*))=>{{let weak=services.notch_ui.clone();let panel_weak=panel.as_weak();let state=owner.clone();let services=services.clone();data.$name(move |$($arg),*| {if let Some(ui)=weak.upgrade(){ui.global::<NotchData>().$invoke($($arg),*);if let Some(panel)=panel_weak.upgrade(){panel.global::<NotchData>().set_request_error(ui.global::<NotchData>().get_request_error());state.borrow_mut().tick(&panel,&services.host,&services.notch);}}});}};}
    forward!(on_answer, invoke_answer, (id, decision));
    forward!(on_choose, invoke_choose, (id, index));
    forward!(on_custom, invoke_custom, (id, text));
    forward!(on_submit_custom, invoke_submit_custom, (id, text));
    forward!(on_submit, invoke_submit, (id));
    forward!(on_terminal, invoke_terminal, (key));
    forward!(on_dismiss, invoke_dismiss, (id));
    let notch = services.notch.clone();
    data.on_hold(move |id, held| {
        if let Ok(id) = id.parse() {
            notch.borrow_mut().panel_hold(id, held);
        }
    });
}

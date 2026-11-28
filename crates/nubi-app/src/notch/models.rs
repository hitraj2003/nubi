use super::*;
use crate::{DeskRow, RequestOption, UsageWindow};
use nubi_core::state::Pending;
use slint::ModelRc;
pub fn model<T: Clone + 'static>(rows: Vec<T>) -> ModelRc<T> {
    Rc::new(VecModel::from(rows)).into()
}
pub fn sync<T: Clone + PartialEq + 'static>(model: &VecModel<T>, rows: Vec<T>) {
    for (index, row) in rows.iter().enumerate() {
        if index >= model.row_count() {
            model.push(row.clone());
        } else if model.row_data(index).as_ref() != Some(row) {
            model.set_row_data(index, row.clone());
        }
    }
    while model.row_count() > rows.len() {
        model.remove(model.row_count() - 1);
    }
}
fn title(name: &str, folder: &str) -> String {
    if name.is_empty() { folder } else { name }.into()
}
pub fn clock(ms: u64) -> String {
    let seconds = ms.div_ceil(1000);
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }
}
fn reset(at: Option<i64>, now: u64) -> String {
    let Some(at) = at.filter(|at| *at != 0) else {
        return String::new();
    };
    let minutes = ((at as f64 * 1000. - now as f64) / 60000.).round() as i64;
    if minutes <= 0 {
        "resetting".into()
    } else if minutes < 60 {
        format!("resets in {minutes}m")
    } else if minutes < 1440 {
        format!("resets in {}h {}m", minutes / 60, minutes % 60)
    } else {
        format!("resets in {:.0}d", (minutes as f64 / 1440.).round())
    }
}
fn summary(input: Option<&serde_json::Value>) -> String {
    let Some(object) = input.and_then(serde_json::Value::as_object) else {
        return String::new();
    };
    ["command", "file_path", "path", "url", "pattern"]
        .into_iter()
        .filter_map(|key| object.get(key).and_then(serde_json::Value::as_str))
        .find(|text| !text.is_empty())
        .or_else(|| object.values().find_map(serde_json::Value::as_str))
        .unwrap_or("")
        .trim()
        .into()
}
#[derive(Default)]
pub struct Draft {
    pub step: usize,
    pub answers: BTreeMap<String, Vec<String>>,
    pub custom: String,
    pub total_ms: u64,
    pub held_left_ms: Option<u64>,
}
impl Draft {
    pub fn selected(&self, pending: &Pending) -> Vec<String> {
        let question = self.question(pending);
        self.answers
            .get(question["question"].as_str().unwrap_or(""))
            .cloned()
            .unwrap_or_default()
    }
    pub fn question<'a>(&self, pending: &'a Pending) -> &'a serde_json::Value {
        pending
            .tool_input
            .as_ref()
            .map(|input| &input["questions"][self.step])
            .unwrap_or(&serde_json::Value::Null)
    }
}
impl Notch {
    pub fn update(&mut self, ui: &NotchWindow) {
        let now = now_ms();
        let data = ui.global::<NotchData>();
        let host = self.host.clone();
        with(&host, |state| {
            let settings = &state.settings;
            ui.set_edge(match settings.edge.as_str() {
                "top" => 0,
                "bottom" => 1,
                "left" => 2,
                _ => 3,
            });
            crate::ui::apply_theme(ui.global::<Theme>(), &settings.notch_theme);
            ui.global::<Theme>()
                .set_notch_accent(settings.notch_accent.as_str().into());
            ui.global::<Theme>()
                .set_reduced(settings.reduce_motion || crate::native::system_reduced_motion());
            let mut sessions: Vec<_> = state
                .sessions
                .values()
                .filter(|session| settings.wants(&session.provider))
                .collect();
            data.set_session_count(sessions.len() as i32);
            sessions.sort_by_key(|session| std::cmp::Reverse(session.last_event_ms));
            sync(
                &self.sessions,
                sessions
                    .into_iter()
                    .take(8)
                    .map(|session| {
                        let seconds =
                            (now.saturating_sub(session.last_event_ms) as f64 / 1000.).round();
                        let ago = if seconds < 60. {
                            format!("{seconds:.0}s")
                        } else if seconds < 3600. {
                            format!("{:.0}m", (seconds / 60.).round())
                        } else {
                            format!("{:.0}h", (seconds / 3600.).round())
                        };
                        let mut label = if session.state == "working" {
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
                        if session.subagents != 0 {
                            label += &format!(" · {} agents", session.subagents);
                        }
                        if session.compact_queued {
                            label += " · Compact queued";
                        }
                        NotchSession {
                            key: session.key.as_str().into(),
                            name: title(&session.name, &session.folder).into(),
                            provider: session.provider.as_str().into(),
                            state: label.into(),
                            context: session
                                .context_tokens
                                .filter(|tokens| *tokens > 0)
                                .map(|tokens| format!("{:.0}k", (tokens as f64 / 1000.).round()))
                                .unwrap_or_default()
                                .into(),
                            ago: ago.into(),
                            working: session.state == "working",
                            attention: session.attention,
                            queued: session.compact_queued,
                            error: session.action_error.as_deref().unwrap_or("").into(),
                        }
                    })
                    .collect(),
            );
            sync(
                &self.providers,
                ["claude", "codex"]
                    .into_iter()
                    .filter(|provider| settings.wants(provider))
                    .map(|provider| provider_row(state, provider, now))
                    .collect(),
            );
            data.set_dnd(crate::host::dnd_active(state));
            // Removing a filtered card must release its hold, like requestClock.release().
            for pending in state
                .pending
                .values_mut()
                .filter(|pending| !settings.wants(&pending.provider))
            {
                pending.held = false;
            }
            let mut pending: Vec<_> = state
                .pending
                .values()
                .filter(|pending| settings.wants(&pending.provider))
                .collect();
            pending.sort_by_key(|pending| (pending.created_ms, pending.id));
            let missed: Vec<_> = state
                .missed
                .iter()
                .filter(|missed| settings.wants(&missed.provider))
                .collect();
            let ids: BTreeSet<_> = pending
                .iter()
                .map(|item| item.id)
                .chain(missed.iter().map(|item| item.id))
                .collect();
            if ids.iter().any(|id| !self.seen.contains(id)) && !crate::host::dnd_active(state) {
                ui.set_tab(0);
                self.popup = Some(Instant::now());
            }
            self.seen = ids;
            self.drafts.retain(|id, _| state.pending.contains_key(id));
            if data
                .get_focused_request()
                .parse::<u64>()
                .ok()
                .is_some_and(|id| {
                    !state
                        .pending
                        .get(&id)
                        .is_some_and(|pending| settings.wants(&pending.provider))
                })
            {
                data.set_focused_request("".into());
                data.set_input_focused(false);
            }
            if pending.is_empty() && missed.is_empty() {
                self.popup = None;
            }
            data.set_pending_count(pending.len() as i32);
            data.set_missed_count(missed.len() as i32);
            data.set_compact_text(
                pending
                    .first()
                    .map(|pending| {
                        if pending.kind == "question" {
                            "Question"
                        } else {
                            "Needs your OK"
                        }
                    })
                    .unwrap_or("")
                    .into(),
            );
            let mut y = 0.;
            let mut rows = Vec::new();
            for pending in pending {
                let draft = self.drafts.entry(pending.id).or_default();
                if draft.total_ms == 0 {
                    draft.total_ms = pending.expires_ms.saturating_sub(pending.created_ms).max(1);
                }
                if pending.held {
                    draft
                        .held_left_ms
                        .get_or_insert_with(|| pending.expires_ms.saturating_sub(now));
                } else {
                    draft.held_left_ms = None;
                }
                let mut row = request_row(pending, draft, settings.questions_mode != "full", now);
                row.side = ui.get_edge() >= 2;
                if row.side && row.slim {
                    row.height = 76.;
                }
                row.y = y;
                y += row.height + 8.;
                rows.push(row);
            }
            for missed in missed {
                rows.push(NotchRequest {
                    id: missed.id.to_string().into(),
                    provider: missed.provider.as_str().into(),
                    session_key: missed.session_key.as_str().into(),
                    name: title(&missed.name, &missed.folder).into(),
                    title: if missed.kind == "question" {
                        "Question waiting in terminal"
                    } else {
                        "Permission waiting in terminal"
                    }
                    .into(),
                    missed: true,
                    side: ui.get_edge() >= 2,
                    y,
                    height: if ui.get_edge() >= 2 { 76. } else { 44. },
                    ..Default::default()
                });
                y += if ui.get_edge() >= 2 { 84. } else { 52. };
            }
            data.set_requests_height((y - 8.).max(42.));
            if !rows.iter().any(|row| {
                row.summary == data.get_detail_tip() || row.title == data.get_detail_tip()
            }) {
                data.set_detail_tip("".into());
            }
            sync_requests(&self.requests, rows);
            if let Some((start, stamp)) = self.refresh {
                if Self::newest(state) > stamp || start.elapsed() >= Duration::from_secs(15) {
                    if Self::newest(state) <= stamp {
                        self.refresh_error = "Could not update usage".into();
                    } else {
                        self.refresh_error.clear();
                    }
                    self.refresh = None;
                }
            }
            data.set_refreshing(self.refresh.is_some());
            if !ui.get_menu_key().is_empty()
                && !state.sessions.contains_key(ui.get_menu_key().as_str())
            {
                ui.set_menu_key("".into());
            }
        });
        if !ui.get_desk_available() {
            data.set_desk_visible(false);
            data.set_desk_loading(false);
            data.set_glance("".into());
            if ui.get_tab() == 3 {
                ui.set_tab(1);
            }
        }
        data.set_refresh_error(
            if ui.get_tab() == 2 {
                self.refresh_error.as_str()
            } else {
                ""
            }
            .into(),
        );
        if !ui.get_desk_available() {
            return;
        }
        let settings = with(&host, |state| state.settings.clone());
        let cache = host
            .desktime
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let snapshot = cache.snapshot(&settings, chrono::Local::now());
        data.set_desk_visible(snapshot.show);
        data.set_desk_loading(snapshot.loading);
        data.set_desk_retry(
            snapshot.error.is_some()
                && !snapshot.loading
                && snapshot.enabled
                && !snapshot.logged_out,
        );
        data.set_desk_step(snapshot.step.into());
        data.set_glance(snapshot.glance.into());
        if ui.get_tab() == 3 && !snapshot.show {
            ui.set_tab(1);
        }
        data.set_desk_title(
            if !snapshot.enabled {
                "DeskTime is inactive"
            } else if snapshot.logged_out {
                "Signed out of DeskTime"
            } else if snapshot.error.is_some() && !snapshot.loading {
                "Couldn't read DeskTime"
            } else if snapshot.loading {
                match snapshot.step {
                    1 => "Reading today…",
                    2 => "Reading this week…",
                    3 => "Reading this month…",
                    _ => "Opening DeskTime…",
                }
            } else {
                "No DeskTime data yet"
            }
            .into(),
        );
        data.set_desk_message(snapshot.error.as_deref().unwrap_or("Log in with your DeskTime email, or install the Nubi extension, log in to DeskTime in your browser and click Allow access.").into());
        let mut y = 0.;
        let side = ui.get_edge() >= 2;
        let period_count = snapshot
            .sections
            .iter()
            .filter(|section| section.view != "day")
            .count();
        let mut sections: Vec<_> = snapshot
            .sections
            .into_iter()
            .filter(|_| snapshot.enabled && !snapshot.logged_out)
            .map(|section| {
                let picked = match section.view.as_str() {
                    "day" => &settings.dt_rows_day,
                    "week" => &settings.dt_rows_week,
                    _ => &settings.dt_rows_month,
                };
                let rows: Vec<_> = section
                    .rows
                    .into_iter()
                    .filter(|row| {
                        picked.contains(&row.key) && !(row.key == "late" && row.value == "None")
                    })
                    .map(|row| DeskRow {
                        label: row.label.into(),
                        value: row.value.into(),
                        note: if settings.dt_notes {
                            row.note.into()
                        } else {
                            "".into()
                        },
                        tone: row.tone.into(),
                    })
                    .collect();
                let row_columns = if section.view == "day" {
                    if side { 2 } else { rows.len().max(1) }
                } else if !side && period_count == 1 {
                    2
                } else {
                    1
                };
                let row_count = rows.len().div_ceil(row_columns);
                let row_height = if section.view == "day" {
                    if settings.dt_notes { 48. } else { 32. }
                } else if settings.dt_notes {
                    36.
                } else {
                    24.
                };
                let height = if section.view == "day" {
                    94. + row_count as f32 * if settings.dt_notes { 48. } else { 32. }
                } else {
                    112. + row_count as f32 * if settings.dt_notes { 36. } else { 24. }
                };
                DeskSection {
                    view: section.view.into(),
                    title: section.title.into(),
                    big: section.big.into(),
                    caption: section.caption.into(),
                    note: section.note.into(),
                    over: section.over,
                    percent: section.percent as f32,
                    pace: section.pace.map(|pace| pace as f32).unwrap_or(-1.),
                    rows: model(rows),
                    y: 0.,
                    height,
                    column: 0,
                    columns: 1,
                    row_columns: row_columns as i32,
                    row_height,
                }
            })
            .collect();
        let mut period_index = 0;
        let day_height = sections
            .iter()
            .find(|section| section.view == "day")
            .map(|section| section.height + 8.)
            .unwrap_or(0.);
        let period_height = sections
            .iter()
            .filter(|section| section.view != "day")
            .map(|section| section.height)
            .fold(0., f32::max);
        for section in &mut sections {
            if section.view == "day" {
                section.y = 0.;
                y = section.height + 8.;
            } else if side {
                section.y = y;
                y += section.height + 8.;
            } else {
                section.y = day_height;
                section.column = period_index;
                section.columns = period_count as i32;
                period_index += 1;
                y = day_height + period_height + 8.;
            }
        }
        sync(&self.desk, sections);
        data.set_desk_height((y - 8.).max(190.));
        if ui.get_tab() == 3 && snapshot.error.is_some() && self.desk.row_count() > 0 {
            data.set_refresh_error(
                format!(
                    "Couldn't refresh. Showing data from {}.",
                    chrono::DateTime::from_timestamp_millis(snapshot.updated_ms)
                        .map(|time| time
                            .with_timezone(&chrono::Local)
                            .format("%H:%M")
                            .to_string())
                        .unwrap_or_default()
                )
                .into(),
            );
        }
    }
    pub fn view(&mut self, ui: &NotchWindow) -> (i32, [f64; 4]) {
        let now = Instant::now();
        let host = self.host.clone();
        self.delayed.retain(|(alert, since)| {
            if since.elapsed() < Duration::from_millis(400) {
                return true;
            }
            if !with(&host, |state| {
                state
                    .pending
                    .values()
                    .any(|pending| alert.session_key.as_ref() == Some(&pending.session_key))
            }) {
                self.alerts.push_back(alert.clone());
            }
            false
        });
        if self
            .alert
            .as_ref()
            .is_some_and(|(_, since)| since.elapsed() >= Duration::from_millis(4200))
        {
            self.alert = None;
            self.alert_gap = Some(now);
        }
        if self.alert.is_none()
            && self
                .alert_gap
                .is_none_or(|since| since.elapsed() >= Duration::from_millis(750))
        {
            if let Some(alert) = self.alerts.pop_front() {
                ui.global::<NotchData>().set_alert_title(
                    format!(
                        "{} · {}",
                        if alert.provider == "codex" {
                            "Codex"
                        } else {
                            "Claude"
                        },
                        alert.title
                    )
                    .into(),
                );
                ui.global::<NotchData>()
                    .set_alert_body(crate::alert_text::notch_body(&alert.body).into());
                ui.global::<NotchData>()
                    .set_alert_kind(alert.kind.as_str().into());
                self.alert = Some((alert, now));
            }
        }
        let settings = with(&self.host, |state| state.settings.clone());
        ui.set_inline_notifications(settings.notifications_inside_notch);
        ui.set_alert_active(self.alert.is_some());
        let data = ui.global::<NotchData>();
        let waiting = data.get_pending_count() + data.get_missed_count() > 0;
        if self.popup.is_some_and(|since| {
            settings.request_popup_secs > 0
                && since.elapsed() >= Duration::from_secs(settings.request_popup_secs.into())
        }) && !self.hovering
        {
            self.popup = None;
        }
        let mode = if self.dragging {
            1
        } else if (self.hovering && (self.clicked || !settings.notch_on_hover))
            || self.popup.is_some()
            || (!data.get_update().id.is_empty() && ui.get_tab() == 0)
            || !ui.get_menu_key().is_empty()
            || (!data.get_focused_request().is_empty() && ui.get_opened())
        {
            3
        } else if self.alert.is_some() && (settings.notifications_inside_notch || ui.get_edge() == 0) {
            2
        } else if settings.notch_on_hover
            && !waiting
            && !self.hovering
            && self.fold_since.elapsed() >= Duration::from_millis(450)
        {
            0
        } else {
            1
        };
        let side = ui.get_edge() >= 2;
        let session_height = self
            .sessions
            .iter()
            .map(|session| if session.error.is_empty() { 40. } else { 72. })
            .sum::<f64>()
            .max(42.);
        let pane = match ui.get_tab() {
            0 => ui.get_requests_content_height() as f64,
            1 => session_height,
            2 => {
                if side {
                    self.providers.row_count() as f64 * 120. - 8.
                } else {
                    112.
                }
            }
            _ => data.get_desk_height() as f64,
        };
        let max = if ui.get_tab() == 3 {
            if side { 780. } else { 600. }
        } else if side {
            620.
        } else {
            420.
        };
        let available =
            f64::from(ui.window().size().height) / f64::from(ui.window().scale_factor());
        let banner_space = if mode == 3 {
            f64::from(ui.get_banner_space())
        } else {
            0.
        };
        let open_limit = (available - 48.)
            .max(100.)
            .min(if side { 812. } else { 676. });
        let open_height = (68. + banner_space + pane.min(max)).min(open_limit);
        let open_height = if ui.get_menu_key().is_empty() {
            open_height
        } else {
            open_height.max(240.).min(open_limit)
        };
        ui.set_open_height(open_height as f32);
        // Match DOM offsetWidth: measure the laid-out content, then round logical pixels.
        let compact_width = f64::from(ui.get_compact_width());
        let target = match mode {
            0 => [56., 6., 3., 4.],
            1 => {
                if side {
                    [
                        self.providers.row_count() as f64 * 54.
                            + 14.
                            + if waiting { 26. } else { 0. },
                        46.,
                        18.,
                        8.,
                    ]
                } else {
                    [compact_width, 44., 16., 8.]
                }
            }
            2 if settings.notifications_inside_notch => {
                let width = f64::from(ui.get_notice_width());
                let height = f64::from(ui.get_notice_height());
                let (along, depth) = if side { (height, width) } else { (width, height) };
                [along, depth, 18., 12.]
            }
            2 => [360., 56., 18., 12.],
            _ => {
                if side {
                    [open_height, f64::from(ui.get_open_width()), 32., 20.]
                } else {
                    [
                        f64::from(ui.get_open_width()),
                        open_height,
                        32.,
                        20.,
                    ]
                }
            }
        };
        (mode, target)
    }
}
pub(crate) fn provider_row(state: &Inner, provider: &str, now: u64) -> NotchProvider {
    let usage = state.usage.get(provider);
    let session = usage.and_then(|usage| {
        usage
            .windows
            .iter()
            .find(|window| window.label == "5h")
            .or(usage.windows.first())
    });
    let week = usage.and_then(|usage| usage.windows.iter().find(|window| window.label == "7d"));
    let activity = if state
        .pending
        .values()
        .any(|pending| pending.provider == provider)
        || state
            .sessions
            .values()
            .any(|session| session.provider == provider && session.attention)
    {
        "attention"
    } else if state
        .sessions
        .values()
        .any(|session| session.provider == provider && session.state == "working")
    {
        "working"
    } else {
        ""
    };
    let status = usage
        .map(|usage| usage.status.as_str())
        .unwrap_or("loading");
    let stale = usage.is_none_or(|usage| {
        usage.windows.is_empty()
            || usage
                .fetched_ms
                .is_none_or(|stamp| now.saturating_sub(stamp) > 15 * 60000)
    });
    let note = if stale && session.is_some() {
        usage
            .and_then(|usage| usage.fetched_ms)
            .map(|stamp| {
                let minutes = now.saturating_sub(stamp) / 60000;
                if minutes < 1 {
                    "Updated just now".into()
                } else {
                    format!("Updated {minutes} min ago")
                }
            })
            .unwrap_or_else(|| "Not updated yet".into())
    } else {
        String::new()
    };
    let windows = usage
        .map(|usage| {
            usage
                .windows
                .iter()
                .take(2)
                .map(|window| UsageWindow {
                    percent: window.used_percent.clamp(0., 100.) as f32,
                    label: match window.label.as_str() {
                        "5h" => "Session",
                        "7d" => "Week",
                        "7d Opus" => "Week, Opus",
                        "7d Sonnet" => "Week, Sonnet",
                        label => label,
                    }
                    .into(),
                    reset: reset(window.resets_at, now).into(),
                })
                .collect()
        })
        .unwrap_or_default();
    NotchProvider {
        provider: provider.into(),
        name: if provider == "claude" {
            "Claude"
        } else {
            "Codex"
        }
        .into(),
        text: session
            .map(|window| format!("{:.0}%", window.used_percent.round()))
            .unwrap_or_else(|| if status == "loading" { "…" } else { "—" }.into())
            .into(),
        session: session
            .map(|window| window.used_percent as f32)
            .unwrap_or(0.),
        week: week.map(|window| window.used_percent as f32).unwrap_or(0.),
        has_week: week.is_some()
            && week
                .zip(session)
                .is_some_and(|(week, session)| !std::ptr::eq(week, session)),
        stale: status != "ok" && session.is_some(),
        activity: activity.into(),
        status: note.into(),
        message: usage
            .map(|usage| {
                if usage.status == "needs_auth" {
                    "Sign in again: the saved Claude token has expired"
                } else {
                    usage.message.as_deref().unwrap_or(&usage.status)
                }
            })
            .unwrap_or("No usage data yet")
            .into(),
        windows: model(windows),
    }
}
pub fn request_row(pending: &Pending, draft: &Draft, system: bool, now: u64) -> NotchRequest {
    let question = draft.question(pending);
    let selected = draft.selected(pending);
    let options: Vec<_> = question["options"]
        .as_array()
        .map(|options| {
            options
                .iter()
                .map(|option| RequestOption {
                    label: option["label"].as_str().unwrap_or("").into(),
                    description: option["description"].as_str().unwrap_or("").into(),
                    selected: selected
                        .iter()
                        .any(|label| Some(label.as_str()) == option["label"].as_str()),
                })
                .collect()
        })
        .unwrap_or_default();
    let count = pending
        .tool_input
        .as_ref()
        .and_then(|input| input["questions"].as_array())
        .map(Vec::len)
        .unwrap_or(0);
    let left = if pending.held {
        draft.held_left_ms
    } else {
        None
    }
    .unwrap_or_else(|| pending.expires_ms.saturating_sub(now));
    NotchRequest {
        id: pending.id.to_string().into(),
        provider: pending.provider.as_str().into(),
        session_key: pending.session_key.as_str().into(),
        name: title(&pending.name, &pending.folder).into(),
        title: if pending.kind == "question" {
            "Question".into()
        } else {
            format!("Allow {}?", pending.tool_name.as_deref().unwrap_or("tool"))
        }
        .into(),
        summary: {
            let summary = summary(pending.tool_input.as_ref());
            if summary.is_empty() {
                pending.tool_name.clone().unwrap_or_else(|| "tool".into())
            } else {
                summary
            }
        }
        .into(),
        question: pending.kind == "question",
        missed: false,
        slim: system && pending.kind != "question",
        held: pending.held,
        full: !system,
        countdown: clock(left).into(),
        remaining: (left as f64
            / if draft.total_ms > 0 {
                draft.total_ms
            } else {
                pending.expires_ms.saturating_sub(pending.created_ms).max(1)
            } as f64)
            .min(1.) as f32,
        step: format!(
            "{}{}",
            if count > 1 {
                format!("{} of {count} ", draft.step + 1)
            } else {
                String::new()
            },
            question["header"]
                .as_str()
                .filter(|text| !text.is_empty())
                .map(|text| format!("{text} "))
                .unwrap_or_default()
        )
        .into(),
        prompt: question["question"].as_str().unwrap_or("").into(),
        custom: draft.custom.as_str().into(),
        question_index: draft.step as i32,
        multi: question["multiSelect"].as_bool().unwrap_or(false),
        can_submit: !selected.is_empty(),
        height: if pending.kind == "question" {
            204. + options.len() as f32 * 48.
        } else if system {
            44.
        } else {
            146.
        },
        options: model(options),
        ..Default::default()
    }
}
// Preserve model identity: countdown updates must not recreate active inputs.
pub(crate) fn sync_requests(target: &VecModel<NotchRequest>, rows: Vec<NotchRequest>) {
    // Remove disappeared cards instead of assigning their successor into an active input.
    for index in (0..target.row_count()).rev() {
        if target
            .row_data(index)
            .is_some_and(|old| !rows.iter().any(|row| row.id == old.id))
        {
            target.remove(index);
        }
    }
    for (index, mut row) in rows.iter().cloned().enumerate() {
        if let Some(previous) = target
            .row_data(index)
            .filter(|previous| previous.id == row.id)
        {
            if previous.question_index == row.question_index && previous.prompt == row.prompt {
                if let Some(options) = previous
                    .options
                    .as_any()
                    .downcast_ref::<VecModel<RequestOption>>()
                {
                    sync(options, row.options.iter().collect());
                    row.options = previous.options.clone();
                }
            }
            if previous == row {
                continue;
            }
            target.set_row_data(index, row);
        } else if index < target.row_count() {
            target.insert(index, row);
        } else {
            target.push(row);
        }
    }
    while target.row_count() > rows.len() {
        target.remove(target.row_count() - 1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use nubi_core::protocol::HookMsg;
    #[test]
    fn source_countdown_and_reset_boundaries() {
        assert_eq!(clock(59_001), "1:00");
        assert_eq!(clock(59_000), "59s");
        assert_eq!(clock(600_000), "10:00");
        assert_eq!(reset(Some(0), 0), "");
        assert_eq!(reset(Some(1), 1000), "resetting");
        assert_eq!(reset(Some(3660), 0), "resets in 1h 1m");
    }
    #[test]
    fn typed_requests_preserve_unicode_multiselect_and_real_deadlines() {
        let (host, _events) = NativeHost::from_settings(Default::default());
        let message = HookMsg {
            provider: "claude".into(),
            session_id: "test".into(),
            event: "PreToolUse".into(),
            tool_name: Some("AskUserQuestion".into()),
            tool_input: Some(
                serde_json::json!({"questions":[{"question":"Which?", "multiSelect":true,"options":[{"label":"日本語"},{"label":"Rust"}]}]}),
            ),
            ..Default::default()
        };
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        let id = host.add_pending(&message, tx, 60_000);
        with(&host, |state| {
            let pending = &state.pending[&id];
            let draft = Draft {
                answers: BTreeMap::from([("Which?".into(), vec!["日本語".into(), "Rust".into()])]),
                ..Default::default()
            };
            let row = request_row(pending, &draft, true, pending.created_ms);
            assert!(row.question && row.multi && row.can_submit);
            assert!(!row.slim);
            assert_eq!(row.options.row_count(), 2);
            assert!(row.options.row_data(0).unwrap().selected);
            assert_eq!(row.countdown, "1:00");
            state
                .answer(
                    id,
                    Reply {
                        decision: "answer".into(),
                        answers: Some(serde_json::Map::from_iter([(
                            "Which?".into(),
                            serde_json::json!("日本語, Rust"),
                        )])),
                        message: None,
                    },
                )
                .unwrap();
        });
        assert_eq!(
            rx.try_recv().unwrap().answers.unwrap()["Which?"],
            "日本語, Rust"
        );
        assert!(
            with(&host, |state| state.answer(
                id,
                Reply {
                    decision: "deny".into(),
                    answers: None,
                    message: None
                }
            ))
            .is_err()
        );
    }

    #[test]
    fn extended_hold_keeps_the_original_countdown_scale() {
        let (host, _events) = NativeHost::from_settings(Default::default());
        let message = HookMsg {
            provider: "claude".into(),
            session_id: "held".into(),
            event: "PermissionRequest".into(),
            ..Default::default()
        };
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let id = host.add_pending(&message, tx, 60_000);
        with(&host, |state| {
            let pending = state.pending.get_mut(&id).unwrap();
            let draft = Draft {
                total_ms: 60_000,
                ..Default::default()
            };
            let before = request_row(pending, &draft, true, pending.created_ms + 30_000);
            pending.expires_ms += 20_000;
            pending.held = true;
            let after = request_row(pending, &draft, true, pending.created_ms + 50_000);
            assert_eq!(before.countdown, "30s");
            assert_eq!(before.countdown, after.countdown);
            assert_eq!(before.remaining, after.remaining);
            assert_eq!(after.remaining, 0.5);
        });
    }
}

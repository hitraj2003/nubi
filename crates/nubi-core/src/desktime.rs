//! DeskTime calendar validation, targets and report calculations.
use crate::settings::Settings;
use chrono::{Datelike, Duration, Local, NaiveDate, Timelike};
use serde::{Deserialize, Serialize};

/// Weekly early-leave reserve; month surplus beyond it becomes late-arrival allowance.
const EARLY_LEAVE_WEEKLY_MIN: f64 = 90.0;
pub const VIEWS: [&str; 3] = ["day", "week", "month"];
/// Row keys the owner can pick per card.
pub const DAY_ROWS: [&str; 5] = ["done", "finish", "target", "late", "nonprod"];
pub const PERIOD_ROWS: [&str; 5] = ["target", "todate", "balance", "eod", "late"];

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Report {
    pub view: String,
    pub productive_min: Option<u32>,
    pub at_work_min: Option<u32>,
    pub range_start: String,
    pub range_end: String,
    pub holidays: Vec<String>,
    pub logged_out: bool,
}

#[derive(Serialize)]
pub struct Row {
    pub key: String,
    pub label: String,
    pub value: String,
    pub note: String,
    /// "pos" | "neg" | "".
    pub tone: String,
}
#[derive(Serialize)]
pub struct Section {
    pub view: String,
    pub title: String,
    /// Remaining time, or overtime with a leading +.
    pub big: String,
    pub caption: String,
    pub over: bool,
    /// Done as a share of the target, 0..=100.
    pub percent: f64,
    /// Week/month: share of the target due by end of today, 0..=100.
    pub pace: Option<f64>,
    /// Week/month: working days in range.
    pub note: String,
    pub rows: Vec<Row>,
    pub fetched_ms: i64,
}
#[derive(Serialize)]
pub struct Snapshot {
    pub show: bool,
    pub enabled: bool,
    pub logged_out: bool,
    pub loading: bool,
    pub step: u8,
    pub error: Option<String>,
    /// Oldest fetch time among the shown sections (ms).
    pub updated_ms: i64,
    pub sections: Vec<Section>,
    /// Sections the owner chose to show.
    pub views: Vec<String>,
    /// Newest successful read of any view (ms), for Settings.
    pub read_ms: i64,
    /// Last session handover from the browser extension (ms); 0 when none since start.
    pub synced_ms: i64,
    /// Views read on each refresh, in order (the loading steps).
    pub fetch: Vec<String>,
    /// Collapsed-notch text; empty when the glance is off or there is no data.
    pub glance: String,
    /// Folder to load as an unpacked extension (Settings shows it).
    pub extension_dir: String,
}

/// Reader-independent cache shared by native surfaces. The stage 7 reader can
/// replace reports without coupling calendar calculations to a window runtime.
#[derive(Default)]
pub struct Cache {
    pub reports: std::collections::BTreeMap<String, (Report, i64)>,
    pub loading: bool,
    pub step: u8,
    pub logged_out: bool,
    pub error: Option<String>,
    pub synced_ms: i64,
}

impl Cache {
    pub fn load() -> Self {
        use std::io::Read;
        let mut bytes = Vec::new();
        if let Ok(file) =
            std::fs::File::open(crate::settings::data_dir().join("desktime-cache.json"))
        {
            let _ = file.take(256 * 1024 + 1).read_to_end(&mut bytes);
        }
        let mut reports: std::collections::BTreeMap<String, (Report, i64)> =
            if bytes.len() <= 256 * 1024 {
                serde_json::from_slice(&bytes).unwrap_or_default()
            } else {
                Default::default()
            };
        let today = Local::now().date_naive();
        reports.retain(|view, (report, _)| {
            VIEWS.contains(&view.as_str())
                && report.view == *view
                && !report.logged_out
                && report.productive_min.is_some()
                && validate_report(report, today).is_ok()
        });
        Self {
            reports,
            ..Default::default()
        }
    }

    pub fn snapshot(&self, settings: &Settings, now: chrono::DateTime<Local>) -> Snapshot {
        let valid = |view: &str| {
            self.reports
                .get(view)
                .filter(|(report, _)| validate_report(report, now.date_naive()).is_ok())
        };
        let month = valid("month").map(|(report, _)| report);
        let sections: Vec<_> = VIEWS
            .iter()
            .filter(|view| settings.dt_views.iter().any(|item| item == **view))
            .filter_map(|view| valid(view))
            .map(|(report, time)| section(report, *time, settings, now, month))
            .collect();
        let glance = if settings.dt_glance
            && settings.desktime_enabled
            && settings.desktime_tab
            && !self.logged_out
        {
            valid("day")
                .map(|(report, _)| {
                    glance(
                        &section(report, 0, settings, now, month),
                        &settings.dt_glance_kind,
                    )
                })
                .unwrap_or_default()
        } else {
            String::new()
        };
        Snapshot {
            show: settings.desktime_tab,
            enabled: settings.desktime_enabled,
            logged_out: self.logged_out,
            loading: self.loading,
            step: self.step,
            error: self.error.clone(),
            updated_ms: sections
                .iter()
                .map(|section| section.fetched_ms)
                .min()
                .unwrap_or(0),
            read_ms: self
                .reports
                .values()
                .map(|(_, time)| *time)
                .max()
                .unwrap_or(0),
            synced_ms: self.synced_ms,
            sections,
            views: settings.dt_views.clone(),
            fetch: fetch_views(settings)
                .into_iter()
                .map(String::from)
                .collect(),
            glance,
            extension_dir: String::new(),
        }
    }
}

pub fn date(s: &str) -> Result<NaiveDate, String> {
    if s.len() != 10 {
        return Err("Dates must use YYYY-MM-DD".into());
    }
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| "Invalid date".into())
}
pub fn clock(s: &str) -> Result<i64, String> {
    let bytes = s.as_bytes();
    if bytes.len() != 5
        || bytes[2] != b':'
        || ![bytes[0], bytes[1], bytes[3], bytes[4]]
            .iter()
            .all(u8::is_ascii_digit)
    {
        return Err("Times must use HH:MM".into());
    }
    let h: i64 = s[..2].parse().map_err(|_| "Invalid time")?;
    let m: i64 = s[3..].parse().map_err(|_| "Invalid time")?;
    if h > 23 || m > 59 {
        return Err("Time is out of range".into());
    }
    Ok(h * 60 + m)
}
pub fn validate_settings(s: &Settings) -> Result<(), String> {
    if clock(&s.daily_target)? == 0 {
        return Err("Daily target must be greater than zero".into());
    }
    clock(&s.end_of_day)?;
    if !s.start_of_day.is_empty() {
        clock(&s.start_of_day)?;
    }
    if s.work_days.len() > 7 || s.work_days.iter().any(|d| *d > 6) || s.day_targets.len() > 7 {
        return Err("Invalid work days".into());
    }
    for (d, target) in &s.day_targets {
        if *d > 6 {
            return Err("Invalid target weekday".into());
        }
        clock(target)?;
    }
    if s.dt_views.is_empty()
        || s.dt_views.len() > 3
        || s.dt_views.iter().any(|v| !VIEWS.contains(&v.as_str()))
        || s.dt_views
            .iter()
            .enumerate()
            .any(|(i, v)| s.dt_views[..i].contains(v))
    {
        return Err("Pick at least one DeskTime section".into());
    }
    if !["left", "done"].contains(&s.dt_glance_kind.as_str()) {
        return Err("Invalid glance".into());
    }
    for (rows, allowed) in [
        (&s.dt_rows_day, &DAY_ROWS[..]),
        (&s.dt_rows_week, &PERIOD_ROWS[..]),
        (&s.dt_rows_month, &PERIOD_ROWS[..]),
    ] {
        if rows.len() > allowed.len() || rows.iter().any(|r| !allowed.contains(&r.as_str())) {
            return Err("Invalid DeskTime rows".into());
        }
    }
    for list in [&s.holidays, &s.half_days] {
        if list.len() > 366 {
            return Err("Date lists are limited to 366 entries".into());
        }
        for d in list {
            date(d)?;
        }
    }
    Ok(())
}

fn range(view: &str, today: NaiveDate) -> Result<(NaiveDate, NaiveDate), String> {
    match view {
        "day" => Ok((today, today)),
        "week" => {
            let start = today - Duration::days(today.weekday().num_days_from_monday() as i64);
            Ok((start, start + Duration::days(6)))
        }
        "month" => {
            let start = today.with_day(1).ok_or("Invalid date")?;
            let next = (start + Duration::days(32))
                .with_day(1)
                .ok_or("Invalid date")?;
            Ok((start, next - Duration::days(1)))
        }
        _ => Err("Unknown DeskTime view".into()),
    }
}
pub fn validate_report(r: &Report, today: NaiveDate) -> Result<(), String> {
    if r.holidays.len() > 366 {
        return Err("Too many holidays".into());
    }
    if r.logged_out {
        // A login-page report carries no numbers; only its shape is checked.
        if r.view.len() > 16
            || r.productive_min.is_some()
            || r.at_work_min.is_some()
            || !r.holidays.is_empty()
        {
            return Err("Invalid logged-out report".into());
        }
        return Ok(());
    }
    let expected = range(&r.view, today)?;
    let start = date(&r.range_start)?;
    let end = date(&r.range_end)?;
    if (start, end) != expected {
        return Err("Unexpected DeskTime date range".into());
    }
    let maximum = ((end - start).num_days() + 1) as u32 * 1440;
    if r.productive_min.is_some_and(|n| n > maximum)
        || r.at_work_min.is_some_and(|n| n > maximum)
        || matches!((r.productive_min, r.at_work_min), (Some(p), Some(a)) if p > a)
    {
        return Err("Invalid DeskTime duration".into());
    }
    for holiday in &r.holidays {
        let d = date(holiday)?;
        if d < start || d > end {
            return Err("Holiday outside range".into());
        }
    }
    Ok(())
}

// ---------- target math (same rules as the DeskTime Remaining v5 extension) ----------
/// Day weight: 0 off day or holiday, 0.5 half day, 1 otherwise.
fn weight(s: &Settings, d: NaiveDate, holidays: &[String]) -> f64 {
    let key = d.to_string();
    let weekday = d.weekday().num_days_from_sunday() as u8;
    if !s.work_days.contains(&weekday) || s.holidays.contains(&key) || holidays.contains(&key) {
        return 0.0;
    }
    if s.half_days.contains(&key) {
        0.5
    } else {
        1.0
    }
}
/// Full-day target for the weekday (per-weekday override, else the daily target).
fn day_target(s: &Settings, d: NaiveDate) -> f64 {
    let weekday = d.weekday().num_days_from_sunday() as u8;
    s.day_targets
        .get(&weekday)
        .and_then(|v| clock(v).ok())
        .unwrap_or_else(|| clock(&s.daily_target).unwrap_or(510)) as f64
}
fn target(s: &Settings, d: NaiveDate, holidays: &[String]) -> f64 {
    weight(s, d, holidays) * day_target(s, d)
}
/// (working days, target minutes) from `start` to `end`, inclusive.
fn span(s: &Settings, start: NaiveDate, end: NaiveDate, holidays: &[String]) -> (f64, f64) {
    start
        .iter_days()
        .take_while(|d| *d <= end)
        .fold((0.0, 0.0), |(n, m), d| {
            (n + weight(s, d, holidays), m + target(s, d, holidays))
        })
}
fn duration(minutes: f64) -> String {
    let m = minutes.abs().round() as i64;
    if m < 60 {
        format!("{m}m")
    } else {
        format!("{}h {:02}m", m / 60, m % 60)
    }
}
fn signed(minutes: f64) -> String {
    format!(
        "{}{}",
        if minutes.round() >= 0.0 { "+" } else { "-" },
        duration(minutes)
    )
}
fn days_text(n: f64) -> String {
    let v = if n.fract() == 0.0 {
        format!("{n:.0}")
    } else {
        format!("{n}")
    };
    format!("{v} day{}", if n == 1.0 { "" } else { "s" })
}
fn hhmm(minutes: i64) -> String {
    let d = minutes.div_euclid(1440);
    let m = minutes.rem_euclid(1440);
    format!(
        "{:02}:{:02}{}",
        m / 60,
        m % 60,
        if d > 0 {
            format!(" +{d}d")
        } else {
            String::new()
        }
    )
}
fn row(key: &str, label: &str, value: String, note: String, tone: &str) -> Row {
    Row {
        key: key.into(),
        label: label.into(),
        value,
        note,
        tone: tone.into(),
    }
}

/// Views to read: the shown ones, plus today for the glance and alert, plus the month for the
/// late-arrival allowance that Today and This week also show.
pub fn fetch_views(s: &Settings) -> Vec<&'static str> {
    let shown = |v: &str| s.dt_views.iter().any(|x| x == v);
    let day = shown("day") || s.dt_glance || s.dt_alert_target;
    VIEWS
        .into_iter()
        .filter(|v| match *v {
            "day" => day,
            "week" => shown("week"),
            _ => !s.dt_views.is_empty(),
        })
        .collect()
}
/// Minutes left until end of day when today is a working day, else 0.
fn available(s: &Settings, now: chrono::DateTime<Local>, holidays: &[String]) -> f64 {
    if weight(s, now.date_naive(), holidays) == 0.0 {
        return 0.0;
    }
    let eod = clock(&s.end_of_day).unwrap_or(1110);
    (eod - (now.hour() as i64 * 60 + now.minute() as i64)).max(0) as f64
}
/// Late-arrival row from the month report: month surplus projected to end of today, minus the
/// weekly early-leave reserve. None without a month report covering today.
fn late_row(month: Option<&Report>, s: &Settings, now: chrono::DateTime<Local>) -> Option<Row> {
    let m = month?;
    let today = now.date_naive();
    let (Ok(start), Ok(end)) = (date(&m.range_start), date(&m.range_end)) else {
        return None;
    };
    if today < start || today > end {
        return None;
    }
    let required = span(s, start, today, &m.holidays).1;
    let surplus = (m.productive_min? as f64 + available(s, now, &m.holidays) - required).round();
    let allowance = (surplus - EARLY_LEAVE_WEEKLY_MIN).max(0.0);
    let mut notes = Vec::new();
    if allowance <= 0.0 {
        notes.push(format!(
            "{} {}",
            duration(surplus),
            if surplus >= 0.0 { "ahead" } else { "behind" }
        ));
        notes.push(format!("need over {}", duration(EARLY_LEAVE_WEEKLY_MIN)));
    } else if let Ok(arrive) = clock(&s.start_of_day) {
        notes.push(format!("arrive by {}", hhmm(arrive + allowance as i64)));
    }
    let value = if allowance > 0.0 {
        duration(allowance)
    } else {
        "None".into()
    };
    Some(row(
        "late",
        "Late arrival",
        value,
        notes.join(" · "),
        if allowance > 0.0 { "pos" } else { "" },
    ))
}
pub fn section(
    r: &Report,
    fetched_ms: i64,
    s: &Settings,
    now: chrono::DateTime<Local>,
    month: Option<&Report>,
) -> Section {
    let today = now.date_naive();
    let (Ok(start), Ok(end)) = (date(&r.range_start), date(&r.range_end)) else {
        unreachable!("validated report")
    };
    let productive = r.productive_min.unwrap_or(0) as f64;
    let (days, total) = span(s, start, end, &r.holidays);
    let remaining = total - productive;
    let share = if total > 0.0 {
        productive / total * 100.0
    } else {
        100.0
    };
    let over = remaining <= 0.0 && total > 0.0;
    let big = if remaining > 0.0 {
        duration(remaining)
    } else if total > 0.0 {
        signed(-remaining)
    } else {
        "0m".into()
    };
    let mut out = Section {
        view: r.view.clone(),
        title: String::new(),
        big,
        caption: String::new(),
        over,
        percent: share.clamp(0.0, 100.0),
        pace: None,
        note: String::new(),
        rows: Vec::new(),
        fetched_ms,
    };
    if r.view == "day" {
        out.title = "Today".into();
        out.caption = if total == 0.0 {
            "no target today".into()
        } else if over {
            "overtime".into()
        } else {
            "left".into()
        };
        out.rows.push(row(
            "done",
            "Done",
            duration(productive),
            format!("{}% of target", share.round()),
            "",
        ));
        if remaining > 0.0 && start == today {
            out.rows.push(row(
                "finish",
                "Finish at",
                (now + Duration::minutes(remaining.ceil() as i64))
                    .format("%H:%M")
                    .to_string(),
                String::new(),
                "",
            ));
        }
        let half = s.half_days.contains(&start.to_string()) && total > 0.0;
        out.rows.push(row(
            "target",
            "Target",
            duration(total),
            if half {
                "half day".into()
            } else {
                String::new()
            },
            "",
        ));
        if start == today {
            out.rows.extend(late_row(month, s, now));
        }
        if let Some(at_work) = r.at_work_min.map(f64::from).filter(|a| *a > productive) {
            out.rows.push(row(
                "nonprod",
                "Non-productive",
                duration(at_work - productive),
                format!("{}% productive", (productive / at_work * 100.0).round()),
                "",
            ));
        }
        return out;
    }
    out.title = if r.view == "week" {
        "This week"
    } else {
        "This month"
    }
    .into();
    out.caption = if over {
        "overtime".into()
    } else {
        "left".into()
    };
    out.note = days_text(days);
    let in_range = today >= start && today <= end;
    let (done_days, required) = if in_range {
        span(s, start, today, &r.holidays)
    } else if today > end {
        (days, total)
    } else {
        (0.0, 0.0)
    };
    if in_range && total > 0.0 {
        out.pace = Some((required / total * 100.0).clamp(0.0, 100.0));
    }
    let daily = clock(&s.daily_target).unwrap_or(510) as f64;
    let per_day = if (days * daily - total).abs() < 0.5 {
        format!(" × {}", duration(daily))
    } else {
        String::new()
    };
    out.rows.push(row(
        "target",
        "Target",
        duration(total),
        format!("{}{per_day}", days_text(days)),
        "",
    ));
    let eod_label = hhmm(clock(&s.end_of_day).unwrap_or(1110));
    let avail = if in_range {
        available(s, now, &r.holidays)
    } else {
        0.0
    };
    if in_range {
        let diff = productive - required;
        let mut notes = vec![if diff >= 0.0 {
            "ahead".to_string()
        } else {
            "behind".to_string()
        }];
        if avail > 0.0 {
            let proj = (diff + avail).round();
            notes.push(format!(
                "{} {} at {eod_label}",
                signed(proj),
                if proj >= 0.0 { "ahead" } else { "behind" }
            ));
        }
        out.rows.push(row(
            "todate",
            "To date",
            duration(productive),
            format!("of {} · {}", duration(required), days_text(done_days)),
            "",
        ));
        out.rows.push(row(
            "balance",
            "Balance",
            signed(diff),
            notes.join(" · "),
            if diff >= 0.0 { "pos" } else { "neg" },
        ));
    }
    if avail > 0.0 {
        let left = remaining - avail;
        let rest_days =
            (span(s, today, end, &r.holidays).0 - weight(s, today, &r.holidays)).max(0.0);
        let label = format!("At {eod_label}");
        out.rows.push(if left <= 0.0 {
            row("eod", &label, signed(-left), "overtime".into(), "pos")
        } else if rest_days > 0.0 {
            row(
                "eod",
                &label,
                format!("{} left", duration(left)),
                format!(
                    "{}/day for {}",
                    duration(left / rest_days),
                    days_text(rest_days)
                ),
                "",
            )
        } else {
            row(
                "eod",
                &label,
                format!("{} left", duration(left)),
                "no working days left".into(),
                "neg",
            )
        });
    }
    if in_range {
        out.rows.extend(late_row(
            if r.view == "month" { Some(r) } else { month },
            s,
            now,
        ));
    }
    out
}

pub fn glance(day: &Section, kind: &str) -> String {
    let cell = |key: &str| day.rows.iter().find(|r| r.key == key);
    match kind {
        "done" => match (cell("done"), cell("target")) {
            (Some(d), Some(t)) => format!("{} / {}", d.value, t.value),
            _ => String::new(),
        },
        _ => {
            if day.over {
                day.big.clone()
            } else if day.big == "0m" {
                String::new()
            } else {
                format!("{} left", day.big)
            }
        }
    }
}
/// Productive minutes still missing today, or None without a target today.
pub fn day_remaining(r: &Report, s: &Settings) -> Option<f64> {
    let d = date(&r.range_start).ok()?;
    let total = target(s, d, &r.holidays);
    (total > 0.0).then(|| total - r.productive_min.unwrap_or(0) as f64)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn day(s: &str) -> NaiveDate {
        date(s).unwrap()
    }
    fn report() -> Report {
        Report {
            view: "week".into(),
            productive_min: Some(500),
            at_work_min: Some(600),
            range_start: "2026-11-02".into(),
            range_end: "2026-11-08".into(),
            holidays: vec![],
            logged_out: false,
        }
    }
    fn at(s: &str) -> chrono::DateTime<Local> {
        use chrono::TimeZone;
        Local
            .from_local_datetime(
                &chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap(),
            )
            .unwrap()
    }
    #[test]
    fn defaults_and_weighted_targets() {
        let mut s = Settings::default();
        assert_eq!(
            span(&s, day("2026-11-02"), day("2026-11-08"), &[]),
            (5.0, 2550.0)
        );
        s.holidays.push("2026-11-03".into());
        s.half_days.push("2026-11-04".into());
        s.day_targets.insert(5, "06:00".into());
        assert_eq!(
            span(&s, day("2026-11-02"), day("2026-11-08"), &[]),
            (3.5, 1635.0)
        );
        assert_eq!(target(&s, day("2026-11-07"), &[]), 0.0);
        assert_eq!(target(&s, day("2026-11-04"), &["2026-11-04".into()]), 0.0);
    }
    #[test]
    fn rejects_untrusted_payloads() {
        let mut r = report();
        assert!(validate_report(&r, day("2026-11-04")).is_ok());
        r.productive_min = Some(10081);
        assert!(validate_report(&r, day("2026-11-04")).is_err());
        r = report();
        r.range_end = "2026-11-09".into();
        assert!(validate_report(&r, day("2026-11-04")).is_err());
        r = report();
        r.holidays = vec!["2026-11-01".into()];
        assert!(validate_report(&r, day("2026-11-04")).is_err());
        r = report();
        r.holidays = vec!["2026-11-04".into(); 367];
        assert!(validate_report(&r, day("2026-11-04")).is_err());
        r = report();
        r.view = "unknown".into();
        assert!(validate_report(&r, day("2026-11-04")).is_err());
        r = report();
        r.productive_min = Some(601);
        assert!(validate_report(&r, day("2026-11-04")).is_err());
        r = report();
        r.logged_out = true;
        r.productive_min = None;
        r.at_work_min = None;
        assert!(validate_report(&r, day("2026-11-04")).is_ok());
        r.productive_min = Some(1);
        assert!(validate_report(&r, day("2026-11-04")).is_err());
        assert!(serde_json::from_str::<Report>(r#"{"view":"day","extra":1}"#).is_err());
    }
    #[test]
    fn calendar_boundaries_and_settings() {
        assert_eq!(
            range("week", day("2027-01-01")).unwrap(),
            (day("2026-12-28"), day("2027-01-03"))
        );
        assert_eq!(
            range("month", day("2028-02-15")).unwrap().1,
            day("2028-02-29")
        );
        let mut s = Settings::default();
        assert!(validate_settings(&s).is_ok());
        s.end_of_day = "24:00".into();
        assert!(validate_settings(&s).is_err());
        assert!(clock("ab:00").is_err());
        assert!(date("2026-02-30").is_err());
    }
    #[test]
    fn day_section_remaining_and_overtime() {
        let s = Settings::default();
        let mut r = Report {
            view: "day".into(),
            productive_min: Some(376),
            at_work_min: Some(414),
            range_start: "2026-11-04".into(),
            range_end: "2026-11-04".into(),
            holidays: vec![],
            logged_out: false,
        };
        let sec = section(&r, 1, &s, at("2026-11-04 11:00"), None);
        assert_eq!(sec.big, "2h 14m");
        assert!(!sec.over);
        assert_eq!(sec.rows[0].note, "74% of target");
        assert_eq!(sec.rows[1].value, "13:14");
        assert_eq!(sec.rows[3].value, "38m");
        r.productive_min = Some(532);
        r.at_work_min = Some(532);
        let sec = section(&r, 1, &s, at("2026-11-04 17:00"), None);
        assert_eq!(sec.big, "+22m");
        assert!(sec.over);
        assert_eq!(sec.percent, 100.0);
        // v5: Finish at disappears once the target is met; no Non-productive row without idle time.
        assert!(!sec
            .rows
            .iter()
            .any(|r| r.key == "finish" || r.key == "nonprod"));
    }
    #[test]
    fn period_balance_projection_and_late_arrival() {
        let s = Settings {
            start_of_day: "09:00".into(),
            ..Settings::default()
        };
        // Wed 2026-11-04 17:30: required Mon-Wed = 3 x 510; 60 min left until 18:30.
        let r = Report {
            view: "week".into(),
            productive_min: Some(1530),
            at_work_min: None,
            range_start: "2026-11-02".into(),
            range_end: "2026-11-08".into(),
            holidays: vec![],
            logged_out: false,
        };
        let now = at("2026-11-04 17:30");
        let sec = section(&r, 1, &s, now, None);
        assert_eq!(sec.note, "5 days");
        assert_eq!(sec.big, "17h 00m");
        assert_eq!(sec.pace, Some(60.0));
        assert_eq!(sec.rows[0].note, "5 days × 8h 30m");
        assert_eq!(sec.rows[1].note, "of 25h 30m · 3 days");
        assert_eq!(sec.rows[2].value, "+0m");
        assert_eq!(sec.rows[2].note, "ahead · +1h 00m ahead at 18:30");
        assert_eq!(
            (
                sec.rows[3].label.as_str(),
                sec.rows[3].value.as_str(),
                sec.rows[3].note.as_str()
            ),
            ("At 18:30", "16h 00m left", "8h 00m/day for 2 days")
        );
        assert_eq!(sec.rows.len(), 4);
        let m = Report {
            view: "month".into(),
            productive_min: Some(1530 + 400),
            at_work_min: None,
            range_start: "2026-11-01".into(),
            range_end: "2026-11-30".into(),
            holidays: vec![],
            logged_out: false,
        };
        let sec = section(&m, 1, &s, now, None);
        // Required Nov 2-4 = 1530; balance 400 + 60 available - 90 reserve = 370 minutes late allowance.
        assert_eq!(sec.rows[4].value, "6h 10m");
        assert_eq!(sec.rows[4].note, "arrive by 15:10");
        // Week and Today show the same allowance from the month report.
        assert_eq!(section(&r, 1, &s, now, Some(&m)).rows[4].value, "6h 10m");
        let d = Report {
            view: "day".into(),
            productive_min: Some(500),
            at_work_min: None,
            range_start: "2026-11-04".into(),
            range_end: "2026-11-04".into(),
            holidays: vec![],
            logged_out: false,
        };
        assert!(section(&d, 1, &s, now, Some(&m))
            .rows
            .iter()
            .any(|r| r.key == "late" && r.value == "6h 10m"));
        let behind = Report {
            productive_min: Some(1000),
            ..m.clone()
        };
        let late = late_row(Some(&behind), &s, now).unwrap();
        assert_eq!(
            (late.value.as_str(), late.note.as_str()),
            ("None", "7h 50m behind · need over 1h 30m")
        );
    }
    #[test]
    fn half_days_count_as_half() {
        let mut s = Settings::default();
        s.half_days.push("2026-11-06".into());
        let r = Report {
            view: "week".into(),
            productive_min: Some(0),
            at_work_min: None,
            range_start: "2026-11-02".into(),
            range_end: "2026-11-08".into(),
            holidays: vec![],
            logged_out: false,
        };
        let sec = section(&r, 1, &s, at("2026-11-02 10:00"), None);
        assert_eq!(sec.note, "4.5 days");
        assert_eq!(sec.rows[0].note, "4.5 days × 8h 30m");
    }
    #[test]
    fn glance_and_fetch_order() {
        let mut s = Settings::default();
        let day = Report {
            view: "day".into(),
            productive_min: Some(300),
            at_work_min: None,
            range_start: "2026-11-04".into(),
            range_end: "2026-11-04".into(),
            holidays: vec![],
            logged_out: false,
        };
        let sec = section(&day, 1, &s, at("2026-11-04 11:00"), None);
        assert_eq!(glance(&sec, "left"), "3h 30m left");
        assert_eq!(glance(&sec, "done"), "5h 00m / 8h 30m");
        assert_eq!(fetch_views(&s), vec!["day", "week", "month"]);
        s.dt_views = vec!["month".into()];
        s.dt_alert_target = false;
        assert_eq!(fetch_views(&s), vec!["month"]);
        s.dt_glance = true;
        assert_eq!(fetch_views(&s), vec!["day", "month"]);
        s.dt_glance_kind = "leave".into();
        assert!(validate_settings(&s).is_err());
        s.dt_glance_kind = "left".into();
        s.dt_views = vec![];
        assert!(validate_settings(&s).is_err());
        s.dt_views = vec!["week".into(), "week".into()];
        assert!(validate_settings(&s).is_err());
    }
}

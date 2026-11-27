use chrono::{DateTime, Datelike, Local, TimeZone};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub summary: String,
    pub start_utc: i64,
    pub end_utc: i64,
    pub location: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalendarCommand {
    SetUrl(String),
    Refresh,
}

pub const UPCOMING_HORIZON_SECS: i64 = 7 * 86_400;

pub(crate) fn current_week_end_utc(now_utc: i64) -> i64 {
    DateTime::from_timestamp(now_utc, 0)
        .and_then(|dt| {
            let local = dt.with_timezone(&Local);
            let days_until_sunday = u64::from(6 - local.weekday().num_days_from_monday());
            let sunday_date = local.date_naive() + chrono::Days::new(days_until_sunday);
            let end_naive = sunday_date.and_hms_opt(23, 59, 59)?;
            Local
                .from_local_datetime(&end_naive)
                .latest()
                .map(|d| d.timestamp())
        })
        .unwrap_or(now_utc + UPCOMING_HORIZON_SECS)
}

pub fn upcoming_events(
    events: &[CalendarEvent],
    now_utc: i64,
    limit: usize,
) -> Vec<&CalendarEvent> {
    let horizon_utc = current_week_end_utc(now_utc);
    let mut upcoming: Vec<&CalendarEvent> = events
        .iter()
        .filter(|ev| {
            let active_or_future = if ev.end_utc > ev.start_utc {
                ev.end_utc >= now_utc
            } else {
                ev.start_utc >= now_utc - 600
            };
            active_or_future && ev.start_utc <= horizon_utc
        })
        .collect();
    upcoming.sort_by_key(|ev| ev.start_utc);
    upcoming.truncate(limit);
    upcoming
}

pub fn format_countdown(start_utc: i64, end_utc: i64, now_utc: i64) -> String {
    if start_utc <= now_utc && now_utc <= end_utc.max(start_utc + 600) {
        return "Now".to_owned();
    }
    let diff = (start_utc - now_utc).max(0);
    let mins = diff / 60;
    if mins < 60 {
        format!("In {}m", mins.max(1))
    } else if mins < 60 * 24 {
        let h = mins / 60;
        let m = mins % 60;
        if m == 0 {
            format!("In {h}h")
        } else {
            format!("In {h}h {m}m")
        }
    } else {
        format!("In {}d", mins / (60 * 24))
    }
}

pub fn format_local_time(utc_ts: i64) -> String {
    DateTime::from_timestamp(utc_ts, 0)
        .map(|dt| dt.with_timezone(&Local).format("%H:%M").to_string())
        .unwrap_or_else(|| "--:--".to_owned())
}

use std::collections::HashSet;
use std::str::FromStr;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use chrono::{
    DateTime, Datelike, Days, Local, Months, NaiveDate, NaiveDateTime, TimeZone, Weekday,
};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use ureq::Agent;

use crate::platform::PlatformEvent;

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

struct RawVEvent {
    uid: Option<String>,
    summary: String,
    location: Option<String>,
    start_utc: i64,
    end_utc: i64,
    tzid: Option<String>,
    rrule: Option<String>,
    exdates: Vec<i64>,
    recurrence_id: Option<i64>,
}

#[cfg(test)]
pub fn parse_ics(ics: &str) -> Vec<CalendarEvent> {
    let now_utc = chrono::Utc::now().timestamp();
    parse_ics_in_range(ics, now_utc - 86_400, now_utc + 90 * 86_400)
}

pub fn parse_ics_in_range(ics: &str, min_ts: i64, max_ts: i64) -> Vec<CalendarEvent> {
    let unfolded = unfold_ics_lines(ics);
    let mut raw_events = Vec::new();
    let mut in_event = false;
    let mut uid: Option<String> = None;
    let mut summary = String::new();
    let mut location: Option<String> = None;
    let mut start_utc: Option<i64> = None;
    let mut end_utc: Option<i64> = None;
    let mut tzid: Option<String> = None;
    let mut rrule: Option<String> = None;
    let mut exdates: Vec<i64> = Vec::new();
    let mut recurrence_id: Option<i64> = None;

    for line in unfolded {
        match line.as_str() {
            "BEGIN:VEVENT" => {
                in_event = true;
                uid = None;
                summary.clear();
                location = None;
                start_utc = None;
                end_utc = None;
                tzid = None;
                rrule = None;
                exdates.clear();
                recurrence_id = None;
            }
            "END:VEVENT" => {
                if in_event && let Some(start) = start_utc {
                    let end = end_utc.unwrap_or(start);
                    let title = if summary.trim().is_empty() {
                        "(No title)".to_owned()
                    } else {
                        summary.trim().to_owned()
                    };
                    raw_events.push(RawVEvent {
                        uid: uid.take(),
                        summary: title,
                        location: location.take().filter(|l| !l.trim().is_empty()),
                        start_utc: start,
                        end_utc: end,
                        tzid: tzid.take(),
                        rrule: rrule.take(),
                        exdates: std::mem::take(&mut exdates),
                        recurrence_id: recurrence_id.take(),
                    });
                }
                in_event = false;
            }
            _ if in_event => {
                let Some((key_part, val)) = line.split_once(':') else {
                    continue;
                };
                let mut key_segments = key_part.split(';');
                let key = key_segments.next().unwrap_or(key_part);
                let line_tzid = key_segments
                    .find_map(|p| p.strip_prefix("TZID="))
                    .map(str::to_owned);
                match key {
                    "UID" => uid = Some(val.trim().to_owned()),
                    "SUMMARY" => summary = unescape_ics(val),
                    "LOCATION" => location = Some(unescape_ics(val)),
                    "DTSTART" => {
                        tzid = line_tzid;
                        start_utc = parse_ics_datetime(val.trim(), tzid.as_deref());
                    }
                    "DTEND" => end_utc = parse_ics_datetime(val.trim(), line_tzid.as_deref()),
                    "RRULE" => rrule = Some(val.trim().to_owned()),
                    "EXDATE" => {
                        for item in val.split(',') {
                            if let Some(ts) = parse_ics_datetime(item.trim(), line_tzid.as_deref())
                            {
                                exdates.push(ts);
                            }
                        }
                    }
                    "RECURRENCE-ID" => {
                        recurrence_id = parse_ics_datetime(val.trim(), line_tzid.as_deref());
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    let overrides: HashSet<(String, i64)> = raw_events
        .iter()
        .filter_map(|ev| Some((ev.uid.clone()?, ev.recurrence_id?)))
        .collect();

    let mut out = Vec::new();
    for ev in raw_events {
        if let Some(rule) = ev.rrule.as_deref() {
            expand_rrule(&ev, rule, min_ts, max_ts, &overrides, &mut out);
        } else {
            out.push(CalendarEvent {
                summary: ev.summary,
                start_utc: ev.start_utc,
                end_utc: ev.end_utc,
                location: ev.location,
            });
        }
    }
    out
}

fn expand_rrule(
    ev: &RawVEvent,
    rule: &str,
    min_ts: i64,
    max_ts: i64,
    overrides: &HashSet<(String, i64)>,
    out: &mut Vec<CalendarEvent>,
) {
    let mut freq = "";
    let mut interval: u32 = 1;
    let mut count: Option<u32> = None;
    let mut until: Option<i64> = None;
    let mut byday: Vec<Weekday> = Vec::new();

    for part in rule.split(';') {
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        match k {
            "FREQ" => freq = v,
            "INTERVAL" => interval = v.parse::<u32>().unwrap_or(1).max(1),
            "COUNT" => count = v.parse::<u32>().ok(),
            "UNTIL" => until = parse_ics_datetime(v.trim(), ev.tzid.as_deref()),
            "BYDAY" => {
                for d in v.split(',') {
                    if let Some(wd) = parse_ics_weekday(d.trim()) {
                        byday.push(wd);
                    }
                }
            }
            _ => {}
        }
    }

    let duration = (ev.end_utc - ev.start_utc).max(0);
    let effective_max = until.map_or(max_ts, |u| max_ts.min(u));
    if ev.start_utc > effective_max {
        return;
    }

    let parsed_tz = ev.tzid.as_deref().and_then(|t| Tz::from_str(t).ok());
    let start_local_dt = DateTime::from_timestamp(ev.start_utc, 0)
        .map(|dt| dt.with_timezone(&Local))
        .unwrap_or_else(Local::now);
    let base_date = parsed_tz
        .and_then(|tz| {
            DateTime::from_timestamp(ev.start_utc, 0).map(|d| d.with_timezone(&tz).date_naive())
        })
        .unwrap_or_else(|| start_local_dt.date_naive());
    let base_time = parsed_tz
        .and_then(|tz| {
            DateTime::from_timestamp(ev.start_utc, 0).map(|d| d.with_timezone(&tz).time())
        })
        .unwrap_or_else(|| start_local_dt.time());

    let to_utc_ts = |date: NaiveDate| -> Option<i64> {
        let ndt = NaiveDateTime::new(date, base_time);
        if let Some(tz) = parsed_tz {
            tz.from_local_datetime(&ndt)
                .earliest()
                .map(|d| d.timestamp())
        } else {
            Local
                .from_local_datetime(&ndt)
                .earliest()
                .map(|d| d.timestamp())
        }
    };

    let mut emitted_count: u32 = 0;
    let mut push_if_valid = |occ_ts: i64| -> bool {
        if let Some(u) = until
            && occ_ts > u
        {
            return false;
        }
        emitted_count += 1;
        if let Some(max_c) = count
            && emitted_count > max_c
        {
            return false;
        }
        if occ_ts > max_ts {
            return false;
        }
        let occ_end = occ_ts + duration;
        let is_excluded = ev.exdates.contains(&occ_ts)
            || ev
                .uid
                .as_ref()
                .is_some_and(|id| overrides.contains(&(id.clone(), occ_ts)));
        if !is_excluded && occ_end >= min_ts {
            out.push(CalendarEvent {
                summary: ev.summary.clone(),
                start_utc: occ_ts,
                end_utc: occ_end,
                location: ev.location.clone(),
            });
        }
        true
    };

    const MAX_ITERS: u32 = 5000;
    match freq {
        "DAILY" => {
            for step in 0..MAX_ITERS {
                let Some(date) = base_date.checked_add_days(Days::new(u64::from(step * interval)))
                else {
                    break;
                };
                let Some(ts) = to_utc_ts(date) else { continue };
                if !byday.is_empty() && !byday.contains(&date.weekday()) {
                    continue;
                }
                if !push_if_valid(ts) {
                    break;
                }
            }
        }
        "WEEKLY" => {
            if byday.is_empty() {
                byday.push(base_date.weekday());
            }
            let week_monday =
                base_date - Days::new(u64::from(base_date.weekday().num_days_from_monday()));
            let mut offsets: Vec<u64> = byday
                .iter()
                .map(|w| u64::from(w.num_days_from_monday()))
                .collect();
            offsets.sort_unstable();
            offsets.dedup();

            'outer: for week_idx in 0..MAX_ITERS {
                let Some(cur_monday) =
                    week_monday.checked_add_days(Days::new(u64::from(week_idx * interval) * 7))
                else {
                    break;
                };
                for &off in &offsets {
                    let Some(date) = cur_monday.checked_add_days(Days::new(off)) else {
                        continue;
                    };
                    if date < base_date {
                        continue;
                    }
                    let Some(ts) = to_utc_ts(date) else { continue };
                    if ts < ev.start_utc {
                        continue;
                    }
                    if !push_if_valid(ts) {
                        break 'outer;
                    }
                }
            }
        }
        "MONTHLY" => {
            for step in 0..MAX_ITERS {
                let Some(date) = base_date.checked_add_months(Months::new(step * interval)) else {
                    break;
                };
                let Some(ts) = to_utc_ts(date) else { continue };
                if !push_if_valid(ts) {
                    break;
                }
            }
        }
        "YEARLY" => {
            for step in 0..MAX_ITERS {
                let Some(date) = base_date.checked_add_months(Months::new(step * interval * 12))
                else {
                    break;
                };
                let Some(ts) = to_utc_ts(date) else { continue };
                if !push_if_valid(ts) {
                    break;
                }
            }
        }
        _ => {
            let _ = push_if_valid(ev.start_utc);
        }
    }
}

fn parse_ics_weekday(code: &str) -> Option<Weekday> {
    let suffix = if code.len() >= 2 {
        &code[code.len() - 2..]
    } else {
        code
    };
    match suffix {
        "MO" => Some(Weekday::Mon),
        "TU" => Some(Weekday::Tue),
        "WE" => Some(Weekday::Wed),
        "TH" => Some(Weekday::Thu),
        "FR" => Some(Weekday::Fri),
        "SA" => Some(Weekday::Sat),
        "SU" => Some(Weekday::Sun),
        _ => None,
    }
}

fn unfold_ics_lines(ics: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in ics.lines() {
        let line = raw.trim_end_matches('\r');
        if let Some(cont) = line.strip_prefix(' ').or_else(|| line.strip_prefix('\t')) {
            if let Some(prev) = out.last_mut() {
                prev.push_str(cont);
            }
        } else if !line.is_empty() {
            out.push(line.to_owned());
        }
    }
    out
}

fn unescape_ics(s: &str) -> String {
    s.replace("\\n", " ")
        .replace("\\N", " ")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}

fn parse_ics_datetime(val: &str, tzid: Option<&str>) -> Option<i64> {
    if let Ok(ndt) = NaiveDateTime::parse_from_str(val, "%Y%m%dT%H%M%SZ") {
        return Some(ndt.and_utc().timestamp());
    }
    if let Ok(ndt) = NaiveDateTime::parse_from_str(val, "%Y%m%dT%H%M%S") {
        if let Some(tz_name) = tzid
            && let Ok(tz) = Tz::from_str(tz_name)
            && let Some(dt) = tz.from_local_datetime(&ndt).earliest()
        {
            return Some(dt.timestamp());
        }
        let local_ts = Local
            .from_local_datetime(&ndt)
            .earliest()
            .map(|dt| dt.timestamp());
        return Some(local_ts.unwrap_or_else(|| ndt.and_utc().timestamp()));
    }
    if let Ok(date) = NaiveDate::parse_from_str(val, "%Y%m%d")
        && let Some(ndt) = date.and_hms_opt(0, 0, 0)
    {
        let local_ts = Local
            .from_local_datetime(&ndt)
            .earliest()
            .map(|dt| dt.timestamp());
        return Some(local_ts.unwrap_or_else(|| ndt.and_utc().timestamp()));
    }
    None
}

pub const UPCOMING_HORIZON_SECS: i64 = 7 * 86_400;

fn current_week_end_utc(now_utc: i64) -> i64 {
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

pub fn spawn_calendar_worker(
    initial_url: String,
    cmd_rx: mpsc::Receiver<CalendarCommand>,
    event_tx: mpsc::Sender<PlatformEvent>,
    on_wake: Arc<dyn Fn() + Send + Sync>,
) {
    std::thread::Builder::new()
        .name("calendar-worker".to_owned())
        .spawn(move || {
            let config = Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(10)))
                .build();
            let agent: Agent = config.into();
            let mut url = initial_url.trim().to_owned();

            loop {
                if !url.is_empty() {
                    let res = fetch_ics(&agent, &url);
                    let _ = event_tx.send(PlatformEvent::CalendarUpdated(res));
                    on_wake();
                } else {
                    let _ = event_tx.send(PlatformEvent::CalendarUpdated(Ok(Vec::new())));
                    on_wake();
                }

                match cmd_rx.recv_timeout(Duration::from_secs(120)) {
                    Ok(CalendarCommand::SetUrl(next)) => {
                        url = next.trim().to_owned();
                    }
                    Ok(CalendarCommand::Refresh) => {}
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .expect("spawn calendar worker thread");
}

fn fetch_ics(agent: &Agent, url: &str) -> Result<Vec<CalendarEvent>, String> {
    const MAX_ICS_BYTES: u64 = 2 * 1024 * 1024;
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Invalid URL (must start with https:// or http://)".to_owned());
    }
    let body = agent
        .get(url)
        .header("User-Agent", "LiebertDynamicIsland/0.1")
        .call()
        .map_err(|e| format!("Sync error: {e}"))?
        .body_mut()
        .with_config()
        .limit(MAX_ICS_BYTES)
        .read_to_string()
        .map_err(|e| format!("Read error: {e}"))?;
    let now_utc = chrono::Utc::now().timestamp();
    let min_ts = now_utc - 86_400;
    let max_ts = now_utc + UPCOMING_HORIZON_SECS + 2 * 86_400;
    let mut events = parse_ics_in_range(&body, min_ts, max_ts);
    events.retain(|ev| ev.end_utc.max(ev.start_utc) >= min_ts && ev.start_utc <= max_ts);
    events.sort_by_key(|ev| ev.start_utc);
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ics_unfolds_and_unescapes() {
        let sample = "\
BEGIN:VCALENDAR\r\n\
BEGIN:VEVENT\r\n\
DTSTART:20261125T150000Z\r\n\
DTEND:20261125T160000Z\r\n\
SUMMARY:Sprint\\, Review \r\n Part 2\r\n\
LOCATION:Room A\\; Meet\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";

        let events = parse_ics(sample);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "Sprint, Review Part 2");
        assert_eq!(events[0].location.as_deref(), Some("Room A; Meet"));
        assert!(events[0].end_utc > events[0].start_utc);
    }

    #[test]
    fn upcoming_events_filters_and_formats_countdown() {
        let events = vec![
            CalendarEvent {
                summary: "Past".to_owned(),
                start_utc: 100,
                end_utc: 200,
                location: None,
            },
            CalendarEvent {
                summary: "Later".to_owned(),
                start_utc: 5000,
                end_utc: 6000,
                location: None,
            },
            CalendarEvent {
                summary: "Soon".to_owned(),
                start_utc: 1300,
                end_utc: 2000,
                location: None,
            },
            CalendarEvent {
                summary: "TooFar".to_owned(),
                start_utc: current_week_end_utc(1000) + 1,
                end_utc: current_week_end_utc(1000) + 3600,
                location: None,
            },
        ];

        let next = upcoming_events(&events, 1000, 5);
        assert_eq!(next.len(), 2);
        assert_eq!(next[0].summary, "Soon");
        assert_eq!(next[1].summary, "Later");
        assert_eq!(format_countdown(1300, 2000, 1000), "In 5m");
        assert_eq!(format_countdown(900, 2000, 1000), "Now");
    }

    #[test]
    fn parse_ics_expands_weekly_rrule_and_respects_exdate() {
        let ics = "\
BEGIN:VCALENDAR\r\n\
BEGIN:VEVENT\r\n\
UID:weekly-1\r\n\
DTSTART;TZID=Asia/Jakarta:20261105T130000\r\n\
DTEND;TZID=Asia/Jakarta:20261105T143000\r\n\
RRULE:FREQ=WEEKLY;BYDAY=TH\r\n\
EXDATE;TZID=Asia/Jakarta:20261119T130000\r\n\
SUMMARY:Sistem Basis Data\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";
        let min_ts = parse_ics_datetime("20261118T000000Z", None).unwrap();
        let max_ts = parse_ics_datetime("20261128T000000Z", None).unwrap();
        let events = parse_ics_in_range(ics, min_ts, max_ts);
        // Nov 19 is excluded by EXDATE; Nov 26 13:00 Asia/Jakarta must appear.
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "Sistem Basis Data");
        let expected_nov26 = parse_ics_datetime("20261126T130000", Some("Asia/Jakarta")).unwrap();
        assert_eq!(events[0].start_utc, expected_nov26);
    }
}

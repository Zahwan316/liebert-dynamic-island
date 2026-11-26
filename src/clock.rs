use chrono::{DateTime, FixedOffset, Local, Timelike, Utc};
#[cfg(feature = "tz-iana")]
use chrono_tz::Tz;

pub const COMMON_TIMEZONES: &[(&str, &str)] = &[
    ("Local", "System Local"),
    ("UTC", "UTC"),
    ("Asia/Jakarta", "WIB — Asia/Jakarta"),
    ("Asia/Makassar", "WITA — Asia/Makassar"),
    ("Asia/Jayapura", "WIT — Asia/Jayapura"),
    ("Asia/Singapore", "SGT — Asia/Singapore"),
    ("Asia/Tokyo", "JST — Asia/Tokyo"),
    ("Asia/Seoul", "KST — Asia/Seoul"),
    ("Europe/London", "GMT/BST — Europe/London"),
    ("Europe/Berlin", "CET — Europe/Berlin"),
    ("America/New_York", "EST/EDT — New York"),
    ("America/Los_Angeles", "PST/PDT — Los Angeles"),
    ("Australia/Sydney", "AEST — Sydney"),
];

pub(crate) fn resolve_fixed_timezone(tz_name: &str) -> Option<(FixedOffset, &'static str)> {
    const HOUR: i32 = 3600;
    match tz_name.trim() {
        "UTC" => Some((FixedOffset::east_opt(0)?, "UTC")),
        "Asia/Jakarta" => Some((FixedOffset::east_opt(7 * HOUR)?, "WIB")),
        "Asia/Makassar" => Some((FixedOffset::east_opt(8 * HOUR)?, "WITA")),
        "Asia/Jayapura" => Some((FixedOffset::east_opt(9 * HOUR)?, "WIT")),
        "Asia/Singapore" => Some((FixedOffset::east_opt(8 * HOUR)?, "SGT")),
        "Asia/Tokyo" => Some((FixedOffset::east_opt(9 * HOUR)?, "JST")),
        "Asia/Seoul" => Some((FixedOffset::east_opt(9 * HOUR)?, "KST")),
        // shortcut: fixed standard offsets when tz-iana is off, enable --features tz-iana for DST
        "Europe/London" => Some((FixedOffset::east_opt(0)?, "GMT")),
        "Europe/Berlin" => Some((FixedOffset::east_opt(HOUR)?, "CET")),
        "America/New_York" => Some((FixedOffset::east_opt(-5 * HOUR)?, "EST")),
        "America/Los_Angeles" => Some((FixedOffset::east_opt(-8 * HOUR)?, "PST")),
        "Australia/Sydney" => Some((FixedOffset::east_opt(10 * HOUR)?, "AEST")),
        _ => None,
    }
}

pub fn is_valid_timezone(tz_name: &str) -> bool {
    let trimmed = tz_name.trim();
    if trimmed.eq_ignore_ascii_case("local") || resolve_fixed_timezone(trimmed).is_some() {
        return true;
    }
    #[cfg(feature = "tz-iana")]
    if trimmed.parse::<Tz>().is_ok() {
        return true;
    }
    false
}

pub fn format_clock(
    now_utc: DateTime<Utc>,
    tz_name: &str,
    use_24h: bool,
    show_seconds: bool,
) -> (String, String) {
    let trimmed = tz_name.trim();
    if !trimmed.eq_ignore_ascii_case("local") {
        #[cfg(feature = "tz-iana")]
        if let Ok(tz) = trimmed.parse::<Tz>() {
            let dt = now_utc.with_timezone(&tz);
            let time_str = format_hms(dt.hour(), dt.minute(), dt.second(), use_24h, show_seconds);
            let label = dt.format("%Z").to_string();
            return (time_str, label);
        }

        if let Some((offset, label)) = resolve_fixed_timezone(trimmed) {
            let dt = now_utc.with_timezone(&offset);
            let time_str = format_hms(dt.hour(), dt.minute(), dt.second(), use_24h, show_seconds);
            return (time_str, label.to_owned());
        }
    }

    let dt = now_utc.with_timezone(&Local);
    let time_str = format_hms(dt.hour(), dt.minute(), dt.second(), use_24h, show_seconds);
    (time_str, "Local".to_owned())
}

fn format_hms(hour: u32, min: u32, sec: u32, use_24h: bool, show_seconds: bool) -> String {
    if use_24h {
        if show_seconds {
            format!("{hour:02}:{min:02}:{sec:02}")
        } else {
            format!("{hour:02}:{min:02}")
        }
    } else {
        let (pm, h12) = match hour {
            0 => (false, 12),
            1..=11 => (false, hour),
            12 => (true, 12),
            _ => (true, hour - 12),
        };
        let suffix = if pm { "PM" } else { "AM" };
        if show_seconds {
            format!("{h12:02}:{min:02}:{sec:02} {suffix}")
        } else {
            format!("{h12:02}:{min:02} {suffix}")
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn formats_iana_timezone_24h_and_12h() {
        let utc_dt = Utc.with_ymd_and_hms(2026, 11, 25, 15, 5, 9).unwrap();

        let (t24, label) = format_clock(utc_dt, "Asia/Jakarta", true, false);
        assert_eq!(t24, "22:05");
        assert_eq!(label, "WIB");

        let (t12, _) = format_clock(utc_dt, "UTC", false, true);
        assert_eq!(t12, "03:05:09 PM");
    }

    #[test]
    fn validates_timezone_names() {
        assert!(is_valid_timezone("Local"));
        assert!(is_valid_timezone("Asia/Tokyo"));
        assert!(!is_valid_timezone("Mars/Olympus"));
    }
}


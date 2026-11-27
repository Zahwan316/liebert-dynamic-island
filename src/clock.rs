use chrono::{DateTime, Local, Timelike, Utc};
use chrono_tz::Tz;

pub fn format_clock(
    now_utc: DateTime<Utc>,
    tz_name: &str,
    use_24h: bool,
    show_seconds: bool,
) -> (String, String) {
    let trimmed = tz_name.trim();
    if !trimmed.eq_ignore_ascii_case("local")
        && let Ok(tz) = trimmed.parse::<Tz>()
    {
        let dt = now_utc.with_timezone(&tz);
        let time_str = format_hms(dt.hour(), dt.minute(), dt.second(), use_24h, show_seconds);
        let label = dt.format("%Z").to_string();
        return (time_str, label);
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
    use crate::clock_light::is_valid_timezone;

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

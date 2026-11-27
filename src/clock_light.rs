use chrono::{DateTime, FixedOffset, Local, Timelike, Utc};

#[allow(dead_code)] // Used by lightweight Settings binary without chrono-tz.
pub fn is_valid_timezone(tz_name: &str) -> bool {
    let trimmed = tz_name.trim();
    if trimmed.eq_ignore_ascii_case("local") || trimmed.eq_ignore_ascii_case("utc") {
        return true;
    }
    if preset_offset(trimmed).is_some() {
        return true;
    }
    #[cfg(unix)]
    {
        let clean = trimmed.trim_start_matches('/');
        if !clean.contains("..")
            && std::path::Path::new("/usr/share/zoneinfo")
                .join(clean)
                .is_file()
        {
            return true;
        }
    }
    false
}

#[allow(dead_code)]
fn preset_offset(tz_name: &str) -> Option<(i32, &'static str)> {
    match tz_name {
        "UTC" => Some((0, "UTC")),
        "Asia/Jakarta" => Some((7 * 3600, "WIB")),
        "Asia/Makassar" => Some((8 * 3600, "WITA")),
        "Asia/Jayapura" => Some((9 * 3600, "WIT")),
        "Asia/Singapore" => Some((8 * 3600, "SGT")),
        "Asia/Tokyo" => Some((9 * 3600, "JST")),
        "Asia/Seoul" => Some((9 * 3600, "KST")),
        "Europe/London" => Some((0, "GMT")),
        "Europe/Berlin" => Some((3600, "CET")),
        "America/New_York" => Some((-5 * 3600, "EST")),
        "America/Los_Angeles" => Some((-8 * 3600, "PST")),
        "Australia/Sydney" => Some((10 * 3600, "AEST")),
        _ => None,
    }
}

#[allow(dead_code)]
pub fn format_clock(
    now_utc: DateTime<Utc>,
    tz_name: &str,
    use_24h: bool,
    show_seconds: bool,
) -> (String, String) {
    let trimmed = tz_name.trim();
    if let Some((secs, label)) = preset_offset(trimmed)
        && let Some(offset) = FixedOffset::east_opt(secs)
    {
        let dt = now_utc.with_timezone(&offset);
        return (
            format_hms(dt.hour(), dt.minute(), dt.second(), use_24h, show_seconds),
            label.to_owned(),
        );
    }

    let dt = now_utc.with_timezone(&Local);
    let label = if trimmed.eq_ignore_ascii_case("local") || trimmed.is_empty() {
        "Local".to_owned()
    } else {
        trimmed.rsplit('/').next().unwrap_or("Local").to_owned()
    };
    (
        format_hms(dt.hour(), dt.minute(), dt.second(), use_24h, show_seconds),
        label,
    )
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

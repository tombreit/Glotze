use jiff::{Timestamp, Zoned, tz::TimeZone};

/// "30:30" for under an hour, "1:05:00" otherwise.
pub fn format_duration(seconds: u64) -> String {
    let h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    let s = seconds % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// "23 May 2026"
pub fn format_date_short(unix_seconds: i64) -> String {
    match to_berlin(unix_seconds) {
        Some(z) => z.strftime("%-d %b %Y").to_string(),
        None => String::new(),
    }
}

/// "20:00"
pub fn format_time(unix_seconds: i64) -> String {
    match to_berlin(unix_seconds) {
        Some(z) => z.strftime("%H:%M").to_string(),
        None => String::new(),
    }
}

fn to_berlin(unix_seconds: i64) -> Option<Zoned> {
    let ts = Timestamp::from_second(unix_seconds).ok()?;
    let tz = TimeZone::get("Europe/Berlin").unwrap_or(TimeZone::UTC);
    Some(ts.to_zoned(tz))
}

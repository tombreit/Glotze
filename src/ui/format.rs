use jiff::{Timestamp, Zoned, tz::TimeZone};

/// Compact, unit-tagged duration for the result columns: "31m" under an hour,
/// "1h 29m" otherwise (and "2h" when the minutes are zero).
pub fn format_duration(seconds: u64) -> String {
    let h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    if h > 0 {
        if m > 0 {
            format!("{h}h {m}m")
        } else {
            format!("{h}h")
        }
    } else {
        format!("{m}m")
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

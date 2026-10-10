use gettextrs::gettext;

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

/// "23 May 2026", with the month name in the user's locale.
pub fn format_date_short(unix_seconds: i64) -> String {
    // Translators: g_date_time_format() pattern for the broadcast date column,
    // e.g. "%-d. %b %Y" for German.
    format_berlin(unix_seconds, &gettext("%-d %b %Y"))
}

/// "20:00"
pub fn format_time(unix_seconds: i64) -> String {
    format_berlin(unix_seconds, "%H:%M")
}

/// Broadcast times are German local time regardless of the user's own zone.
fn format_berlin(unix_seconds: i64, pattern: &str) -> String {
    let tz =
        glib::TimeZone::from_identifier(Some("Europe/Berlin")).unwrap_or_else(glib::TimeZone::utc);
    glib::DateTime::from_unix_utc(unix_seconds)
        .and_then(|dt| dt.to_timezone(&tz))
        .and_then(|dt| dt.format(pattern))
        .map(String::from)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_formats() {
        assert_eq!(format_duration(0), "0m");
        assert_eq!(format_duration(31 * 60 + 59), "31m");
        assert_eq!(format_duration(3600), "1h");
        assert_eq!(format_duration(3600 + 29 * 60), "1h 29m");
    }
}

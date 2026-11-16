use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

pub fn describe(timestamp: i64, now: i64) -> String {
    let elapsed = now.saturating_sub(timestamp);
    if elapsed < 0 {
        return "in the future".to_string();
    }
    let (value, unit) = match elapsed {
        0..=59 => return "just now".to_string(),
        60..=3_599 => (elapsed / 60, "minute"),
        3_600..=86_399 => (elapsed / 3_600, "hour"),
        86_400..=604_799 => (elapsed / 86_400, "day"),
        604_800..=2_591_999 => (elapsed / 604_800, "week"),
        2_592_000..=31_535_999 => (elapsed / 2_592_000, "month"),
        _ => (elapsed / 31_536_000, "year"),
    };
    match value {
        1 => format!("{} {unit} ago", if unit == "hour" { "an" } else { "a" }),
        _ => format!("{value} {unit}s ago"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn describes_relative_time_with_natural_singulars() {
        for (elapsed, expected) in [
            (0, "just now"),
            (30, "just now"),
            (60, "a minute ago"),
            (180, "3 minutes ago"),
            (3_600, "an hour ago"),
            (86_400, "a day ago"),
            (3 * 86_400, "3 days ago"),
            (7 * 86_400, "a week ago"),
            (4 * 7 * 86_400, "4 weeks ago"),
            (30 * 86_400, "a month ago"),
            (365 * 86_400, "a year ago"),
            (2 * 365 * 86_400, "2 years ago"),
        ] {
            assert_eq!(describe(NOW - elapsed, NOW), expected);
        }
        assert_eq!(describe(NOW + 60, NOW), "in the future");
    }
}

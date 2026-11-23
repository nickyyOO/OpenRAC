//! Times and places in a player's words: "Today at 17:01", "on Batalia".

use chrono::{DateTime, Datelike, Local, TimeZone};
use openrac_launcher_service::core::reports::{ReportEntry, ReportKind};

/// Where a session was: "on Batalia", "in level 12", "in the menus".
pub fn place(entry: &ReportEntry) -> String {
    match (&entry.planet, entry.level) {
        (Some(planet), _) => format!("on {planet}"),
        (None, Some(level)) => format!("in level {level}"),
        (None, None) => "in the menus".into(),
    }
}

/// "23 minutes", "1 hour 14 minutes", "less than a minute".
pub fn duration(seconds: u64) -> String {
    if seconds < 60 {
        return "less than a minute".into();
    }
    let minutes = (seconds as f64 / 60.0).round() as u64;
    let part = |n: u64, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
    match (minutes / 60, minutes % 60) {
        (0, m) => part(m, "minute"),
        (h, 0) => part(h, "hour"),
        (h, m) => format!("{} {}", part(h, "hour"), part(m, "minute")),
    }
}

/// What a row says happened: "Crashed on Batalia", "Played 23 minutes on Kerwan".
pub fn headline(entry: &ReportEntry) -> String {
    if entry.kind == ReportKind::Crash {
        return format!("Crashed {}", place(entry));
    }
    let time = entry.duration.map(|d| format!("Played {}", duration(d))).unwrap_or_else(|| "Played".into());
    if entry.crashed {
        format!("{time}, then crashed {}", place(entry))
    } else if entry.level.is_some() || entry.planet.is_some() {
        format!("{time} {}", place(entry))
    } else {
        time
    }
}

/// "Today at 17:01", "Yesterday at 9:05", "14 October at 10:00", with the year when it is not
/// this one. `seconds`: since 1970.
pub fn when(seconds: u64) -> String {
    when_from(seconds, Local::now())
}

fn when_from<Tz: TimeZone>(seconds: u64, now: DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let Some(then) = DateTime::from_timestamp(seconds as i64, 0) else { return String::new() };
    let then = then.with_timezone(&now.timezone());
    let time = then.format("%-H:%M");
    let days = (now.date_naive() - then.date_naive()).num_days();
    match days {
        0 => format!("Today at {time}"),
        1 => format!("Yesterday at {time}"),
        _ if then.year() == now.year() => format!("{} at {time}", then.format("%-d %B")),
        _ => format!("{} at {time}", then.format("%-d %B %Y")),
    }
}

/// "Last played today at 17:01 on Batalia".
pub fn last_played(entry: &ReportEntry) -> String {
    let day = when(entry.time);
    let mut chars = day.chars();
    let first = chars.next().map(|c| c.to_lowercase().to_string()).unwrap_or_default();
    format!("Last played {first}{} {}", chars.as_str(), place(entry))
}

/// A save's time as the game writes it ("2026-10-09 17:01:21", the console's own clock) in words;
/// the game's own text when it cannot be read.
pub fn stamp(stamp: &str) -> String {
    let parsed = chrono::NaiveDateTime::parse_from_str(stamp, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(stamp, "%Y-%m-%d %H:%M"))
        .ok()
        .and_then(|t| Local.from_local_datetime(&t).earliest());
    match parsed {
        Some(t) => when(t.timestamp().max(0) as u64),
        None => stamp.to_string(),
    }
}

pub use openrac_launcher_service::labels::grouped;

/// 1 234 567 bytes -> "1,2 MB".
pub fn size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    // Rounded before the unit is chosen: 999 999 bytes is 1000 KB, which is a megabyte.
    value = if unit == 0 { value.round() } else { (value * 10.0).round() / 10.0 };
    if value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    let text = if unit == 0 {
        grouped(value as u64)
    } else if value < 10.0 {
        format!("{value:.1}").replace('.', ",")
    } else {
        format!("{value:.0}")
    };
    format!("{text} {}", UNITS[unit])
}

/// 12.345 -> "12,35 %": how much of a game's code is matched.
pub fn percent(p: f64) -> String {
    format!("{p:.2} %").replace('.', ",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn days_read_as_a_player_says_them() {
        let now = Utc.with_ymd_and_hms(2026, 11, 23, 18, 0, 0).unwrap();
        let at = |y, mo, d, h, mi| Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap().timestamp() as u64;
        assert_eq!(when_from(at(2026, 11, 23, 17, 1), now), "Today at 17:01");
        assert_eq!(when_from(at(2026, 11, 22, 9, 5), now), "Yesterday at 9:05");
        assert_eq!(when_from(at(2026, 10, 14, 10, 0), now), "14 October at 10:00");
        assert_eq!(when_from(at(2025, 10, 14, 10, 0), now), "14 October 2025 at 10:00");
    }

    #[test]
    fn durations_read_in_words() {
        assert_eq!(duration(30), "less than a minute");
        assert_eq!(duration(23 * 60), "23 minutes");
        assert_eq!(duration(74 * 60), "1 hour 14 minutes");
        assert_eq!(duration(120 * 60), "2 hours");
    }

    #[test]
    fn numbers_and_sizes_read_as_people_write_them() {
        assert_eq!(grouped(2202320), "2\u{202f}202\u{202f}320");
        assert_eq!(grouped(999), "999");
        assert_eq!(size(512), "512 bytes");
        assert_eq!(size(1_234_567), "1,2 MB");
        assert_eq!(size(999_999), "1,0 MB");
        assert_eq!(size(45_000), "45 KB");
    }

    #[test]
    fn percent_has_two_places() {
        assert_eq!(percent(12.345), "12,35 %");
    }
}

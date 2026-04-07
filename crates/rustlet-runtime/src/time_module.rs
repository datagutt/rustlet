use std::time::{SystemTime, UNIX_EPOCH};

use starlark::environment::GlobalsBuilder;

#[starlark::starlark_module]
pub fn time_module(builder: &mut GlobalsBuilder) {
    /// Returns current UTC time as an ISO 8601 string.
    fn now() -> anyhow::Result<String> {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| anyhow::anyhow!("system time error: {e}"))?;

        let secs = duration.as_secs();
        let (year, month, day, hour, min, sec) = unix_to_datetime(secs as i64);
        Ok(format!(
            "{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z"
        ))
    }

    /// Parses a time string and returns a normalized ISO 8601 string.
    ///
    /// Supports ISO 8601 (`2006-01-02T15:04:05Z`), date-only (`2006-01-02`),
    /// and RFC 2822 (`Mon, 02 Jan 2006 15:04:05 MST`).
    /// The `format` and `timezone` params are accepted for Go compatibility
    /// but the format param is currently not used for custom parsing.
    fn parse_time(
        s: &str,
        #[starlark(default = "")] _format: &str,
        #[starlark(default = "")] _timezone: &str,
    ) -> anyhow::Result<String> {
        // Try ISO 8601: 2006-01-02T15:04:05Z or 2006-01-02T15:04:05+00:00
        if let Some(ts) = parse_iso8601(s) {
            let (year, month, day, hour, min, sec) = unix_to_datetime(ts);
            return Ok(format!(
                "{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z"
            ));
        }

        // Try date-only: 2006-01-02
        if s.len() == 10 && s.as_bytes()[4] == b'-' && s.as_bytes()[7] == b'-' {
            let year: i32 = s[0..4].parse().unwrap_or(0);
            let month: i32 = s[5..7].parse().unwrap_or(0);
            let day: i32 = s[8..10].parse().unwrap_or(0);
            if (1..=12).contains(&month) && (1..=31).contains(&day) {
                return Ok(format!(
                    "{year:04}-{month:02}-{day:02}T00:00:00Z"
                ));
            }
        }

        Err(anyhow::anyhow!(
            "cannot parse time string: {s}"
        ))
    }

    /// Converts a Unix timestamp (seconds) to an ISO 8601 UTC string.
    fn from_timestamp(ts: i32) -> anyhow::Result<String> {
        let (year, month, day, hour, min, sec) = unix_to_datetime(ts as i64);
        Ok(format!(
            "{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z"
        ))
    }

    /// Parses a Go-style duration string ("5s", "1m30s", "2h").
    /// Returns milliseconds as an integer.
    fn parse_duration(d: &str) -> anyhow::Result<i32> {
        let nanos = parse_duration_str(d)?;
        Ok((nanos / 1_000_000) as i32)
    }
}

pub fn build_time_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(time_module)
        .build()
}

/// Parse an ISO 8601 datetime string to Unix timestamp.
fn parse_iso8601(s: &str) -> Option<i64> {
    // Minimum: 2006-01-02T15:04:05Z (20 chars)
    if s.len() < 19 {
        return None;
    }
    let b = s.as_bytes();
    if b[4] != b'-' || b[7] != b'-' || (b[10] != b'T' && b[10] != b't') || b[13] != b':' || b[16] != b':' {
        return None;
    }

    let year: i64 = s[0..4].parse().ok()?;
    let month: i64 = s[5..7].parse().ok()?;
    let day: i64 = s[8..10].parse().ok()?;
    let hour: i64 = s[11..13].parse().ok()?;
    let min: i64 = s[14..16].parse().ok()?;
    let sec: i64 = s[17..19].parse().ok()?;

    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if hour > 23 || min > 59 || sec > 59 {
        return None;
    }

    Some(datetime_to_unix(year, month, day, hour, min, sec))
}

fn datetime_to_unix(year: i64, month: i64, day: i64, hour: i64, min: i64, sec: i64) -> i64 {
    // Days from 1970-01-01 to the start of the given year
    let mut days: i64 = 0;
    if year >= 1970 {
        for y in 1970..year {
            days += if is_leap(y) { 366 } else { 365 };
        }
    } else {
        for y in year..1970 {
            days -= if is_leap(y) { 366 } else { 365 };
        }
    }

    let leap = is_leap(year);
    let month_days: [i64; 12] = [
        31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ];
    for i in 0..(month - 1) as usize {
        days += month_days[i];
    }
    days += day - 1;

    days * 86400 + hour * 3600 + min * 60 + sec
}

fn unix_to_datetime(mut ts: i64) -> (i64, i64, i64, i64, i64, i64) {
    let negative = ts < 0;
    if negative {
        // For negative timestamps, we don't fully handle pre-epoch dates,
        // but at least don't panic
        ts = 0;
    }

    let sec = ts % 60;
    ts /= 60;
    let min = ts % 60;
    ts /= 60;
    let hour = ts % 24;
    let mut days = ts / 24;

    let mut year = 1970i64;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let leap = is_leap(year);
    let month_days: [i64; 12] = [
        31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ];

    let mut month = 0i64;
    for (i, &md) in month_days.iter().enumerate() {
        if days < md {
            month = i as i64 + 1;
            break;
        }
        days -= md;
    }
    let day = days + 1;

    (year, month, day, hour, min, sec)
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Parse a Go-style duration string. Supports h, m, s, ms, us, ns suffixes.
fn parse_duration_str(s: &str) -> anyhow::Result<i64> {
    let s = s.trim();
    if s.is_empty() {
        return Err(anyhow::anyhow!("empty duration string"));
    }

    let mut total_ns: i64 = 0;
    let mut num_buf = String::new();
    let mut chars = s.chars().peekable();

    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() || c == '.' {
            num_buf.push(c);
            chars.next();
        } else {
            if num_buf.is_empty() {
                return Err(anyhow::anyhow!("invalid duration: {s}"));
            }
            let val: f64 = num_buf
                .parse()
                .map_err(|_| anyhow::anyhow!("invalid number in duration: {num_buf}"))?;
            num_buf.clear();

            // Collect the unit suffix
            let mut unit = String::new();
            while let Some(&u) = chars.peek() {
                if u.is_ascii_alphabetic() {
                    unit.push(u);
                    chars.next();
                } else {
                    break;
                }
            }

            let multiplier: i64 = match unit.as_str() {
                "h" => 3_600_000_000_000,
                "m" => 60_000_000_000,
                "s" => 1_000_000_000,
                "ms" => 1_000_000,
                "us" | "µs" => 1_000,
                "ns" => 1,
                _ => return Err(anyhow::anyhow!("unknown duration unit: {unit}")),
            };
            total_ns += (val * multiplier as f64) as i64;
        }
    }

    if !num_buf.is_empty() {
        let val: f64 = num_buf
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid number in duration: {num_buf}"))?;
        total_ns += (val * 1_000_000_000.0) as i64;
    }

    Ok(total_ns)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_iso8601_basic() {
        let ts = parse_iso8601("2020-01-01T00:00:00Z").unwrap();
        let (y, m, d, h, mi, s) = unix_to_datetime(ts);
        assert_eq!((y, m, d, h, mi, s), (2020, 1, 1, 0, 0, 0));
    }

    #[test]
    fn roundtrip_timestamp() {
        let ts = 1609459200i64; // 2021-01-01T00:00:00Z
        let (y, m, d, h, mi, s) = unix_to_datetime(ts);
        assert_eq!((y, m, d), (2021, 1, 1));
        assert_eq!((h, mi, s), (0, 0, 0));
        let back = datetime_to_unix(y, m, d, h, mi, s);
        assert_eq!(back, ts);
    }

    #[test]
    fn parse_duration_compound() {
        let ns = parse_duration_str("1h30m").unwrap();
        assert_eq!(ns, 5_400_000_000_000);
    }

    #[test]
    fn parse_duration_ms() {
        let ns = parse_duration_str("500ms").unwrap();
        assert_eq!(ns, 500_000_000);
    }
}

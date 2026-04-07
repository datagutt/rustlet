use std::time::{SystemTime, UNIX_EPOCH};

use starlark::environment::GlobalsBuilder;
use starlark::values::float::StarlarkFloat;
use starlark::values::list::ListRef;
use starlark::values::none::NoneType;
use starlark::values::{Value, ValueLike};

fn to_f64(v: Value) -> anyhow::Result<f64> {
    if let Some(f) = v.downcast_ref::<StarlarkFloat>() {
        return Ok(f.0);
    }
    if let Some(i) = v.unpack_i32() {
        return Ok(i as f64);
    }
    Err(anyhow::anyhow!("expected number, got {}", v.get_type()))
}

fn current_unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn format_relative_time(seconds: i64) -> String {
    let abs = seconds.unsigned_abs();

    let (value, unit) = if abs < 60 {
        return "now".to_string();
    } else if abs < 3600 {
        (abs / 60, "minute")
    } else if abs < 86400 {
        (abs / 3600, "hour")
    } else if abs < 86400 * 30 {
        (abs / 86400, "day")
    } else if abs < 86400 * 365 {
        (abs / (86400 * 30), "month")
    } else {
        (abs / (86400 * 365), "year")
    };

    let plural = if value == 1 { "" } else { "s" };
    if seconds > 0 {
        format!("{value} {unit}{plural} from now")
    } else {
        format!("{value} {unit}{plural} ago")
    }
}

fn format_bytes_si(size: f64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB", "PB", "EB"];
    let base = 1000.0_f64;
    if size < base {
        return format!("{} B", size as i64);
    }
    let exp = (size.ln() / base.ln()).floor() as usize;
    let exp = exp.min(units.len() - 1);
    let val = size / base.powi(exp as i32);
    if val == val.floor() {
        format!("{} {}", val as i64, units[exp])
    } else {
        format!("{:.1} {}", val, units[exp])
    }
}

fn format_bytes_iec(size: f64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    let base = 1024.0_f64;
    if size < base {
        return format!("{} B", size as i64);
    }
    let exp = (size.ln() / base.ln()).floor() as usize;
    let exp = exp.min(units.len() - 1);
    let val = size / base.powi(exp as i32);
    if val == val.floor() {
        format!("{} {}", val as i64, units[exp])
    } else {
        format!("{:.1} {}", val, units[exp])
    }
}

fn insert_commas(s: &str) -> String {
    let bytes = s.as_bytes();
    let len = bytes.len();
    if len <= 3 {
        return s.to_string();
    }
    let mut result = String::with_capacity(len + len / 3);
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push(',');
        }
        result.push(b as char);
    }
    result
}

fn ordinal_suffix(n: i64) -> &'static str {
    let abs = n.unsigned_abs();
    let last_two = abs % 100;
    if (11..=13).contains(&last_two) {
        return "th";
    }
    match abs % 10 {
        1 => "st",
        2 => "nd",
        3 => "rd",
        _ => "th",
    }
}

fn parse_byte_string(s: &str) -> anyhow::Result<i64> {
    let s = s.trim();
    if s.is_empty() {
        return Err(anyhow::anyhow!("empty byte string"));
    }

    // Find where the numeric part ends
    let mut num_end = 0;
    for (i, c) in s.char_indices() {
        if c.is_ascii_digit() || c == '.' || c == '-' {
            num_end = i + c.len_utf8();
        } else {
            break;
        }
    }

    let num_str = s[..num_end].trim();
    let unit_str = s[num_end..].trim();

    let num: f64 = num_str
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid number in byte string: {s}"))?;

    let multiplier: f64 = match unit_str.to_uppercase().as_str() {
        "" | "B" => 1.0,
        "KB" => 1_000.0,
        "MB" => 1_000_000.0,
        "GB" => 1_000_000_000.0,
        "TB" => 1_000_000_000_000.0,
        "PB" => 1_000_000_000_000_000.0,
        "EB" => 1_000_000_000_000_000_000.0,
        "KIB" => 1_024.0,
        "MIB" => 1_048_576.0,
        "GIB" => 1_073_741_824.0,
        "TIB" => 1_099_511_627_776.0,
        "PIB" => 1_125_899_906_842_624.0,
        "EIB" => 1_152_921_504_606_846_976.0,
        _ => return Err(anyhow::anyhow!("unknown byte unit: {unit_str}")),
    };

    Ok((num * multiplier) as i64)
}

/// Percent-encode a string for use in URLs (application/x-www-form-urlencoded).
/// Spaces become '+', unreserved chars are left as-is, everything else is %XX.
fn url_encode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b' ' => result.push('+'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            _ => {
                result.push('%');
                result.push(HEX_UPPER[(b >> 4) as usize] as char);
                result.push(HEX_UPPER[(b & 0x0F) as usize] as char);
            }
        }
    }
    result
}

const HEX_UPPER: &[u8; 16] = b"0123456789ABCDEF";

fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'A'..=b'F' => Some(c - b'A' + 10),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

fn url_decode(s: &str) -> anyhow::Result<String> {
    let bytes = s.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                result.push(b' ');
                i += 1;
            }
            b'%' => {
                if i + 2 >= bytes.len() {
                    return Err(anyhow::anyhow!("incomplete percent-encoding in: {s}"));
                }
                let high = hex_digit(bytes[i + 1])
                    .ok_or_else(|| anyhow::anyhow!("invalid hex digit in: {s}"))?;
                let low = hex_digit(bytes[i + 2])
                    .ok_or_else(|| anyhow::anyhow!("invalid hex digit in: {s}"))?;
                result.push((high << 4) | low);
                i += 3;
            }
            b => {
                result.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(result).map_err(|e| anyhow::anyhow!("invalid UTF-8 after decoding: {e}"))
}

/// Convert Java SimpleDateFormat patterns to Go-style reference time layout,
/// then format a unix timestamp with it.
fn time_format_impl(format: &str, ts: i64) -> String {
    let (year, month, day, hour, min, sec) = unix_to_datetime(ts);
    let weekday = day_of_week_from_ts(ts);
    let weekday_short = match weekday {
        0 => "Sun",
        1 => "Mon",
        2 => "Tue",
        3 => "Wed",
        4 => "Thu",
        5 => "Fri",
        _ => "Sat",
    };
    let weekday_long = match weekday {
        0 => "Sunday",
        1 => "Monday",
        2 => "Tuesday",
        3 => "Wednesday",
        4 => "Thursday",
        5 => "Friday",
        _ => "Saturday",
    };
    let month_short = match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        _ => "Dec",
    };
    let month_long = match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        _ => "December",
    };

    let hour12 = if hour == 0 {
        12
    } else if hour > 12 {
        hour - 12
    } else {
        hour
    };
    let am_pm = if hour < 12 { "AM" } else { "PM" };

    // Process the format string, consuming repeated pattern letters
    let chars: Vec<char> = format.chars().collect();
    let len = chars.len();
    let mut result = String::with_capacity(format.len() * 2);
    let mut i = 0;

    while i < len {
        let c = chars[i];
        if c.is_ascii_alphabetic() {
            let start = i;
            while i < len && chars[i] == c {
                i += 1;
            }
            let count = i - start;
            match c {
                'y' => {
                    if count <= 2 {
                        result.push_str(&format!("{:02}", year % 100));
                    } else {
                        result.push_str(&format!("{:04}", year));
                    }
                }
                'M' => match count {
                    1 => result.push_str(&format!("{}", month)),
                    2 => result.push_str(&format!("{:02}", month)),
                    3 => result.push_str(month_short),
                    _ => result.push_str(month_long),
                },
                'd' => {
                    if count == 1 {
                        result.push_str(&format!("{}", day));
                    } else {
                        result.push_str(&format!("{:02}", day));
                    }
                }
                'H' => {
                    if count == 1 {
                        result.push_str(&format!("{}", hour));
                    } else {
                        result.push_str(&format!("{:02}", hour));
                    }
                }
                'h' => {
                    if count == 1 {
                        result.push_str(&format!("{}", hour12));
                    } else {
                        result.push_str(&format!("{:02}", hour12));
                    }
                }
                'm' => {
                    if count == 1 {
                        result.push_str(&format!("{}", min));
                    } else {
                        result.push_str(&format!("{:02}", min));
                    }
                }
                's' => {
                    if count == 1 {
                        result.push_str(&format!("{}", sec));
                    } else {
                        result.push_str(&format!("{:02}", sec));
                    }
                }
                'a' => result.push_str(am_pm),
                'E' => {
                    if count <= 3 {
                        result.push_str(weekday_short);
                    } else {
                        result.push_str(weekday_long);
                    }
                }
                _ => {
                    // Unknown pattern letter, emit as-is
                    for _ in 0..count {
                        result.push(c);
                    }
                }
            }
        } else if c == '\'' {
            // Quoted literal text
            i += 1;
            while i < len && chars[i] != '\'' {
                result.push(chars[i]);
                i += 1;
            }
            if i < len {
                i += 1; // skip closing quote
            }
        } else {
            result.push(c);
            i += 1;
        }
    }

    result
}

fn unix_to_datetime(mut ts: i64) -> (i64, i64, i64, i64, i64, i64) {
    if ts < 0 {
        ts = 0;
    }

    let sec = ts % 60;
    ts /= 60;
    let min = ts % 60;
    ts /= 60;
    let hour = ts % 24;
    let mut days = ts / 24;

    let mut year = 1970_i64;
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
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];

    let mut month = 0_i64;
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

/// Day of week from unix timestamp: 0=Sunday, 1=Monday, ..., 6=Saturday.
/// Jan 1, 1970 was a Thursday (day 4).
fn day_of_week_from_ts(ts: i64) -> i32 {
    let days = if ts >= 0 {
        ts / 86400
    } else {
        // floor division for negative timestamps
        (ts - 86399) / 86400
    };
    ((days % 7 + 4 + 7) % 7) as i32
}

/// Format a float with a pattern like "#,###.##".
/// '#' positions before the decimal point control grouping,
/// '#' positions after the decimal point control decimal places.
fn format_float_pattern(pattern: &str, num: f64) -> String {
    let negative = num < 0.0;
    let num = num.abs();

    let (int_pattern, frac_digits) = if let Some(dot_pos) = pattern.rfind('.') {
        let frac_part = &pattern[dot_pos + 1..];
        let frac_digits = frac_part.chars().filter(|&c| c == '#' || c == '0').count();
        (&pattern[..dot_pos], frac_digits)
    } else {
        (pattern, 0_usize)
    };

    let use_grouping = int_pattern.contains(',');

    let rounded = if frac_digits > 0 {
        let factor = 10_f64.powi(frac_digits as i32);
        (num * factor).round() / factor
    } else {
        num.round()
    };

    let int_part = rounded.trunc() as u64;
    let int_str = int_part.to_string();

    let int_formatted = if use_grouping {
        insert_commas(&int_str)
    } else {
        int_str
    };

    let mut result = String::new();
    if negative {
        result.push('-');
    }
    result.push_str(&int_formatted);

    if frac_digits > 0 {
        let frac = rounded - rounded.trunc();
        let frac_val = (frac * 10_f64.powi(frac_digits as i32)).round() as u64;
        result.push('.');
        let frac_str = format!("{:0>width$}", frac_val, width = frac_digits);
        result.push_str(&frac_str);
    }

    result
}

#[starlark::starlark_module]
pub fn humanize_module(builder: &mut GlobalsBuilder) {
    fn time(timestamp: i32) -> anyhow::Result<String> {
        let now = current_unix_timestamp();
        let diff = (timestamp as i64) - now;
        Ok(format_relative_time(diff))
    }

    fn relative_time(
        ts_a: i32,
        ts_b: i32,
        #[starlark(default = "")] label_a: &str,
        #[starlark(default = "")] label_b: &str,
    ) -> anyhow::Result<String> {
        let diff = (ts_b as i64) - (ts_a as i64);
        if diff == 0 {
            return Ok("now".to_string());
        }

        let abs = diff.unsigned_abs();
        let (value, unit) = if abs < 60 {
            return if diff > 0 {
                if label_b.is_empty() {
                    Ok("now".to_string())
                } else {
                    Ok(label_b.to_string())
                }
            } else if label_a.is_empty() {
                Ok("now".to_string())
            } else {
                Ok(label_a.to_string())
            };
        } else if abs < 3600 {
            (abs / 60, "minute")
        } else if abs < 86400 {
            (abs / 3600, "hour")
        } else if abs < 86400 * 30 {
            (abs / 86400, "day")
        } else if abs < 86400 * 365 {
            (abs / (86400 * 30), "month")
        } else {
            (abs / (86400 * 365), "year")
        };

        let plural = if value == 1 { "" } else { "s" };
        if diff > 0 {
            if label_b.is_empty() {
                Ok(format!("{value} {unit}{plural} from now"))
            } else {
                Ok(format!("{value} {unit}{plural} {label_b}"))
            }
        } else if label_a.is_empty() {
            Ok(format!("{value} {unit}{plural} ago"))
        } else {
            Ok(format!("{value} {unit}{plural} {label_a}"))
        }
    }

    fn time_format<'v>(
        format: &str,
        #[starlark(default = NoneType)] timestamp: Value<'v>,
    ) -> anyhow::Result<String> {
        let ts = if timestamp.is_none() {
            current_unix_timestamp()
        } else if let Some(i) = timestamp.unpack_i32() {
            i as i64
        } else {
            return Err(anyhow::anyhow!(
                "expected int or None for timestamp, got {}",
                timestamp.get_type()
            ));
        };

        Ok(time_format_impl(format, ts))
    }

    fn day_of_week(timestamp: i32) -> anyhow::Result<i32> {
        Ok(day_of_week_from_ts(timestamp as i64))
    }

    fn bytes<'v>(
        size: Value<'v>,
        #[starlark(default = false)] iec: bool,
    ) -> anyhow::Result<String> {
        let n = to_f64(size)?;
        if iec {
            Ok(format_bytes_iec(n))
        } else {
            Ok(format_bytes_si(n))
        }
    }

    fn parse_bytes(s: &str) -> anyhow::Result<i32> {
        let val = parse_byte_string(s)?;
        Ok(val as i32)
    }

    fn comma<'v>(num: Value<'v>) -> anyhow::Result<String> {
        if let Some(i) = num.unpack_i32() {
            let negative = i < 0;
            let s = insert_commas(&i.unsigned_abs().to_string());
            return Ok(if negative { format!("-{s}") } else { s });
        }
        if let Some(f) = num.downcast_ref::<StarlarkFloat>() {
            let val = f.0;
            let negative = val < 0.0;
            let abs = val.abs();
            let int_part = abs.trunc() as u64;
            let frac = abs - abs.trunc();

            let int_str = insert_commas(&int_part.to_string());
            let frac_str = if frac == 0.0 {
                String::new()
            } else {
                // Format the fractional part, trimming trailing zeros
                let raw = format!("{:.10}", frac);
                let raw = raw.trim_start_matches('0').trim_end_matches('0');
                if raw == "." {
                    String::new()
                } else {
                    raw.to_string()
                }
            };

            let sign = if negative { "-" } else { "" };
            return Ok(format!("{sign}{int_str}{frac_str}"));
        }
        Err(anyhow::anyhow!("expected number, got {}", num.get_type()))
    }

    fn float<'v>(format: &str, num: Value<'v>) -> anyhow::Result<String> {
        let n = to_f64(num)?;
        Ok(format_float_pattern(format, n))
    }

    fn int<'v>(format: &str, num: Value<'v>) -> anyhow::Result<String> {
        let n = to_f64(num)?;
        Ok(format_float_pattern(format, n.trunc()))
    }

    fn ordinal<'v>(num: Value<'v>) -> anyhow::Result<String> {
        let n = if let Some(i) = num.unpack_i32() {
            i as i64
        } else if let Some(f) = num.downcast_ref::<StarlarkFloat>() {
            f.0 as i64
        } else {
            return Err(anyhow::anyhow!("expected number, got {}", num.get_type()));
        };
        let suffix = ordinal_suffix(n);
        Ok(format!("{n}{suffix}"))
    }

    fn ftoa<'v>(num: Value<'v>, #[starlark(default = -1)] digits: i32) -> anyhow::Result<String> {
        let n = to_f64(num)?;
        if digits < 0 {
            Ok(format!("{n}"))
        } else {
            Ok(format!("{n:.prec$}", prec = digits as usize))
        }
    }

    fn plural(
        quantity: i32,
        singular: &str,
        #[starlark(default = "")] plural: &str,
    ) -> anyhow::Result<String> {
        let word = if quantity == 1 {
            singular.to_string()
        } else if plural.is_empty() {
            format!("{singular}s")
        } else {
            plural.to_string()
        };
        Ok(format!("{quantity} {word}"))
    }

    fn plural_word(
        quantity: i32,
        singular: &str,
        #[starlark(default = "")] plural: &str,
    ) -> anyhow::Result<String> {
        if quantity == 1 {
            Ok(singular.to_string())
        } else if plural.is_empty() {
            Ok(format!("{singular}s"))
        } else {
            Ok(plural.to_string())
        }
    }

    fn word_series<'v>(words: Value<'v>, conjunction: &str) -> anyhow::Result<String> {
        let list = ListRef::from_value(words)
            .ok_or_else(|| anyhow::anyhow!("expected list, got {}", words.get_type()))?;
        let items: Vec<String> = list
            .iter()
            .map(|v| {
                v.unpack_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| v.to_string())
            })
            .collect();

        Ok(match items.len() {
            0 => String::new(),
            1 => items[0].clone(),
            2 => format!("{} {} {}", items[0], conjunction, items[1]),
            _ => {
                let (last, rest) = items.split_last().unwrap();
                format!("{} {} {}", rest.join(", "), conjunction, last)
            }
        })
    }

    fn oxford_word_series<'v>(words: Value<'v>, conjunction: &str) -> anyhow::Result<String> {
        let list = ListRef::from_value(words)
            .ok_or_else(|| anyhow::anyhow!("expected list, got {}", words.get_type()))?;
        let items: Vec<String> = list
            .iter()
            .map(|v| {
                v.unpack_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| v.to_string())
            })
            .collect();

        Ok(match items.len() {
            0 => String::new(),
            1 => items[0].clone(),
            2 => format!("{} {} {}", items[0], conjunction, items[1]),
            _ => {
                let (last, rest) = items.split_last().unwrap();
                format!("{}, {} {}", rest.join(", "), conjunction, last)
            }
        })
    }

    fn url_encode(s: &str) -> anyhow::Result<String> {
        Ok(crate::humanize_module::url_encode(s))
    }

    fn url_decode(s: &str) -> anyhow::Result<String> {
        crate::humanize_module::url_decode(s)
    }
}

pub fn build_humanize_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(humanize_module)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ordinal_suffix() {
        assert_eq!(ordinal_suffix(1), "st");
        assert_eq!(ordinal_suffix(2), "nd");
        assert_eq!(ordinal_suffix(3), "rd");
        assert_eq!(ordinal_suffix(4), "th");
        assert_eq!(ordinal_suffix(11), "th");
        assert_eq!(ordinal_suffix(12), "th");
        assert_eq!(ordinal_suffix(13), "th");
        assert_eq!(ordinal_suffix(21), "st");
        assert_eq!(ordinal_suffix(22), "nd");
        assert_eq!(ordinal_suffix(23), "rd");
        assert_eq!(ordinal_suffix(111), "th");
        assert_eq!(ordinal_suffix(112), "th");
        assert_eq!(ordinal_suffix(113), "th");
    }

    #[test]
    fn test_insert_commas() {
        assert_eq!(insert_commas("1"), "1");
        assert_eq!(insert_commas("12"), "12");
        assert_eq!(insert_commas("123"), "123");
        assert_eq!(insert_commas("1234"), "1,234");
        assert_eq!(insert_commas("123456"), "123,456");
        assert_eq!(insert_commas("1234567"), "1,234,567");
    }

    #[test]
    fn test_format_bytes_si() {
        assert_eq!(format_bytes_si(0.0), "0 B");
        assert_eq!(format_bytes_si(500.0), "500 B");
        assert_eq!(format_bytes_si(1000.0), "1 KB");
        assert_eq!(format_bytes_si(1400000000.0), "1.4 GB");
    }

    #[test]
    fn test_format_bytes_iec() {
        assert_eq!(format_bytes_iec(0.0), "0 B");
        assert_eq!(format_bytes_iec(1024.0), "1 KiB");
        assert_eq!(format_bytes_iec(1073741824.0), "1 GiB");
    }

    #[test]
    fn test_parse_byte_string() {
        assert_eq!(parse_byte_string("42 MB").unwrap(), 42_000_000);
        assert_eq!(parse_byte_string("1 GiB").unwrap(), 1_073_741_824);
        assert_eq!(parse_byte_string("100 B").unwrap(), 100);
        assert_eq!(parse_byte_string("1KB").unwrap(), 1000);
    }

    #[test]
    fn test_url_encode_decode() {
        assert_eq!(url_encode("hello world"), "hello+world");
        assert_eq!(url_encode("foo=bar&baz=qux"), "foo%3Dbar%26baz%3Dqux");
        assert_eq!(url_decode("hello+world").unwrap(), "hello world");
        assert_eq!(
            url_decode("foo%3Dbar%26baz%3Dqux").unwrap(),
            "foo=bar&baz=qux"
        );
    }

    #[test]
    fn test_day_of_week() {
        // Jan 1, 1970 was Thursday = 4
        assert_eq!(day_of_week_from_ts(0), 4);
        // Jan 4, 1970 was Sunday = 0
        assert_eq!(day_of_week_from_ts(3 * 86400), 0);
    }

    #[test]
    fn test_format_relative_time() {
        assert_eq!(format_relative_time(0), "now");
        assert_eq!(format_relative_time(30), "now");
        assert_eq!(format_relative_time(-30), "now");
        assert_eq!(format_relative_time(-3600), "1 hour ago");
        assert_eq!(format_relative_time(-7200), "2 hours ago");
        assert_eq!(format_relative_time(86400), "1 day from now");
        assert_eq!(format_relative_time(-172800), "2 days ago");
    }

    #[test]
    fn test_time_format() {
        // 2021-01-01T00:00:00Z = 1609459200
        let ts = 1609459200_i64;
        assert_eq!(time_format_impl("yyyy-MM-dd", ts), "2021-01-01");
        assert_eq!(time_format_impl("HH:mm:ss", ts), "00:00:00");
        assert_eq!(time_format_impl("yyyy", ts), "2021");
        assert_eq!(time_format_impl("MM/dd/yyyy", ts), "01/01/2021");
    }

    #[test]
    fn test_format_float_pattern() {
        assert_eq!(format_float_pattern("#,###.##", 1234.56), "1,234.56");
        assert_eq!(format_float_pattern("#,###.##", 1234567.89), "1,234,567.89");
        assert_eq!(format_float_pattern("#.##", 3.14159), "3.14");
        assert_eq!(format_float_pattern("#,###.", 1234567.0), "1,234,567");
    }
}

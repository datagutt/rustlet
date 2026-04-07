use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::values::Value;

use crate::starlark_time::{StarlarkTime, parse_iso8601, datetime_to_unix};

#[starlark::starlark_module]
pub fn time_module(builder: &mut GlobalsBuilder) {
    fn now<'v>(eval: &mut Evaluator<'v, '_, '_>) -> anyhow::Result<Value<'v>> {
        Ok(eval.heap().alloc(StarlarkTime::now()))
    }

    fn parse_time<'v>(
        s: &str,
        #[starlark(default = "")] _format: &str,
        #[starlark(default = "")] _timezone: &str,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        // Try ISO 8601: 2006-01-02T15:04:05Z
        if let Some(ts) = parse_iso8601(s) {
            return Ok(eval.heap().alloc(StarlarkTime::from_unix(ts, 0)));
        }

        // Try date-only: 2006-01-02
        if s.len() == 10 && s.as_bytes()[4] == b'-' && s.as_bytes()[7] == b'-' {
            let year: i64 = s[0..4].parse().unwrap_or(0);
            let month: i64 = s[5..7].parse().unwrap_or(0);
            let day: i64 = s[8..10].parse().unwrap_or(0);
            if (1..=12).contains(&month) && (1..=31).contains(&day) {
                let ts = datetime_to_unix(year, month, day, 0, 0, 0);
                return Ok(eval.heap().alloc(StarlarkTime::from_unix(ts, 0)));
            }
        }

        Err(anyhow::anyhow!("cannot parse time string: {s}"))
    }

    fn from_timestamp<'v>(
        ts: i32,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        Ok(eval.heap().alloc(StarlarkTime::from_unix(ts as i64, 0)))
    }

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
    use crate::starlark_time::{parse_iso8601, unix_to_datetime, datetime_to_unix};

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

    #[test]
    fn time_format_go() {
        let t = StarlarkTime::from_unix(1609459200, 0); // 2021-01-01T00:00:00Z
        assert_eq!(t.format_go("2006-01-02"), "2021-01-01");
    }

    #[test]
    fn time_display() {
        let t = StarlarkTime::from_unix(0, 0);
        assert_eq!(t.to_string(), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn time_components() {
        let t = StarlarkTime::from_unix(1609459200, 0);
        let (y, m, d, h, mi, s) = t.components();
        assert_eq!((y, m, d, h, mi, s), (2021, 1, 1, 0, 0, 0));
    }
}

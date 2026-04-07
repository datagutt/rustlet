use std::fmt;

use allocative::Allocative;
use starlark::environment::{Methods, MethodsBuilder, MethodsStatic};
use starlark::starlark_simple_value;
use starlark::values::{
    Heap, NoSerialize, ProvidesStaticType, StarlarkValue, Value, ValueLike,
};
use starlark_derive::starlark_value;

#[derive(Debug, Clone, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkTime {
    pub unix_secs: i64,
    pub unix_nanos: i64,
    pub utc_offset_secs: i32,
    #[allocative(skip)]
    pub tz_name: String,
}

starlark_simple_value!(StarlarkTime);

impl StarlarkTime {
    pub fn from_unix(secs: i64, nanos: i64) -> Self {
        Self {
            unix_secs: secs,
            unix_nanos: nanos,
            utc_offset_secs: 0,
            tz_name: "UTC".to_string(),
        }
    }

    pub fn now() -> Self {
        let duration = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        Self::from_unix(duration.as_secs() as i64, duration.subsec_nanos() as i64)
    }

    fn local_secs(&self) -> i64 {
        self.unix_secs + self.utc_offset_secs as i64
    }

    pub fn components(&self) -> (i64, i64, i64, i64, i64, i64) {
        unix_to_datetime(self.local_secs())
    }

    pub fn format_go(&self, layout: &str) -> String {
        let (year, month, day, hour, min, sec) = self.components();
        let hour12 = if hour == 0 { 12 } else if hour > 12 { hour - 12 } else { hour };
        let ampm = if hour < 12 { "AM" } else { "PM" };
        let ampm_lower = if hour < 12 { "am" } else { "pm" };

        let wday = weekday(self.local_secs());
        let day_names = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
        let day_abbrevs = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        let month_names = ["January", "February", "March", "April", "May", "June",
                           "July", "August", "September", "October", "November", "December"];
        let month_abbrevs = ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
                             "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

        let offset_h = self.utc_offset_secs.abs() / 3600;
        let offset_m = (self.utc_offset_secs.abs() % 3600) / 60;
        let offset_sign = if self.utc_offset_secs >= 0 { "+" } else { "-" };

        // Token-based parsing to avoid replacement collisions.
        // Go reference time: Mon Jan 2 15:04:05 MST 2006
        // Tokens sorted longest-first to match greedily.
        let tokens: &[(&str, String)] = &[
            ("2006", format!("{year:04}")),
            ("06", format!("{:02}", year % 100)),
            ("January", month_names[(month - 1) as usize].to_string()),
            ("Jan", month_abbrevs[(month - 1) as usize].to_string()),
            ("Monday", day_names[wday as usize].to_string()),
            ("Mon", day_abbrevs[wday as usize].to_string()),
            ("MST", self.tz_name.clone()),
            ("-0700", format!("{offset_sign}{offset_h:02}{offset_m:02}")),
            ("-07:00", format!("{offset_sign}{offset_h:02}:{offset_m:02}")),
            ("-07", format!("{offset_sign}{offset_h:02}")),
            ("15", format!("{hour:02}")),
            ("PM", ampm.to_string()),
            ("pm", ampm_lower.to_string()),
            ("05", format!("{sec:02}")),
            ("04", format!("{min:02}")),
            ("03", format!("{hour12:02}")),
            ("02", format!("{day:02}")),
            ("01", format!("{month:02}")),
            ("5", format!("{sec}")),
            ("4", format!("{min}")),
            ("3", format!("{hour12}")),
            ("2", format!("{day}")),
            ("1", format!("{month}")),
        ];

        let bytes = layout.as_bytes();
        let mut result = String::with_capacity(layout.len() + 16);
        let mut i = 0;
        while i < bytes.len() {
            let mut matched = false;
            for (pat, replacement) in tokens {
                if bytes[i..].starts_with(pat.as_bytes()) {
                    result.push_str(replacement);
                    i += pat.len();
                    matched = true;
                    break;
                }
            }
            if !matched {
                result.push(bytes[i] as char);
                i += 1;
            }
        }
        result
    }
}

impl fmt::Display for StarlarkTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (year, month, day, hour, min, sec) = self.components();
        if self.utc_offset_secs == 0 {
            write!(f, "{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z")
        } else {
            let sign = if self.utc_offset_secs >= 0 { '+' } else { '-' };
            let oh = self.utc_offset_secs.unsigned_abs() / 3600;
            let om = (self.utc_offset_secs.unsigned_abs() % 3600) / 60;
            write!(f, "{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}{sign}{oh:02}:{om:02}")
        }
    }
}

#[starlark_value(type = "Time")]
impl<'v> StarlarkValue<'v> for StarlarkTime {
    fn get_methods() -> Option<&'static Methods> {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(time_methods)
    }

    fn has_attr(&self, attribute: &str, _heap: &'v Heap) -> bool {
        matches!(attribute, "year" | "month" | "day" | "hour" | "minute" | "second"
            | "nanosecond" | "unix" | "unix_nano")
    }

    fn dir_attr(&self) -> Vec<String> {
        vec![
            "year".into(), "month".into(), "day".into(),
            "hour".into(), "minute".into(), "second".into(),
            "nanosecond".into(), "unix".into(), "unix_nano".into(),
        ]
    }

    fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
        let (year, month, day, hour, min, sec) = self.components();
        match attribute {
            "year" => Some(heap.alloc(year as i32)),
            "month" => Some(heap.alloc(month as i32)),
            "day" => Some(heap.alloc(day as i32)),
            "hour" => Some(heap.alloc(hour as i32)),
            "minute" => Some(heap.alloc(min as i32)),
            "second" => Some(heap.alloc(sec as i32)),
            "nanosecond" => Some(heap.alloc(self.unix_nanos as i32)),
            "unix" => Some(heap.alloc(self.unix_secs as i32)),
            "unix_nano" => Some(heap.alloc(self.unix_secs * 1_000_000_000 + self.unix_nanos)),
            _ => None,
        }
    }

    fn compare(&self, other: Value<'v>) -> starlark::Result<std::cmp::Ordering> {
        match other.downcast_ref::<StarlarkTime>() {
            Some(o) => {
                let a = (self.unix_secs, self.unix_nanos);
                let b = (o.unix_secs, o.unix_nanos);
                Ok(a.cmp(&b))
            }
            None => Err(starlark::Error::new_other(
                anyhow::anyhow!("cannot compare Time with {}", other.get_type()),
            )),
        }
    }

    fn equals(&self, other: Value<'v>) -> starlark::Result<bool> {
        match other.downcast_ref::<StarlarkTime>() {
            Some(o) => Ok(self.unix_secs == o.unix_secs && self.unix_nanos == o.unix_nanos),
            None => Ok(false),
        }
    }
}

#[starlark::starlark_module]
fn time_methods(builder: &mut MethodsBuilder) {
    fn format<'v>(#[starlark(this)] this: Value<'v>, layout: &str) -> anyhow::Result<String> {
        let t = this
            .downcast_ref::<StarlarkTime>()
            .ok_or_else(|| anyhow::anyhow!("expected Time"))?;
        Ok(t.format_go(layout))
    }

    fn in_location<'v>(
        #[starlark(this)] this: Value<'v>,
        location: &str,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let t = this
            .downcast_ref::<StarlarkTime>()
            .ok_or_else(|| anyhow::anyhow!("expected Time"))?;

        let offset = tz_offset(location)?;
        let new_t = StarlarkTime {
            unix_secs: t.unix_secs,
            unix_nanos: t.unix_nanos,
            utc_offset_secs: offset,
            tz_name: location.to_string(),
        };
        Ok(eval.heap().alloc(new_t))
    }
}

// Simplified timezone offset lookup for common IANA zones.
// Returns UTC offset in seconds.
fn tz_offset(name: &str) -> anyhow::Result<i32> {
    let offset_hours: f32 = match name {
        "UTC" | "GMT" | "Etc/UTC" | "Etc/GMT" => 0.0,
        // US
        "America/New_York" | "US/Eastern" | "EST" => -5.0,
        "America/Chicago" | "US/Central" | "CST" => -6.0,
        "America/Denver" | "US/Mountain" | "MST" => -7.0,
        "America/Los_Angeles" | "US/Pacific" | "PST" => -8.0,
        "America/Anchorage" | "US/Alaska" => -9.0,
        "Pacific/Honolulu" | "US/Hawaii" => -10.0,
        // Europe
        "Europe/London" | "Europe/Dublin" | "Europe/Lisbon" => 0.0,
        "Europe/Paris" | "Europe/Berlin" | "Europe/Rome" | "Europe/Madrid"
        | "Europe/Amsterdam" | "Europe/Brussels" | "Europe/Vienna"
        | "Europe/Zurich" | "Europe/Stockholm" | "Europe/Oslo"
        | "Europe/Copenhagen" | "Europe/Warsaw" | "Europe/Prague"
        | "Europe/Budapest" | "CET" => 1.0,
        "Europe/Helsinki" | "Europe/Athens" | "Europe/Bucharest"
        | "Europe/Istanbul" | "Europe/Kiev" | "Europe/Kyiv" | "EET" => 2.0,
        "Europe/Moscow" | "Europe/Minsk" => 3.0,
        // Asia
        "Asia/Dubai" => 4.0,
        "Asia/Kolkata" | "Asia/Calcutta" => 5.5,
        "Asia/Dhaka" => 6.0,
        "Asia/Bangkok" | "Asia/Jakarta" => 7.0,
        "Asia/Shanghai" | "Asia/Hong_Kong" | "Asia/Singapore" | "Asia/Taipei" => 8.0,
        "Asia/Tokyo" | "Asia/Seoul" => 9.0,
        // Australia
        "Australia/Sydney" | "Australia/Melbourne" => 10.0,
        "Australia/Adelaide" => 9.5,
        "Australia/Perth" => 8.0,
        "Australia/Brisbane" => 10.0,
        // Pacific
        "Pacific/Auckland" | "NZ" => 12.0,
        "Pacific/Fiji" => 12.0,
        // South America
        "America/Sao_Paulo" | "America/Argentina/Buenos_Aires" => -3.0,
        "America/Santiago" => -4.0,
        "America/Bogota" | "America/Lima" => -5.0,
        // Africa
        "Africa/Cairo" => 2.0,
        "Africa/Lagos" | "Africa/Johannesburg" => 1.0,
        "Africa/Nairobi" => 3.0,
        // India
        "Asia/Karachi" => 5.0,
        // Canada
        "America/Toronto" => -5.0,
        "America/Vancouver" => -8.0,
        "America/Edmonton" => -7.0,
        "America/Winnipeg" => -6.0,
        "America/Halifax" => -4.0,
        "America/St_Johns" => -3.5,
        // Other
        "Asia/Kathmandu" => 5.75,
        "Asia/Colombo" => 5.5,
        "Asia/Yangon" => 6.5,
        _ => {
            // Try parsing fixed offset like "+05:00" or "-08:00"
            if let Some(secs) = parse_fixed_offset(name) {
                return Ok(secs);
            }
            return Err(anyhow::anyhow!("unknown timezone: {name}"));
        }
    };
    Ok((offset_hours * 3600.0) as i32)
}

fn parse_fixed_offset(s: &str) -> Option<i32> {
    let s = s.trim();
    if s.len() < 3 { return None; }
    let (sign, rest) = match s.as_bytes()[0] {
        b'+' => (1, &s[1..]),
        b'-' => (-1, &s[1..]),
        _ => return None,
    };
    if let Some((h, m)) = rest.split_once(':') {
        let h: i32 = h.parse().ok()?;
        let m: i32 = m.parse().ok()?;
        Some(sign * (h * 3600 + m * 60))
    } else {
        let h: i32 = rest.parse().ok()?;
        Some(sign * h * 3600)
    }
}

// Re-export datetime helpers for use by time_module and humanize_module
pub fn datetime_to_unix(year: i64, month: i64, day: i64, hour: i64, min: i64, sec: i64) -> i64 {
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

pub fn unix_to_datetime(mut ts: i64) -> (i64, i64, i64, i64, i64, i64) {
    let negative = ts < 0;
    if negative {
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

pub fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

// Day of week: 0=Sunday, 6=Saturday
pub fn weekday(unix_secs: i64) -> i32 {
    // 1970-01-01 was Thursday (4)
    let days = unix_secs.div_euclid(86400);
    ((days + 4) % 7) as i32
}

pub fn parse_iso8601(s: &str) -> Option<i64> {
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

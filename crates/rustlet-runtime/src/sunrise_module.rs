use starlark::environment::GlobalsBuilder;
use starlark::values::float::StarlarkFloat;
use starlark::values::tuple::AllocTuple;
use starlark::values::{Value, ValueLike};
#[allow(deprecated)]
use sunrise::sunrise_sunset;

use crate::starlark_time::StarlarkTime;

#[starlark::starlark_module]
pub fn sunrise_module(builder: &mut GlobalsBuilder) {
    fn sunrise<'v>(
        lat: Value<'v>,
        lng: Value<'v>,
        date: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let lat = unpack_number(lat)?;
        let lng = unpack_number(lng)?;
        let time = unpack_time(date)?;
        let (year, month, day, _, _, _) = time.components();
        let (rise, _) = sunrise_sunset(lat, lng, year as i32, month as u32, day as u32);
        Ok(eval.heap().alloc(StarlarkTime::from_unix(rise + 1, 0)))
    }

    fn sunset<'v>(
        lat: Value<'v>,
        lng: Value<'v>,
        date: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let lat = unpack_number(lat)?;
        let lng = unpack_number(lng)?;
        let time = unpack_time(date)?;
        let (year, month, day, _, _, _) = time.components();
        let (_, set) = sunrise_sunset(lat, lng, year as i32, month as u32, day as u32);
        Ok(eval.heap().alloc(StarlarkTime::from_unix(set - 1, 0)))
    }

    fn elevation<'v>(lat: Value<'v>, lng: Value<'v>, time: Value<'v>) -> anyhow::Result<f64> {
        let lat = unpack_number(lat)?;
        let lng = unpack_number(lng)?;
        let time = unpack_time(time)?;
        Ok(solar_elevation_degrees(lat, lng, time))
    }

    fn elevation_time<'v>(
        lat: Value<'v>,
        lng: Value<'v>,
        elev: Value<'v>,
        date: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let lat = unpack_number(lat)?;
        let lng = unpack_number(lng)?;
        let elev = unpack_number(elev)?;
        let time = unpack_time(date)?;
        let (year, month, day, _, _, _) = time.components();

        let (morning, evening) = if (elev - (-50.0 / 60.0)).abs() < 1e-6 {
            let (rise, set) = sunrise_sunset(lat, lng, year as i32, month as u32, day as u32);
            (Some(rise + 1), Some(set - 1))
        } else {
            let start = crate::starlark_time::datetime_to_unix(year, month, day, 0, 0, 0);
            let noon = crate::starlark_time::datetime_to_unix(year, month, day, 12, 0, 0);
            let end = start + 86_400;
            (
                find_elevation_crossing(lat, lng, elev, start, noon),
                find_elevation_crossing(lat, lng, elev, noon, end),
            )
        };

        match (morning, evening) {
            (Some(morning), Some(evening)) => Ok(eval.heap().alloc(AllocTuple([
                eval.heap().alloc(StarlarkTime::from_unix(morning, 0)),
                eval.heap().alloc(StarlarkTime::from_unix(evening, 0)),
            ]))),
            _ => Ok(Value::new_none()),
        }
    }
}

pub fn build_sunrise_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(sunrise_module)
        .build()
}

fn unpack_time(value: Value<'_>) -> anyhow::Result<&StarlarkTime> {
    value
        .downcast_ref::<StarlarkTime>()
        .ok_or_else(|| anyhow::anyhow!("expected Time, got {}", value.get_type()))
}

fn solar_elevation_degrees(lat: f64, lng: f64, time: &StarlarkTime) -> f64 {
    let unix = time.unix_secs as f64 + time.unix_nanos as f64 / 1_000_000_000.0;
    let julian_day = unix / 86_400.0 + 2_440_587.5;
    let julian_century = (julian_day - 2_451_545.0) / 36_525.0;

    let geom_mean_long_sun = (280.46646
        + julian_century * (36_000.76983 + julian_century * 0.0003032))
        .rem_euclid(360.0);
    let geom_mean_anom_sun =
        357.52911 + julian_century * (35_999.05029 - 0.0001537 * julian_century);
    let eccent_earth_orbit =
        0.016708634 - julian_century * (0.000042037 + 0.0000001267 * julian_century);
    let sun_eq_of_center = geom_mean_anom_sun.to_radians().sin()
        * (1.914602 - julian_century * (0.004817 + 0.000014 * julian_century))
        + (2.0 * geom_mean_anom_sun).to_radians().sin() * (0.019993 - 0.000101 * julian_century)
        + (3.0 * geom_mean_anom_sun).to_radians().sin() * 0.000289;
    let sun_true_long = geom_mean_long_sun + sun_eq_of_center;
    let sun_app_long =
        sun_true_long - 0.00569 - 0.00478 * (125.04 - 1934.136 * julian_century).to_radians().sin();
    let mean_obliq_ecliptic = 23.0
        + (26.0
            + ((21.448
                - julian_century
                    * (46.815 + julian_century * (0.00059 - julian_century * 0.001813)))
                / 60.0))
            / 60.0;
    let obliq_corr =
        mean_obliq_ecliptic + 0.00256 * (125.04 - 1934.136 * julian_century).to_radians().cos();
    let sun_declination = (obliq_corr.to_radians().sin() * sun_app_long.to_radians().sin()).asin();

    let y = (obliq_corr.to_radians() / 2.0).tan().powi(2);
    let eq_time = 4.0
        * (y * (2.0 * geom_mean_long_sun.to_radians()).sin()
            - 2.0 * eccent_earth_orbit * geom_mean_anom_sun.to_radians().sin()
            + 4.0
                * eccent_earth_orbit
                * y
                * geom_mean_anom_sun.to_radians().sin()
                * (2.0 * geom_mean_long_sun.to_radians()).cos()
            - 0.5 * y.powi(2) * (4.0 * geom_mean_long_sun.to_radians()).sin()
            - 1.25 * eccent_earth_orbit.powi(2) * (2.0 * geom_mean_anom_sun.to_radians()).sin())
        .to_degrees();

    let minutes =
        (((time.unix_secs + time.utc_offset_secs as i64) % 86_400 + 86_400) % 86_400) as f64 / 60.0;
    let true_solar_time = (minutes + eq_time + 4.0 * lng).rem_euclid(1440.0);
    let hour_angle = if true_solar_time / 4.0 < 0.0 {
        true_solar_time / 4.0 + 180.0
    } else {
        true_solar_time / 4.0 - 180.0
    };

    (lat.to_radians().sin() * sun_declination.sin()
        + lat.to_radians().cos() * sun_declination.cos() * hour_angle.to_radians().cos())
    .asin()
    .to_degrees()
}

fn find_elevation_crossing(lat: f64, lng: f64, target: f64, start: i64, end: i64) -> Option<i64> {
    let mut lo = start;
    let mut hi = end;
    let mut lo_diff = solar_elevation_degrees(lat, lng, &StarlarkTime::from_unix(lo, 0)) - target;
    let hi_diff = solar_elevation_degrees(lat, lng, &StarlarkTime::from_unix(hi, 0)) - target;
    if lo_diff == 0.0 {
        return Some(lo);
    }
    if hi_diff == 0.0 {
        return Some(hi);
    }
    if lo_diff.signum() == hi_diff.signum() {
        return None;
    }

    for _ in 0..40 {
        let mid = lo + (hi - lo) / 2;
        let mid_diff = solar_elevation_degrees(lat, lng, &StarlarkTime::from_unix(mid, 0)) - target;
        if mid_diff.abs() < 1e-4 || hi - lo <= 1 {
            return Some(mid);
        }
        if mid_diff.signum() == lo_diff.signum() {
            lo = mid;
            lo_diff = mid_diff;
        } else {
            hi = mid;
        }
    }

    Some(lo + (hi - lo) / 2)
}

fn unpack_number(value: Value) -> anyhow::Result<f64> {
    if let Some(value) = value.unpack_i32() {
        Ok(value as f64)
    } else if let Some(value) = value.downcast_ref::<StarlarkFloat>() {
        Ok(value.0)
    } else {
        Err(anyhow::anyhow!("expected number, got {}", value.get_type()))
    }
}

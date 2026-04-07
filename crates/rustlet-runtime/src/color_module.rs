use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::values::float::StarlarkFloat;
use starlark::values::{Value, ValueLike};

use crate::starlark_color::StarlarkColor;

use rustlet_render::parse_color;

fn to_f64(v: Value) -> anyhow::Result<f64> {
    if let Some(f) = v.downcast_ref::<StarlarkFloat>() {
        return Ok(f.0);
    }
    if let Some(i) = v.unpack_i32() {
        return Ok(i as f64);
    }
    Err(anyhow::anyhow!(
        "expected number, got {}",
        v.get_type()
    ))
}

#[starlark::starlark_module]
pub fn color_module(builder: &mut GlobalsBuilder) {
    fn rgb<'v>(
        r: i32,
        g: i32,
        b: i32,
        #[starlark(default = 255)] a: i32,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        validate_channel("r", r)?;
        validate_channel("g", g)?;
        validate_channel("b", b)?;
        validate_channel("a", a)?;
        Ok(eval.heap().alloc(StarlarkColor {
            r: r as u8,
            g: g as u8,
            b: b as u8,
            a: a as u8,
        }))
    }

    fn hex<'v>(
        value: &str,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let c = parse_color(value)?;
        Ok(eval.heap().alloc(StarlarkColor {
            r: (c.red() * 255.0).round() as u8,
            g: (c.green() * 255.0).round() as u8,
            b: (c.blue() * 255.0).round() as u8,
            a: (c.alpha() * 255.0).round() as u8,
        }))
    }

    fn hsv<'v>(
        h: Value<'v>,
        s: Value<'v>,
        v: Value<'v>,
        #[starlark(default = 255)] a: i32,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let hf = to_f64(h)?;
        let sf = to_f64(s)?;
        let vf = to_f64(v)?;

        if !(0.0..=360.0).contains(&hf) {
            return Err(anyhow::anyhow!("h must be 0..360, got {hf}"));
        }
        if !(0.0..=1.0).contains(&sf) {
            return Err(anyhow::anyhow!("s must be 0.0..1.0, got {sf}"));
        }
        if !(0.0..=1.0).contains(&vf) {
            return Err(anyhow::anyhow!("v must be 0.0..1.0, got {vf}"));
        }
        validate_channel("a", a)?;

        let (r, g, b) = hsv_to_rgb(hf, sf, vf);
        Ok(eval.heap().alloc(StarlarkColor {
            r,
            g,
            b,
            a: a as u8,
        }))
    }
}

fn validate_channel(name: &str, val: i32) -> anyhow::Result<()> {
    if !(0..=255).contains(&val) {
        return Err(anyhow::anyhow!("{name} must be 0..255, got {val}"));
    }
    Ok(())
}

/// Standard HSV to RGB conversion.
/// h: 0..360, s: 0..1, v: 0..1
fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let c = v * s;
    let h_prime = h / 60.0;
    let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match h_prime as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    (
        ((r1 + m) * 255.0).round() as u8,
        ((g1 + m) * 255.0).round() as u8,
        ((b1 + m) * 255.0).round() as u8,
    )
}

pub fn build_color_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(color_module)
        .build()
}

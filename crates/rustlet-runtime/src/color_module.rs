use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::values::float::StarlarkFloat;
use starlark::values::{Value, ValueLike};

use crate::starlark_color::{hsv_to_rgb, StarlarkColor};

use rustlet_render::parse_color;

fn to_f64(v: Value) -> anyhow::Result<f64> {
    if let Some(f) = v.downcast_ref::<StarlarkFloat>() {
        return Ok(f.0);
    }
    if let Some(i) = v.unpack_i32() {
        return Ok(i as f64);
    }
    Err(anyhow::anyhow!("expected number, got {}", v.get_type()))
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
        Ok(eval.heap().alloc(StarlarkColor::new(
            clamp_channel(r),
            clamp_channel(g),
            clamp_channel(b),
            clamp_channel(a),
        )))
    }

    fn hex<'v>(value: &str, eval: &mut Evaluator<'v, '_, '_>) -> anyhow::Result<Value<'v>> {
        if value.is_empty() {
            return Ok(eval.heap().alloc(StarlarkColor::new(0, 0, 0, 0)));
        }
        let c = parse_color(value)?;
        Ok(eval.heap().alloc(StarlarkColor::new(
            (c.red() * 255.0).round() as u8,
            (c.green() * 255.0).round() as u8,
            (c.blue() * 255.0).round() as u8,
            (c.alpha() * 255.0).round() as u8,
        )))
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

        let (r, g, b) = hsv_to_rgb(hf, sf, vf);
        Ok(eval
            .heap()
            .alloc(StarlarkColor::new(r, g, b, clamp_channel(a))))
    }
}

fn clamp_channel(val: i32) -> u8 {
    val.clamp(0, 255) as u8
}

pub fn build_color_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(color_module)
        .build()
}

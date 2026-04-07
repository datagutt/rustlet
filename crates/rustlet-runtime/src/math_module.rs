use starlark::environment::GlobalsBuilder;
use starlark::values::float::StarlarkFloat;
use starlark::values::{Value, ValueLike};

/// Extract a float from a Value that may be int or float.
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
pub fn math_module(builder: &mut GlobalsBuilder) {
    fn floor<'v>(x: Value<'v>) -> anyhow::Result<i32> {
        Ok(to_f64(x)?.floor() as i32)
    }

    fn ceil<'v>(x: Value<'v>) -> anyhow::Result<i32> {
        Ok(to_f64(x)?.ceil() as i32)
    }

    fn round<'v>(x: Value<'v>) -> anyhow::Result<i32> {
        Ok(to_f64(x)?.round() as i32)
    }

    fn fabs<'v>(x: Value<'v>) -> anyhow::Result<StarlarkFloat> {
        Ok(StarlarkFloat(to_f64(x)?.abs()))
    }

    fn pow<'v>(base: Value<'v>, exp: Value<'v>) -> anyhow::Result<StarlarkFloat> {
        Ok(StarlarkFloat(to_f64(base)?.powf(to_f64(exp)?)))
    }

    fn sqrt<'v>(x: Value<'v>) -> anyhow::Result<StarlarkFloat> {
        let v = to_f64(x)?;
        if v < 0.0 {
            return Err(anyhow::anyhow!("sqrt of negative number"));
        }
        Ok(StarlarkFloat(v.sqrt()))
    }

    fn log<'v>(x: Value<'v>) -> anyhow::Result<StarlarkFloat> {
        let v = to_f64(x)?;
        if v <= 0.0 {
            return Err(anyhow::anyhow!("log of non-positive number"));
        }
        Ok(StarlarkFloat(v.ln()))
    }

    fn sin<'v>(x: Value<'v>) -> anyhow::Result<StarlarkFloat> {
        Ok(StarlarkFloat(to_f64(x)?.sin()))
    }

    fn cos<'v>(x: Value<'v>) -> anyhow::Result<StarlarkFloat> {
        Ok(StarlarkFloat(to_f64(x)?.cos()))
    }

    fn tan<'v>(x: Value<'v>) -> anyhow::Result<StarlarkFloat> {
        Ok(StarlarkFloat(to_f64(x)?.tan()))
    }

    fn atan2<'v>(y: Value<'v>, x: Value<'v>) -> anyhow::Result<StarlarkFloat> {
        Ok(StarlarkFloat(to_f64(y)?.atan2(to_f64(x)?)))
    }
}

pub fn build_math_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(math_module)
        .build()
}

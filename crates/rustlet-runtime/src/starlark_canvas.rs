use std::fmt;

use allocative::Allocative;
use starlark::environment::{Methods, MethodsBuilder, MethodsStatic};
use starlark::starlark_simple_value;
use starlark::values::{NoSerialize, ProvidesStaticType, StarlarkValue, Value, ValueLike};
use starlark_derive::starlark_value;

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkCanvas {
    pub width: i32,
    pub height: i32,
    pub is_2x: bool,
}

starlark_simple_value!(StarlarkCanvas);

impl fmt::Display for StarlarkCanvas {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "canvas({}x{}", self.width, self.height)?;
        if self.is_2x {
            write!(f, ", 2x")?;
        }
        write!(f, ")")
    }
}

#[starlark_value(type = "Canvas")]
impl<'v> StarlarkValue<'v> for StarlarkCanvas {
    fn get_methods() -> Option<&'static Methods> {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(canvas_methods)
    }
}

#[starlark::starlark_module]
fn canvas_methods(builder: &mut MethodsBuilder) {
    fn width(#[starlark(this)] this: Value) -> anyhow::Result<i32> {
        let canvas = this
            .downcast_ref::<StarlarkCanvas>()
            .ok_or_else(|| anyhow::anyhow!("expected Canvas"))?;
        Ok(canvas.width)
    }

    fn height(#[starlark(this)] this: Value) -> anyhow::Result<i32> {
        let canvas = this
            .downcast_ref::<StarlarkCanvas>()
            .ok_or_else(|| anyhow::anyhow!("expected Canvas"))?;
        Ok(canvas.height)
    }

    fn is2x(#[starlark(this)] this: Value) -> anyhow::Result<bool> {
        let canvas = this
            .downcast_ref::<StarlarkCanvas>()
            .ok_or_else(|| anyhow::anyhow!("expected Canvas"))?;
        Ok(canvas.is_2x)
    }
}

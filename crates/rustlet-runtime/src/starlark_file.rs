use std::fmt;

use allocative::Allocative;
use starlark::environment::{Methods, MethodsBuilder, MethodsStatic};
use starlark::starlark_simple_value;
use starlark::values::{
    NoSerialize, ProvidesStaticType, StarlarkValue, Value, ValueLike,
};
use starlark_derive::starlark_value;

/// Wraps file content (base64-encoded) with a .readall() method that returns the raw bytes as a string.
#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkFile {
    #[allocative(skip)]
    pub data: String,
}

starlark_simple_value!(StarlarkFile);

impl fmt::Display for StarlarkFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<file {} bytes>", self.data.len())
    }
}

#[starlark_value(type = "file")]
impl<'v> StarlarkValue<'v> for StarlarkFile {
    fn get_methods() -> Option<&'static Methods> {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(file_methods)
    }
}

#[starlark::starlark_module]
fn file_methods(builder: &mut MethodsBuilder) {
    fn readall<'v>(
        #[starlark(this)] this: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let f = this
            .downcast_ref::<StarlarkFile>()
            .ok_or_else(|| anyhow::anyhow!("expected file"))?;
        Ok(eval.heap().alloc(f.data.as_str()))
    }
}

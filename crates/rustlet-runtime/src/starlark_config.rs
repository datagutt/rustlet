use std::collections::HashMap;
use std::fmt;

use allocative::Allocative;
use starlark::environment::{Methods, MethodsBuilder, MethodsStatic};
use starlark::starlark_simple_value;
use starlark::values::none::NoneType;
use starlark::values::{Heap, NoSerialize, ProvidesStaticType, StarlarkValue, Value, ValueLike};
use starlark_derive::starlark_value;

/// Wraps a config dict with .str(), .bool(), .get() methods like pixlet's config object.
#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkConfig {
    #[allocative(skip)]
    pub entries: HashMap<String, String>,
}

starlark_simple_value!(StarlarkConfig);

impl fmt::Display for StarlarkConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "config({})", self.entries.len())
    }
}

#[starlark_value(type = "config")]
impl<'v> StarlarkValue<'v> for StarlarkConfig {
    fn get_methods() -> Option<&'static Methods> {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(config_methods)
    }

    fn at(&self, index: Value<'v>, heap: &'v Heap) -> starlark::Result<Value<'v>> {
        let key = index
            .unpack_str()
            .ok_or_else(|| starlark::Error::new_other(anyhow::anyhow!("key must be a string")))?;
        match self.entries.get(key) {
            Some(v) => Ok(heap.alloc(v.as_str())),
            None => Err(starlark::Error::new_other(anyhow::anyhow!(
                "key not found: {key}"
            ))),
        }
    }
}

#[starlark::starlark_module]
fn config_methods(builder: &mut MethodsBuilder) {
    fn get<'v>(
        #[starlark(this)] this: Value<'v>,
        key: &str,
        #[starlark(default = starlark::values::none::NoneType)] default: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let cfg = this
            .downcast_ref::<StarlarkConfig>()
            .ok_or_else(|| anyhow::anyhow!("expected config"))?;
        match cfg.entries.get(key) {
            Some(v) => Ok(eval.heap().alloc(v.as_str())),
            None => Ok(default),
        }
    }

    fn str<'v>(
        #[starlark(this)] this: Value<'v>,
        key: &str,
        #[starlark(default = NoneType)] default: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let cfg = this
            .downcast_ref::<StarlarkConfig>()
            .ok_or_else(|| anyhow::anyhow!("expected config"))?;
        match cfg.entries.get(key) {
            Some(v) => Ok(eval.heap().alloc(v.as_str())),
            None => Ok(default),
        }
    }

    fn bool<'v>(
        #[starlark(this)] this: Value<'v>,
        key: &str,
        #[starlark(default = NoneType)] default: Value<'v>,
    ) -> anyhow::Result<Value<'v>> {
        let cfg = this
            .downcast_ref::<StarlarkConfig>()
            .ok_or_else(|| anyhow::anyhow!("expected config"))?;
        match cfg.entries.get(key) {
            Some(v) => Ok(Value::new_bool(v == "true" || v == "True" || v == "1")),
            None => Ok(default),
        }
    }
}

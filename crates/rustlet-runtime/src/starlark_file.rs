use std::fmt;

use allocative::Allocative;
use starlark::environment::{Methods, MethodsBuilder, MethodsStatic};
use starlark::starlark_simple_value;
use starlark::values::{NoSerialize, ProvidesStaticType, StarlarkValue, Value, ValueLike};
use starlark_derive::starlark_value;

use crate::starlark_bytes::StarlarkBytes;

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkFile {
    #[allocative(skip)]
    pub path: String,
    #[allocative(skip)]
    pub data: Vec<u8>,
}

starlark_simple_value!(StarlarkFile);

impl fmt::Display for StarlarkFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<file {}>", self.path)
    }
}

#[starlark_value(type = "file")]
impl<'v> StarlarkValue<'v> for StarlarkFile {
    fn get_methods() -> Option<&'static Methods> {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(file_methods)
    }
    fn has_attr(&self, attribute: &str, _heap: &'v starlark::values::Heap) -> bool {
        matches!(attribute, "path")
    }

    fn dir_attr(&self) -> Vec<String> {
        vec!["path".to_owned()]
    }

    fn get_attr(&self, attribute: &str, heap: &'v starlark::values::Heap) -> Option<Value<'v>> {
        match attribute {
            "path" => Some(heap.alloc(self.path.as_str())),
            _ => None,
        }
    }
}

#[starlark::starlark_module]
fn file_methods(builder: &mut MethodsBuilder) {
    fn readall<'v>(
        #[starlark(this)] this: Value<'v>,
        #[starlark(default = "")] mode: &str,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let f = this
            .downcast_ref::<StarlarkFile>()
            .ok_or_else(|| anyhow::anyhow!("expected file"))?;
        match mode {
            "" | "r" | "rt" => match std::str::from_utf8(&f.data) {
                Ok(s) => Ok(eval.heap().alloc(s)),
                Err(_) if mode.is_empty() => Ok(eval.heap().alloc(StarlarkBytes {
                    data: f.data.clone(),
                })),
                Err(e) => Err(anyhow::anyhow!("file is not valid UTF-8: {e}")),
            },
            "rb" => Ok(eval.heap().alloc(StarlarkBytes {
                data: f.data.clone(),
            })),
            _ => Err(anyhow::anyhow!("unsupported mode: {mode}")),
        }
    }
}

use std::fmt;

use allocative::Allocative;
use starlark::starlark_simple_value;
use starlark::values::{NoSerialize, ProvidesStaticType, StarlarkValue, Value, ValueLike};
use starlark_derive::starlark_value;

#[derive(Debug, Clone, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkBytes {
    #[allocative(skip)]
    pub data: Vec<u8>,
}

starlark_simple_value!(StarlarkBytes);

impl fmt::Display for StarlarkBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bytes(len={})", self.data.len())
    }
}

#[starlark_value(type = "bytes")]
impl<'v> StarlarkValue<'v> for StarlarkBytes {
    fn equals(&self, other: Value<'v>) -> starlark::Result<bool> {
        match other.downcast_ref::<StarlarkBytes>() {
            Some(other) => Ok(self.data == other.data),
            None => Ok(false),
        }
    }
}

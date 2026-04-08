use std::fmt;
use std::hash::Hash;

use allocative::Allocative;
use starlark::collections::StarlarkHasher;
use starlark::environment::{Methods, MethodsBuilder, MethodsStatic};
use starlark::starlark_simple_value;
use starlark::values::{
    Heap, NoSerialize, ProvidesStaticType, StarlarkValue, Value, ValueError, ValueLike,
};
use starlark_derive::starlark_value;

#[derive(Debug, Clone, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkDuration {
    pub total_nanos: i64,
}

starlark_simple_value!(StarlarkDuration);

impl StarlarkDuration {
    pub fn from_nanos(total_nanos: i64) -> Self {
        Self { total_nanos }
    }
}

impl fmt::Display for StarlarkDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.total_nanos % 1_000_000_000 == 0 {
            write!(f, "{}s", self.total_nanos / 1_000_000_000)
        } else if self.total_nanos % 1_000_000 == 0 {
            write!(f, "{}ms", self.total_nanos / 1_000_000)
        } else {
            write!(f, "{}ns", self.total_nanos)
        }
    }
}

#[starlark_value(type = "Duration")]
impl<'v> StarlarkValue<'v> for StarlarkDuration {
    fn get_methods() -> Option<&'static Methods> {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(duration_methods)
    }

    fn has_attr(&self, attribute: &str, _heap: &'v Heap) -> bool {
        matches!(
            attribute,
            "hours" | "minutes" | "seconds" | "milliseconds" | "microseconds" | "nanoseconds"
        )
    }

    fn dir_attr(&self) -> Vec<String> {
        vec![
            "hours".into(),
            "minutes".into(),
            "seconds".into(),
            "milliseconds".into(),
            "microseconds".into(),
            "nanoseconds".into(),
        ]
    }

    fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match attribute {
            "hours" => Some(heap.alloc(self.total_nanos / 3_600_000_000_000)),
            "minutes" => Some(heap.alloc(self.total_nanos / 60_000_000_000)),
            "seconds" => Some(heap.alloc(self.total_nanos / 1_000_000_000)),
            "milliseconds" => Some(heap.alloc(self.total_nanos / 1_000_000)),
            "microseconds" => Some(heap.alloc(self.total_nanos / 1_000)),
            "nanoseconds" => Some(heap.alloc(self.total_nanos)),
            _ => None,
        }
    }

    fn equals(&self, other: Value<'v>) -> starlark::Result<bool> {
        match other.downcast_ref::<StarlarkDuration>() {
            Some(o) => Ok(self.total_nanos == o.total_nanos),
            None => Ok(false),
        }
    }

    fn write_hash(&self, hasher: &mut StarlarkHasher) -> starlark::Result<()> {
        self.total_nanos.hash(hasher);
        Ok(())
    }

    fn plus(&self, heap: &'v Heap) -> starlark::Result<Value<'v>> {
        Ok(heap.alloc(self.clone()))
    }

    fn minus(&self, heap: &'v Heap) -> starlark::Result<Value<'v>> {
        Ok(heap.alloc(StarlarkDuration::from_nanos(
            self.total_nanos.saturating_neg(),
        )))
    }

    fn add(&self, rhs: Value<'v>, heap: &'v Heap) -> Option<starlark::Result<Value<'v>>> {
        let rhs = rhs.downcast_ref::<StarlarkDuration>()?;
        Some(Ok(heap.alloc(StarlarkDuration::from_nanos(
            self.total_nanos.saturating_add(rhs.total_nanos),
        ))))
    }

    fn sub(&self, other: Value<'v>, heap: &'v Heap) -> starlark::Result<Value<'v>> {
        if let Some(other) = other.downcast_ref::<StarlarkDuration>() {
            return Ok(heap.alloc(StarlarkDuration::from_nanos(
                self.total_nanos.saturating_sub(other.total_nanos),
            )));
        }
        ValueError::unsupported_with(self, "-", other)
    }
}

#[starlark::starlark_module]
fn duration_methods(_builder: &mut MethodsBuilder) {}

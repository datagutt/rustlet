use std::fmt;

use allocative::Allocative;
use anyhow::Result;
use starlark::environment::{Methods, MethodsBuilder, MethodsStatic};
use starlark::starlark_simple_value;
use starlark::values::{
    Heap, NoSerialize, ProvidesStaticType, StarlarkValue, Value, ValueLike,
};
use starlark_derive::starlark_value;

use rustlet_render::parse_color;

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

starlark_simple_value!(StarlarkColor);

impl StarlarkColor {
    pub fn hex(&self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }

    /// Accept either a StarlarkColor or a hex string, returning a tiny_skia::Color.
    pub fn color_from_value(v: Value) -> Result<Option<tiny_skia::Color>> {
        if v.is_none() {
            return Ok(None);
        }
        if let Some(sc) = v.downcast_ref::<StarlarkColor>() {
            return Ok(Some(tiny_skia::Color::from_rgba8(sc.r, sc.g, sc.b, sc.a)));
        }
        if let Some(s) = v.unpack_str() {
            if s.is_empty() {
                return Ok(None);
            }
            return Ok(Some(parse_color(s)?));
        }
        Err(anyhow::anyhow!(
            "color must be a Color or string, got {}",
            v.get_type()
        ))
    }
}

impl fmt::Display for StarlarkColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.hex())
    }
}

#[starlark_value(type = "Color")]
impl<'v> StarlarkValue<'v> for StarlarkColor {
    fn get_methods() -> Option<&'static Methods> {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(color_methods)
    }

    fn has_attr(&self, attribute: &str, _heap: &'v Heap) -> bool {
        matches!(attribute, "r" | "g" | "b" | "a")
    }

    fn dir_attr(&self) -> Vec<String> {
        vec![
            "r".to_owned(),
            "g".to_owned(),
            "b".to_owned(),
            "a".to_owned(),
        ]
    }

    fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match attribute {
            "r" => Some(heap.alloc(self.r as i32)),
            "g" => Some(heap.alloc(self.g as i32)),
            "b" => Some(heap.alloc(self.b as i32)),
            "a" => Some(heap.alloc(self.a as i32)),
            _ => None,
        }
    }

    fn equals(&self, other: Value<'v>) -> starlark::Result<bool> {
        match other.downcast_ref::<StarlarkColor>() {
            Some(o) => Ok(self.r == o.r && self.g == o.g && self.b == o.b && self.a == o.a),
            None => Ok(false),
        }
    }
}

#[starlark::starlark_module]
fn color_methods(builder: &mut MethodsBuilder) {
    fn hex(#[starlark(this)] this: Value) -> anyhow::Result<String> {
        let color = this
            .downcast_ref::<StarlarkColor>()
            .ok_or_else(|| anyhow::anyhow!("expected Color"))?;
        Ok(color.hex())
    }

    fn rgb(#[starlark(this)] this: Value) -> anyhow::Result<Vec<i32>> {
        let c = this
            .downcast_ref::<StarlarkColor>()
            .ok_or_else(|| anyhow::anyhow!("expected Color"))?;
        Ok(vec![c.r as i32, c.g as i32, c.b as i32])
    }

    fn rgba(#[starlark(this)] this: Value) -> anyhow::Result<Vec<i32>> {
        let c = this
            .downcast_ref::<StarlarkColor>()
            .ok_or_else(|| anyhow::anyhow!("expected Color"))?;
        Ok(vec![c.r as i32, c.g as i32, c.b as i32, c.a as i32])
    }
}

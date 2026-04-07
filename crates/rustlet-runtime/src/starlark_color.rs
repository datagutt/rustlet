use std::fmt;
use std::hash::Hash;
use std::sync::Mutex;

use allocative::Allocative;
use anyhow::Result;
use starlark::collections::StarlarkHasher;
use starlark::environment::{Methods, MethodsBuilder, MethodsStatic};
use starlark::starlark_simple_value;
use starlark::values::tuple::AllocTuple;
use starlark::values::{Heap, NoSerialize, ProvidesStaticType, StarlarkValue, Value, ValueLike};
use starlark_derive::starlark_value;

use rustlet_render::parse_color;

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkColor {
    #[allocative(skip)]
    rgba: Mutex<u32>,
}

starlark_simple_value!(StarlarkColor);

impl StarlarkColor {
    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            rgba: Mutex::new(pack_rgba(r, g, b, a)),
        }
    }

    pub fn rgba_components(&self) -> (u8, u8, u8, u8) {
        unpack_rgba(*self.rgba.lock().unwrap())
    }

    fn set_rgba_components(&self, r: u8, g: u8, b: u8, a: u8) {
        *self.rgba.lock().unwrap() = pack_rgba(r, g, b, a);
    }

    pub fn hex(&self) -> String {
        let (r, g, b, a) = self.rgba_components();
        if a == 255 {
            format!("#{:02x}{:02x}{:02x}", r, g, b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", r, g, b, a)
        }
    }

    pub fn hsv_components(&self) -> (f64, f64, f64, u8) {
        let (r, g, b, a) = self.rgba_components();
        rgba_to_hsv(r, g, b, a)
    }

    /// Accept either a StarlarkColor or a hex string, returning a tiny_skia::Color.
    pub fn color_from_value(v: Value) -> Result<Option<tiny_skia::Color>> {
        if v.is_none() {
            return Ok(None);
        }
        if let Some(sc) = v.downcast_ref::<StarlarkColor>() {
            let (r, g, b, a) = sc.rgba_components();
            return Ok(Some(tiny_skia::Color::from_rgba8(r, g, b, a)));
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
        write!(f, "Color(\"{}\")", self.hex())
    }
}

#[starlark_value(type = "Color")]
impl<'v> StarlarkValue<'v> for StarlarkColor {
    fn get_methods() -> Option<&'static Methods> {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(color_methods)
    }

    fn has_attr(&self, attribute: &str, _heap: &'v Heap) -> bool {
        matches!(attribute, "r" | "g" | "b" | "a" | "h" | "s" | "v")
    }

    fn dir_attr(&self) -> Vec<String> {
        vec![
            "r".to_owned(),
            "g".to_owned(),
            "b".to_owned(),
            "a".to_owned(),
            "h".to_owned(),
            "s".to_owned(),
            "v".to_owned(),
        ]
    }

    fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
        let (r, g, b, a) = self.rgba_components();
        let (h, s, v, _) = self.hsv_components();
        match attribute {
            "r" => Some(heap.alloc(r as i32)),
            "g" => Some(heap.alloc(g as i32)),
            "b" => Some(heap.alloc(b as i32)),
            "a" => Some(heap.alloc(a as i32)),
            "h" => Some(heap.alloc(h)),
            "s" => Some(heap.alloc(s)),
            "v" => Some(heap.alloc(v)),
            _ => None,
        }
    }

    fn set_attr(&self, attribute: &str, new_value: Value<'v>) -> starlark::Result<()> {
        match attribute {
            "r" | "g" | "b" | "a" => {
                let component = new_value.unpack_i32().ok_or_else(|| {
                    starlark::Error::new_other(anyhow::anyhow!(
                        "value for {attribute:?} must be an integer"
                    ))
                })?;
                let (mut r, mut g, mut b, mut a) = self.rgba_components();
                match attribute {
                    "r" => r = clamp_u8(component),
                    "g" => g = clamp_u8(component),
                    "b" => b = clamp_u8(component),
                    "a" => a = clamp_u8(component),
                    _ => {}
                }
                self.set_rgba_components(r, g, b, a);
                Ok(())
            }
            "h" | "s" | "v" => {
                let component = unpack_numeric(new_value).ok_or_else(|| {
                    starlark::Error::new_other(anyhow::anyhow!(
                        "value for {attribute:?} must be a number"
                    ))
                })?;
                let (mut h, mut s, mut v, a) = self.hsv_components();
                match attribute {
                    "h" => h = component,
                    "s" => s = component,
                    "v" => v = component,
                    _ => {}
                }
                let (r, g, b) = hsv_to_rgb(h, s, v);
                self.set_rgba_components(r, g, b, a);
                Ok(())
            }
            _ => Err(starlark::Error::new_other(anyhow::anyhow!(
                "cannot assign to field {attribute:?}"
            ))),
        }
    }

    fn equals(&self, other: Value<'v>) -> starlark::Result<bool> {
        match other.downcast_ref::<StarlarkColor>() {
            Some(o) => Ok(*self.rgba.lock().unwrap() == *o.rgba.lock().unwrap()),
            None => Ok(false),
        }
    }

    fn write_hash(&self, hasher: &mut StarlarkHasher) -> starlark::Result<()> {
        self.rgba.lock().unwrap().hash(hasher);
        Ok(())
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

    fn rgb<'v>(
        #[starlark(this)] this: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let c = this
            .downcast_ref::<StarlarkColor>()
            .ok_or_else(|| anyhow::anyhow!("expected Color"))?;
        let (r, g, b, _) = c.rgba_components();
        Ok(eval
            .heap()
            .alloc(AllocTuple([r as i32, g as i32, b as i32])))
    }

    fn rgba<'v>(
        #[starlark(this)] this: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let c = this
            .downcast_ref::<StarlarkColor>()
            .ok_or_else(|| anyhow::anyhow!("expected Color"))?;
        let (r, g, b, a) = c.rgba_components();
        Ok(eval
            .heap()
            .alloc(AllocTuple([r as i32, g as i32, b as i32, a as i32])))
    }

    fn hsv<'v>(
        #[starlark(this)] this: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let c = this
            .downcast_ref::<StarlarkColor>()
            .ok_or_else(|| anyhow::anyhow!("expected Color"))?;
        let (h, s, v, _) = c.hsv_components();
        Ok(eval.heap().alloc(AllocTuple([h, s, v])))
    }

    fn hsva<'v>(
        #[starlark(this)] this: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let c = this
            .downcast_ref::<StarlarkColor>()
            .ok_or_else(|| anyhow::anyhow!("expected Color"))?;
        let (h, s, v, a) = c.hsv_components();
        let items = vec![
            eval.heap().alloc(h),
            eval.heap().alloc(s),
            eval.heap().alloc(v),
            eval.heap().alloc(a as i32),
        ];
        Ok(eval.heap().alloc(AllocTuple(items)))
    }
}

fn pack_rgba(r: u8, g: u8, b: u8, a: u8) -> u32 {
    ((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | (a as u32)
}

fn unpack_rgba(rgba: u32) -> (u8, u8, u8, u8) {
    (
        ((rgba >> 24) & 0xff) as u8,
        ((rgba >> 16) & 0xff) as u8,
        ((rgba >> 8) & 0xff) as u8,
        (rgba & 0xff) as u8,
    )
}

fn clamp_u8(value: i32) -> u8 {
    value.clamp(0, 255) as u8
}

fn unpack_numeric(value: Value) -> Option<f64> {
    if let Some(float) = value.downcast_ref::<starlark::values::float::StarlarkFloat>() {
        Some(float.0)
    } else {
        value.unpack_i32().map(|v| v as f64)
    }
}

pub(crate) fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let mut h = h % 360.0;
    if h < 0.0 {
        h += 360.0;
    }
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);

    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r1, g1, b1) = match h {
        h if h < 60.0 => (c, x, 0.0),
        h if h < 120.0 => (x, c, 0.0),
        h if h < 180.0 => (0.0, c, x),
        h if h < 240.0 => (0.0, x, c),
        h if h < 300.0 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    (
        ((r1 + m) * 255.0).round() as u8,
        ((g1 + m) * 255.0).round() as u8,
        ((b1 + m) * 255.0).round() as u8,
    )
}

fn rgba_to_hsv(r: u8, g: u8, b: u8, a: u8) -> (f64, f64, f64, u8) {
    let r = r as f64 / 255.0;
    let g = g as f64 / 255.0;
    let b = b as f64 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        let mut value = 60.0 * ((g - b) / delta);
        if value < 0.0 {
            value += 360.0;
        }
        value
    } else if max == g {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };

    let s = if max == 0.0 { 0.0 } else { delta / max };
    (h, s, max, a)
}

use std::fmt;
use std::sync::Arc;

use allocative::Allocative;
use anyhow::Result;
use starlark::environment::{Methods, MethodsBuilder, MethodsStatic};
use starlark::eval::Evaluator;
use starlark::starlark_simple_value;
use starlark::values::none::NoneType;
use starlark::values::tuple::{AllocTuple, TupleRef};
use starlark::values::{Heap, NoSerialize, ProvidesStaticType, StarlarkValue, Value, ValueLike};
use starlark_derive::starlark_value;

use rustlet_render::{Rect, Widget};

use crate::starlark_bytes::StarlarkBytes;

#[derive(Clone, Debug, Default)]
pub struct RootMeta {
    pub delay: i32,
    pub max_age: i32,
    pub show_full_animation: bool,
}

pub struct SharedWidget(pub Arc<dyn Widget>);

impl Widget for SharedWidget {
    fn paint_bounds(&self, bounds: Rect, frame_idx: i32) -> Rect {
        self.0.paint_bounds(bounds, frame_idx)
    }

    fn paint(&self, pixmap: &mut tiny_skia::Pixmap, bounds: Rect, frame_idx: i32) {
        self.0.paint(pixmap, bounds, frame_idx)
    }

    fn frame_count(&self, bounds: Rect) -> i32 {
        self.0.frame_count(bounds)
    }

    fn size(&self) -> Option<(i32, i32)> {
        self.0.size()
    }
}

#[derive(Clone, Debug)]
pub enum WidgetAttrValue {
    None,
    Bool(bool),
    Int(i32),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Widget(Box<StarlarkWidget>),
    List(Vec<WidgetAttrValue>),
    Tuple(Vec<WidgetAttrValue>),
}

impl WidgetAttrValue {
    fn to_value<'v>(&self, heap: &'v Heap) -> Value<'v> {
        match self {
            WidgetAttrValue::None => Value::new_none(),
            WidgetAttrValue::Bool(v) => heap.alloc(*v),
            WidgetAttrValue::Int(v) => heap.alloc(*v),
            WidgetAttrValue::Float(v) => heap.alloc(*v),
            WidgetAttrValue::String(v) => heap.alloc(v.as_str()),
            WidgetAttrValue::Bytes(v) => heap.alloc(StarlarkBytes { data: v.clone() }),
            WidgetAttrValue::Widget(v) => heap.alloc((**v).clone()),
            WidgetAttrValue::List(values) => {
                let items: Vec<Value<'v>> = values.iter().map(|v| v.to_value(heap)).collect();
                heap.alloc(items)
            }
            WidgetAttrValue::Tuple(values) => {
                let items: Vec<Value<'v>> = values.iter().map(|v| v.to_value(heap)).collect();
                heap.alloc(AllocTuple(items))
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct WidgetAttr {
    pub name: String,
    pub value: WidgetAttrValue,
}

#[derive(Clone, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkWidget {
    #[allocative(skip)]
    inner: Arc<dyn Widget>,
    type_name: String,
    #[allocative(skip)]
    root_meta: Option<RootMeta>,
    #[allocative(skip)]
    attrs: Vec<WidgetAttr>,
}

starlark_simple_value!(StarlarkWidget);

impl StarlarkWidget {
    pub fn new(widget: Box<dyn Widget>, type_name: &str) -> Self {
        Self::new_with_attrs(widget, type_name, Vec::new())
    }

    pub fn new_with_attrs(
        widget: Box<dyn Widget>,
        type_name: &str,
        attrs: Vec<WidgetAttr>,
    ) -> Self {
        Self {
            inner: Arc::from(widget),
            type_name: type_name.to_string(),
            root_meta: None,
            attrs,
        }
    }

    pub fn new_root(widget: Box<dyn Widget>, meta: RootMeta) -> Self {
        Self::new_root_with_attrs(widget, meta, Vec::new())
    }

    pub fn new_root_with_attrs(
        widget: Box<dyn Widget>,
        meta: RootMeta,
        attrs: Vec<WidgetAttr>,
    ) -> Self {
        Self {
            inner: Arc::from(widget),
            type_name: "Root".to_string(),
            root_meta: Some(meta),
            attrs,
        }
    }

    pub fn root_meta(&self) -> Option<&RootMeta> {
        self.root_meta.as_ref()
    }

    pub fn type_name(&self) -> &str {
        &self.type_name
    }

    pub fn take_widget(&self) -> Result<Box<dyn Widget>> {
        Ok(Box::new(SharedWidget(Arc::clone(&self.inner))))
    }

    fn find_attr(&self, name: &str) -> Option<&WidgetAttrValue> {
        self.attrs.iter().find(|a| a.name == name).map(|a| &a.value)
    }
}

impl fmt::Debug for SharedWidget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SharedWidget(..)")
    }
}

impl fmt::Debug for StarlarkWidget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StarlarkWidget({})", self.type_name)
    }
}

impl fmt::Display for StarlarkWidget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}(...)", self.type_name)
    }
}

#[starlark_value(type = "Widget")]
impl<'v> StarlarkValue<'v> for StarlarkWidget {
    fn get_methods() -> Option<&'static Methods> {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(widget_methods)
    }

    fn has_attr(&self, attribute: &str, _heap: &'v Heap) -> bool {
        self.find_attr(attribute).is_some()
    }

    fn dir_attr(&self) -> Vec<String> {
        self.attrs.iter().map(|a| a.name.clone()).collect()
    }

    fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
        self.find_attr(attribute).map(|a| a.to_value(heap))
    }

    fn equals(&self, other: Value<'v>) -> starlark::Result<bool> {
        match other.downcast_ref::<StarlarkWidget>() {
            Some(other) => Ok(self.type_name == other.type_name
                && Arc::ptr_eq(&self.inner, &other.inner)
                && self.root_meta.is_some() == other.root_meta.is_some()),
            None => Ok(false),
        }
    }
}

fn parse_bounds_arg(bounds: Value) -> anyhow::Result<Rect> {
    if bounds.is_none() {
        return Ok(Rect::new(0, 0, 64, 32));
    }
    let tuple =
        TupleRef::from_value(bounds).ok_or_else(|| anyhow::anyhow!("bounds must be a 4-tuple"))?;
    let values = tuple.content();
    if values.len() != 4 {
        return Err(anyhow::anyhow!("bounds must be a 4-tuple"));
    }
    let x0 = values[0]
        .unpack_i32()
        .ok_or_else(|| anyhow::anyhow!("bounds[0] must be int"))?;
    let y0 = values[1]
        .unpack_i32()
        .ok_or_else(|| anyhow::anyhow!("bounds[1] must be int"))?;
    let x1 = values[2]
        .unpack_i32()
        .ok_or_else(|| anyhow::anyhow!("bounds[2] must be int"))?;
    let y1 = values[3]
        .unpack_i32()
        .ok_or_else(|| anyhow::anyhow!("bounds[3] must be int"))?;
    Ok(Rect::new(x0, y0, x1 - x0, y1 - y0))
}

#[starlark::starlark_module]
fn widget_methods(builder: &mut MethodsBuilder) {
    fn size<'v>(
        #[starlark(this)] this: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let widget = this
            .downcast_ref::<StarlarkWidget>()
            .ok_or_else(|| anyhow::anyhow!("expected Widget"))?;
        let (width, height) = widget.inner.size().unwrap_or_else(|| {
            let bounds = widget.inner.paint_bounds(Rect::new(0, 0, 64, 32), 0);
            (bounds.width, bounds.height)
        });
        Ok(eval.heap().alloc(AllocTuple([width, height])))
    }

    fn frame_count(
        #[starlark(this)] this: Value,
        #[starlark(default = NoneType)] bounds: Value,
    ) -> anyhow::Result<i32> {
        let widget = this
            .downcast_ref::<StarlarkWidget>()
            .ok_or_else(|| anyhow::anyhow!("expected Widget"))?;
        let bounds = parse_bounds_arg(bounds)?;
        Ok(widget.inner.frame_count(bounds))
    }
}

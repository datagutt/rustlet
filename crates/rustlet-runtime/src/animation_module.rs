use std::fmt;

use allocative::Allocative;
use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::starlark_simple_value;
use starlark::values::float::StarlarkFloat;
use starlark::values::list::ListRef;
use starlark::values::none::NoneType;
use starlark::values::{Heap, NoSerialize, ProvidesStaticType, StarlarkValue, Value, ValueLike};
use starlark_derive::starlark_value;

use rustlet_render::{
    AnimatedPositioned, Curve, Direction, FillMode, Keyframe, Origin, Rounding, Transform,
    Transformation,
};

use crate::starlark_widgets::{StarlarkWidget, WidgetAttr, WidgetAttrValue};

// Transforms and keyframes are plain data, but they must be passed across the
// Starlark-Rust boundary. We wrap them in custom Starlark values rather than
// using a dict so the `AnimatedPositioned`/`Transformation` constructors can
// downcast them and pull out the actual data.

fn attr(name: &str, value: WidgetAttrValue) -> WidgetAttr {
    WidgetAttr {
        name: name.to_string(),
        value,
    }
}

fn to_f64(value: Value) -> anyhow::Result<f64> {
    if let Some(i) = value.unpack_i32() {
        return Ok(i as f64);
    }
    if let Some(f) = value.downcast_ref::<StarlarkFloat>() {
        return Ok(f.0);
    }
    Err(anyhow::anyhow!(
        "expected number, got {}",
        value.get_type()
    ))
}

fn parse_curve_value(value: Value<'_>) -> anyhow::Result<Curve> {
    if value.is_none() {
        return Ok(Curve::Linear);
    }
    if let Some(s) = value.unpack_str() {
        return Curve::parse(s).ok_or_else(|| anyhow::anyhow!("unknown curve: {s}"));
    }
    if let Some(c) = value.downcast_ref::<StarlarkCurve>() {
        return Ok(c.0);
    }
    Err(anyhow::anyhow!(
        "curve must be a string or curve value, got {}",
        value.get_type()
    ))
}

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkTransform(#[allocative(skip)] pub Transform);

starlark_simple_value!(StarlarkTransform);

impl fmt::Display for StarlarkTransform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

#[starlark_value(type = "animation.Transform")]
impl<'v> StarlarkValue<'v> for StarlarkTransform {
    fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match (&self.0, attribute) {
            (Transform::Translate { x, .. }, "x") => Some(heap.alloc(StarlarkFloat(*x))),
            (Transform::Translate { y, .. }, "y") => Some(heap.alloc(StarlarkFloat(*y))),
            (Transform::Rotate { angle }, "angle") => Some(heap.alloc(StarlarkFloat(*angle))),
            (Transform::Scale { x, .. }, "x") => Some(heap.alloc(StarlarkFloat(*x))),
            (Transform::Scale { y, .. }, "y") => Some(heap.alloc(StarlarkFloat(*y))),
            (Transform::Shear { x_angle, .. }, "x_angle") => {
                Some(heap.alloc(StarlarkFloat(*x_angle)))
            }
            (Transform::Shear { y_angle, .. }, "y_angle") => {
                Some(heap.alloc(StarlarkFloat(*y_angle)))
            }
            _ => None,
        }
    }
}

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkKeyframe {
    pub percentage: f64,
    #[allocative(skip)]
    pub transforms: Vec<Transform>,
    #[allocative(skip)]
    pub curve: Curve,
}

starlark_simple_value!(StarlarkKeyframe);

impl fmt::Display for StarlarkKeyframe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Keyframe({})", self.percentage)
    }
}

#[starlark_value(type = "animation.Keyframe")]
impl<'v> StarlarkValue<'v> for StarlarkKeyframe {
    fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match attribute {
            "percentage" => Some(heap.alloc(StarlarkFloat(self.percentage))),
            _ => None,
        }
    }
}

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkOrigin(#[allocative(skip)] pub Origin);

starlark_simple_value!(StarlarkOrigin);

impl fmt::Display for StarlarkOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Origin({}, {})", self.0.x, self.0.y)
    }
}

#[starlark_value(type = "animation.Origin")]
impl<'v> StarlarkValue<'v> for StarlarkOrigin {
    fn get_attr(&self, attribute: &str, heap: &'v Heap) -> Option<Value<'v>> {
        match attribute {
            "x" => Some(heap.alloc(StarlarkFloat(self.0.x))),
            "y" => Some(heap.alloc(StarlarkFloat(self.0.y))),
            _ => None,
        }
    }
}

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkCurve(#[allocative(skip)] pub Curve);

starlark_simple_value!(StarlarkCurve);

impl fmt::Display for StarlarkCurve {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Curve({:?})", self.0)
    }
}

#[starlark_value(type = "animation.Curve")]
impl<'v> StarlarkValue<'v> for StarlarkCurve {}

fn collect_transforms(value: Value<'_>) -> anyhow::Result<Vec<Transform>> {
    if value.is_none() {
        return Ok(Vec::new());
    }
    let list = ListRef::from_value(value)
        .ok_or_else(|| anyhow::anyhow!("transforms must be a list"))?;
    let mut result = Vec::with_capacity(list.len());
    for item in list.iter() {
        let t = item
            .downcast_ref::<StarlarkTransform>()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "expected animation.Transform, got {}",
                    item.get_type()
                )
            })?;
        result.push(t.0);
    }
    Ok(result)
}

fn collect_keyframes(value: Value<'_>) -> anyhow::Result<Vec<Keyframe>> {
    if value.is_none() {
        return Ok(Vec::new());
    }
    let list = ListRef::from_value(value)
        .ok_or_else(|| anyhow::anyhow!("keyframes must be a list"))?;
    let mut result = Vec::with_capacity(list.len());
    for item in list.iter() {
        let kf = item.downcast_ref::<StarlarkKeyframe>().ok_or_else(|| {
            anyhow::anyhow!("expected animation.Keyframe, got {}", item.get_type())
        })?;
        result.push(Keyframe {
            percentage: kf.percentage,
            transforms: kf.transforms.clone(),
            curve: kf.curve,
        });
    }
    Ok(result)
}

fn parse_origin(value: Value<'_>) -> anyhow::Result<Origin> {
    if value.is_none() {
        return Ok(Origin::default());
    }
    if let Some(o) = value.downcast_ref::<StarlarkOrigin>() {
        return Ok(o.0);
    }
    Err(anyhow::anyhow!(
        "origin must be an animation.Origin, got {}",
        value.get_type()
    ))
}

#[starlark::starlark_module]
pub fn animation_module(builder: &mut GlobalsBuilder) {
    fn AnimatedPositioned<'v>(
        child: Value<'v>,
        duration: i32,
        #[starlark(default = NoneType)] curve: Value<'v>,
        #[starlark(default = 0)] x_start: i32,
        #[starlark(default = 0)] x_end: i32,
        #[starlark(default = 0)] y_start: i32,
        #[starlark(default = 0)] y_end: i32,
        #[starlark(default = 0)] delay: i32,
        #[starlark(default = 0)] hold: i32,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let parsed_curve = parse_curve_value(curve)?;
        let child_widget = StarlarkWidget::from_value(child)
            .ok_or_else(|| anyhow::anyhow!("child must be a widget, got {}", child.get_type()))?
            .take_widget()?;

        let widget = AnimatedPositioned {
            child: child_widget,
            x_start,
            y_start,
            x_end,
            y_end,
            duration,
            curve: parsed_curve,
            delay,
            hold,
        };
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(widget),
            "AnimatedPositioned",
            vec![
                attr("duration", WidgetAttrValue::Int(duration)),
                attr("delay", WidgetAttrValue::Int(delay)),
                attr("hold", WidgetAttrValue::Int(hold)),
                attr("x_start", WidgetAttrValue::Int(x_start)),
                attr("x_end", WidgetAttrValue::Int(x_end)),
                attr("y_start", WidgetAttrValue::Int(y_start)),
                attr("y_end", WidgetAttrValue::Int(y_end)),
            ],
        )))
    }

    fn Transformation<'v>(
        child: Value<'v>,
        keyframes: Value<'v>,
        duration: i32,
        #[starlark(default = 0)] delay: i32,
        #[starlark(default = 0)] width: i32,
        #[starlark(default = 0)] height: i32,
        #[starlark(default = NoneType)] origin: Value<'v>,
        #[starlark(default = "")] direction: &str,
        #[starlark(default = "")] fill_mode: &str,
        #[starlark(default = "")] rounding: &str,
        #[starlark(default = false)] wait_for_child: bool,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let kfs = collect_keyframes(keyframes)?;
        let origin_val = parse_origin(origin)?;
        let direction_val = Direction::from_str(direction)
            .ok_or_else(|| anyhow::anyhow!("invalid direction: {direction}"))?;
        let fill_mode_val = FillMode::from_str(fill_mode)
            .ok_or_else(|| anyhow::anyhow!("invalid fill_mode: {fill_mode}"))?;
        let rounding_val = Rounding::from_str(rounding)
            .ok_or_else(|| anyhow::anyhow!("invalid rounding: {rounding}"))?;
        let child_widget = StarlarkWidget::from_value(child)
            .ok_or_else(|| anyhow::anyhow!("child must be a widget, got {}", child.get_type()))?
            .take_widget()?;

        let mut widget = Transformation::new(child_widget, kfs, duration);
        widget.delay = delay;
        widget.width = width;
        widget.height = height;
        widget.origin = origin_val;
        widget.direction = direction_val;
        widget.fill_mode = fill_mode_val;
        widget.rounding = rounding_val;
        widget.wait_for_child = wait_for_child;

        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(widget),
            "Transformation",
            vec![
                attr("duration", WidgetAttrValue::Int(duration)),
                attr("delay", WidgetAttrValue::Int(delay)),
                attr("width", WidgetAttrValue::Int(width)),
                attr("height", WidgetAttrValue::Int(height)),
                attr("direction", WidgetAttrValue::String(direction.to_string())),
                attr("fill_mode", WidgetAttrValue::String(fill_mode.to_string())),
                attr("rounding", WidgetAttrValue::String(rounding.to_string())),
                attr("wait_for_child", WidgetAttrValue::Bool(wait_for_child)),
            ],
        )))
    }

    fn Keyframe<'v>(
        percentage: Value<'v>,
        transforms: Value<'v>,
        #[starlark(default = NoneType)] curve: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let pct = to_f64(percentage)?;
        let transforms = collect_transforms(transforms)?;
        let curve_val = parse_curve_value(curve)?;
        Ok(eval.heap().alloc(StarlarkKeyframe {
            percentage: pct,
            transforms,
            curve: curve_val,
        }))
    }

    fn Origin<'v>(
        x: Value<'v>,
        y: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let x = to_f64(x)?;
        let y = to_f64(y)?;
        Ok(eval.heap().alloc(StarlarkOrigin(Origin { x, y })))
    }

    fn Translate<'v>(
        x: Value<'v>,
        y: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let x = to_f64(x)?;
        let y = to_f64(y)?;
        Ok(eval
            .heap()
            .alloc(StarlarkTransform(Transform::Translate { x, y })))
    }

    fn Rotate<'v>(
        angle: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let angle = to_f64(angle)?;
        Ok(eval
            .heap()
            .alloc(StarlarkTransform(Transform::Rotate { angle })))
    }

    fn Scale<'v>(
        x: Value<'v>,
        y: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let x = to_f64(x)?;
        let y = to_f64(y)?;
        Ok(eval
            .heap()
            .alloc(StarlarkTransform(Transform::Scale { x, y })))
    }

    fn Shear<'v>(
        #[starlark(default = NoneType)] x_angle: Value<'v>,
        #[starlark(default = NoneType)] y_angle: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let x_angle = if x_angle.is_none() { 0.0 } else { to_f64(x_angle)? };
        let y_angle = if y_angle.is_none() { 0.0 } else { to_f64(y_angle)? };
        Ok(eval
            .heap()
            .alloc(StarlarkTransform(Transform::Shear { x_angle, y_angle })))
    }
}

pub fn build_animation_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(animation_module)
        .build()
}

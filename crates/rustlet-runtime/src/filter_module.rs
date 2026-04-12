//! Starlark bindings for `filter.star`. Constructors mirror pixlet's
//! `filter_runtime` module and wrap a child widget with an image-processing
//! filter implemented in rustlet-render.

use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::values::float::StarlarkFloat;
use starlark::values::{Value, ValueLike};

use rustlet_render::{
    FilterBlur, FilterBrightness, FilterContrast, FilterEdgeDetection, FilterEmboss,
    FilterFlipHorizontal, FilterFlipVertical, FilterGamma, FilterGrayscale, FilterHue,
    FilterInvert, FilterRotate, FilterSaturation, FilterSepia, FilterShear, FilterSharpen,
    FilterThreshold, Widget,
};

use crate::starlark_widgets::{StarlarkWidget, WidgetAttr, WidgetAttrValue};

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

fn attr(name: &str, value: WidgetAttrValue) -> WidgetAttr {
    WidgetAttr {
        name: name.to_string(),
        value,
    }
}

fn take_child<'v>(child: Value<'v>) -> anyhow::Result<Box<dyn Widget>> {
    StarlarkWidget::from_value(child)
        .ok_or_else(|| anyhow::anyhow!("child must be a widget, got {}", child.get_type()))?
        .take_widget()
}

#[starlark::starlark_module]
pub fn filter_module(builder: &mut GlobalsBuilder) {
    fn Blur<'v>(
        child: Value<'v>,
        radius: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let radius = to_f64(radius)?; let w = FilterBlur {
            child: take_child(child)?,
            radius,
        };
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(w),
            "Blur",
            vec![attr("radius", WidgetAttrValue::String(radius.to_string()))],
        )))
    }

    fn Brightness<'v>(
        child: Value<'v>,
        change: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let change = to_f64(change)?;
        let w = FilterBrightness {
            child: take_child(child)?,
            change,
        };
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(w),
            "Brightness",
            vec![attr("change", WidgetAttrValue::String(change.to_string()))],
        )))
    }

    fn Contrast<'v>(
        child: Value<'v>,
        change: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let change = to_f64(change)?;
        let w = FilterContrast {
            child: take_child(child)?,
            change,
        };
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(w),
            "Contrast",
            vec![attr("change", WidgetAttrValue::String(change.to_string()))],
        )))
    }

    fn EdgeDetection<'v>(
        child: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let w = FilterEdgeDetection {
            child: take_child(child)?,
        };
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(w),
            "EdgeDetection",
            vec![],
        )))
    }

    fn Emboss<'v>(
        child: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let w = FilterEmboss {
            child: take_child(child)?,
        };
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(w),
            "Emboss",
            vec![],
        )))
    }

    fn FlipHorizontal<'v>(
        child: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let w = FilterFlipHorizontal {
            child: take_child(child)?,
        };
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(w),
            "FlipHorizontal",
            vec![],
        )))
    }

    fn FlipVertical<'v>(
        child: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let w = FilterFlipVertical {
            child: take_child(child)?,
        };
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(w),
            "FlipVertical",
            vec![],
        )))
    }

    fn Gamma<'v>(
        child: Value<'v>,
        gamma: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let gamma = to_f64(gamma)?;
        let w = FilterGamma {
            child: take_child(child)?,
            gamma,
        };
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(w),
            "Gamma",
            vec![attr("gamma", WidgetAttrValue::String(gamma.to_string()))],
        )))
    }

    fn Grayscale<'v>(
        child: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let w = FilterGrayscale {
            child: take_child(child)?,
        };
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(w),
            "Grayscale",
            vec![],
        )))
    }

    fn Hue<'v>(
        child: Value<'v>,
        change: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let change = to_f64(change)?;
        let w = FilterHue {
            child: take_child(child)?,
            change,
        };
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(w),
            "Hue",
            vec![attr("change", WidgetAttrValue::String(change.to_string()))],
        )))
    }

    fn Invert<'v>(
        child: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let w = FilterInvert {
            child: take_child(child)?,
        };
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(w),
            "Invert",
            vec![],
        )))
    }

    fn Rotate<'v>(
        child: Value<'v>,
        angle: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let angle = to_f64(angle)?;
        let w = FilterRotate {
            child: take_child(child)?,
            angle,
        };
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(w),
            "Rotate",
            vec![attr("angle", WidgetAttrValue::String(angle.to_string()))],
        )))
    }

    fn Saturation<'v>(
        child: Value<'v>,
        change: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let change = to_f64(change)?;
        let w = FilterSaturation {
            child: take_child(child)?,
            change,
        };
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(w),
            "Saturation",
            vec![attr("change", WidgetAttrValue::String(change.to_string()))],
        )))
    }

    fn Sepia<'v>(
        child: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let w = FilterSepia {
            child: take_child(child)?,
        };
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(w),
            "Sepia",
            vec![],
        )))
    }

    fn Sharpen<'v>(
        child: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let w = FilterSharpen {
            child: take_child(child)?,
        };
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(w),
            "Sharpen",
            vec![],
        )))
    }

    fn Shear<'v>(
        child: Value<'v>,
        #[starlark(default = 0)] x_angle: Value<'v>,
        #[starlark(default = 0)] y_angle: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let x_angle = to_f64(x_angle)?;
        let y_angle = to_f64(y_angle)?;
        let w = FilterShear {
            child: take_child(child)?,
            x_angle,
            y_angle,
        };
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(w),
            "Shear",
            vec![
                attr("x_angle", WidgetAttrValue::String(x_angle.to_string())),
                attr("y_angle", WidgetAttrValue::String(y_angle.to_string())),
            ],
        )))
    }

    fn Threshold<'v>(
        child: Value<'v>,
        level: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let level = to_f64(level)?;
        let w = FilterThreshold {
            child: take_child(child)?,
            level,
        };
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(w),
            "Threshold",
            vec![attr("level", WidgetAttrValue::String(level.to_string()))],
        )))
    }
}

pub fn build_filter_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(filter_module)
        .build()
}

use starlark::eval::Evaluator;
use starlark::environment::GlobalsBuilder;
use starlark::values::list::ListRef;
use starlark::values::none::NoneType;
use starlark::values::Value;

use rustlet_render::{
    parse_color, Animation, BoxWidget, Circle, Column, ImageWidget, Insets, Marquee, MarqueeAlign,
    Padding, Row, ScrollDirection, Sequence, Stack, Text, Widget, WrappedText, WrapAlign,
    CrossAlign, MainAlign,
};

use crate::starlark_widgets::{RootMeta, StarlarkWidget};

/// Extract child widgets from a Starlark list Value.
fn extract_children(children_val: Value) -> anyhow::Result<Vec<Box<dyn Widget>>> {
    if children_val.is_none() {
        return Ok(Vec::new());
    }
    let list = ListRef::from_value(children_val)
        .ok_or_else(|| anyhow::anyhow!("children must be a list"))?;
    let mut out = Vec::with_capacity(list.len());
    for item in list.iter() {
        let sw = StarlarkWidget::from_value(item)
            .ok_or_else(|| anyhow::anyhow!("child must be a Widget, got {}", item.get_type()))?;
        out.push(sw.take_widget()?);
    }
    Ok(out)
}

/// Extract an optional child widget from a Starlark Value.
fn extract_optional_child(child_val: Value) -> anyhow::Result<Option<Box<dyn Widget>>> {
    if child_val.is_none() {
        return Ok(None);
    }
    let sw = StarlarkWidget::from_value(child_val)
        .ok_or_else(|| anyhow::anyhow!("child must be a Widget, got {}", child_val.get_type()))?;
    Ok(Some(sw.take_widget()?))
}

/// Extract a required child widget from a Starlark Value.
fn extract_child(child_val: Value) -> anyhow::Result<Box<dyn Widget>> {
    let sw = StarlarkWidget::from_value(child_val)
        .ok_or_else(|| anyhow::anyhow!("child must be a Widget, got {}", child_val.get_type()))?;
    sw.take_widget()
}

/// Parse color string, returning None for empty string.
fn parse_optional_color(s: &str) -> anyhow::Result<Option<tiny_skia::Color>> {
    if s.is_empty() {
        Ok(None)
    } else {
        Ok(Some(parse_color(s)?))
    }
}

#[starlark::starlark_module]
pub fn render_module(builder: &mut GlobalsBuilder) {
    fn Root<'v>(
        child: Value<'v>,
        #[starlark(default = 0)] delay: i32,
        #[starlark(default = 0)] max_age: i32,
        #[starlark(default = false)] show_full_animation: bool,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let child_widget = extract_child(child)?;
        let meta = RootMeta {
            delay,
            max_age,
            show_full_animation,
        };
        Ok(eval.heap().alloc(StarlarkWidget::new_root(child_widget, meta)))
    }

    #[allow(non_snake_case)]
    fn r#Box<'v>(
        #[starlark(default = NoneType)] child: Value<'v>,
        #[starlark(default = 0)] width: i32,
        #[starlark(default = 0)] height: i32,
        #[starlark(default = 0)] padding: i32,
        #[starlark(default = "")] color: &str,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let widget = BoxWidget {
            child: extract_optional_child(child)?,
            width,
            height,
            padding,
            color: parse_optional_color(color)?,
        };
        Ok(eval.heap().alloc(StarlarkWidget::new(Box::new(widget), "Box")))
    }

    fn Text<'v>(
        content: &str,
        #[starlark(default = "")] font: &str,
        #[starlark(default = 0)] height: i32,
        #[starlark(default = 0)] offset: i32,
        #[starlark(default = "")] color: &str,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let mut t = Text::new(content);
        if !font.is_empty() {
            t = t.with_font(font);
        }
        if height > 0 {
            t = t.with_height(height);
        }
        if offset != 0 {
            t = t.with_offset(offset);
        }
        if !color.is_empty() {
            t = t.with_color(parse_color(color)?);
        }
        Ok(eval.heap().alloc(StarlarkWidget::new(Box::new(t), "Text")))
    }

    fn Row<'v>(
        #[starlark(default = NoneType)] children: Value<'v>,
        #[starlark(default = "")] main_align: &str,
        #[starlark(default = "")] cross_align: &str,
        #[starlark(default = false)] expanded: bool,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let kids = extract_children(children)?;
        let widget = Row::new(
            kids,
            MainAlign::from_str(main_align),
            CrossAlign::from_str(cross_align),
            expanded,
        );
        Ok(eval.heap().alloc(StarlarkWidget::new(Box::new(widget), "Row")))
    }

    fn Column<'v>(
        #[starlark(default = NoneType)] children: Value<'v>,
        #[starlark(default = "")] main_align: &str,
        #[starlark(default = "")] cross_align: &str,
        #[starlark(default = false)] expanded: bool,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let kids = extract_children(children)?;
        let widget = Column::new(
            kids,
            MainAlign::from_str(main_align),
            CrossAlign::from_str(cross_align),
            expanded,
        );
        Ok(eval.heap().alloc(StarlarkWidget::new(Box::new(widget), "Column")))
    }

    fn Stack<'v>(
        #[starlark(default = NoneType)] children: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let kids = extract_children(children)?;
        let widget = Stack { children: kids };
        Ok(eval.heap().alloc(StarlarkWidget::new(Box::new(widget), "Stack")))
    }

    fn Padding<'v>(
        child: Value<'v>,
        #[starlark(default = 0)] pad: i32,
        #[starlark(default = false)] expanded: bool,
        #[starlark(default = "")] color: &str,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let widget = Padding {
            child: extract_child(child)?,
            pad: Insets::uniform(pad),
            expanded,
            color: parse_optional_color(color)?,
        };
        Ok(eval.heap().alloc(StarlarkWidget::new(Box::new(widget), "Padding")))
    }

    fn Marquee<'v>(
        child: Value<'v>,
        #[starlark(default = 0)] width: i32,
        #[starlark(default = 0)] height: i32,
        #[starlark(default = 0)] offset_start: i32,
        #[starlark(default = 0)] offset_end: i32,
        #[starlark(default = "")] scroll_direction: &str,
        #[starlark(default = "")] align: &str,
        #[starlark(default = 0)] delay: i32,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let widget = Marquee {
            child: extract_child(child)?,
            width,
            height,
            offset_start,
            offset_end,
            scroll_direction: ScrollDirection::from_str(scroll_direction),
            align: MarqueeAlign::from_str(align),
            delay,
        };
        Ok(eval.heap().alloc(StarlarkWidget::new(Box::new(widget), "Marquee")))
    }

    fn Animation<'v>(
        #[starlark(default = NoneType)] children: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let kids = extract_children(children)?;
        let widget = Animation { children: kids };
        Ok(eval.heap().alloc(StarlarkWidget::new(Box::new(widget), "Animation")))
    }

    fn Sequence<'v>(
        #[starlark(default = NoneType)] children: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let kids = extract_children(children)?;
        let widget = Sequence { children: kids };
        Ok(eval.heap().alloc(StarlarkWidget::new(Box::new(widget), "Sequence")))
    }

    fn Image<'v>(
        src: &str,
        #[starlark(default = 0)] width: i32,
        #[starlark(default = 0)] height: i32,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, src)?;
        let w = if width > 0 { Some(width) } else { None };
        let h = if height > 0 { Some(height) } else { None };
        let widget = ImageWidget::from_bytes(&bytes, w, h)?;
        Ok(eval.heap().alloc(StarlarkWidget::new(Box::new(widget), "Image")))
    }

    fn Circle<'v>(
        #[starlark(default = NoneType)] child: Value<'v>,
        #[starlark(default = "")] color: &str,
        #[starlark(default = 0)] diameter: i32,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let mut widget = Circle::new(diameter);
        widget.child = extract_optional_child(child)?;
        widget.color = parse_optional_color(color)?;
        Ok(eval.heap().alloc(StarlarkWidget::new(Box::new(widget), "Circle")))
    }

    fn WrappedText<'v>(
        content: &str,
        #[starlark(default = "")] font: &str,
        #[starlark(default = 0)] width: i32,
        #[starlark(default = 0)] height: i32,
        #[starlark(default = 0)] line_spacing: i32,
        #[starlark(default = "")] color: &str,
        #[starlark(default = "")] align: &str,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let mut wt = WrappedText::new(content);
        if !font.is_empty() {
            wt = wt.with_font(font);
        }
        if width > 0 {
            wt = wt.with_width(width);
        }
        if height > 0 {
            wt = wt.with_height(height);
        }
        if line_spacing > 0 {
            wt = wt.with_line_spacing(line_spacing);
        }
        if !color.is_empty() {
            wt = wt.with_color(parse_color(color)?);
        }
        if !align.is_empty() {
            wt = wt.with_align(WrapAlign::from_str(align));
        }
        Ok(eval.heap().alloc(StarlarkWidget::new(Box::new(wt), "WrappedText")))
    }
}

/// Build a Globals containing the render module constructors.
pub fn build_render_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(render_module)
        .build()
}

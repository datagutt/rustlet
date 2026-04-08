use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::values::float::StarlarkFloat;
use starlark::values::list::ListRef;
use starlark::values::none::NoneType;
use starlark::values::tuple::TupleRef;
use starlark::values::{Value, ValueLike};

use rustlet_render::{
    Animation, Arc, BoxWidget, ChartType, Circle, Column, CrossAlign, Emoji, ImageWidget, Insets,
    Line, MainAlign, Marquee, MarqueeAlign, Padding, PieChart, Plot, Polygon, Row, ScrollDirection,
    Sequence, Stack, Text, Widget, WrapAlign, WrappedText,
};

use crate::starlark_bytes::StarlarkBytes;
use crate::starlark_color::StarlarkColor;
use crate::starlark_widgets::{RootMeta, StarlarkWidget, WidgetAttr, WidgetAttrValue};

thread_local! {
    static RENDER_IS_2X: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn attr(name: &str, value: WidgetAttrValue) -> WidgetAttr {
    WidgetAttr {
        name: name.to_string(),
        value,
    }
}

pub(crate) fn set_render_context(is_2x: bool) {
    RENDER_IS_2X.with(|cell| cell.set(is_2x));
}

fn default_font(font: &str) -> &str {
    if !font.is_empty() {
        return font;
    }

    RENDER_IS_2X.with(|cell| {
        if cell.get() {
            "terminus-16"
        } else {
            rustlet_render::fonts::DEFAULT_FONT
        }
    })
}

fn clone_widget_attr(value: Value) -> anyhow::Result<WidgetAttrValue> {
    if value.is_none() {
        return Ok(WidgetAttrValue::None);
    }
    let widget = StarlarkWidget::from_value(value)
        .ok_or_else(|| anyhow::anyhow!("expected Widget, got {}", value.get_type()))?;
    Ok(WidgetAttrValue::Widget(Box::new(widget.clone())))
}

fn value_to_attr(value: Value) -> anyhow::Result<WidgetAttrValue> {
    if value.is_none() {
        Ok(WidgetAttrValue::None)
    } else if let Some(v) = value.unpack_i32() {
        Ok(WidgetAttrValue::Int(v))
    } else if let Some(v) = value.unpack_bool() {
        Ok(WidgetAttrValue::Bool(v))
    } else if let Some(v) = value.unpack_str() {
        Ok(WidgetAttrValue::String(v.to_string()))
    } else if let Some(v) = value.downcast_ref::<StarlarkBytes>() {
        Ok(WidgetAttrValue::Bytes(v.data.clone()))
    } else if StarlarkWidget::from_value(value).is_some() {
        clone_widget_attr(value)
    } else if let Some(tuple) = TupleRef::from_value(value) {
        tuple
            .content()
            .iter()
            .map(|v| value_to_attr(*v))
            .collect::<anyhow::Result<Vec<_>>>()
            .map(WidgetAttrValue::Tuple)
    } else if let Some(list) = ListRef::from_value(value) {
        list.iter()
            .map(value_to_attr)
            .collect::<anyhow::Result<Vec<_>>>()
            .map(WidgetAttrValue::List)
    } else if let Some(color) = value.downcast_ref::<StarlarkColor>() {
        Ok(WidgetAttrValue::String(color.hex()))
    } else {
        Err(anyhow::anyhow!(
            "unsupported attribute value type {}",
            value.get_type()
        ))
    }
}

fn extract_children(
    children_val: Value,
) -> anyhow::Result<(Vec<Box<dyn Widget>>, WidgetAttrValue)> {
    if children_val.is_none() {
        return Ok((Vec::new(), WidgetAttrValue::List(Vec::new())));
    }
    let list = ListRef::from_value(children_val)
        .ok_or_else(|| anyhow::anyhow!("children must be a list"))?;
    let mut widgets = Vec::with_capacity(list.len());
    let mut attrs = Vec::with_capacity(list.len());
    for item in list.iter() {
        let sw = StarlarkWidget::from_value(item)
            .ok_or_else(|| anyhow::anyhow!("child must be a Widget, got {}", item.get_type()))?;
        widgets.push(sw.take_widget()?);
        attrs.push(WidgetAttrValue::Widget(Box::new(sw.clone())));
    }
    Ok((widgets, WidgetAttrValue::List(attrs)))
}

fn extract_optional_child(
    child_val: Value,
) -> anyhow::Result<(Option<Box<dyn Widget>>, WidgetAttrValue)> {
    if child_val.is_none() {
        return Ok((None, WidgetAttrValue::None));
    }
    let sw = StarlarkWidget::from_value(child_val)
        .ok_or_else(|| anyhow::anyhow!("child must be a Widget, got {}", child_val.get_type()))?;
    Ok((
        Some(sw.take_widget()?),
        WidgetAttrValue::Widget(Box::new(sw.clone())),
    ))
}

fn extract_child(child_val: Value) -> anyhow::Result<(Box<dyn Widget>, WidgetAttrValue)> {
    let (child, attr) = extract_optional_child(child_val)?;
    child
        .map(|child| (child, attr))
        .ok_or_else(|| anyhow::anyhow!("child must be provided"))
}

fn extract_color(v: Value) -> anyhow::Result<Option<tiny_skia::Color>> {
    StarlarkColor::color_from_value(v)
}

fn color_attr(v: Value) -> anyhow::Result<WidgetAttrValue> {
    if v.is_none() {
        Ok(WidgetAttrValue::None)
    } else if let Some(s) = v.unpack_str() {
        Ok(WidgetAttrValue::String(s.to_string()))
    } else if let Some(color) = v.downcast_ref::<StarlarkColor>() {
        Ok(WidgetAttrValue::String(color.hex()))
    } else {
        Err(anyhow::anyhow!("color must be a Color or string"))
    }
}

fn extract_insets(v: Value) -> anyhow::Result<Insets> {
    if v.is_none() {
        return Ok(Insets::uniform(0));
    }
    if let Some(n) = v.unpack_i32() {
        return Ok(Insets::uniform(n));
    }
    if let Some(t) = TupleRef::from_value(v) {
        let c = t.content();
        if c.len() != 4 {
            return Err(anyhow::anyhow!(
                "pad tuple must have 4 elements, got {}",
                c.len()
            ));
        }
        let left = c[0]
            .unpack_i32()
            .ok_or_else(|| anyhow::anyhow!("pad tuple values must be int"))?;
        let top = c[1]
            .unpack_i32()
            .ok_or_else(|| anyhow::anyhow!("pad tuple values must be int"))?;
        let right = c[2]
            .unpack_i32()
            .ok_or_else(|| anyhow::anyhow!("pad tuple values must be int"))?;
        let bottom = c[3]
            .unpack_i32()
            .ok_or_else(|| anyhow::anyhow!("pad tuple values must be int"))?;
        return Ok(Insets::new(left, top, right, bottom));
    }
    Err(anyhow::anyhow!(
        "pad must be an int or tuple of 4 ints, got {}",
        v.get_type()
    ))
}

fn to_f64(value: Value) -> anyhow::Result<f64> {
    if let Some(i) = value.unpack_i32() {
        return Ok(i as f64);
    }
    if let Some(f) = value.downcast_ref::<StarlarkFloat>() {
        return Ok(f.0);
    }
    Err(anyhow::anyhow!("expected number, got {}", value.get_type()))
}

fn to_f32(value: Value) -> anyhow::Result<f32> {
    Ok(to_f64(value)? as f32)
}

fn float_tuple(value: Value) -> anyhow::Result<[f64; 2]> {
    let tuple = TupleRef::from_value(value).ok_or_else(|| anyhow::anyhow!("expected a 2-tuple"))?;
    let values = tuple.content();
    if values.len() != 2 {
        return Err(anyhow::anyhow!("expected a 2-tuple"));
    }
    Ok([to_f64(values[0])?, to_f64(values[1])?])
}

fn extract_plot_data(value: Value) -> anyhow::Result<(Vec<[f64; 2]>, WidgetAttrValue)> {
    let list = ListRef::from_value(value).ok_or_else(|| anyhow::anyhow!("data must be a list"))?;
    let mut data = Vec::with_capacity(list.len());
    let mut attrs = Vec::with_capacity(list.len());
    for item in list.iter() {
        let pair = float_tuple(item)?;
        data.push(pair);
        attrs.push(WidgetAttrValue::Tuple(vec![
            WidgetAttrValue::Float(pair[0]),
            WidgetAttrValue::Float(pair[1]),
        ]));
    }
    Ok((data, WidgetAttrValue::List(attrs)))
}

fn extract_float_list(value: Value, field: &str) -> anyhow::Result<(Vec<f64>, WidgetAttrValue)> {
    let list =
        ListRef::from_value(value).ok_or_else(|| anyhow::anyhow!("{field} must be a list"))?;
    let mut values_out = Vec::with_capacity(list.len());
    let mut attrs = Vec::with_capacity(list.len());
    for item in list.iter() {
        let n = to_f64(item)?;
        values_out.push(n);
        attrs.push(WidgetAttrValue::Float(n));
    }
    Ok((values_out, WidgetAttrValue::List(attrs)))
}

fn extract_color_list(
    value: Value,
    field: &str,
) -> anyhow::Result<(Vec<tiny_skia::Color>, WidgetAttrValue)> {
    let list =
        ListRef::from_value(value).ok_or_else(|| anyhow::anyhow!("{field} must be a list"))?;
    let mut colors = Vec::with_capacity(list.len());
    let mut attrs = Vec::with_capacity(list.len());
    for item in list.iter() {
        let color =
            extract_color(item)?.ok_or_else(|| anyhow::anyhow!("{field} colors cannot be None"))?;
        colors.push(color);
        attrs.push(color_attr(item)?);
    }
    Ok((colors, WidgetAttrValue::List(attrs)))
}

fn extract_vertices(value: Value) -> anyhow::Result<(Vec<(f64, f64)>, WidgetAttrValue)> {
    let list =
        ListRef::from_value(value).ok_or_else(|| anyhow::anyhow!("vertices must be a list"))?;
    let mut vertices = Vec::with_capacity(list.len());
    let mut attrs = Vec::with_capacity(list.len());
    for item in list.iter() {
        let pair = float_tuple(item)?;
        vertices.push((pair[0], pair[1]));
        attrs.push(WidgetAttrValue::Tuple(vec![
            WidgetAttrValue::Float(pair[0]),
            WidgetAttrValue::Float(pair[1]),
        ]));
    }
    Ok((vertices, WidgetAttrValue::List(attrs)))
}

fn extract_optional_range(value: Value) -> anyhow::Result<(Option<[f64; 2]>, WidgetAttrValue)> {
    if value.is_none() {
        return Ok((None, WidgetAttrValue::None));
    }
    let range = float_tuple(value)?;
    Ok((
        Some(range),
        WidgetAttrValue::Tuple(vec![
            WidgetAttrValue::Float(range[0]),
            WidgetAttrValue::Float(range[1]),
        ]),
    ))
}

fn extract_blob(value: Value) -> anyhow::Result<(Vec<u8>, WidgetAttrValue)> {
    if let Some(s) = value.unpack_str() {
        return Ok((
            s.as_bytes().to_vec(),
            WidgetAttrValue::String(s.to_string()),
        ));
    }
    if let Some(bytes) = value.downcast_ref::<StarlarkBytes>() {
        return Ok((
            bytes.data.clone(),
            WidgetAttrValue::Bytes(bytes.data.clone()),
        ));
    }
    Err(anyhow::anyhow!(
        "expected string or bytes, got {}",
        value.get_type()
    ))
}

fn main_align_name(align: MainAlign) -> &'static str {
    match align {
        MainAlign::Start => "start",
        MainAlign::End => "end",
        MainAlign::Center => "center",
        MainAlign::SpaceBetween => "space_between",
        MainAlign::SpaceAround => "space_around",
        MainAlign::SpaceEvenly => "space_evenly",
    }
}

fn cross_align_name(align: CrossAlign) -> &'static str {
    match align {
        CrossAlign::Start => "start",
        CrossAlign::Center => "center",
        CrossAlign::End => "end",
    }
}

fn wrap_align_name(align: WrapAlign) -> &'static str {
    match align {
        WrapAlign::Left => "left",
        WrapAlign::Center => "center",
        WrapAlign::Right => "right",
    }
}

fn scroll_direction_name(direction: ScrollDirection) -> &'static str {
    match direction {
        ScrollDirection::Horizontal => "horizontal",
        ScrollDirection::Vertical => "vertical",
    }
}

fn marquee_align_name(align: MarqueeAlign) -> &'static str {
    match align {
        MarqueeAlign::Start => "start",
        MarqueeAlign::Center => "center",
        MarqueeAlign::End => "end",
    }
}

fn chart_type_name(kind: ChartType) -> &'static str {
    match kind {
        ChartType::Line => "line",
        ChartType::Scatter => "scatter",
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
        let (child_widget, child_attr) = extract_child(child)?;
        let meta = RootMeta {
            delay,
            max_age,
            show_full_animation,
        };
        let attrs = vec![
            attr("child", child_attr),
            attr("delay", WidgetAttrValue::Int(delay)),
            attr("max_age", WidgetAttrValue::Int(max_age)),
            attr(
                "show_full_animation",
                WidgetAttrValue::Bool(show_full_animation),
            ),
        ];
        Ok(eval.heap().alloc(StarlarkWidget::new_root_with_attrs(
            child_widget,
            meta,
            attrs,
        )))
    }

    #[allow(non_snake_case)]
    fn r#Box<'v>(
        #[starlark(default = NoneType)] child: Value<'v>,
        #[starlark(default = 0)] width: i32,
        #[starlark(default = 0)] height: i32,
        #[starlark(default = 0)] padding: i32,
        #[starlark(default = NoneType)] color: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let (child_widget, child_attr) = extract_optional_child(child)?;
        let widget = BoxWidget {
            child: child_widget,
            width,
            height,
            padding,
            color: extract_color(color)?,
        };
        let attrs = vec![
            attr("child", child_attr),
            attr("width", WidgetAttrValue::Int(width)),
            attr("height", WidgetAttrValue::Int(height)),
            attr("padding", WidgetAttrValue::Int(padding)),
            attr("color", color_attr(color)?),
        ];
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(widget),
            "Box",
            attrs,
        )))
    }

    fn Text<'v>(
        content: &str,
        #[starlark(default = "")] font: &str,
        #[starlark(default = 0)] height: i32,
        #[starlark(default = 0)] offset: i32,
        #[starlark(default = NoneType)] color: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let mut t = Text::new(content).with_font(default_font(font));
        if height > 0 {
            t = t.with_height(height);
        }
        if offset != 0 {
            t = t.with_offset(offset);
        }
        if let Some(c) = extract_color(color)? {
            t = t.with_color(c);
        }

        let attrs = vec![
            attr("content", WidgetAttrValue::String(t.content.clone())),
            attr("font", WidgetAttrValue::String(t.font.clone())),
            attr("height", WidgetAttrValue::Int(t.height)),
            attr("offset", WidgetAttrValue::Int(t.offset)),
            attr("color", color_attr(color)?),
        ];
        Ok(eval
            .heap()
            .alloc(StarlarkWidget::new_with_attrs(Box::new(t), "Text", attrs)))
    }

    fn Row<'v>(
        #[starlark(default = NoneType)] children: Value<'v>,
        #[starlark(default = "")] main_align: &str,
        #[starlark(default = "")] cross_align: &str,
        #[starlark(default = false)] expanded: bool,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let (kids, child_attrs) = extract_children(children)?;
        let main = MainAlign::from_str(main_align);
        let cross = CrossAlign::from_str(cross_align);
        let widget = Row::new(kids, main, cross, expanded);
        let attrs = vec![
            attr("children", child_attrs),
            attr(
                "main_align",
                WidgetAttrValue::String(main_align_name(main).to_string()),
            ),
            attr(
                "cross_align",
                WidgetAttrValue::String(cross_align_name(cross).to_string()),
            ),
            attr("expanded", WidgetAttrValue::Bool(expanded)),
        ];
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(widget),
            "Row",
            attrs,
        )))
    }

    fn Column<'v>(
        #[starlark(default = NoneType)] children: Value<'v>,
        #[starlark(default = "")] main_align: &str,
        #[starlark(default = "")] cross_align: &str,
        #[starlark(default = false)] expanded: bool,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let (kids, child_attrs) = extract_children(children)?;
        let main = MainAlign::from_str(main_align);
        let cross = CrossAlign::from_str(cross_align);
        let widget = Column::new(kids, main, cross, expanded);
        let attrs = vec![
            attr("children", child_attrs),
            attr(
                "main_align",
                WidgetAttrValue::String(main_align_name(main).to_string()),
            ),
            attr(
                "cross_align",
                WidgetAttrValue::String(cross_align_name(cross).to_string()),
            ),
            attr("expanded", WidgetAttrValue::Bool(expanded)),
        ];
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(widget),
            "Column",
            attrs,
        )))
    }

    fn Stack<'v>(
        #[starlark(default = NoneType)] children: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let (kids, child_attrs) = extract_children(children)?;
        let widget = Stack { children: kids };
        let attrs = vec![attr("children", child_attrs)];
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(widget),
            "Stack",
            attrs,
        )))
    }

    fn Padding<'v>(
        child: Value<'v>,
        #[starlark(default = NoneType)] pad: Value<'v>,
        #[starlark(default = false)] expanded: bool,
        #[starlark(default = NoneType)] color: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let (child_widget, child_attr) = extract_child(child)?;
        let insets = extract_insets(pad)?;
        let widget = Padding {
            child: child_widget,
            pad: insets,
            expanded,
            color: extract_color(color)?,
        };
        let pad_attr = if pad.is_none() {
            WidgetAttrValue::None
        } else {
            value_to_attr(pad)?
        };
        let attrs = vec![
            attr("child", child_attr),
            attr("pad", pad_attr),
            attr("expanded", WidgetAttrValue::Bool(expanded)),
            attr("color", color_attr(color)?),
        ];
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(widget),
            "Padding",
            attrs,
        )))
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
        let (child_widget, child_attr) = extract_child(child)?;
        let direction = ScrollDirection::from_str(scroll_direction);
        let align = MarqueeAlign::from_str(align);
        let widget = Marquee {
            child: child_widget,
            width,
            height,
            offset_start,
            offset_end,
            scroll_direction: direction,
            align,
            delay,
        };
        let attrs = vec![
            attr("child", child_attr),
            attr("width", WidgetAttrValue::Int(width)),
            attr("height", WidgetAttrValue::Int(height)),
            attr("offset_start", WidgetAttrValue::Int(offset_start)),
            attr("offset_end", WidgetAttrValue::Int(offset_end)),
            attr(
                "scroll_direction",
                WidgetAttrValue::String(scroll_direction_name(direction).to_string()),
            ),
            attr(
                "align",
                WidgetAttrValue::String(marquee_align_name(align).to_string()),
            ),
            attr("delay", WidgetAttrValue::Int(delay)),
        ];
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(widget),
            "Marquee",
            attrs,
        )))
    }

    fn Animation<'v>(
        #[starlark(default = NoneType)] children: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let (kids, child_attrs) = extract_children(children)?;
        let widget = Animation { children: kids };
        let attrs = vec![attr("children", child_attrs)];
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(widget),
            "Animation",
            attrs,
        )))
    }

    fn Sequence<'v>(
        #[starlark(default = NoneType)] children: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let (kids, child_attrs) = extract_children(children)?;
        let widget = Sequence { children: kids };
        let attrs = vec![attr("children", child_attrs)];
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(widget),
            "Sequence",
            attrs,
        )))
    }

    fn Image<'v>(
        src: Value<'v>,
        #[starlark(default = 0)] width: i32,
        #[starlark(default = 0)] height: i32,
        #[starlark(default = 1)] hold_frames: i32,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let (bytes, src_attr) = extract_blob(src)?;
        let w = if width > 0 { Some(width) } else { None };
        let h = if height > 0 { Some(height) } else { None };
        let mut widget = ImageWidget::from_bytes(&bytes, w, h)?;
        widget.hold_frames = hold_frames.max(1);
        let delay = widget.delay_ms;
        let attrs = vec![
            attr("src", src_attr),
            attr("width", WidgetAttrValue::Int(width)),
            attr("height", WidgetAttrValue::Int(height)),
            attr("delay", WidgetAttrValue::Int(delay)),
            attr("hold_frames", WidgetAttrValue::Int(widget.hold_frames)),
        ];
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(widget),
            "Image",
            attrs,
        )))
    }

    fn Circle<'v>(
        #[starlark(default = NoneType)] child: Value<'v>,
        #[starlark(default = NoneType)] color: Value<'v>,
        #[starlark(default = 0)] diameter: i32,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let (child_widget, child_attr) = extract_optional_child(child)?;
        let mut widget = Circle::new(diameter);
        widget.child = child_widget;
        widget.color = extract_color(color)?;
        let attrs = vec![
            attr("child", child_attr),
            attr("color", color_attr(color)?),
            attr("diameter", WidgetAttrValue::Int(diameter)),
        ];
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(widget),
            "Circle",
            attrs,
        )))
    }

    fn Arc<'v>(
        x: Value<'v>,
        y: Value<'v>,
        radius: Value<'v>,
        start_angle: Value<'v>,
        end_angle: Value<'v>,
        color: Value<'v>,
        width: Value<'v>,
        #[starlark(default = false)] antialias: bool,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let widget = Arc {
            x: to_f32(x)?,
            y: to_f32(y)?,
            radius: to_f32(radius)?,
            start_angle: to_f32(start_angle)?,
            end_angle: to_f32(end_angle)?,
            color: extract_color(color)?.ok_or_else(|| anyhow::anyhow!("color is required"))?,
            width: to_f32(width)?,
        };
        let attrs = vec![
            attr("x", WidgetAttrValue::Float(widget.x as f64)),
            attr("y", WidgetAttrValue::Float(widget.y as f64)),
            attr("radius", WidgetAttrValue::Float(widget.radius as f64)),
            attr(
                "start_angle",
                WidgetAttrValue::Float(widget.start_angle as f64),
            ),
            attr("end_angle", WidgetAttrValue::Float(widget.end_angle as f64)),
            attr("color", color_attr(color)?),
            attr("width", WidgetAttrValue::Float(widget.width as f64)),
            attr("antialias", WidgetAttrValue::Bool(antialias)),
        ];
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(widget),
            "Arc",
            attrs,
        )))
    }

    fn Line<'v>(
        x1: Value<'v>,
        y1: Value<'v>,
        x2: Value<'v>,
        y2: Value<'v>,
        color: Value<'v>,
        width: Value<'v>,
        #[starlark(default = false)] antialias: bool,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let widget = Line {
            x1: to_f32(x1)?,
            y1: to_f32(y1)?,
            x2: to_f32(x2)?,
            y2: to_f32(y2)?,
            color: extract_color(color)?.ok_or_else(|| anyhow::anyhow!("color is required"))?,
            width: to_f32(width)?,
        };
        let attrs = vec![
            attr("x1", WidgetAttrValue::Float(widget.x1 as f64)),
            attr("y1", WidgetAttrValue::Float(widget.y1 as f64)),
            attr("x2", WidgetAttrValue::Float(widget.x2 as f64)),
            attr("y2", WidgetAttrValue::Float(widget.y2 as f64)),
            attr("color", color_attr(color)?),
            attr("width", WidgetAttrValue::Float(widget.width as f64)),
            attr("antialias", WidgetAttrValue::Bool(antialias)),
        ];
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(widget),
            "Line",
            attrs,
        )))
    }

    fn PieChart<'v>(
        colors: Value<'v>,
        weights: Value<'v>,
        diameter: i32,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let (colors_out, colors_attr) = extract_color_list(colors, "colors")?;
        let (weights_out, weights_attr) = extract_float_list(weights, "weights")?;
        let widget = PieChart {
            colors: colors_out,
            weights: weights_out,
            diameter,
        };
        let attrs = vec![
            attr("colors", colors_attr),
            attr("weights", weights_attr),
            attr("diameter", WidgetAttrValue::Int(diameter)),
        ];
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(widget),
            "PieChart",
            attrs,
        )))
    }

    fn Plot<'v>(
        data: Value<'v>,
        width: i32,
        height: i32,
        #[starlark(default = NoneType)] color: Value<'v>,
        #[starlark(default = NoneType)] color_inverted: Value<'v>,
        #[starlark(default = NoneType)] x_lim: Value<'v>,
        #[starlark(default = NoneType)] y_lim: Value<'v>,
        #[starlark(default = false)] fill: bool,
        #[starlark(default = "")] chart_type: &str,
        #[starlark(default = NoneType)] fill_color: Value<'v>,
        #[starlark(default = NoneType)] fill_color_inverted: Value<'v>,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let (data_out, data_attr) = extract_plot_data(data)?;
        let (x_lim_out, x_lim_attr) = extract_optional_range(x_lim)?;
        let (y_lim_out, y_lim_attr) = extract_optional_range(y_lim)?;
        let chart_type = ChartType::from_str(chart_type);
        let widget = Plot {
            data: data_out,
            width,
            height,
            color: extract_color(color)?
                .unwrap_or(tiny_skia::Color::from_rgba8(255, 255, 255, 255)),
            color_inverted: extract_color(color_inverted)?,
            x_lim: x_lim_out,
            y_lim: y_lim_out,
            fill,
            fill_color: extract_color(fill_color)?,
            fill_color_inverted: extract_color(fill_color_inverted)?,
            chart_type,
        };
        let attrs = vec![
            attr("data", data_attr),
            attr("width", WidgetAttrValue::Int(width)),
            attr("height", WidgetAttrValue::Int(height)),
            attr("color", color_attr(color)?),
            attr("color_inverted", color_attr(color_inverted)?),
            attr("x_lim", x_lim_attr),
            attr("y_lim", y_lim_attr),
            attr("fill", WidgetAttrValue::Bool(fill)),
            attr(
                "chart_type",
                WidgetAttrValue::String(chart_type_name(chart_type).to_string()),
            ),
            attr("fill_color", color_attr(fill_color)?),
            attr("fill_color_inverted", color_attr(fill_color_inverted)?),
        ];
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(widget),
            "Plot",
            attrs,
        )))
    }

    fn Polygon<'v>(
        vertices: Value<'v>,
        #[starlark(default = NoneType)] fill_color: Value<'v>,
        #[starlark(default = NoneType)] stroke_color: Value<'v>,
        #[starlark(default = 0)] stroke_width: Value<'v>,
        #[starlark(default = false)] antialias: bool,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let (vertices_out, vertices_attr) = extract_vertices(vertices)?;
        let stroke_width = to_f64(stroke_width)?;
        let widget = Polygon {
            vertices: vertices_out,
            fill_color: extract_color(fill_color)?,
            stroke_color: extract_color(stroke_color)?,
            stroke_width: stroke_width as f32,
        };
        let attrs = vec![
            attr("vertices", vertices_attr),
            attr("fill_color", color_attr(fill_color)?),
            attr("stroke_color", color_attr(stroke_color)?),
            attr("stroke_width", WidgetAttrValue::Float(stroke_width)),
            attr("antialias", WidgetAttrValue::Bool(antialias)),
        ];
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(widget),
            "Polygon",
            attrs,
        )))
    }

    fn WrappedText<'v>(
        content: &str,
        #[starlark(default = "")] font: &str,
        #[starlark(default = 0)] width: i32,
        #[starlark(default = 0)] height: i32,
        #[starlark(default = 0)] linespacing: i32,
        #[starlark(default = NoneType)] color: Value<'v>,
        #[starlark(default = "")] align: &str,
        #[starlark(default = false)] wordbreak: bool,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let mut wt = WrappedText::new(content).with_font(default_font(font));
        if width > 0 {
            wt = wt.with_width(width);
        }
        if height > 0 {
            wt = wt.with_height(height);
        }
        if linespacing > 0 {
            wt = wt.with_line_spacing(linespacing);
        }
        if let Some(c) = extract_color(color)? {
            wt = wt.with_color(c);
        }
        if !align.is_empty() {
            wt = wt.with_align(WrapAlign::from_str(align));
        }
        wt = wt.with_word_break(wordbreak);

        let attrs = vec![
            attr("content", WidgetAttrValue::String(wt.content.clone())),
            attr("font", WidgetAttrValue::String(wt.font.clone())),
            attr("height", WidgetAttrValue::Int(wt.height)),
            attr("width", WidgetAttrValue::Int(wt.width)),
            attr("linespacing", WidgetAttrValue::Int(wt.line_spacing)),
            attr("color", color_attr(color)?),
            attr(
                "align",
                WidgetAttrValue::String(wrap_align_name(wt.align).to_string()),
            ),
            attr("wordbreak", WidgetAttrValue::Bool(wt.word_break)),
        ];
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(wt),
            "WrappedText",
            attrs,
        )))
    }

    fn Emoji<'v>(
        emoji: &str,
        #[starlark(default = 20)] width: i32,
        #[starlark(default = 20)] height: i32,
        eval: &mut Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let widget = Emoji::new(emoji, width, height);
        let attrs = vec![
            attr("emoji", WidgetAttrValue::String(emoji.to_string())),
            attr("width", WidgetAttrValue::Int(width)),
            attr("height", WidgetAttrValue::Int(height)),
        ];
        Ok(eval.heap().alloc(StarlarkWidget::new_with_attrs(
            Box::new(widget),
            "Emoji",
            attrs,
        )))
    }
}

pub fn build_render_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(render_module)
        .build()
}

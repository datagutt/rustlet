use std::fmt;
use std::sync::Arc;

use allocative::Allocative;
use anyhow::Result;
use starlark::starlark_simple_value;
use starlark::values::{NoSerialize, ProvidesStaticType, StarlarkValue};
use starlark_derive::starlark_value;

use rustlet_render::{Rect, Widget};

/// Optional Root-specific metadata stored alongside a widget when
/// the Starlark-side constructor is `render.Root(...)`.
#[derive(Clone, Debug, Default)]
pub struct RootMeta {
    pub delay: i32,
    pub max_age: i32,
    pub show_full_animation: bool,
}

/// Thin wrapper around `Arc<dyn Widget>` that implements Widget itself,
/// allowing shared ownership so starlark values can be reused across
/// multiple parent widgets.
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

/// Wrapper that holds any render widget as a Starlark value.
///
/// Widgets are stored behind Arc so they can be shared across multiple
/// parent widgets (e.g. reusing the same Box in a loop).
#[derive(ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkWidget {
    #[allocative(skip)]
    inner: Arc<dyn Widget>,
    type_name: String,
    #[allocative(skip)]
    root_meta: Option<RootMeta>,
}

starlark_simple_value!(StarlarkWidget);

impl StarlarkWidget {
    pub fn new(widget: Box<dyn Widget>, type_name: &str) -> Self {
        Self {
            inner: Arc::from(widget),
            type_name: type_name.to_string(),
            root_meta: None,
        }
    }

    pub fn new_root(widget: Box<dyn Widget>, meta: RootMeta) -> Self {
        Self {
            inner: Arc::from(widget),
            type_name: "Root".to_string(),
            root_meta: Some(meta),
        }
    }

    pub fn is_root(&self) -> bool {
        self.root_meta.is_some()
    }

    pub fn root_meta(&self) -> Option<&RootMeta> {
        self.root_meta.as_ref()
    }

    pub fn type_name(&self) -> &str {
        &self.type_name
    }

    /// Get the widget as a Box<dyn Widget> via Arc sharing.
    /// Can be called multiple times (widgets are reference-counted).
    pub fn take_widget(&self) -> Result<Box<dyn Widget>> {
        Ok(Box::new(SharedWidget(Arc::clone(&self.inner))))
    }
}

impl fmt::Debug for StarlarkWidget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StarlarkWidget({})", self.type_name)
    }
}

impl fmt::Display for StarlarkWidget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Widget({})", self.type_name)
    }
}

#[starlark_value(type = "Widget")]
impl<'v> StarlarkValue<'v> for StarlarkWidget {}

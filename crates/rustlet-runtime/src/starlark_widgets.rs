use std::fmt;
use std::sync::{Arc, Mutex};

use allocative::Allocative;
use anyhow::{anyhow, Result};
use starlark::starlark_simple_value;
use starlark::values::{NoSerialize, ProvidesStaticType, StarlarkValue};
use starlark_derive::starlark_value;

use rustlet_render::Widget;

/// Optional Root-specific metadata stored alongside a widget when
/// the Starlark-side constructor is `render.Root(...)`.
#[derive(Clone, Debug, Default)]
pub struct RootMeta {
    pub delay: i32,
    pub max_age: i32,
    pub show_full_animation: bool,
}

/// Wrapper that holds any render widget as a Starlark value.
///
/// The inner widget can be taken out exactly once via `take_widget`.
/// This allows the widget tree to be assembled in Starlark and then
/// extracted into native Rust ownership for rendering.
#[derive(ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkWidget {
    #[allocative(skip)]
    inner: Arc<Mutex<Option<Box<dyn Widget>>>>,
    type_name: String,
    #[allocative(skip)]
    root_meta: Option<RootMeta>,
}

starlark_simple_value!(StarlarkWidget);

impl StarlarkWidget {
    pub fn new(widget: Box<dyn Widget>, type_name: &str) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Some(widget))),
            type_name: type_name.to_string(),
            root_meta: None,
        }
    }

    pub fn new_root(widget: Box<dyn Widget>, meta: RootMeta) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Some(widget))),
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

    /// Extract the inner widget, consuming it. Errors if already taken.
    pub fn take_widget(&self) -> Result<Box<dyn Widget>> {
        self.inner
            .lock()
            .map_err(|e| anyhow!("lock poisoned: {e}"))?
            .take()
            .ok_or_else(|| anyhow!("widget ({}) already consumed", self.type_name))
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

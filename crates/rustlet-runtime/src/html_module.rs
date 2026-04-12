//! Starlark bindings for pixlet's `html.star`. Wraps the `scraper` crate to give
//! applets a goquery/jQuery-like API: parse an HTML string with `html(body=...)`
//! then navigate via `.find(selector)`, `.children()`, `.attr(name)`, `.text()`.
//!
//! The returned `Selection` is immutable, single-shot data and held by id inside
//! a small per-document table, so traversal methods can return new Selections
//! without reparsing the source.

use std::fmt;

use allocative::Allocative;
use scraper::{Html, Node, Selector};
use starlark::environment::{GlobalsBuilder, Methods, MethodsBuilder, MethodsStatic};
use starlark::starlark_simple_value;
use starlark::values::none::NoneType;
use starlark::values::{
    Heap, NoSerialize, ProvidesStaticType, StarlarkValue, Value, ValueLike,
};
use starlark_derive::starlark_value;

use ego_tree::NodeId;

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkHtmlSelection {
    #[allocative(skip)]
    html: std::sync::Arc<Html>,
    #[allocative(skip)]
    nodes: Vec<NodeId>,
}

starlark_simple_value!(StarlarkHtmlSelection);

impl fmt::Display for StarlarkHtmlSelection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<Selection {} nodes>", self.nodes.len())
    }
}

impl StarlarkHtmlSelection {
    fn new(html: std::sync::Arc<Html>, nodes: Vec<NodeId>) -> Self {
        Self { html, nodes }
    }

    fn alloc_new<'v>(
        heap: &'v Heap,
        html: std::sync::Arc<Html>,
        nodes: Vec<NodeId>,
    ) -> Value<'v> {
        heap.alloc(StarlarkHtmlSelection::new(html, nodes))
    }

    fn first_node(&self) -> Option<&scraper::ElementRef<'_>> {
        None
    }

    fn element_refs(&self) -> Vec<scraper::ElementRef<'_>> {
        let mut out = Vec::new();
        for &id in &self.nodes {
            if let Some(node_ref) = self.html.tree.get(id) {
                if let Some(el) = scraper::ElementRef::wrap(node_ref) {
                    out.push(el);
                }
            }
        }
        out
    }
}

#[starlark_value(type = "html.Selection")]
impl<'v> StarlarkValue<'v> for StarlarkHtmlSelection {
    fn get_methods() -> Option<&'static Methods> {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(selection_methods)
    }
}

fn parse_selector(selector: &str) -> anyhow::Result<Selector> {
    Selector::parse(selector).map_err(|e| anyhow::anyhow!("invalid selector: {e}"))
}

#[starlark::starlark_module]
fn selection_methods(builder: &mut MethodsBuilder) {
    fn attr<'v>(
        #[starlark(this)] this: Value<'v>,
        name: &str,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let sel = this
            .downcast_ref::<StarlarkHtmlSelection>()
            .ok_or_else(|| anyhow::anyhow!("expected Selection"))?;
        for el in sel.element_refs() {
            if let Some(val) = el.value().attr(name) {
                return Ok(eval.heap().alloc(val));
            }
        }
        Ok(Value::new_none())
    }

    fn text<'v>(
        #[starlark(this)] this: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let sel = this
            .downcast_ref::<StarlarkHtmlSelection>()
            .ok_or_else(|| anyhow::anyhow!("expected Selection"))?;
        let mut out = String::new();
        for el in sel.element_refs() {
            out.push_str(&el.text().collect::<String>());
        }
        Ok(eval.heap().alloc(out.as_str()))
    }

    fn find<'v>(
        #[starlark(this)] this: Value<'v>,
        selector: &str,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let sel = this
            .downcast_ref::<StarlarkHtmlSelection>()
            .ok_or_else(|| anyhow::anyhow!("expected Selection"))?;
        let selector = parse_selector(selector)?;
        let mut ids = Vec::new();
        for el in sel.element_refs() {
            for matched in el.select(&selector) {
                ids.push(matched.id());
            }
        }
        Ok(StarlarkHtmlSelection::alloc_new(
            eval.heap(),
            sel.html.clone(),
            ids,
        ))
    }

    fn filter<'v>(
        #[starlark(this)] this: Value<'v>,
        selector: &str,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let sel = this
            .downcast_ref::<StarlarkHtmlSelection>()
            .ok_or_else(|| anyhow::anyhow!("expected Selection"))?;
        let selector = parse_selector(selector)?;
        let mut ids = Vec::new();
        for el in sel.element_refs() {
            // scraper's ElementRef::select is descendants-only; use manual check on self.
            if selector.matches(&el) {
                ids.push(el.id());
            }
        }
        Ok(StarlarkHtmlSelection::alloc_new(
            eval.heap(),
            sel.html.clone(),
            ids,
        ))
    }

    fn children<'v>(
        #[starlark(this)] this: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let sel = this
            .downcast_ref::<StarlarkHtmlSelection>()
            .ok_or_else(|| anyhow::anyhow!("expected Selection"))?;
        let mut ids = Vec::new();
        for el in sel.element_refs() {
            for child in el.children() {
                if let Some(ch) = scraper::ElementRef::wrap(child) {
                    ids.push(ch.id());
                }
            }
        }
        Ok(StarlarkHtmlSelection::alloc_new(
            eval.heap(),
            sel.html.clone(),
            ids,
        ))
    }

    fn parent<'v>(
        #[starlark(this)] this: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let sel = this
            .downcast_ref::<StarlarkHtmlSelection>()
            .ok_or_else(|| anyhow::anyhow!("expected Selection"))?;
        let mut ids = Vec::new();
        for el in sel.element_refs() {
            if let Some(parent) = el.parent() {
                if let Some(pe) = scraper::ElementRef::wrap(parent) {
                    ids.push(pe.id());
                }
            }
        }
        Ok(StarlarkHtmlSelection::alloc_new(
            eval.heap(),
            sel.html.clone(),
            ids,
        ))
    }

    fn first<'v>(
        #[starlark(this)] this: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let sel = this
            .downcast_ref::<StarlarkHtmlSelection>()
            .ok_or_else(|| anyhow::anyhow!("expected Selection"))?;
        let ids: Vec<NodeId> = sel.nodes.iter().take(1).copied().collect();
        Ok(StarlarkHtmlSelection::alloc_new(
            eval.heap(),
            sel.html.clone(),
            ids,
        ))
    }

    fn last<'v>(
        #[starlark(this)] this: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let sel = this
            .downcast_ref::<StarlarkHtmlSelection>()
            .ok_or_else(|| anyhow::anyhow!("expected Selection"))?;
        let ids: Vec<NodeId> = sel.nodes.iter().last().copied().into_iter().collect();
        Ok(StarlarkHtmlSelection::alloc_new(
            eval.heap(),
            sel.html.clone(),
            ids,
        ))
    }

    fn eq<'v>(
        #[starlark(this)] this: Value<'v>,
        index: i32,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let sel = this
            .downcast_ref::<StarlarkHtmlSelection>()
            .ok_or_else(|| anyhow::anyhow!("expected Selection"))?;
        let idx = if index < 0 {
            sel.nodes.len() as i32 + index
        } else {
            index
        } as usize;
        let ids: Vec<NodeId> = sel.nodes.get(idx).copied().into_iter().collect();
        Ok(StarlarkHtmlSelection::alloc_new(
            eval.heap(),
            sel.html.clone(),
            ids,
        ))
    }

    fn len<'v>(
        #[starlark(this)] this: Value<'v>,
    ) -> anyhow::Result<i32> {
        let sel = this
            .downcast_ref::<StarlarkHtmlSelection>()
            .ok_or_else(|| anyhow::anyhow!("expected Selection"))?;
        Ok(sel.nodes.len() as i32)
    }

    fn is_selector<'v>(
        #[starlark(this)] this: Value<'v>,
        selector: &str,
    ) -> anyhow::Result<bool> {
        let sel = this
            .downcast_ref::<StarlarkHtmlSelection>()
            .ok_or_else(|| anyhow::anyhow!("expected Selection"))?;
        let selector = parse_selector(selector)?;
        for el in sel.element_refs() {
            if selector.matches(&el) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn siblings<'v>(
        #[starlark(this)] this: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let sel = this
            .downcast_ref::<StarlarkHtmlSelection>()
            .ok_or_else(|| anyhow::anyhow!("expected Selection"))?;
        let mut ids = Vec::new();
        for el in sel.element_refs() {
            if let Some(parent) = el.parent() {
                for child in parent.children() {
                    if child.id() == el.id() {
                        continue;
                    }
                    if let Some(ce) = scraper::ElementRef::wrap(child) {
                        ids.push(ce.id());
                    }
                }
            }
        }
        Ok(StarlarkHtmlSelection::alloc_new(
            eval.heap(),
            sel.html.clone(),
            ids,
        ))
    }
}

#[starlark::starlark_module]
pub fn html_module(builder: &mut GlobalsBuilder) {
    /// Parse an HTML document and return a Selection rooted at the document's
    /// root element. Matches pixlet's `html(body=...)`.
    fn html<'v>(
        body: &str,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let doc = Html::parse_document(body);
        let root_id = doc.tree.root().id();
        Ok(StarlarkHtmlSelection::alloc_new(
            eval.heap(),
            std::sync::Arc::new(doc),
            vec![root_id],
        ))
    }
}

pub fn build_html_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(html_module)
        .build()
}

// Silence unused warnings while this module is the sole consumer of these items.
#[allow(dead_code)]
fn _keep() {
    let _ = Node::Document;
    let _ = NoneType;
    let _: Option<&scraper::ElementRef> = None;
    let _ = StarlarkHtmlSelection::first_node;
}

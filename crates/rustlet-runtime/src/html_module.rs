//! Starlark bindings for pixlet's `html.star`. Wraps the `scraper` crate
//! (html5ever + CSS selectors) to give applets a goquery/jQuery-like API:
//! `html(body=...)`, `.find(selector)`, `.children()`, `.attr(name)`, `.text()`.
//!
//! `scraper::Html` is not `Sync` because tendril uses interior mutability, so
//! we can't hold an `Arc<Html>` across the Starlark boundary. Instead we store
//! the raw source string plus each selection's address (a path of child
//! indices from the document root). On every method call we re-parse and walk
//! to those addresses, then query with the real HTML parser. Parsing a small
//! pixlet applet payload takes microseconds so this is fine in practice.

use std::fmt;

use allocative::Allocative;
use scraper::{ElementRef, Html, Selector};
use starlark::environment::{GlobalsBuilder, Methods, MethodsBuilder, MethodsStatic};
use starlark::starlark_simple_value;
use starlark::values::{
    Heap, NoSerialize, ProvidesStaticType, StarlarkValue, Value, ValueLike,
};
use starlark_derive::starlark_value;

/// A path from the document's root to a specific element, encoded as a list of
/// child indices at each depth. The root itself is an empty path.
#[derive(Debug, Clone, Allocative, Default)]
pub struct NodePath(#[allocative(skip)] Vec<usize>);

#[derive(Debug, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkHtmlSelection {
    #[allocative(skip)]
    source: String,
    paths: Vec<NodePath>,
}

starlark_simple_value!(StarlarkHtmlSelection);

impl fmt::Display for StarlarkHtmlSelection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<Selection {} nodes>", self.paths.len())
    }
}

impl StarlarkHtmlSelection {
    fn new(source: String, paths: Vec<NodePath>) -> Self {
        Self { source, paths }
    }

    fn alloc_new<'v>(heap: &'v Heap, source: String, paths: Vec<NodePath>) -> Value<'v> {
        heap.alloc(StarlarkHtmlSelection::new(source, paths))
    }

    fn parse(&self) -> Html {
        Html::parse_document(&self.source)
    }

    /// Walk the provided document and collect the ElementRefs referenced by
    /// this selection's paths. Paths that no longer match (e.g. because the
    /// source changed, which it can't) are silently skipped.
    fn resolve<'a>(&'a self, doc: &'a Html) -> Vec<ElementRef<'a>> {
        let mut out = Vec::with_capacity(self.paths.len());
        for path in &self.paths {
            if let Some(el) = walk_path(doc, &path.0) {
                out.push(el);
            }
        }
        out
    }
}

/// Starting from the document root, follow a sequence of element-child
/// indices. At each step, the `idx`-th element child (not text/comment) is
/// picked. Returns `None` if any index is out of range.
fn walk_path<'a>(doc: &'a Html, path: &[usize]) -> Option<ElementRef<'a>> {
    // The "root selection" represents the document element.
    let mut current = doc.root_element();
    for &idx in path {
        let mut it = current.children().filter_map(ElementRef::wrap);
        current = it.nth(idx)?;
    }
    Some(current)
}

/// Build a path to an ElementRef by walking up to the root and recording the
/// element-sibling index at each level.
fn path_for(doc: &Html, el: ElementRef<'_>) -> NodePath {
    let root_id = doc.root_element().id();
    let mut segments: Vec<usize> = Vec::new();
    let mut node_id = el.id();
    while node_id != root_id {
        let node_ref = doc.tree.get(node_id).expect("valid node id");
        let parent_ref = match node_ref.parent() {
            Some(p) => p,
            None => break,
        };
        let mut idx = 0;
        for child in parent_ref.children() {
            if ElementRef::wrap(child).is_none() {
                continue;
            }
            if child.id() == node_id {
                break;
            }
            idx += 1;
        }
        segments.push(idx);
        node_id = parent_ref.id();
    }
    segments.reverse();
    NodePath(segments)
}

fn parse_selector(selector: &str) -> anyhow::Result<Selector> {
    Selector::parse(selector).map_err(|e| anyhow::anyhow!("invalid selector: {e}"))
}

#[starlark_value(type = "html.Selection")]
impl<'v> StarlarkValue<'v> for StarlarkHtmlSelection {
    fn get_methods() -> Option<&'static Methods> {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(selection_methods)
    }
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
        let doc = sel.parse();
        for el in sel.resolve(&doc) {
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
        let doc = sel.parse();
        let mut out = String::new();
        for el in sel.resolve(&doc) {
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
        let doc = sel.parse();
        let mut paths = Vec::new();
        for el in sel.resolve(&doc) {
            for matched in el.select(&selector) {
                paths.push(path_for(&doc, matched));
            }
        }
        Ok(StarlarkHtmlSelection::alloc_new(
            eval.heap(),
            sel.source.clone(),
            paths,
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
        let doc = sel.parse();
        let mut paths = Vec::new();
        for el in sel.resolve(&doc) {
            if selector.matches(&el) {
                paths.push(path_for(&doc, el));
            }
        }
        Ok(StarlarkHtmlSelection::alloc_new(
            eval.heap(),
            sel.source.clone(),
            paths,
        ))
    }

    fn children<'v>(
        #[starlark(this)] this: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let sel = this
            .downcast_ref::<StarlarkHtmlSelection>()
            .ok_or_else(|| anyhow::anyhow!("expected Selection"))?;
        let doc = sel.parse();
        let mut paths = Vec::new();
        for el in sel.resolve(&doc) {
            for child in el.children() {
                if let Some(ch) = ElementRef::wrap(child) {
                    paths.push(path_for(&doc, ch));
                }
            }
        }
        Ok(StarlarkHtmlSelection::alloc_new(
            eval.heap(),
            sel.source.clone(),
            paths,
        ))
    }

    fn parent<'v>(
        #[starlark(this)] this: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let sel = this
            .downcast_ref::<StarlarkHtmlSelection>()
            .ok_or_else(|| anyhow::anyhow!("expected Selection"))?;
        let doc = sel.parse();
        let mut paths = Vec::new();
        for el in sel.resolve(&doc) {
            if let Some(parent) = el.parent().and_then(ElementRef::wrap) {
                paths.push(path_for(&doc, parent));
            }
        }
        Ok(StarlarkHtmlSelection::alloc_new(
            eval.heap(),
            sel.source.clone(),
            paths,
        ))
    }

    fn siblings<'v>(
        #[starlark(this)] this: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let sel = this
            .downcast_ref::<StarlarkHtmlSelection>()
            .ok_or_else(|| anyhow::anyhow!("expected Selection"))?;
        let doc = sel.parse();
        let mut paths = Vec::new();
        for el in sel.resolve(&doc) {
            if let Some(parent) = el.parent() {
                for child in parent.children() {
                    if child.id() == el.id() {
                        continue;
                    }
                    if let Some(ce) = ElementRef::wrap(child) {
                        paths.push(path_for(&doc, ce));
                    }
                }
            }
        }
        Ok(StarlarkHtmlSelection::alloc_new(
            eval.heap(),
            sel.source.clone(),
            paths,
        ))
    }

    fn first<'v>(
        #[starlark(this)] this: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let sel = this
            .downcast_ref::<StarlarkHtmlSelection>()
            .ok_or_else(|| anyhow::anyhow!("expected Selection"))?;
        let paths = sel.paths.iter().take(1).cloned().collect();
        Ok(StarlarkHtmlSelection::alloc_new(
            eval.heap(),
            sel.source.clone(),
            paths,
        ))
    }

    fn last<'v>(
        #[starlark(this)] this: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let sel = this
            .downcast_ref::<StarlarkHtmlSelection>()
            .ok_or_else(|| anyhow::anyhow!("expected Selection"))?;
        let paths: Vec<NodePath> = sel.paths.last().cloned().into_iter().collect();
        Ok(StarlarkHtmlSelection::alloc_new(
            eval.heap(),
            sel.source.clone(),
            paths,
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
            sel.paths.len() as i32 + index
        } else {
            index
        };
        let paths: Vec<NodePath> = if idx >= 0 {
            sel.paths
                .get(idx as usize)
                .cloned()
                .into_iter()
                .collect()
        } else {
            Vec::new()
        };
        Ok(StarlarkHtmlSelection::alloc_new(
            eval.heap(),
            sel.source.clone(),
            paths,
        ))
    }

    fn len<'v>(#[starlark(this)] this: Value<'v>) -> anyhow::Result<i32> {
        let sel = this
            .downcast_ref::<StarlarkHtmlSelection>()
            .ok_or_else(|| anyhow::anyhow!("expected Selection"))?;
        Ok(sel.paths.len() as i32)
    }

    fn is_selector<'v>(
        #[starlark(this)] this: Value<'v>,
        selector: &str,
    ) -> anyhow::Result<bool> {
        let sel = this
            .downcast_ref::<StarlarkHtmlSelection>()
            .ok_or_else(|| anyhow::anyhow!("expected Selection"))?;
        let selector = parse_selector(selector)?;
        let doc = sel.parse();
        for el in sel.resolve(&doc) {
            if selector.matches(&el) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[starlark::starlark_module]
pub fn html_module(builder: &mut GlobalsBuilder) {
    /// Parse an HTML document and return a Selection rooted at the document
    /// element. Matches pixlet's `html(body=...)`.
    fn html<'v>(
        body: &str,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        Ok(StarlarkHtmlSelection::alloc_new(
            eval.heap(),
            body.to_string(),
            vec![NodePath::default()],
        ))
    }
}

pub fn build_html_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(html_module)
        .build()
}

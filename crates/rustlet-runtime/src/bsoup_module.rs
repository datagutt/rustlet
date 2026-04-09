use std::fmt;

use allocative::Allocative;
use roxmltree::{Document, Node};
use starlark::environment::{GlobalsBuilder, Methods, MethodsBuilder, MethodsStatic};
use starlark::starlark_simple_value;
use starlark::values::{NoSerialize, ProvidesStaticType, StarlarkValue, Value, ValueLike};
use starlark_derive::starlark_value;

#[derive(Debug, Clone, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkSoupDocument {
    #[allocative(skip)]
    source: String,
}

#[derive(Debug, Clone, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkSoupNode {
    #[allocative(skip)]
    source: String,
    path: Vec<usize>,
}

starlark_simple_value!(StarlarkSoupDocument);
starlark_simple_value!(StarlarkSoupNode);

impl fmt::Display for StarlarkSoupDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<bsoup document>")
    }
}

impl fmt::Display for StarlarkSoupNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<bsoup node>")
    }
}

#[starlark_value(type = "bsoup_document")]
impl<'v> StarlarkValue<'v> for StarlarkSoupDocument {
    fn get_methods() -> Option<&'static Methods> {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(bsoup_document_methods)
    }
}

#[starlark_value(type = "bsoup_node")]
impl<'v> StarlarkValue<'v> for StarlarkSoupNode {
    fn get_methods() -> Option<&'static Methods> {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(bsoup_node_methods)
    }
}

#[starlark::starlark_module]
fn bsoup_document_methods(builder: &mut MethodsBuilder) {
    fn find<'v>(
        #[starlark(this)] this: Value<'v>,
        tag: &str,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let doc = this
            .downcast_ref::<StarlarkSoupDocument>()
            .ok_or_else(|| anyhow::anyhow!("expected bsoup document"))?;
        let parsed = parse_document(&doc.source)?;
        if let Some(node) = find_descendant(parsed.root_element(), tag) {
            return Ok(eval.heap().alloc(StarlarkSoupNode {
                source: doc.source.clone(),
                path: node_path(node),
            }));
        }
        Ok(Value::new_none())
    }
}

#[starlark::starlark_module]
fn bsoup_node_methods(builder: &mut MethodsBuilder) {
    fn find<'v>(
        #[starlark(this)] this: Value<'v>,
        tag: &str,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let node = this
            .downcast_ref::<StarlarkSoupNode>()
            .ok_or_else(|| anyhow::anyhow!("expected bsoup node"))?;
        let parsed = parse_document(&node.source)?;
        let resolved = resolve_node(&parsed, &node.path)?;
        if let Some(found) = find_descendant(resolved, tag) {
            return Ok(eval.heap().alloc(StarlarkSoupNode {
                source: node.source.clone(),
                path: node_path(found),
            }));
        }
        Ok(Value::new_none())
    }

    fn get_text(#[starlark(this)] this: Value) -> anyhow::Result<String> {
        let node = this
            .downcast_ref::<StarlarkSoupNode>()
            .ok_or_else(|| anyhow::anyhow!("expected bsoup node"))?;
        let parsed = parse_document(&node.source)?;
        let resolved = resolve_node(&parsed, &node.path)?;
        Ok(node_text(resolved))
    }
}

#[starlark::starlark_module]
pub fn bsoup_module(builder: &mut GlobalsBuilder) {
    fn parseHtml(source: &str) -> anyhow::Result<StarlarkSoupDocument> {
        parse_document(source)?;
        Ok(StarlarkSoupDocument {
            source: source.to_string(),
        })
    }
}

pub fn build_bsoup_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(bsoup_module)
        .build()
}

fn parse_document(source: &str) -> anyhow::Result<Document<'_>> {
    Document::parse(source).map_err(|e| anyhow::anyhow!("invalid HTML/XML: {e}"))
}

fn find_descendant<'a>(root: Node<'a, 'a>, tag: &str) -> Option<Node<'a, 'a>> {
    root.descendants()
        .find(|node| node.is_element() && node.tag_name().name() == tag)
}

fn node_path(node: Node<'_, '_>) -> Vec<usize> {
    let mut path = Vec::new();
    let mut current = node;
    while let Some(parent) = current.parent() {
        let index = parent
            .children()
            .position(|child| child == current)
            .unwrap_or(0);
        path.push(index);
        current = parent;
        if current.parent().is_none() {
            break;
        }
    }
    path.reverse();
    path
}

fn resolve_node<'a>(doc: &'a Document<'a>, path: &[usize]) -> anyhow::Result<Node<'a, 'a>> {
    let mut node = doc.root();
    for index in path {
        node = node
            .children()
            .nth(*index)
            .ok_or_else(|| anyhow::anyhow!("invalid bsoup node path"))?;
    }
    Ok(node)
}

fn node_text(node: Node<'_, '_>) -> String {
    node.descendants()
        .filter(|descendant| descendant.is_text())
        .filter_map(|descendant| descendant.text())
        .collect::<Vec<_>>()
        .join("")
}

use std::fmt;

use allocative::Allocative;
use roxmltree::{Document, Node};
use starlark::environment::{GlobalsBuilder, Methods, MethodsBuilder, MethodsStatic};
use starlark::starlark_simple_value;
use starlark::values::tuple::AllocTuple;
use starlark::values::{NoSerialize, ProvidesStaticType, StarlarkValue, Value, ValueLike};
use starlark_derive::starlark_value;

#[derive(Debug, Clone, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkXPathNode {
    #[allocative(skip)]
    source: String,
    path: Vec<usize>,
}

starlark_simple_value!(StarlarkXPathNode);

impl fmt::Display for StarlarkXPathNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "XPath(...)")
    }
}

#[starlark_value(type = "XPath")]
impl<'v> StarlarkValue<'v> for StarlarkXPathNode {
    fn get_methods() -> Option<&'static Methods> {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(xpath_methods)
    }
}

#[starlark::starlark_module]
fn xpath_methods(builder: &mut MethodsBuilder) {
    fn query<'v>(
        #[starlark(this)] this: Value<'v>,
        path: &str,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let node = unpack_node(this)?;
        let parsed = parse_document(&node.source)?;
        let current = resolve_node(&parsed, &node.path)?;
        Ok(xpath_query(&current, path)
            .into_iter()
            .next()
            .map(|node| eval.heap().alloc(node_inner_text(node)))
            .unwrap_or(Value::new_none()))
    }

    fn query_all<'v>(
        #[starlark(this)] this: Value<'v>,
        path: &str,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let node = unpack_node(this)?;
        let parsed = parse_document(&node.source)?;
        let current = resolve_node(&parsed, &node.path)?;
        let values = xpath_query(&current, path)
            .into_iter()
            .map(|node| eval.heap().alloc(node_inner_text(node)))
            .collect::<Vec<_>>();
        Ok(eval.heap().alloc(AllocTuple(values)))
    }

    fn query_node<'v>(
        #[starlark(this)] this: Value<'v>,
        path: &str,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let node = unpack_node(this)?;
        let parsed = parse_document(&node.source)?;
        let current = resolve_node(&parsed, &node.path)?;
        Ok(xpath_query(&current, path)
            .into_iter()
            .next()
            .map(|found| {
                eval.heap().alloc(StarlarkXPathNode {
                    source: node.source.clone(),
                    path: node_path(found),
                })
            })
            .unwrap_or(Value::new_none()))
    }

    fn query_all_nodes<'v>(
        #[starlark(this)] this: Value<'v>,
        path: &str,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let node = unpack_node(this)?;
        let parsed = parse_document(&node.source)?;
        let current = resolve_node(&parsed, &node.path)?;
        let values = xpath_query(&current, path)
            .into_iter()
            .map(|found| {
                eval.heap().alloc(StarlarkXPathNode {
                    source: node.source.clone(),
                    path: node_path(found),
                })
            })
            .collect::<Vec<_>>();
        Ok(eval.heap().alloc(AllocTuple(values)))
    }
}

#[starlark::starlark_module]
pub fn xpath_module(builder: &mut GlobalsBuilder) {
    fn loads(xml: &str) -> anyhow::Result<StarlarkXPathNode> {
        let parsed = parse_document(xml)?;
        Ok(StarlarkXPathNode {
            source: xml.to_owned(),
            path: node_path(parsed.root_element()),
        })
    }
}

pub fn build_xpath_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(xpath_module)
        .build()
}

fn unpack_node(value: Value<'_>) -> anyhow::Result<&StarlarkXPathNode> {
    value
        .downcast_ref::<StarlarkXPathNode>()
        .ok_or_else(|| anyhow::anyhow!("expected XPath node, got {}", value.get_type()))
}

fn parse_document(source: &str) -> anyhow::Result<Document<'_>> {
    Document::parse(source).map_err(|e| anyhow::anyhow!("invalid XML: {e}"))
}

fn resolve_node<'a>(doc: &'a Document<'a>, path: &[usize]) -> anyhow::Result<Node<'a, 'a>> {
    let mut node = doc.root();
    for index in path {
        node = node
            .children()
            .nth(*index)
            .ok_or_else(|| anyhow::anyhow!("invalid xpath node path"))?;
    }
    Ok(node)
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

fn xpath_query<'a>(root: &Node<'a, 'a>, path: &str) -> Vec<Node<'a, 'a>> {
    let mut segments = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if segments.is_empty() {
        return Vec::new();
    }

    if root.is_element() && root.tag_name().name() == segments[0] {
        segments.remove(0);
    }

    let mut nodes = vec![*root];
    for segment in segments {
        nodes = nodes
            .into_iter()
            .flat_map(|node| {
                node.children()
                    .filter(move |child| child.is_element() && child.tag_name().name() == segment)
            })
            .collect();
        if nodes.is_empty() {
            break;
        }
    }
    nodes
}

fn node_inner_text(node: Node<'_, '_>) -> String {
    node.descendants()
        .filter(|descendant| descendant.is_text())
        .filter_map(|descendant| descendant.text())
        .collect::<Vec<_>>()
        .join("")
}

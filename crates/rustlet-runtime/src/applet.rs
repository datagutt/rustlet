use std::collections::HashMap;

use anyhow::{anyhow, Result};
use starlark::environment::{FrozenModule, Globals, GlobalsBuilder, Module};
use starlark::eval::{Evaluator, ReturnFileLoader};
use starlark::syntax::{AstModule, Dialect};
use starlark::values::dict::AllocDict;
use starlark::values::structs::AllocStruct;

use rustlet_render::Root;

use crate::render_module::build_render_globals;
use crate::starlark_widgets::StarlarkWidget;

pub struct Applet {
    globals: Globals,
}

impl Applet {
    pub fn new() -> Self {
        let globals = GlobalsBuilder::standard()
            .with(crate::render_module::render_module)
            .build();
        Self { globals }
    }

    /// Parse and run a Starlark applet, returning one or more Roots.
    ///
    /// The source must define a `main(config)` function that returns
    /// a `render.Root(...)` widget (or a list of them).
    pub fn run(
        &self,
        id: &str,
        src: &str,
        config: &HashMap<String, String>,
    ) -> Result<Vec<Root>> {
        let render_frozen = build_render_frozen_module()?;

        let ast = AstModule::parse(id, src.to_owned(), &Dialect::Standard)
            .map_err(|e| anyhow!("{e}"))?;

        let module = Module::new();

        let mut modules_map: HashMap<&str, &FrozenModule> = HashMap::new();
        modules_map.insert("render.star", &render_frozen);
        let loader = ReturnFileLoader {
            modules: &modules_map,
        };

        let mut eval = Evaluator::new(&module);
        eval.set_loader(&loader);
        eval.eval_module(ast, &self.globals)
            .map_err(|e| anyhow!("{e}"))?;

        let main_val = module
            .get("main")
            .ok_or_else(|| anyhow!("script does not define a `main` function"))?;

        let heap = module.heap();
        let config_val =
            heap.alloc(AllocDict(config.iter().map(|(k, v)| (k.as_str(), v.as_str()))));

        let result = eval
            .eval_function(main_val, &[config_val], &[])
            .map_err(|e| anyhow!("{e}"))?;

        extract_roots(result)
    }
}

/// Build a FrozenModule for "render.star" that exports a single `render` symbol
/// containing all widget constructors as attributes.
fn build_render_frozen_module() -> Result<FrozenModule> {
    let render_globals = build_render_globals();

    // Create a module with a `render` struct containing all constructors
    let module = Module::new();
    let heap = module.heap();

    // Collect all globals into a struct value
    let entries: Vec<(&str, starlark::values::Value)> = render_globals
        .iter()
        .map(|(name, val)| (name, val.to_value()))
        .collect();

    let render_struct = heap.alloc(AllocStruct(entries));
    module.set("render", render_struct);

    module
        .freeze()
        .map_err(|e| anyhow!("failed to freeze render module: {e:?}"))
}

/// Convert a Starlark return value (single Root widget or list of them) into Vec<Root>.
fn extract_roots(value: starlark::values::Value) -> Result<Vec<Root>> {
    // Try single widget first
    if let Some(sw) = StarlarkWidget::from_value(value) {
        let root = extract_single_root(sw)?;
        return Ok(vec![root]);
    }

    // Try list of widgets
    if let Some(list) = starlark::values::list::ListRef::from_value(value) {
        let mut roots = Vec::with_capacity(list.len());
        for item in list.iter() {
            let sw = StarlarkWidget::from_value(item)
                .ok_or_else(|| anyhow!("list item must be a Root widget"))?;
            roots.push(extract_single_root(sw)?);
        }
        return Ok(roots);
    }

    Err(anyhow!(
        "main() must return a Root widget or list of Root widgets, got {}",
        value.get_type()
    ))
}

/// Extract a Root from a StarlarkWidget that was created with render.Root().
fn extract_single_root(sw: &StarlarkWidget) -> Result<Root> {
    let meta = sw
        .root_meta()
        .ok_or_else(|| anyhow!("expected a Root widget, got {}", sw.type_name()))?;

    let child = sw.take_widget()?;
    let mut root = Root::new(child);
    if meta.delay > 0 {
        root.delay = meta.delay;
    }
    root.max_age = meta.max_age;
    root.show_full_animation = meta.show_full_animation;
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_simple_text() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"render.star\", \"render\")\n",
            "\n",
            "def main(config):\n",
            "    return render.Root(\n",
            "        child = render.Text(\"Hello\"),\n",
            "    )\n",
        );
        let config = HashMap::new();
        let roots = applet.run("test.star", src, &config).unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn eval_box_with_color() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"render.star\", \"render\")\n",
            "\n",
            "def main(config):\n",
            "    return render.Root(\n",
            "        child = render.Box(color = \"#ff0000\"),\n",
            "    )\n",
        );
        let config = HashMap::new();
        let roots = applet.run("test.star", src, &config).unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn eval_row_with_children() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"render.star\", \"render\")\n",
            "\n",
            "def main(config):\n",
            "    return render.Root(\n",
            "        child = render.Row(\n",
            "            children = [\n",
            "                render.Text(\"A\"),\n",
            "                render.Text(\"B\"),\n",
            "            ],\n",
            "        ),\n",
            "    )\n",
        );
        let config = HashMap::new();
        let roots = applet.run("test.star", src, &config).unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn missing_main_errors() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"render.star\", \"render\")\n",
            "\n",
            "x = 42\n",
        );
        let config = HashMap::new();
        let result = applet.run("test.star", src, &config);
        match result {
            Ok(_) => panic!("expected error for missing main"),
            Err(e) => {
                let err_msg = e.to_string();
                assert!(
                    err_msg.contains("main"),
                    "error should mention 'main', got: {err_msg}"
                );
            }
        }
    }
}

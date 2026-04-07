use std::collections::HashMap;

use anyhow::{anyhow, Result};
use starlark::environment::{FrozenModule, Globals, GlobalsBuilder, Module};
use starlark::eval::{Evaluator, ReturnFileLoader};
use starlark::syntax::{AstModule, Dialect};
use starlark::values::dict::AllocDict;
use starlark::values::structs::AllocStruct;

use rustlet_render::Root;

use crate::base64_module::build_base64_globals;
use crate::color_module::build_color_globals;
use crate::json_module::build_json_globals;
use crate::math_module::build_math_globals;
use crate::random_module::build_random_globals;
use crate::render_module::build_render_globals;
use crate::starlark_canvas::StarlarkCanvas;
use crate::starlark_widgets::StarlarkWidget;
use crate::time_module::build_time_globals;

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
        width: u32,
        height: u32,
    ) -> Result<Vec<Root>> {
        self.run_with_options(id, src, config, width, height, false)
    }

    pub fn run_with_options(
        &self,
        id: &str,
        src: &str,
        config: &HashMap<String, String>,
        width: u32,
        height: u32,
        is_2x: bool,
    ) -> Result<Vec<Root>> {
        let render_frozen = build_render_frozen_module(width, height, is_2x)?;
        let time_frozen = build_simple_frozen_module("time", build_time_globals())?;
        let base64_frozen = build_simple_frozen_module("base64", build_base64_globals())?;
        let json_frozen = build_simple_frozen_module("json", build_json_globals())?;
        let math_frozen = build_math_frozen_module()?;
        let random_frozen = build_simple_frozen_module("random", build_random_globals())?;
        let color_frozen = build_simple_frozen_module("color", build_color_globals())?;

        let ast = AstModule::parse(id, src.to_owned(), &Dialect::Standard)
            .map_err(|e| anyhow!("{e}"))?;

        let module = Module::new();

        let mut modules_map: HashMap<&str, &FrozenModule> = HashMap::new();
        modules_map.insert("render.star", &render_frozen);
        modules_map.insert("time.star", &time_frozen);
        modules_map.insert("encoding/base64.star", &base64_frozen);
        modules_map.insert("encoding/json.star", &json_frozen);
        modules_map.insert("math.star", &math_frozen);
        modules_map.insert("random.star", &random_frozen);
        modules_map.insert("color.star", &color_frozen);
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
/// containing all widget constructors plus canvas constants.
fn build_render_frozen_module(width: u32, height: u32, is_2x: bool) -> Result<FrozenModule> {
    let render_globals = build_render_globals();

    let module = Module::new();
    let heap = module.heap();

    let mut entries: Vec<(&str, starlark::values::Value)> = render_globals
        .iter()
        .map(|(name, val)| (name, val.to_value()))
        .collect();

    // Inject canvas constants
    entries.push(("CANVAS_WIDTH", heap.alloc(width as i32)));
    entries.push(("CANVAS_HEIGHT", heap.alloc(height as i32)));

    let render_struct = heap.alloc(AllocStruct(entries));
    module.set("render", render_struct);

    let canvas = heap.alloc(StarlarkCanvas {
        width: width as i32,
        height: height as i32,
        is_2x,
    });
    module.set("canvas", canvas);

    module
        .freeze()
        .map_err(|e| anyhow!("failed to freeze render module: {e:?}"))
}

/// Build a FrozenModule that exports a single named symbol wrapping all
/// functions from the given Globals as struct attributes.
fn build_simple_frozen_module(name: &str, globals: starlark::environment::Globals) -> Result<FrozenModule> {
    let module = Module::new();
    let heap = module.heap();

    let entries: Vec<(&str, starlark::values::Value)> = globals
        .iter()
        .map(|(n, val)| (n, val.to_value()))
        .collect();

    let struct_val = heap.alloc(AllocStruct(entries));
    module.set(name, struct_val);

    module
        .freeze()
        .map_err(|e| anyhow!("failed to freeze {name} module: {e:?}"))
}

/// Build math module with float constants alongside functions.
fn build_math_frozen_module() -> Result<FrozenModule> {
    use starlark::values::float::StarlarkFloat;

    let math_globals = build_math_globals();

    let module = Module::new();
    let heap = module.heap();

    let mut entries: Vec<(&str, starlark::values::Value)> = math_globals
        .iter()
        .map(|(name, val)| (name, val.to_value()))
        .collect();

    entries.push(("pi", heap.alloc(StarlarkFloat(std::f64::consts::PI))));
    entries.push(("e", heap.alloc(StarlarkFloat(std::f64::consts::E))));

    let struct_val = heap.alloc(AllocStruct(entries));
    module.set("math", struct_val);

    module
        .freeze()
        .map_err(|e| anyhow!("failed to freeze math module: {e:?}"))
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
        let roots = applet.run("test.star", src, &config, 64, 32).unwrap();
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
        let roots = applet.run("test.star", src, &config, 64, 32).unwrap();
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
        let roots = applet.run("test.star", src, &config, 64, 32).unwrap();
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
        let result = applet.run("test.star", src, &config, 64, 32);
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

    #[test]
    fn canvas_constants() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"render.star\", \"render\")\n",
            "\n",
            "def main(config):\n",
            "    w = render.CANVAS_WIDTH\n",
            "    h = render.CANVAS_HEIGHT\n",
            "    return render.Root(\n",
            "        child = render.Text(str(w) + \"x\" + str(h)),\n",
            "    )\n",
        );
        let config = HashMap::new();
        let roots = applet.run("test.star", src, &config, 64, 32).unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn time_now() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"time.star\", \"time\")\n",
            "load(\"render.star\", \"render\")\n",
            "\n",
            "def main(config):\n",
            "    t = time.now()\n",
            "    return render.Root(\n",
            "        child = render.Text(t),\n",
            "    )\n",
        );
        let config = HashMap::new();
        let roots = applet.run("test.star", src, &config, 64, 32).unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn base64_round_trip() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"encoding/base64.star\", \"base64\")\n",
            "load(\"render.star\", \"render\")\n",
            "\n",
            "def main(config):\n",
            "    encoded = base64.encode(\"hello world\")\n",
            "    decoded = base64.decode(encoded)\n",
            "    if decoded != \"hello world\":\n",
            "        fail(\"round-trip failed: \" + decoded)\n",
            "    return render.Root(\n",
            "        child = render.Text(decoded),\n",
            "    )\n",
        );
        let config = HashMap::new();
        let roots = applet.run("test.star", src, &config, 64, 32).unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn math_pow() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"math.star\", \"math\")\n",
            "load(\"render.star\", \"render\")\n",
            "\n",
            "def main(config):\n",
            "    result = math.pow(2, 10)\n",
            "    if result != 1024:\n",
            "        fail(\"expected 1024, got \" + str(result))\n",
            "    return render.Root(\n",
            "        child = render.Text(str(result)),\n",
            "    )\n",
        );
        let config = HashMap::new();
        let roots = applet.run("test.star", src, &config, 64, 32).unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn random_number() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"random.star\", \"random\")\n",
            "load(\"render.star\", \"render\")\n",
            "\n",
            "def main(config):\n",
            "    n = random.number(1, 100)\n",
            "    if n < 1 or n > 100:\n",
            "        fail(\"out of range: \" + str(n))\n",
            "    return render.Root(\n",
            "        child = render.Text(str(n)),\n",
            "    )\n",
        );
        let config = HashMap::new();
        let roots = applet.run("test.star", src, &config, 64, 32).unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn json_round_trip() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"encoding/json.star\", \"json\")\n",
            "load(\"render.star\", \"render\")\n",
            "\n",
            "def main(config):\n",
            "    data = json.decode('{\"key\": \"value\"}')\n",
            "    if data[\"key\"] != \"value\":\n",
            "        fail(\"decode failed\")\n",
            "    return render.Root(\n",
            "        child = render.Text(data[\"key\"]),\n",
            "    )\n",
        );
        let config = HashMap::new();
        let roots = applet.run("test.star", src, &config, 64, 32).unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn time_from_timestamp() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"time.star\", \"time\")\n",
            "load(\"render.star\", \"render\")\n",
            "\n",
            "def main(config):\n",
            "    t = time.from_timestamp(0)\n",
            "    if t != \"1970-01-01T00:00:00Z\":\n",
            "        fail(\"expected epoch, got \" + t)\n",
            "    return render.Root(\n",
            "        child = render.Text(t),\n",
            "    )\n",
        );
        let config = HashMap::new();
        let roots = applet.run("test.star", src, &config, 64, 32).unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn time_parse_duration() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"time.star\", \"time\")\n",
            "load(\"render.star\", \"render\")\n",
            "\n",
            "def main(config):\n",
            "    ms = time.parse_duration(\"5s\")\n",
            "    if ms != 5000:\n",
            "        fail(\"expected 5000, got \" + str(ms))\n",
            "    return render.Root(\n",
            "        child = render.Text(str(ms)),\n",
            "    )\n",
        );
        let config = HashMap::new();
        let roots = applet.run("test.star", src, &config, 64, 32).unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn color_rgb_in_box() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"render.star\", \"render\")\n",
            "load(\"color.star\", \"color\")\n",
            "\n",
            "def main(config):\n",
            "    return render.Root(\n",
            "        child = render.Box(color = color.rgb(255, 0, 0)),\n",
            "    )\n",
        );
        let config = HashMap::new();
        let roots = applet.run("test.star", src, &config, 64, 32).unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn color_hex_constructor() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"render.star\", \"render\")\n",
            "load(\"color.star\", \"color\")\n",
            "\n",
            "def main(config):\n",
            "    return render.Root(\n",
            "        child = render.Box(color = color.hex(\"#ff0000\")),\n",
            "    )\n",
        );
        let config = HashMap::new();
        let roots = applet.run("test.star", src, &config, 64, 32).unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn color_attributes() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"render.star\", \"render\")\n",
            "load(\"color.star\", \"color\")\n",
            "\n",
            "def main(config):\n",
            "    c = color.rgb(255, 128, 0)\n",
            "    if c.r != 255:\n",
            "        fail(\"expected r=255, got \" + str(c.r))\n",
            "    if c.g != 128:\n",
            "        fail(\"expected g=128, got \" + str(c.g))\n",
            "    if c.b != 0:\n",
            "        fail(\"expected b=0, got \" + str(c.b))\n",
            "    if c.a != 255:\n",
            "        fail(\"expected a=255, got \" + str(c.a))\n",
            "    return render.Root(\n",
            "        child = render.Text(str(c.r)),\n",
            "    )\n",
        );
        let config = HashMap::new();
        let roots = applet.run("test.star", src, &config, 64, 32).unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn color_hex_method() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"render.star\", \"render\")\n",
            "load(\"color.star\", \"color\")\n",
            "\n",
            "def main(config):\n",
            "    c = color.rgb(255, 0, 0)\n",
            "    h = c.hex()\n",
            "    if h != \"#ff0000\":\n",
            "        fail(\"expected #ff0000, got \" + h)\n",
            "    return render.Root(\n",
            "        child = render.Text(h),\n",
            "    )\n",
        );
        let config = HashMap::new();
        let roots = applet.run("test.star", src, &config, 64, 32).unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn color_hsv() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"render.star\", \"render\")\n",
            "load(\"color.star\", \"color\")\n",
            "\n",
            "def main(config):\n",
            "    c = color.hsv(0, 1.0, 1.0)\n",
            "    if c.r != 255:\n",
            "        fail(\"expected r=255, got \" + str(c.r))\n",
            "    if c.g != 0:\n",
            "        fail(\"expected g=0, got \" + str(c.g))\n",
            "    if c.b != 0:\n",
            "        fail(\"expected b=0, got \" + str(c.b))\n",
            "    return render.Root(\n",
            "        child = render.Text(str(c.r)),\n",
            "    )\n",
        );
        let config = HashMap::new();
        let roots = applet.run("test.star", src, &config, 64, 32).unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn color_string_still_works() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"render.star\", \"render\")\n",
            "\n",
            "def main(config):\n",
            "    return render.Root(\n",
            "        child = render.Box(color = \"#00ff00\"),\n",
            "    )\n",
        );
        let config = HashMap::new();
        let roots = applet.run("test.star", src, &config, 64, 32).unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn color_display_format() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"render.star\", \"render\")\n",
            "load(\"color.star\", \"color\")\n",
            "\n",
            "def main(config):\n",
            "    c = color.rgb(255, 0, 0)\n",
            "    s = str(c)\n",
            "    if s != \"#ff0000\":\n",
            "        fail(\"expected #ff0000, got \" + s)\n",
            "    return render.Root(\n",
            "        child = render.Text(s),\n",
            "    )\n",
        );
        let config = HashMap::new();
        let roots = applet.run("test.star", src, &config, 64, 32).unwrap();
        assert_eq!(roots.len(), 1);
    }
}

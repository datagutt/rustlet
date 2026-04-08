use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use dupe::Dupe;
use starlark::environment::{FrozenModule, Globals, GlobalsBuilder, Module};
use starlark::eval::{Evaluator, FileLoader};
use starlark::syntax::{AstModule, Dialect};
use starlark::values::dict::AllocDict;
use starlark::values::structs::AllocStruct;
use starlark::values::Value;

use rustlet_render::Root;

use crate::base64_module::build_base64_globals;
use crate::color_module::build_color_globals;
use crate::http_module::{build_http_globals, set_request_context};
use crate::humanize_module::build_humanize_globals;
use crate::json_module::build_json_globals;
use crate::math_module::build_math_globals;
use crate::random_module::{build_random_globals, seed_for_execution};
use crate::render_module::{build_render_globals, set_render_context};
use crate::schema_module::build_schema_globals;
use crate::starlark_canvas::StarlarkCanvas;
use crate::starlark_config::StarlarkConfig;
use crate::starlark_file::StarlarkFile;
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
        self.run_with_options(id, src, config, width, height, false, None)
    }

    pub fn run_with_options(
        &self,
        id: &str,
        src: &str,
        config: &HashMap<String, String>,
        width: u32,
        height: u32,
        is_2x: bool,
        base_dir: Option<&Path>,
    ) -> Result<Vec<Root>> {
        seed_for_execution(id);
        set_request_context(id);
        set_render_context(is_2x);

        let render_frozen = build_render_frozen_module(width, height, is_2x)?;
        let time_frozen = build_simple_frozen_module("time", build_time_globals())?;
        let base64_frozen = build_simple_frozen_module("base64", build_base64_globals())?;
        let json_frozen = build_simple_frozen_module("json", build_json_globals())?;
        let math_frozen = build_math_frozen_module()?;
        let random_frozen = build_simple_frozen_module("random", build_random_globals())?;
        let color_frozen = build_simple_frozen_module("color", build_color_globals())?;
        let humanize_frozen = build_simple_frozen_module("humanize", build_humanize_globals())?;
        let http_frozen = build_simple_frozen_module("http", build_http_globals())?;
        let schema_frozen = build_simple_frozen_module("schema", build_schema_globals())?;

        let ast =
            AstModule::parse(id, src.to_owned(), &Dialect::Standard).map_err(|e| anyhow!("{e}"))?;

        let module = Module::new();

        let mut modules_map: HashMap<&str, &FrozenModule> = HashMap::new();
        modules_map.insert("render.star", &render_frozen);
        modules_map.insert("time.star", &time_frozen);
        modules_map.insert("encoding/base64.star", &base64_frozen);
        modules_map.insert("encoding/json.star", &json_frozen);
        modules_map.insert("math.star", &math_frozen);
        modules_map.insert("random.star", &random_frozen);
        modules_map.insert("color.star", &color_frozen);
        modules_map.insert("humanize.star", &humanize_frozen);
        modules_map.insert("http.star", &http_frozen);
        modules_map.insert("schema.star", &schema_frozen);
        let loader = AppletFileLoader {
            modules: modules_map,
            base_dir: base_dir.map(|p| p.to_path_buf()),
        };

        let mut eval = Evaluator::new(&module);
        eval.set_loader(&loader);
        eval.eval_module(ast, &self.globals)
            .map_err(|e| anyhow!("{e}"))?;

        let main_val = module
            .get("main")
            .ok_or_else(|| anyhow!("script does not define a `main` function"))?;

        let heap = module.heap();
        let config_val = heap.alloc(StarlarkConfig {
            entries: config.clone(),
        });

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
    let font_list = rustlet_render::fonts::get_font_list();
    let fonts_dict = heap.alloc(AllocDict(
        font_list
            .iter()
            .map(|name| (*name, heap.alloc(*name) as Value)),
    ));
    entries.push(("fonts", fonts_dict));

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
fn build_simple_frozen_module(
    name: &str,
    globals: starlark::environment::Globals,
) -> Result<FrozenModule> {
    let module = Module::new();
    let heap = module.heap();

    let entries: Vec<(&str, starlark::values::Value)> =
        globals.iter().map(|(n, val)| (n, val.to_value())).collect();

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

/// File loader that resolves known modules first, then loads asset files from disk.
struct AppletFileLoader<'a> {
    modules: HashMap<&'a str, &'a FrozenModule>,
    base_dir: Option<PathBuf>,
}

impl<'a> FileLoader for AppletFileLoader<'a> {
    fn load(&self, path: &str) -> starlark::Result<FrozenModule> {
        // Check known modules first
        if let Some(module) = self.modules.get(path) {
            return Ok((*module).dupe());
        }

        // Try loading as an asset file relative to the star file's directory
        if let Some(ref base) = self.base_dir {
            let file_path = base.join(path);
            if file_path.exists() {
                return build_asset_frozen_module(&file_path).map_err(starlark::Error::new_other);
            }
        }

        Err(starlark::Error::new_other(anyhow!(
            "module not found: {path}"
        )))
    }
}

fn build_asset_frozen_module(file_path: &Path) -> Result<FrozenModule> {
    let data = std::fs::read(file_path)?;

    let module = Module::new();
    let heap = module.heap();
    module.set(
        "file",
        heap.alloc(StarlarkFile {
            path: file_path.to_string_lossy().to_string(),
            data,
        }),
    );

    module
        .freeze()
        .map_err(|e| anyhow!("failed to freeze asset module: {e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;

    fn read_http_request(stream: &mut std::net::TcpStream) -> String {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 1024];

        loop {
            let n = stream.read(&mut chunk).unwrap();
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);

            if let Some(header_end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                let header_len = header_end + 4;
                let header_text = String::from_utf8_lossy(&buf[..header_len]);
                let content_length = header_text
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("Content-Length: ")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if buf.len() >= header_len + content_length {
                    break;
                }
            }
        }

        String::from_utf8_lossy(&buf).to_string()
    }

    fn spawn_http_server(
        expected_requests: usize,
    ) -> (String, Arc<Mutex<Vec<String>>>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let request_count = Arc::new(AtomicUsize::new(0));

        let requests_clone = Arc::clone(&requests);
        let count_clone = Arc::clone(&request_count);
        let handle = thread::spawn(move || {
            for stream in listener.incoming().take(expected_requests) {
                let mut stream = stream.unwrap();
                let request = read_http_request(&mut stream);
                requests_clone.lock().unwrap().push(request.clone());

                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");

                let response = if path.starts_with("/cached") {
                    let count = count_clone.fetch_add(1, Ordering::SeqCst) + 1;
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Encoding: identity\r\nX-Foo: a\r\nX-Foo: b\r\nContent-Length: {}\r\n\r\n{{\"count\":{count}}}",
                        format!("{{\"count\":{count}}}").len()
                    )
                } else if path.starts_with("/form") {
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 2\r\n\r\nok"
                        .to_string()
                } else if path.starts_with("/json") {
                    "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: 15\r\n\r\n{\"posted\":true}"
                        .to_string()
                } else {
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_string()
                };

                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        (format!("http://{}", addr), requests, handle)
    }

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
        let src = concat!("load(\"render.star\", \"render\")\n", "\n", "x = 42\n",);
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
            "    if t.year < 2020:\n",
            "        fail(\"year too low\")\n",
            "    return render.Root(\n",
            "        child = render.Text(str(t)),\n",
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
    fn base64_binary_decode_and_image_loading() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"encoding/base64.star\", \"base64\")\n",
            "load(\"render.star\", \"render\")\n",
            "\n",
            "PNG = \"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABAQMAAAAl21bKAAAAA1BMVEX/AAAZ4gk3AAAACklEQVR4nGNiAAAABgADNjd8qAAAAABJRU5ErkJggg==\"\n",
            "\n",
            "def main(config):\n",
            "    data = base64.decode(PNG)\n",
            "    if type(data) != \"bytes\":\n",
            "        fail(\"png decode must return bytes\")\n",
            "    if base64.encode(data) != PNG:\n",
            "        fail(\"bytes re-encode broken\")\n",
            "    img = render.Image(src = data)\n",
            "    if img.size() != (1, 1):\n",
            "        fail(\"image decode from bytes broken\")\n",
            "    return render.Root(child = img)\n",
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
    fn random_seed_and_float_match_pixlet_shape() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"random.star\", \"random\")\n",
            "load(\"render.star\", \"render\")\n",
            "\n",
            "def main(config):\n",
            "    random.seed(4711)\n",
            "    sequence = [random.number(0, 1 << 20) for _ in range(32)]\n",
            "    f = random.float()\n",
            "    if f < 0 or f >= 1:\n",
            "        fail(\"float out of range\")\n",
            "    random.seed(4711)\n",
            "    for i in range(len(sequence)):\n",
            "        if sequence[i] != random.number(0, 1 << 20):\n",
            "            fail(\"identical seed mismatch\")\n",
            "    random.seed(4712)\n",
            "    same = 0\n",
            "    for i in range(len(sequence)):\n",
            "        if sequence[i] == random.number(0, 1 << 20):\n",
            "            same += 1\n",
            "    if same == len(sequence):\n",
            "        fail(\"different seeds produced same sequence\")\n",
            "    if random.number(9223372036854775807, 9223372036854775807) != 9223372036854775807:\n",
            "        fail(\"max edge case broken\")\n",
            "    secure = random.number(0, 10, secure = True)\n",
            "    if secure < 0 or secure > 10:\n",
            "        fail(\"secure random out of range\")\n",
            "    return render.Root(\n",
            "        child = render.Text(\"ok\"),\n",
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
            "    if str(t) != \"1970-01-01T00:00:00Z\":\n",
            "        fail(\"expected epoch, got \" + str(t))\n",
            "    if t.year != 1970:\n",
            "        fail(\"expected year 1970\")\n",
            "    return render.Root(\n",
            "        child = render.Text(str(t)),\n",
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
            "    duration = time.parse_duration(\"5s\")\n",
            "    if duration.seconds != 5:\n",
            "        fail(\"expected 5 seconds, got \" + str(duration.seconds))\n",
            "    return render.Root(\n",
            "        child = render.Text(str(duration.seconds)),\n",
            "    )\n",
        );
        let config = HashMap::new();
        let roots = applet.run("test.star", src, &config, 64, 32).unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn time_duration_arithmetic() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"time.star\", \"time\")\n",
            "load(\"render.star\", \"render\")\n",
            "\n",
            "def main(config):\n",
            "    epoch = time.from_timestamp(0)\n",
            "    later = epoch + time.parse_duration(\"90m\")\n",
            "    if str(later) != \"1970-01-01T01:30:00Z\":\n",
            "        fail(\"time + duration broken: \" + str(later))\n",
            "    delta = later - epoch\n",
            "    if delta.seconds != 5400 or delta.minutes != 90:\n",
            "        fail(\"time difference broken\")\n",
            "    if str(later - time.parse_duration(\"30m\")) != \"1970-01-01T01:00:00Z\":\n",
            "        fail(\"time - duration broken\")\n",
            "    shifted = epoch.in_location(\"+02:00\")\n",
            "    if str(shifted) != \"1970-01-01T02:00:00+02:00\":\n",
            "        fail(\"in_location broken: \" + str(shifted))\n",
            "    return render.Root(child = render.Text(str(delta.seconds)))\n",
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
    fn color_mutation_and_hsv_attrs_match_pixlet_shape() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"render.star\", \"render\")\n",
            "load(\"color.star\", \"color\")\n",
            "\n",
            "def main(config):\n",
            "    c = color.rgb(300, -10, 10, 999)\n",
            "    if c.rgba() != (255, 0, 10, 255):\n",
            "        fail(\"rgb clamp broken: \" + str(c.rgba()))\n",
            "    c.h = 120\n",
            "    c.s = 1\n",
            "    c.v = 1\n",
            "    if c.rgb() != (0, 255, 0):\n",
            "        fail(\"hsv field mutation broken: \" + str(c.rgb()))\n",
            "    h, s, v = c.hsv()\n",
            "    if h != c.h or s != c.s or v != c.v:\n",
            "        fail(\"hsv getters broken\")\n",
            "    if c.hsva() != (120.0, 1.0, 1.0, 255):\n",
            "        fail(\"hsva broken: \" + str(c.hsva()))\n",
            "    c.a = 64\n",
            "    if c.hex() != \"#00ff0040\":\n",
            "        fail(\"alpha mutation broken: \" + c.hex())\n",
            "    return render.Root(child = render.Text(c.hex()))\n",
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
            "    if s != \"Color(\\\"#ff0000\\\")\":\n",
            "        fail(\"expected Color(\\\"#ff0000\\\"), got \" + s)\n",
            "    return render.Root(\n",
            "        child = render.Text(s),\n",
            "    )\n",
        );
        let config = HashMap::new();
        let roots = applet.run("test.star", src, &config, 64, 32).unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn render_surface_compatibility() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"render.star\", \"render\")\n",
            "\n",
            "def main(config):\n",
            "    if render.fonts[\"6x13\"] != \"6x13\":\n",
            "        fail(\"missing fonts map\")\n",
            "    b1 = render.Box(width = 64, height = 32, color = \"#000\")\n",
            "    if b1.width != 64 or b1.height != 32 or b1.color != \"#000\":\n",
            "        fail(\"box attrs broken\")\n",
            "    if b1.frame_count() != 1:\n",
            "        fail(\"box frame_count broken\")\n",
            "    t1 = render.Text(content = \"foo\", font = render.fonts[\"6x13\"], color = \"#fff\", height = 10)\n",
            "    if t1.font != \"6x13\" or t1.color != \"#fff\":\n",
            "        fail(\"text attrs broken\")\n",
            "    tw, th = t1.size()\n",
            "    if tw <= 0 or th <= 0:\n",
            "        fail(\"text size broken\")\n",
            "    line = render.Line(x1 = 0, y1 = 0, x2 = 5, y2 = 5, width = 1, color = \"#fff\")\n",
            "    arc = render.Arc(x = 5, y = 5, radius = 3, start_angle = 0, end_angle = 3.14, width = 1, color = \"#fff\")\n",
            "    pie = render.PieChart(colors = [\"#fff\", \"#000\"], weights = [1, 2], diameter = 10)\n",
            "    plot = render.Plot(data = [(0, 1), (1, 2)], width = 10, height = 8, chart_type = \"scatter\")\n",
            "    poly = render.Polygon(vertices = [(0, 0), (2, 0), (1, 2)], fill_color = \"#f00\", stroke_width = 1)\n",
            "    row = render.Row(children = [b1, t1, line, arc, pie, plot, poly], main_align = \"space_evenly\", cross_align = \"center\")\n",
            "    if row.main_align != \"space_evenly\" or row.cross_align != \"center\":\n",
            "        fail(\"row attrs broken\")\n",
            "    if len(row.children) != 7:\n",
            "        fail(\"row children broken\")\n",
            "    root = render.Root(child = row)\n",
            "    if len(root.child.children) != 7:\n",
            "        fail(\"root child attrs broken\")\n",
            "    return root\n",
        );
        let config = HashMap::new();
        let roots = applet.run("test.star", src, &config, 64, 32).unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn canvas_helpers_match_pixlet_shape() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"render.star\", \"render\", \"canvas\")\n",
            "\n",
            "def main(config):\n",
            "    if canvas.width() != 128 or canvas.height() != 64:\n",
            "        fail(\"scaled canvas helpers broken\")\n",
            "    if canvas.width(True) != 64 or canvas.height(True) != 32:\n",
            "        fail(\"raw canvas helpers broken\")\n",
            "    if canvas.size() != (128, 64):\n",
            "        fail(\"canvas.size broken\")\n",
            "    if canvas.size(True) != (64, 32):\n",
            "        fail(\"canvas.size(raw) broken\")\n",
            "    if not canvas.is2x():\n",
            "        fail(\"canvas.is2x broken\")\n",
            "    return render.Root(child = render.Text(\"ok\"))\n",
        );
        let config = HashMap::new();
        let roots = applet
            .run_with_options("test.star", src, &config, 128, 64, true, None)
            .unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn two_x_defaults_text_and_wrapped_text_fonts() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"render.star\", \"render\")\n",
            "\n",
            "def main(config):\n",
            "    t1 = render.Text(\"plain\")\n",
            "    if t1.font != \"terminus-16\":\n",
            "        fail(\"2x Text default font broken: \" + t1.font)\n",
            "    t2 = render.WrappedText(\"wrapped\")\n",
            "    if t2.font != \"terminus-16\":\n",
            "        fail(\"2x WrappedText default font broken: \" + t2.font)\n",
            "    t3 = render.Text(\"explicit\", font = \"tb-8\")\n",
            "    if t3.font != \"tb-8\":\n",
            "        fail(\"explicit font override broken\")\n",
            "    return render.Root(child = render.Column(children = [t1, t2, t3]))\n",
        );
        let config = HashMap::new();
        let roots = applet
            .run_with_options("test.star", src, &config, 128, 64, true, None)
            .unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn wrapped_text_exposes_wordbreak_attr() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"render.star\", \"render\")\n",
            "\n",
            "def main(config):\n",
            "    wt = render.WrappedText(content = \"abcdef\", width = 15, wordbreak = True)\n",
            "    if not wt.wordbreak:\n",
            "        fail(\"wordbreak attr missing\")\n",
            "    if wt.size()[1] <= 8:\n",
            "        fail(\"wordbreak did not affect wrapping\")\n",
            "    return render.Root(child = wt)\n",
        );
        let config = HashMap::new();
        let roots = applet.run("test.star", src, &config, 64, 32).unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn file_readall_and_image_asset_loading() {
        let applet = Applet::new();
        let dir = tempfile::tempdir().unwrap();
        let text_path = dir.path().join("hello.txt");
        let image_path = dir.path().join("icon.png");

        std::fs::write(&text_path, "hello world").unwrap();

        let mut img = tiny_skia::Pixmap::new(1, 1).unwrap();
        img.fill(tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        let png = img.encode_png().unwrap();
        std::fs::write(&image_path, png).unwrap();

        let src = format!(
            concat!(
                "load(\"hello.txt\", hello = \"file\")\n",
                "load(\"icon.png\", icon = \"file\")\n",
                "load(\"render.star\", \"render\")\n",
                "\n",
                "def main(config):\n",
                "    if hello.readall() != \"hello world\":\n",
                "        fail(\"text readall broken\")\n",
                "    if hello.path != \"{}\":\n",
                "        fail(\"file path broken\")\n",
                "    img = render.Image(src = icon.readall())\n",
                "    if img.size() != (1, 1):\n",
                "        fail(\"binary image loading broken\")\n",
                "    return render.Root(child = img)\n",
            ),
            text_path.to_string_lossy()
        );

        let config = HashMap::new();
        let roots = applet
            .run_with_options("main.star", &src, &config, 64, 32, false, Some(dir.path()))
            .unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn image_accepts_svg_text() {
        let applet = Applet::new();
        let src = concat!(
            "load(\"render.star\", \"render\")\n",
            "\n",
            "SVG = \"<svg xmlns='http://www.w3.org/2000/svg' width='2' height='1' viewBox='0 0 2 1'><rect width='2' height='1' fill='#00ff00'/></svg>\"\n",
            "\n",
            "def main(config):\n",
            "    img = render.Image(src = SVG)\n",
            "    if img.size() != (2, 1):\n",
            "        fail(\"svg image size broken: \" + str(img.size()))\n",
            "    return render.Root(child = img)\n",
        );
        let config = HashMap::new();
        let roots = applet.run("test.star", src, &config, 64, 32).unwrap();
        assert_eq!(roots.len(), 1);
    }

    #[test]
    fn http_surface_matches_pixlet_shape() {
        let (base_url, requests, handle) = spawn_http_server(3);
        let applet = Applet::new();
        let src = format!(
            concat!(
                "load(\"http.star\", \"http\")\n",
                "load(\"render.star\", \"render\")\n",
                "\n",
                "BASE = \"{}\"\n",
                "\n",
                "def main(config):\n",
                "    rep1 = http.get(BASE + \"/cached\", params = {{\"foo\": \"bar baz\"}}, headers = {{\"X-Test\": \"alpha\"}}, ttl_seconds = 60)\n",
                "    if rep1.status != \"200 OK\" or rep1.status_code != 200:\n",
                "        fail(\"status fields broken\")\n",
                "    if rep1.headers[\"X-Foo\"] != \"a,b\":\n",
                "        fail(\"headers shape broken: \" + rep1.headers[\"X-Foo\"])\n",
                "    if rep1.encoding != \"identity\":\n",
                "        fail(\"encoding broken: \" + rep1.encoding)\n",
                "    if rep1.json()[\"count\"] != 1:\n",
                "        fail(\"json parsing broken\")\n",
                "    rep2 = http.get(BASE + \"/cached\", params = {{\"foo\": \"bar baz\"}}, headers = {{\"X-Test\": \"alpha\"}}, ttl_seconds = 60)\n",
                "    if rep2.body() != rep1.body():\n",
                "        fail(\"cache/body broken\")\n",
                "    rep3 = http.post(BASE + \"/form\", form_body = {{\"foo\": \"bar baz\"}}, auth = (\"u\", \"p\"))\n",
                "    if rep3.body() != \"ok\":\n",
                "        fail(\"form post broken\")\n",
                "    rep4 = http.post(BASE + \"/json\", json_body = {{\"hello\": \"world\"}})\n",
                "    if rep4.status_code != 201 or not rep4.json()[\"posted\"]:\n",
                "        fail(\"json post broken\")\n",
                "    if http.status_text(404) != \"Not Found\":\n",
                "        fail(\"status_text broken\")\n",
                "    return render.Root(child = render.Text(\"ok\"))\n",
            ),
            base_url,
        );

        let config = HashMap::new();
        let roots = applet.run("test.star", &src, &config, 64, 32).unwrap();
        assert_eq!(roots.len(), 1);

        handle.join().unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert!(requests[0].starts_with("GET /cached?foo=bar+baz HTTP/1.1"));
        let req0 = requests[0].to_ascii_lowercase();
        assert!(req0.contains("x-test: alpha"));
        assert!(req0.contains("x-tidbyt-app: test.star"));
        assert!(req0.contains("x-tidbyt-cache-seconds: 60"));
        assert!(requests[1].starts_with("POST /form HTTP/1.1"));
        let req1 = requests[1].to_ascii_lowercase();
        assert!(req1.contains("authorization: basic dtpw"));
        assert!(req1.contains("content-type: application/x-www-form-urlencoded"));
        assert!(requests[2].starts_with("POST /json HTTP/1.1"));
        let req2 = requests[2].to_ascii_lowercase();
        assert!(req2.contains("content-type: application/json"));
    }
}

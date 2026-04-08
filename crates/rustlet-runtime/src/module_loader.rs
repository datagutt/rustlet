use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

use anyhow::{anyhow, Result};
use dupe::Dupe;
use starlark::environment::{FrozenModule, Globals, Module};
use starlark::eval::{Evaluator, FileLoader};
use starlark::syntax::{AstModule, Dialect};
use starlark::values::dict::AllocDict;
use starlark::values::structs::AllocStruct;
use starlark::values::Value;

use crate::base64_module::build_base64_globals;
use crate::color_module::build_color_globals;
use crate::http_module::build_http_globals;
use crate::humanize_module::build_humanize_globals;
use crate::json_module::build_json_globals;
use crate::math_module::build_math_globals;
use crate::random_module::build_random_globals;
use crate::render_module::build_render_globals;
use crate::schema_module::build_schema_globals;
use crate::starlark_canvas::StarlarkCanvas;
use crate::starlark_file::StarlarkFile;
use crate::time_module::build_time_globals;

pub(crate) struct BuiltinModuleRegistry {
    modules: HashMap<String, FrozenModule>,
}

impl BuiltinModuleRegistry {
    pub(crate) fn new(width: u32, height: u32, is_2x: bool) -> Result<Self> {
        let mut modules = HashMap::new();
        modules.insert(
            "render.star".to_string(),
            build_render_frozen_module(width, height, is_2x)?,
        );
        modules.insert(
            "time.star".to_string(),
            build_simple_frozen_module("time", build_time_globals())?,
        );
        modules.insert(
            "encoding/base64.star".to_string(),
            build_simple_frozen_module("base64", build_base64_globals())?,
        );
        modules.insert(
            "encoding/json.star".to_string(),
            build_simple_frozen_module("json", build_json_globals())?,
        );
        modules.insert("math.star".to_string(), build_math_frozen_module()?);
        modules.insert(
            "random.star".to_string(),
            build_simple_frozen_module("random", build_random_globals())?,
        );
        modules.insert(
            "color.star".to_string(),
            build_simple_frozen_module("color", build_color_globals())?,
        );
        modules.insert(
            "humanize.star".to_string(),
            build_simple_frozen_module("humanize", build_humanize_globals())?,
        );
        modules.insert(
            "http.star".to_string(),
            build_simple_frozen_module("http", build_http_globals())?,
        );
        modules.insert(
            "schema.star".to_string(),
            build_simple_frozen_module("schema", build_schema_globals())?,
        );
        Ok(Self { modules })
    }

    pub(crate) fn loader<'a>(
        &'a self,
        globals: &'a Globals,
        base_dir: Option<&Path>,
    ) -> AppletFileLoader<'a> {
        AppletFileLoader {
            state: Rc::new(ModuleLoadState {
                globals,
                builtins: &self.modules,
                base_dir: base_dir.map(Path::to_path_buf),
                cache: RefCell::new(HashMap::new()),
                loading: RefCell::new(Vec::new()),
            }),
            current_dir: PathBuf::new(),
        }
    }
}

pub(crate) struct AppletFileLoader<'a> {
    state: Rc<ModuleLoadState<'a>>,
    current_dir: PathBuf,
}

impl FileLoader for AppletFileLoader<'_> {
    fn load(&self, path: &str) -> starlark::Result<FrozenModule> {
        self.load_module(path).map_err(starlark::Error::new_other)
    }
}

struct ModuleLoadState<'a> {
    globals: &'a Globals,
    builtins: &'a HashMap<String, FrozenModule>,
    base_dir: Option<PathBuf>,
    cache: RefCell<HashMap<String, FrozenModule>>,
    loading: RefCell<Vec<String>>,
}

impl AppletFileLoader<'_> {
    fn load_module(&self, path: &str) -> Result<FrozenModule> {
        if let Some(module) = self.state.builtins.get(path) {
            return Ok(module.dupe());
        }

        let base_dir = self
            .state
            .base_dir
            .as_ref()
            .ok_or_else(|| anyhow!("module not found: {path}"))?;
        let normalized = normalize_load_path(&self.current_dir, path)?;
        let module_id = path_buf_to_module_id(&normalized);

        if let Some(module) = self.state.cache.borrow().get(&module_id) {
            return Ok(module.dupe());
        }

        let abs_path = base_dir.join(&normalized);
        if !abs_path.exists() {
            return Err(anyhow!("module not found: {path}"));
        }

        if self
            .state
            .loading
            .borrow()
            .iter()
            .any(|entry| entry == &module_id)
        {
            return Err(anyhow!("circular dependency loading {module_id}"));
        }

        self.state.loading.borrow_mut().push(module_id.clone());
        let _guard = LoadingGuard {
            loading: &self.state.loading,
            path: module_id.clone(),
        };

        let module = if abs_path.extension().and_then(|ext| ext.to_str()) == Some("star") {
            self.load_star_module(&module_id, &abs_path)?
        } else {
            build_asset_frozen_module(&abs_path)?
        };

        self.state
            .cache
            .borrow_mut()
            .insert(module_id, module.dupe());
        Ok(module)
    }

    fn load_star_module(&self, module_id: &str, abs_path: &Path) -> Result<FrozenModule> {
        let src = std::fs::read_to_string(abs_path)?;
        let ast =
            AstModule::parse(module_id, src, &Dialect::Standard).map_err(|e| anyhow!("{e}"))?;
        let module = Module::new();
        let child_dir = Path::new(module_id)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let loader = AppletFileLoader {
            state: Rc::clone(&self.state),
            current_dir: child_dir,
        };
        {
            let mut eval = Evaluator::new(&module);
            eval.set_loader(&loader);
            eval.eval_module(ast, self.state.globals)
                .map_err(|e| anyhow!("{e}"))?;
        }
        module
            .freeze()
            .map_err(|e| anyhow!("failed to freeze module {module_id}: {e:?}"))
    }
}

struct LoadingGuard<'a> {
    loading: &'a RefCell<Vec<String>>,
    path: String,
}

impl Drop for LoadingGuard<'_> {
    fn drop(&mut self) {
        let popped = self.loading.borrow_mut().pop();
        debug_assert_eq!(popped.as_deref(), Some(self.path.as_str()));
    }
}

fn normalize_load_path(current_dir: &Path, path: &str) -> Result<PathBuf> {
    let joined = current_dir.join(path);
    let mut normalized = PathBuf::new();

    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(anyhow!("module path escapes applet root: {path}"));
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(anyhow!("unsupported module path: {path}"));
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        return Err(anyhow!("invalid module path: {path}"));
    }

    Ok(normalized)
}

fn path_buf_to_module_id(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn build_render_frozen_module(width: u32, height: u32, is_2x: bool) -> Result<FrozenModule> {
    let render_globals = build_render_globals();

    let module = Module::new();
    let heap = module.heap();

    let mut entries: Vec<(&str, starlark::values::Value)> = render_globals
        .iter()
        .map(|(name, val)| (name, val.to_value()))
        .collect();

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

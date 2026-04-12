use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::values::Value;

use crate::json_module::{json_to_starlark, starlark_to_serde};

#[starlark::starlark_module]
pub fn yaml_module(builder: &mut GlobalsBuilder) {
    fn decode<'v>(s: &str, eval: &mut Evaluator<'v, '_, '_>) -> anyhow::Result<Value<'v>> {
        let parsed: serde_json::Value =
            serde_yaml::from_str(s).map_err(|e| anyhow::anyhow!("YAML parse error: {e}"))?;
        json_to_starlark(&parsed, eval.heap())
    }

    fn encode(value: Value, #[starlark(default = 2)] indent: i32) -> anyhow::Result<String> {
        if indent <= 0 {
            return Err(anyhow::anyhow!("indent must be positive"));
        }
        let serde = starlark_to_serde(value)?;
        let yaml =
            serde_yaml::to_string(&serde).map_err(|e| anyhow::anyhow!("YAML encode error: {e}"))?;
        if indent == 2 {
            Ok(yaml)
        } else {
            Ok(reindent_yaml(&yaml, indent as usize))
        }
    }
}

pub fn build_yaml_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(yaml_module)
        .build()
}

fn reindent_yaml(yaml: &str, indent: usize) -> String {
    let mut out = String::with_capacity(yaml.len());
    for line in yaml.lines() {
        let leading = line.chars().take_while(|c| *c == ' ').count();
        let levels = leading / 2;
        out.push_str(&" ".repeat(levels * indent));
        out.push_str(line.trim_start_matches(' '));
        out.push('\n');
    }
    out
}

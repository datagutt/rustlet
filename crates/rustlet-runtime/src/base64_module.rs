use base64::Engine;
use starlark::environment::GlobalsBuilder;

#[starlark::starlark_module]
pub fn base64_module(builder: &mut GlobalsBuilder) {
    fn encode(data: &str) -> anyhow::Result<String> {
        Ok(base64::engine::general_purpose::STANDARD.encode(data.as_bytes()))
    }

    fn decode(data: &str) -> anyhow::Result<String> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| anyhow::anyhow!("base64 decode error: {e}"))?;
        String::from_utf8(bytes).map_err(|e| anyhow::anyhow!("invalid UTF-8 in decoded data: {e}"))
    }
}

pub fn build_base64_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(base64_module)
        .build()
}

use base64::Engine;
use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::values::{Value, ValueLike};

use crate::starlark_bytes::StarlarkBytes;

#[starlark::starlark_module]
pub fn base64_module(builder: &mut GlobalsBuilder) {
    fn encode<'v>(data: Value<'v>) -> anyhow::Result<String> {
        if let Some(text) = data.unpack_str() {
            return Ok(base64::engine::general_purpose::STANDARD.encode(text.as_bytes()));
        }
        if let Some(bytes) = data.downcast_ref::<StarlarkBytes>() {
            return Ok(base64::engine::general_purpose::STANDARD.encode(&bytes.data));
        }
        Err(anyhow::anyhow!(
            "base64.encode expects string or bytes, got {}",
            data.get_type()
        ))
    }

    fn decode<'v>(data: &str, eval: &mut Evaluator<'v, '_, '_>) -> anyhow::Result<Value<'v>> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| anyhow::anyhow!("base64 decode error: {e}"))?;
        match std::str::from_utf8(&bytes) {
            Ok(text) => Ok(eval.heap().alloc(text)),
            Err(_) => Ok(eval.heap().alloc(StarlarkBytes { data: bytes })),
        }
    }
}

pub fn build_base64_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(base64_module)
        .build()
}

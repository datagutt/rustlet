use std::io::Read;

use flate2::read::GzDecoder;
use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::values::Value;
use starlark::values::ValueLike;

use crate::starlark_bytes::StarlarkBytes;

#[starlark::starlark_module]
pub fn gzip_module(builder: &mut GlobalsBuilder) {
    fn decompress<'v>(data: Value<'v>, eval: &mut Evaluator<'v, '_, '_>) -> anyhow::Result<Value<'v>> {
        let input = if let Some(text) = data.unpack_str() {
            text.as_bytes().to_vec()
        } else if let Some(bytes) = data.downcast_ref::<StarlarkBytes>() {
            bytes.data.clone()
        } else {
            return Err(anyhow::anyhow!(
                "gzip.decompress expects string or bytes, got {}",
                data.get_type()
            ));
        };

        let mut decoder = GzDecoder::new(input.as_slice());
        let mut output = Vec::new();
        decoder
            .read_to_end(&mut output)
            .map_err(|e| anyhow::anyhow!("gzip decompress error: {e}"))?;

        match std::str::from_utf8(&output) {
            Ok(text) => Ok(eval.heap().alloc(text)),
            Err(_) => Ok(eval.heap().alloc(StarlarkBytes { data: output })),
        }
    }
}

pub fn build_gzip_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(gzip_module)
        .build()
}

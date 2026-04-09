use base64::Engine;
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use starlark::environment::GlobalsBuilder;
use starlark::values::none::NoneType;
use starlark::values::{Value, ValueLike};

use crate::starlark_bytes::StarlarkBytes;

#[starlark::starlark_module]
pub fn hash_module(builder: &mut GlobalsBuilder) {
    fn md5<'v>(
        data: Value<'v>,
        #[starlark(default = NoneType)] encoding: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        hash_value::<Md5>(data, encoding, eval)
    }

    fn sha1<'v>(
        data: Value<'v>,
        #[starlark(default = NoneType)] encoding: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        hash_value::<Sha1>(data, encoding, eval)
    }

    fn sha256<'v>(
        data: Value<'v>,
        #[starlark(default = NoneType)] encoding: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        hash_value::<Sha256>(data, encoding, eval)
    }
}

pub fn build_hash_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(hash_module)
        .build()
}

fn hash_value<'v, D: Digest + Default>(
    data: Value<'v>,
    encoding: Value<'v>,
    eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
) -> anyhow::Result<Value<'v>> {
    let bytes = value_to_bytes(data)?;
    let digest = D::digest(bytes);
    encode_digest(&digest, encoding, eval)
}

fn value_to_bytes(value: Value) -> anyhow::Result<Vec<u8>> {
    if let Some(text) = value.unpack_str() {
        Ok(text.as_bytes().to_vec())
    } else if let Some(bytes) = value.downcast_ref::<StarlarkBytes>() {
        Ok(bytes.data.clone())
    } else {
        Err(anyhow::anyhow!(
            "expected string or bytes, got {}",
            value.get_type()
        ))
    }
}

fn encode_digest<'v>(
    digest: &[u8],
    encoding: Value<'v>,
    eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
) -> anyhow::Result<Value<'v>> {
    match encoding.unpack_str().unwrap_or("hex") {
        "hex" => Ok(eval.heap().alloc(hex_encode(digest))),
        "base64" => Ok(eval
            .heap()
            .alloc(base64::engine::general_purpose::STANDARD.encode(digest))),
        "binary" => Ok(eval.heap().alloc(StarlarkBytes {
            data: digest.to_vec(),
        })),
        other => Err(anyhow::anyhow!(
            "unsupported hash encoding {other:?}, expected hex, base64, or binary"
        )),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

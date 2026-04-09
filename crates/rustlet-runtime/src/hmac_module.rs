use base64::Engine;
use hmac::{Hmac, Mac};
use md5::Md5;
use sha1::Sha1;
use sha2::Sha256;
use starlark::environment::GlobalsBuilder;
use starlark::values::none::NoneType;
use starlark::values::{Value, ValueLike};

use crate::starlark_bytes::StarlarkBytes;

#[starlark::starlark_module]
pub fn hmac_module(builder: &mut GlobalsBuilder) {
    fn md5<'v>(
        key: Value<'v>,
        s: &str,
        #[starlark(default = false)] binary: bool,
        #[starlark(default = NoneType)] encoding: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let key_bytes = value_to_bytes(key)?;
        let mut mac = Hmac::<Md5>::new_from_slice(&key_bytes)
            .map_err(|e| anyhow::anyhow!("invalid HMAC key: {e}"))?;
        mac.update(s.as_bytes());
        let digest = mac.finalize().into_bytes();
        encode_digest(&digest, binary, encoding, eval)
    }

    fn sha1<'v>(
        key: Value<'v>,
        s: &str,
        #[starlark(default = false)] binary: bool,
        #[starlark(default = NoneType)] encoding: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let key_bytes = value_to_bytes(key)?;
        let mut mac = Hmac::<Sha1>::new_from_slice(&key_bytes)
            .map_err(|e| anyhow::anyhow!("invalid HMAC key: {e}"))?;
        mac.update(s.as_bytes());
        let digest = mac.finalize().into_bytes();
        encode_digest(&digest, binary, encoding, eval)
    }

    fn sha256<'v>(
        key: Value<'v>,
        s: &str,
        #[starlark(default = false)] binary: bool,
        #[starlark(default = NoneType)] encoding: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let key_bytes = value_to_bytes(key)?;
        let mut mac = Hmac::<Sha256>::new_from_slice(&key_bytes)
            .map_err(|e| anyhow::anyhow!("invalid HMAC key: {e}"))?;
        mac.update(s.as_bytes());
        let digest = mac.finalize().into_bytes();
        encode_digest(&digest, binary, encoding, eval)
    }
}

pub fn build_hmac_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(hmac_module)
        .build()
}

fn encode_digest<'v>(
    digest: &[u8],
    binary: bool,
    encoding: Value<'v>,
    eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
) -> anyhow::Result<Value<'v>> {
    let encoding = if binary {
        "binary"
    } else {
        encoding.unpack_str().unwrap_or("hex")
    };

    match encoding {
        "hex" => Ok(eval.heap().alloc(hex_encode(&digest))),
        "base64" => Ok(eval
            .heap()
            .alloc(base64::engine::general_purpose::STANDARD.encode(digest))),
        "binary" => Ok(eval.heap().alloc(StarlarkBytes {
            data: digest.to_vec(),
        })),
        other => Err(anyhow::anyhow!(
            "unsupported hmac encoding {other:?}, expected hex, base64, or binary"
        )),
    }
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

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

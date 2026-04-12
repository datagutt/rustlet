use std::io::Cursor;

use allocative::Allocative;
use base64::Engine as _;
use starlark::environment::GlobalsBuilder;
use starlark::eval::Evaluator;
use starlark::values::Value;
use tink_core::keyset::{Handle, JsonReader, JsonWriter};
use tink_core::{Aead, HybridDecrypt};

use crate::execution_context::{current_app_id, with_secret_decrypter};

#[derive(Allocative)]
pub struct SecretDecryptionKey {
    #[allocative(skip)]
    pub encrypted_keyset_json: Vec<u8>,
    #[allocative(skip)]
    pub key_encryption_key: Box<dyn Aead>,
}

#[derive(Clone, Allocative)]
pub struct SecretEncryptionKey {
    #[allocative(skip)]
    pub public_keyset_json: Vec<u8>,
}

impl Clone for SecretDecryptionKey {
    fn clone(&self) -> Self {
        Self {
            encrypted_keyset_json: self.encrypted_keyset_json.clone(),
            key_encryption_key: self.key_encryption_key.box_clone(),
        }
    }
}

impl SecretEncryptionKey {
    pub fn encrypt(&self, app_id: &str, plaintext: &str) -> anyhow::Result<String> {
        tink_hybrid::init();

        let mut reader = JsonReader::new(Cursor::new(&self.public_keyset_json));
        let handle = Handle::read_with_no_secrets(&mut reader)
            .map_err(|e| anyhow::anyhow!("reading keyset JSON: {e}"))?;
        let encryptor = tink_hybrid::new_encrypt(&handle)
            .map_err(|e| anyhow::anyhow!("NewHybridEncrypt: {e}"))?;
        let ciphertext = encryptor
            .encrypt(plaintext.as_bytes(), app_id.as_bytes())
            .map_err(|e| anyhow::anyhow!("encrypting secret: {e}"))?;
        Ok(base64::engine::general_purpose::STANDARD.encode(ciphertext))
    }
}

impl SecretDecryptionKey {
    pub(crate) fn decrypter_for_app(
        &self,
        _app_id: &str,
    ) -> anyhow::Result<Box<dyn HybridDecrypt>> {
        tink_hybrid::init();

        let mut reader = JsonReader::new(Cursor::new(&self.encrypted_keyset_json));
        let handle = Handle::read(&mut reader, self.key_encryption_key.box_clone())
            .map_err(|e| anyhow::anyhow!("reading keyset JSON: {e}"))?;
        tink_hybrid::new_decrypt(&handle).map_err(|e| anyhow::anyhow!("NewHybridDecrypt: {e}"))
    }

    pub fn from_private_keyset_json(
        encrypted_keyset_json: Vec<u8>,
        key_encryption_key: Box<dyn Aead>,
    ) -> Self {
        Self {
            encrypted_keyset_json,
            key_encryption_key,
        }
    }

    pub fn to_public_key(&self) -> anyhow::Result<SecretEncryptionKey> {
        tink_hybrid::init();

        let mut reader = JsonReader::new(Cursor::new(&self.encrypted_keyset_json));
        let handle = Handle::read(&mut reader, self.key_encryption_key.box_clone())
            .map_err(|e| anyhow::anyhow!("reading keyset JSON: {e}"))?;
        let public = handle
            .public()
            .map_err(|e| anyhow::anyhow!("creating public keyset: {e}"))?;
        let mut out = Vec::new();
        let mut writer = JsonWriter::new(&mut out);
        public
            .write_with_no_secrets(&mut writer)
            .map_err(|e| anyhow::anyhow!("writing public keyset JSON: {e}"))?;
        Ok(SecretEncryptionKey {
            public_keyset_json: out,
        })
    }
}

#[starlark::starlark_module]
pub fn secret_module(builder: &mut GlobalsBuilder) {
    fn decrypt<'v>(value: &str, eval: &mut Evaluator<'v, '_, '_>) -> anyhow::Result<Value<'v>> {
        let compact = value
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>();
        let ciphertext = base64::engine::general_purpose::STANDARD
            .decode(compact)
            .map_err(|e| anyhow::anyhow!("base64 decoding of secret: {value}: {e}"))?;

        let Some(cleartext) = with_secret_decrypter(|decrypter| {
            let Some(decrypter) = decrypter else {
                return Ok(None);
            };
            let decrypted = decrypter
                .decrypt(&ciphertext, current_app_id().as_bytes())
                .map_err(|e| anyhow::anyhow!("decrypting secret {value}: {e}"))?;
            Ok(Some(decrypted))
        })?
        else {
            return Ok(Value::new_none());
        };

        let cleartext = String::from_utf8(cleartext)
            .map_err(|e| anyhow::anyhow!("secret is not valid UTF-8: {e}"))?;
        Ok(eval.heap().alloc(cleartext))
    }
}

pub fn build_secret_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(secret_module)
        .build()
}

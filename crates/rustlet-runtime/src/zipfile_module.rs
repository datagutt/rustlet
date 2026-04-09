use std::fmt;
use std::io::Read;

use allocative::Allocative;
use flate2::read::DeflateDecoder;
use starlark::environment::{GlobalsBuilder, Methods, MethodsBuilder, MethodsStatic};
use starlark::starlark_simple_value;
use starlark::values::tuple::AllocTuple;
use starlark::values::{NoSerialize, ProvidesStaticType, StarlarkValue, Value, ValueLike};
use starlark_derive::starlark_value;

use crate::starlark_bytes::StarlarkBytes;

#[derive(Debug, Clone, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkZipArchive {
    #[allocative(skip)]
    bytes: Vec<u8>,
}

#[derive(Debug, Clone, ProvidesStaticType, NoSerialize, Allocative)]
pub struct StarlarkZipEntry {
    #[allocative(skip)]
    data: Vec<u8>,
}

starlark_simple_value!(StarlarkZipArchive);
starlark_simple_value!(StarlarkZipEntry);

impl fmt::Display for StarlarkZipArchive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<zipfile archive>")
    }
}

impl fmt::Display for StarlarkZipEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<zipfile entry>")
    }
}

#[starlark_value(type = "zipfile_archive")]
impl<'v> StarlarkValue<'v> for StarlarkZipArchive {
    fn get_methods() -> Option<&'static Methods> {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(zip_archive_methods)
    }
}

#[starlark_value(type = "zipfile_entry")]
impl<'v> StarlarkValue<'v> for StarlarkZipEntry {
    fn get_methods() -> Option<&'static Methods> {
        static RES: MethodsStatic = MethodsStatic::new();
        RES.methods(zip_entry_methods)
    }
}

#[starlark::starlark_module]
fn zip_archive_methods(builder: &mut MethodsBuilder) {
    fn namelist<'v>(
        #[starlark(this)] this: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let archive = unpack_archive(this)?;
        let names = parse_central_directory(&archive.bytes)?
            .into_iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>();
        let values = names
            .into_iter()
            .map(|name| eval.heap().alloc(name))
            .collect::<Vec<_>>();
        Ok(eval.heap().alloc(AllocTuple(values)))
    }

    fn open<'v>(
        #[starlark(this)] this: Value<'v>,
        name: &str,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let archive = unpack_archive(this)?;
        let entry = parse_central_directory(&archive.bytes)?
            .into_iter()
            .find(|entry| entry.name == name)
            .ok_or_else(|| anyhow::anyhow!("zip entry not found: {name}"))?;
        let data = read_entry_data(&archive.bytes, &entry)?;
        Ok(eval.heap().alloc(StarlarkZipEntry { data }))
    }
}

#[starlark::starlark_module]
fn zip_entry_methods(builder: &mut MethodsBuilder) {
    fn read<'v>(
        #[starlark(this)] this: Value<'v>,
        eval: &mut starlark::eval::Evaluator<'v, '_, '_>,
    ) -> anyhow::Result<Value<'v>> {
        let entry = this
            .downcast_ref::<StarlarkZipEntry>()
            .ok_or_else(|| anyhow::anyhow!("expected zipfile entry"))?;
        match std::str::from_utf8(&entry.data) {
            Ok(text) => Ok(eval.heap().alloc(text)),
            Err(_) => Ok(eval.heap().alloc(StarlarkBytes {
                data: entry.data.clone(),
            })),
        }
    }
}

#[starlark::starlark_module]
pub fn zipfile_module(builder: &mut GlobalsBuilder) {
    fn ZipFile<'v>(data: Value<'v>) -> anyhow::Result<StarlarkZipArchive> {
        let bytes = if let Some(text) = data.unpack_str() {
            text.as_bytes().to_vec()
        } else if let Some(bytes) = data.downcast_ref::<StarlarkBytes>() {
            bytes.data.clone()
        } else {
            return Err(anyhow::anyhow!(
                "ZipFile expects string or bytes, got {}",
                data.get_type()
            ));
        };

        parse_central_directory(&bytes)?;
        Ok(StarlarkZipArchive { bytes })
    }
}

pub fn build_zipfile_globals() -> starlark::environment::Globals {
    starlark::environment::GlobalsBuilder::new()
        .with(zipfile_module)
        .build()
}

fn unpack_archive(value: Value<'_>) -> anyhow::Result<&StarlarkZipArchive> {
    value
        .downcast_ref::<StarlarkZipArchive>()
        .ok_or_else(|| anyhow::anyhow!("expected zipfile archive, got {}", value.get_type()))
}

#[derive(Debug, Clone)]
struct ZipEntryInfo {
    name: String,
    compression_method: u16,
    compressed_size: usize,
    local_header_offset: usize,
}

fn parse_central_directory(bytes: &[u8]) -> anyhow::Result<Vec<ZipEntryInfo>> {
    let eocd_offset = bytes
        .windows(4)
        .rposition(|window| window == [0x50, 0x4b, 0x05, 0x06])
        .ok_or_else(|| anyhow::anyhow!("invalid zip archive: missing end of central directory"))?;

    let entry_count = read_u16(bytes, eocd_offset + 10)? as usize;
    let central_directory_offset = read_u32(bytes, eocd_offset + 16)? as usize;
    let mut cursor = central_directory_offset;
    let mut entries = Vec::with_capacity(entry_count);

    for _ in 0..entry_count {
        if read_u32(bytes, cursor)? != 0x0201_4b50 {
            return Err(anyhow::anyhow!(
                "invalid zip archive: missing central directory header"
            ));
        }

        let compression_method = read_u16(bytes, cursor + 10)?;
        let compressed_size = read_u32(bytes, cursor + 20)? as usize;
        let file_name_length = read_u16(bytes, cursor + 28)? as usize;
        let extra_field_length = read_u16(bytes, cursor + 30)? as usize;
        let file_comment_length = read_u16(bytes, cursor + 32)? as usize;
        let local_header_offset = read_u32(bytes, cursor + 42)? as usize;
        let name_start = cursor + 46;
        let name_end = name_start + file_name_length;
        let name = std::str::from_utf8(bytes.get(name_start..name_end).ok_or_else(|| {
            anyhow::anyhow!("invalid zip archive: central directory file name out of bounds")
        })?)?
        .to_owned();

        entries.push(ZipEntryInfo {
            name,
            compression_method,
            compressed_size,
            local_header_offset,
        });

        cursor = name_end + extra_field_length + file_comment_length;
    }

    Ok(entries)
}

fn read_entry_data(bytes: &[u8], entry: &ZipEntryInfo) -> anyhow::Result<Vec<u8>> {
    let offset = entry.local_header_offset;
    if read_u32(bytes, offset)? != 0x0403_4b50 {
        return Err(anyhow::anyhow!(
            "invalid zip archive: missing local file header"
        ));
    }

    let file_name_length = read_u16(bytes, offset + 26)? as usize;
    let extra_field_length = read_u16(bytes, offset + 28)? as usize;
    let data_start = offset + 30 + file_name_length + extra_field_length;
    let data_end = data_start + entry.compressed_size;
    let compressed = bytes
        .get(data_start..data_end)
        .ok_or_else(|| anyhow::anyhow!("invalid zip archive: file data out of bounds"))?;

    match entry.compression_method {
        0 => Ok(compressed.to_vec()),
        8 => {
            let mut decoder = DeflateDecoder::new(compressed);
            let mut decoded = Vec::new();
            decoder.read_to_end(&mut decoded)?;
            Ok(decoded)
        }
        method => Err(anyhow::anyhow!(
            "unsupported zip compression method: {method}"
        )),
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> anyhow::Result<u16> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| anyhow::anyhow!("invalid zip archive: unexpected EOF"))?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> anyhow::Result<u32> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| anyhow::anyhow!("invalid zip archive: unexpected EOF"))?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

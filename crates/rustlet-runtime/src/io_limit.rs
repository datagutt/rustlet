//! Bounded reads for decompression entry points. Callers inflate
//! attacker-influenced compressed data, whose decompressed size is otherwise
//! unbounded — a small input can expand to gigabytes (a "decompression bomb").
//! Reading through a hard cap turns that OOM into a recoverable error.

use std::io::Read;

/// Safety cap for a single decompression. Generous for legitimate pixlet apps
/// (display data is tiny; even bundled datasets are well under this) while
/// bounding worst-case memory per call.
pub const MAX_DECOMPRESSED_BYTES: usize = 64 * 1024 * 1024; // 64 MiB

/// Read all bytes from `reader`, failing if the output would exceed `max`.
pub fn read_to_end_limited<R: Read>(reader: R, max: usize) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::new();
    // take(max + 1): if we manage to read more than `max`, it's over the cap.
    let n = reader.take(max as u64 + 1).read_to_end(&mut out)?;
    if n > max {
        return Err(anyhow::anyhow!(
            "decompressed data exceeds {max} byte limit"
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_when_under_limit() {
        let data = b"hello world";
        let out = read_to_end_limited(&data[..], 100).unwrap();
        assert_eq!(out, data);
    }

    #[test]
    fn reads_when_exactly_at_limit() {
        let data = b"abcde"; // 5 bytes
        let out = read_to_end_limited(&data[..], 5).unwrap();
        assert_eq!(out, data);
    }

    #[test]
    fn errors_when_over_limit() {
        let data = b"abcdef"; // 6 bytes
        let err = read_to_end_limited(&data[..], 5).unwrap_err();
        assert!(err.to_string().contains("limit"), "got: {err}");
    }
}

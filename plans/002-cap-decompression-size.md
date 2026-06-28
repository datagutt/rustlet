# Plan 002: Cap decompression output to defuse gzip/zip decompression bombs

> **Status: DONE** — reviewed; commit `9c789d1` on branch
> `worktree-agent-a91acabbbeff65015` (off `ef1cf8f`), not yet merged. Record kept.

## Status

- **Priority**: P2 | **Effort**: S–M | **Risk**: MED | **Depends on**: 001 (merged) | **Category**: security
- **Planned at**: commit `c6e025f`, 2026-06-28

## Why this matters

`gzip.decompress(data)` and the zip-entry reader inflated caller-supplied compressed
bytes with `read_to_end` and NO size limit. An applet routinely feeds these data
fetched over the network (`http.get(...)` → `gzip.decompress(...)`), so an attacker
controlling the upstream can send a small decompression bomb that inflates to
gigabytes — a DoS in the long-running `serve`/`api` processes. Capping turns an OOM
into a clean error. (The HTTP response body path is bounded by ureq — not in scope.)

## What changed (as executed)

- New `crates/rustlet-runtime/src/io_limit.rs`: `MAX_DECOMPRESSED_BYTES = 64 MiB` and
  `read_to_end_limited<R: Read>(reader, max)` using `reader.take(max+1).read_to_end(..)`
  then erroring if `n > max`. Includes 3 unit tests (under/at/over limit).
- `crates/rustlet-runtime/src/lib.rs`: added `mod io_limit;` (between `i18n_module` and `json_module`).
- `gzip_module.rs` and `zipfile_module.rs` (deflate branch): route inflation through
  `crate::io_limit::read_to_end_limited(...)`. The now-unused `use std::io::Read;`
  imports were removed from both (necessary; in-scope).
- The stored (method 0) zip branch was left unchanged (already bounded).

## Scope

**In scope**: `io_limit.rs` (new), `lib.rs`, `gzip_module.rs`, `zipfile_module.rs`,
`plans/README.md`. **Out of scope**: `http_module.rs`; the stored zip branch; public
Starlark signatures.

## Done criteria (met)

- [x] `cargo build` exits 0
- [x] `cargo test -p rustlet-runtime` passes incl 3 new io_limit tests (reviewer-run on stable: exit 0)
- [x] fmt --check (stable) exit 0
- [x] clippy clean (base proven clippy-clean by sibling plans' `--workspace` runs; 002 adds only clean code)
- [x] `read_to_end_limited` used in both gzip + zipfile; old unbounded gzip call gone
- [x] Only the 4 in-scope files changed

## Maintenance notes

- 64 MiB is a single named const; raise in one place with justification if a real app
  needs more — never remove the cap.
- Route any future decompression (brotli/zstd/nested archives) through `read_to_end_limited`.

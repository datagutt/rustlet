# Plan 004: Make `magnify` reject overflowing dimensions instead of panicking

> **Status: DONE** — reviewed; commit `0db5512` on branch
> `worktree-agent-a4203a52a59820159` (off `ef1cf8f`), not yet merged. Record kept.

## Status

- **Priority**: P2 | **Effort**: S | **Risk**: LOW | **Depends on**: 001 (merged) | **Category**: bug
- **Planned at**: commit `c6e025f`, 2026-06-28

## Why this matters

`magnify(frames, factor)` computed `w * factor` / `h * factor` on `u32` with no
overflow guard. A large `--magnify` overflowed: debug panics; release wraps to a
small dimension, `Pixmap::new` succeeds wrong, then the copy loop panics on an
out-of-bounds slice range. Either way the render crashed. Returning a `Result` lets
the CLI/API surface a readable error.

## What changed (as executed)

`crates/rustlet-encode/src/filter.rs`:
- `pub fn magnify(...) -> anyhow::Result<Vec<Pixmap>>`; `checked_mul` on both `w`/`h`
  with `ok_or_else(|| anyhow::anyhow!("magnify overflow: ..."))`; `Pixmap::new(...)
  .ok_or_else(...)` instead of `.expect(...)`; `Ok(out)` in the closure; `Ok(frames.to_vec())`
  for `factor <= 1`. The pixel-copy loop is byte-identical.
- New test `magnify_overflow_returns_error` (`u32::MAX` → `Err`).
- 3 existing test callers now `.unwrap()`.

`crates/rustlet-cli/src/util.rs` and `crates/rustlet-cli/src/main.rs`: the two
production callers now `rustlet_encode::magnify(&frames, magnify)?` (both enclosing fns
already return `Result`).

## Scope

**In scope**: `filter.rs`, `cli/util.rs`, `cli/main.rs`, `plans/README.md`.
**Out of scope**: the pixel-copy logic; `image_widget.rs`; a CLI bound on `--magnify`.

## Done criteria (met)

- [x] `cargo build` exits 0
- [x] `cargo test -p rustlet-encode` 19/19 incl the new overflow test
- [x] `rustup run stable cargo clippy --workspace --all-targets` exits 0
- [x] `rustup run stable cargo fmt --check` exits 0
- [x] old `let new_w = w * factor` gone; both callers use `?`
- [x] only the 3 in-scope files changed

## Maintenance notes

- The inner copy loop is unchanged; the error path is unreachable for legitimate
  factors (1–4). If `magnify` gains a streaming variant, apply the same `checked_mul`.

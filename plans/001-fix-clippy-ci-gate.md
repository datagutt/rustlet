# Plan 001: Make `cargo clippy --workspace --all-targets` pass so the CI clippy gate is green again

> **Status: DONE** — merged into main as `4197ae5`. File kept as the record.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: dx / correctness
- **Planned at**: commit `c6e025f`, 2026-06-28

## Why this matters

`cargo clippy --workspace --all-targets` exited 101 — the exact command CI's
"Clippy" step runs (`.github/workflows/ci.yml`) — so the gate was dead. The
failures were all deny-by-default correctness lints firing in test code (clippy's
`correctness` group is `deny`, not `warn`), not real logic bugs:

- `clippy::erasing_op` ("always returns zero") on pixel-index expressions like `(0 * 64 + 60)`.
- `clippy::approx_constant` ("approximate value of PI") on test-data literals like `3.14`/`3.14159`.

## Current state (at plan time)

19 error-level lints, all inside `#[cfg(test)]` modules. Production code emitted
only clippy *warnings* (out of scope).

`clippy::erasing_op` — 6 sites (`(0 * W + C)`):
- `crates/rustlet-render/src/render/vector.rs` — 5 sites (`(0*64+60/59/30/29/0)`)
- `crates/rustlet-render/src/render/anim/transformation.rs` — `(0 * 4 + 2)`

`clippy::approx_constant` — 13 sites in test fns: `plot.rs` (`compute_limits_from_data`,
`compute_limits_explicit`, `compute_limits_equal_y`, `compute_limits_equal_x`) and
`humanize_module.rs` (`test_format_float_pattern`).

## Commands

| Purpose       | Command                                          | Expected            |
|---------------|--------------------------------------------------|---------------------|
| Clippy (gate) | `cargo clippy --workspace --all-targets`         | exit 0              |
| Tests         | `cargo test --workspace`                          | all pass            |

> **Toolchain note**: CI uses **stable**. If your default `cargo` is nightly, run
> `rustup run stable cargo clippy --workspace --all-targets` to match CI. The
> lints fire on both, so the fix is toolchain-independent.
>
> **Do NOT run `cargo fmt`** in this plan — repo-wide fmt drift is plan 008's job.

## Scope

**In scope** (test modules only): `vector.rs`, `anim/transformation.rs`, `plot.rs`,
`humanize_module.rs`, `plans/README.md`.
**Out of scope**: any production code; the clippy *warnings* (do not `cargo clippy --fix`).

## Steps (as executed)

1. Removed the `0 * W` factor from the 6 pixel-index expressions
   (`(0 * 64 + 60) as usize` → `60`, etc.). `(2 * 64 + 0)` at `vector.rs` left alone.
2. Added `#[allow(clippy::approx_constant)]` (with an explanatory comment) above the
   5 affected test functions.
3. Verified: stable `cargo clippy --workspace --all-targets` → exit 0; tests pass.

## Done criteria (met)

- [x] `cargo clippy --workspace --all-targets` exits 0 (verified on stable)
- [x] `cargo test --workspace` exits 0
- [x] `git grep "0 \* 64 + "` / `"0 \* 4 + 2"` return nothing
- [x] Only the 4 test files changed

## Maintenance notes

- Keeping the gate green is the real fix; consider `-D warnings` in CI only after
  the ~61 existing warnings are cleared (separate work).
- New tests with `0 * W` index math or near-PI literals will trip the same lints;
  prefer plain literals and a commented `#[allow(clippy::approx_constant)]` for
  genuinely coincidental data.

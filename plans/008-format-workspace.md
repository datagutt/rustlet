# Plan 008: Format the workspace so the CI `cargo fmt --check` gate passes

> **Status: DONE** — merged into main as `490121c`. File kept as the record.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none (`cargo fmt` is idempotent)
- **Category**: dx
- **Planned at**: commit `0d2c4d2`, 2026-06-28 (discovered while executing plan 001)

## Why this matters

`cargo fmt --check` exited non-zero at HEAD: ~35 files (120 diff hunks) were not
formatted to rustfmt's output, on **both** stable and nightly. CI's `test` job runs
`cargo fmt --check` as its **first** step (before clippy/tests), so the whole job
failed immediately. Pure formatting drift (no semantic change); fixed by a single
`cargo fmt` pass on the CI (stable) toolchain.

## Key constraint

No `rust-toolchain.toml` exists; local default is nightly but **CI uses stable**.
Format with the **stable** `rustfmt` (`rustup run stable cargo fmt`) — nightly
output can differ and would leave CI red.

## Scope

**In scope**: every file `cargo fmt` reformats (workspace-wide) — the one plan where
a broad diff is expected. **Out of scope**: any non-formatting edit; `rustfmt.toml`.

## Steps (as executed)

1. Confirmed stable rustfmt (`rustfmt 1.9.0-stable`).
2. Ran `rustup run stable cargo fmt`; `rustup run stable cargo fmt --check` → exit 0.
3. Confirmed formatting-only via `git diff --ignore-all-space` (only reflow + trailing
   commas); `cargo build` exit 0; main-crate tests pass.

## Done criteria (met)

- [x] `rustup run stable cargo fmt --check` exits 0
- [x] `rustup run stable cargo build --workspace` exits 0
- [x] Tests pass (4 main crates; `rustlet-compat` needs the Go pixlet binary — CI-only)
- [x] Diff is formatting-only (35 files, 422/-316), commit message `style(rustlet): format workspace with cargo fmt`

## Maintenance notes

- Root cause: formatting not enforced locally before commits. A cheap follow-up is a
  pre-commit hook running `cargo fmt --check`.
- Reviewer can approve on "tests pass + diff is formatting-only".

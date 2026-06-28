# Plan 003: Give the in-memory HTTP cache a real hard size bound

> **Status: DONE** — reviewed; commit `c6747bf` on branch
> `worktree-agent-a47fdeb7f8fd45179` (off `ef1cf8f`), not yet merged. Record kept.

## Status

- **Priority**: P2 | **Effort**: S | **Risk**: LOW | **Depends on**: 001 (merged) | **Category**: correctness / perf
- **Planned at**: commit `c6e025f`, 2026-06-28

## Why this matters

`CLAUDE.md` documents the HTTP cache as "evicted at 256 entries", but the code never
enforced a hard cap: at 256 entries it only pruned *expired* ones, so many distinct
long-TTL requests grew the cache without bound — a slow memory leak in the
long-running `serve`/`api` processes. This makes the documented cap real.

## What changed (as executed)

`crates/rustlet-runtime/src/http_module.rs`:
- Added `const MAX_CACHE_ENTRIES: usize = 256;`.
- Added `fn enforce_capacity(cache: &mut HashMap<u64, CachedResponse>)`: returns early
  under cap; else prunes expired, then evicts the soonest-to-expire entries
  (`min_by_key(|(_, v)| v.expires_at)`) until under cap.
- `put_cached` now calls `enforce_capacity(&mut cache)` before insert (replacing the
  buggy `if cache.len() > 256 { retain expired }`). CachedResponse construction unchanged.
- New `#[cfg(test)] mod tests` with `enforce_capacity_bounds_size_and_evicts_soonest_expiring`.

## Scope

**In scope**: `http_module.rs`, `plans/README.md`. **Out of scope**: cache key fn,
request/response shaping, TTL parsing, `get_cached`; no LRU/external cache crate.

## Done criteria (met)

- [x] `cargo build` exits 0
- [x] `cargo test -p rustlet-runtime` passes (104 tests incl the new eviction test)
- [x] `rustup run stable cargo clippy --workspace --all-targets` exits 0
- [x] `rustup run stable cargo fmt --check` exits 0
- [x] `cache.len() > 256` gone; `MAX_CACHE_ENTRIES` defined + used
- [x] Only `http_module.rs` changed

## Maintenance notes

- Stopgap honoring the documented 256-entry cap; the roadmap's "shared render cache"
  would supersede it.
- Soonest-expiry eviction is O(n) but only runs at capacity (n ≤ 256). If the cap is
  raised by orders of magnitude, revisit with an LRU structure.

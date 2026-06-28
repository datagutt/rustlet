# Plan 006: Fix `unix_to_datetime` so pre-1970 (negative) timestamps decode correctly

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat c6e025f..HEAD -- crates/rustlet-runtime/src/starlark_time.rs crates/rustlet-runtime/src/time_module.rs`
> Main has advanced past `c6e025f` (a workspace `cargo fmt` reformatted many
> files), so line numbers below may have shifted. Match on the code *content* of
> the "Current state" excerpts, not line numbers; on a content mismatch, treat it
> as a STOP condition.

## Status

- **Priority**: P3
- **Effort**: M
- **Risk**: MED (calendar arithmetic; thorough tests required)
- **Depends on**: 001 (merged)
- **Category**: bug / pixlet-compat
- **Planned at**: commit `c6e025f`, 2026-06-28 (verify against current HEAD)

## Why this matters

`unix_to_datetime` clamps any negative timestamp to `0`, so every instant before
1970-01-01 decodes as 1970-01-01 00:00:00. Its inverse, `datetime_to_unix`,
already handles years before 1970 correctly, so the pair is asymmetric and any
applet handling historical dates (or doing time arithmetic that crosses the
epoch) silently gets wrong results. Pixlet, built on Go's `time` package, handles
pre-1970 instants, so this is also a compatibility gap. The fix makes the decoder
symmetric with the encoder using Euclidean division.

## Current state

`crates/rustlet-runtime/src/starlark_time.rs`:

- The broken decoder:
  ```rust
  pub fn unix_to_datetime(mut ts: i64) -> (i64, i64, i64, i64, i64, i64) {
      let negative = ts < 0;
      if negative {
          ts = 0;
      }

      let sec = ts % 60;
      ts /= 60;
      let min = ts % 60;
      ts /= 60;
      let hour = ts % 24;
      let mut days = ts / 24;

      let mut year = 1970i64;
      loop {
          let days_in_year = if is_leap(year) { 366 } else { 365 };
          if days < days_in_year {
              break;
          }
          days -= days_in_year;
          year += 1;
      }

      let leap = is_leap(year);
      let month_days: [i64; 12] = [
          31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
      ];

      let mut month = 0i64;
      for (i, &md) in month_days.iter().enumerate() {
          if days < md {
              month = i as i64 + 1;
              break;
          }
          days -= md;
      }
      let day = days + 1;

      (year, month, day, hour, min, sec)
  }
  ```
- The matching encoder `datetime_to_unix` already loops `for y in year..1970 { days -= ... }` for years before 1970, so it is correct and is the round-trip oracle.
- `pub fn is_leap(year: i64) -> bool` is available.
- Callers of `unix_to_datetime` (all fixed transitively): `humanize_module.rs`
  (one call), `starlark_time.rs` (`local_secs()` path), `time_module.rs` (two calls).

`crates/rustlet-runtime/src/time_module.rs` has a `#[cfg(test)] mod tests` that
already imports these helpers (`use crate::starlark_time::{datetime_to_unix, parse_iso8601, unix_to_datetime};`)
and tests `unix_to_datetime`. New tests go there.

Repo conventions: time math is hand-rolled by design; conventional-commit
messages scoped by package.

## Commands you will need

| Purpose       | Command                                         | Expected on success |
|---------------|-------------------------------------------------|---------------------|
| Build         | `cargo build`                                   | exit 0              |
| Tests (crate) | `cargo test -p rustlet-runtime`                 | all pass            |
| Clippy        | `rustup run stable cargo clippy --workspace --all-targets` | exit 0   |
| Format        | `rustup run stable cargo fmt` then `--check`    | exit 0              |

(CI uses **stable**; this machine defaults to nightly — use `rustup run stable`.)

## Scope

**In scope**: `crates/rustlet-runtime/src/starlark_time.rs` (the `unix_to_datetime`
body), `crates/rustlet-runtime/src/time_module.rs` (new tests), `plans/README.md`.

**Out of scope**: `datetime_to_unix`, `is_leap`, `weekday`, `parse_iso8601`
(correct; used as oracle); timezone/offset handling, DST, formatting.

## Git workflow

- Branch: commit on your worktree branch. Message: `fix(runtime): decode pre-1970 timestamps in unix_to_datetime`. Do NOT push or open a PR.

## Steps

### Step 1: Rewrite `unix_to_datetime` to handle negative timestamps

```rust
pub fn unix_to_datetime(ts: i64) -> (i64, i64, i64, i64, i64, i64) {
    // Euclidean division keeps the time-of-day non-negative for negative
    // timestamps: e.g. ts = -1 is 1969-12-31 23:59:59, not a negative h/m/s.
    let secs_of_day = ts.rem_euclid(86400);
    let sec = secs_of_day % 60;
    let min = (secs_of_day / 60) % 60;
    let hour = secs_of_day / 3600;
    let mut days = ts.div_euclid(86400); // may be negative (days before epoch)

    let mut year = 1970i64;
    if days >= 0 {
        loop {
            let days_in_year = if is_leap(year) { 366 } else { 365 };
            if days < days_in_year {
                break;
            }
            days -= days_in_year;
            year += 1;
        }
    } else {
        // Borrow whole years until the remaining day-of-year is non-negative.
        while days < 0 {
            year -= 1;
            days += if is_leap(year) { 366 } else { 365 };
        }
    }

    let leap = is_leap(year);
    let month_days: [i64; 12] = [
        31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31,
    ];

    let mut month = 0i64;
    for (i, &md) in month_days.iter().enumerate() {
        if days < md {
            month = i as i64 + 1;
            break;
        }
        days -= md;
    }
    let day = days + 1;

    (year, month, day, hour, min, sec)
}
```

(The signature drops `mut` from the parameter.)

**Verify**: `cargo build -p rustlet-runtime` → exit 0.

### Step 2: Add tests for the negative path and round-trip symmetry

In `time_module.rs`'s existing `#[cfg(test)] mod tests` block:

```rust
#[test]
fn unix_to_datetime_one_second_before_epoch() {
    assert_eq!(unix_to_datetime(-1), (1969, 12, 31, 23, 59, 59));
}

#[test]
fn unix_to_datetime_one_day_before_epoch() {
    assert_eq!(unix_to_datetime(-86400), (1969, 12, 31, 0, 0, 0));
}

#[test]
fn unix_to_datetime_epoch_unchanged() {
    assert_eq!(unix_to_datetime(0), (1970, 1, 1, 0, 0, 0));
}

#[test]
fn unix_to_datetime_roundtrips_across_epoch() {
    for ts in [
        -1_i64, -86_400, -1_000_000, -63_072_000, -126_230_400, 0, 1, 1_700_000_000,
    ] {
        let (y, mo, d, h, mi, s) = unix_to_datetime(ts);
        let back = datetime_to_unix(y, mo, d, h, mi, s);
        assert_eq!(back, ts, "roundtrip failed for ts={ts}: {y}-{mo}-{d} {h}:{mi}:{s}");
    }
}
```

**Verify**: `cargo test -p rustlet-runtime time_module` → all pass.

### Step 3: Full verification

- `cargo test -p rustlet-runtime` → all pass
- `rustup run stable cargo clippy --workspace --all-targets` → exit 0
- `rustup run stable cargo fmt --check` → exit 0

## Test plan

New tests: one-second-before-epoch, one-day-before-epoch, epoch-unchanged, and a
round-trip loop over negative and positive timestamps (oracle = the
already-correct `datetime_to_unix`). Existing time/humanize tests must still pass.

## Done criteria (ALL must hold)

- [ ] `cargo build` exits 0
- [ ] `cargo test -p rustlet-runtime` passes incl the 4 new tests
- [ ] `rustup run stable cargo clippy --workspace --all-targets` exits 0
- [ ] `git grep -n "ts = 0;" -- crates/rustlet-runtime/src/starlark_time.rs` returns nothing (clamp gone)
- [ ] No files outside the in-scope list modified
- [ ] `plans/README.md` status row updated

## STOP conditions

- The round-trip test fails for any timestamp — report the failing `ts` and
  decoded tuple; do not tweak constants until it passes.
- An existing test asserted the old clamp-to-epoch behavior — report it.
- The "Current state" excerpt content doesn't match live code (drift).

## Maintenance notes

- `unix_to_datetime` and `datetime_to_unix` are now mutually inverse across the
  epoch; keep them that way (any change needs a round-trip test).
- Does not touch timezone offsets; a localized pre-1970 time still flows through
  the existing offset logic.

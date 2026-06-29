# Plan 009: Include the server response body in device API client error messages

> **Drift check (run first)**:
> `git diff --stat 9eb01ad..HEAD -- crates/rustlet-cli/src/api.rs`
> Match the "Current state" excerpt on content; on mismatch, treat as STOP.

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: 005 (merged — its test `non_2xx_response_is_error_with_status_and_body` currently fails and this plan makes it pass)
- **Category**: bug
- **Planned at**: commit `9eb01ad`, 2026-06-29

## Why this matters

The device API client builds its `ureq::Agent` without disabling ureq's
"treat non-2xx HTTP status as an error" default. So for any 4xx/5xx response,
`.call()? / .send()?` returns an error **before** the client's `read_success_body`
/ `expect_2xx` functions run — and those functions are the ones that read the
response body into the error message (`"{op} failed: HTTP {status}: {body}"`).
The result: every device command (`push`, `devices`, `list`, `delete`) reports
only the bare HTTP status and **drops the server's explanation body**, which is
usually the useful part when a Tidbyt/Tronbyt API call fails. Plan 005's test
`non_2xx_response_is_error_with_status_and_body` caught this (it asserts the body
`boom` appears in the error; today it does not). This restores the
clearly-intended behavior so the existing body-reading code actually runs.

## Current state

`crates/rustlet-cli/src/api.rs`, `Client::new` builds the agent:

```rust
let agent: ureq::Agent = ureq::Agent::config_builder()
    .timeout_global(Some(REQUEST_TIMEOUT))
    .user_agent(crate::util::user_agent())
    .build()
    .into();
```

`devices()`/`installations()` call `.call().map_err(...)? ` then
`read_success_body(response, "...")`; `push()`/`delete()` call `.send(...)/.call()`
then `expect_2xx(response, "...")`. Both `read_success_body` and `expect_2xx`
contain a non-2xx branch that reads the body and `bail!`s with status + body — but
that branch is unreachable while ureq errors on non-2xx first.

The crate uses `ureq` 3.x (`ureq = "3.3.0"` in `crates/rustlet-cli/Cargo.toml`),
whose config builder exposes `http_status_as_error(bool)` (default `true`).

## Commands you will need

| Purpose       | Command                                                  | Expected on success |
|---------------|----------------------------------------------------------|---------------------|
| Build         | `cargo build -p rustlet-cli`                             | exit 0              |
| Tests (file)  | `cargo test -p rustlet-cli --lib api::`                  | all pass incl `non_2xx_response_is_error_with_status_and_body` |
| Clippy        | `rustup run stable cargo clippy --workspace --all-targets` | exit 0            |
| Format        | `rustup run stable cargo fmt` then `--check`             | exit 0              |

(CI uses **stable**; this machine defaults to nightly — use `rustup run stable`.)

## Scope

**In scope**: `crates/rustlet-cli/src/api.rs` (the `Client::new` agent builder
only), `plans/README.md`. **Out of scope**: the request methods, `read_success_body`
/ `expect_2xx` bodies (they are correct — this just lets them run), the tests
(plan 005's test should pass unchanged).

## Git workflow

- Branch: commit on your worktree branch. Message: `fix(cli): include response body in device api client errors`. Do NOT push or open a PR.

## Steps

### Step 1: Disable ureq's status-as-error so the manual body-reading runs

In `Client::new`, add `.http_status_as_error(false)` to the config builder chain:

```rust
let agent: ureq::Agent = ureq::Agent::config_builder()
    .timeout_global(Some(REQUEST_TIMEOUT))
    .http_status_as_error(false)
    .user_agent(crate::util::user_agent())
    .build()
    .into();
```

**Verify**: `cargo build -p rustlet-cli` → exit 0. (If `http_status_as_error` is
not a method on this ureq version's config builder, STOP and report — see STOP
conditions.)

### Step 2: Verify the device-client tests, including the previously-failing one

**Verify**: `cargo test -p rustlet-cli --lib api::` → all pass, including
`non_2xx_response_is_error_with_status_and_body` (the error now contains both
`500` and `boom`).

### Step 3: Full gates

- `rustup run stable cargo fmt --check` → exit 0
- `rustup run stable cargo clippy --workspace --all-targets` → exit 0

## Test plan

No new tests. Plan 005's `non_2xx_response_is_error_with_status_and_body` is the
regression test — it must pass after this change. The other 4 device-client tests
(happy paths) must still pass.

## Done criteria (ALL must hold)

- [ ] `cargo build -p rustlet-cli` exits 0
- [ ] `cargo test -p rustlet-cli --lib api::` passes (all 10 `api::tests`)
- [ ] `rustup run stable cargo clippy --workspace --all-targets` exits 0
- [ ] `rustup run stable cargo fmt --check` exits 0
- [ ] `git grep -n "http_status_as_error" -- crates/rustlet-cli/src/api.rs` shows the new line
- [ ] Only `api.rs` changed (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

- `http_status_as_error` is not a valid method on this ureq version's config
  builder (compile error) — report it; an alternative is to read the body from the
  ureq `StatusCode` error variant instead, but do not improvise that without
  reporting first.
- Disabling status-as-error breaks an existing happy-path test (it should not —
  the manual `expect_2xx`/`read_success_body` already handle both 2xx and non-2xx).
- The "Current state" excerpt doesn't match live code (drift).

## Maintenance notes

- With `http_status_as_error(false)`, ALL request methods now rely on their manual
  `expect_2xx` / `read_success_body` status checks (which already exist) — a
  reviewer should confirm every method does check status, so a non-2xx is never
  silently treated as success.
- This is the bug surfaced by plan 005's characterization test; keep that test as
  the guard.

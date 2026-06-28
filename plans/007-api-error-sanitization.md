# Plan 007: Stop the `api` render server from leaking internal error detail to clients

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving on. If any
> STOP condition occurs, stop and report — do not improvise. When done, update
> the status row for this plan in `plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat c6e025f..HEAD -- crates/rustlet-cli/src/commands/api.rs`
> Main has advanced past `c6e025f` (a workspace `cargo fmt` reformatted files), so
> line numbers may have shifted. Match on the code *content* of the excerpts
> below, not line numbers; on a content mismatch, treat it as a STOP condition.

## Status

- **Priority**: P3
- **Effort**: S
- **Risk**: LOW
- **Depends on**: 001 (merged)
- **Category**: security (information disclosure)
- **Planned at**: commit `c6e025f`, 2026-06-28 (verify against current HEAD)

## Why this matters

The `api` subcommand is the machine-facing render server ("Run an HTTP render
server for other tools"). On a render failure it returns the full internal error
chain to the client (`format!("render failed: {e:#}")`), including filesystem
paths, the Starlark `load()` targets the applet tried, upstream URLs it fetched,
and panic messages — reconnaissance-grade detail, and the server can be bound
beyond localhost (`-i/--host`). The fix logs the full detail server-side (the
file already uses `eprintln!`) and returns a short generic message to the client.
Success and client-input validation responses are unchanged.

Scope note: the `serve` dev server (`commands/serve/handlers.rs`) is intentionally
NOT in scope — it is a localhost-default live-preview tool where verbose browser
errors are a feature. This plan targets only the machine-facing `api` server.

## Current state

`crates/rustlet-cli/src/commands/api.rs`, `async fn handle_render`. The leaking
arms (content, not line numbers):

```rust
// path rejection (BAD_REQUEST)
return (
    StatusCode::BAD_REQUEST,
    format!("path rejected: {e:#}"),
)
    .into_response()
// ...
Ok(Ok(Err(e))) => (
    StatusCode::INTERNAL_SERVER_ERROR,
    format!("render failed: {e:#}"),
)
    .into_response(),
Ok(Err(join_err)) => (
    StatusCode::INTERNAL_SERVER_ERROR,
    format!("render task panicked: {join_err}"),
)
    .into_response(),
Err(_elapsed) => (
    StatusCode::GATEWAY_TIMEOUT,
    format!("render exceeded {}s", RENDER_TIMEOUT.as_secs()),
)
    .into_response(),
```

- The filter-parse arm `(StatusCode::BAD_REQUEST, format!("{e}"))` reports invalid
  client input (an unknown filter name) and is fine to keep.
- The timeout arm is already generic — keep it.
- The file imports `StatusCode`/`IntoResponse` and uses `eprintln!` elsewhere, so
  server-side logging needs no new import.

Repo conventions: `eprintln!` for server diagnostics; conventional-commit messages
scoped by package.

## Commands you will need

| Purpose       | Command                                          | Expected on success |
|---------------|--------------------------------------------------|---------------------|
| Build         | `cargo build -p rustlet-cli`                     | exit 0              |
| Tests (all)   | `cargo test --workspace`                         | all pass (compat may fail if Go pixlet binary absent — environmental) |
| Clippy        | `rustup run stable cargo clippy --workspace --all-targets` | exit 0     |
| Format        | `rustup run stable cargo fmt` then `--check`     | exit 0              |

(CI uses **stable**; this machine defaults to nightly — use `rustup run stable`.)

## Scope

**In scope**: `crates/rustlet-cli/src/commands/api.rs`, `plans/README.md`.

**Out of scope**: `crates/rustlet-cli/src/commands/serve/handlers.rs`
(`render_error`) — the dev server stays verbose by design; the success path; the
filter-parse `BAD_REQUEST` arm; the timeout arm; the `sandbox_path` logic.

## Git workflow

- Branch: commit on your worktree branch. Message: `fix(cli): sanitize api server error responses, log detail`. Do NOT push or open a PR.

## Steps

### Step 1: Sanitize the three leaking arms; log full detail server-side

- Path rejection arm:
  ```rust
  Err(e) => {
      eprintln!("api path rejected: {e:#}");
      return (StatusCode::BAD_REQUEST, "path rejected").into_response();
  }
  ```
- Render-failed arm:
  ```rust
  Ok(Ok(Err(e))) => {
      eprintln!("api render failed: {e:#}");
      (StatusCode::INTERNAL_SERVER_ERROR, "render failed").into_response()
  }
  ```
- Render-panicked arm:
  ```rust
  Ok(Err(join_err)) => {
      eprintln!("api render task panicked: {join_err}");
      (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
  }
  ```

Leave the filter-parse `BAD_REQUEST` arm and the `Err(_elapsed)` timeout arm
exactly as they are.

**Verify**: `cargo build -p rustlet-cli` → exit 0.

### Step 2: Full verification

- `rustup run stable cargo clippy --workspace --all-targets` → exit 0
- `rustup run stable cargo fmt --check` → exit 0
- `cargo test --workspace` → all pass (compat env-failure excepted)

### Step 3 (optional manual check)

```
cargo run -p rustlet-cli -- api -i 127.0.0.1 -p 8099 &
curl -s -XPOST localhost:8099/api/render -H 'content-type: application/json' \
  -d '{"path":"does-not-exist.star","config":{}}'
# expect a short body ("render failed" / "path rejected"), not a path/stack chain
```

## Test plan

This is response-shaping in async axum handlers; the authoritative verification is
the grep-based done criteria plus the optional curl. The whole suite must still
pass; do not weaken existing `api` tests (e.g. `sandbox_path`).

## Done criteria (ALL must hold)

- [ ] `cargo build` exits 0
- [ ] `cargo test --workspace` exits 0 (compat env-failure excepted)
- [ ] `rustup run stable cargo clippy --workspace --all-targets` exits 0
- [ ] `git grep -n "render failed: {e:#}" -- crates/rustlet-cli/src/commands/api.rs` returns nothing
- [ ] `git grep -n "path rejected: {e:#}" -- crates/rustlet-cli/src/commands/api.rs` returns nothing
- [ ] `git grep -n "render task panicked: {join_err}" -- crates/rustlet-cli/src/commands/api.rs` returns nothing
- [ ] `git grep -n "eprintln!(\"api render failed" -- crates/rustlet-cli/src/commands/api.rs` shows the server-side log
- [ ] No files outside the in-scope list modified
- [ ] `plans/README.md` status row updated

## STOP conditions

- The match arms in `handle_render` differ structurally from "Current state" (drift).
- Returning a `&'static str` body from an arm causes a type error (the tuple form
  yields `Response`; if not, report the exact error).

## Maintenance notes

- Keep the asymmetry deliberate: `serve` (dev, localhost) verbose; `api`
  (machine-facing) generic. If `serve` is later hardened for non-localhost use,
  revisit `render_error` there.
- If `tracing` is later adopted, replace these `eprintln!` calls and consider a
  correlation id returned to the client.

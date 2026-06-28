# Plan 005: Add HTTP-level test coverage for the device API client

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving on. If any
> STOP condition occurs, stop and report — do not improvise. When done, update
> the status row for this plan in `plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat c6e025f..HEAD -- crates/rustlet-cli/src/api.rs`
> Main has advanced past `c6e025f` (a workspace `cargo fmt` reformatted files), so
> line numbers may have shifted. Match on the code *content* of the excerpts
> below, not line numbers; on a content mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED (network-style tests; mitigated by ephemeral ports, read timeouts, one connection per test)
- **Depends on**: 001 (merged)
- **Category**: tests
- **Planned at**: commit `c6e025f`, 2026-06-28 (verify against current HEAD)

## Why this matters

`crates/rustlet-cli/src/api.rs` is the synchronous HTTP client behind every device
command (`push`, `devices`, `list`, `delete`). It has 5 unit tests, but all only
cover JSON (de)serialization and argument validation — **none exercise the actual
request/response path**: the URL it builds, the `Authorization` header it sends,
the JSON envelope it parses, or its non-2xx error handling. A regression in any of
those ships silently. Because `Client::new` takes the base URL as a parameter, the
request path is trivially testable against a local mock server.

## Current state

`crates/rustlet-cli/src/api.rs` defines:

- `pub struct Client { agent, base_url, token }` with `Client::new(base_url: &str, token: &str) -> Result<Self>`.
- `fn url(&self, path)` → `"{base_url}/v0/{path}"` (base url trimmed of trailing `/`).
- `fn auth_header(&self)` → `"Bearer {token}"`.
- `pub fn devices(&self) -> Result<Vec<Device>>` — `GET v0/devices`, parses `{"devices":[{ "id", "displayName" }]}`.
- `pub fn installations(&self, device_id) -> Result<Vec<Installation>>` — `GET v0/devices/{id}/installations`, parses `{"installations":[{ "id", "appID" }]}`.
- `pub fn push(&self, device_id, image: &[u8], installation_id: Option<&str>, background: bool) -> Result<()>` — `POST v0/devices/{id}/push`, JSON body `deviceID`, `image` (base64), `installationID`, `background`; sends `Authorization` + `Content-Type: application/json`.
- `pub fn delete(&self, device_id, installation_id) -> Result<()>` — `DELETE v0/devices/{id}/installations/{iid}`.
- Non-2xx responses → `Err` via `expect_2xx`/`read_success_body`, message `"{op} failed: HTTP {status}: {body}"`.
- `use base64::Engine;` is imported at the top.
- An existing `#[cfg(test)] mod tests { use super::*; ... }` holds the current 5 tests.

Repo conventions: tests are inline `#[cfg(test)] mod tests`; `applet.rs` already
uses a raw `std::net::TcpListener` thread as a mock HTTP server — follow that
approach (no new dependency). Conventional-commit messages scoped by package.

## Commands you will need

| Purpose       | Command                                          | Expected on success |
|---------------|--------------------------------------------------|---------------------|
| Build         | `cargo build -p rustlet-cli`                     | exit 0              |
| Tests (file)  | `cargo test -p rustlet-cli --lib api::`          | new tests pass      |
| Tests (all)   | `cargo test --workspace`                         | all pass (compat may fail if Go pixlet binary absent — environmental) |
| Clippy        | `rustup run stable cargo clippy --workspace --all-targets` | exit 0     |
| Format        | `rustup run stable cargo fmt` then `--check`     | exit 0              |

(CI uses **stable**; this machine defaults to nightly — use `rustup run stable`.)

## Scope

**In scope**: `crates/rustlet-cli/src/api.rs` (extend the existing test module
only), `plans/README.md`.

**Out of scope**: any non-test code in `api.rs` (if a test fails because the client
has a real bug, STOP and report it — do not fix production code here); the thin
command wrappers (`commands/{push,devices,list,delete}.rs`); adding any new crate
dependency (the mock server uses only `std`).

## Git workflow

- Branch: commit on your worktree branch. Message: `test(cli): cover device api client request/response paths`. Do NOT push or open a PR.

## Steps

### Step 1: Add the test imports and a minimal mock HTTP server

Inside the existing `mod tests { ... }` block in `api.rs`, just after
`use super::*;`, add:

```rust
use base64::Engine as _;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Read one HTTP request (headers + body, using Content-Length) from `stream`.
fn read_http_request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set read timeout");
    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 1024];
    loop {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                let text = String::from_utf8_lossy(&buf);
                if let Some(idx) = text.find("\r\n\r\n") {
                    let header_len = idx + 4;
                    let content_length = text
                        .lines()
                        .find_map(|line| {
                            let lower = line.to_ascii_lowercase();
                            lower
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                        })
                        .unwrap_or(0);
                    if buf.len() >= header_len + content_length {
                        break;
                    }
                }
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buf).to_string()
}

/// Build a canned HTTP response with a Content-Length and an explicit close.
fn http_response(status_line: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// Spawn a one-shot mock server. Returns its base URL, the recorded raw request,
/// and the server thread's join handle.
fn spawn_mock(response: String) -> (String, Arc<Mutex<Vec<String>>>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let req_clone = Arc::clone(&requests);
    let handle = thread::spawn(move || {
        if let Some(Ok(mut stream)) = listener.incoming().next() {
            let req = read_http_request(&mut stream);
            req_clone.lock().expect("lock").push(req);
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    (format!("http://{addr}"), requests, handle)
}
```

**Verify**: `cargo build -p rustlet-cli --tests` → exit 0.

### Step 2: Add the request/response tests

Append inside the same `mod tests` block:

```rust
#[test]
fn devices_parses_envelope_and_sends_auth() {
    let resp = http_response("200 OK", r#"{"devices":[{"id":"dev1","displayName":"Kitchen"}]}"#);
    let (base, requests, handle) = spawn_mock(resp);
    let client = Client::new(&base, "tok").unwrap();
    let devices = client.devices().unwrap();
    handle.join().unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].id, "dev1");
    assert_eq!(devices[0].display_name, "Kitchen");
    let req = requests.lock().unwrap()[0].clone();
    assert!(req.starts_with("GET /v0/devices "), "req: {req}");
    assert!(req.to_lowercase().contains("authorization: bearer tok"), "req: {req}");
}

#[test]
fn installations_parses_envelope() {
    let resp = http_response("200 OK", r#"{"installations":[{"id":"app1","appID":"weather"}]}"#);
    let (base, requests, handle) = spawn_mock(resp);
    let client = Client::new(&base, "tok").unwrap();
    let installs = client.installations("dev1").unwrap();
    handle.join().unwrap();
    assert_eq!(installs.len(), 1);
    assert_eq!(installs[0].id, "app1");
    assert_eq!(installs[0].app_id, "weather");
    let req = requests.lock().unwrap()[0].clone();
    assert!(req.starts_with("GET /v0/devices/dev1/installations "), "req: {req}");
}

#[test]
fn push_sends_payload_path_and_auth() {
    let (base, requests, handle) = spawn_mock(http_response("200 OK", "{}"));
    let client = Client::new(&base, "tok").unwrap();
    client.push("dev1", b"imgbytes", Some("inst1"), false).unwrap();
    handle.join().unwrap();
    let req = requests.lock().unwrap()[0].clone();
    assert!(req.starts_with("POST /v0/devices/dev1/push "), "req: {req}");
    assert!(req.to_lowercase().contains("authorization: bearer tok"), "req: {req}");
    assert!(req.contains("\"deviceID\":\"dev1\""), "req: {req}");
    let b64 = base64::engine::general_purpose::STANDARD.encode(b"imgbytes");
    assert!(req.contains(&b64), "req: {req}");
}

#[test]
fn delete_hits_installation_path() {
    let (base, requests, handle) = spawn_mock(http_response("200 OK", "{}"));
    let client = Client::new(&base, "tok").unwrap();
    client.delete("dev1", "inst1").unwrap();
    handle.join().unwrap();
    let req = requests.lock().unwrap()[0].clone();
    assert!(req.starts_with("DELETE /v0/devices/dev1/installations/inst1 "), "req: {req}");
}

#[test]
fn non_2xx_response_is_error_with_status_and_body() {
    let (base, _requests, handle) = spawn_mock(http_response("500 Internal Server Error", "boom"));
    let client = Client::new(&base, "tok").unwrap();
    let err = client.devices().unwrap_err();
    handle.join().unwrap();
    let msg = err.to_string();
    assert!(msg.contains("500"), "msg: {msg}");
    assert!(msg.contains("boom"), "msg: {msg}");
}
```

**Verify**: `cargo test -p rustlet-cli --lib api::` → all `api::tests` pass (5 existing + 5 new).

### Step 3: Full verification

- `rustup run stable cargo clippy --workspace --all-targets` → exit 0
- `rustup run stable cargo fmt --check` → exit 0
- `cargo test --workspace` → all pass (compat env-failure excepted)

## Test plan

5 new tests covering `devices` (envelope + auth + path), `installations`
(envelope + path), `push` (path + auth + base64 payload), `delete` (path), and the
non-2xx error path. Pattern source: the `std::net::TcpListener` mock in
`applet.rs`. All use ephemeral ports and a 5s read timeout.

## Done criteria (ALL must hold)

- [ ] `cargo test -p rustlet-cli --lib api::` passes with 10 tests in `api::tests`
- [ ] `cargo test --workspace` exits 0 (compat env-failure excepted)
- [ ] `rustup run stable cargo clippy --workspace --all-targets` exits 0
- [ ] `rustup run stable cargo fmt --check` exits 0
- [ ] No non-test code in `api.rs` changed (`git diff` shows additions only inside `mod tests`)
- [ ] `plans/README.md` status row updated

## STOP conditions

- A new test fails because the client produces a wrong URL/header/parse — that is a
  real bug; report it, do not edit production code here.
- The `Client` method signatures differ from "Current state" (drift).
- Tests hang or are flaky across runs — report it; do not paper over with sleeps.

## Maintenance notes

- The mock-server helpers are local to this test module.
- Follow-up (deferred): wrapper-level tests for `commands/{push,devices,list,delete}.rs`
  (argument parsing, config resolution, exit codes).
- A reviewer should confirm tests assert on wire behavior (path/header/body), not
  just that the call returned `Ok`.

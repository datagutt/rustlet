# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Rustlet

Rustlet is a Rust reimplementation of [Pixlet](https://github.com/tidbyt/pixlet), a framework for building apps for pixel-based displays like Tidbyt. It uses Starlark (a Python dialect) to define applets that render widget trees to small pixel grids (default 64x32), then encodes output as animated GIF or WebP.

## Commands

```bash
# build
cargo build
cargo build --release

# test (183 tests across all crates)
cargo test
cargo test -p rustlet-render
cargo test -p rustlet-runtime
cargo test -p rustlet-encode
cargo test -p rustlet-cli

# run a single test
cargo test -p rustlet-render test_name

# lint
cargo clippy --workspace

# run
cargo run -p rustlet-cli -- render examples/hello_world.star -o output.gif
cargo run -p rustlet-cli -- render examples/hello_world.star -o output.webp

# update snapshot tests (rustlet-render uses insta)
cargo insta review
```

## Architecture

Four crates in a Cargo workspace:

```
rustlet-cli  ->  rustlet-runtime  ->  rustlet-render
             ->  rustlet-render
             ->  rustlet-encode
```

**rustlet-render**: Pure rendering engine. Defines the `Widget` trait and 18+ widget implementations (Text, Box, Row, Column, Stack, Padding, Marquee, Animation, etc.), the `Root` container, BDF font parsing, and layout primitives. Row/Column share a `Vector` layout engine with MainAlign/CrossAlign. 27 BDF bitmap fonts are embedded at compile time via `include_str!()`.

**rustlet-runtime**: Starlark scripting runtime. The `Applet` struct is the main entry point: it parses `.star` files, provides standard library modules (render, time, math, http, encoding, humanize, schema, color, random), evaluates `main(config)`, and extracts `Root` widget trees. Each Starlark module is defined as a frozen Starlark module with native Rust functions registered via `#[starlark::starlark_module]`. Widgets cross the boundary as `StarlarkWidget` wrappers around `Arc<dyn Widget>`.

**rustlet-encode**: Encodes `Vec<Pixmap>` frames into animated GIF (256-color quantized) or WebP (lossless). Also provides 14 color filters (applied as 3x3 color matrices) and integer magnification.

**rustlet-cli**: Clap-based CLI binary with a single `render` subcommand.

### Rendering pipeline

1. CLI reads `.star` file and parses args
2. `Applet::run_with_options` evaluates Starlark, calls `main(config)`, extracts `Root`
3. `Root::paint_frames(width, height)` recursively paints widget tree into `Pixmap` frames (up to 2000 max)
4. `rustlet_encode::encode` applies optional filter/magnify, encodes to GIF or WebP

### Starlark integration pattern

Widget constructors in `render_module.rs` are native Starlark functions. Each creates a Rust widget struct from `rustlet-render`, wraps it in `StarlarkWidget` (via `Arc<dyn Widget>`), and allocates on the Starlark heap. Custom Starlark value types (`StarlarkColor`, `StarlarkTime`, `StarlarkResponse`, `StarlarkFile`, `StarlarkConfig`, `StarlarkCanvas`) use `#[derive(StarlarkValue)]` with the `allocative`, `ProvidesStaticType`, and `NoSerialize` traits.

### Key design decisions

- Time handling is hand-rolled (no chrono). Timezone support uses a static offset table (~60 IANA zones, no DST).
- HTTP caching is in-memory (`LazyLock<Mutex<HashMap>>`) with TTL-based expiry, evicted at 256 entries.
- Emoji rendering uses resvg with Twemoji SVGs (configurable directory), falling back to a colored placeholder.
- Fonts are BDF bitmap fonts rendered pixel-by-pixel to Pixmap. No anti-aliasing (appropriate for pixel displays).
- The `.reference/` directory (gitignored) contains original Go pixlet source and starlark-rust crate source used during development.

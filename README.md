# Rustlet

Build apps for pixel-based displays, but for hipsters.

Rustlet is a Rust reimplementation of [Pixlet](https://github.com/tronbyt/pixlet)
(the Tronbyt community fork), an app runtime and UX toolkit for highly
constrained displays like 64x32 RGB LED matrices. Applets are written in
[Starlark](https://github.com/bazelbuild/starlark) (a Python dialect), and the
Starlark API is compatible with upstream pixlet, so most existing `.star` apps
run unmodified.

## Getting started

### Build from source

Rustlet is a Cargo workspace. To build the CLI:

```bash
# clone and build a release binary
git clone https://github.com/datagutt/rustlet.git
cd rustlet
cargo build --release

# the binary lands in target/release/rustlet-cli
./target/release/rustlet-cli --help
```

Or install it to your Cargo bin directory:

```bash
cargo install --path crates/rustlet-cli
rustlet-cli --help
```

### Hello, World

Rustlet applets are written in Starlark, the same Python-like language used by
pixlet. Here is the canonical hello world program:

```starlark
load("render.star", "render")

def main(config):
    return render.Root(
        child = render.Box(
            color = "#000033",
            child = render.Row(
                expanded = True,
                main_align = "center",
                cross_align = "center",
                children = [
                    render.Text("Hello!"),
                ],
            ),
        ),
    )
```

Render it to an animated GIF or WebP:

```bash
rustlet-cli render examples/hello_world.star -o hello.gif
rustlet-cli render examples/hello_world.star -o hello.webp
```

## How it works

Rustlet evaluates a `.star` file, calls its `main(config)` function, and walks
the returned widget tree to paint frames into pixmaps. Frames are then encoded
as an animated GIF (256 color quantized) or a lossless WebP.

The rendering engine and the Starlark runtime are split into four crates:

| Crate | Responsibility |
|---|---|
| `rustlet-render` | Widget trait and 18+ widget implementations (Text, Box, Row, Column, Stack, Padding, Marquee, Animation, ...), BDF font parsing, layout primitives. 27 BDF fonts are embedded at compile time. |
| `rustlet-runtime` | Starlark scripting runtime. Parses `.star` files, provides standard library modules (render, time, math, http, encoding, humanize, schema, color, random), evaluates `main(config)`, and extracts the widget tree. |
| `rustlet-encode` | Encodes pixmap frames to animated GIF or WebP, applies color filters (14 built in), and handles integer magnification. |
| `rustlet-cli` | Clap based CLI binary exposing `render`, `lint`, `format`, `schema`, and `version` subcommands. |

### Example: a clock

This applet accepts a `timezone` parameter and produces a two frame animation
displaying the current time with a blinking separator between hours and minutes.

```starlark
load("render.star", "render")
load("time.star", "time")

def main(config):
    timezone = config.get("timezone") or "America/New_York"
    now = time.now().in_location(timezone)

    return render.Root(
        delay = 500,
        child = render.Box(
            child = render.Animation(
                children = [
                    render.Text(
                        content = now.format("3:04 PM"),
                        font = "6x13",
                    ),
                    render.Text(
                        content = now.format("3 04 PM"),
                        font = "6x13",
                    ),
                ],
            ),
        ),
    )
```

Render it:

```bash
rustlet-cli render clock.star --format webp -o clock.webp
```

## CLI

```
rustlet-cli <COMMAND>

Commands:
  render   Render a .star file to an image
  lint     Lint a .star file or app directory (parses, sandbox evaluates, validates manifest.yaml)
  format   Format .star files (requires `buildifier` on $PATH, same tool pixlet uses)
  schema   Print the configuration schema for a Rustlet app
  version  Show the version of Rustlet
```

### `render` options

```
--width <WIDTH>            Display width in pixels [default: 64]
--height <HEIGHT>          Display height in pixels [default: 32]
--format <FORMAT>          Output format [possible values: gif, webp]
--filter <FILTER>          Color filter applied before encoding (none, dimmed,
                           red-shift, warm, sunset, sepia, vintage, dusk, cool,
                           bw, ice, moonlight, neon, pastel)
--magnify <MAGNIFY>        Integer magnification factor [default: 1]
--2x                       Double the canvas size (128x64), use terminus-16 as
                           the default font. Auto-enabled when the manifest
                           declares `supports2x: true` and the applet is loaded
                           from a directory.
--twemoji-dir <PATH>       Directory containing Twemoji SVG files, named by
                           codepoint (e.g. `1f600.svg`)
```

## Features

* Pure Rust rendering pipeline (tiny-skia based), no CGo, no external draw libs.
* Starlark API compatible with pixlet (most `.star` applets run unmodified).
* Standard library modules: `render`, `time`, `math`, `http`, `encoding`,
  `humanize`, `schema`, `color`, `random`, `sunrise`.
* Animated GIF and lossless WebP output.
* 27 embedded BDF bitmap fonts (tom-thumb, 5x8, 6x13, CG pixel variants, etc.).
* 14 color filters and integer magnification applied at encode time.
* Optional Twemoji rendering via resvg, configurable via `--twemoji-dir`.
* Built in `lint` and `format` subcommands, including `manifest.yaml` validation.
* 2x mode (128x64) for higher resolution displays.

## Status

Rustlet is a work in progress reimplementation, focused on matching pixlet's
rendering output. The core `render` pipeline, the Starlark standard library, and
the `lint` / `format` / `schema` subcommands are implemented. Features that
talk to device or server infrastructure (`pixlet serve`, `pixlet push`,
`pixlet login`, `pixlet devices`) are not implemented.

## Testing

```bash
cargo test                        # run the full test suite
cargo test -p rustlet-render      # just the renderer
cargo insta review                # review snapshot diffs (rustlet-render uses insta)
```

## Credits

Rustlet is a port of the [Tronbyt pixlet fork](https://github.com/tronbyt/pixlet),
itself a community fork of the original [Tidbyt pixlet](https://github.com/tidbyt/pixlet).
The Starlark API, widget semantics, and reference pixel output all come from
upstream pixlet. The `.reference/` directory (gitignored) keeps the original Go
pixlet source alongside `starlark-rust` for cross referencing during development.

## License

See [LICENSE](LICENSE).

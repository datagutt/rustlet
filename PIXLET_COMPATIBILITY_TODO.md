# Pixlet Compatibility Audit

This file tracks the work required to make `rustlet` behave like the reference implementation in `.reference/pixlet`.

Scope for this audit:

- Compare the current Rust runtime/render/CLI surface against the reference Pixlet source.
- Separate hard incompatibilities from lower-priority tooling gaps.
- Break the work into phases that can be landed as independent commits.

## Current Findings

### Critical runtime API gaps

- [x] Missing widget constructors in `render.star`: `Arc`, `Line`, `PieChart`, `Plot`, `Polygon`.
- [x] Missing `render.fonts` map.
- [x] Missing Pixlet-style widget attributes and methods on Starlark widget values:
  `size()`, `frame_count()`, child fields, layout fields, `Image.delay`, `Image.hold_frames`, etc.
- [x] `canvas` API is incomplete:
  `size()` is missing, `width(raw?)` and `height(raw?)` do not support `raw`, and current code relies on non-Pixlet `CANVAS_WIDTH` / `CANVAS_HEIGHT` constants.
- [x] Asset loading is incompatible with Pixlet:
  `load("foo.png", foo = "file")` should return a file object with `path` and `readall(mode?)`, not a base64-encoded string wrapper.

### Critical binary/data incompatibilities

- [x] `encoding/base64.star` is incompatible with Pixlet byte handling:
  `decode()` currently forces UTF-8 and cannot return binary data.
- [x] `render.Image(src=...)` is incompatible:
  the runtime currently base64-decodes `src` again instead of consuming Pixlet-style raw bytes / raw SVG text.
- [x] Asset `readall("rb")` behavior is missing, which blocks binary image workflows used by reference Pixlet apps.
- [x] SVG input compatibility is incomplete because `render.Image` assumes a base64 payload instead of trying SVG/text/image decoders in Pixlet order.

### Runtime semantics gaps

- [x] `random.star` is incomplete: missing `seed()`, `float()`, `secure=True` handling, and Pixlet's deterministic thread-scoped seeding behavior.
- [x] `color.star` is incomplete: missing writable `h`, `s`, `v` fields and missing `hsv()` / `hsva()` methods on `Color`.
- [ ] `time.star` is only partially compatible; Pixlet supports richer parsing/format/location behavior than the current implementation.
- [ ] `http.star` behavior diverges from Pixlet: request argument surface and caching/header semantics do not match the reference implementation.
- [ ] `schema.star` is currently a lightweight struct factory, not a compatibility-complete implementation.

### Rendering behavior risks

- [ ] Default font selection does not match Pixlet at 2x:
  Pixlet defaults `render.Text` to `terminus-16` on 2x canvases.
- [ ] `WrappedText` runtime constructor does not expose Pixlet's `wordbreak` parameter.
- [ ] Text rendering is not Pixlet-compatible for bidi/emoji segmentation and likely differs in measured size and layout.
- [ ] Wrapped text uses a simple char-width heuristic instead of Pixlet's actual text measurement flow, which can change line breaks.
- [ ] Animated GIF image handling should be verified against Pixlet disposal/delay behavior.
- [ ] Rendering parity is not being checked against reference Pixlet tests/snapshots yet.

### CLI / tooling gaps

- [ ] CLI surface is far smaller than Pixlet: only `render` exists today.
- [ ] Missing core Pixlet commands and workflows:
  `lint`, `format`, `schema`, `serve`, `version`, and manifest-aware validation flows.
- [ ] 2x CLI behavior is only partially compatible with Pixlet.

## Phased Plan

### Phase 0: Audit and tracking

- [x] Compare current Rust crates with `.reference/pixlet`.
- [x] Write the compatibility TODO into the repository.
- [x] Keep this file updated as each phase lands.

Suggested commit:

- `docs: add Pixlet compatibility audit and phased todo`

### Phase 1: Runtime boundary parity

Goal: make Starlark-side APIs look like Pixlet before changing deeper render internals.

- [x] Add missing widget constructors to `render.star`.
- [x] Add `render.fonts`.
- [x] Expose widget attrs/methods needed by Pixlet apps and Pixlet tests.
- [x] Make `canvas` support `width(raw?)`, `height(raw?)`, `size(raw?)`, `is2x()`.
- [x] Replace asset loading with a Pixlet-compatible file object.
- [x] Add runtime tests modeled after `.reference/pixlet/runtime/render_test.go` and asset loader tests from `.reference/pixlet/runtime/applet_test.go`.

Suggested commit:

- `runtime: align render module surface with pixlet`

### Phase 2: Binary and module semantics parity

Goal: make the runtime behave like Pixlet for data flow and module contracts.

- [x] Rework base64 support to preserve binary data.
- [x] Make `render.Image` accept Pixlet-style raw bytes / SVG text and expose `delay` / `hold_frames`.
- [x] Fix `random.star` behavior and API.
- [x] Close high-impact gaps in `color.star`.
- [x] Add compatibility tests for binary image loading, random determinism, color mutation, and SVG image loading.

Suggested commit:

- `runtime: fix pixlet binary and module semantics`

### Phase 2b: Time and HTTP semantics parity

Goal: close the remaining runtime-behavior gaps that are larger than the binary/module slice above.

- [ ] Align `time.star` with Pixlet duration and location semantics.
- [ ] Align `http.star` request arguments, response shape, and caching/header semantics with Pixlet.
- [ ] Add compatibility tests for HTTP response shape and time arithmetic/location behavior.

Suggested commit:

- `runtime: align pixlet time and http semantics`

### Phase 3: Render correctness parity

Goal: reduce output differences for real-world Pixlet applets.

- [ ] Match Pixlet 2x default font behavior.
- [ ] Add `WrappedText.wordbreak`.
- [ ] Review text layout, bidi shaping, emoji handling, and measurement against reference Pixlet behavior.
- [ ] Verify GIF composition/disposal and animation timing against Pixlet.
- [ ] Port or recreate representative reference render tests for widgets and layouts.

Suggested commit:

- `render: improve pixlet compatibility for text image and layout`

### Phase 4: CLI and manifest parity

Goal: close the gap between `rustlet` and the `pixlet` developer workflow.

- [ ] Add missing CLI subcommands in priority order: `version`, `lint`, `schema`, `format`, `serve`.
- [ ] Implement manifest-aware validation paths.
- [ ] Align 2x CLI defaults and output naming with Pixlet.

Suggested commit:

- `cli: add pixlet-compatible developer workflows`

## Recommended execution order

1. Land Phase 1 before touching deeper render details.
2. Land Phase 2 before trying to validate real-world app compatibility.
3. Land Phase 3 with compatibility tests, not ad hoc visual fixes.
4. Land Phase 4 after runtime/render parity is stable.

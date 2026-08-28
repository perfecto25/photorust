# Architecture

How PhotoRust is put together, and the conventions that cause real bugs if
missed. For the rules Claude and contributors must follow when changing it,
see [CLAUDE.md](../CLAUDE.md). For the GPU work specifically, see
[gpu-migration.md](gpu-migration.md).

---

## The split: C++ shell, Rust core

A **C++/Qt QWidgets shell** over a **Rust image engine**, joined by
[CXX-Qt](https://github.com/KDAB/cxx-qt).

```
┌──────────────────────────────────────────────────────────┐
│  C++ / Qt shell (QWidgets)                                │
│  window, docks, panels, menus, options bar                │
│  canvas viewport, tool input, keyboard command registry   │
└───────────────────────────┬──────────────────────────────┘
                            │  CXX-Qt bridge (safe FFI)
┌───────────────────────────┴──────────────────────────────┐
│  Rust core (the engine)                                   │
│  pixel buffers, layers, compositing, blend modes          │
│  filters, selections, masks, history, .psd I/O            │
│  GPU compute via wgpu, with a CPU path as the reference   │
└──────────────────────────────────────────────────────────┘
```

The rule of thumb: **if it is a widget, it is C++; if it touches pixels, it is
Rust.**

QWidgets is the right tool for the UI because its dock widgets, menus and
toolbars map almost one-to-one onto Photoshop's interface. The Rust↔Qt
bindings do not idiomatically cover QWidgets — they are oriented around
QML/QtQuick — so the UI stays in C++. The engine is where pixel buffers,
threading and manual memory management live, which is exactly where Rust pays
off.

---

## Tech stack

| Layer          | Choice                                                      |
|----------------|-------------------------------------------------------------|
| UI toolkit     | Qt 6, QWidgets (C++)                                        |
| UI theming     | QSS stylesheets reproducing the CS6 dark "Kona" theme       |
| Image engine   | Rust (stable toolchain, `cargo`)                            |
| Parallelism    | `rayon` for CPU pixel work                                  |
| GPU            | `wgpu` compute — Vulkan on Linux, Metal on macOS            |
| FFI bridge     | CXX-Qt (`cxx-qt`, `cxx-qt-lib`, `cxx-qt-build`)             |
| Build system   | CMake driving Cargo via Corrosion                           |
| `.psd` support | Custom Rust parser                                          |

Targets **Linux and macOS**. Windows is out of scope, and the top-level
`CMakeLists.txt` fails the configure step there rather than half-working.

---

## Rendering backends

Pixel work goes through a `RenderBackend` seam in `core/src/gpu/`. Two
implementations exist: the CPU one (the existing `rayon` code, and the
reference every GPU path is checked against) and a `wgpu` one.

The GPU is **used when available and never required**. A machine with no
usable adapter runs every feature; it is only slower. Selection happens once at
startup and is reported in Help ▸ About.

```
PHOTORUST_BACKEND=cpu    force the CPU
PHOTORUST_BACKEND=gpu    refuse to fall back silently
WGPU_BACKEND=vulkan      pin a specific graphics API
```

Current state, measured on an AMD Radeon 780M:

| Operation      | Status                                                    |
|----------------|-----------------------------------------------------------|
| Gaussian blur  | On the GPU. 4–70× faster depending on size and radius.    |
| Compositing    | Shader complete and parity-tested, **not enabled** — it is slower until layer pixels stay resident on the GPU. |
| Everything else| CPU.                                                      |

Full detail, benchmarks and the phase plan: [gpu-migration.md](gpu-migration.md).

---

## Module layout

```
core/                 Rust image engine
  src/buffer.rs         RGBA8 pixel buffers, rectangles
  src/blend.rs          the 27 blend modes
  src/layer.rs          layer model and stack
  src/compositor.rs     stack → final image (parallel, rayon)
  src/gpu/              rendering backend seam: wgpu device, CPU fallback,
                        and the blur and compositing compute shaders
  src/brush.rs          dab-based stroke rendering
  src/healing.rs        inpainting, Poisson cloning and red-eye removal
  src/replace.rs        colour replacement for the Color Replacement Brush
  src/mixer.rs          wet-paint mixing for the Mixer Brush
  src/stamp.rs          source sampling for the Clone Stamp
  src/gradient.rs       colour ramps and the five gradient shapes
  src/bucket.rs         flood filling for the Paint Bucket
  src/focus.rs          the Blur and Sharpen tools
  src/smudge.rs         the Smudge tool's carried patch
  src/tone.rs           the Dodge, Burn and Sponge tools
  src/path.rs           vector paths for the Pen tool and Paths panel
  src/selection.rs      coverage-mask selections
  src/magnetic.rs       edge snapping for the Magnetic Lasso
  src/wand.rs           Magic Wand flood and Quick Selection region growing
  src/perspective.rs    homography warp for the Perspective Crop tool
  src/slice.rs          web-export slices and auto-slice generation
  src/annotation.rs     colour samplers, notes, count markers, ruler
  src/filters/          adjustments and convolutions
  src/history.rs        bounded linear undo
  src/document.rs       one open image; ties the above together
  src/psd/              .psd parsing and writing
  src/bridge.rs         the CXX-Qt QObject exposed to C++

shell/                C++ / Qt QWidgets application
  src/MainWindow.*      menus, docks, options bar
  src/dialogs/          Keyboard Shortcuts, Color Settings, Indexed Color, etc.
  src/canvas/           viewport, zoom/pan, channel masking, input → document coordinates
  src/panels/           Layers, Channels, Paths, Color, History (LayerIcons/PathIcons hold the artwork)
  src/tools/            tool strip and tool metadata
  src/shortcuts/        command registry and keymap loading
  resources/theme.qss       CS6 dark theme
  resources/shortcuts.json  CS6 default keymap
```

---

## Conventions worth knowing before editing

These are the ones that cause real bugs if missed:

- **The GPU is used when one is available, and never required.** Every
  accelerated operation has a CPU implementation that is the reference, and the
  engine falls back to it when there is no usable adapter, when the input is
  the wrong bit depth or too large for a storage binding, or when a device
  call fails mid-operation. A missing GPU costs speed, never features. The
  backend is chosen once at startup and reported in Help ▸ About; set
  `PHOTORUST_BACKEND=cpu` to force the CPU. See `docs/gpu-migration.md`.
- **Colour is straight (non-premultiplied) alpha** everywhere in the engine.
  Premultiplication happens once, when a buffer is handed to Qt.
- **Layer stacks are stored bottom-first** (index 0 is the Background). The
  Layers panel shows them top-first. That flip happens *only* in `bridge.rs`;
  everything downstream of it speaks panel indices, everything upstream speaks
  stack indices.
- **Shortcuts are data.** Never hard-code a key combo in a widget — register a
  command and bind it in `shortcuts.json` (CLAUDE.md §9).
- **Blur and convolution run on premultiplied colour.** Filtering straight
  alpha lets the colour of fully transparent pixels bleed into visible ones,
  which shows up as dark halos on soft edges.
- **Selection queries walk the whole mask.** `is_empty`, `bounds` and `outline`
  are all O(canvas). They memoise, but the answers still have to be hoisted out
  of per-pixel loops and off the per-dab repaint path. `canvasChanged` fires on
  every brush dab; only `selectionChanged` should trigger a re-trace.
- **`QImage`s crossing the bridge must own their pixels.** Wrapping a Rust
  allocation with `QImage::from_raw_bytes` and returning it does not work: the
  wrapper is a temporary whose destructor frees the Rust buffer, leaving the
  C++ side pointing at freed memory. `bridge.rs::pixmap_to_qimage` deep-copies
  for this reason, and the comment there explains what a zero-copy version
  would need.

User keymap overrides are written to
`~/.config/PhotoRust/shortcuts.json` (Linux) or the equivalent
`AppConfigLocation` on macOS; only bindings that differ from the defaults are
stored, so future default changes still reach the user.

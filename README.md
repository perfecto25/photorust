# PhotoRust

A from-scratch clone of Adobe Photoshop CS6 (the 2012–2013 "Kona" dark UI).

A **C++/Qt QWidgets shell** over a **Rust image engine**, joined by
[CXX-Qt](https://github.com/KDAB/cxx-qt).

- [docs/REFERENCE.md](docs/REFERENCE.md) — the interface in full: window layout,
  every tool and flyout, the panels, and the complete keymap for Linux and macOS.
- [CLAUDE.md](CLAUDE.md) — architecture, and the rules that keep the two halves
  separate.

Targets **Linux and macOS**. Windows is out of scope, and the top-level
`CMakeLists.txt` fails the configure step there rather than half-working.

---

## Status

This is an early but working milestone: the app builds, runs, and you can
paint, manage layers, select, filter and undo.

**Working**

- CS6-style dark UI: menu bar, options bar, left tool strip, dockable panels
  (Color/Swatches tabbed, History, Layers), status bar.
- Tool strip with SVG line-art icons, the corner triangle marking tools that
  have hidden tools, press-and-hold (or right-click) flyout menus listing each
  CS6 tool group, and the footer swatch / Quick Mask / screen-mode controls.
- Canvas viewport with zoom (CS6's zoom stops), cursor-anchored wheel zoom,
  pan (space-drag or middle-drag), and a transparency checkerboard.
- Layers: add, delete, duplicate, reorder, merge down, flatten, rename,
  show/hide, opacity, fill opacity, clipping masks, layer masks, thumbnails.
- All **27 Photoshop blend modes**, including the non-separable ones (Hue,
  Saturation, Color, Luminosity, Darker/Lighter Color).
- Brush engine: dab-based strokes with spacing, hardness falloff, flow and
  opacity; a whole stroke is one undo step.
- Selections: all four marquee variants (rectangular, elliptical, single row,
  single column) with antialiased edges, plus add/subtract/intersect, invert,
  and feather. `M` selects the marquee and `Shift+M` cycles rectangular ↔
  elliptical, as CS6 does. Marching ants trace the mask's real 50% contour, so
  an ellipse reads as an ellipse and a subtracted region shows its hole.
- Adjustments (destructive and as non-destructive adjustment layers) and
  filters (Gaussian blur, sharpen, unsharp mask, noise).
- A Photoshop-style **Color Picker**: square field + vertical ramp driven by
  the H/S/B/R/G/B radio buttons (each re-maps both axes as CS6 does),
  new/current comparison, HSB/RGB/Lab/CMYK readouts, hex entry, and web-safe
  snapping.
- Undo/redo with a linear History panel, bounded by state count and memory.
- Photoshop CS6 default keymap, loaded from data and user-remappable.

**Not yet implemented**

- **PSD layer pixel data.** The parser reads the header, layer records and the
  flattened composite, so opening a `.psd` gives you a single Background layer.
  Per-layer channel data, ZIP compression, 16/32-bit and CMYK/Lab return an
  explicit `Unsupported` error rather than wrong pixels. Writing emits a valid
  single-layer file. This is the largest remaining piece — see CLAUDE.md §8.
- GPU canvas rendering. Painting is `QPainter` today; the backend-agnostic
  renderer described in CLAUDE.md §7 has not been built.
- Text, shapes, paths, gradients, transforms, layer effects, adjustment-layer
  parameter dialogs, Channels/Paths/Navigator panels.
- The Marquee is the only fully implemented strip group. For every other tool
  the flyout lists the full CS6 group, but only the first entry works; the
  rest are shown disabled rather than silently falling back to the parent
  tool. Quick Mask toggles its button but does not yet change editing
  behaviour, and screen modes are not implemented.
- Tool icons are line-art reconstructions in CS6's visual language, not
  Adobe's artwork.
- In the Color Picker: Lab values are editable but have no radio buttons, so
  L/a/b cannot drive the field and ramp. CMYK is a direct conversion with no
  press profile, so it will not match a colour-managed Photoshop (which uses
  an ICC profile such as US Web Coated). "Add to Swatches" and "Color
  Libraries" are present for layout but disabled.

---

## Prerequisites

- **Qt 6** (6.4+) with QWidgets development headers
- A **stable Rust** toolchain (1.75+)
- **CMake** 3.24+
- A C++17 compiler

Corrosion and the CXX-Qt CMake module are fetched automatically at configure
time, so the first configure needs network access.

```bash
# Fedora
sudo dnf install qt6-qtbase-devel qt6-qtsvg-devel cmake gcc-c++ mold

# Debian / Ubuntu
sudo apt install qt6-base-dev libqt6svg6-dev cmake g++ mold

# macOS
brew install qt cmake rust
```

`mold` (or `lld`) is not strictly required, but the Rust build warns without
one and linking is much slower with GNU `ld.bfd`.

## Build and run

```bash
cmake -S . -B build -DCMAKE_BUILD_TYPE=Debug
cmake --build build -j$(nproc)

./build/photorust
```

CMake drives both halves: Corrosion invokes Cargo for `core/`, and the
resulting static library is linked into the shell. The theme, keymap and icon
are staged next to the executable, so a fresh build runs without installing.

To get a launcher entry and a desktop icon, install it:

```bash
cmake --install build --prefix ~/.local
```

That puts the binary in `~/.local/bin`, the icon into the hicolor theme and
`org.photorust.PhotoRust.desktop` into `~/.local/share/applications`. The
running window already shows the icon without installing; the desktop entry is
what gives it a launcher, a name and a file association for `.psd`.

### Tests

The engine's tests live beside the code they cover:

```bash
cd core && cargo test        # 221 tests
```

or through CTest, which runs them as part of the project:

```bash
ctest --test-dir build --output-on-failure
```

---

## Layout

```
core/                 Rust image engine
  src/buffer.rs         RGBA8 pixel buffers, rectangles
  src/blend.rs          the 27 blend modes
  src/layer.rs          layer model and stack
  src/compositor.rs     stack → final image (parallel, rayon)
  src/brush.rs          dab-based stroke rendering
  src/selection.rs      coverage-mask selections
  src/filters/          adjustments and convolutions
  src/history.rs        bounded linear undo
  src/document.rs       one open image; ties the above together
  src/psd/              .psd parsing and writing
  src/bridge.rs         the CXX-Qt QObject exposed to C++

shell/                C++ / Qt QWidgets application
  src/MainWindow.*      menus, docks, options bar
  src/canvas/           viewport, zoom/pan, input → document coordinates
  src/panels/           Layers, Color, History
  src/tools/            tool strip and tool metadata
  src/shortcuts/        command registry and keymap loading
  resources/theme.qss       CS6 dark theme
  resources/shortcuts.json  CS6 default keymap
```

## Conventions worth knowing before editing

These are the ones that cause real bugs if missed:

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

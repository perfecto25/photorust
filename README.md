# PhotoRust

![Photorust](icons/icon_small.svg)


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
  (Color/Swatches/Info tabbed, History, Layers), status bar.
- **Several documents open at once**, one tab each above the canvas. Each keeps
  its own layers, selection, history and zoom; File ▸ New and File ▸ Open add a
  tab rather than replacing what you had.
- Tool strip with SVG line-art icons, the corner triangle marking tools that
  have hidden tools, press-and-hold (or right-click) flyout menus listing each
  CS6 tool group, and the footer swatch / Quick Mask / screen-mode controls.
- Canvas viewport with zoom (CS6's zoom stops), an editable zoom field in the
  status bar, cursor-anchored wheel zoom,
  pan (space-drag or middle-drag), and a transparency checkerboard.
- Layers: add, delete, duplicate, reorder, merge down, flatten, rename,
  show/hide, opacity, fill opacity, clipping masks, layer masks, thumbnails.
- **Layer locks**, enforced in the engine rather than by greying out buttons:
  lock transparent pixels, lock image pixels, lock position, and lock all. A
  pixel-locked layer refuses every tool — brushes, healing, filters, fills — and
  a fully locked one cannot be deleted or merged either. Locked rows carry a
  padlock badge, solid for Lock All and outlined for a partial lock.
- A **CS6-shaped Layers panel**: the filter row, blend mode and Opacity, the Lock
  row and Fill, delegate-painted rows (eye column, bordered thumbnail, italic
  Background, padlock badge) and CS6's seven footer glyphs — all drawn as line
  art on the same 20×20 grid as the tool icons.
- All **27 Photoshop blend modes**, including the non-separable ones (Hue,
  Saturation, Color, Luminosity, Darker/Lighter Color).
- Brush engine: dab-based strokes with spacing, hardness falloff, flow and
  opacity; a whole stroke is one undo step. Dab edges are **area-sampled**, so a
  hard brush has a genuinely antialiased edge rather than a staircase. **Tip shape** (roundness and angle),
  **scattering** (spread and dab count) and **shape dynamics** (size, angle and
  roundness jitter), all reproducible from a fixed per-stroke seed so the live
  preview matches the commit.
- **Pencil** tool: the same engine with antialiasing off, so it lays whole
  pixels only, plus CS6's Auto Erase.
- **Color Replacement Brush**: recolours pixels matching a sampled colour,
  blending in Hue, Saturation, Color or Luminosity, with CS6's Sampling
  (Continuous, Once, Background Swatch), Limits (Discontiguous, Contiguous,
  Find Edges), Tolerance and Anti-alias. Color mode keeps each pixel's
  brightness, so shading survives being recoloured.
- **Dodge**, **Burn** and **Sponge**: the darkroom toning tools, with CS6's Range
  (shadows / midtones / highlights), Exposure, Protect Tones, and the Sponge's
  Desaturate / Saturate and Vibrance. Protect Tones works on luminance and keeps
  the pixel's own colour, so a dodge cannot bleach a hue or clip to white.
- **Blur**, **Sharpen** and **Smudge**: all three work dab by dab on what they
  pass over, so dwelling goes on deepening the effect. Blur and Sharpen are one
  tool with its sign flipped — toward or away from the local average — with CS6's
  Strength, cut-down Mode list, Sample All Layers and Sharpen's Protect Detail.
  Smudge carries a *patch* of pixels along the stroke, so structure streaks in the
  direction of travel, and Finger Painting drags in the foreground colour. Distinct
  from the Blur and Sharpen *filters*, which are one pass over a whole layer.
- **Paint Bucket**: the Magic Wand's flood, filled instead of selected — so
  Tolerance, Contiguous and Anti-alias behave identically to the wand's — plus
  Mode, Opacity and All Layers.
- **Pen tool**: full vector paths — corner and smooth anchors, direction
  handles that stay collinear through a smooth point, Auto Add/Delete, Rubber
  Band preview, closing a subpath, and Add/Delete Anchor Point and Convert
  Point as their own tools. Path Selection and Direct Selection edit whole
  subpaths or individual anchors/handles. A **Paths panel** holds named paths
  and a Photoshop-style Work Path, with Fill Path, Stroke Path and Load Path
  as a Selection (nonzero winding, so an oppositely-wound subpath cuts a hole).
  Freeform Pen fits a drag to corner anchors by simplification rather than
  true curve-fitting — a deliberate simplification, noted in the reference.
  Path geometry is not itself undoable, the same choice already made for
  slices and annotations; what a finished path paints or selects is.
- **Gradient tool**: all five CS6 shapes (linear, radial, angle, reflected,
  diamond) over CS6's 15 default presets, with Mode, Opacity, Reverse, Dither and
  Transparency. Ramps interpolate in straight alpha so a fade to transparent
  keeps its colour, and dither works on the colour quantisation so hard-edged
  presets stay hard. Preset swatches are rendered by the engine, so they cannot
  drift from what the tool paints.
- **Clone Stamp**: `Alt`+click sets the source, and the stroke copies those
  pixels verbatim — CS6's **Aligned** and **Sample** (current layer, current and
  below, all layers) included. Each stroke samples a snapshot taken when it
  began, so cloning with a short offset repeats the source once instead of
  smearing it down the stroke.
- **Mixer Brush**: paint that mixes. CS6's Wet, Load, Mix and Flow, its preset
  menu, the load swatch with Load and Clean Brush, the two after-each-stroke
  toggles, `Alt`+click to load paint off the canvas, and Sample All Layers. A wet brush picks colour up as it travels and
  carries it along, so a stroke across a boundary smears; a dry one paints its
  own paint like an ordinary brush and stops when the load runs out.
- A CS6-style **brush preset picker** behind the options bar's tip button:
  preview, Size and Hardness sliders, and a grid covering CS6's families — soft
  and hard round, flat and chisel, charcoal and chalk, spatter, star, grass.
  Thumbnails are rendered by the brush engine itself, so they cannot drift from
  what the brush paints.
- The whole healing family. **Spot Healing Brush** with CS6's three types:
  Proximity Match (a Laplace solve that continues the surrounding shading),
  Create Texture (the same, plus grain matched to the neighbourhood) and
  Content-Aware (patch synthesis inward from the boundary, so edges carry
  across). **Healing Brush**, which transplants an `Alt`-clicked source's
  texture by Poisson solve, so it takes the destination's lighting rather than
  the source's. **Patch** and **Content-Aware Move** (with Extend), both working
  on a dragged region. **Red Eye**, which neutralises only where red genuinely
  dominates, so it can be dragged loosely over an eye. Each is one undo step.
- Selections: all four marquee variants (rectangular, elliptical, single row,
  single column), all three lassos (freehand, polygonal, and magnetic with
  live-wire edge snapping), and both colour tools (Quick Selection's
  edge-stopping region growing, and the Magic Wand) — all with antialiased
  edges, add/subtract/intersect, invert and feather. `Shift+M`, `Shift+L` and
  `Shift+W` cycle within each group, as CS6 does. Marching ants trace the
  mask's real 50% contour, so an ellipse reads as an ellipse and a subtracted
  region shows its hole.
- Crop: a CS6-style box with eight handles, a dimmed shield outside it and a
  rule-of-thirds overlay, aspect-ratio presets, and CS6's Delete Cropped Pixels
  option — clear it and the pixels outside are kept, hanging off the canvas
  edge, ready to come back.
- Perspective Crop: mark the four corners of something that should be
  rectangular and it is straightened onto a rectangle through a homography,
  with bilinear resampling. Every layer and mask goes through the same
  transform, so the stack stays in register.
- Annotation tools: colour samplers (ten, as in CS6), a ruler reporting
  X/Y/W/H/angle/distance, pinned text notes, and numbered count markers. None
  of them touch pixels — they are document data alongside slices.
- An **Info panel** (`F8`) in CS6's layout: live RGB and CMYK under the cursor,
  cursor position, selection size, a numbered grid of sampler values, and the
  document's memory footprint. With the Ruler active it switches to CS6's
  ruler layout — angle and length in place of CMYK, and the ruler's deltas in
  place of the selection size.
- Slices: cut the canvas with the Slice tool and the rest is auto-sliced around
  it, numbered in reading order with CS6's blue and grey badges. Slice Select
  moves, resizes and deletes them, and File ▸ Save Slices writes each one out
  as its own PNG.
- Adjustments (destructive and as non-destructive adjustment layers) and
  filters (Gaussian blur, sharpen, unsharp mask, noise).
- A Photoshop-style **Color Picker**: square field + vertical ramp driven by
  the H/S/B/R/G/B radio buttons (each re-maps both axes as CS6 does),
  new/current comparison, HSB/RGB/Lab/CMYK readouts, hex entry, and web-safe
  snapping.
- Undo/redo with a linear History panel, bounded by state count and memory.
  A snapshot carries the canvas size as well as the layer stack, so Crop and
  Canvas Size step back correctly.
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
- Marquee, Lasso and Quick Selection are the fully implemented strip groups.
  For every other tool the flyout lists the full CS6 group, but only the first
  entry works; the rest are shown disabled rather than silently falling back to
  the parent tool. Quick Mask toggles its button but does not yet change editing
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
cd core && cargo test        # 358 tests
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
  src/canvas/           viewport, zoom/pan, input → document coordinates
  src/panels/           Layers, Paths, Color, History (LayerIcons/PathIcons hold the artwork)
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


## Development progress

```
ToolTip floating bar

- move tool - done
- marquee tools - done
- lasso tools - done
- quick selection tools - done
- crop tools - done
- eye dropper tools - done
- healing tools - done
- brush tools - done
- stamp and clone tools - in progress
- history brush tool - not started
- eraser tools - not started. 1/3 done
- gradient tools - done
- blur tools - done
- dodge tools - done
- pen tools - done
- type tools - done
- selection tools - not started
- shape tools - not started
- hand tools - not started
- zoom tools - 80% done, need work
- Color picker tool - basic functionality, need to add eye dropper picker

Top Menu bar - about 10% done, missing many features like Adjustments, Filters, etc

Auto recovery - save working project to temp file for auto recover - not started

Layers - add ability to lock layer from modification - done
```

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

## Large images

Measured on a 16507×16196 PNG world map (267 million pixels, 1.07 GB as
RGBA); any picture that size will do. Every cost below grows with the pixel
count, so a 12-megapixel photograph pays the same in proportion; it is just
fast enough not to notice.

| Step                                   | Time    |
|----------------------------------------|---------|
| Decode the PNG (Qt, worker thread)     | 1.9 s   |
| Build the document from the pixels    | 1.4 s   |
| Composite the whole document           | 1.0–1.4 s |
| Rebuild the view pyramid from it       | 0.1 s   |
| Update the pyramid after a stroke      | 0.45 ms |
| Fetch what a fit-to-screen paint shows | 1.7 ms  |
| Fetch what a 100% paint shows (with margin) | 11 ms |
| Start a stroke                         | under 1 ms (was 0.56 s: a needless undo snapshot) |
| Each mouse move of a stroke, with repaint, at 100% | 0.8 ms (was 70 ms: see the checkerboard below) |
| End a stroke, everything included      | 0.05 s — the history commit (was 0.72 s: a copy of every layer; then 1.1–1.9 s: see "What the shell asks after a stroke") |
| Undo or redo a stroke                  | 0.05 s, 1 MB of history (was impossible: see Part 3) |

The plan has three parts, all built — Part 3 by a different route than
first planned; see there.

### Part 1 — redraw only what changed (built)

When the document changes, only the part that changed is composited again.
The engine records what changed (`crate::damage`); the canvas asks
(`updateDisplay`, which also brings Part 2's pyramid up to date) and fetches
just that rectangle of the part on screen. A brush stroke on the map costs
its own bounds, under half a millisecond, where it used to cost a full
composite at both ends.

**The rule that keeps it correct: a change nobody described is a change to
the whole canvas.** All mutable access to the document in `bridge.rs` goes
through one of two accessors:

- `doc_mut()` — records an undescribed change. The canvas redraws everything,
  as it always did. This is the default, and it is always correct.
- `doc_mut_within()` — records nothing. The caller **must** describe what it
  changed with `mark_damage(rect)` (or `sync_within(rect)`) before handing
  control back. Wrong here means a stale picture, so use it only where the
  extent is certain.

A mark only ever *adds* to the damage. It cannot clear an undescribed change
made earlier by a different call; only the canvas catching up does that
(`crate::damage` explains the trap). `edit()` gives the whole engine and
counts as an undescribed change; plain `rust_mut()` is for engine-only state —
colours, brush and tool options, dialog bookkeeping — that never reaches the
composite. Use `edit()` only where the document is reached through the
binding: a brush setter that used it flagged the whole canvas on every call.

**A query is not a change.** Something that only reads the document, or
fills a cache on it — selection bounds memoises — goes through
`doc_mut_within()` with nothing to mark. Through `doc_mut()`, the Info panel
asking for the selection's bounds after each stroke made the canvas redraw
the whole document every time.

**Finding the culprit.** `PHOTORUST_TRACE_EDITS=1` prints every undescribed
change with the bridge call that made it. `CanvasView::wholeRedraws()` counts
the times the canvas recomposited everything, and
`tst_canvasdamage`'s `everydayUseLeavesTheCanvasNothingToRedraw` holds it
still through strokes, brush changes, hovering and tool switches in the real
window — so the next setter or query that does this fails there, by name.

`Document::composite_damage` grows a rectangle by the reach of any layer
style in the stack, because a drop shadow or outer glow draws past the pixels
that cast it. Everything else in compositing is per pixel.

Described today: brush, pencil, clone, pattern stamp and healing strokes
(begin, extend and patch change nothing on screen; the end marks the stroke's
bounds), and the Type tool's layer calls — adding a type layer, holding it
back for an edit, committing it — which mark the text layer's old and new
bounds, before and after the change, because a hidden layer's style does not
count towards the reach. Adding one into a clipping group still redraws
everything: the clipped layers above change base. Moving a layer — every
step of a Move-tool drag, and arrow-key nudges — marks where it and the
layers linked to it were and now are; a fill, adjustment or group layer
still redraws everything. Everything else still redraws the whole canvas — filters, fills,
transforms, layer changes. Undo, redo and history jumps describe themselves:
history knows which tiles it put back (Part 3). Describing more of the rest
is the natural way
to extend this: find the operation's extent, switch it to `doc_mut_within`,
mark it, and add a case to `shell/tests/tst_canvasdamage.cpp`, which checks
both that the damage is small and that what the canvas is handed there equals
the same rectangle of the full composite.

Two side effects of the damage tracking are worth knowing:

- The shell calls `refresh()` / `refreshAll()` about 110 times, often right
  after the engine has already refreshed the canvas. Those second refreshes
  now find nothing to redraw and cost nothing; they used to be a second full
  composite.
- Quick Mask strokes cannot be shown by patch — the stroke changes a veil
  over everything — so they are previewed whole: `CanvasView::m_override`
  holds that preview and is drawn instead of the view until the next
  catch-up drops it.

Panels must not composite the whole document on `canvasChanged`: it fires
after every stroke, and on the map each one costs over a second. The Channels
panel uses `compositeThumbnail`, which composites a few dozen rows rather than
the whole document. Whole-document composites remain in the adjustment
dialogs' histograms (Curves, Levels, Threshold), the Properties panel's
Curves editor, and saving; they run once per dialog or save, not per change.

### Part 2 — a reduced-size copy for zoomed-out views (built)

The composite the canvas shows lives in the engine as a pyramid
(`core/src/view.rs`): the composite itself at level 0, then copies halved
again and again down to 256 pixels on the long side — seven levels for the
map. The canvas never holds the whole picture. It holds only the part on
screen, from the level that matches the zoom (`CanvasView::viewLevel`: the
smallest level with at least one pixel per *device* pixel, so a high-density
screen gets the detail it can show), with half as much again on each side so
a short pan needs nothing new. It fetches another piece (`displayImage`) when
the view moves off what it has or the zoom calls for another level.

Every level is **premultiplied**. A level pixel is the average of the 2×2
block above it, and averaging straight-alpha pixels lets the colour of a
transparent one bleed in and darken soft edges; averaging premultiplied
pixels is correct. It is also the format Qt paints fastest.

The levels are kept up to date from Part 1's damage: a stroke recomposites
its bounds into level 0 and shrinks just that rectangle down through the
rest. Only an undescribed change rebuilds them all, and that costs the
composite plus 0.1 s. `view.rs` has the test that matters most: an update by
damage leaves every level byte-for-byte what a rebuild would, odd edges
included.

What it was actually worth, measured — including one thing it was expected
to fix and did not need to:

- **Zoomed-out quality.** QPainter's smooth scaling of the whole picture was
  cheap (2.3 ms a paint at fit-to-screen) because it samples only the source
  pixels it needs — which at 1:20 means skipping 19 in 20. Fine lines, text
  and map detail shimmered or vanished. Drawing from a level whose pixels are
  true averages shows them properly.
- **The channel mask.** Hiding a channel converted the whole picture on every
  paint: 0.9 s a repaint on the map. It now converts the part held, a window's
  worth.
- **Memory and full refreshes.** The canvas no longer keeps a gigabyte
  `QImage`, and a full refresh no longer deep-copies one across the bridge
  (0.47 s). The engine's pyramid is a third bigger than level 0 alone, but it
  replaces the canvas's copy rather than adding to it.

Not a GPU candidate, though shrinking is per pixel: each level is read back
by the CPU the moment it is made, which is the case CLAUDE.md §7 turns away.

Two limits. A crop fetched mid-stroke — scrolling with the wheel while
painting — comes from the document, which does not have the stroke yet, so
the stroke's dabs disappear from that part of the view until it ends.
And stroke patches are laid into a level below 0 scaled by QPainter, which is
close but not exact; the stroke's end fetches that rectangle from the pyramid
and replaces them.

### Part 3 — history as tiles (built)

History used to keep a full copy of the layer stack per state, under a 1 GiB
budget. That made undo impossible on a large picture — before and after would
not both fit, so the state before an edit was evicted the moment the edit was
committed; any document with more than half a gigabyte of layers had no undo
at all — and every commit copied every layer (0.72 s on the map, blocking the
GUI thread just after a stroke ends).

Now (`core/src/history.rs`) history keeps **one** whole stack, the state at
the cursor, and for every other state only what differs: each layer's
settings, kept whole because they are small, and just the 256×256 tiles of
its pixels and mask that changed. Committing compares the live stack with
history's copy tile by tile — spread over every core, it is most of the
0.05 s a commit now takes on the map — records the old contents of the tiles
that differ, and copies the new ones in. Undo swaps a state's tiles back,
which yields the tiles to redo with, and brings the live stack into line the
same way. The budget now covers the recorded changes only; history's own copy
is the one the document always needs.

Nothing tells history what changed; the comparison finds it. So no operation
can forget to record something, and undo is correct for anything the engine
does. The model test in `history.rs` holds it to that: random edits of every
kind — painting, fills, layers added, deleted, reordered, cropped, masked and
unmasked, settings, coalesced steps — interleaved with undo, redo and jumps,
checked byte for byte against the full-snapshot history it replaced.

Comparing a gigabyte on every commit was still a full core-load for every
step of a Move-tool drag, which commits each step. So a `Pixmap` carries a
**stamp**: two with the same nonzero stamp hold the same bytes. Every
`&mut self` method on `Pixmap` clears it before it can change a byte, a
clone keeps it, and history sets one only on its copy and the live buffer
once it has made them agree. A commit skips any layer whose stamps match —
nearly all of them — and compares the rest as before, so nothing still has to
say what it changed. A new `&mut self` method on `Pixmap` must call `touch`
first; `a_stamp_is_shared_by_copies_and_lost_by_any_change` lists them.

Undo also tells the canvas where it changed the picture (`Document::
restored_damage`): the tiles it put back, where their layers sit, or
"everywhere" if a layer came, went, moved or changed a setting. Undoing a
stroke on the map redraws its tiles, not the document.

**Why not tiled layer storage**, which is what this part originally proposed:
it would reach the same result for undo and commit, but layer pixels are read
or written directly in about 386 places across 14 engine files, all written
against one contiguous buffer. History was the only thing that needed tiles,
and it is one module with four callers. Tiled storage is still the natural
next step for keeping layers resident on the GPU, which compositing needs
before it can be switched on there (`docs/gpu-migration.md`, phase 4) — and
history's tile arithmetic is the place to start it from.

What remains: opening a document copies it once for history (part of the
1 s it takes to open the map), and so history's copy doubles the memory a
document holds — the same as the old minimum of the document plus one state,
but no longer growing with every step.

### What the shell asks after a stroke

A stroke's end is 0.05 s in the engine, and it used to take 1.1–1.9 s in the
window, all of it in listeners. Found by timing each slot connected to the
engine's signals, one by one, after a stroke:

- The selection-bounds query and the brush setters above, which made the
  canvas rebuild the whole pyramid (about 1.1 s on the map).
- The Properties panel shows the active layer's content bounds, and the
  bounds were found by looking at every pixel through `get()`, on one core —
  1.05 s for the map. `Pixmap::content_bounds` works inward from the edges
  instead: an opaque layer answers at its first pixel each row, and only a
  mostly empty layer is scanned through, in parallel.

Anything connected to `canvasChanged`, `layersChanged` or `historyChanged`
runs after every stroke. On a large picture, a listener that looks at every
pixel is felt as the next stroke starting late and straight — the release is
still being handled when it begins.

### Other things a large image runs into

- **Mouse events are merged while the GUI thread is busy.** Anything that
  blocks it at the start of a stroke or during one shows up as a straight
  segment where the pointer curved, because Qt delivers the moves it missed
  as one — including the history commit after the *previous* stroke, if the
  next one starts before it finishes (now 0.05 s). Two such costs were found
  this way: `begin_stroke` copied the whole
  layer stack for a cancel that had nothing to restore (brush-type strokes
  touch the layer only when they end; the tools that change it as they go
  take their own copy), and the canvas's transparency checkerboard.
- **Paint only what is on screen.** The checkerboard was laid a square at a
  time over the whole document, visible or not — two million `fillRect`s a
  repaint at 100% on the map, 70 ms for every mouse move of a stroke. It is
  now one fill with a tiled brush. Any per-repaint loop over document
  coordinates has the same trap.

- **Qt refuses to decode images over 256 MB** unless told otherwise.
  `main.cpp` raises the limit to 4 GB, enough for a 30000-pixel-square PSD.
- **Opening decodes on a worker thread** in `MainWindow::loadPath`, with the
  status bar's spinner turning; building the document and the first
  composite still block the GUI thread. The shell hands the engine one block
  of RGBA bytes (`loadImageRgba`); cxx-qt-lib gives no access to a QImage's
  bits, so reading one pixel at a time across the bridge — what `loadImage`
  does — is a quarter of a billion calls for the map.
- **A GPU blur is sent in bands of rows** (`TAPS_PER_SUBMISSION` in
  `gpu/wgpu_backend.rs`). One submission covering a large image at a large
  radius runs long enough for the driver to reset the GPU, and a lost device
  is then refused and every later call goes to the CPU.

---

## Module layout

```
core/                 Rust image engine
  src/buffer.rs         RGBA8 pixel buffers, rectangles
  src/blend.rs          the 27 blend modes
  src/layer.rs          layer model and stack
  src/compositor.rs     stack → final image (parallel, rayon)
  src/damage.rs         what part of the canvas changed since it was drawn
  src/view.rs           the composite as the canvas shows it, at every zoom
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
  src/history.rs        undo: one copy of the stack, and the tiles each step changed
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
- **Reach the document through `doc_mut()` or `doc_mut_within()`** in
  `bridge.rs`, never `rust_mut().doc`. The first tells the canvas to redraw
  everything; the second is faster and obliges you to `mark_damage` what you
  changed. See [Large images](#large-images).
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
  would need. The failure only shows on small images — a large buffer usually
  still holds the right pixels after it is freed — so a test on a big composite
  will not catch its return; the copy was once removed in error for exactly
  that reason. The canvas no longer asks for whole composites (see Part 2), so
  the copy is now of what is on screen; the remaining whole-composite callers
  are dialogs and saving.

User keymap overrides are written to
`~/.config/PhotoRust/shortcuts.json` (Linux) or the equivalent
`AppConfigLocation` on macOS; only bindings that differ from the defaults are
stored, so future default changes still reach the user.

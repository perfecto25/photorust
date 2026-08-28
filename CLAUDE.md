# CLAUDE.md

This file gives Claude Code (and any contributor) the context needed to work on this
project effectively. Read it fully before making architectural decisions.

Companion documents:

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — tech stack, module layout, and
  the conventions that cause real bugs if missed.
- [docs/gpu-migration.md](docs/gpu-migration.md) — GPU phase plan, benchmarks,
  and findings. Read before any GPU work.
- [docs/BUILDING.md](docs/BUILDING.md) — prerequisites and build steps.
- [docs/ROADMAP.md](docs/ROADMAP.md) — what is done and what is not.
- [docs/REFERENCE.md](docs/REFERENCE.md) — the CS6 interface being reproduced.

---

## 1. What this project is

A from-scratch clone of **Adobe Photoshop CS6 (the 2012–2013 "Kona" dark UI)**.

Hard requirements that shape every decision:

- The **graphical user interface must match the original as closely as possible** — the
  dark theme, the dockable/floating panels (Color, Swatches, Layers, Channels, Paths,
  History, etc.), the left tool strip, the top options bar, and the overall layout.
- The **feature set should mirror CS6** — layers, blend modes, masks, selections,
  brushes, filters, adjustment layers, history/undo, and `.psd` import/export.
- **Keyboard shortcuts must match Photoshop's defaults** and should be remappable like
  the original.
- **Primary platforms are Linux and macOS.** Windows is not a target. Do not introduce
  Windows-only dependencies.

This is a large, long-horizon project. Favor clean module boundaries over speed.

---

## 2. Architecture: "C++ shell, Rust core"

The project is deliberately split into two layers, each playing to its language's
strength. **Do not blur this boundary.**

```
┌─────────────────────────────────────────────────────────┐
│  C++ / Qt shell  (QWidgets)                               │
│  - Main window, dock widgets, toolbars, menus, panels     │
│  - Photoshop-style docking UX (native to QWidgets)        │
│  - Canvas viewport (presents with QPainter today)          │
│  - Keyboard shortcut / command registry                   │
│  - Tool input handling (mouse, tablet/pressure)           │
└───────────────────────────┬───────────────────────────────┘
                            │  CXX-Qt bridge (safe FFI)
┌───────────────────────────┴───────────────────────────────┐
│  Rust core  (the "engine")                                 │
│  - Pixel buffers & memory management                       │
│  - Layer model + compositing + blend modes                 │
│  - Filters / convolutions / adjustments                    │
│  - Selections, masks                                       │
│  - History / undo stack                                    │
│  - .psd parsing and serialization                          │
│  - Multithreaded image operations (rayon)                  │
│  - GPU compute via wgpu, CPU path as the reference (§7)    │
└─────────────────────────────────────────────────────────┘
```

### Why this split

- **QWidgets (C++) is the right tool for the UI.** It is Qt's classic desktop toolkit and
  ships with native dock widgets, menus, and toolbars — the panel-docking behavior maps
  almost one-to-one onto Photoshop's interface, so we get it largely for free.
- **The Rust↔Qt bindings do NOT idiomatically cover QWidgets.** The available bindings
  (CXX-Qt, qmetaobject-rs) are oriented around QML/QtQuick. Trying to build the docking
  UI from Rust would mean reconstructing dock behavior by hand in QML. So the **UI stays
  in C++.**
- **Rust is the right tool for the engine.** The image core is the part full of pixel
  buffers, threading, and manual memory management — exactly where Rust's safety and
  performance pay off most.

> Rule of thumb: **if it's a widget, it's C++. If it touches pixels, it's Rust.**

---

## 3. The bridge (CXX-Qt)

- We use **CXX-Qt** (by KDAB) for Rust ⇄ C++ interop. It is the actively maintained,
  idiomatic option and is built on top of the `cxx` crate to keep the `unsafe` surface
  small.
- The Rust core exposes its functionality as **QObject subclasses defined in Rust**,
  which the C++ shell instantiates and calls like any other QObject.
- When a Qt C++ API we need isn't wrapped by the bindings, we can drop into C++ directly
  from Rust using the `cpp!` macro — so a missing binding is never a hard blocker.
- Keep the bridge **thin and explicit**. Define a small, stable set of bridge types
  (e.g. `EngineHandle`, `LayerId`, `ImageBuffer` views). Do not leak large or rapidly
  changing Rust internals across the FFI boundary.

---

## 4. Tech stack

| Layer            | Choice                                                        |
|------------------|---------------------------------------------------------------|
| UI toolkit       | Qt 6, **QWidgets** (C++)                                       |
| UI theming       | QSS stylesheets to reproduce the CS6 dark "Kona" theme        |
| Image engine     | **Rust** (stable toolchain, `cargo`)                          |
| FFI bridge       | **CXX-Qt** (`cxx-qt`, `cxx-qt-lib`, `cxx-qt-build`)           |
| Build system     | **CMake** orchestrating C++ + invoking Cargo for the Rust core|
| GPU              | **`wgpu`** compute behind a backend seam, CPU fallback (§7)    |
| `.psd` support   | Custom Rust parser (optionally referencing `libpsd`)          |

---

## 5. Directory layout

> The full, current module-by-module map is in
> [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). This is the shape, not an
> inventory.


```
/
├── CLAUDE.md                # this file
├── CMakeLists.txt           # top-level build; builds C++ shell, invokes Cargo
├── shell/                   # C++ / Qt QWidgets application
│   ├── src/
│   │   ├── main.cpp
│   │   ├── MainWindow.*      # dock layout, menus, toolbars
│   │   ├── panels/           # Layers, Color, History, etc. (one widget each)
│   │   ├── canvas/           # viewport widget, zoom/pan, input mapping
│   │   ├── tools/            # tool input handling (brush, selection, crop…)
│   │   └── shortcuts/        # command registry + keymap loading
│   └── resources/
│       ├── theme.qss         # CS6 dark theme
│       ├── icons/            # tool + panel icons
│       └── shortcuts.json    # default Photoshop keymap (remappable)
├── core/                    # Rust image engine
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── bridge.rs         # CXX-Qt QObject definitions exposed to C++
│       ├── buffer.rs         # pixel buffers
│       ├── layer.rs          # layer model
│       ├── compositor.rs     # blend modes + compositing
│       ├── gpu/              # wgpu backend seam, CPU fallback, shaders
│       ├── filters/          # convolutions, adjustments
│       ├── selection.rs
│       ├── history.rs        # undo/redo
│       └── psd/              # .psd read/write
└── tests/
```

---

## 6. Build & run

Prerequisites and per-distro package names: [docs/BUILDING.md](docs/BUILDING.md).

```bash
# Configure + build everything (CMake drives Cargo for the Rust core)
cmake -S . -B build -DCMAKE_BUILD_TYPE=Debug
cmake --build build -j$(nproc)

# Run
./build/photorust

# Work on the Rust core in isolation
cd core
cargo build
cargo test

# Force the CPU backend, or pin a graphics API
PHOTORUST_BACKEND=cpu ./build/photorust
WGPU_BACKEND=vulkan   ./build/photorust
```

When debugging, remember failures can originate on **either** side of the bridge — check
both the C++ shell and the Rust core.

---

## 7. GPU acceleration

**This exists now.** The abstraction layer this section used to ask for is
`core/src/gpu/`, built on **`wgpu`** — Vulkan on Linux, Metal on macOS, without
routing through MoltenVK by hand. The full phase plan, measurements and open
questions live in **`docs/gpu-migration.md`**; read it before doing GPU work.

The rules that matter when touching this:

- **The CPU path is the reference and never goes away.** Every accelerated
  operation has a CPU implementation it must agree with, and a machine with no
  usable adapter runs every feature. A GPU is a speed-up, never a requirement.
- **Every migrated operation needs a parity test** against the CPU before it
  counts as done. A wrong blend mode is a subtly wrong colour, not a crash, so
  this will not be caught by "it ran".
- **Measure before enabling.** Compositing is the cautionary tale: the shader is
  complete and matches the CPU on all 27 blend modes, but it is *slower*,
  because each call re-uploads the whole layer stack. It is deliberately left
  switched off. Correct-but-slower is not worth shipping.
- **No `#[cfg(target_os)]` branching on graphics APIs** in engine code. `wgpu`
  is the abstraction; if something needs a platform branch, it belongs behind
  the `RenderBackend` seam.
- **One device per process.** Use `gpu::shared()`. Creating a device per call
  site — or per test — crashes the driver.
- The C++ shell gets **no shader code**. Pixels are Rust's business (§2). The
  one legitimate exception would be canvas presentation, which is not built.

### Writing a new pixel operation — filter, adjustment, brush, anything

This applies to **all** future work, not just to the migration phases. Do not
add a CPU-only pixel operation and consider it finished.

1. **Write the CPU version first.** It is the reference, it is what runs on a
   machine without a GPU, and the GPU version is defined as "agrees with this".
2. **Decide whether it fits the GPU.** It fits when the work is per-pixel or
   per-neighbourhood, the same for every pixel, and there is enough of it. It
   does *not* fit when the algorithm is inherently sequential (flood fill,
   magic wand region growing), when it touches only a small region (a brush
   dab's bounding box), or when the caller would immediately need the result
   back on the CPU anyway.
3. **If it fits, add it to `RenderBackend`** and implement both sides. Keep the
   trait narrow — it is for work worth accelerating, not for every function in
   the engine.
4. **Write the parity test before believing it.** Compare against the CPU
   across sizes that are not multiples of the workgroup, at edges, and with
   transparency. Match exactly where the operation is discrete (Dissolve), and
   within a level or so where float accumulation order differs.
5. **Benchmark it, then decide whether to switch it on.** Add a case to
   `core/examples/` and record the numbers in `docs/gpu-migration.md`. A
   correct shader that is slower stays off — compositing is the standing
   example.
6. **Gate on size, and fall back on anything unexpected.** `MIN_GPU_PIXELS`
   exists because the fixed per-operation cost dominates small images. Wrong
   bit depth, oversized input or a device error must fall back to the CPU, not
   fail the user's edit.

If an operation does not fit the GPU, say so in a comment where the next person
will look, and why. "Considered and rejected, because flood fill is sequential"
is a useful thing to find; silence is not.

---

## 8. Known hard problems (don't underestimate these)

- **The `.psd` file format is the single nastiest piece of work.** It is poorly
  documented, full of legacy quirks, and faithful parse/write is a sub-project of its
  own. Reference `libpsd` and the published format spec; isolate all of it under
  `core/src/psd/`. Expect this to take real time.
- **UI fidelity is exacting.** Matching CS6 means matching spacing, panel behavior, icon
  placement, and theme details — not just "a dark UI." Compare against reference
  screenshots frequently.
- **Real-time performance** for brush strokes and large-image compositing is a core
  requirement, not a nice-to-have. Keep hot pixel paths in Rust, use SIMD/threads, and
  avoid copying buffers across the FFI bridge — pass views/handles, not data.
- **Moving pixels to and from the GPU is the cost that dominates**, not the
  arithmetic once they are there. An operation that uploads its input and reads
  the result straight back can easily be slower than doing it on the CPU — that
  is exactly why GPU compositing is written but not enabled. Batch work while it
  is resident; measure before assuming a shader is a win.

---

## 9. Keyboard shortcuts

- Shortcuts are **data, not code**: a central **command registry** in the C++ shell maps
  key combos → command IDs → engine/UI actions.
- Default bindings live in `shell/resources/shortcuts.json` and **must match Photoshop's
  CS6 defaults**.
- The mapping must be **user-remappable**, mirroring Photoshop's own behavior.
- Adding a feature = register its command in the registry, then bind it in the keymap.
  Never hard-code a key combo inside a widget.

---

## 10. Conventions for working in this repo

- Respect the layer boundary: **UI/widgets → C++**, **pixels/engine → Rust**. If you find
  yourself doing image math in C++ or building widgets in Rust, stop and reconsider.
- Keep the **CXX-Qt bridge surface minimal and stable**; changing it is expensive on both
  sides.
- Do not add Windows-only dependencies (Linux + macOS only).
- New panels go under `shell/src/panels/` as self-contained widgets and dock into
  `MainWindow`.
- New filters/adjustments go under `core/src/filters/` and are exposed to the shell
  through the bridge.
- Prefer correctness and clear module boundaries over premature optimization — except on
  the known hot paths (compositing, brush rendering), where performance is the spec.
- **Every new pixel operation is a GPU candidate.** Write the CPU version first,
  then apply the checklist in §7 before calling it done. "It works on the CPU"
  is not a finished feature for anything that touches more than a few thousand
  pixels.

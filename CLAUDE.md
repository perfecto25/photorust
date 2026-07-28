# CLAUDE.md

This file gives Claude Code (and any contributor) the context needed to work on this
project effectively. Read it fully before making architectural decisions.

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
│  - Canvas viewport + GPU presentation                     │
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
│  - Multithreaded image operations                          │
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
| GPU              | Abstraction layer over OpenGL/Vulkan (see §7)                 |
| `.psd` support   | Custom Rust parser (optionally referencing `libpsd`)          |

---

## 5. Suggested directory layout

```
/
├── CLAUDE.md                # this file
├── CMakeLists.txt           # top-level build; builds C++ shell, invokes Cargo
├── shell/                   # C++ / Qt QWidgets application
│   ├── src/
│   │   ├── main.cpp
│   │   ├── MainWindow.*      # dock layout, menus, toolbars
│   │   ├── panels/           # Layers, Color, History, etc. (one widget each)
│   │   ├── canvas/           # viewport widget + GPU presentation
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
│       ├── filters/          # convolutions, adjustments
│       ├── selection.rs
│       ├── history.rs        # undo/redo
│       └── psd/              # .psd read/write
└── tests/
```

---

## 6. Build & run

> Exact commands depend on the final CMake setup; keep this section updated as it
> stabilizes.

Prerequisites: a C++ toolchain, **Qt 6** (with QWidgets), a stable **Rust** toolchain,
and **CMake**.

```bash
# Configure + build everything (CMake drives Cargo for the Rust core)
cmake -S . -B build
cmake --build build

# Run
./build/shell/photoclone        # Linux
./build/shell/photoclone.app    # macOS bundle

# Work on the Rust core in isolation
cd core
cargo build
cargo test
```

When debugging, remember failures can originate on **either** side of the bridge — check
both the C++ shell and the Rust core.

---

## 7. GPU acceleration (platform-specific — plan for this early)

GPU support differs between our two target platforms and **must be designed against an
abstraction layer from day one**, not retrofitted:

- **Linux:** OpenGL / Vulkan directly.
- **macOS:** Apple has deprecated OpenGL in favor of **Metal**. Plan to route through
  **MoltenVK** (Vulkan-on-Metal) or otherwise abstract the backend.

Qt abstracts a lot of this for general UI rendering. But **if we hand-roll the canvas
renderer** (likely, for real-time brushes and large-image compositing), put it behind a
backend-agnostic interface. `wgpu` (Rust, targets Vulkan/Metal/etc.) is a strong option
if the renderer lives on the Rust side.

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

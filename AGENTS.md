# AGENTS.md

Guidance for coding agents working in this repo. The project rules you must
not violate live in [CLAUDE.md](CLAUDE.md) — read it before touching the
C++/Rust boundary or GPU code. Deep docs: `docs/ARCHITECTURE.md`,
`docs/BUILDING.md`, `docs/gpu-migration.md`.

## Shape

Photoshop CS6 clone. **C++/Qt 6 QWidgets shell** (`shell/`) over a **Rust
image engine** (`core/`), joined by CXX-Qt (`core/src/bridge.rs` is the only
FFI surface). Rule of thumb: widgets → C++, pixels → Rust. Linux + macOS
only — configure fails on Windows by design; do not add Windows-only
dependencies. Matching CS6 exactly — theme, dock behaviour, tool layout,
shortcut defaults — is the spec, not polish; `docs/REFERENCE.md` is the
target.

## Build & run

```bash
cmake -S . -B build -DCMAKE_BUILD_TYPE=Debug   # first configure downloads Corrosion + cxx-qt-cmake: needs network
cmake --build build -j$(nproc)
./build/photorust                              # resources are staged next to the binary; no install step needed
```

Install `mold` (or `lld`): the Rust build warns without one and links far
slower with GNU ld.

## Tests

```bash
cd core && cargo test                       # engine tests
ctest --test-dir build --output-on-failure  # everything: cargo tests + Qt shell tests
ctest --test-dir build -R tst_keymap        # one shell test
```

- Standalone `cargo test` in `core/` needs `qmake` on PATH (cxx-qt-build);
  CTest sets `QMAKE` itself.
- Shell tests are Qt Test binaries run headless with
  `QT_QPA_PLATFORM=offscreen` (CTest sets this; needed if run by hand).
- GPU parity tests pass trivially on machines with no GPU — green there proves
  nothing about acceleration. To verify the fallback explicitly:
  `PHOTORUST_BACKEND=cpu cargo test`, `WGPU_BACKEND=dx12 cargo test`.

## CMake/Cargo sync points (easy to break)

- `QT_MODULES` in root `CMakeLists.txt` must match the `qt_module()` list in
  `core/build.rs`, or the Cargo build aborts with a mismatch error.
- `cxx-qt-lib` must stay on `features = ["qt_gui"]` — never `qt_full` (drags
  in Qt Qml for a QWidgets-only app and breaks that match).
- `include(CTest)` in root `CMakeLists.txt` must stay **before**
  `add_subdirectory(shell)`; after it, the shell tests silently stop
  registering.
- `core/Cargo.toml` has `[profile.dev] opt-level = 2` on purpose (an
  unoptimized engine makes the debug UI unusably slow). Do not remove.
- The shell links `photorust_core` PUBLICly — it carries the CXX-Qt generated
  headers and initializer objects as usage requirements. Keep that ordering.

## Bridge (CXX-Qt)

- Keep the FFI surface thin and stable — changing it is expensive on both
  sides. Stick to the small set of bridge types; don't leak Rust internals
  across. On hot paths (compositing, brushes) pass views/handles, not buffer
  copies.
- A Qt API with no Rust binding is not a blocker: drop into C++ with the
  `cpp!` macro.
- `QImage`s crossing the FFI must own their pixels — `bridge.rs::pixmap_to_qimage`
  deep-copies. Wrapping a Rust buffer with `QImage::from_raw_bytes` and
  returning it leaves C++ pointing at freed memory.

## Engine conventions that cause real bugs

- Layer stacks are **bottom-first** (index 0 = Background); the Layers panel
  shows top-first. The flip happens **only** in `core/src/bridge.rs` —
  everything Rust-side uses stack indices, everything past the bridge uses
  panel indices.
- Colour is **straight (non-premultiplied) alpha** engine-wide. Exception:
  blur/convolution run premultiplied (filtering straight alpha bleeds
  transparent colour into visible pixels as dark halos).
- Selection queries (`is_empty`, `bounds`, `outline`) are O(canvas); hoist
  them out of per-pixel/per-dab paths. `canvasChanged` fires per brush dab;
  only `selectionChanged` may trigger a re-trace.
- Shortcuts are data: register a command in `shortcuts/CommandRegistry`, bind
  it in `shell/resources/shortcuts.json`. Never hard-code a key combo in a
  widget.

## Any new pixel operation (GPU rules, condensed)

CLAUDE.md §7 and `docs/gpu-migration.md` are required reading first; this is
only the shape:

1. Write the CPU version first — it is the reference and never goes away.
2. Add a GPU path only if the op fits (uniform per-pixel/per-neighbourhood,
   enough work); say so in a comment where it doesn't and why.
3. Parity-test GPU vs CPU (odd sizes, edges, transparency, exact for discrete
   ops) **before** believing the shader.
4. Benchmark with `cargo run --release --example blur_bench` (or add an
   example); record numbers in `docs/gpu-migration.md`. Correct-but-slower
   stays disabled — the compositing shader is the standing example.
5. Gate on `MIN_GPU_PIXELS` (128×128) and fall back to CPU on any failure
   (wrong bit depth, oversized input, device error — never fail the edit).
6. One wgpu device per process via `gpu::shared()` — a device per call site or
   per test crashes the driver.

Also: no `#[cfg(target_os)]` branching on graphics APIs in engine code —
`wgpu` is the abstraction, platform specifics hide behind the `RenderBackend`
seam. The C++ shell gets no shader code. A CPU-only pixel op is not
"finished" — either migrate it per the checklist or leave a comment at the op
saying why it doesn't fit (e.g. flood fill is sequential).

## Where new code goes

- Panels/widgets/dialogs → `shell/src/panels|dialogs`, docked from `MainWindow`.
- Filters/adjustments → `core/src/filters/`, exposed via `bridge.rs`.
- `.psd` work stays isolated in `core/src/psd/`. It is the nastiest subsystem
  (poorly documented format); unsupported features (16/32-bit, CMYK/Lab, ZIP
  compression) are refused with a clear error rather than emitting wrong
  pixels — protect that behaviour.
- Commit style: short, lowercase, no prefixes (see `git log`).

# GPU migration plan

How PhotoRust moves from a CPU-only engine to one that uses the GPU when the
machine has a usable one, without losing the CPU path.

This is a working document. Tick items as they land; keep the notes about *why*
a thing is hard, because those are the parts that bite twice.

**Status: Phase 0 and Phase 1 complete and live. Gaussian blur runs on the
GPU, 4–70× faster. Phase 2 (compositing) is written and proven correct but
deliberately not enabled — it is slower until layer pixels stay resident on
the GPU, which moves Phase 4 ahead of it.**

---

## Ground rules

These come from CLAUDE.md and from what the code already does. Breaking one of
them is a design change, not an implementation detail.

- **The CPU path never goes away.** It is the reference implementation. The GPU
  is correct exactly insofar as it agrees with the CPU, and a machine with no
  usable adapter must still run every feature.
- **Pixels are Rust's business** (CLAUDE.md §2). The shell does not gain shader
  code. The one legitimate exception is presentation — see Phase 5.
- **Backend-agnostic by construction** (CLAUDE.md §7). No `#[cfg(target_os)]`
  branching on Vulkan vs Metal in engine code; `wgpu` is the abstraction and
  the backend seam is `RenderBackend`.
- **Every migrated operation needs a parity test** against the CPU result
  before it is considered done.
- **No silent truncation.** `max_texture_dimension_2d` is 16384 on the
  development machine and documents can exceed it. An operation that cannot fit
  its input tiles it or hands back to the CPU. It never quietly crops.

---

## What is not true yet

- `GpuBackend::composite` **still returns the CPU result** — not because the
  shader is missing, but because it is slower. See Phase 2.
- Only **8-bit** pixmaps take the GPU blur path. 16- and 32-bit fall back —
  see the note on `bpc` under Phase 1.
- Nothing keeps pixels resident on the GPU. Every accelerated operation
  uploads and reads back, so chaining two of them pays the transfer twice.
  That is Phase 4, and it is what limits live-preview responsiveness today.
- Presentation is still a CPU blit (`QPainter::drawImage`). Phase 5.

---

## Phase 0 — Foundation ✅ done

- [x] Add `wgpu` + `pollster` to `core/Cargo.toml`
- [x] `core/src/gpu/mod.rs` — `RenderBackend` trait, `BackendKind`,
      `BackendPreference`, `select()`
- [x] `core/src/gpu/device.rs` — `GpuProbe`, adapter discovery, `DeviceInfo`
      with `max_texture_dimension`
- [x] `core/src/gpu/cpu.rs` — `CpuBackend` wrapping the existing rayon code
- [x] `core/src/gpu/wgpu_backend.rs` — owns device/queue, `fits_in_texture()`
- [x] `select()` never fails; missing or broken GPU falls back with the reason
      recorded in `DeviceInfo::detail`
- [x] `PHOTORUST_BACKEND=cpu|gpu|auto` override
- [x] Engine holds one backend, chosen once at startup
- [x] Bridge: `renderBackend()`, `renderBackendDetail()`, `usingGpu()`
- [x] Surfaced in Help > About
- [x] Tests: forced-CPU selection, fallback always yields a working backend,
      every backend describes itself
- [x] Verified on real hardware: AMD Radeon 780M / Vulkan / 16384px

---

## Phase 1 — First compute pipeline: Gaussian blur ✅ done

**Storage buffers, not textures.** The plan originally called for
`gpu/texture.rs`. Buffers turned out to be the better mapping and removed two
whole problem areas: no 256-byte `bytes_per_row` alignment on readback, and no
`max_texture_dimension_2d` ceiling, so **no tiling was needed at all**. The
bound is `max_storage_buffer_binding_size` in bytes, which is a single flat
check. Reasoning is written up at the top of `gpu/transfer.rs`.

- [x] `gpu/transfer.rs` — upload, scratch buffers, and blocking readback
- [x] `gpu/blur.wgsl` — two compute passes, horizontal then vertical
- [x] Kernel weights uploaded as a storage buffer, not baked per radius
- [x] Reuse `convolve::gaussian_kernel_1d` rather than reimplementing it, so
      the weights cannot drift from the CPU's
- [x] Wire into `GpuBackend::gaussian_blur`
- [x] Fall back to CPU on wrong depth, oversized image, or device error
- [x] Route real call sites through `convolve::gaussian_blur_accelerated`:
      HDR Toning (`document.rs`), Filter ▸ Blur ▸ Gaussian Blur
      (`filters/mod.rs`), and `unsharp_mask`
- [x] `gpu::shared()` — one device per process, rather than threading a
      backend handle through every signature in the engine

**Decisions that differed from the plan, and why:**

- **Premultiply stays on the CPU.** It is integer-exact there and matches the
  reference bit for bit; it is O(n) and not the bottleneck. Doing it in the
  shader would have added a parity risk for no measurable gain.
- **8-bit only.** The CPU reference itself is only correct at 8-bit —
  `blur_pass` writes at `x * 4` while the row stride is `width * 4 * bpc`, so
  16/32-bit blur was already broken before any of this. Rather than invent
  new, differently-wrong behaviour on the GPU, deeper pixmaps fall back.
  **This is a pre-existing bug worth fixing on its own.**
- **The threshold is pixel count, not radius.** The plan assumed small radii
  should go to the CPU. Measurement says otherwise: once the image is large
  enough the GPU wins even at sigma 0.5. What actually matters is the fixed
  per-operation cost, so the gate is `MIN_GPU_PIXELS = 128×128`. Measured
  crossover: 48×48 loses at every radius, 96×96 wins at every radius.

**Proving it:**

- [x] Parity against CPU across radii (1, 5, 50), max 1 level of difference
- [x] Parity at a radius wider than the image, so every sample hits the edge
      clamp
- [x] Parity on sizes that are not multiples of the 8×8 workgroup (1×1, 13×7,
      8×33)
- [x] Fully transparent image stays transparent (catches premultiply bugs)
- [x] Flat image stays flat (catches unnormalised weights)
- [x] Tests pass with `PHOTORUST_BACKEND=cpu`, so the fallback path is covered
- [x] `core/examples/blur_bench.rs` — plain example rather than Criterion, to
      avoid a dependency for numbers that are just wall-clock ms
- [ ] Confirm in the running app that HDR Toning at radius 300 feels faster
      *(built and ready; not yet exercised through the UI)*

---

## Phase 2 — Compositing ⚠️ implemented, correct, and **not enabled**

The shader is written and matches the CPU everywhere. It is also **slower**,
so `RenderBackend::composite` still returns the CPU result. This is the
phase's real finding and it changes the plan — see below.

- [x] `composite.wgsl` — full port of `composite_row` and `blend_over`
- [x] **All 27 blend modes**, separable and non-separable, including
      `set_lum` / `set_sat` / `clip_color` for Hue/Saturation/Color/Luminosity
- [x] `clip_coverage` — including stacked clipping layers, where the base
      search must skip past other clipping layers
- [x] `dissolve_threshold` — reproduces the hash exactly, `bitcast` rather than
      a value conversion so negative coordinates wrap as Rust's `as u32` does
- [x] Layer masks, opacity, fill opacity, layer offsets, solid-colour layers
- [x] Hidden layers uploaded as placeholders (without their pixels) so the
      clipping-base search still finds the right layer
- [x] Parity test per blend mode, plus masks, offsets, clipping groups, and
      partial opacity
- [x] Dissolve parity asserted **exactly**, not within a tolerance — any drift
      means a different random pattern
- [x] Adjustment layers detected and sent to the CPU (`pack_stack` returns
      `None`); they need the `Adjustment` enum in WGSL, which is Phase 3
- [x] Benchmark: 1, 5, 10, 25 layers
- [ ] **Enable it.** Blocked on Phase 4 — see below.

### Why it is not enabled

Measured at **0.3–0.8×**, i.e. consistently slower than the CPU, with a maximum
per-channel difference of 0. The shader is not the problem; the transfer is.
Every call uploads the whole stack, so a 25-layer 2000×1500 document moves
~300 MB before any blending starts.

This is not a tuning problem and no threshold fixes it — the upload is
proportional to the work, so the ratio does not improve with size. Compositing
becomes profitable only when layer pixels **stay resident** on the GPU between
frames, which is Phase 4.

Shipping a correctness-neutral 2× slowdown would be worse than shipping
nothing, so the GPU path is kept exercised through
`RenderBackend::composite_on_gpu_for_testing` and switched on when residency
lands. **This reorders the plan: Phase 4 is now a prerequisite for Phase 2's
benefit, not an optimisation after it.**

### Also learned

Creating a wgpu device per test crashes the driver — 17 parallel tests each
calling `select()` reliably segfaulted. Tests now use `gpu::shared()`, which is
what the application does anyway. Worth remembering before adding any test that
brings up its own device.

---

## Phase 3 — Adjustments and point filters

Cheap to port once Phase 1's plumbing exists, because they are per-pixel with
no neighbourhood. High value because there are many of them.

- [ ] Extend `RenderBackend` with an adjustment entry point
- [ ] Port the `Adjustment` enum (`filters/adjust.rs`) — all variants are
      per-pixel and map directly to a shader branch
- [ ] Respect the selection mask, which `apply_adjustment` now blends by
      coverage (`document.rs`) — the shader needs the mask as a second texture
- [ ] Port the 14 `apply_*` operations in `document.rs`: HDR Toning, Shadows/
      Highlights, Selective Color, Replace Color, Color Balance, Channel Mixer,
      Gradient Map, and the rest
- [ ] `convolve.rs` remainder: `box_blur`, `sharpen`, `unsharp_mask`,
      `convolve` (generic kernel)
- [ ] Parity test per adjustment
- [ ] **Batch chained adjustments into one pass** where possible — a dialog
      with a live preview currently pays a full round trip per slider move, and
      that round trip is what will dominate once the maths is fast

---

## Phase 4 — Residency: stop round-tripping ⬅️ **do this next**

Promoted ahead of Phase 3. Phase 2 measured the cost of *not* having it: the
compositor is correct and finished but cannot be switched on, because every
call re-uploads the whole stack. Residency is what unlocks it.

Until this phase, every operation uploads and reads back. For a slider drag
that is the whole cost. This is where the GPU stops being a co-processor and
starts being where the image lives.

- [ ] Layer textures live on the GPU; `Pixmap` becomes the CPU-side mirror
- [ ] Dirty-region tracking so only changed tiles are re-uploaded
- [ ] Readback only when actually needed: save, PSD export, clipboard, or a
      CPU-only operation
- [ ] Define the ownership rule crisply — **who is authoritative, the texture
      or the `Pixmap`?** Getting this wrong produces heisenbugs where an
      operation reads stale pixels. Write it down here before coding.
- [ ] Interaction with `history.rs` (undo snapshots) — decide whether history
      stores CPU copies (simple, memory-hungry) or GPU copies (fast, and then
      undo depth is bounded by VRAM)

---

## Phase 5 — GPU presentation

The one place the shell legitimately gains GPU code. Today `CanvasView` is a
plain `QWidget` and presents with `QPainter::drawImage` (`CanvasView.cpp:2566`)
— a CPU blit of the whole composite every repaint.

- [ ] `CanvasView` becomes `QOpenGLWidget` (or Qt RHI), link `Qt::OpenGLWidgets`
- [ ] Sample the composite texture directly instead of blitting a `QImage`
- [ ] GPU-side zoom/pan, so scrolling a large document stops costing a full
      resample
- [ ] **Texture sharing across the CXX-Qt boundary is the hard part.** wgpu and
      Qt must agree on a shared context or an external-memory handle. If that
      proves unworkable, the honest fallback is readback-then-upload, which is
      still no worse than today. Budget real time for this and be willing to
      abandon it.
- [ ] Keep a `QWidget` path for machines on the CPU backend
- [ ] Verify overlays (selection marching ants, guides, transform handles,
      quick-mask veil) still draw correctly over the GPU surface

---

## Phase 6 — Real-time paths

Hardest and least certain, which is why it is last. Brush latency is a
correctness-of-feel problem: a technically faster brush that adds a frame of
latency is worse.

- [ ] Brush dab rendering (`brush.rs`)
- [ ] Mixer brush (`mixer.rs`) — reads the canvas as it paints, so it is the
      most round-trip-sensitive
- [ ] Smudge (`smudge.rs`), healing (`healing.rs`, already rayon-parallel),
      focus/blur tools (`focus.rs`), dodge/burn (`tone.rs`)
- [ ] Bucket fill (`bucket.rs`) and magic wand (`wand.rs`) — flood fill is
      inherently sequential; a GPU port needs a different algorithm
      (jump-flooding or similar), so this may simply stay on the CPU
- [ ] Measure end-to-end input latency, not throughput

---

## Phase 7 — Finish

- [ ] Preferences UI for backend choice (currently env var only)
- [ ] Warn when the GPU was requested and unavailable, rather than only
      recording it in About
- [ ] **Verify on macOS/Metal.** Everything so far is tested on Vulkan only.
      CLAUDE.md §7 names macOS as a first-class target and it has not been
      exercised at all.
- [x] Verify with no usable GPU. `WGPU_BACKEND=dx12` on Linux leaves no
      adapter available, which exercises the real `request_adapter` failure
      path: all 623 tests pass, the backend reports
      `CPU — 16 threads / no usable GPU (...)`, and blur runs at 1.0× the CPU
      time. `PHOTORUST_BACKEND=cpu` is verified separately.
- [ ] Verify under a software rasteriser (lavapipe), which is a different case
      again: an adapter *is* present but slow
- [ ] Update CLAUDE.md §7 to describe what was actually built
- [ ] Record final benchmark numbers here

---

## Open questions

Decide these before the phase that depends on them, not during.

0. **Fix 16/32-bit blur on the CPU.** Found while doing Phase 1 and unrelated
   to the GPU: `blur_pass` and `Pixmap::premultiply` both index as though
   `bpc == 1`, so blurring a 16- or 32-bit pixmap writes into the wrong bytes.
   The GPU path sidesteps it by falling back, which means the bug is now
   *hidden* on 8-bit-only workflows. Worth fixing on its own merits.
1. **Internal working format.** Deferred, not answered: the blur works on
   packed RGBA8 because that is what the reference does. Phase 3's adjustments
   need real precision, so the f32 question returns there.
2. **Undo and VRAM.** If history holds GPU copies, undo depth becomes bounded
   by VRAM. Probably keep history on the CPU and accept the readback.
3. **Colour management.** Nothing here addresses colour spaces. If ICC handling
   arrives later it will want to live in the same shaders, so avoid designs
   that make adding a transform stage awkward.
4. **Is Phase 5 worth it?** If texture sharing across CXX-Qt turns out to be
   genuinely painful, presentation could stay on the CPU indefinitely with most
   of the benefit already collected in Phases 1–4.

---

## Benchmarks

Fill in as phases land. Numbers without a machine description are not useful.

`cargo run --release --example blur_bench`

| Operation | Size | CPU | GPU | Speedup |
|---|---|---|---|---|
| Gaussian blur r=5 | 1000×652 | 43.6 ms | 10.7 ms | 4.1× |
| Gaussian blur r=25 | 1000×652 | 181.9 ms | 28.3 ms | 6.4× |
| Gaussian blur r=100 | 1000×652 | 740.3 ms | 32.7 ms | 22.6× |
| Gaussian blur r=300 | 1000×652 | 2139.2 ms | 56.9 ms | **37.6×** |
| Gaussian blur r=5 | 2000×1500 | 192.1 ms | 36.0 ms | 5.3× |
| Gaussian blur r=25 | 2000×1500 | 849.6 ms | 44.4 ms | 19.2× |
| Gaussian blur r=100 | 2000×1500 | 3340.4 ms | 69.0 ms | 48.4× |
| Gaussian blur r=300 | 2000×1500 | 9916.1 ms | 140.2 ms | **70.7×** |
| Composite, 1 layer | 1280×800 | 7.9 ms | 11.0 ms | 0.7× |
| Composite, 10 layers | 1280×800 | 31.6 ms | 41.9 ms | 0.8× |
| Composite, 25 layers | 1280×800 | 79.3 ms | 124.6 ms | 0.6× |
| Composite, 10 layers | 2000×1500 | 76.3 ms | 152.6 ms | 0.5× |
| Composite, 25 layers | 2000×1500 | 211.1 ms | 326.7 ms | 0.6× |

Compositing is **slower on the GPU** at every size and layer count measured,
with a maximum per-channel difference of 0. `cargo run --release --example
composite_bench`. This is the upload cost, not the shader; see Phase 2.

Maximum per-channel difference from the CPU reference: **1 level** (0 at small
radii). That is float accumulation order, not a behavioural difference.

The 2000×1500 r=300 case is the headline: **9.9 seconds down to 0.14**. That is
the difference between HDR Toning being unusable on a large image and being
interactive.

Crossover measurements (1000×652 unless noted):

| Size | r=1 | r=5 | r=25 |
|---|---|---|---|
| 48×48 | 0.0× | 0.1× | 0.5× |
| 64×64 | 1.7× | 0.4× | 1.3× |
| 96×96 | 1.9× | 4.2× | 2.7× |
| 128×128 | 2.0× | 5.0× | 5.6× |

Development machine: AMD Radeon 780M (RADV PHOENIX), Vulkan, integrated,
16384px max texture. **Integrated** matters: it shares memory with the CPU, so
transfers are cheaper than on a discrete card. The `MIN_GPU_PIXELS` gate is set
conservatively for that reason.

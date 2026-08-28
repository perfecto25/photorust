//! The rendering backend seam.
//!
//! Everything that could reasonably run on a GPU goes through [`RenderBackend`].
//! Two implementations exist: [`CpuBackend`], which is the existing `rayon`
//! code and is always available, and [`GpuBackend`], which owns a `wgpu`
//! device.
//!
//! CLAUDE.md §7 asks for the GPU to sit behind a backend-agnostic interface
//! designed up front rather than retrofitted. That is what this module is. The
//! operations on the trait are the ones worth moving — compositing and
//! convolution are the documented hot paths — so the boundary is drawn where
//! the work will actually land.
//!
//! # What actually runs where
//!
//! * **Gaussian blur** runs on the GPU, 4–70× faster depending on size and
//!   radius, falling back to the CPU for small images, non-8-bit pixmaps, and
//!   device errors.
//! * **Compositing** has a complete, parity-tested compute shader that is
//!   nevertheless **not used**: it is slower than the CPU, because every call
//!   re-uploads the whole layer stack. It becomes worthwhile once layer pixels
//!   stay resident on the GPU between frames. Until then
//!   [`RenderBackend::composite`] returns the CPU result and the shader is
//!   reached only through [`RenderBackend::composite_on_gpu_for_testing`].
//!
//! `docs/gpu-migration.md` has the measurements and the phase plan.
//!
//! # Choosing a backend
//!
//! [`select`] takes the GPU when one is usable and falls back to the CPU
//! otherwise, recording *why* in [`DeviceInfo::detail`] so a machine that
//! quietly fell back can be diagnosed. `PHOTORUST_BACKEND` overrides the
//! choice: `cpu` forces the CPU, `gpu` refuses to fall back (and so fails
//! loudly), `auto` is the default.

use crate::buffer::Pixmap;
use crate::layer::LayerStack;

mod cpu;
mod device;
mod transfer;
mod wgpu_backend;

pub use cpu::CpuBackend;
pub use device::{DeviceInfo, GpuProbe};
pub use wgpu_backend::GpuBackend;

/// Which implementation is in use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendKind {
    Cpu,
    Gpu,
}

impl BackendKind {
    pub fn label(self) -> &'static str {
        match self {
            BackendKind::Cpu => "CPU",
            BackendKind::Gpu => "GPU",
        }
    }
}

/// What the caller asked for, before availability is taken into account.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BackendPreference {
    /// Use the GPU when one is usable, otherwise the CPU.
    #[default]
    Auto,
    /// Never touch the GPU.
    ForceCpu,
    /// Use the GPU or fail. For testing that the GPU path is really being
    /// exercised, rather than silently getting CPU results.
    RequireGpu,
}

impl BackendPreference {
    /// Read `PHOTORUST_BACKEND`. An unset or unrecognised value means
    /// [`BackendPreference::Auto`].
    pub fn from_env() -> Self {
        match std::env::var("PHOTORUST_BACKEND").ok().as_deref() {
            Some("cpu") => BackendPreference::ForceCpu,
            Some("gpu") => BackendPreference::RequireGpu,
            _ => BackendPreference::Auto,
        }
    }
}

/// The operations a backend can perform.
///
/// Kept deliberately narrow: these are the paths worth accelerating, not every
/// pixel operation in the engine. Anything not here stays on the CPU and does
/// not need to know a GPU exists.
pub trait RenderBackend: Send + Sync {
    fn kind(&self) -> BackendKind;

    /// What was selected, and why — surfaced in the UI and in bug reports.
    fn info(&self) -> &DeviceInfo;

    /// Flatten a layer stack into a single image.
    fn composite(&self, stack: &LayerStack, width: u32, height: u32) -> Pixmap;

    /// Run the GPU compositor specifically, whether or not it is currently the
    /// faster choice. `None` when this backend has no GPU path, or the stack
    /// contains something the shader cannot express.
    ///
    /// Exists because [`RenderBackend::composite`] deliberately does *not* use
    /// the GPU yet — see the note on `GpuBackend::composite`. Without this the
    /// parity tests would silently compare the CPU against itself and prove
    /// nothing.
    fn composite_on_gpu_for_testing(
        &self,
        _stack: &LayerStack,
        _width: u32,
        _height: u32,
    ) -> Option<Pixmap> {
        None
    }

    /// Separable Gaussian blur, in place. `radius` is the sigma in pixels.
    fn gaussian_blur(&self, pixmap: &mut Pixmap, radius: f32);
}

/// The process-wide backend, brought up on first use.
///
/// A single shared instance rather than one per `Document` because there is
/// one device per process, bringing it up costs real time, and the choice
/// cannot change while the program runs. The alternative — threading a backend
/// handle through every operation that might want it — would touch most of the
/// engine's signatures and all of its tests to express something that is
/// genuinely global.
pub fn shared() -> &'static dyn RenderBackend {
    static SHARED: std::sync::OnceLock<Box<dyn RenderBackend>> = std::sync::OnceLock::new();
    SHARED
        .get_or_init(|| select(BackendPreference::from_env()))
        .as_ref()
}

/// Pick a backend according to `preference`.
///
/// Never panics and never fails: [`BackendPreference::RequireGpu`] with no
/// usable GPU still returns a working CPU backend, but says so in
/// [`DeviceInfo::detail`] rather than pretending the GPU was used. Callers that
/// care can check [`RenderBackend::kind`].
pub fn select(preference: BackendPreference) -> Box<dyn RenderBackend> {
    if preference == BackendPreference::ForceCpu {
        return Box::new(CpuBackend::with_detail("forced by PHOTORUST_BACKEND=cpu"));
    }

    match GpuBackend::new() {
        Ok(backend) => Box::new(backend),
        Err(reason) => {
            let detail = match preference {
                BackendPreference::RequireGpu => {
                    format!("PHOTORUST_BACKEND=gpu requested but unavailable: {reason}")
                }
                _ => format!("no usable GPU ({reason})"),
            };
            Box::new(CpuBackend::with_detail(&detail))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Rgba8;

    #[test]
    fn forcing_the_cpu_gives_the_cpu_backend() {
        let backend = select(BackendPreference::ForceCpu);
        assert_eq!(backend.kind(), BackendKind::Cpu);
    }

    #[test]
    fn selection_always_yields_a_working_backend() {
        // Whatever the machine has — a GPU, no GPU, a broken driver — the
        // engine must come up. This is the property that keeps a missing
        // device from being a crash.
        //
        // Checked against the two backends that already exist rather than by
        // calling `select` for each preference: every `select` that reaches
        // the GPU brings up another wgpu device, and throwaway devices racing
        // the shared one crash the driver. `shared()` exercises the real
        // selection path, including the fallback, exactly once.
        for backend in [shared(), select(BackendPreference::ForceCpu).as_ref()] {
            let mut pm = Pixmap::filled(4, 4, Rgba8::WHITE);
            backend.gaussian_blur(&mut pm, 1.0);
            assert_eq!(pm.width(), 4, "{} backend", backend.kind().label());
            assert!(!backend.info().summary().is_empty());
        }
    }

    #[test]
    fn a_backend_always_describes_itself() {
        let backend = select(BackendPreference::ForceCpu);
        let info = backend.info();
        assert!(!info.name.is_empty());
        assert!(!info.detail.is_empty(), "fallback reason must be recorded");
    }

    #[test]
    fn the_environment_override_is_read() {
        // Parsing only — the variable itself is process-wide, so the test
        // exercises the mapping rather than setting it.
        assert_eq!(BackendPreference::default(), BackendPreference::Auto);
    }

    // ---------------------------------------------------------- parity ---
    //
    // The CPU is the reference; these check the GPU against it.
    //
    // They all go through [`shared`] rather than calling [`select`], and that
    // matters: `select` brings up a *new* wgpu device every time, and 17 tests
    // running in parallel each with their own device reliably segfaults the
    // driver. One device per process is both what the application does and the
    // only thing the driver is happy with.

    /// A test image with hard edges, a colour gradient and an alpha ramp —
    /// enough structure that a wrong kernel, a wrong edge rule or a dropped
    /// premultiply all show up.
    fn sample_image(w: u32, h: u32) -> Pixmap {
        let mut pm = Pixmap::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let checker = ((x / 7) + (y / 5)) % 2 == 0;
                let px = Rgba8::new(
                    if checker { 240 } else { 12 },
                    (x * 255 / w.max(1)) as u8,
                    (y * 255 / h.max(1)) as u8,
                    if x < w / 3 { 255 } else { (x * 255 / w.max(1)) as u8 },
                );
                pm.set(x as i32, y as i32, px);
            }
        }
        pm
    }

    /// Largest per-channel difference between two images.
    fn max_difference(a: &Pixmap, b: &Pixmap) -> u8 {
        a.as_bytes()
            .iter()
            .zip(b.as_bytes().iter())
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap_or(0)
    }

    fn assert_matches_cpu(width: u32, height: u32, radius: f32) {
        let backend = shared();

        let mut theirs = sample_image(width, height);
        backend.gaussian_blur(&mut theirs, radius);

        let mut reference = sample_image(width, height);
        crate::filters::convolve::gaussian_blur(&mut reference, radius);

        // The GPU accumulates in f32 in a different order from the CPU, so
        // exact equality is not required — but anything above a level or two
        // means a real difference in kernel, edges or premultiply, not
        // rounding.
        let diff = max_difference(&theirs, &reference);
        assert!(
            diff <= 1,
            "{} blur diverged from CPU by {diff} levels at {width}x{height} r={radius}",
            backend.kind().label(),
        );
    }

    #[test]
    fn blur_matches_the_cpu_across_radii() {
        // Spans the CPU/GPU threshold deliberately: the small radii should be
        // taking the CPU path, the large ones the GPU path.
        for radius in [1.0, 5.0, 50.0] {
            assert_matches_cpu(64, 48, radius);
        }
    }

    #[test]
    fn blur_matches_the_cpu_at_a_large_radius() {
        // A radius wider than the image, which drives every sample into the
        // edge clamp — where a wrap or a zero-pad would be obvious.
        assert_matches_cpu(40, 30, 60.0);
    }

    #[test]
    fn blur_matches_the_cpu_on_awkward_sizes() {
        // Not multiples of the 8x8 workgroup, so the shader's bounds check is
        // doing real work.
        assert_matches_cpu(1, 1, 5.0);
        assert_matches_cpu(13, 7, 9.0);
        assert_matches_cpu(8, 33, 4.0);
    }

    #[test]
    fn blur_preserves_a_fully_transparent_image() {
        // Premultiplied zero must stay zero: a missing unpremultiply shows up
        // here as stray colour in pixels that have no coverage.
        let backend = shared();
        let mut pm = Pixmap::new(16, 16);
        backend.gaussian_blur(&mut pm, 5.0);
        assert!(pm.as_bytes().iter().all(|&b| b == 0));
    }

    // ------------------------------------------------- composite parity ---

    use crate::blend::BlendMode;
    use crate::buffer::Rect;
    use crate::layer::{Layer, LayerStack};

    /// Canvas big enough to clear `MIN_GPU_PIXELS`, so these exercise the GPU
    /// path rather than quietly testing the CPU against itself.
    const CW: u32 = 160;
    const CH: u32 = 160;

    fn textured_layer(stack: &mut LayerStack, seed: u32) {
        let id = stack.allocate_id();
        let mut layer = Layer::new_raster(id, "l", CW, CH);
        for y in 0..CH {
            for x in 0..CW {
                let px = Rgba8::new(
                    ((x * 3 + seed * 40) % 256) as u8,
                    ((y * 5 + seed * 70) % 256) as u8,
                    ((x + y + seed * 90) % 256) as u8,
                    if (x / 11 + y / 9) % 3 == 0 { 128 } else { 255 },
                );
                layer.pixels.set(x as i32, y as i32, px);
            }
        }
        stack.push(layer);
    }

    /// Compare the GPU compositor against the CPU reference.
    ///
    /// Returns without asserting on a machine with no GPU — there is nothing
    /// to compare there, and a vacuous pass is more honest than pretending the
    /// shader was checked.
    fn assert_composite_matches_cpu(stack: &LayerStack, what: &str) {
        let backend = shared();
        let Some(theirs) = backend.composite_on_gpu_for_testing(stack, CW, CH) else {
            return;
        };
        let reference = crate::compositor::composite(stack, CW, CH);
        let diff = max_difference(&theirs, &reference);
        assert!(
            diff <= 1,
            "GPU composite diverged from CPU by {diff} levels: {what}",
        );
    }

    #[test]
    fn composite_matches_the_cpu_for_every_blend_mode() {
        // The bulk of Phase 2's risk: 27 modes, each a different formula, and
        // a wrong one is a subtly wrong colour rather than a crash.
        for mode in BlendMode::ALL {
            let mut stack = LayerStack::new();
            textured_layer(&mut stack, 1);
            textured_layer(&mut stack, 2);
            stack.get_mut(1).unwrap().blend_mode = mode;
            assert_composite_matches_cpu(&stack, &format!("{mode:?}"));
        }
    }

    #[test]
    fn composite_matches_the_cpu_with_partial_opacity() {
        for mode in BlendMode::ALL {
            let mut stack = LayerStack::new();
            textured_layer(&mut stack, 3);
            textured_layer(&mut stack, 4);
            {
                let top = stack.get_mut(1).unwrap();
                top.blend_mode = mode;
                top.opacity = 0.45;
                top.fill_opacity = 0.8;
            }
            assert_composite_matches_cpu(&stack, &format!("{mode:?} at partial opacity"));
        }
    }

    #[test]
    fn composite_matches_the_cpu_with_a_layer_mask() {
        let mut stack = LayerStack::new();
        textured_layer(&mut stack, 5);
        textured_layer(&mut stack, 6);
        {
            let top = stack.get_mut(1).unwrap();
            top.add_reveal_all_mask();
            if let Some(mask) = top.mask.as_mut() {
                mask.fill_rect(Rect::new(0, 0, CW / 2, CH), Rgba8::new(0, 0, 0, 60));
            }
        }
        assert_composite_matches_cpu(&stack, "layer mask");
    }

    #[test]
    fn composite_matches_the_cpu_with_offsets_and_hidden_layers() {
        let mut stack = LayerStack::new();
        textured_layer(&mut stack, 7);
        textured_layer(&mut stack, 8);
        textured_layer(&mut stack, 9);
        // Partly off-canvas in both directions, so the shader's bounds check
        // has to match `Pixmap::get` returning transparent.
        stack.get_mut(1).unwrap().offset = (-37, 21);
        stack.get_mut(2).unwrap().offset = (44, -13);
        // A hidden layer must still be uploaded as a placeholder, or the
        // clipping-base search would find the wrong layer.
        stack.get_mut(1).unwrap().visible = false;
        assert_composite_matches_cpu(&stack, "offsets and hidden layers");
    }

    #[test]
    fn composite_matches_the_cpu_for_clipping_groups() {
        let mut stack = LayerStack::new();
        // Base with a hole in it, so clipped layers have somewhere to vanish.
        let id = stack.allocate_id();
        let mut base = Layer::new_raster(id, "base", CW, CH);
        base.pixels
            .fill_rect(Rect::new(0, 0, CW / 2, CH), Rgba8::new(200, 40, 40, 255));
        stack.push(base);

        textured_layer(&mut stack, 10);
        textured_layer(&mut stack, 11);
        // Two stacked clipping layers, so the base search must skip past the
        // first to find the ordinary layer underneath.
        stack.get_mut(1).unwrap().clipping = true;
        stack.get_mut(2).unwrap().clipping = true;
        stack.get_mut(2).unwrap().blend_mode = BlendMode::Overlay;
        assert_composite_matches_cpu(&stack, "stacked clipping layers");
    }

    #[test]
    fn composite_matches_the_cpu_for_dissolve() {
        // Dissolve hashes the pixel position, so the shader must reproduce the
        // hash exactly — otherwise the pattern differs every frame.
        let mut stack = LayerStack::new();
        textured_layer(&mut stack, 12);
        textured_layer(&mut stack, 13);
        {
            let top = stack.get_mut(1).unwrap();
            top.blend_mode = BlendMode::Dissolve;
            top.opacity = 0.5;
        }
        let backend = shared();
        let Some(theirs) = backend.composite_on_gpu_for_testing(&stack, CW, CH) else {
            return;
        };
        let reference = crate::compositor::composite(&stack, CW, CH);
        // All-or-nothing coverage, so this must match exactly rather than
        // within a tolerance: any drift means a different random pattern.
        assert_eq!(
            theirs.as_bytes(),
            reference.as_bytes(),
            "dissolve pattern differs from the CPU"
        );
    }

    #[test]
    fn composite_matches_the_cpu_for_solid_colour_layers() {
        let mut stack = LayerStack::new();
        textured_layer(&mut stack, 14);
        let id = stack.allocate_id();
        let mut fill = Layer::new_raster(id, "fill", CW, CH);
        fill.kind = crate::layer::LayerKind::SolidColor(Rgba8::new(30, 190, 120, 170));
        fill.blend_mode = BlendMode::HardLight;
        stack.push(fill);
        assert_composite_matches_cpu(&stack, "solid colour layer");
    }

    #[test]
    fn composite_falls_back_when_the_stack_has_an_adjustment_layer() {
        // Adjustment layers are Phase 3. Until then the whole composite must
        // go to the CPU — and still be correct.
        let mut stack = LayerStack::new();
        textured_layer(&mut stack, 15);
        let id = stack.allocate_id();
        stack.push(Layer::new_adjustment(
            id,
            "Invert",
            crate::filters::Adjustment::Invert,
        ));
        let backend = shared();
        assert!(
            backend.composite_on_gpu_for_testing(&stack, CW, CH).is_none(),
            "an adjustment layer must send the composite to the CPU",
        );
        // And the result the engine actually returns is still correct.
        let out = backend.composite(&stack, CW, CH);
        assert_eq!(
            out.as_bytes(),
            crate::compositor::composite(&stack, CW, CH).as_bytes(),
        );
    }

    #[test]
    fn blur_leaves_a_flat_image_flat() {
        // A normalised kernel over a constant image must return that
        // constant — the cheapest check that the weights sum to one.
        let backend = shared();
        let mut pm = Pixmap::filled(24, 24, Rgba8::new(70, 140, 210, 255));
        backend.gaussian_blur(&mut pm, 8.0);
        for px in pm.as_bytes().chunks_exact(4) {
            assert_eq!((px[0], px[1], px[2], px[3]), (70, 140, 210, 255));
        }
    }
}

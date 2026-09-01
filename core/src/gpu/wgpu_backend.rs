//! The GPU backend.
//!
//! # State of this file
//!
//! * `gaussian_blur` runs on the GPU as two compute passes.
//! * `composite` has a full compute shader that is correct but slower than the
//!   CPU, so it is not used yet — see the note on the method itself.
//!
//! Where an operation cannot be done on the GPU (wrong bit depth, image too
//! large for a storage binding, or a radius so small the transfer costs more
//! than the blur saves) it falls back to the CPU rather than refusing. The
//! result is identical either way; only the time taken differs.

use super::transfer;
use super::{BackendKind, DeviceInfo, GpuProbe, RenderBackend};
use crate::buffer::Pixmap;
use crate::layer::{LayerKind, LayerStack};

/// Below this many pixels the CPU wins, whatever the radius.
///
/// The GPU pays a fixed cost per operation — buffer creation, two dispatches,
/// and a readback that stalls until the queue drains — and on a small image
/// that fixed cost is the whole cost. Measured on the development machine:
/// 48×48 (2304px) loses at every radius, 96×96 (9216px) wins at every radius,
/// 64×64 is noise. 128×128 is chosen as the gate: comfortably past the
/// crossover, and deliberately conservative because a discrete GPU pays more
/// to move pixels across PCIe than this integrated one does.
///
/// Note it is *not* a radius threshold. Once the image is big enough the GPU
/// wins even at sigma 0.5, so gating on radius would have turned away work it
/// handles perfectly well. See `docs/gpu-migration.md` for the numbers.
const MIN_GPU_PIXELS: u64 = 128 * 128;

/// Uniform block for `blur.wgsl`. Four 4-byte fields, so no padding is needed
/// to satisfy the 16-byte uniform alignment.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurParams {
    width: u32,
    height: u32,
    taps: i32,
    horizontal: u32,
}

/// Uniform block for `composite.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CompositeParams {
    width: u32,
    height: u32,
    layer_count: u32,
    _pad: u32,
}

const KIND_RASTER: u32 = 0;
const KIND_SOLID: u32 = 1;
const NO_MASK: u32 = 0xffff_ffff;

/// Per-layer description handed to `composite.wgsl`. Field order and size must
/// match `LayerMeta` there exactly.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct LayerMeta {
    pixel_offset: u32,
    width: u32,
    height: u32,
    off_x: i32,
    off_y: i32,
    mask_offset: u32,
    mask_width: u32,
    mask_height: u32,
    alpha: f32,
    blend_mode: u32,
    kind: u32,
    clipping: u32,
    solid: u32,
    visible: u32,
    _pad0: u32,
    _pad1: u32,
}

/// Everything `composite.wgsl` needs, flattened into three arrays.
///
/// One buffer per layer would be simpler to build but runs into
/// `max_storage_buffers_per_shader_stage` after a handful of layers, so the
/// pixels are concatenated and each layer records where its own start.
struct PackedStack {
    pixels: Vec<u32>,
    masks: Vec<u32>,
    metas: Vec<LayerMeta>,
}

/// Flatten a stack for the shader, or `None` if it contains something the
/// shader cannot do.
///
/// The only such case today is an adjustment layer: those recolour the
/// accumulated backdrop and would need the whole `Adjustment` enum ported into
/// WGSL, which is Phase 3. Returning `None` sends the whole composite to the
/// CPU, which is correct — just slower.
fn pack_stack(stack: &LayerStack) -> Option<PackedStack> {
    let layers = stack.as_slice();
    let mut packed = PackedStack {
        pixels: Vec::new(),
        masks: Vec::new(),
        metas: Vec::with_capacity(layers.len()),
    };

    for layer in layers {
        let (kind, solid) = match &layer.kind {
            LayerKind::Raster => (KIND_RASTER, 0u32),
            LayerKind::SolidColor(c) => (
                KIND_SOLID,
                u32::from(c.r) | (u32::from(c.g) << 8) | (u32::from(c.b) << 16) | (u32::from(c.a) << 24),
            ),
            // An adjustment recolours the backdrop, and the two evaluated
            // fills need per-pixel work the shader has no description of.
            // Either sends the whole stack back to the CPU, which is the
            // fallback this returns.
            // A group is a second accumulator: its members composite into a
            // buffer of their own before that buffer is blended. The shader
            // has one backdrop, so this goes back to the CPU too.
            LayerKind::Adjustment(_)
            | LayerKind::Gradient(_)
            | LayerKind::Pattern(_)
            | LayerKind::Group => return None,
        };

        let visible = !layer.is_invisible();

        // An invisible layer is never sampled — the main loop skips it and
        // `clip_coverage` returns before reading — so its pixels are not
        // uploaded. A hidden 4K layer should not cost bandwidth.
        let (pixel_offset, width, height) = if visible && kind == KIND_RASTER {
            let offset = packed.pixels.len() as u32;
            let (w, h) = (layer.pixels.width(), layer.pixels.height());
            packed
                .pixels
                .extend(layer.pixels.as_bytes().chunks_exact(4).map(|px| {
                    u32::from(px[0])
                        | (u32::from(px[1]) << 8)
                        | (u32::from(px[2]) << 16)
                        | (u32::from(px[3]) << 24)
                }));
            (offset, w, h)
        } else {
            (0, layer.pixels.width(), layer.pixels.height())
        };

        // Only the coverage byte is needed, so masks upload one value per
        // pixel rather than a full RGBA quad.
        let (mask_offset, mask_width, mask_height) = match &layer.mask {
            Some(mask) if layer.mask_enabled && visible => {
                let offset = packed.masks.len() as u32;
                packed
                    .masks
                    .extend(mask.as_bytes().chunks_exact(4).map(|px| u32::from(px[3])));
                (offset, mask.width(), mask.height())
            }
            _ => (NO_MASK, 0, 0),
        };

        packed.metas.push(LayerMeta {
            pixel_offset,
            width,
            height,
            off_x: layer.offset.0,
            off_y: layer.offset.1,
            mask_offset,
            mask_width,
            mask_height,
            alpha: layer.effective_alpha(),
            blend_mode: layer.blend_mode as u32,
            kind,
            clipping: u32::from(layer.clipping),
            solid,
            visible: u32::from(visible),
            _pad0: 0,
            _pad1: 0,
        });
    }

    Some(packed)
}

pub struct GpuBackend {
    device: wgpu::Device,
    queue: wgpu::Queue,
    info: DeviceInfo,
    blur_pipeline: wgpu::ComputePipeline,
    blur_layout: wgpu::BindGroupLayout,
    composite_pipeline: wgpu::ComputePipeline,
    composite_layout: wgpu::BindGroupLayout,
    /// Largest storage buffer the device will bind, in bytes. An image needing
    /// more than this goes to the CPU.
    max_binding: u64,
}

impl GpuBackend {
    /// Bring up a device and build the pipelines, or report why not.
    pub fn new() -> Result<GpuBackend, String> {
        let GpuProbe {
            device,
            queue,
            info,
            max_storage_binding,
        } = GpuProbe::new()?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gaussian blur"),
            source: wgpu::ShaderSource::Wgsl(include_str!("blur.wgsl").into()),
        });

        let storage = |read_only: bool| wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let entry = |binding: u32, ty: wgpu::BindingType| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty,
            count: None,
        };

        let blur_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur bindings"),
            entries: &[
                entry(0, storage(true)),
                entry(1, storage(false)),
                entry(2, storage(true)),
                entry(
                    3,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blur layout"),
            bind_group_layouts: &[Some(&blur_layout)],
            ..Default::default()
        });

        let blur_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("blur"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("composite"),
            source: wgpu::ShaderSource::Wgsl(include_str!("composite.wgsl").into()),
        });

        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composite bindings"),
            entries: &[
                entry(0, storage(true)),  // pixels
                entry(1, storage(true)),  // masks
                entry(2, storage(true)),  // layer metadata
                entry(3, storage(false)), // output
                entry(
                    4,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
            ],
        });

        let composite_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("composite layout"),
                bind_group_layouts: &[Some(&composite_layout)],
                ..Default::default()
            });

        let composite_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("composite"),
            layout: Some(&composite_pipeline_layout),
            module: &composite_shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        Ok(GpuBackend {
            device,
            queue,
            info,
            blur_pipeline,
            blur_layout,
            composite_pipeline,
            composite_layout,
            max_binding: max_storage_binding,
        })
    }

    /// Composite a packed stack. Errors are the caller's cue to fall back.
    fn composite_on_gpu(
        &self,
        packed: &PackedStack,
        width: u32,
        height: u32,
    ) -> Result<Pixmap, String> {
        use wgpu::util::DeviceExt;

        let out_size = (width as u64) * (height as u64) * 4;
        if out_size > self.max_binding {
            return Err("canvas exceeds the storage binding limit".into());
        }

        // wgpu rejects zero-sized bindings, and a stack can legitimately have
        // no pixels (all solid-colour layers) or no masks.
        let pixel_data: &[u32] = if packed.pixels.is_empty() { &[0] } else { &packed.pixels };
        let mask_data: &[u32] = if packed.masks.is_empty() { &[0] } else { &packed.masks };
        if (pixel_data.len() as u64) * 4 > self.max_binding {
            return Err("layer pixels exceed the storage binding limit".into());
        }

        let pixels = transfer::upload(&self.device, "layer pixels", bytemuck::cast_slice(pixel_data));
        let masks = transfer::upload(&self.device, "layer masks", bytemuck::cast_slice(mask_data));
        let metas = transfer::upload(&self.device, "layer meta", bytemuck::cast_slice(&packed.metas));
        let out = transfer::scratch(&self.device, "composite out", out_size);

        let params = CompositeParams {
            width,
            height,
            layer_count: packed.metas.len() as u32,
            _pad: 0,
        };
        let params_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("composite params"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composite bind group"),
            layout: &self.composite_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: pixels.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: masks.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: metas.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: out.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: params_buf.as_entire_binding() },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("composite") });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("composite"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.composite_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
        }
        self.queue.submit(Some(encoder.finish()));

        let bytes = transfer::readback(&self.device, &self.queue, &out, out_size)?;
        Pixmap::from_raw(width, height, bytes)
            .ok_or_else(|| "readback did not match canvas size".into())
    }

    /// Whether an RGBA8 image of this size fits in one storage binding.
    pub fn fits_in_buffer(&self, width: u32, height: u32) -> bool {
        let bytes = width as u64 * height as u64 * 4;
        bytes > 0 && bytes <= self.max_binding
    }

    /// Is the GPU path worth taking for this image?
    fn blur_is_worthwhile(&self, pixmap: &Pixmap) -> bool {
        let pixels = pixmap.width() as u64 * pixmap.height() as u64;
        // The CPU blur only handles 8-bit correctly — `blur_pass` writes at
        // `x * 4` while the row stride is `width * 4 * bpc` — so deeper
        // pixmaps stay on the reference path rather than being given new and
        // differently-wrong behaviour here.
        pixmap.bpc() == 1
            && pixels >= MIN_GPU_PIXELS
            && self.fits_in_buffer(pixmap.width(), pixmap.height())
    }

    /// The two separable passes. Errors are the caller's cue to fall back.
    fn blur_on_gpu(&self, pixmap: &mut Pixmap, radius: f32) -> Result<(), String> {
        let width = pixmap.width();
        let height = pixmap.height();
        let sigma = radius.max(0.01);
        // Identical to the CPU: three sigma of taps, normalised.
        let taps = (sigma * 3.0).ceil() as i32;
        let weights = crate::filters::convolve::gaussian_kernel_1d(sigma, taps);

        let size = (width as u64) * (height as u64) * 4;

        // Premultiply on the CPU, where it is integer-exact and matches the
        // reference bit for bit. It is O(n) and not the bottleneck.
        pixmap.premultiply();

        let src = transfer::upload(&self.device, "blur src", pixmap.as_bytes());
        let mid = transfer::scratch(&self.device, "blur mid", size);
        let dst = transfer::scratch(&self.device, "blur dst", size);
        let weight_buf = transfer::upload(
            &self.device,
            "blur weights",
            bytemuck::cast_slice(&weights),
        );

        // Horizontal into `mid`, then vertical into `dst`.
        self.dispatch_blur(&src, &mid, &weight_buf, width, height, taps, true);
        self.dispatch_blur(&mid, &dst, &weight_buf, width, height, taps, false);

        let out = transfer::readback(&self.device, &self.queue, &dst, size)?;
        if out.len() != pixmap.as_bytes().len() {
            return Err(format!(
                "readback size mismatch: got {}, expected {}",
                out.len(),
                pixmap.as_bytes().len()
            ));
        }
        pixmap.as_bytes_mut().copy_from_slice(&out);

        pixmap.unpremultiply();
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch_blur(
        &self,
        src: &wgpu::Buffer,
        dst: &wgpu::Buffer,
        weights: &wgpu::Buffer,
        width: u32,
        height: u32,
        taps: i32,
        horizontal: bool,
    ) {
        use wgpu::util::DeviceExt;

        let params = BlurParams {
            width,
            height,
            taps,
            horizontal: u32::from(horizontal),
        };
        let params_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("blur params"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blur bind group"),
            layout: &self.blur_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: src.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: dst.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: weights.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: params_buf.as_entire_binding() },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("blur pass") });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("blur"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.blur_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            // Workgroup is 8x8; round up so edge pixels are covered. The
            // shader bounds-checks, so overshoot is harmless.
            pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
        }
        self.queue.submit(Some(encoder.finish()));
    }
}

impl RenderBackend for GpuBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Gpu
    }

    fn info(&self) -> &DeviceInfo {
        &self.info
    }

    /// Deliberately still the CPU. See below.
    ///
    /// The compute shader is written, matches the CPU on all 27 blend modes,
    /// and is covered by the parity tests — but it is **slower**, measured at
    /// 0.3–0.8× on the development machine. The reason is structural rather
    /// than a shader problem: every call uploads the whole stack, so a 25-layer
    /// 2000×1500 document moves ~300 MB before any blending happens, and the
    /// transfer dwarfs the work.
    ///
    /// Compositing only pays off once layer pixels stay resident on the GPU
    /// between frames, which is Phase 4. Enabling it before then would trade
    /// correctness-neutral slowness for nothing, so this stays on the CPU and
    /// the GPU path is kept exercised through
    /// [`RenderBackend::composite_on_gpu_for_testing`].
    fn composite(&self, stack: &LayerStack, width: u32, height: u32) -> Pixmap {
        crate::compositor::composite(stack, width, height)
    }

    fn composite_on_gpu_for_testing(
        &self,
        stack: &LayerStack,
        width: u32,
        height: u32,
    ) -> Option<Pixmap> {
        if stack.is_empty() || (width as u64 * height as u64) < MIN_GPU_PIXELS {
            return None;
        }
        // An adjustment layer means the shader cannot express the stack.
        let packed = pack_stack(stack)?;
        match self.composite_on_gpu(&packed, width, height) {
            Ok(out) => Some(out),
            Err(reason) => {
                log_gpu_fallback("composite", &reason);
                None
            }
        }
    }

    fn gaussian_blur(&self, pixmap: &mut Pixmap, radius: f32) {
        if radius <= 0.0 || pixmap.is_empty() {
            return;
        }
        if !self.blur_is_worthwhile(pixmap) {
            crate::filters::convolve::gaussian_blur(pixmap, radius);
            return;
        }
        if let Err(reason) = self.blur_on_gpu(pixmap, radius) {
            // A device that failed mid-operation must not cost the user their
            // edit. The CPU path is always available and always correct.
            log_gpu_fallback("blur", &reason);
            crate::filters::convolve::gaussian_blur(pixmap, radius);
        }
    }
}

fn log_gpu_fallback(op: &str, reason: &str) {
    eprintln!("photorust: GPU {op} failed, using CPU instead ({reason})");
}

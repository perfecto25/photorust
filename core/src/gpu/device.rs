//! Adapter discovery and capability probing.

use super::BackendKind;

/// What backend is in use and what it is running on.
///
/// Held by every backend and surfaced through the bridge, so "is this machine
/// actually using the GPU?" is answerable without a debugger.
#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub kind: BackendKind,
    /// The adapter name for a GPU, or the CPU description.
    pub name: String,
    /// The graphics API in use — "Vulkan", "Metal", and so on. Empty on CPU.
    pub api: String,
    /// Why this backend was chosen. For the CPU backend after a failed GPU
    /// probe this carries the reason, which is the thing worth having when a
    /// machine unexpectedly runs slowly.
    pub detail: String,
    /// Largest 2D texture the device will accept, in pixels.
    ///
    /// A real limit, not a formality: a document wider than this cannot be
    /// handed to the GPU as a single texture, so a migrated operation has to
    /// tile or defer to the CPU. `u32::MAX` on the CPU backend.
    pub max_texture_dimension: u32,
}

impl DeviceInfo {
    /// A one-line summary for the UI: "GPU — NVIDIA GeForce RTX 3080 (Vulkan)".
    pub fn summary(&self) -> String {
        if self.api.is_empty() {
            format!("{} — {}", self.kind.label(), self.name)
        } else {
            format!("{} — {} ({})", self.kind.label(), self.name, self.api)
        }
    }
}

/// A `wgpu` device that came up successfully, with the facts about it.
pub struct GpuProbe {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub info: DeviceInfo,
    /// Largest storage buffer the device will bind, in bytes.
    ///
    /// This, rather than the texture dimension, is what actually bounds the
    /// compute paths: they pass images as flat buffers.
    pub max_storage_binding: u64,
}

impl GpuProbe {
    /// Bring up a `wgpu` device, or explain why not.
    ///
    /// Runs synchronously via `pollster`: this happens once, at startup, and
    /// an async runtime would otherwise have to be threaded through an engine
    /// that is deliberately synchronous.
    pub fn new() -> Result<GpuProbe, String> {
        // No display handle: the engine renders to textures and hands the
        // result to Qt, so it never needs to present to a window.
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        // Let wgpu pick within the primary set — Vulkan on Linux, Metal on
        // macOS. Naming one here would re-hardcode the platform split this
        // abstraction exists to avoid.
        descriptor.backends = wgpu::Backends::PRIMARY;
        // Applied last so `WGPU_BACKEND` can override the default — which is
        // how a specific API, or none at all, can be forced to reproduce a
        // driver problem or exercise the CPU fallback on a machine that does
        // have a GPU.
        let instance = wgpu::Instance::new(descriptor.with_env());

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|e| format!("no graphics adapter: {e}"))?;

        let adapter_info = adapter.get_info();
        let limits = adapter.limits();

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("photorust"),
            // Ask for exactly what the adapter offers rather than the
            // conservative defaults, so large documents are not rejected by a
            // limit the hardware does not actually have.
            required_limits: limits.clone(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .map_err(|e| format!("device request failed: {e}"))?;

        Ok(GpuProbe {
            device,
            queue,
            max_storage_binding: limits.max_storage_buffer_binding_size as u64,
            info: DeviceInfo {
                kind: BackendKind::Gpu,
                name: adapter_info.name.clone(),
                api: format!("{:?}", adapter_info.backend),
                detail: format!("{:?} adapter", adapter_info.device_type),
                max_texture_dimension: limits.max_texture_dimension_2d,
            },
        })
    }
}

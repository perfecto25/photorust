//! The CPU backend: the engine's existing `rayon` code behind the trait.
//!
//! Always available, and the reference implementation — the GPU path is
//! correct exactly insofar as it agrees with this one.

use super::{BackendKind, DeviceInfo, RenderBackend};
use crate::buffer::Pixmap;
use crate::layer::LayerStack;

pub struct CpuBackend {
    info: DeviceInfo,
}

impl CpuBackend {
    /// `detail` records why the CPU is being used — a forced preference, or
    /// the reason the GPU probe failed.
    pub fn with_detail(detail: &str) -> Self {
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        CpuBackend {
            info: DeviceInfo {
                kind: BackendKind::Cpu,
                name: format!("{threads} threads"),
                api: String::new(),
                detail: detail.to_string(),
                // No texture size limit worth speaking of; a CPU buffer is
                // bounded by memory, not by the API.
                max_texture_dimension: u32::MAX,
            },
        }
    }
}

impl Default for CpuBackend {
    fn default() -> Self {
        CpuBackend::with_detail("selected directly")
    }
}

impl RenderBackend for CpuBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Cpu
    }

    fn info(&self) -> &DeviceInfo {
        &self.info
    }

    fn composite(&self, stack: &LayerStack, width: u32, height: u32) -> Pixmap {
        crate::compositor::composite(stack, width, height)
    }

    fn gaussian_blur(&self, pixmap: &mut Pixmap, radius: f32) {
        crate::filters::convolve::gaussian_blur(pixmap, radius);
    }
}

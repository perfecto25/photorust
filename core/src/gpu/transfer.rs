//! Moving pixels between a [`Pixmap`] and the GPU.
//!
//! # Why buffers rather than textures
//!
//! The obvious mapping for image work is a 2D texture, but storage buffers are
//! a better fit here and avoid two real traps:
//!
//!   * **Row alignment.** `copy_texture_to_buffer` requires `bytes_per_row` to
//!     be a multiple of 256, so every readback of an arbitrary-width image
//!     needs a padded staging buffer and a row-by-row un-padding pass. Buffer
//!     copies have no such rule.
//!   * **Dimension limits.** `max_texture_dimension_2d` is 16384 on the
//!     development machine, so a wide document would need tiling with
//!     kernel-radius overlap. A buffer is bounded by
//!     `max_storage_buffer_binding_size` in *bytes*, which is a far weaker
//!     constraint and a single flat check.
//!
//! The cost is losing hardware filtering and texture cache locality. Neither
//! matters for a separable blur, which reads along one axis and does its own
//! edge clamping.

use wgpu::util::DeviceExt;

/// Upload raw RGBA8 bytes as a storage buffer.
pub fn upload(device: &wgpu::Device, label: &str, bytes: &[u8]) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    })
}

/// An empty storage buffer of `size` bytes, ready to be written by a shader.
pub fn scratch(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    })
}

/// Copy a storage buffer back to the host.
///
/// Blocks until the GPU is done. The engine is synchronous by design, and an
/// operation the user is waiting on has nothing useful to do in the meantime.
pub fn readback(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &wgpu::Buffer,
    size: u64,
) -> Result<Vec<u8>, String> {
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("blur readback"),
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("readback") });
    encoder.copy_buffer_to_buffer(source, 0, &staging, 0, size);
    queue.submit(Some(encoder.finish()));

    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        // A send failure means the receiver is gone, which cannot happen while
        // this function is still on the stack.
        let _ = tx.send(result);
    });

    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| format!("device poll failed: {e}"))?;

    match rx.recv() {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(format!("buffer map failed: {e}")),
        Err(e) => return Err(format!("map callback never arrived: {e}")),
    }

    let view = slice
        .get_mapped_range()
        .map_err(|e| format!("mapped range unavailable: {e}"))?;
    let data = view.to_vec();
    drop(view);
    staging.unmap();
    Ok(data)
}

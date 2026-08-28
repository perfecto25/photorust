//! CPU vs GPU compositing, by layer count.
//!
//!     cargo run --release --example composite_bench

use photorust_core::blend::BlendMode;
use photorust_core::buffer::{Pixmap, Rgba8};
use photorust_core::gpu::shared;
use photorust_core::layer::{Layer, LayerStack};
use std::time::Instant;

fn stack_of(n: usize, w: u32, h: u32) -> LayerStack {
    let mut stack = LayerStack::new();
    for i in 0..n {
        let id = stack.allocate_id();
        let mut l = Layer::new_raster(id, "l", w, h);
        for y in 0..h {
            for x in 0..w {
                l.pixels.set(x as i32, y as i32, Rgba8::new(
                    ((x * 3 + i as u32 * 40) % 256) as u8,
                    ((y * 5 + i as u32 * 70) % 256) as u8,
                    ((x + y + i as u32 * 90) % 256) as u8,
                    if i == 0 { 255 } else { 190 },
                ));
            }
        }
        // A mix of modes, so the shader's switch is exercised rather than
        // running the cheapest branch every time.
        l.blend_mode = match i % 4 {
            0 => BlendMode::Normal,
            1 => BlendMode::Multiply,
            2 => BlendMode::Overlay,
            _ => BlendMode::SoftLight,
        };
        stack.push(l);
    }
    stack
}

fn time<F: FnMut() -> Pixmap>(mut f: F) -> (f64, Pixmap) {
    let warm = f();
    let start = Instant::now();
    let out = f();
    (start.elapsed().as_secs_f64() * 1000.0, { drop(warm); out })
}

fn main() {
    let backend = shared();
    println!("backend: {}\n", backend.info().summary());

    for (w, h) in [(1280u32, 800u32), (2000, 1500)] {
        println!("{w}x{h}");
        println!("  {:>7}  {:>10}  {:>10}  {:>8}", "layers", "CPU ms", "GPU ms", "speedup");
        for n in [1usize, 5, 10, 25] {
            let stack = stack_of(n, w, h);
            let (cpu, a) = time(|| photorust_core::compositor::composite(&stack, w, h));
            // The GPU path explicitly: `composite` deliberately still returns
            // the CPU result, because this benchmark is what proved it slower.
            let (gpu, b) = time(|| {
                backend
                    .composite_on_gpu_for_testing(&stack, w, h)
                    .expect("no GPU compositor on this machine")
            });
            let diff = a.as_bytes().iter().zip(b.as_bytes()).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
            println!("  {n:>7}  {cpu:>10.1}  {gpu:>10.1}  {:>7.1}x   (max diff {diff})", cpu / gpu.max(0.0001));
        }
        println!();
    }
}

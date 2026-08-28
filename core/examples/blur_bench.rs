//! CPU vs GPU Gaussian blur, at the sizes and radii the app actually uses.
//!
//! Deliberately a plain example rather than a Criterion benchmark: the numbers
//! that matter here are wall-clock seconds on one machine, and a statistical
//! harness would add a dependency and a minute of build time to tell us the
//! same thing.
//!
//!     cargo run --release --example blur_bench

use photorust_core::buffer::{Pixmap, Rgba8};
use photorust_core::gpu::shared;
use std::time::Instant;

fn sample(w: u32, h: u32) -> Pixmap {
    let mut pm = Pixmap::new(w, h);
    for y in 0..h {
        for x in 0..w {
            pm.set(
                x as i32,
                y as i32,
                Rgba8::new(
                    ((x * 7 + y * 3) % 256) as u8,
                    (x * 255 / w.max(1)) as u8,
                    (y * 255 / h.max(1)) as u8,
                    255,
                ),
            );
        }
    }
    pm
}

fn time<F: FnMut()>(mut f: F) -> f64 {
    // One warm-up, so pipeline creation and first-touch allocation do not land
    // in the measurement.
    f();
    let start = Instant::now();
    f();
    start.elapsed().as_secs_f64() * 1000.0
}

fn main() {
    // `shared()`, not `select(Auto)`, so PHOTORUST_BACKEND is honoured here
    // exactly as it is in the application.
    let backend = shared();
    println!("backend: {}", backend.info().summary());
    println!("detail : {}\n", backend.info().detail);

    // 1000x652 is the horse photo used throughout the HDR Toning work.
    for (w, h) in [(1000u32, 652u32), (2000, 1500)] {
        println!("{w}x{h}");
        println!("  {:>8}  {:>10}  {:>10}  {:>8}", "radius", "CPU ms", "backend ms", "speedup");
        for radius in [5.0f32, 25.0, 100.0, 300.0] {
            let mut cpu_image = sample(w, h);
            let cpu = time(|| {
                let mut scratch = cpu_image.clone();
                photorust_core::filters::convolve::gaussian_blur(&mut scratch, radius);
                cpu_image = scratch;
            });

            let mut gpu_image = sample(w, h);
            let gpu = time(|| {
                let mut scratch = gpu_image.clone();
                backend.gaussian_blur(&mut scratch, radius);
                gpu_image = scratch;
            });

            let diff = cpu_image
                .as_bytes()
                .iter()
                .zip(gpu_image.as_bytes().iter())
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap_or(0);

            println!(
                "  {radius:>8.0}  {cpu:>10.1}  {gpu:>10.1}  {:>7.1}x   (max diff {diff})",
                cpu / gpu.max(0.0001)
            );
        }
        println!();
    }
}

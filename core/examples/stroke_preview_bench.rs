//! What one mouse-move event costs during a brush stroke.
//!
//! Compares the two live-preview paths: re-compositing the whole document
//! (`preview_stroke`) against compositing only the dabs just placed
//! (`preview_stroke_patch`). The gap is what decides how many pointer samples
//! the brush actually gets, and so whether a fast stroke comes out curved or
//! as a polygon.

use photorust_core::brush::Brush;
use photorust_core::buffer::Rgba8;
use photorust_core::document::Document;
use std::time::Instant;

/// A stroke's worth of pointer positions, as a hand scribbling would give.
fn path(n: usize, w: u32, h: u32) -> Vec<(f32, f32)> {
    (0..n)
        .map(|i| {
            let t = i as f32 / n as f32;
            (
                w as f32 * (0.1 + 0.8 * t),
                h as f32 * (0.5 + 0.35 * (t * 9.0).sin()),
            )
        })
        .collect()
}

fn bench(w: u32, h: u32, layers: usize) {
    let moves = 60;
    let points = path(moves, w, h);
    let brush = Brush { size: 9.0, ..Brush::default() };
    let color = Rgba8::new(0, 0, 0, 255);

    let mut setup = || {
        let mut d = Document::new(w, h, Rgba8::new(255, 255, 255, 255));
        for _ in 1..layers {
            d.add_layer(None);
        }
        d.commit("Setup");
        d.begin_stroke(&brush, points[0].0, points[0].1, 1.0);
        d
    };

    let mut whole = setup();
    let t = Instant::now();
    for &(x, y) in &points[1..] {
        whole.extend_stroke(&brush, x, y, 1.0);
        std::hint::black_box(whole.preview_stroke(color, 1.0));
    }
    let full = t.elapsed().as_secs_f64() * 1000.0 / (moves - 1) as f64;

    let mut patched = setup();
    let t = Instant::now();
    for &(x, y) in &points[1..] {
        patched.extend_stroke(&brush, x, y, 1.0);
        std::hint::black_box(patched.preview_stroke_patch(color, 1.0));
    }
    let patch = t.elapsed().as_secs_f64() * 1000.0 / (moves - 1) as f64;

    println!(
        "{w}x{h} ({:.1} Mpx), {layers} layer(s):\n  \
         whole document {full:7.2} ms/move -> {:5.0} samples/s\n  \
         patch          {patch:7.2} ms/move -> {:5.0} samples/s   ({:.0}x faster)",
        (w * h) as f64 / 1.0e6,
        1000.0 / full,
        1000.0 / patch,
        full / patch,
    );
}

fn main() {
    bench(1280, 800, 1);
    bench(2816, 2112, 1);
    bench(2816, 2112, 4);
}

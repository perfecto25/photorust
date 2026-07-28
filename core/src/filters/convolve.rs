//! Neighbourhood filters: convolution kernels and separable blurs.
//!
//! Convolution runs on **premultiplied** colour. Blurring straight-alpha RGB
//! lets the colour of fully transparent pixels bleed into visible ones, which
//! shows up as dark halos around soft edges.

use crate::buffer::Pixmap;
use rayon::prelude::*;

/// A square convolution kernel.
#[derive(Clone, Debug)]
pub struct Kernel {
    /// Side length; always odd so there is a well-defined centre.
    pub size: usize,
    pub weights: Vec<f32>,
    /// Sum of weights, applied as `1/divisor` after accumulation.
    pub divisor: f32,
    pub bias: f32,
}

impl Kernel {
    /// Build a kernel, normalising by the weight sum. Panics unless `size` is
    /// odd and `weights.len() == size * size`.
    pub fn new(size: usize, weights: Vec<f32>) -> Self {
        assert!(size % 2 == 1, "kernel size must be odd, got {}", size);
        assert_eq!(
            weights.len(),
            size * size,
            "expected {} weights for a {}x{} kernel",
            size * size,
            size,
            size
        );
        let sum: f32 = weights.iter().sum();
        // A zero-sum kernel (edge detect) must not be scaled to infinity.
        let divisor = if sum.abs() < 1e-6 { 1.0 } else { sum };
        Self {
            size,
            weights,
            divisor,
            bias: 0.0,
        }
    }

    pub fn radius(&self) -> i32 {
        (self.size / 2) as i32
    }

    /// The classic 3x3 sharpen kernel from the Filter ▸ Sharpen menu.
    pub fn sharpen() -> Self {
        Self::new(
            3,
            vec![0.0, -1.0, 0.0, -1.0, 5.0, -1.0, 0.0, -1.0, 0.0],
        )
    }

    pub fn edge_detect() -> Self {
        Self::new(
            3,
            vec![-1.0, -1.0, -1.0, -1.0, 8.0, -1.0, -1.0, -1.0, -1.0],
        )
    }

    pub fn emboss() -> Self {
        let mut k = Self::new(3, vec![-2.0, -1.0, 0.0, -1.0, 1.0, 1.0, 0.0, 1.0, 2.0]);
        // Emboss centres on mid-grey rather than black.
        k.bias = 0.5;
        k
    }
}

/// Apply a kernel to `pixmap` in place.
///
/// Edges use clamp-to-edge sampling, which avoids the dark border that
/// zero-padding would produce.
pub fn convolve(pixmap: &mut Pixmap, kernel: &Kernel) {
    if pixmap.is_empty() {
        return;
    }
    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;
    let radius = kernel.radius();

    let mut src = pixmap.clone();
    src.premultiply();

    let src_ref = &src;
    let stride = pixmap.stride();

    // Rows are independent, so each worker owns one output row.
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, out_row)| {
            let y = y as i32;
            for x in 0..width {
                let mut acc = [0.0f32; 4];
                for ky in -radius..=radius {
                    for kx in -radius..=radius {
                        let w = kernel.weights
                            [((ky + radius) as usize) * kernel.size + (kx + radius) as usize];
                        if w == 0.0 {
                            continue;
                        }
                        let sx = (x + kx).clamp(0, width - 1);
                        let sy = (y + ky).clamp(0, height - 1);
                        let p = src_ref.get(sx, sy);
                        acc[0] += p.r as f32 * w;
                        acc[1] += p.g as f32 * w;
                        acc[2] += p.b as f32 * w;
                        acc[3] += p.a as f32 * w;
                    }
                }

                let inv = 1.0 / kernel.divisor;
                let bias = kernel.bias * 255.0;
                let a = (acc[3] * inv + bias).clamp(0.0, 255.0);
                let i = x as usize * 4;
                // Un-premultiply back to straight alpha for storage.
                if a <= 0.0 {
                    out_row[i] = 0;
                    out_row[i + 1] = 0;
                    out_row[i + 2] = 0;
                    out_row[i + 3] = 0;
                } else {
                    for c in 0..3 {
                        let v = (acc[c] * inv + bias).clamp(0.0, a);
                        out_row[i + c] = ((v * 255.0 / a).clamp(0.0, 255.0) + 0.5) as u8;
                    }
                    out_row[i + 3] = (a + 0.5) as u8;
                }
            }
        });
}

/// Gaussian blur with the given standard-deviation-like `radius`, in pixels.
///
/// Implemented as two 1-D passes; a 2-D Gaussian is separable, so this is
/// `O(r)` per pixel rather than `O(r²)`.
pub fn gaussian_blur(pixmap: &mut Pixmap, radius: f32) {
    if radius <= 0.0 || pixmap.is_empty() {
        return;
    }
    let sigma = radius.max(0.01);
    // Three sigma captures ~99.7% of the kernel's mass; going wider costs time
    // for no visible change.
    let taps = (sigma * 3.0).ceil() as i32;
    let kernel = gaussian_kernel_1d(sigma, taps);

    pixmap.premultiply();
    blur_pass(pixmap, &kernel, taps, true);
    blur_pass(pixmap, &kernel, taps, false);
    pixmap.unpremultiply();
}

fn gaussian_kernel_1d(sigma: f32, taps: i32) -> Vec<f32> {
    let mut k = Vec::with_capacity((taps * 2 + 1) as usize);
    let two_sigma_sq = 2.0 * sigma * sigma;
    for i in -taps..=taps {
        let x = i as f32;
        k.push((-(x * x) / two_sigma_sq).exp());
    }
    let sum: f32 = k.iter().sum();
    for v in k.iter_mut() {
        *v /= sum;
    }
    k
}

/// One separable pass. `horizontal` selects the axis.
fn blur_pass(pixmap: &mut Pixmap, kernel: &[f32], taps: i32, horizontal: bool) {
    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;
    let src = pixmap.clone();
    let src_ref = &src;
    let stride = pixmap.stride();

    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, out_row)| {
            let y = y as i32;
            for x in 0..width {
                let mut acc = [0.0f32; 4];
                for (i, &w) in kernel.iter().enumerate() {
                    let d = i as i32 - taps;
                    let (sx, sy) = if horizontal {
                        ((x + d).clamp(0, width - 1), y)
                    } else {
                        (x, (y + d).clamp(0, height - 1))
                    };
                    let p = src_ref.get(sx, sy);
                    acc[0] += p.r as f32 * w;
                    acc[1] += p.g as f32 * w;
                    acc[2] += p.b as f32 * w;
                    acc[3] += p.a as f32 * w;
                }
                let i = x as usize * 4;
                for c in 0..4 {
                    out_row[i + c] = (acc[c].clamp(0.0, 255.0) + 0.5) as u8;
                }
            }
        });
}

/// Box blur — a flat kernel. Cheaper than Gaussian; used for live previews.
pub fn box_blur(pixmap: &mut Pixmap, radius: u32) {
    if radius == 0 || pixmap.is_empty() {
        return;
    }
    let taps = radius as i32;
    let n = (taps * 2 + 1) as f32;
    let kernel = vec![1.0 / n; (taps * 2 + 1) as usize];

    pixmap.premultiply();
    blur_pass(pixmap, &kernel, taps, true);
    blur_pass(pixmap, &kernel, taps, false);
    pixmap.unpremultiply();
}

/// Filter ▸ Sharpen ▸ Sharpen.
pub fn sharpen(pixmap: &mut Pixmap) {
    convolve(pixmap, &Kernel::sharpen());
}

/// Unsharp mask: add back a scaled copy of the difference against a blurred
/// version. `threshold` suppresses sharpening of low-contrast areas (noise).
pub fn unsharp_mask(pixmap: &mut Pixmap, amount: f32, radius: f32, threshold: u8) {
    if amount <= 0.0 || radius <= 0.0 || pixmap.is_empty() {
        return;
    }
    let original = pixmap.clone();
    let mut blurred = pixmap.clone();
    gaussian_blur(&mut blurred, radius);

    let width = pixmap.width();
    let height = pixmap.height();
    let thresh = threshold as i32;

    for y in 0..height {
        for x in 0..width {
            let o = original.get(x as i32, y as i32);
            if o.a == 0 {
                continue;
            }
            let b = blurred.get(x as i32, y as i32);
            let mut out = o;
            for (oc, bc, dst) in [
                (o.r, b.r, &mut out.r),
                (o.g, b.g, &mut out.g),
                (o.b, b.b, &mut out.b),
            ] {
                let diff = oc as i32 - bc as i32;
                if diff.abs() >= thresh {
                    *dst = (oc as f32 + diff as f32 * amount).clamp(0.0, 255.0) as u8;
                }
            }
            pixmap.set(x as i32, y as i32, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Rgba8;

    #[test]
    fn kernel_normalises_by_weight_sum() {
        let k = Kernel::new(3, vec![1.0; 9]);
        assert_eq!(k.divisor, 9.0);
        assert_eq!(k.radius(), 1);
    }

    #[test]
    fn zero_sum_kernel_does_not_divide_by_zero() {
        let k = Kernel::edge_detect();
        assert_eq!(k.divisor, 1.0);
    }

    #[test]
    #[should_panic(expected = "must be odd")]
    fn even_kernel_size_is_rejected() {
        Kernel::new(2, vec![1.0; 4]);
    }

    #[test]
    #[should_panic(expected = "expected 9 weights")]
    fn mismatched_weight_count_is_rejected() {
        Kernel::new(3, vec![1.0; 4]);
    }

    #[test]
    fn blur_of_a_flat_image_is_unchanged() {
        let color = Rgba8::new(120, 130, 140, 255);
        let mut pm = Pixmap::filled(16, 16, color);
        gaussian_blur(&mut pm, 3.0);
        // Clamp-to-edge sampling means even border pixels see only `color`.
        for y in 0..16 {
            for x in 0..16 {
                let p = pm.get(x, y);
                assert!(
                    (p.r as i32 - 120).abs() <= 1
                        && (p.g as i32 - 130).abs() <= 1
                        && (p.b as i32 - 140).abs() <= 1,
                    "({},{}) drifted to {:?}",
                    x,
                    y,
                    p
                );
            }
        }
    }

    #[test]
    fn blur_spreads_a_single_dot() {
        let mut pm = Pixmap::new(9, 9);
        pm.set(4, 4, Rgba8::WHITE);
        gaussian_blur(&mut pm, 2.0);
        // Energy moved outward: neighbours are no longer empty...
        assert!(pm.get(3, 4).a > 0, "blur did not spread");
        // ...and the centre gave some up.
        assert!(pm.get(4, 4).a < 255);
    }

    #[test]
    fn blur_does_not_bleed_color_from_transparent_pixels() {
        // A red dot on a transparent field. If the blur ran on straight alpha,
        // the transparent (0,0,0,0) neighbours would darken the result.
        let mut pm = Pixmap::new(9, 9);
        pm.set(4, 4, Rgba8::new(255, 0, 0, 255));
        gaussian_blur(&mut pm, 1.5);
        let p = pm.get(4, 4);
        assert!(p.r > 200, "red channel darkened to {}", p.r);
        assert!(p.g < 40 && p.b < 40, "color bled: {:?}", p);
    }

    #[test]
    fn zero_radius_blur_is_a_no_op() {
        let mut pm = Pixmap::filled(4, 4, Rgba8::new(1, 2, 3, 255));
        let before = pm.as_bytes().to_vec();
        gaussian_blur(&mut pm, 0.0);
        box_blur(&mut pm, 0);
        assert_eq!(pm.as_bytes(), &before[..]);
    }

    #[test]
    fn filters_handle_empty_pixmaps() {
        let mut pm = Pixmap::new(0, 0);
        gaussian_blur(&mut pm, 2.0);
        box_blur(&mut pm, 2);
        sharpen(&mut pm);
        unsharp_mask(&mut pm, 1.0, 1.0, 0);
        assert!(pm.is_empty());
    }

    #[test]
    fn sharpen_preserves_a_flat_region() {
        let mut pm = Pixmap::filled(8, 8, Rgba8::new(100, 100, 100, 255));
        sharpen(&mut pm);
        // 5*c - 4*c == c for a constant neighbourhood.
        let p = pm.get(4, 4);
        assert!((p.r as i32 - 100).abs() <= 1, "got {}", p.r);
    }

    #[test]
    fn sharpen_increases_edge_contrast() {
        let mut pm = Pixmap::new(8, 8);
        for y in 0..8 {
            for x in 0..8 {
                let v = if x < 4 { 80 } else { 160 };
                pm.set(x, y, Rgba8::new(v, v, v, 255));
            }
        }
        let before_dark = pm.get(3, 4).r;
        let before_light = pm.get(4, 4).r;
        sharpen(&mut pm);
        let after_dark = pm.get(3, 4).r;
        let after_light = pm.get(4, 4).r;
        assert!(
            (after_light as i32 - after_dark as i32)
                > (before_light as i32 - before_dark as i32),
            "edge did not sharpen"
        );
    }

    #[test]
    fn unsharp_threshold_suppresses_low_contrast() {
        let build = || {
            let mut pm = Pixmap::new(8, 8);
            for y in 0..8 {
                for x in 0..8 {
                    // A very gentle gradient — below a high threshold.
                    let v = 100 + x as u8;
                    pm.set(x, y, Rgba8::new(v, v, v, 255));
                }
            }
            pm
        };
        let mut high = build();
        unsharp_mask(&mut high, 2.0, 2.0, 250);
        assert_eq!(high.as_bytes(), build().as_bytes(), "threshold ignored");
    }

    #[test]
    fn box_blur_matches_flat_input() {
        let mut pm = Pixmap::filled(8, 8, Rgba8::new(50, 60, 70, 255));
        box_blur(&mut pm, 2);
        let p = pm.get(4, 4);
        assert!((p.r as i32 - 50).abs() <= 1 && (p.b as i32 - 70).abs() <= 1);
    }
}

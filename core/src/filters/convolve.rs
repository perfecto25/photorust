//! Neighbourhood filters: convolution kernels and separable blurs.
//!
//! Convolution runs on **premultiplied** colour. Blurring straight-alpha RGB
//! lets the colour of fully transparent pixels bleed into visible ones, which
//! shows up as dark halos around soft edges.

use crate::buffer::{Pixmap, Rgba8};
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

/// Gaussian blur through the active rendering backend.
///
/// Use this from anything the user waits on. [`gaussian_blur`] above stays the
/// CPU reference: the GPU backend falls back to it, and the parity tests
/// compare against it, so it must not itself dispatch or the two would recurse.
pub fn gaussian_blur_accelerated(pixmap: &mut Pixmap, radius: f32) {
    crate::gpu::shared().gaussian_blur(pixmap, radius);
}

/// Shared with the GPU backend, which must use byte-identical weights or its
/// output will drift from the CPU reference.
pub(crate) fn gaussian_kernel_1d(sigma: f32, taps: i32) -> Vec<f32> {
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
    gaussian_blur_accelerated(&mut blurred, radius);

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

/// Replace every pixel with the average of the whole image — Filter ▸ Blur ▸
/// Average.
///
/// Not a convolution at all despite living here: it is one mean over the
/// region, which is why it takes no radius. Alpha is averaged with the
/// colour so a partly transparent layer flattens to one even wash rather
/// than developing edges where its coverage changed.
pub fn average(pixmap: &mut Pixmap) {
    let count = (pixmap.width() as u64) * (pixmap.height() as u64);
    if count == 0 {
        return;
    }
    let (mut r, mut g, mut b, mut a) = (0u64, 0u64, 0u64, 0u64);
    for y in 0..pixmap.height() as i32 {
        for x in 0..pixmap.width() as i32 {
            let px = pixmap.get(x, y);
            r += px.r as u64;
            g += px.g as u64;
            b += px.b as u64;
            a += px.a as u64;
        }
    }
    let mean = Rgba8::new(
        (r / count) as u8,
        (g / count) as u8,
        (b / count) as u8,
        (a / count) as u8,
    );
    for y in 0..pixmap.height() as i32 {
        for x in 0..pixmap.width() as i32 {
            pixmap.set(x, y, mean);
        }
    }
}

/// Smear the image along a line — Filter ▸ Blur ▸ Motion Blur.
///
/// `angle` is in degrees anticlockwise from the horizontal and `distance` is
/// the length of the smear in pixels. Sampled along the line rather than
/// built as a 2-D kernel: the kernel would be mostly zeroes, and the cost
/// would grow with the square of the distance instead of with the distance.
pub fn motion_blur(pixmap: &mut Pixmap, angle: f32, distance: f32) {
    let steps = distance.max(0.0).round() as i32;
    if steps < 1 {
        return;
    }
    let radians = angle.to_radians();
    let (dx, dy) = (radians.cos(), -radians.sin());
    let source = pixmap.clone();

    for y in 0..pixmap.height() as i32 {
        for x in 0..pixmap.width() as i32 {
            let (mut r, mut g, mut b, mut a, mut n) = (0f32, 0f32, 0f32, 0f32, 0f32);
            // Centred on the pixel, so the smear runs equally both ways and
            // the image does not appear to shift as the distance is raised.
            for step in -steps / 2..=steps / 2 {
                let sx = (x as f32 + dx * step as f32).round() as i32;
                let sy = (y as f32 + dy * step as f32).round() as i32;
                if !pixmap.rect().contains(sx, sy) {
                    continue;
                }
                let px = source.get(sx, sy);
                r += px.r as f32;
                g += px.g as f32;
                b += px.b as f32;
                a += px.a as f32;
                n += 1.0;
            }
            if n > 0.0 {
                pixmap.set(
                    x,
                    y,
                    Rgba8::new(
                        (r / n).round() as u8,
                        (g / n).round() as u8,
                        (b / n).round() as u8,
                        (a / n).round() as u8,
                    ),
                );
            }
        }
    }
}

/// How finely a radial blur samples the path each pixel travels — CS6's
/// Draft / Good / Best.
///
/// This is the whole difference between the three: a spin is the average of
/// the pixels along an arc, and how many of them are actually looked at
/// decides whether the result reads as motion or as a handful of ghost copies
/// of the image rotated over each other. Draft is for finding the amount you
/// want, Best is for the one you keep.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RadialQuality {
    Draft,
    #[default]
    Good,
    Best,
}

impl RadialQuality {
    pub fn from_i32(value: i32) -> RadialQuality {
        match value {
            0 => RadialQuality::Draft,
            2 => RadialQuality::Best,
            _ => RadialQuality::Good,
        }
    }

    /// Samples per pixel of path travelled, and the ceiling on them.
    ///
    /// The ceiling matters: the path grows with the distance from the centre,
    /// so without one the corners of a large image would cost hundreds of
    /// samples each and the filter would take minutes.
    ///
    /// What the three cost, measured on a 2000×1500 image at Amount 100:
    /// Draft 0.32s, Good 1.7s, Best 5.6s. That is the shape CS6 has too —
    /// Draft is for finding the amount you want and Best is for the one you
    /// keep — and it is why the Radial Blur dialog has no live preview.
    fn sampling(self) -> (f32, usize) {
        match self {
            RadialQuality::Draft => (0.25, 16),
            RadialQuality::Good => (1.0, 96),
            RadialQuality::Best => (2.0, 320),
        }
    }
}

/// Spin or zoom the image about a centre — Filter ▸ Blur ▸ Radial Blur.
///
/// `spin` chooses between the two: turning the samples about the centre, or
/// running them in and out along the radius. `amount` is a percentage, as
/// CS6's slider is, and `center` is where the blur turns about, in normalized
/// 0..1 coordinates so that it survives a preview at any scale — CS6's Blur
/// Center box sets it the same way.
///
/// Each output pixel is the mean of the source along the path it would travel,
/// sampled bilinearly and clamped at the edges, like the separable blurs above.
/// The number of samples follows the length of that path rather than being
/// fixed, because the path is short near the centre and long at the rim: a
/// fixed count is either wasted in the middle or far too sparse at the edge,
/// and the sparse end is what produces concentric ghost arcs instead of a
/// sweep.
pub fn radial_blur(
    pixmap: &mut Pixmap,
    amount: f32,
    spin: bool,
    center: (f32, f32),
    quality: RadialQuality,
) {
    let strength = (amount.max(0.0) / 100.0).min(1.0);
    if strength <= 0.0 || pixmap.is_empty() {
        return;
    }

    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;
    let cx = center.0.clamp(0.0, 1.0) * width as f32;
    let cy = center.1.clamp(0.0, 1.0) * height as f32;

    // Blurring straight RGBA drags colour out of transparent pixels and leaves
    // a halo round anything with a soft edge; the separable blurs premultiply
    // for the same reason.
    pixmap.premultiply();
    let source = pixmap.clone();
    let src = &source;

    // What Amount 100 means, matched against CS6: a spin sweeps through 60°
    // at every radius — enough to smear a flower into rings, which is what
    // its maximum does — and a zoom runs from three quarters of the radius
    // out to five quarters. The two need separate numbers because they are
    // different quantities, an angle and a scale; sharing one made the spin
    // far too timid at the top of its slider.
    let span = if spin {
        strength * 60.0_f32.to_radians()
    } else {
        strength * 0.5
    };
    let (density, cap) = quality.sampling();
    let stride = pixmap.stride();

    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            for x in 0..width {
                let ox = x as f32 + 0.5 - cx;
                let oy = y as f32 + 0.5 - cy;
                let radius = (ox * ox + oy * oy).sqrt();

                // How far this pixel travels: an arc of `radius * span` for a
                // spin, a straight run of `radius * span` for a zoom. Both
                // vanish at the centre, which is why the middle stays sharp.
                let path = radius * span;
                let steps = ((path * density).ceil() as usize).clamp(1, cap);

                let i = x as usize * 4;
                if steps <= 1 {
                    continue;
                }

                let angle = oy.atan2(ox);
                let mut acc = [0.0f32; 4];
                for step in 0..steps {
                    // Centred on the pixel, so the blur reaches equally both
                    // ways rather than trailing off to one side.
                    let t = step as f32 / (steps - 1) as f32 - 0.5;
                    let (sx, sy) = if spin {
                        let turned = angle + t * span;
                        (cx + radius * turned.cos(), cy + radius * turned.sin())
                    } else {
                        let scaled = radius * (1.0 + t * span);
                        (cx + scaled * angle.cos(), cy + scaled * angle.sin())
                    };
                    let p = crate::resample::bilinear(src, sx - 0.5, sy - 0.5);
                    acc[0] += p.r as f32;
                    acc[1] += p.g as f32;
                    acc[2] += p.b as f32;
                    acc[3] += p.a as f32;
                }

                let n = steps as f32;
                for c in 0..4 {
                    out[i + c] = (acc[c] / n).clamp(0.0, 255.0).round() as u8;
                }
            }
        });

    pixmap.unpremultiply();
}

/// Blur flat areas while leaving edges alone — Filter ▸ Blur ▸ Surface Blur.
///
/// A neighbour only counts if it is within `threshold` of the pixel being
/// worked on, so a region blurs within itself but never across a boundary
/// into something a different colour. That is the whole difference between
/// this and a box blur, and it is why it cannot be separated into two passes
/// the way an ordinary blur can: which neighbours count depends on the centre
/// pixel, so the horizontal pass would not know what the vertical one wanted.
pub fn surface_blur(pixmap: &mut Pixmap, radius: u32, threshold: u32) {
    if radius == 0 {
        return;
    }
    let reach = radius as i32;
    let limit = threshold.max(1) as i32;
    let source = pixmap.clone();

    for y in 0..pixmap.height() as i32 {
        for x in 0..pixmap.width() as i32 {
            let centre = source.get(x, y);
            let (mut r, mut g, mut b, mut a, mut n) = (0u32, 0u32, 0u32, 0u32, 0u32);
            for dy in -reach..=reach {
                for dx in -reach..=reach {
                    let (sx, sy) = (x + dx, y + dy);
                    if !source.rect().contains(sx, sy) {
                        continue;
                    }
                    let px = source.get(sx, sy);
                    let difference = (px.r as i32 - centre.r as i32).abs()
                        .max((px.g as i32 - centre.g as i32).abs())
                        .max((px.b as i32 - centre.b as i32).abs());
                    if difference > limit {
                        continue;
                    }
                    r += px.r as u32;
                    g += px.g as u32;
                    b += px.b as u32;
                    a += px.a as u32;
                    n += 1;
                }
            }
            if n > 0 {
                pixmap.set(
                    x,
                    y,
                    Rgba8::new(
                        (r / n) as u8,
                        (g / n) as u8,
                        (b / n) as u8,
                        (a / n) as u8,
                    ),
                );
            }
        }
    }
}

#[cfg(test)]
mod blur_tests {
    use super::*;
    use crate::filters::Filter;

    /// Left half black, right half white — a single hard edge to blur across.
    /// Detail everywhere, for the tests that need to see how far each part of
    /// the image moved.
    fn checkerboard(size: u32, square: u32) -> Pixmap {
        let mut px = Pixmap::new(size, size);
        for y in 0..size as i32 {
            for x in 0..size as i32 {
                let on = ((x as u32 / square) + (y as u32 / square)) % 2 == 0;
                let v = if on { 0 } else { 255 };
                px.set(x, y, Rgba8::new(v, v, v, 255));
            }
        }
        px
    }

    fn split(width: u32, height: u32) -> Pixmap {
        let mut px = Pixmap::new(width, height);
        for y in 0..height as i32 {
            for x in 0..width as i32 {
                let v = if (x as u32) < width / 2 { 0 } else { 255 };
                px.set(x, y, Rgba8::new(v, v, v, 255));
            }
        }
        px
    }

    #[test]
    fn average_flattens_everything_to_one_colour() {
        let mut px = split(8, 4);
        average(&mut px);
        let first = px.get(0, 0);
        for y in 0..4 {
            for x in 0..8 {
                assert_eq!(px.get(x, y), first, "average left more than one colour");
            }
        }
        // Half black and half white average to the middle, not to either end.
        assert!(first.r > 100 && first.r < 155, "got {}", first.r);
    }

    #[test]
    fn blur_more_softens_further_than_blur() {
        // How far the edge has spread is how many pixels are neither black nor
        // white, which is what "more" has to mean.
        let spread = |mut px: Pixmap, f: Filter| {
            f.apply(&mut px);
            (0..8).filter(|&x| {
                let v = px.get(x, 2).r;
                v > 20 && v < 235
            }).count()
        };
        let soft = spread(split(8, 4), Filter::Blur);
        let softer = spread(split(8, 4), Filter::BlurMore);
        assert!(softer > soft, "Blur More spread {} against Blur's {}", softer, soft);
    }

    #[test]
    fn motion_blur_smears_along_its_angle_and_not_across_it() {
        // A horizontal smear crosses a vertical edge, so the edge softens...
        let mut across = split(16, 8);
        motion_blur(&mut across, 0.0, 8.0);
        let softened = (0..16).filter(|&x| {
            let v = across.get(x, 4).r;
            v > 20 && v < 235
        }).count();
        assert!(softened > 2, "a horizontal smear did not blur a vertical edge");

        // ...while a vertical smear runs along it and leaves it hard.
        let mut along = split(16, 8);
        motion_blur(&mut along, 90.0, 8.0);
        assert_eq!(along.get(3, 4).r, 0, "the dark side should stay dark");
        assert_eq!(along.get(12, 4).r, 255, "the light side should stay light");
    }

    #[test]
    fn surface_blur_keeps_the_edge_a_box_blur_would_lose() {
        // A threshold below the black-to-white step means no neighbour across
        // the edge ever counts, so the edge survives — which is the whole
        // point of this filter over an ordinary blur.
        let mut kept = split(16, 8);
        surface_blur(&mut kept, 3, 40);
        assert_eq!(kept.get(7, 4).r, 0, "the edge bled despite the threshold");
        assert_eq!(kept.get(8, 4).r, 255, "the edge bled despite the threshold");

        // Raised past the step, it behaves like a blur again.
        let mut blurred = split(16, 8);
        surface_blur(&mut blurred, 3, 255);
        let v = blurred.get(7, 4).r;
        assert!(v > 0 && v < 255, "a wide-open threshold should blur, got {}", v);
    }

    #[test]
    fn radial_blur_leaves_its_centre_alone() {
        let mut px = split(32, 32);
        let before = px.get(16, 16);
        radial_blur(&mut px, 50.0, true, (0.5, 0.5), RadialQuality::Good);
        // Nothing sweeps at the centre of a spin: the arc there has no length.
        assert_eq!(px.get(16, 16), before, "the centre of a spin should not move");
        // ...but pixels away from it do, where their arc carries them across
        // the edge. Counted over the whole image rather than sampled at one
        // point: most of a split image is uniform, and an arc that stays
        // inside one half averages white with white and reports nothing.
        let before = split(32, 32);
        let mut after = split(32, 32);
        radial_blur(&mut after, 50.0, true, (0.5, 0.5), RadialQuality::Good);
        let moved = (0..32)
            .flat_map(|y| (0..32).map(move |x| (x, y)))
            .filter(|&(x, y)| after.get(x, y) != before.get(x, y))
            .count();
        assert!(moved > 20, "a spin barely moved anything: {} pixels", moved);
    }

    #[test]
    fn a_full_spin_sweeps_far_enough_to_smear_a_flower_into_rings() {
        // The number this pins is what Amount 100 means, and getting it wrong
        // is not a bug you can see in a unit test any other way: the filter
        // works, it just does far less than CS6's does, which is exactly how
        // this shipped once. A narrow spike is smeared round and the angle it
        // ends up covering is measured.
        let size = 200i32;
        let centre = size as f32 / 2.0;
        let mut px = Pixmap::filled(size as u32, size as u32, Rgba8::BLACK);
        for y in 0..size {
            for x in 0..size {
                let angle = (y as f32 - centre).atan2(x as f32 - centre);
                if angle.abs() < 0.05 {
                    px.set(x, y, Rgba8::WHITE);
                }
            }
        }

        radial_blur(&mut px, 100.0, true, (0.5, 0.5), RadialQuality::Good);

        // Round a circle well out from the centre, how far either side of the
        // spike did any of it reach?
        let radius = 80.0;
        let mut reach: f32 = 0.0;
        for step in 0..720 {
            let angle = step as f32 * std::f32::consts::PI / 360.0 - std::f32::consts::PI;
            let x = (centre + radius * angle.cos()).round() as i32;
            let y = (centre + radius * angle.sin()).round() as i32;
            if px.get(x, y).r > 8 {
                reach = reach.max(angle.abs());
            }
        }

        let degrees = reach.to_degrees();
        assert!(
            (25.0..40.0).contains(&degrees),
            "a full spin reached {:.0}° either side of the spike; CS6's sweeps 60° in total, \
             so this should be near 30",
            degrees
        );
    }

    #[test]
    fn the_still_point_of_a_spin_follows_the_blur_centre() {
        // The centre is CS6's Blur Center box, in fractions of the image. If
        // it were ignored — the mistake it replaced — every spin would turn
        // about the middle whatever the box said.
        //
        // On a checkerboard rather than the split, because every part of it
        // has detail to lose: two halves of flat colour would report nothing
        // wherever an arc stayed inside one of them.
        let mut px = checkerboard(64, 4);
        radial_blur(&mut px, 100.0, true, (0.25, 0.25), RadialQuality::Good);

        let reference = checkerboard(64, 4);
        // Distance from the nominated centre decides how far a pixel travels,
        // so the far corner must be disturbed more than the near one. Measured
        // as a sum over a patch, since a single pixel can sit inside a run of
        // one colour and report nothing.
        let disturbance = |x0: i32, y0: i32| -> i64 {
            let mut total = 0i64;
            for y in y0..y0 + 8 {
                for x in x0..x0 + 8 {
                    let a = px.get(x, y).r as i64;
                    let b = reference.get(x, y).r as i64;
                    total += (a - b).abs();
                }
            }
            total
        };

        let near = disturbance(12, 12);
        let far = disturbance(52, 52);
        assert!(
            far > near,
            "the spin did not turn about the centre it was given: near {} far {}",
            near,
            far
        );
    }

    #[test]
    fn a_better_quality_samples_the_arc_more_finely() {
        // Draft, Good and Best differ only in how many points along the arc
        // are looked at. Too few and the result is a handful of rotated copies
        // of the image laid over each other — the concentric ghosting that
        // makes a spin look wrong — rather than a sweep. A sweep is smoother
        // across the direction of travel, so the count of neighbouring pixels
        // that differ sharply is the thing to measure.
        let hard_edges = |px: &Pixmap| -> usize {
            let mut count = 0;
            for y in 0..px.height() as i32 {
                for x in 1..px.width() as i32 {
                    let a = px.get(x, y).r as i32;
                    let b = px.get(x - 1, y).r as i32;
                    if (a - b).abs() > 64 {
                        count += 1;
                    }
                }
            }
            count
        };

        let mut draft = split(64, 64);
        radial_blur(&mut draft, 100.0, true, (0.5, 0.5), RadialQuality::Draft);
        let mut best = split(64, 64);
        radial_blur(&mut best, 100.0, true, (0.5, 0.5), RadialQuality::Best);

        assert!(
            hard_edges(&best) < hard_edges(&draft),
            "Best left as many hard steps as Draft ({} vs {}), so it is not sampling any \
             more finely",
            hard_edges(&best),
            hard_edges(&draft)
        );
    }

    #[test]
    fn spin_and_zoom_smear_in_different_directions() {
        // A spin travels along the arc, a zoom along the radius — at right
        // angles to each other. Swapping the two is an easy mistake and the
        // images stay superficially plausible either way.
        let mut spun = split(64, 64);
        radial_blur(&mut spun, 100.0, true, (0.5, 0.5), RadialQuality::Good);
        let mut zoomed = split(64, 64);
        radial_blur(&mut zoomed, 100.0, false, (0.5, 0.5), RadialQuality::Good);

        // Directly above the centre, a spin moves sideways across the split's
        // vertical edge and softens it; a zoom moves straight up and down it,
        // along the edge, and leaves it hard.
        let softness = |px: &Pixmap| -> i32 {
            let left = px.get(30, 8).r as i32;
            let right = px.get(33, 8).r as i32;
            255 - (right - left).abs()
        };

        assert!(
            softness(&spun) > softness(&zoomed),
            "a spin should soften the edge it crosses more than a zoom running along it: \
             spin {} zoom {}",
            softness(&spun),
            softness(&zoomed)
        );
    }
}

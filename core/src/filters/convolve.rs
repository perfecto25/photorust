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

/// Average every pixel within `radius` — a *circular* window, not a square.
///
/// This is the shape a lens actually throws an out-of-focus point into, which
/// is why it is the blur Smart Sharpen removes when you tell it the softness
/// came from a lens. A Gaussian's long tail and a box's corners both give the
/// wrong halo when you sharpen against them.
///
/// Done with a summed-area table, because a disc is not separable: written the
/// obvious way it would cost `(2r+1)²` reads per pixel, and CS6 allows a
/// radius of 64. A disc is a stack of horizontal spans, and a table of running
/// sums answers "what is in this span" in constant time, so a pixel costs
/// `O(r)` — one span per row of the disc.
pub fn disc_blur(pixmap: &mut Pixmap, radius: u32) {
    if radius == 0 || pixmap.is_empty() {
        return;
    }
    let width = pixmap.width() as usize;
    let height = pixmap.height() as usize;
    let reach = radius as i32;

    pixmap.premultiply();

    // (height + 1) × (width + 1) so that a span sum needs no bounds test, and
    // `u32` because the largest total a channel can reach is 255 × the pixel
    // count, which stays inside it for any image this program can open.
    let stride_sat = (width + 1) * 4;
    let mut sat = vec![0u32; (height + 1) * stride_sat];
    {
        let src = pixmap.as_bytes();
        for y in 0..height {
            let above = y * stride_sat;
            let here = (y + 1) * stride_sat;
            let mut running = [0u32; 4];
            for x in 0..width {
                let i = (y * width + x) * 4;
                for c in 0..4 {
                    running[c] += src[i + c] as u32;
                    sat[here + (x + 1) * 4 + c] = sat[above + (x + 1) * 4 + c] + running[c];
                }
            }
        }
    }

    // Half-widths of the disc, one per row offset, worked out once.
    let spans: Vec<i32> = (-reach..=reach)
        .map(|dy| (((reach * reach - dy * dy) as f32).sqrt()) as i32)
        .collect();

    let sat_ref = &sat;
    let spans_ref = &spans;
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            for x in 0..width as i32 {
                let mut total = [0u64; 4];
                let mut count = 0u64;
                for (index, &half) in spans_ref.iter().enumerate() {
                    let yy = y + index as i32 - reach;
                    if yy < 0 || yy >= height as i32 {
                        continue;
                    }
                    let x0 = (x - half).max(0) as usize;
                    let x1 = (x + half).min(width as i32 - 1) as usize;
                    let top = yy as usize * stride_sat;
                    let bottom = (yy as usize + 1) * stride_sat;
                    for c in 0..4 {
                        // Grouped so that neither half goes negative on the
                        // way: the two added corners always outweigh the two
                        // subtracted ones, but `a - b - d` on its own need
                        // not, and these are unsigned.
                        let plus = sat_ref[bottom + (x1 + 1) * 4 + c] as u64
                            + sat_ref[top + x0 * 4 + c] as u64;
                        let minus = sat_ref[bottom + x0 * 4 + c] as u64
                            + sat_ref[top + (x1 + 1) * 4 + c] as u64;
                        total[c] += plus - minus;
                    }
                    count += (x1 - x0 + 1) as u64;
                }
                let i = x as usize * 4;
                let n = count.max(1);
                for c in 0..4 {
                    out[i + c] = (total[c] / n) as u8;
                }
            }
        });

    pixmap.unpremultiply();
}

/// Which kind of softness Smart Sharpen is being asked to undo.
///
/// It matters because sharpening is an attempt to invert a blur, and the
/// answer depends on which blur: the same amount against the wrong shape
/// leaves halos where the right one leaves detail.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SharpenRemove {
    #[default]
    GaussianBlur,
    LensBlur,
    MotionBlur,
}

impl SharpenRemove {
    pub fn from_i32(value: i32) -> SharpenRemove {
        match value {
            1 => SharpenRemove::LensBlur,
            2 => SharpenRemove::MotionBlur,
            _ => SharpenRemove::GaussianBlur,
        }
    }
}

/// Filter ▸ Sharpen ▸ Smart Sharpen.
///
/// Plain Sharpen is an unsharp mask against a Gaussian and nothing else. This
/// adds the two things that make it "smart":
///
/// * **What to remove.** The sharpening is done against the blur you say the
///   softness came from — a Gaussian, a lens's disc, or a motion streak at a
///   given angle — rather than always a Gaussian.
/// * **Reduce Noise.** Fine detail below a noise floor is held back, so that
///   grain is not crisped along with the subject. A soft curve rather than a
///   cut-off: a hard threshold sharpens one side of it and not the other,
///   which shows up as mottling in a smooth area.
///
/// `amount` is a percentage as CS6's slider is, `radius` in pixels, and
/// `reduce_noise` a percentage. `angle` is only read for a motion streak.
pub fn smart_sharpen(
    pixmap: &mut Pixmap,
    amount: f32,
    radius: f32,
    reduce_noise: f32,
    remove: SharpenRemove,
    angle: f32,
) {
    if amount <= 0.0 || radius <= 0.0 || pixmap.is_empty() {
        return;
    }
    let strength = amount / 100.0;

    let original = pixmap.clone();
    let mut blurred = pixmap.clone();
    match remove {
        SharpenRemove::GaussianBlur => gaussian_blur_accelerated(&mut blurred, radius),
        SharpenRemove::LensBlur => disc_blur(&mut blurred, radius.round().max(1.0) as u32),
        // The radius names how far the softness reaches either way, so the
        // streak it came from is twice that long.
        SharpenRemove::MotionBlur => motion_blur(&mut blurred, angle, radius * 2.0),
    }

    // Detail this size and below is taken to be noise. At Reduce Noise 100%
    // that is about a tenth of the range, which is enough to hold back film
    // grain and sensor noise without touching anything that reads as texture.
    let floor = (reduce_noise.clamp(0.0, 100.0) / 100.0) * 26.0;

    let width = pixmap.width() as i32;
    let blur = &blurred;
    let source = &original;
    let stride = pixmap.stride();

    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            for x in 0..width {
                let o = source.get(x, y);
                if o.a == 0 {
                    continue;
                }
                let b = blur.get(x, y);
                let i = x as usize * 4;
                for (c, (oc, bc)) in [(o.r, b.r), (o.g, b.g), (o.b, b.b)].into_iter().enumerate()
                {
                    let diff = oc as f32 - bc as f32;
                    // Passes detail well above the floor at full strength and
                    // fades everything below it away, with no corner in the
                    // curve for a smooth area to break along.
                    let gain = if floor > 0.0 {
                        let d = diff * diff;
                        d / (d + floor * floor)
                    } else {
                        1.0
                    };
                    out[i + c] = (oc as f32 + diff * strength * gain).clamp(0.0, 255.0) as u8;
                }
            }
        });
}

/// Filter ▸ Sharpen ▸ Sharpen, and its stronger sibling.
///
/// Both are an unsharp mask at a fixed amount rather than a 3×3 convolution.
/// The classic `[0 -1 0; -1 5 -1; 0 -1 0]` kernel — which this used to be, and
/// which [`Kernel::sharpen`] still is — has enormous gain at the finest detail
/// an image can hold, so it does not so much sharpen a photograph as amplify
/// its noise: one pass multiplies the high-frequency energy of a photograph by
/// about six, two passes by forty, and by three the picture is gone. Photoshop
/// can be applied over and over, each pass adding a little, and that is what
/// these numbers reproduce: a pass multiplies it by about 1.5.
///
/// `Sharpen More` is a heavier dose of the same thing, not a different filter,
/// which is exactly how CS6 describes it.
pub fn sharpen(pixmap: &mut Pixmap) {
    unsharp_mask(pixmap, 0.5, 1.0, 0);
}

/// Filter ▸ Sharpen ▸ Sharpen More.
pub fn sharpen_more(pixmap: &mut Pixmap) {
    unsharp_mask(pixmap, 1.0, 1.0, 0);
}

/// Filter ▸ Sharpen ▸ Sharpen Edges: sharpen where there is an edge and leave
/// everything else alone.
///
/// Plain Sharpen treats a patch of film grain in a flat sky exactly like the
/// boundary of a petal, and crisping the grain is not what anybody wanted.
/// This weighs each pixel's sharpening by how much of an edge is actually
/// there, so flat areas come through untouched and can therefore take a
/// heavier hand where it counts.
///
/// The edge is measured on the *blurred* copy rather than the original, so
/// that noise — which is by definition what a blur removes — does not read as
/// an edge and invite the sharpening it was meant to escape.
pub fn sharpen_edges(pixmap: &mut Pixmap) {
    if pixmap.is_empty() {
        return;
    }
    const AMOUNT: f32 = 1.0;
    /// Gradients below this are flat ground, above the second are a definite
    /// edge; in between the effect fades in, because a hard cut-off draws a
    /// visible outline round every shape it decides to sharpen.
    const EDGE_FLOOR: f32 = 6.0;
    const EDGE_CEILING: f32 = 36.0;

    let original = pixmap.clone();
    let mut blurred = pixmap.clone();
    gaussian_blur_accelerated(&mut blurred, 1.0);

    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;
    let blur = &blurred;

    let luma = |x: i32, y: i32| -> f32 {
        let p = blur.get(x.clamp(0, width - 1), y.clamp(0, height - 1));
        0.299 * p.r as f32 + 0.587 * p.g as f32 + 0.114 * p.b as f32
    };

    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            for x in 0..width {
                // Sobel, on brightness: an edge is an edge whatever colour it
                // separates.
                let gx = -luma(x - 1, y - 1) - 2.0 * luma(x - 1, y) - luma(x - 1, y + 1)
                    + luma(x + 1, y - 1)
                    + 2.0 * luma(x + 1, y)
                    + luma(x + 1, y + 1);
                let gy = -luma(x - 1, y - 1) - 2.0 * luma(x, y - 1) - luma(x + 1, y - 1)
                    + luma(x - 1, y + 1)
                    + 2.0 * luma(x, y + 1)
                    + luma(x + 1, y + 1);
                let magnitude = (gx * gx + gy * gy).sqrt() / 4.0;

                let t = ((magnitude - EDGE_FLOOR) / (EDGE_CEILING - EDGE_FLOOR)).clamp(0.0, 1.0);
                // Smoothstep, so the mask has no corner in it to show up as a
                // ring where the sharpening starts.
                let weight = t * t * (3.0 - 2.0 * t) * AMOUNT;
                if weight <= 0.0 {
                    continue;
                }

                let o = original.get(x, y);
                if o.a == 0 {
                    continue;
                }
                let b = blur.get(x, y);
                let i = x as usize * 4;
                for (c, (oc, bc)) in [(o.r, b.r), (o.g, b.g), (o.b, b.b)].into_iter().enumerate() {
                    let diff = oc as f32 - bc as f32;
                    out[i + c] = (oc as f32 + diff * weight).clamp(0.0, 255.0) as u8;
                }
            }
        });
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
    if steps < 1 || pixmap.is_empty() {
        return;
    }
    let radians = angle.to_radians();
    let (dx, dy) = (radians.cos(), -radians.sin());

    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;
    let source = pixmap.clone();
    let src = &source;
    let stride = pixmap.stride();

    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            for x in 0..width {
                let (mut r, mut g, mut b, mut a, mut n) = (0f32, 0f32, 0f32, 0f32, 0f32);
                // Centred on the pixel, so the smear runs equally both ways
                // and the image does not appear to shift as the distance is
                // raised.
                for step in -steps / 2..=steps / 2 {
                    let sx = (x as f32 + dx * step as f32).round() as i32;
                    let sy = (y as f32 + dy * step as f32).round() as i32;
                    if sx < 0 || sy < 0 || sx >= width || sy >= height {
                        continue;
                    }
                    let px = src.get(sx, sy);
                    r += px.r as f32;
                    g += px.g as f32;
                    b += px.b as f32;
                    a += px.a as f32;
                    n += 1.0;
                }
                if n > 0.0 {
                    let i = x as usize * 4;
                    out[i] = (r / n).round() as u8;
                    out[i + 1] = (g / n).round() as u8;
                    out[i + 2] = (b / n).round() as u8;
                    out[i + 3] = (a / n).round() as u8;
                }
            }
        });
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

/// The median of every pixel within `radius` — Filter ▸ Noise ▸ Median.
///
/// Unlike a mean, a median throws away outliers rather than smearing them
/// about, which is why it is the tool for a speck of dust or a stuck pixel:
/// one wrong value among a few hundred cannot move the middle one. At larger
/// radii it flattens the picture into poster-like patches, which is the look
/// CS6's reference image shows.
///
/// Built on the same sliding histogram as [`surface_blur`], and for the same
/// reason: the obvious way costs `(2r+1)²` reads per pixel and CS6 allows a
/// radius of 100. Sliding the window costs `O(r)` and the median is then read
/// off by walking the tally until half the window is behind you.
pub fn median_filter(pixmap: &mut Pixmap, radius: u32) {
    if radius == 0 || pixmap.is_empty() {
        return;
    }
    let medians = median_of(pixmap, radius);
    pixmap.as_bytes_mut().copy_from_slice(medians.as_bytes());
}

/// Filter ▸ Noise ▸ Dust & Scratches.
///
/// The median again, but only where the pixel is far enough from it to be
/// worth suspecting. `threshold` is how different a pixel has to be before it
/// is treated as a speck rather than as detail — at 0 every pixel is replaced
/// and this is exactly Median, and at 255 nothing is, which is what makes the
/// slider a way to keep the picture while losing the dust.
pub fn dust_and_scratches(pixmap: &mut Pixmap, radius: u32, threshold: u32) {
    if radius == 0 || pixmap.is_empty() {
        return;
    }
    let medians = median_of(pixmap, radius);
    let limit = threshold.min(255) as i32;
    let median_bytes = medians.as_bytes();

    for (out, &median) in pixmap.as_bytes_mut().iter_mut().zip(median_bytes) {
        if (*out as i32 - median as i32).abs() > limit {
            *out = median;
        }
    }
}

/// The median of each pixel's neighbourhood, as its own image.
///
/// Shared by Median and Dust & Scratches, which differ only in what they do
/// with the answer.
fn median_of(source: &Pixmap, radius: u32) -> Pixmap {
    let width = source.width() as i32;
    let height = source.height() as i32;
    let reach = radius as i32;

    let mut out_map = source.clone();
    let stride = out_map.stride();

    out_map
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            let top = (y - reach).max(0);
            let bottom = (y + reach).min(height - 1);

            let mut hist = [[0u32; 256]; 4];
            let mut column = |hist: &mut [[u32; 256]; 4], x: i32, add: bool| {
                if x < 0 || x >= width {
                    return;
                }
                for yy in top..=bottom {
                    let line = source.row(yy as u32);
                    let i = x as usize * 4;
                    for c in 0..4 {
                        let bin = &mut hist[c][line[i + c] as usize];
                        if add {
                            *bin += 1;
                        } else {
                            *bin -= 1;
                        }
                    }
                }
            };

            for x in 0..=reach.min(width - 1) {
                column(&mut hist, x, true);
            }

            // Every column of the window holds the same number of rows, so the
            // count is the same for all four channels and is worked out once.
            for x in 0..width {
                let left = (x - reach).max(0);
                let right = (x + reach).min(width - 1);
                let total = (right - left + 1) as u32 * (bottom - top + 1) as u32;
                let target = total / 2;

                let i = x as usize * 4;
                for c in 0..4 {
                    let mut seen = 0u32;
                    let mut value = 255usize;
                    for (bin, &n) in hist[c].iter().enumerate() {
                        seen += n;
                        if seen > target {
                            value = bin;
                            break;
                        }
                    }
                    out[i + c] = value as u8;
                }

                column(&mut hist, x - reach, false);
                column(&mut hist, x + reach + 1, true);
            }
        });

    out_map
}

/// Blur flat areas while leaving edges alone — Filter ▸ Blur ▸ Surface Blur.
///
/// A neighbour only counts if it is within `threshold` of the pixel being
/// worked on, so a region blurs within itself but never across a boundary
/// into something a different colour. That is the whole difference between
/// this and a box blur, and it is why it cannot be separated into two passes
/// the way an ordinary blur can: which neighbours count depends on the centre
/// pixel, so the horizontal pass would not know what the vertical one wanted.
///
/// It is done with a **sliding histogram** rather than by visiting every
/// neighbour of every pixel. Written the obvious way this costs `(2r+1)²`
/// reads per pixel — at CS6's largest radius that is forty thousand, which on
/// a photograph is minutes of frozen application, and is how this first
/// shipped. Instead each row keeps a histogram of the window's contents and
/// slides it one column at a time, so a pixel costs `O(r)` to move the window
/// plus `O(threshold)` to add up the bins in range.
///
/// On a 2000×1500 image at threshold 15, that is the difference between 2.4s
/// and 0.04s at radius 5, and between about thirteen minutes and 0.35s at
/// radius 100.
///
/// The histogram is per channel, which means the channels also decide
/// independently whether a neighbour is near enough to count. That is what
/// Photoshop does, and it is the price of the histogram — a joint test across
/// R, G and B cannot be answered from three separate tallies.
pub fn surface_blur(pixmap: &mut Pixmap, radius: u32, threshold: u32) {
    if radius == 0 || pixmap.is_empty() {
        return;
    }
    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;
    let reach = radius as i32;
    let limit = threshold.max(1) as i32;

    let source = pixmap.clone();
    let src = &source;
    let stride = pixmap.stride();

    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            let top = (y - reach).max(0);
            let bottom = (y + reach).min(height - 1);

            // One tally of values per channel over the window, which starts
            // at the left edge and is slid rightwards a column at a time.
            let mut hist = [[0u32; 256]; 4];
            let mut add_column = |hist: &mut [[u32; 256]; 4], x: i32| {
                if x < 0 || x >= width {
                    return;
                }
                for yy in top..=bottom {
                    let row = src.row(yy as u32);
                    let i = x as usize * 4;
                    for c in 0..4 {
                        hist[c][row[i + c] as usize] += 1;
                    }
                }
            };
            let mut remove_column = |hist: &mut [[u32; 256]; 4], x: i32| {
                if x < 0 || x >= width {
                    return;
                }
                for yy in top..=bottom {
                    let row = src.row(yy as u32);
                    let i = x as usize * 4;
                    for c in 0..4 {
                        hist[c][row[i + c] as usize] -= 1;
                    }
                }
            };

            for x in 0..=reach.min(width - 1) {
                add_column(&mut hist, x);
            }

            let centre_row = src.row(y as u32);
            for x in 0..width {
                let i = x as usize * 4;
                for c in 0..4 {
                    let centre = centre_row[i + c] as i32;
                    let low = (centre - limit).max(0) as usize;
                    let high = (centre + limit).min(255) as usize;

                    // Only the bins near enough to the centre value count.
                    // The centre pixel is always one of them, so `count` is
                    // never zero and there is no division to guard.
                    let mut total = 0u32;
                    let mut count = 0u32;
                    for (value, &n) in hist[c][low..=high].iter().enumerate() {
                        total += n * (low + value) as u32;
                        count += n;
                    }
                    out[i + c] = (total / count.max(1)) as u8;
                }

                // Slide the window one to the right for the next pixel.
                remove_column(&mut hist, x - reach);
                add_column(&mut hist, x + reach + 1);
            }
        });
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

    /// A flat field with fine grain in it, plus one real edge. The two things
    /// a sharpener has to tell apart.
    fn grain_and_an_edge(size: u32) -> Pixmap {
        let mut px = Pixmap::new(size, size);
        for y in 0..size as i32 {
            for x in 0..size as i32 {
                let base = if x < size as i32 / 2 { 90.0 } else { 170.0 };
                let grain = (((x * 37 + y * 91) % 13) as f32 - 6.0) * 1.2;
                let v = (base + grain).clamp(0.0, 255.0) as u8;
                px.set(x, y, Rgba8::new(v, v, v, 255));
            }
        }
        px
    }

    /// How much fine detail an image holds, as the RMS of its Laplacian. A
    /// sharpener raises it; the question is by how much per pass.
    fn detail_energy(px: &Pixmap) -> f32 {
        let w = px.width() as i32;
        let h = px.height() as i32;
        let mut total = 0.0f64;
        let mut n = 0u32;
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let at = |dx: i32, dy: i32| px.get(x + dx, y + dy).r as f64;
                let l = 4.0 * at(0, 0) - at(-1, 0) - at(1, 0) - at(0, -1) - at(0, 1);
                total += l * l;
                n += 1;
            }
        }
        ((total / n.max(1) as f64).sqrt()) as f32
    }

    #[test]
    fn sharpening_three_times_does_not_destroy_the_image() {
        // The bug this pins: the 3×3 sharpen kernel has huge gain at the
        // finest detail an image can hold, so repeated passes multiply noise
        // rather than adding a little crispness each time. Photoshop's can be
        // applied over and over.
        let start = detail_energy(&grain_and_an_edge(64));

        let mut px = grain_and_an_edge(64);
        let mut previous = start;
        for pass in 1..=3 {
            sharpen(&mut px);
            let now = detail_energy(&px);
            let step = now / previous;
            assert!(
                step < 2.0,
                "pass {} multiplied the fine detail by {:.1}; the old 3×3 kernel did about \
                 6 on the first pass and forty by the second, which is what ruins a photograph",
                pass,
                step
            );
            previous = now;
        }
        assert!(
            previous > start,
            "three passes of Sharpen left the image no crisper than it started"
        );
    }

    #[test]
    fn sharpen_more_bites_harder_than_sharpen() {
        let mut gentle = grain_and_an_edge(64);
        sharpen(&mut gentle);
        let mut heavy = grain_and_an_edge(64);
        sharpen_more(&mut heavy);
        assert!(
            detail_energy(&heavy) > detail_energy(&gentle),
            "Sharpen More is no stronger than Sharpen"
        );
    }

    #[test]
    fn a_disc_blur_is_round_not_square() {
        // The point of the disc: a single bright pixel spreads into a circle,
        // not the square a box blur would leave. A small radius, because one
        // pixel spread over a large disc averages down below a single level
        // and there would be nothing left to measure.
        let mut px = Pixmap::filled(41, 41, Rgba8::BLACK);
        px.set(20, 20, Rgba8::WHITE);
        disc_blur(&mut px, 4);

        // Three out along a row: inside a radius of four, so it catches the
        // bright pixel.
        let side = px.get(23, 20).r;
        // Three out along both axes at once, which is 4.24 away — outside the
        // disc, though well inside the square a box blur would use.
        let corner = px.get(23, 23).r;

        assert!(side > 0, "the disc did not spread the pixel sideways at all");
        assert_eq!(corner, 0, "the blur reached into the corners, so it is a square");
    }

    #[test]
    fn a_disc_blur_conserves_what_it_spreads() {
        // A summed-area table is easy to get off by one, and the symptom is a
        // slight overall lightening or darkening rather than anything you
        // would see in the shape. Total brightness must survive.
        let mut px = Pixmap::new(64, 48);
        for y in 0..48i32 {
            for x in 0..64i32 {
                let v = ((x * 13 + y * 29) % 251) as u8;
                px.set(x, y, Rgba8::new(v, v, v, 255));
            }
        }
        let before: u64 = px.as_bytes().chunks_exact(4).map(|p| p[0] as u64).sum();
        disc_blur(&mut px, 6);
        let after: u64 = px.as_bytes().chunks_exact(4).map(|p| p[0] as u64).sum();

        let drift = (before as f64 - after as f64).abs() / before as f64;
        assert!(drift < 0.02, "the disc blur shifted total brightness by {:.1}%", drift * 100.0);
    }

    #[test]
    fn smart_sharpen_holds_back_noise_but_not_an_edge() {
        // Reduce Noise is the whole difference between this and an unsharp
        // mask. Turned up, it must leave the grain in a flat field roughly
        // where it found it while still working the edge.
        let before = grain_and_an_edge(64);

        let mut quiet = before.clone();
        smart_sharpen(&mut quiet, 200.0, 1.0, 100.0, SharpenRemove::GaussianBlur, 0.0);
        let mut noisy = before.clone();
        smart_sharpen(&mut noisy, 200.0, 1.0, 0.0, SharpenRemove::GaussianBlur, 0.0);

        let disturbance = |after: &Pixmap, x0: i32, x1: i32| -> i64 {
            let mut total = 0i64;
            for y in 4..60 {
                for x in x0..x1 {
                    total += (after.get(x, y).r as i64 - before.get(x, y).r as i64).abs();
                }
            }
            total
        };

        let flat_quiet = disturbance(&quiet, 4, 20);
        let flat_noisy = disturbance(&noisy, 4, 20);
        assert!(
            flat_quiet * 2 < flat_noisy,
            "Reduce Noise at full did nothing to the grain: {} vs {}",
            flat_quiet,
            flat_noisy
        );
        assert!(
            disturbance(&quiet, 30, 35) > 0,
            "Reduce Noise at full also stopped it sharpening the edge"
        );
    }

    #[test]
    fn what_smart_sharpen_removes_changes_the_result() {
        // The Remove list is the other thing that makes it smart, and an
        // implementation that quietly used a Gaussian whatever was chosen
        // would look perfectly reasonable.
        let source = grain_and_an_edge(64);
        let run = |remove, angle| {
            let mut px = source.clone();
            smart_sharpen(&mut px, 200.0, 3.0, 0.0, remove, angle);
            px
        };

        let gaussian = run(SharpenRemove::GaussianBlur, 0.0);
        let lens = run(SharpenRemove::LensBlur, 0.0);
        let motion = run(SharpenRemove::MotionBlur, 0.0);

        assert_ne!(gaussian.as_bytes(), lens.as_bytes(), "Lens Blur behaved as a Gaussian");
        assert_ne!(gaussian.as_bytes(), motion.as_bytes(), "Motion Blur behaved as a Gaussian");

        // ...and the motion streak has to follow the angle it is given: the
        // edge here runs down the image, so sharpening against a horizontal
        // streak crosses it and a vertical one runs along it.
        let across = run(SharpenRemove::MotionBlur, 0.0);
        let along = run(SharpenRemove::MotionBlur, 90.0);
        assert_ne!(across.as_bytes(), along.as_bytes(), "the motion angle was ignored");
    }

    #[test]
    fn sharpen_edges_leaves_flat_grain_alone() {
        // The whole point of it: crisp the boundary, do not crisp the noise.
        // Measured as what each filter did on the flat side of the field
        // against what it did across the edge.
        let before = grain_and_an_edge(64);

        let mut edges = before.clone();
        sharpen_edges(&mut edges);
        let mut plain = before.clone();
        sharpen(&mut plain);

        let disturbance = |after: &Pixmap, x0: i32, x1: i32| -> i64 {
            let mut total = 0i64;
            for y in 4..60 {
                for x in x0..x1 {
                    total += (after.get(x, y).r as i64 - before.get(x, y).r as i64).abs();
                }
            }
            total
        };

        // Well away from the boundary, which sits at x = 32.
        let flat_edges = disturbance(&edges, 4, 20);
        let flat_plain = disturbance(&plain, 4, 20);
        assert!(
            flat_edges * 4 < flat_plain,
            "Sharpen Edges worked the flat grain nearly as hard as Sharpen did: {} vs {}",
            flat_edges,
            flat_plain
        );

        // ...but it must still be doing something where the edge is.
        let across = disturbance(&edges, 30, 35);
        assert!(across > 0, "Sharpen Edges did not touch the edge either");
    }

    #[test]
    fn a_median_removes_a_speck_a_mean_would_only_spread() {
        // The whole reason this filter exists. One wrong pixel among a few
        // hundred cannot move the middle value at all, where an average would
        // smear it over its neighbourhood.
        let mut px = Pixmap::filled(21, 21, Rgba8::new(120, 120, 120, 255));
        px.set(10, 10, Rgba8::WHITE);

        let mut blurred = px.clone();
        box_blur(&mut blurred, 3);
        assert_ne!(
            blurred.get(11, 10).r,
            120,
            "a box blur should have spread the speck to its neighbour"
        );

        median_filter(&mut px, 3);
        assert_eq!(px.get(10, 10).r, 120, "the median did not remove the speck");
        assert_eq!(px.get(11, 10).r, 120, "the median spread the speck instead");
    }

    #[test]
    fn the_median_agrees_with_sorting_the_neighbourhood() {
        // The sliding window is the risk here, exactly as it is in Surface
        // Blur: a column dropped a step early still gives a plausible median,
        // just the wrong one, and only in some columns.
        fn directly(src: &Pixmap, radius: i32) -> Pixmap {
            let mut out = src.clone();
            let w = src.width() as i32;
            let h = src.height() as i32;
            for y in 0..h {
                for x in 0..w {
                    for c in 0..4 {
                        let mut window = Vec::new();
                        for yy in (y - radius).max(0)..=(y + radius).min(h - 1) {
                            for xx in (x - radius).max(0)..=(x + radius).min(w - 1) {
                                let p = src.get(xx, yy);
                                window.push([p.r, p.g, p.b, p.a][c]);
                            }
                        }
                        window.sort_unstable();
                        let mut p = out.get(x, y);
                        let middle = window[window.len() / 2];
                        match c {
                            0 => p.r = middle,
                            1 => p.g = middle,
                            2 => p.b = middle,
                            _ => p.a = middle,
                        }
                        out.set(x, y, p);
                    }
                }
            }
            out
        }

        // Not square, and not a multiple of any radius used.
        let mut src = Pixmap::new(29, 19);
        for y in 0..19i32 {
            for x in 0..29i32 {
                let v = ((x * 17 + y * 43) % 251) as u8;
                src.set(x, y, Rgba8::new(v, 255 - v, v / 3, 190 + (v % 66)));
            }
        }

        for radius in [1u32, 2, 5, 25] {
            let mut fast = src.clone();
            median_filter(&mut fast, radius);
            assert_eq!(
                fast.as_bytes(),
                directly(&src, radius as i32).as_bytes(),
                "the sliding window disagrees with sorting the neighbourhood at radius {}",
                radius
            );
        }
    }

    #[test]
    fn dust_and_scratches_keeps_what_is_within_its_threshold() {
        // The threshold is what separates it from Median: at zero it is the
        // median exactly, and raising it hands more of the picture back.
        let mut speckled = Pixmap::filled(21, 21, Rgba8::new(120, 120, 120, 255));
        speckled.set(10, 10, Rgba8::WHITE);
        // A quieter blemish, well within a threshold of 60.
        speckled.set(5, 5, Rgba8::new(150, 150, 150, 255));

        let mut wide = speckled.clone();
        dust_and_scratches(&mut wide, 3, 60);
        assert_eq!(wide.get(10, 10).r, 120, "the speck survived a wide threshold");
        assert_eq!(
            wide.get(5, 5).r,
            150,
            "a difference inside the threshold was removed anyway"
        );

        let mut none = speckled.clone();
        dust_and_scratches(&mut none, 3, 0);
        let mut median = speckled.clone();
        median_filter(&mut median, 3);
        assert_eq!(
            none.as_bytes(),
            median.as_bytes(),
            "at threshold zero this should be exactly Median"
        );
    }

    #[test]
    fn the_sliding_histogram_agrees_with_visiting_every_neighbour() {
        // Surface Blur slides a tally of the window's contents sideways
        // rather than re-reading every neighbour, which is what makes a large
        // radius affordable at all. A window slid wrong — a column dropped one
        // step early, or one added twice at an edge — still produces a
        // plausible blur, just not the right one, and only in some columns.
        // So it is checked against the definition it is an optimisation of.
        fn directly(src: &Pixmap, radius: i32, threshold: i32) -> Pixmap {
            let mut out = src.clone();
            let w = src.width() as i32;
            let h = src.height() as i32;
            for y in 0..h {
                for x in 0..w {
                    let centre = src.get(x, y);
                    let centre = [centre.r, centre.g, centre.b, centre.a];
                    let mut totals = [0u32; 4];
                    let mut counts = [0u32; 4];
                    for yy in (y - radius).max(0)..=(y + radius).min(h - 1) {
                        for xx in (x - radius).max(0)..=(x + radius).min(w - 1) {
                            let p = src.get(xx, yy);
                            for (c, value) in [p.r, p.g, p.b, p.a].into_iter().enumerate() {
                                if (value as i32 - centre[c] as i32).abs() <= threshold {
                                    totals[c] += value as u32;
                                    counts[c] += 1;
                                }
                            }
                        }
                    }
                    let v = |c: usize| (totals[c] / counts[c].max(1)) as u8;
                    out.set(x, y, Rgba8::new(v(0), v(1), v(2), v(3)));
                }
            }
            out
        }

        // Not square, and not a multiple of the radius, so an off-by-one at
        // either edge shows up.
        let mut src = Pixmap::new(37, 23);
        for y in 0..23i32 {
            for x in 0..37i32 {
                let v = ((x * 11 + y * 29) % 251) as u8;
                src.set(x, y, Rgba8::new(v, 255 - v, v / 2, 200 + (v % 56)));
            }
        }

        for (radius, threshold) in [(1u32, 10u32), (3, 30), (8, 60), (40, 20)] {
            let mut fast = src.clone();
            surface_blur(&mut fast, radius, threshold);
            let slow = directly(&src, radius as i32, threshold as i32);
            assert_eq!(
                fast.as_bytes(),
                slow.as_bytes(),
                "the sliding window disagrees with visiting every neighbour at radius {} \
                 threshold {}",
                radius,
                threshold
            );
        }
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

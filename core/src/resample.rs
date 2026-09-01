//! Scaling a [`Pixmap`] to a new pixel size.
//!
//! Backs Image ▸ Image Size. Everything here works on **premultiplied**
//! colour and converts back afterwards: filtering straight alpha lets the
//! colour of fully transparent pixels bleed into visible ones, which shows up
//! as dark halos around soft edges — the same reason `convolve` premultiplies
//! before blurring.

use crate::buffer::{Pixmap, Rgba8};

/// How pixels are interpolated when the size changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resample {
    /// No interpolation. Keeps hard edges crisp, which is what pixel art and
    /// screenshots want; everything else it makes jagged.
    Nearest,
    /// Linear blend of the four neighbours.
    Bilinear,
    /// Catmull-Rom over sixteen neighbours. Slightly sharper than bilinear on
    /// enlargement, which is what makes it the usual default for photographs.
    Bicubic,
}

impl Resample {
    /// Map a CS6 Resample menu index onto what the engine implements.
    ///
    /// Photoshop offers seven entries, several of which differ only in how
    /// much sharpening they add on top. They collapse onto the three
    /// interpolators here rather than pretending to a distinction the engine
    /// does not yet make.
    pub fn from_i32(v: i32) -> Resample {
        match v {
            // Nearest Neighbor
            5 => Resample::Nearest,
            // Bilinear
            6 => Resample::Bilinear,
            // Automatic, Preserve Details, Bicubic Smoother/Sharper/Smooth.
            _ => Resample::Bicubic,
        }
    }
}

/// Scale `src` to `width` x `height`.
///
/// Returns an empty pixmap for a zero dimension, and a plain clone when the
/// size is unchanged, so callers do not have to special-case either.
pub fn resample(src: &Pixmap, width: u32, height: u32, mode: Resample) -> Pixmap {
    if width == 0 || height == 0 {
        return Pixmap::new(width.max(1), height.max(1));
    }
    if src.width() == width && src.height() == height {
        return src.clone();
    }
    if src.is_empty() {
        return Pixmap::new(width, height);
    }

    let mut premultiplied = src.clone();
    premultiplied.premultiply();

    // Shrinking by more than half leaves interpolation reading only a fraction
    // of the source pixels, so detail turns into aliasing. Averaging the whole
    // source footprint of each destination pixel is what stops a downscale
    // coming out speckled — it is the box filter Photoshop applies for
    // reduction.
    let shrinking = width * 2 <= src.width() || height * 2 <= src.height();

    let mut out = Pixmap::new(width, height);
    let sx_scale = src.width() as f32 / width as f32;
    let sy_scale = src.height() as f32 / height as f32;

    for y in 0..height {
        for x in 0..width {
            let px = if mode == Resample::Nearest {
                let sx = ((x as f32 + 0.5) * sx_scale).floor() as i32;
                let sy = ((y as f32 + 0.5) * sy_scale).floor() as i32;
                sample_clamped(&premultiplied, sx, sy)
            } else if shrinking {
                area_average(&premultiplied, x, y, sx_scale, sy_scale)
            } else {
                // Half-pixel offsets put the sample at the centre of the
                // destination pixel; without them the image creeps by half a
                // pixel each time it is scaled.
                let fx = (x as f32 + 0.5) * sx_scale - 0.5;
                let fy = (y as f32 + 0.5) * sy_scale - 0.5;
                match mode {
                    Resample::Bilinear => bilinear(&premultiplied, fx, fy),
                    _ => bicubic(&premultiplied, fx, fy),
                }
            };
            out.set(x as i32, y as i32, px);
        }
    }

    out.unpremultiply();
    out
}

/// Read a pixel, clamping to the edge rather than reading nothing.
#[inline]
fn sample_clamped(src: &Pixmap, x: i32, y: i32) -> Rgba8 {
    let cx = x.clamp(0, src.width() as i32 - 1);
    let cy = y.clamp(0, src.height() as i32 - 1);
    src.get(cx, cy)
}

/// Mean of every source pixel covered by one destination pixel.
fn area_average(src: &Pixmap, x: u32, y: u32, sx_scale: f32, sy_scale: f32) -> Rgba8 {
    let x0 = (x as f32 * sx_scale).floor() as i32;
    let x1 = (((x + 1) as f32 * sx_scale).ceil() as i32).max(x0 + 1);
    let y0 = (y as f32 * sy_scale).floor() as i32;
    let y1 = (((y + 1) as f32 * sy_scale).ceil() as i32).max(y0 + 1);

    let mut acc = [0.0f32; 4];
    let mut n = 0.0f32;
    for sy in y0..y1 {
        for sx in x0..x1 {
            let p = sample_clamped(src, sx, sy);
            acc[0] += p.r as f32;
            acc[1] += p.g as f32;
            acc[2] += p.b as f32;
            acc[3] += p.a as f32;
            n += 1.0;
        }
    }
    if n == 0.0 {
        return Rgba8::TRANSPARENT;
    }
    Rgba8::new(
        to_u8(acc[0] / n),
        to_u8(acc[1] / n),
        to_u8(acc[2] / n),
        to_u8(acc[3] / n),
    )
}

fn bilinear(src: &Pixmap, fx: f32, fy: f32) -> Rgba8 {
    let x0 = fx.floor() as i32;
    let y0 = fy.floor() as i32;
    let tx = fx - x0 as f32;
    let ty = fy - y0 as f32;

    let mut acc = [0.0f32; 4];
    for (dy, wy) in [(0, 1.0 - ty), (1, ty)] {
        for (dx, wx) in [(0, 1.0 - tx), (1, tx)] {
            let p = sample_clamped(src, x0 + dx, y0 + dy);
            let w = wx * wy;
            acc[0] += p.r as f32 * w;
            acc[1] += p.g as f32 * w;
            acc[2] += p.b as f32 * w;
            acc[3] += p.a as f32 * w;
        }
    }
    Rgba8::new(to_u8(acc[0]), to_u8(acc[1]), to_u8(acc[2]), to_u8(acc[3]))
}

/// Catmull-Rom weight for one axis.
#[inline]
fn catmull_rom(t: f32) -> f32 {
    let a = t.abs();
    if a <= 1.0 {
        1.5 * a * a * a - 2.5 * a * a + 1.0
    } else if a < 2.0 {
        -0.5 * a * a * a + 2.5 * a * a - 4.0 * a + 2.0
    } else {
        0.0
    }
}

fn bicubic(src: &Pixmap, fx: f32, fy: f32) -> Rgba8 {
    let x0 = fx.floor() as i32;
    let y0 = fy.floor() as i32;
    let tx = fx - x0 as f32;
    let ty = fy - y0 as f32;

    let mut acc = [0.0f32; 4];
    let mut total = 0.0f32;
    for dy in -1..=2 {
        let wy = catmull_rom(dy as f32 - ty);
        if wy == 0.0 {
            continue;
        }
        for dx in -1..=2 {
            let wx = catmull_rom(dx as f32 - tx);
            if wx == 0.0 {
                continue;
            }
            let p = sample_clamped(src, x0 + dx, y0 + dy);
            let w = wx * wy;
            acc[0] += p.r as f32 * w;
            acc[1] += p.g as f32 * w;
            acc[2] += p.b as f32 * w;
            acc[3] += p.a as f32 * w;
            total += w;
        }
    }
    // Catmull-Rom overshoots, so the weights are renormalised and the result
    // clamped; without this a bright edge grows a dark ring beside it.
    if total.abs() > 1e-6 {
        for v in acc.iter_mut() {
            *v /= total;
        }
    }
    Rgba8::new(to_u8(acc[0]), to_u8(acc[1]), to_u8(acc[2]), to_u8(acc[3]))
}

#[inline]
fn to_u8(v: f32) -> u8 {
    (v + 0.5).clamp(0.0, 255.0) as u8
}

/// Scale an 8-bit coverage mask — a selection or a layer mask.
pub fn resample_coverage(
    src: &[u8],
    src_width: u32,
    src_height: u32,
    width: u32,
    height: u32,
) -> Vec<u8> {
    if width == 0 || height == 0 || src_width == 0 || src_height == 0 {
        return vec![0; (width as usize) * (height as usize)];
    }
    let mut out = vec![0u8; (width as usize) * (height as usize)];
    let sx_scale = src_width as f32 / width as f32;
    let sy_scale = src_height as f32 / height as f32;

    for y in 0..height {
        for x in 0..width {
            // Averaged rather than point-sampled, so a feathered edge stays
            // feathered instead of turning into a staircase.
            let x0 = (x as f32 * sx_scale).floor() as u32;
            let x1 = (((x + 1) as f32 * sx_scale).ceil() as u32)
                .max(x0 + 1)
                .min(src_width);
            let y0 = (y as f32 * sy_scale).floor() as u32;
            let y1 = (((y + 1) as f32 * sy_scale).ceil() as u32)
                .max(y0 + 1)
                .min(src_height);

            let mut sum = 0u32;
            let mut n = 0u32;
            for sy in y0..y1.max(y0 + 1) {
                for sx in x0..x1.max(x0 + 1) {
                    let i = (sy.min(src_height - 1) * src_width + sx.min(src_width - 1)) as usize;
                    sum += src[i] as u32;
                    n += 1;
                }
            }
            out[(y * width + x) as usize] = if n == 0 { 0 } else { (sum / n) as u8 };
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled(w: u32, h: u32, c: Rgba8) -> Pixmap {
        Pixmap::filled(w, h, c)
    }

    #[test]
    fn an_unchanged_size_is_returned_verbatim() {
        let src = filled(4, 4, Rgba8::new(10, 20, 30, 255));
        for mode in [Resample::Nearest, Resample::Bilinear, Resample::Bicubic] {
            let out = resample(&src, 4, 4, mode);
            assert_eq!(out.as_bytes(), src.as_bytes(), "{mode:?}");
        }
    }

    #[test]
    fn a_flat_image_stays_flat_at_any_size() {
        // The cheapest check that the filter weights sum to one: interpolating
        // a constant must give that constant back, not a gradient.
        let src = filled(8, 8, Rgba8::new(70, 140, 210, 255));
        for mode in [Resample::Nearest, Resample::Bilinear, Resample::Bicubic] {
            for (w, h) in [(3u32, 3u32), (16, 16), (8, 20)] {
                let out = resample(&src, w, h, mode);
                assert_eq!(out.width(), w);
                assert_eq!(out.height(), h);
                for px in out.as_bytes().chunks_exact(4) {
                    assert_eq!(
                        (px[0], px[1], px[2], px[3]),
                        (70, 140, 210, 255),
                        "{mode:?} at {w}x{h}",
                    );
                }
            }
        }
    }

    #[test]
    fn nearest_keeps_the_original_values() {
        // Nearest must not invent intermediate colours, whatever the ratio.
        let mut src = Pixmap::new(2, 1);
        src.set(0, 0, Rgba8::new(0, 0, 0, 255));
        src.set(1, 0, Rgba8::new(255, 255, 255, 255));

        let out = resample(&src, 6, 1, Resample::Nearest);
        for x in 0..6 {
            let v = out.get(x, 0).r;
            assert!(v == 0 || v == 255, "invented value {v} at x={x}");
        }
    }

    #[test]
    fn downscaling_averages_rather_than_dropping_pixels() {
        // A checkerboard halved should go grey. Point-sampling would pick one
        // phase and come back pure black or pure white.
        let mut src = Pixmap::new(8, 8);
        for y in 0..8 {
            for x in 0..8 {
                let v = if (x + y) % 2 == 0 { 0 } else { 255 };
                src.set(x, y, Rgba8::new(v, v, v, 255));
            }
        }
        let out = resample(&src, 2, 2, Resample::Bicubic);
        for y in 0..2 {
            for x in 0..2 {
                let v = out.get(x, y).r;
                assert!((100..=155).contains(&v), "expected mid-grey, got {v}");
            }
        }
    }

    #[test]
    fn transparent_edges_do_not_bleed_dark() {
        // A red pixel beside a transparent one, enlarged. Filtering straight
        // alpha would drag the transparent pixel's black into the red; on
        // premultiplied colour the hue holds and only alpha falls off.
        let mut src = Pixmap::new(2, 1);
        src.set(0, 0, Rgba8::new(255, 0, 0, 255));
        src.set(1, 0, Rgba8::TRANSPARENT);

        let out = resample(&src, 8, 1, Resample::Bilinear);
        for x in 0..8 {
            let p = out.get(x, 0);
            if p.a > 10 {
                assert!(p.r > 200, "red darkened to {p:?} at x={x}");
                assert!(p.g < 40 && p.b < 40, "colour bled at x={x}: {p:?}");
            }
        }
    }

    #[test]
    fn coverage_scales_and_keeps_its_range() {
        let src = vec![0u8, 255, 0, 255];
        let out = resample_coverage(&src, 2, 2, 4, 4);
        assert_eq!(out.len(), 16);
        assert!(out.iter().any(|&v| v > 0), "coverage vanished");
        assert!(out.iter().all(|&v| v <= 255));
    }
}

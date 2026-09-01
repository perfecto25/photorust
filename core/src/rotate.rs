//! Turning a [`Pixmap`] by an arbitrary angle.
//!
//! Backs Image ▸ Image Rotation ▸ Arbitrary. Right angles and flips only
//! permute pixels that already exist and go through [`Pixmap::transformed`];
//! this module is for the angles in between, which have to resample.
//!
//! Sampling happens on **premultiplied** colour, for the reason spelled out in
//! [`crate::resample`]: filtering straight alpha lets the colour of fully
//! transparent pixels bleed into visible ones, which shows up as a dark fringe
//! along the new diagonal edges. Unlike `resample`, the premultiplication is
//! done per sample rather than by calling [`Pixmap::premultiply`], so pixmaps
//! deeper than 8 bits per channel are not disturbed.
//!
//! **Not a GPU candidate** (CLAUDE.md §7 step 2). The arithmetic per pixel is
//! a gather and a lerp — trivial next to the cost of moving the layer to the
//! device — and the result is wanted straight back on the CPU to become the
//! layer's new pixels. It is also a once-per-menu-click operation, not
//! something in a hot path.

use crate::buffer::{Pixmap, Rgba8};

/// The size a `width` x `height` rectangle covers once turned by `degrees`.
///
/// This is the tight bounding box of the turned rectangle, which is how big
/// the canvas has to become for Arbitrary rotation not to cut the corners off.
pub fn rotated_bounds(width: u32, height: u32, degrees: f32) -> (u32, u32) {
    let (sin, cos) = degrees.to_radians().sin_cos();
    // cos(90°) comes out as 4e-8 rather than 0, and one ceil() later that is a
    // whole extra column of transparent pixels on a quarter turn.
    let snap = |v: f32| if v.abs() < 1e-6 { 0.0 } else { v.abs() };
    let (sin, cos) = (snap(sin), snap(cos));
    let (w, h) = (width as f32, height as f32);
    let out_w = (w * cos + h * sin).ceil().max(1.0);
    let out_h = (w * sin + h * cos).ceil().max(1.0);
    (out_w as u32, out_h as u32)
}

/// Rotate `src` clockwise by `degrees` about its own centre.
///
/// The result is the tight bounding box of the turned rectangle, so at any
/// angle off a right angle it is larger than the source and the new corners
/// are transparent. The centre of the image stays the centre of the image,
/// which is what lets the caller place the result by moving that one point.
pub fn rotate(src: &Pixmap, degrees: f32) -> Pixmap {
    use crate::metadata::Orientation;

    let degrees = degrees.rem_euclid(360.0);
    if src.is_empty() {
        return src.clone();
    }
    // The right angles are a permutation of the existing pixels. Taking them
    // through the sampler instead would soften an image that need not lose a
    // thing.
    if degrees == 0.0 {
        return src.clone();
    }
    if degrees == 90.0 {
        return src.transformed(Orientation::Rotate90Cw);
    }
    if degrees == 180.0 {
        return src.transformed(Orientation::Rotate180);
    }
    if degrees == 270.0 {
        return src.transformed(Orientation::Rotate90Ccw);
    }

    let (out_w, out_h) = rotated_bounds(src.width(), src.height(), degrees);
    let (sin, cos) = degrees.to_radians().sin_cos();

    let scx = src.width() as f32 / 2.0;
    let scy = src.height() as f32 / 2.0;
    let dcx = out_w as f32 / 2.0;
    let dcy = out_h as f32 / 2.0;

    let mut out = Pixmap::new_with_depth(out_w, out_h, src.bpc());
    for y in 0..out_h {
        for x in 0..out_w {
            // Walk backwards, from the destination to the source: every output
            // pixel then gets exactly one source to read, which mapping the
            // other way cannot promise — it leaves holes.
            let dx = x as f32 + 0.5 - dcx;
            let dy = y as f32 + 0.5 - dcy;
            let sx = dx * cos + dy * sin + scx;
            let sy = -dx * sin + dy * cos + scy;
            out.set(x as i32, y as i32, bilinear(src, sx - 0.5, sy - 0.5));
        }
    }
    out
}

/// Blend of the four neighbours around `(x, y)`, in premultiplied colour.
///
/// Anything off the edge of `src` reads as transparent, which is what gives
/// the turned image a soft border rather than a stair-stepped one.
fn bilinear(src: &Pixmap, x: f32, y: f32) -> Rgba8 {
    let x0 = x.floor();
    let y0 = y.floor();
    let fx = x - x0;
    let fy = y - y0;
    let x0 = x0 as i32;
    let y0 = y0 as i32;

    let mut acc = [0.0f32; 4];
    for (dx, dy, weight) in [
        (0, 0, (1.0 - fx) * (1.0 - fy)),
        (1, 0, fx * (1.0 - fy)),
        (0, 1, (1.0 - fx) * fy),
        (1, 1, fx * fy),
    ] {
        if weight == 0.0 {
            continue;
        }
        let px = src.get(x0 + dx, y0 + dy);
        let alpha = px.a as f32 / 255.0;
        acc[0] += px.r as f32 * alpha * weight;
        acc[1] += px.g as f32 * alpha * weight;
        acc[2] += px.b as f32 * alpha * weight;
        acc[3] += px.a as f32 * weight;
    }

    if acc[3] <= 0.0 {
        return Rgba8::TRANSPARENT;
    }
    // Back to straight alpha.
    let scale = 255.0 / acc[3];
    let channel = |v: f32| (v * scale + 0.5).clamp(0.0, 255.0) as u8;
    Rgba8::new(
        channel(acc[0]),
        channel(acc[1]),
        channel(acc[2]),
        (acc[3] + 0.5).clamp(0.0, 255.0) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker(w: u32, h: u32) -> Pixmap {
        let mut px = Pixmap::new(w, h);
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let v = if (x + y) % 2 == 0 { 255 } else { 40 };
                px.set(x, y, Rgba8::new(v, v / 2, 0, 255));
            }
        }
        px
    }

    #[test]
    fn a_right_angle_is_exact_not_resampled() {
        let src = checker(5, 3);
        let turned = rotate(&src, 90.0);
        assert_eq!((turned.width(), turned.height()), (3, 5));
        for y in 0..3i32 {
            for x in 0..5i32 {
                // (x, y) lands at (h - 1 - y, x) under a clockwise quarter turn.
                assert_eq!(turned.get(2 - y, x), src.get(x, y));
            }
        }
    }

    #[test]
    fn a_full_turn_changes_nothing() {
        let src = checker(6, 4);
        let turned = rotate(&src, 360.0);
        assert_eq!(turned.as_bytes(), src.as_bytes());
    }

    #[test]
    fn the_bounding_box_grows_to_hold_the_corners() {
        // A square turned 45° needs room for its diagonal.
        let (w, h) = rotated_bounds(100, 100, 45.0);
        let diagonal = (100.0f32 * 2.0f32.sqrt()).ceil() as u32;
        assert_eq!((w, h), (diagonal, diagonal));

        // A right angle swaps the sides and adds nothing.
        assert_eq!(rotated_bounds(80, 20, 90.0), (20, 80));
        assert_eq!(rotated_bounds(80, 20, 180.0), (80, 20));
    }

    #[test]
    fn the_new_corners_are_transparent() {
        let src = Pixmap::filled(40, 40, Rgba8::opaque(200, 100, 50));
        let turned = rotate(&src, 30.0);
        assert_eq!(turned.get(0, 0).a, 0);
        let (w, h) = (turned.width() as i32, turned.height() as i32);
        assert_eq!(turned.get(w - 1, h - 1).a, 0);
        // ...while the middle is untouched.
        assert_eq!(turned.get(w / 2, h / 2), Rgba8::opaque(200, 100, 50));
    }

    #[test]
    fn transparent_pixels_do_not_bleed_their_colour() {
        // A transparent black border around an opaque white square: sampling
        // straight alpha would drag that black into the edge and leave a dark
        // fringe. Premultiplied, the edge stays white and only fades out.
        let mut src = Pixmap::new(20, 20);
        for y in 4..16i32 {
            for x in 4..16i32 {
                src.set(x, y, Rgba8::WHITE);
            }
        }
        let turned = rotate(&src, 17.0);
        for y in 0..turned.height() as i32 {
            for x in 0..turned.width() as i32 {
                let px = turned.get(x, y);
                if px.a > 0 {
                    assert_eq!(
                        (px.r, px.g, px.b),
                        (255, 255, 255),
                        "pixel at {x},{y} picked up colour from transparent neighbours"
                    );
                }
            }
        }
    }

    #[test]
    fn turning_by_a_negative_angle_matches_the_other_way_round() {
        let src = checker(9, 5);
        let left = rotate(&src, -90.0);
        let right = rotate(&src, 270.0);
        assert_eq!(left.as_bytes(), right.as_bytes());
    }

    #[test]
    fn depth_survives_the_turn() {
        let src = Pixmap::new_with_depth(16, 16, 2);
        assert_eq!(rotate(&src, 33.0).bpc(), 2);
    }
}

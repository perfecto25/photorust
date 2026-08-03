//! Perspective correction — the maths behind the Perspective Crop tool.
//!
//! The user marks the four corners of something that *should* be rectangular
//! but was photographed at an angle: a page, a painting, a building face. The
//! tool then maps that quadrilateral onto a rectangle, which straightens the
//! subject and crops to it in one step.
//!
//! The mapping is a **homography**: the general 8-degree-of-freedom projective
//! transform, which is exactly what a change of camera viewpoint does to a
//! plane. Four point correspondences determine it uniquely, and four corners is
//! precisely what the tool collects.

use crate::buffer::{Pixmap, Rgba8};

/// A projective plane-to-plane map.
///
/// Stored as the eight free coefficients of the 3×3 matrix; the ninth is fixed
/// at 1, which is the usual normalisation and costs no generality.
#[derive(Clone, Copy, Debug)]
pub struct Homography {
    /// `[a, b, c, d, e, f, g, h]` for
    /// `x' = (a·x + b·y + c) / (g·x + h·y + 1)` and
    /// `y' = (d·x + e·y + f) / (g·x + h·y + 1)`.
    m: [f64; 8],
}

impl Homography {
    /// The map taking each `from[i]` to the corresponding `to[i]`.
    ///
    /// Returns `None` for a degenerate correspondence — three collinear
    /// corners, or a quad collapsed to a line — where no such map exists.
    pub fn from_quads(from: &[(f32, f32); 4], to: &[(f32, f32); 4]) -> Option<Homography> {
        // Two rows per correspondence, from
        //   x'·(g·x + h·y + 1) = a·x + b·y + c
        //   y'·(g·x + h·y + 1) = d·x + e·y + f
        // rearranged so the unknowns are on the left.
        let mut a = [[0.0f64; 9]; 8];
        for i in 0..4 {
            let (x, y) = (from[i].0 as f64, from[i].1 as f64);
            let (u, v) = (to[i].0 as f64, to[i].1 as f64);

            a[i * 2] = [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y, u];
            a[i * 2 + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y, v];
        }
        solve8(&mut a).map(|m| Homography { m })
    }

    /// Apply the map to a point.
    pub fn map(&self, x: f32, y: f32) -> (f32, f32) {
        let [a, b, c, d, e, f, g, h] = self.m;
        let (x, y) = (x as f64, y as f64);
        let w = g * x + h * y + 1.0;
        // A point on the horizon maps to infinity. Pushing it far off-canvas
        // rather than returning a NaN keeps every caller's bounds check
        // working without a special case.
        if w.abs() < 1e-12 {
            return (f32::MAX, f32::MAX);
        }
        (
            ((a * x + b * y + c) / w) as f32,
            ((d * x + e * y + f) / w) as f32,
        )
    }
}

/// Gaussian elimination with partial pivoting on an 8×9 augmented matrix.
///
/// `None` when the system is singular, which is how a degenerate quad shows up.
fn solve8(a: &mut [[f64; 9]; 8]) -> Option<[f64; 8]> {
    for col in 0..8 {
        // Pivot on the largest remaining magnitude in this column, for
        // numerical stability — corner coordinates differ by orders of
        // magnitude between a thumbnail and a full-size photo.
        let mut pivot = col;
        for row in (col + 1)..8 {
            if a[row][col].abs() > a[pivot][col].abs() {
                pivot = row;
            }
        }
        if a[pivot][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, pivot);

        let lead = a[col][col];
        for k in col..9 {
            a[col][k] /= lead;
        }
        for row in 0..8 {
            if row == col {
                continue;
            }
            let factor = a[row][col];
            if factor == 0.0 {
                continue;
            }
            for k in col..9 {
                a[row][k] -= factor * a[col][k];
            }
        }
    }

    let mut out = [0.0f64; 8];
    for (i, v) in out.iter_mut().enumerate() {
        *v = a[i][8];
        if !v.is_finite() {
            return None;
        }
    }
    Some(out)
}

/// A sensible output size for a quad, before any user override.
///
/// The longer of each pair of opposite edges, which keeps the detail from the
/// side of the subject nearest the camera rather than averaging it away.
pub fn suggested_size(quad: &[(f32, f32); 4]) -> (u32, u32) {
    let edge = |a: (f32, f32), b: (f32, f32)| ((b.0 - a.0).hypot(b.1 - a.1)) as f64;

    // Corners are ordered top-left, top-right, bottom-right, bottom-left.
    let width = edge(quad[0], quad[1]).max(edge(quad[3], quad[2]));
    let height = edge(quad[0], quad[3]).max(edge(quad[1], quad[2]));

    (
        (width.round() as u32).clamp(1, 30_000),
        (height.round() as u32).clamp(1, 30_000),
    )
}

/// The map from destination pixel coordinates back to the source quad.
///
/// Warping is done by inverse mapping — walking the *output* and asking where
/// each pixel came from — because that fills every output pixel exactly once.
/// Mapping forward would leave holes wherever the transform stretches.
pub fn inverse_map(quad: &[(f32, f32); 4], width: u32, height: u32) -> Option<Homography> {
    let w = width as f32;
    let h = height as f32;
    let dest = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)];
    Homography::from_quads(&dest, quad)
}

/// Bilinear sample, in document coordinates, of a pixmap placed at `offset`.
///
/// Interpolation runs on **premultiplied** colour and is unpremultiplied on the
/// way out. Averaging straight colour across an alpha edge would pull the
/// invisible colour of transparent pixels into the visible result — the classic
/// dark halo around a warped cut-out.
fn sample(pixels: &Pixmap, offset: (i32, i32), x: f32, y: f32) -> Rgba8 {
    // Bilinear weights treat integer coordinates as pixel *centres*, but `x`
    // and `y` are continuous positions where the centre of pixel 0 sits at
    // 0.5. Without this half-pixel shift an identity transform still smears
    // every pixel across its four neighbours.
    let lx = x - offset.0 as f32 - 0.5;
    let ly = y - offset.1 as f32 - 0.5;

    let x0 = lx.floor();
    let y0 = ly.floor();
    let fx = lx - x0;
    let fy = ly - y0;
    let (x0, y0) = (x0 as i32, y0 as i32);

    // Reject anything more than a pixel outside; `Pixmap::get` reads
    // transparent beyond its edge, so the border interpolates to nothing.
    if x0 < -1 || y0 < -1 || x0 > pixels.width() as i32 || y0 > pixels.height() as i32 {
        return Rgba8::TRANSPARENT;
    }

    let mut acc = [0.0f32; 4];
    let corners = [
        (x0, y0, (1.0 - fx) * (1.0 - fy)),
        (x0 + 1, y0, fx * (1.0 - fy)),
        (x0, y0 + 1, (1.0 - fx) * fy),
        (x0 + 1, y0 + 1, fx * fy),
    ];
    for (cx, cy, weight) in corners {
        if weight <= 0.0 {
            continue;
        }
        let px = pixels.get(cx, cy);
        let a = px.a as f32 / 255.0;
        acc[0] += px.r as f32 * a * weight;
        acc[1] += px.g as f32 * a * weight;
        acc[2] += px.b as f32 * a * weight;
        acc[3] += px.a as f32 * weight;
    }

    if acc[3] <= 0.0 {
        return Rgba8::TRANSPARENT;
    }
    let inv = 255.0 / acc[3];
    Rgba8::new(
        (acc[0] * inv).round().clamp(0.0, 255.0) as u8,
        (acc[1] * inv).round().clamp(0.0, 255.0) as u8,
        (acc[2] * inv).round().clamp(0.0, 255.0) as u8,
        acc[3].round().clamp(0.0, 255.0) as u8,
    )
}

/// Warp `pixels` — placed at `offset` in document space — into a new buffer of
/// `width` × `height`, pulling each output pixel from where `map` says it came.
pub fn warp(
    pixels: &Pixmap,
    offset: (i32, i32),
    map: &Homography,
    width: u32,
    height: u32,
) -> Pixmap {
    let mut out = Pixmap::new(width, height);
    if pixels.is_empty() {
        return out;
    }

    for y in 0..height {
        for x in 0..width {
            // Sample at the pixel centre, not its corner.
            let (sx, sy) = map.map(x as f32 + 0.5, y as f32 + 0.5);
            out.set(x as i32, y as i32, sample(pixels, offset, sx, sy));
        }
    }
    out
}

/// As [`warp`], for an 8-bit coverage mask laid over the whole canvas.
///
/// Used for the selection, which has no alpha channel to worry about, so this
/// interpolates the coverage values directly.
pub fn warp_mask(
    coverage: &[u8],
    src_width: u32,
    src_height: u32,
    map: &Homography,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let mut out = vec![0u8; (width as usize) * (height as usize)];
    if coverage.len() != (src_width as usize) * (src_height as usize) {
        return out;
    }

    let at = |x: i32, y: i32| -> f32 {
        if x < 0 || y < 0 || x >= src_width as i32 || y >= src_height as i32 {
            return 0.0;
        }
        coverage[(y as usize) * (src_width as usize) + x as usize] as f32
    };

    for y in 0..height {
        for x in 0..width {
            let (sx, sy) = map.map(x as f32 + 0.5, y as f32 + 0.5);
            // The same half-pixel shift `sample` applies, for the same reason.
            let (sx, sy) = (sx - 0.5, sy - 0.5);
            let x0 = sx.floor();
            let y0 = sy.floor();
            let fx = sx - x0;
            let fy = sy - y0;
            let (x0, y0) = (x0 as i32, y0 as i32);

            let value = at(x0, y0) * (1.0 - fx) * (1.0 - fy)
                + at(x0 + 1, y0) * fx * (1.0 - fy)
                + at(x0, y0 + 1) * (1.0 - fx) * fy
                + at(x0 + 1, y0 + 1) * fx * fy;
            out[(y as usize) * (width as usize) + x as usize] =
                value.round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn a_rectangle_to_itself_is_the_identity() {
        let quad = [(0.0, 0.0), (10.0, 0.0), (10.0, 20.0), (0.0, 20.0)];
        let h = Homography::from_quads(&quad, &quad).expect("identity should exist");

        for &(x, y) in &[(0.0, 0.0), (5.0, 5.0), (10.0, 20.0), (3.5, 17.25)] {
            let (mx, my) = h.map(x, y);
            assert!(close(mx, x, 1e-3) && close(my, y, 1e-3), "({}, {}) → ({}, {})", x, y, mx, my);
        }
    }

    #[test]
    fn the_corners_land_exactly_where_they_were_sent() {
        let from = [(0.0, 0.0), (100.0, 0.0), (100.0, 50.0), (0.0, 50.0)];
        // A believable keystone: the top edge further away, so it is narrower.
        let to = [(20.0, 4.0), (80.0, 10.0), (95.0, 60.0), (5.0, 55.0)];
        let h = Homography::from_quads(&from, &to).expect("non-degenerate");

        for i in 0..4 {
            let (mx, my) = h.map(from[i].0, from[i].1);
            assert!(
                close(mx, to[i].0, 1e-2) && close(my, to[i].1, 1e-2),
                "corner {} landed at ({}, {}), wanted {:?}",
                i, mx, my, to[i]
            );
        }
    }

    #[test]
    fn a_degenerate_quad_has_no_homography() {
        let from = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        // All four corners collapsed onto a line.
        let line = [(0.0, 0.0), (5.0, 0.0), (10.0, 0.0), (15.0, 0.0)];
        assert!(Homography::from_quads(&from, &line).is_none());

        // And a quad collapsed to a single point.
        let point = [(3.0, 3.0); 4];
        assert!(Homography::from_quads(&from, &point).is_none());
    }

    #[test]
    fn suggested_size_takes_the_longer_of_each_edge_pair() {
        // Top edge 60 wide, bottom edge 100 — the near side wins.
        let quad = [(20.0, 0.0), (80.0, 0.0), (100.0, 50.0), (0.0, 50.0)];
        let (w, h) = suggested_size(&quad);
        assert_eq!(w, 100);
        // The sides are the same length by symmetry: hypot(20, 50) ≈ 53.85.
        assert_eq!(h, 54);
    }

    #[test]
    fn warping_an_unchanged_rectangle_returns_the_image() {
        let mut pm = Pixmap::filled(16, 16, Rgba8::WHITE);
        pm.set(4, 4, Rgba8::BLACK);

        let quad = [(0.0, 0.0), (16.0, 0.0), (16.0, 16.0), (0.0, 16.0)];
        let map = inverse_map(&quad, 16, 16).expect("identity");
        let out = warp(&pm, (0, 0), &map, 16, 16);

        assert_eq!(out.get(4, 4), Rgba8::BLACK, "the marked pixel moved");
        assert_eq!(out.get(0, 0), Rgba8::WHITE);
        assert_eq!(out.get(15, 15), Rgba8::WHITE);
    }

    #[test]
    fn warping_straightens_a_skewed_region() {
        // A white canvas with a black trapezoid on it. Marking the trapezoid's
        // corners should produce a solid black output.
        let mut pm = Pixmap::filled(64, 64, Rgba8::WHITE);
        let quad = [(20.0, 10.0), (44.0, 10.0), (56.0, 50.0), (8.0, 50.0)];
        for y in 10..50 {
            // Interpolate the trapezoid's left and right edges at this row.
            let t = (y - 10) as f32 / 40.0;
            let left = 20.0 + (8.0 - 20.0) * t;
            let right = 44.0 + (56.0 - 44.0) * t;
            for x in (left.ceil() as i32 + 1)..(right.floor() as i32 - 1) {
                pm.set(x, y, Rgba8::BLACK);
            }
        }

        let (w, h) = suggested_size(&quad);
        let map = inverse_map(&quad, w, h).expect("non-degenerate");
        let out = warp(&pm, (0, 0), &map, w, h);

        // The middle of the straightened result is all trapezoid.
        assert_eq!(out.get((w / 2) as i32, (h / 2) as i32), Rgba8::BLACK);
        assert_eq!(out.get((w / 4) as i32, (h / 2) as i32), Rgba8::BLACK);
        assert_eq!(out.get((w * 3 / 4) as i32, (h / 2) as i32), Rgba8::BLACK);
    }

    #[test]
    fn sampling_off_the_edge_reads_transparent() {
        let pm = Pixmap::filled(8, 8, Rgba8::BLACK);
        // A quad far outside the image: everything should come back empty
        // rather than clamping the border colour across the output.
        let quad = [(100.0, 100.0), (120.0, 100.0), (120.0, 120.0), (100.0, 120.0)];
        let map = inverse_map(&quad, 20, 20).expect("non-degenerate");
        let out = warp(&pm, (0, 0), &map, 20, 20);

        assert_eq!(out.get(10, 10).a, 0);
    }

    #[test]
    fn warping_does_not_halo_across_an_alpha_edge() {
        // Half opaque white, half fully transparent *black*. Interpolating
        // straight colour would drag that hidden black into the visible half.
        let mut pm = Pixmap::new(16, 16);
        for y in 0..16 {
            for x in 0..8 {
                pm.set(x, y, Rgba8::WHITE);
            }
        }

        let quad = [(0.0, 0.0), (16.0, 0.0), (16.0, 16.0), (0.0, 16.0)];
        let map = inverse_map(&quad, 16, 16).expect("identity");
        let out = warp(&pm, (0, 0), &map, 16, 16);

        for y in 0..16 {
            for x in 0..8 {
                let px = out.get(x, y);
                if px.a > 0 {
                    assert_eq!(px.r, 255, "colour darkened at ({}, {})", x, y);
                }
            }
        }
    }

    #[test]
    fn a_mask_warps_with_the_pixels() {
        let mut coverage = vec![0u8; 16 * 16];
        for y in 0..8 {
            for x in 0..16 {
                coverage[y * 16 + x] = 255;
            }
        }

        let quad = [(0.0, 0.0), (16.0, 0.0), (16.0, 16.0), (0.0, 16.0)];
        let map = inverse_map(&quad, 16, 16).expect("identity");
        let out = warp_mask(&coverage, 16, 16, &map, 16, 16);

        assert_eq!(out[2 * 16 + 8], 255, "the selected half was lost");
        assert_eq!(out[12 * 16 + 8], 0, "coverage bled into the empty half");
    }

    #[test]
    fn a_mask_of_the_wrong_size_is_rejected() {
        let quad = [(0.0, 0.0), (8.0, 0.0), (8.0, 8.0), (0.0, 8.0)];
        let map = inverse_map(&quad, 8, 8).expect("identity");
        let out = warp_mask(&[0u8; 4], 16, 16, &map, 8, 8);
        assert_eq!(out.len(), 64);
        assert!(out.iter().all(|&c| c == 0));
    }
}

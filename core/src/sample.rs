//! Matching a colour under a brush — how Photoshop's sampling tools decide
//! which pixels beneath a dab they are allowed to touch.
//!
//! The Color Replacement Brush and the Background Eraser are the same tool
//! wearing different hats: both sample a reference colour, both test every
//! pixel under the dab against it, and both offer the identical **Sampling**,
//! **Limits** and **Tolerance** controls on the options bar. Only what they do
//! with a match differs — one mixes a colour in, the other takes the alpha out.
//!
//! So the deciding lives here and the acting lives with each tool. [`replace`]
//! re-exports [`Sampling`] and [`Limits`] under its own names, since they are
//! the same buttons in CS6's bar.
//!
//! [`replace`]: crate::replace

use crate::brush::Brush;
use crate::buffer::{Pixmap, Rect, Rgba8};

/// Where the colour being matched comes from. CS6's Sampling buttons.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(i32)]
pub enum Sampling {
    /// Re-read under the brush as it moves, so dragging across a boundary acts
    /// on whatever is currently beneath. CS6's default.
    #[default]
    Continuous = 0,
    /// Read once, where the stroke began.
    Once = 1,
    /// Match the background swatch, sampling nothing from the image.
    BackgroundSwatch = 2,
}

impl Sampling {
    pub fn from_i32(v: i32) -> Sampling {
        match v {
            1 => Sampling::Once,
            2 => Sampling::BackgroundSwatch,
            _ => Sampling::Continuous,
        }
    }
}

/// How far a match is allowed to spread within a dab. CS6's Limits.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(i32)]
pub enum Limits {
    /// Every matching pixel under the brush, connected or not.
    Discontiguous = 0,
    /// Only pixels joined to the one under the brush centre. CS6's default.
    #[default]
    Contiguous = 1,
    /// As contiguous, but stopping at strong edges — so working along a
    /// boundary does not leak across it.
    FindEdges = 2,
}

impl Limits {
    pub fn from_i32(v: i32) -> Limits {
        match v {
            0 => Limits::Discontiguous,
            2 => Limits::FindEdges,
            _ => Limits::Contiguous,
        }
    }
}

/// Normalised gradient above which Find Edges refuses to spread.
const EDGE_LIMIT: f32 = 0.35;

/// How strongly a pixel counts as a match, `0.0..=1.0`.
///
/// With antialiasing the strength tapers as the difference approaches the
/// tolerance, which softens the edge of the matched region; without it the test
/// is a hard in-or-out.
pub fn match_strength(pixel: Rgba8, reference: Rgba8, tolerance: u32, antialias: bool) -> f32 {
    let d = |a: u8, b: u8| (a as i32 - b as i32).unsigned_abs();
    let distance = d(pixel.r, reference.r)
        .max(d(pixel.g, reference.g))
        .max(d(pixel.b, reference.b)) as f32;
    let tolerance = tolerance.max(1) as f32;

    if !antialias {
        return if distance <= tolerance { 1.0 } else { 0.0 };
    }
    // Solid out to 70% of the tolerance, then fading to nothing at it.
    let solid = tolerance * 0.7;
    if distance <= solid {
        1.0
    } else if distance >= tolerance {
        0.0
    } else {
        1.0 - (distance - solid) / (tolerance - solid)
    }
}

/// Pixels within the dab reachable from its centre without leaving the matching
/// region — and, for Find Edges, without crossing a strong edge.
///
/// Contiguity is resolved *within one dab* rather than across the image: that is
/// what stops a match jumping a boundary even when the far side of it matches
/// too, while keeping each dab's cost proportional to the brush, not the canvas.
#[allow(clippy::too_many_arguments)]
pub fn reachable(
    pixels: &Pixmap,
    region: Rect,
    centre: (i32, i32),
    reference: Rgba8,
    brush: &Brush,
    cx: f32,
    cy: f32,
    radius: f32,
    limits: Limits,
    tolerance: u32,
    antialias: bool,
) -> Vec<bool> {
    let w = region.width as usize;
    let h = region.height as usize;
    let mut reached = vec![false; w * h];
    if !region.contains(centre.0, centre.1) {
        return reached;
    }

    let luma = |c: Rgba8| 0.299 * c.r as f32 + 0.587 * c.g as f32 + 0.114 * c.b as f32;
    let index = |x: i32, y: i32| ((y - region.y) as usize) * w + (x - region.x) as usize;

    let mut queue = std::collections::VecDeque::new();
    reached[index(centre.0, centre.1)] = true;
    queue.push_back(centre);

    let dab = Brush { size: radius * 2.0, ..*brush };
    while let Some((x, y)) = queue.pop_front() {
        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            let (nx, ny) = (x + dx, y + dy);
            if !region.contains(nx, ny) {
                continue;
            }
            let next = index(nx, ny);
            if reached[next] {
                continue;
            }
            // Only spread within the dab's own footprint.
            if dab.pixel_coverage(
                nx as f32 + 0.5 - cx,
                ny as f32 + 0.5 - cy,
                brush.angle,
                brush.roundness,
            ) <= 0.0
            {
                continue;
            }
            if match_strength(pixels.get(nx, ny), reference, tolerance, antialias) <= 0.0 {
                continue;
            }
            if limits == Limits::FindEdges {
                // A big jump in brightness between neighbours is a boundary;
                // stopping there is what keeps the match off the other side.
                let step = (luma(pixels.get(nx, ny)) - luma(pixels.get(x, y))).abs() / 255.0;
                if step > EDGE_LIMIT {
                    continue;
                }
            }
            reached[next] = true;
            queue.push_back((nx, ny));
        }
    }
    reached
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hard_match_is_in_or_out_at_the_tolerance() {
        let grey = Rgba8::opaque(100, 100, 100);
        assert_eq!(match_strength(Rgba8::opaque(120, 100, 100), grey, 32, false), 1.0);
        assert_eq!(match_strength(Rgba8::opaque(140, 100, 100), grey, 32, false), 0.0);
    }

    #[test]
    fn an_antialiased_match_tapers_toward_the_tolerance() {
        let grey = Rgba8::opaque(100, 100, 100);
        // Solid well inside, gone at the tolerance, part-way in between.
        assert_eq!(match_strength(Rgba8::opaque(110, 100, 100), grey, 40, true), 1.0);
        assert_eq!(match_strength(Rgba8::opaque(141, 100, 100), grey, 40, true), 0.0);
        let edge = match_strength(Rgba8::opaque(135, 100, 100), grey, 40, true);
        assert!(edge > 0.0 && edge < 1.0, "the edge did not taper: {edge}");
    }

    #[test]
    fn contiguous_spread_stops_at_a_barrier() {
        // A wall of unmatched colour down the middle: the flood starts on the
        // left and must not appear on the right, though both sides match.
        let mut pm = Pixmap::new(21, 21);
        pm.fill(Rgba8::opaque(10, 10, 10));
        for y in 0..21 {
            pm.set(10, y, Rgba8::opaque(240, 240, 240));
        }

        let brush = Brush { size: 20.0, hardness: 1.0, ..Brush::default() };
        let region = pm.rect();
        let reached = reachable(
            &pm,
            region,
            (4, 10),
            Rgba8::opaque(10, 10, 10),
            &brush,
            10.0,
            10.0,
            10.0,
            Limits::Contiguous,
            32,
            false,
        );
        let at = |x: i32, y: i32| reached[(y as usize) * 21 + x as usize];
        assert!(at(4, 10), "the flood did not reach its own start");
        assert!(at(8, 10), "the flood stopped short of the barrier");
        assert!(!at(10, 10), "the flood ran into the barrier");
        assert!(!at(15, 10), "the flood jumped the barrier");
    }
}

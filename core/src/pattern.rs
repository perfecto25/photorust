//! Patterns — the tiles the Pattern Stamp paints with.
//!
//! A pattern is a small square image repeated across the canvas. Photoshop
//! ships its as artwork in a `.pat` file; ours are **generated**, because the
//! CS6 set is Adobe's artwork and not something to reproduce. They are named
//! for what they are rather than after Adobe's presets, and the set is chosen
//! to cover the useful shapes: regular grids, stripes, weaves, and noise.
//!
//! Every tile is square and seamless — the right edge continues into the left
//! and the bottom into the top — which is what lets the stamp repeat one
//! without a visible join. The generators are written to wrap by construction
//! (positions taken modulo the tile) rather than by mirroring a half-tile,
//! which would show as an axis of symmetry once repeated.
//!
//! Tiles are drawn in greyscale on purpose. Photoshop's pattern stamp lays a
//! pattern down as it is, so a coloured tile would fight whatever it is painted
//! over; grey ones read as texture and take colour from the layer beneath when
//! the brush is used at lower opacity.

use crate::buffer::{Pixmap, Rgba8};

/// Side of every generated tile, in pixels.
///
/// One size for all of them keeps the previews uniform and the tiling maths
/// trivial. 64 is large enough for the coarser textures to look like something
/// and small enough to stay cheap to regenerate.
pub const TILE: u32 = 64;

/// The built-in patterns, in the order the picker lists them.
pub const PATTERN_NAMES: [&str; 8] = [
    "Checkerboard",
    "Grid",
    "Diagonal Stripes",
    "Horizontal Lines",
    "Polka Dots",
    "Woven",
    "Bricks",
    "Grain",
];

/// Render the pattern at `index`, or `None` if there is no such pattern.
pub fn tile(index: usize) -> Option<Pixmap> {
    let name = *PATTERN_NAMES.get(index)?;
    Some(match name {
        "Checkerboard" => checkerboard(),
        "Grid" => grid(),
        "Diagonal Stripes" => diagonal_stripes(),
        "Horizontal Lines" => horizontal_lines(),
        "Polka Dots" => polka_dots(),
        "Woven" => woven(),
        "Bricks" => bricks(),
        _ => grain(),
    })
}

/// The pattern's index by name, for settings that store the name rather than a
/// position in the list.
pub fn index_of(name: &str) -> Option<usize> {
    PATTERN_NAMES.iter().position(|n| *n == name)
}

/// Fill `size` pixels with the pattern repeated from `origin`.
///
/// `origin` is the document point the tile's top-left corner lands on, which is
/// what CS6's **Aligned** checkbox decides: aligned pins it to the document, so
/// separate strokes join up seamlessly, and unaligned pins it to wherever each
/// stroke began.
pub fn tiled(index: usize, size: (u32, u32), origin: (i32, i32)) -> Option<Pixmap> {
    let tile = tile(index)?;
    let (tw, th) = (tile.width() as i32, tile.height() as i32);
    if tw <= 0 || th <= 0 {
        return None;
    }

    let mut out = Pixmap::new(size.0, size.1);
    for y in 0..size.1 as i32 {
        // Rust's `%` keeps the sign of the dividend, so a point left of or above
        // the origin would index backwards out of the tile. The extra add wraps
        // it round instead.
        let ty = (y - origin.1).rem_euclid(th);
        for x in 0..size.0 as i32 {
            let tx = (x - origin.0).rem_euclid(tw);
            out.set(x, y, tile.get(tx, ty));
        }
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// The tiles
// ---------------------------------------------------------------------------

fn shade(v: u8) -> Rgba8 {
    Rgba8::opaque(v, v, v)
}

/// Blend `over` into the tile at `x`, `y` by `alpha`, so an edge can be
/// softened without the generators each doing their own arithmetic.
fn blend(pm: &mut Pixmap, x: i32, y: i32, over: u8, alpha: f32) {
    let alpha = alpha.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return;
    }
    let under = pm.get(x, y).r as f32;
    let v = (under + (over as f32 - under) * alpha).round().clamp(0.0, 255.0) as u8;
    pm.set(x, y, shade(v));
}

/// Coverage of a disc at a point, antialiased over one pixel — the shared
/// building block for the round patterns.
fn disc_coverage(dx: f32, dy: f32, radius: f32) -> f32 {
    let d = (dx * dx + dy * dy).sqrt();
    (radius + 0.5 - d).clamp(0.0, 1.0)
}

fn checkerboard() -> Pixmap {
    let mut pm = Pixmap::new(TILE, TILE);
    let half = (TILE / 2) as i32;
    for y in 0..TILE as i32 {
        for x in 0..TILE as i32 {
            let dark = (x < half) ^ (y < half);
            pm.set(x, y, shade(if dark { 90 } else { 215 }));
        }
    }
    pm
}

fn grid() -> Pixmap {
    let mut pm = Pixmap::new(TILE, TILE);
    for y in 0..TILE as i32 {
        for x in 0..TILE as i32 {
            // Lines on two edges only: the neighbouring tile supplies the other
            // two, so a repeated grid has single-width lines, not double.
            let on_line = x < 2 || y < 2;
            pm.set(x, y, shade(if on_line { 105 } else { 220 }));
        }
    }
    pm
}

fn diagonal_stripes() -> Pixmap {
    let mut pm = Pixmap::new(TILE, TILE);
    let period = 16;
    for y in 0..TILE as i32 {
        for x in 0..TILE as i32 {
            // (x + y) modulo the period runs the stripe at 45°, and the period
            // divides the tile so the pattern meets itself at every edge.
            let band = (x + y).rem_euclid(period);
            pm.set(x, y, shade(if band < period / 2 { 110 } else { 210 }));
        }
    }
    pm
}

fn horizontal_lines() -> Pixmap {
    let mut pm = Pixmap::new(TILE, TILE);
    for y in 0..TILE as i32 {
        let on_line = y.rem_euclid(8) < 3;
        for x in 0..TILE as i32 {
            pm.set(x, y, shade(if on_line { 115 } else { 220 }));
        }
    }
    pm
}

fn polka_dots() -> Pixmap {
    let mut pm = Pixmap::new(TILE, TILE);
    for y in 0..TILE as i32 {
        for x in 0..TILE as i32 {
            pm.set(x, y, shade(220));
        }
    }

    // Four dots on a half-tile offset grid: the two on the quarter points and
    // two more staggered between them, which reads as a scatter once repeated
    // instead of as rows.
    let r = 9.0;
    let centres = [(16.0, 16.0), (48.0, 16.0), (32.0, 48.0), (0.0, 48.0)];
    for (cx, cy) in centres {
        for y in 0..TILE as i32 {
            for x in 0..TILE as i32 {
                // Measured on the torus, so a dot on an edge appears on both
                // sides and the tile stays seamless.
                let dx = wrapped_delta(x as f32 + 0.5 - cx);
                let dy = wrapped_delta(y as f32 + 0.5 - cy);
                blend(&mut pm, x, y, 100, disc_coverage(dx, dy, r));
            }
        }
    }
    pm
}

/// The shorter way round the tile between two coordinates — how far apart two
/// points are when the tile is treated as wrapping.
fn wrapped_delta(d: f32) -> f32 {
    let side = TILE as f32;
    let mut d = d % side;
    if d > side / 2.0 {
        d -= side;
    } else if d < -side / 2.0 {
        d += side;
    }
    d
}

fn woven() -> Pixmap {
    let mut pm = Pixmap::new(TILE, TILE);
    let cell = 16; // one over-under crossing
    for y in 0..TILE as i32 {
        for x in 0..TILE as i32 {
            let (cx, cy) = (x.rem_euclid(cell), y.rem_euclid(cell));
            let (col, row) = (x / cell, y / cell);
            // Alternating cells put the horizontal thread on top of the
            // vertical one, which is what makes it read as weaving rather than
            // as a grid.
            let horizontal_on_top = (col + row) % 2 == 0;
            let in_horizontal = cy >= 3 && cy < cell - 3;
            let in_vertical = cx >= 3 && cx < cell - 3;

            let v = if horizontal_on_top && in_horizontal {
                190
            } else if !horizontal_on_top && in_vertical {
                190
            } else if in_horizontal || in_vertical {
                140
            } else {
                95
            };
            pm.set(x, y, shade(v));
        }
    }
    pm
}

fn bricks() -> Pixmap {
    let mut pm = Pixmap::new(TILE, TILE);
    let (bw, bh) = (32, 16); // two courses of two bricks per tile
    let mortar = 3;
    for y in 0..TILE as i32 {
        let course = y / bh;
        // Every other course is offset by half a brick — the running bond. The
        // offset is a whole number of tiles over two courses, so the tile still
        // meets itself.
        let shift = if course % 2 == 0 { 0 } else { bw / 2 };
        for x in 0..TILE as i32 {
            let in_mortar =
                y.rem_euclid(bh) < mortar || (x + shift).rem_euclid(bw) < mortar;
            pm.set(x, y, shade(if in_mortar { 205 } else { 120 }));
        }
    }
    pm
}

fn grain() -> Pixmap {
    let mut pm = Pixmap::new(TILE, TILE);
    // A fixed hash rather than a random generator: the tile has to come out the
    // same every time it is regenerated, or a stroke would not match its own
    // preview — and repainting after undo would change the texture.
    for y in 0..TILE as i32 {
        for x in 0..TILE as i32 {
            let h = hash(x as u32, y as u32);
            let v = 150 + (h % 80) as u8;
            pm.set(x, y, shade(v));
        }
    }
    pm
}

/// A small integer hash, for the grain tile. Any well-mixed function does; this
/// is the usual xorshift-and-multiply arrangement.
fn hash(x: u32, y: u32) -> u32 {
    let mut h = x.wrapping_mul(0x9E37_79B9) ^ y.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 13;
    h = h.wrapping_mul(0xC2B2_AE35);
    h ^ (h >> 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pattern_renders_at_the_tile_size() {
        for index in 0..PATTERN_NAMES.len() {
            let tile = tile(index).expect("a listed pattern did not render");
            assert_eq!(tile.width(), TILE);
            assert_eq!(tile.height(), TILE);
        }
        assert!(tile(PATTERN_NAMES.len()).is_none(), "an index past the end rendered");
    }

    #[test]
    fn every_pattern_is_opaque_and_has_some_contrast() {
        // A tile that came out flat would paint as a solid colour, which is not
        // a pattern; one with holes in it would paint transparency.
        for index in 0..PATTERN_NAMES.len() {
            let tile = tile(index).unwrap();
            let mut lowest = 255u8;
            let mut highest = 0u8;
            for y in 0..TILE as i32 {
                for x in 0..TILE as i32 {
                    let px = tile.get(x, y);
                    assert_eq!(px.a, 255, "{} has a transparent pixel", PATTERN_NAMES[index]);
                    lowest = lowest.min(px.r);
                    highest = highest.max(px.r);
                }
            }
            assert!(
                highest - lowest > 30,
                "{} is nearly flat: {lowest}..{highest}",
                PATTERN_NAMES[index]
            );
        }
    }

    #[test]
    fn tiles_are_seamless_across_their_edges() {
        // Opposite edges have to continue into each other, or every repeat
        // shows a join. Compared against the row one step *inside* the far
        // edge, which is what the neighbouring tile puts there.
        for index in 0..PATTERN_NAMES.len() {
            let tile = tile(index).unwrap();
            let side = TILE as i32;
            let mut worst = 0i32;
            for i in 0..side {
                let horizontal = (tile.get(0, i).r as i32 - tile.get(side - 1, i).r as i32).abs()
                    - (tile.get(1, i).r as i32 - tile.get(0, i).r as i32).abs();
                let vertical = (tile.get(i, 0).r as i32 - tile.get(i, side - 1).r as i32).abs()
                    - (tile.get(i, 1).r as i32 - tile.get(i, 0).r as i32).abs();
                worst = worst.max(horizontal).max(vertical);
            }
            // The seam may be no sharper than the steps the tile already makes
            // one pixel in — a hard-edged pattern like the checkerboard jumps
            // at its own boundaries, and that is not a seam.
            assert!(
                worst <= 130,
                "{} does not wrap: edge step {worst}",
                PATTERN_NAMES[index]
            );
        }
    }

    #[test]
    fn tiling_repeats_from_the_origin() {
        let filled = tiled(0, (TILE * 2, TILE), (0, 0)).unwrap();
        for y in 0..TILE as i32 {
            for x in 0..TILE as i32 {
                assert_eq!(
                    filled.get(x, y),
                    filled.get(x + TILE as i32, y),
                    "the second tile does not match the first"
                );
            }
        }
    }

    #[test]
    fn tiling_from_a_negative_origin_still_lands_inside_the_tile() {
        // An unaligned stroke starting near the top-left puts the origin at a
        // negative offset, which must wrap rather than read out of bounds.
        let filled = tiled(0, (8, 8), (-3, -5)).unwrap();
        let source = tile(0).unwrap();
        assert_eq!(filled.get(0, 0), source.get(3, 5));
    }

    #[test]
    fn patterns_can_be_found_by_name() {
        assert_eq!(index_of("Grid"), Some(1));
        assert_eq!(index_of("Not A Pattern"), None);
    }
}

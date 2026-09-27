//! The composite as the canvas shows it: full size, and a pyramid of halved
//! copies for when it is zoomed out.
//!
//! At fit-to-screen a 16000-pixel-square map shows about 1.5 million of its
//! 267 million pixels, but the canvas used to draw from the whole picture —
//! handing QPainter a gigabyte to scale down on every repaint. Now it asks
//! for just the part it can see, from the level whose resolution is closest
//! above the screen's: level 0 is the composite itself, level 1 half its
//! size, level 2 a quarter, and so on down to [`SMALLEST_SIDE`].
//!
//! Every level is kept **premultiplied**. Averaging four straight-alpha
//! pixels lets the colour of a transparent one bleed into its neighbours and
//! darkens soft edges when zoomed out; averaging premultiplied pixels is
//! simply correct. It is also the format Qt paints fastest, so a crop crosses
//! the bridge without conversion.
//!
//! A level is kept up to date the way the canvas used to keep its one
//! picture: from the damage the engine records (`crate::damage`). A stroke
//! recomposites its bounds into level 0 and shrinks that rectangle down
//! through the rest; only an undescribed change rebuilds the lot.
//!
//! Not a GPU candidate, though shrinking is per-pixel and uniform: every
//! level is read back by the CPU the moment it is made — to crop for the
//! canvas — which is the case CLAUDE.md §7 turns away. It is also cheap: the
//! levels below 0 add up to a third of level 0, and an update touches only
//! the damaged part of each.

use crate::buffer::{Pixmap, Rect};
use rayon::prelude::*;

/// Levels stop halving once the next would be smaller than this on its long
/// side. At that size a whole-picture thumbnail fits in any viewport, and a
/// canvas zoomed out further just scales the last level down.
pub const SMALLEST_SIDE: u32 = 256;

/// The composite at every level. See the module notes.
#[derive(Default)]
pub struct ViewPyramid {
    /// Level 0 first. Empty until the first rebuild.
    levels: Vec<Pixmap>,
}

impl ViewPyramid {
    /// How many levels there are, 0 before the first rebuild.
    pub fn level_count(&self) -> usize {
        self.levels.len()
    }

    /// The size of `level`, or `(0, 0)` past the last.
    pub fn level_size(&self, level: usize) -> (u32, u32) {
        self.levels.get(level).map_or((0, 0), |p| (p.width(), p.height()))
    }

    /// Whether this was built for a composite of this size. After a crop or
    /// a canvas resize it was not, and has to be rebuilt.
    pub fn fits(&self, width: u32, height: u32) -> bool {
        self.level_size(0) == (width, height)
    }

    /// Start again from a whole composite, already premultiplied.
    pub fn rebuild(&mut self, composite: Pixmap) {
        self.levels.clear();
        self.levels.push(composite);
        loop {
            let last = self.levels.last().expect("level 0 was pushed");
            let (w, h) = (last.width(), last.height());
            if w.max(h) / 2 < SMALLEST_SIDE || w <= 1 || h <= 1 {
                break;
            }
            let mut next = Pixmap::new(half(w), half(h));
            let whole = next.rect();
            shrink_into(last, &mut next, whole);
            self.levels.push(next);
        }
    }

    /// Replace `region` of level 0 with `patch` — its premultiplied composite,
    /// `region`-sized — and shrink the change down through every level.
    pub fn update(&mut self, region: Rect, patch: &Pixmap) {
        let Some(base) = self.levels.first_mut() else {
            return;
        };
        let region = region.intersect(&base.rect());
        if region.is_empty() {
            return;
        }
        base.blit(patch, region.x, region.y);

        let mut damaged = region;
        for level in 1..self.levels.len() {
            // A pixel at this level stands for a 2×2 block of the one above,
            // so the damage halves outwards: any block it touches is redone.
            damaged = Rect::new(
                damaged.x / 2,
                damaged.y / 2,
                (half_up(damaged.right()) - damaged.x / 2) as u32,
                (half_up(damaged.bottom()) - damaged.y / 2) as u32,
            );
            let (above, below) = self.levels.split_at_mut(level);
            let target = &mut below[0];
            let damaged_here = damaged.intersect(&target.rect());
            shrink_into(&above[level - 1], target, damaged_here);
            damaged = damaged_here;
        }
    }

    /// `rect` of `level`, as a `rect`-sized copy. Whatever of `rect` lies
    /// outside the level comes back transparent.
    pub fn crop(&self, level: usize, rect: Rect) -> Option<Pixmap> {
        self.levels.get(level).map(|pixels| pixels.crop(rect))
    }
}

fn half(n: u32) -> u32 {
    n.div_ceil(2)
}

fn half_up(n: i32) -> i32 {
    (n + 1).div_euclid(2)
}

/// Fill `rect` of `dst` from `src`, one size up: each pixel the average of
/// the 2×2 block above it. At an odd edge the block is cut short and the
/// average taken over what is there, so the last row and column of a level
/// are not dimmed by samples from outside the picture.
fn shrink_into(src: &Pixmap, dst: &mut Pixmap, rect: Rect) {
    let rect = rect.intersect(&dst.rect());
    if rect.is_empty() {
        return;
    }
    let (sw, sh) = (src.width() as usize, src.height() as usize);
    let stride = dst.stride();
    let (x0, x1) = (rect.x as usize, rect.right() as usize);
    let (y0, y1) = (rect.y as usize, rect.bottom() as usize);
    dst.as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .filter(|(y, _)| *y >= y0 && *y < y1)
        .for_each(|(y, row)| {
            let rows = [src.row((2 * y) as u32), src.row((2 * y + 1).min(sh - 1) as u32)];
            let tall = 2 * y + 1 < sh;
            for x in x0..x1 {
                let wide = 2 * x + 1 < sw;
                let mut sum = [0u32; 4];
                let mut n = 0u32;
                for (r, row_src) in rows.iter().enumerate() {
                    if r == 1 && !tall {
                        continue;
                    }
                    for dx in 0..if wide { 2 } else { 1 } {
                        let i = (2 * x + dx) * 4;
                        for c in 0..4 {
                            sum[c] += row_src[i + c] as u32;
                        }
                        n += 1;
                    }
                }
                let o = x * 4;
                for c in 0..4 {
                    row[o + c] = ((sum[c] + n / 2) / n) as u8;
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Rgba8;

    /// A premultiplied picture with something different in every pixel.
    fn picture(width: u32, height: u32, seed: u32) -> Pixmap {
        let mut p = Pixmap::new(width, height);
        for y in 0..height as i32 {
            for x in 0..width as i32 {
                let v = (x as u32 * 7 + y as u32 * 13 + seed) % 251;
                let a = 60 + (x as u32 * 3 + y as u32) % 196;
                p.set(x, y, Rgba8::new(v as u8, (v / 2) as u8, 255 - v as u8, a as u8));
            }
        }
        p.premultiply();
        p
    }

    #[test]
    fn levels_halve_down_to_the_smallest_side() {
        let mut pyramid = ViewPyramid::default();
        pyramid.rebuild(picture(1500, 700, 0));
        let sizes: Vec<_> = (0..pyramid.level_count()).map(|l| pyramid.level_size(l)).collect();
        // 1500 → 750 → 375, and one more would be under 256.
        assert_eq!(sizes, vec![(1500, 700), (750, 350), (375, 175)]);
    }

    #[test]
    fn a_small_picture_is_its_own_only_level() {
        let mut pyramid = ViewPyramid::default();
        pyramid.rebuild(picture(300, 200, 0));
        assert_eq!(pyramid.level_count(), 1);
    }

    #[test]
    fn each_level_is_the_average_of_the_one_above() {
        let mut pyramid = ViewPyramid::default();
        pyramid.rebuild(picture(1031, 517, 3)); // odd, so the edges are cut short
        let (above, below) = (&pyramid.levels[0], &pyramid.levels[1]);
        for (x, y) in [(0, 0), (100, 50), (515, 258), (515, 0), (0, 258)] {
            let mut sum = [0u32; 4];
            let mut n = 0;
            for dy in 0..2 {
                for dx in 0..2 {
                    let (sx, sy) = (2 * x + dx, 2 * y + dy);
                    if sx < 1031 && sy < 517 {
                        let p = above.get(sx, sy);
                        for (c, v) in [p.r, p.g, p.b, p.a].into_iter().enumerate() {
                            sum[c] += v as u32;
                        }
                        n += 1;
                    }
                }
            }
            let want: Vec<u8> = sum.iter().map(|s| ((s + n / 2) / n) as u8).collect();
            let got = below.get(x, y);
            assert_eq!(vec![got.r, got.g, got.b, got.a], want, "at {x},{y}");
        }
    }

    #[test]
    fn an_update_leaves_every_level_as_a_rebuild_would() {
        // The whole point of updating by damage: it must land on exactly the
        // pyramid a rebuild from the changed picture gives, at every level —
        // including a change straddling an odd edge.
        let (w, h) = (1203, 811);
        let before = picture(w, h, 0);
        let after_patch = picture(w, h, 99);

        let mut updated = ViewPyramid::default();
        updated.rebuild(before.clone());
        let mut changed = before;
        for region in [Rect::new(301, 97, 150, 83), Rect::new(1100, 700, 103, 111)] {
            let patch = after_patch.crop(region);
            changed.blit(&patch, region.x, region.y);
            updated.update(region, &patch);
        }

        let mut rebuilt = ViewPyramid::default();
        rebuilt.rebuild(changed);
        assert_eq!(updated.level_count(), rebuilt.level_count());
        for level in 0..rebuilt.level_count() {
            assert_eq!(
                updated.levels[level].as_bytes(),
                rebuilt.levels[level].as_bytes(),
                "level {level} differs"
            );
        }
    }

    #[test]
    fn shrinking_premultiplied_keeps_a_soft_edge_its_colour() {
        // Red at full opacity beside fully transparent pixels whose stored
        // colour is black. Averaged straight, the edge would come out a
        // darkened red; premultiplied, it is red at half coverage.
        let mut p = Pixmap::new(600, 600);
        for y in 0..600 {
            for x in 0..600 {
                let colour = if x % 2 == 0 { Rgba8::new(255, 0, 0, 255) } else { Rgba8::new(0, 0, 0, 0) };
                p.set(x, y, colour);
            }
        }
        p.premultiply();
        let mut pyramid = ViewPyramid::default();
        pyramid.rebuild(p);
        let mut shrunk = pyramid.crop(1, Rect::new(10, 10, 1, 1)).unwrap();
        shrunk.unpremultiply();
        let px = shrunk.get(0, 0);
        assert_eq!((px.r, px.g, px.b), (255, 0, 0));
        assert!((px.a as i32 - 128).abs() <= 1);
    }
}

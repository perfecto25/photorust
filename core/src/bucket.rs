//! Flood filling — the engine behind the Paint Bucket.
//!
//! The Paint Bucket is the Magic Wand with a colour instead of a selection: the
//! same question ("which pixels belong with the one I clicked?") answered by the
//! same flood, and then filled rather than selected. So the matching lives in
//! [`crate::wand::magic_wand`] and this module only paints through the mask it
//! returns. Tolerance, Contiguous and Anti-alias therefore behave *identically*
//! to the wand's, which is what CS6 users expect — the two tools share those
//! settings' meaning down to the per-channel maximum distance.

use crate::blend::BlendMode;
use crate::buffer::{Pixmap, Rect, Rgba8};
use crate::compositor;
use crate::selection::Selection;

/// What the Paint Bucket fills with — CS6's **Fill** menu.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(i32)]
pub enum BucketFill {
    /// The foreground colour. The default, and the only one implemented:
    /// patterns are a sub-system of their own.
    #[default]
    Foreground = 0,
    Pattern = 1,
}

impl BucketFill {
    pub fn from_i32(v: i32) -> BucketFill {
        match v {
            1 => BucketFill::Pattern,
            _ => BucketFill::Foreground,
        }
    }
}

/// The Paint Bucket's options bar.
#[derive(Clone, Copy, Debug)]
pub struct BucketOptions {
    pub mode: BlendMode,
    /// Master opacity, `0.0..=1.0`.
    pub opacity: f32,
    /// 0–255, how far a pixel may differ per channel and still be filled. The
    /// same scale as the Magic Wand's.
    pub tolerance: u32,
    /// Soften the boundary of the filled region by about half a pixel.
    pub antialias: bool,
    /// Fill only the region joined to the clicked pixel. On by default; off,
    /// every matching pixel in the layer is filled however far away.
    pub contiguous: bool,
    /// Decide what matches from the composite rather than the active layer. The
    /// fill still lands on the active layer alone.
    pub all_layers: bool,
    /// The layer's Lock Transparent Pixels. Set from the layer, not the bar.
    pub preserve_alpha: bool,
}

impl Default for BucketOptions {
    fn default() -> Self {
        // CS6 opens on Tolerance 32 with Anti-alias and Contiguous ticked.
        Self {
            mode: BlendMode::Normal,
            opacity: 1.0,
            tolerance: 32,
            antialias: true,
            contiguous: true,
            all_layers: false,
            preserve_alpha: false,
        }
    }
}

/// A flood mask and where it sits relative to the pixels being filled.
///
/// `origin` is the mask's top-left in the target's own coordinates, so a mask
/// built from the document-space composite (Sample All Layers) and one built from
/// the layer itself can be applied by the same loop.
pub struct FloodMask<'a> {
    pub coverage: &'a [u8],
    pub width: u32,
    pub height: u32,
    pub origin: (i32, i32),
}

impl FloodMask<'_> {
    fn at(&self, x: i32, y: i32) -> f32 {
        let (mx, my) = (x - self.origin.0, y - self.origin.1);
        if mx < 0 || my < 0 || mx >= self.width as i32 || my >= self.height as i32 {
            return 0.0;
        }
        let index = (my as usize) * (self.width as usize) + mx as usize;
        self.coverage.get(index).map_or(0.0, |v| *v as f32 / 255.0)
    }
}

/// Fill `pixels` with `color` through `mask`. Returns the region changed.
///
/// `offset` is where the pixmap sits in document space, needed only to ask the
/// selection about a pixel.
pub fn fill(
    pixels: &mut Pixmap,
    mask: &FloodMask<'_>,
    color: Rgba8,
    options: &BucketOptions,
    offset: (i32, i32),
    selection: Option<&Selection>,
) -> Rect {
    let opacity = options.opacity.clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return Rect::default();
    }
    let selection = selection.filter(|sel| !sel.is_empty());

    // Only where the mask reaches: a contiguous fill in a corner should not cost
    // a pass over the whole layer.
    let region = Rect::new(
        mask.origin.0,
        mask.origin.1,
        mask.width,
        mask.height,
    )
    .intersect(&pixels.rect());
    let mut dirty = Rect::default();

    for y in region.y..region.bottom() {
        for x in region.x..region.right() {
            let mut alpha = mask.at(x, y) * opacity;
            if alpha <= 0.0 {
                continue;
            }
            if let Some(sel) = selection {
                alpha *= sel.coverage_at(x + offset.0, y + offset.1);
                if alpha <= 0.0 {
                    continue;
                }
            }

            let dst = pixels.get(x, y);
            if options.preserve_alpha {
                if dst.a == 0 {
                    continue;
                }
                alpha *= dst.a as f32 / 255.0;
            }

            let out = compositor::blend_pixel(dst, color, alpha, options.mode);
            if out != dst {
                pixels.set(x, y, out);
                dirty = dirty.union(&Rect::new(x, y, 1, 1));
            }
        }
    }

    dirty
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wand::magic_wand;

    /// The mask the tool would build, for a click at `seed`.
    fn flood(pixels: &Pixmap, seed: (i32, i32), options: &BucketOptions) -> Vec<u8> {
        magic_wand(pixels, seed, options.tolerance, options.contiguous, options.antialias)
    }

    /// A whole-canvas mask, which is the shape `magic_wand` returns.
    fn mask_of(coverage: &[u8], width: u32, height: u32) -> FloodMask<'_> {
        FloodMask { coverage, width, height, origin: (0, 0) }
    }

    /// Two rooms of flat colour with a wall between them.
    fn rooms() -> Pixmap {
        let mut pm = Pixmap::filled(40, 20, Rgba8::WHITE);
        pm.fill_rect(Rect::new(19, 0, 2, 20), Rgba8::BLACK);
        pm
    }

    #[test]
    fn a_contiguous_fill_stops_at_the_wall() {
        let mut pm = rooms();
        let options = BucketOptions { antialias: false, ..BucketOptions::default() };
        let coverage = flood(&pm, (5, 10), &options);
        let red = Rgba8::opaque(220, 0, 0);
        let dirty = fill(&mut pm, &mask_of(&coverage, 40, 20), red, &options, (0, 0), None);

        assert!(!dirty.is_empty());
        assert_eq!(pm.get(5, 10), red, "the clicked room was not filled");
        assert_eq!(pm.get(30, 10), Rgba8::WHITE, "the fill leaked past the wall");
        assert_eq!(pm.get(19, 10), Rgba8::BLACK, "the wall itself was filled");
    }

    #[test]
    fn turning_contiguous_off_fills_both_rooms() {
        let mut pm = rooms();
        let options = BucketOptions {
            contiguous: false,
            antialias: false,
            ..BucketOptions::default()
        };
        let coverage = flood(&pm, (5, 10), &options);
        let red = Rgba8::opaque(220, 0, 0);
        fill(&mut pm, &mask_of(&coverage, 40, 20), red, &options, (0, 0), None);

        assert_eq!(pm.get(5, 10), red);
        assert_eq!(pm.get(30, 10), red, "the far room was left alone");
    }

    #[test]
    fn tolerance_decides_what_counts_as_the_same_colour() {
        let mut pm = Pixmap::filled(20, 20, Rgba8::opaque(100, 100, 100));
        // A patch 20 levels off: inside a tolerance of 32, outside one of 10.
        pm.fill_rect(Rect::new(10, 0, 10, 20), Rgba8::opaque(120, 120, 120));

        let tight = BucketOptions { tolerance: 10, antialias: false, ..BucketOptions::default() };
        let mut a = pm.clone();
        let coverage = flood(&a, (2, 10), &tight);
        fill(&mut a, &mask_of(&coverage, 20, 20), Rgba8::BLACK, &tight, (0, 0), None);
        assert_eq!(a.get(15, 10), Rgba8::opaque(120, 120, 120), "a tight tolerance spread");

        let loose = BucketOptions { tolerance: 32, antialias: false, ..BucketOptions::default() };
        let mut b = pm.clone();
        let coverage = flood(&b, (2, 10), &loose);
        fill(&mut b, &mask_of(&coverage, 20, 20), Rgba8::BLACK, &loose, (0, 0), None);
        assert_eq!(b.get(15, 10), Rgba8::BLACK, "a loose tolerance did not reach the patch");
    }

    #[test]
    fn opacity_and_mode_apply_to_the_fill() {
        let mut pm = Pixmap::filled(10, 10, Rgba8::WHITE);
        let options = BucketOptions {
            opacity: 0.5,
            antialias: false,
            ..BucketOptions::default()
        };
        let coverage = flood(&pm, (5, 5), &options);
        fill(&mut pm, &mask_of(&coverage, 10, 10), Rgba8::BLACK, &options, (0, 0), None);
        let px = pm.get(5, 5);
        assert!((px.r as i32 - 128).abs() <= 2, "half opacity gave {}", px.r);
    }

    #[test]
    fn the_transparency_lock_keeps_the_fill_off_empty_pixels() {
        let mut pm = Pixmap::new(20, 20);
        pm.fill_rect(Rect::new(0, 0, 20, 10), Rgba8::WHITE);
        let options = BucketOptions {
            preserve_alpha: true,
            antialias: false,
            contiguous: false,
            ..BucketOptions::default()
        };
        // Clicking the empty half matches every transparent pixel; the lock must
        // stop any of them gaining coverage.
        let coverage = flood(&pm, (10, 15), &options);
        fill(&mut pm, &mask_of(&coverage, 20, 20), Rgba8::BLACK, &options, (0, 0), None);
        assert_eq!(pm.get(10, 15).a, 0, "the fill gave a transparent pixel coverage");
    }

    #[test]
    fn a_selection_confines_the_fill() {
        let mut pm = Pixmap::filled(40, 20, Rgba8::WHITE);
        let mut sel = Selection::new(40, 20);
        sel.apply_rect(Rect::new(0, 0, 20, 20), crate::selection::SelectionOp::Replace);
        let options = BucketOptions { antialias: false, ..BucketOptions::default() };
        let coverage = flood(&pm, (5, 10), &options);
        fill(&mut pm, &mask_of(&coverage, 40, 20), Rgba8::BLACK, &options, (0, 0),
             Some(&sel));

        assert_eq!(pm.get(5, 10), Rgba8::BLACK);
        assert_eq!(pm.get(30, 10), Rgba8::WHITE, "the fill escaped the selection");
    }

    #[test]
    fn a_mask_offset_from_the_layer_lands_in_the_right_place() {
        // What Sample All Layers produces: a document-space mask applied to a
        // layer that sits somewhere inside the canvas.
        let mut layer = Pixmap::filled(10, 10, Rgba8::WHITE);
        // A mask covering document (4,4)-(14,14), i.e. the layer's own (0,0) if
        // the layer sits at (4,4).
        let coverage = vec![255u8; 100];
        let mask = FloodMask { coverage: &coverage, width: 10, height: 10, origin: (0, 0) };
        let options = BucketOptions { antialias: false, ..BucketOptions::default() };
        let dirty = fill(&mut layer, &mask, Rgba8::BLACK, &options, (4, 4), None);
        assert_eq!(dirty, Rect::new(0, 0, 10, 10));
        assert_eq!(layer.get(9, 9), Rgba8::BLACK);
    }
}

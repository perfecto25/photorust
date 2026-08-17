//! Erasing by colour — the Background Eraser and the Magic Eraser.
//!
//! Both take alpha away rather than laying colour down, and both decide *which*
//! pixels by matching a colour, but they ask the question at different scales:
//!
//! - The **Background Eraser** is a brush. Under each dab it samples a colour —
//!   continuously, once at the start, or from the background swatch — and rubs
//!   out what matches, leaving what does not. Dragged along the edge of a
//!   subject with the crosshair on the background, it cuts the subject out.
//! - The **Magic Eraser** is one click: the same flood the Magic Wand makes,
//!   erased instead of selected.
//!
//! The deciding is shared with the Color Replacement Brush and lives in
//! [`crate::sample`]; what is left here is the erasing.
//!
//! Erasing is a *multiply* on alpha, never a subtraction: a pixel half erased
//! twice ends up three-quarters gone rather than fully, which is what makes
//! going over an edge again deepen it smoothly instead of punching through.

use crate::brush::Brush;
use crate::buffer::{Pixmap, Rect, Rgba8};
use crate::sample::{self, Limits, Sampling};

/// The Background Eraser's options bar.
#[derive(Clone, Copy, Debug, Default)]
pub struct BackgroundEraseOptions {
    pub sampling: Sampling,
    pub limits: Limits,
    /// 0-255, how far a pixel may differ per channel and still be erased. CS6
    /// shows this as a percentage; the conversion happens at the bridge.
    pub tolerance: u32,
    /// Never erase what matches the foreground colour — CS6's Protect
    /// Foreground Color, for keeping a colour that also appears in the
    /// background.
    pub protect_foreground: bool,
}

/// State carried across one Background Eraser stroke.
pub struct BackgroundEraser {
    options: BackgroundEraseOptions,
    /// The colour being erased, for the sampling modes that fix it up front.
    reference: Option<Rgba8>,
}

impl BackgroundEraser {
    /// Start a stroke. `reference` is used for Once and Background Swatch
    /// sampling; Continuous ignores it and reads the layer as it goes.
    pub fn new(options: BackgroundEraseOptions, reference: Option<Rgba8>) -> Self {
        Self { options, reference }
    }

    pub fn options(&self) -> BackgroundEraseOptions {
        self.options
    }

    /// Apply one dab, editing `pixels` in place. Returns the region changed.
    ///
    /// `(cx, cy)` is in the pixmap's own coordinates. `foreground` is the
    /// colour Protect Foreground Color keeps, and is only read when that option
    /// is on.
    pub fn apply_dab(
        &mut self,
        pixels: &mut Pixmap,
        brush: &Brush,
        cx: f32,
        cy: f32,
        pressure: f32,
        foreground: Rgba8,
    ) -> Rect {
        let radius = brush.radius() * pressure.clamp(0.05, 1.0);
        if radius <= 0.0 {
            return Rect::default();
        }

        let bounds = Rect::new(
            (cx - radius - 1.0).floor() as i32,
            (cy - radius - 1.0).floor() as i32,
            (radius * 2.0 + 3.0) as u32,
            (radius * 2.0 + 3.0) as u32,
        );
        let region = bounds.intersect(&pixels.rect());
        if region.is_empty() {
            return Rect::default();
        }

        // The colour to match against — the pixel under the crosshair, which is
        // why the Background Eraser is aimed at the background and not at the
        // edge it is cutting.
        let centre = (cx.floor() as i32, cy.floor() as i32);
        let reference = match self.options.sampling {
            Sampling::Continuous => {
                if !pixels.rect().contains(centre.0, centre.1) {
                    return Rect::default();
                }
                let under = pixels.get(centre.0, centre.1);
                // Sampling ground this stroke already cleared would take the
                // colour of nothing — transparent black — and go on to erase
                // everything dark. Skipping the dab leaves the hole alone.
                if under.a == 0 {
                    return Rect::default();
                }
                under
            }
            _ => match self.reference {
                Some(colour) => colour,
                None => return Rect::default(),
            },
        };

        // Contiguity is resolved within the dab, so an erase cannot jump an
        // edge into matching colour on the far side of the subject.
        let reachable = match self.options.limits {
            Limits::Discontiguous => None,
            limits => Some(sample::reachable(
                pixels,
                region,
                centre,
                reference,
                brush,
                cx,
                cy,
                radius,
                limits,
                self.options.tolerance,
                MATCH_ANTIALIAS,
            )),
        };

        let dab = Brush { size: radius * 2.0, ..*brush };
        let strength = (brush.flow * brush.opacity).clamp(0.0, 1.0);
        let mut dirty = Rect::default();

        for y in region.y..region.bottom() {
            for x in region.x..region.right() {
                let existing = pixels.get(x, y);
                if existing.a == 0 {
                    continue;
                }
                if let Some(mask) = reachable.as_ref() {
                    let local = ((y - region.y) as usize) * (region.width as usize)
                        + (x - region.x) as usize;
                    if !mask[local] {
                        continue;
                    }
                }

                let brush_cover = dab.pixel_coverage(
                    x as f32 + 0.5 - cx,
                    y as f32 + 0.5 - cy,
                    brush.angle,
                    brush.roundness,
                );
                if brush_cover <= 0.0 {
                    continue;
                }

                let matched = sample::match_strength(
                    existing,
                    reference,
                    self.options.tolerance,
                    MATCH_ANTIALIAS,
                );
                if matched <= 0.0 {
                    continue;
                }

                // Protect Foreground Color wins over the match: the point of it
                // is to keep a colour that would otherwise qualify.
                if self.options.protect_foreground
                    && sample::match_strength(
                        existing,
                        foreground,
                        self.options.tolerance,
                        MATCH_ANTIALIAS,
                    ) > 0.0
                {
                    continue;
                }

                let weight = (brush_cover * matched * strength).clamp(0.0, 1.0);
                if weight <= 0.0 {
                    continue;
                }

                let alpha = existing.a as f32 / 255.0 * (1.0 - weight);
                pixels.set(
                    x,
                    y,
                    Rgba8::new(existing.r, existing.g, existing.b, (alpha * 255.0 + 0.5) as u8),
                );
                dirty = dirty.union(&Rect::new(x, y, 1, 1));
            }
        }

        dirty
    }
}

/// The Background Eraser always softens the edge of what it matches.
///
/// The Color Replacement Brush offers this as a checkbox, but CS6's Background
/// Eraser has no such control and a hard match would leave a jagged fringe
/// exactly where the tool is used — along the edge of a subject.
const MATCH_ANTIALIAS: bool = true;

/// Erase `pixels` through a coverage mask — what the Magic Eraser commits.
///
/// `mask` is canvas-sized and indexed in document space; `offset` is the
/// layer's position, so a moved layer is still erased where the user clicked.
/// `opacity` scales how much is taken away, so a half-opacity Magic Eraser
/// leaves the region half there, as Photoshop's does.
pub fn erase_through_mask(
    pixels: &mut Pixmap,
    mask: &[u8],
    canvas_width: u32,
    offset: (i32, i32),
    opacity: f32,
) -> Rect {
    let opacity = opacity.clamp(0.0, 1.0);
    if opacity <= 0.0 || canvas_width == 0 {
        return Rect::default();
    }

    let mut dirty = Rect::default();
    let rows = mask.len() / canvas_width as usize;
    for y in 0..pixels.height() as i32 {
        let doc_y = y + offset.1;
        if doc_y < 0 || doc_y as usize >= rows {
            continue;
        }
        for x in 0..pixels.width() as i32 {
            let doc_x = x + offset.0;
            if doc_x < 0 || doc_x >= canvas_width as i32 {
                continue;
            }
            let coverage = mask[doc_y as usize * canvas_width as usize + doc_x as usize] as f32
                / 255.0
                * opacity;
            if coverage <= 0.0 {
                continue;
            }
            let existing = pixels.get(x, y);
            if existing.a == 0 {
                continue;
            }
            let alpha = existing.a as f32 / 255.0 * (1.0 - coverage);
            pixels.set(
                x,
                y,
                Rgba8::new(existing.r, existing.g, existing.b, (alpha * 255.0 + 0.5) as u8),
            );
            dirty = dirty.union(&Rect::new(doc_x, doc_y, 1, 1));
        }
    }
    dirty
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A layer split down the middle: matching background on the left, a
    /// subject on the right that must survive.
    fn split_layer() -> Pixmap {
        let mut pm = Pixmap::new(40, 20);
        pm.fill(Rgba8::opaque(30, 120, 220)); // "sky"
        pm.fill_rect(Rect::new(20, 0, 20, 20), Rgba8::opaque(220, 200, 40)); // "subject"
        pm
    }

    fn brush() -> Brush {
        Brush { size: 16.0, hardness: 1.0, spacing: 0.25, ..Brush::default() }
    }

    #[test]
    fn it_erases_what_it_sampled_and_leaves_what_it_did_not() {
        let mut pm = split_layer();
        let options = BackgroundEraseOptions {
            sampling: Sampling::Continuous,
            limits: Limits::Contiguous,
            tolerance: 40,
            protect_foreground: false,
        };
        let mut eraser = BackgroundEraser::new(options, None);
        // Crosshair on the sky, close enough that the dab overlaps the subject.
        eraser.apply_dab(&mut pm, &brush(), 16.0, 10.0, 1.0, Rgba8::BLACK);

        assert_eq!(pm.get(14, 10).a, 0, "the sampled colour was not erased");
        assert_eq!(pm.get(24, 10).a, 255, "the subject was erased along with it");
    }

    #[test]
    fn sampling_once_keeps_matching_the_first_colour() {
        // Dragging on over the subject with Once must not start erasing the
        // subject just because the crosshair has moved onto it.
        let mut pm = split_layer();
        let options = BackgroundEraseOptions {
            sampling: Sampling::Once,
            limits: Limits::Discontiguous,
            tolerance: 40,
            protect_foreground: false,
        };
        let sky = pm.get(4, 10);
        let mut eraser = BackgroundEraser::new(options, Some(sky));
        eraser.apply_dab(&mut pm, &brush(), 30.0, 10.0, 1.0, Rgba8::BLACK);

        assert_eq!(pm.get(30, 10).a, 255, "the subject was erased under the crosshair");
    }

    #[test]
    fn protecting_the_foreground_keeps_that_colour() {
        let mut pm = split_layer();
        let options = BackgroundEraseOptions {
            sampling: Sampling::Once,
            limits: Limits::Discontiguous,
            tolerance: 40,
            protect_foreground: true,
        };
        let sky = pm.get(4, 10);
        // Protecting exactly the colour being erased leaves nothing to do.
        let mut eraser = BackgroundEraser::new(options, Some(sky));
        eraser.apply_dab(&mut pm, &brush(), 8.0, 10.0, 1.0, sky);
        assert_eq!(pm.get(8, 10).a, 255, "a protected colour was erased anyway");
    }

    #[test]
    fn erasing_never_takes_more_than_everything() {
        let mut pm = Pixmap::new(20, 20);
        pm.fill(Rgba8::opaque(50, 50, 50));
        let options = BackgroundEraseOptions {
            sampling: Sampling::Once,
            limits: Limits::Discontiguous,
            tolerance: 255,
            protect_foreground: false,
        };
        let mut eraser = BackgroundEraser::new(options, Some(Rgba8::opaque(50, 50, 50)));
        for _ in 0..8 {
            eraser.apply_dab(&mut pm, &brush(), 10.0, 10.0, 1.0, Rgba8::BLACK);
        }
        assert_eq!(pm.get(10, 10).a, 0);
    }

    #[test]
    fn a_mask_erase_scales_with_opacity() {
        let mut pm = Pixmap::new(4, 4);
        pm.fill(Rgba8::opaque(10, 20, 30));
        let mask = vec![255u8; 16];

        erase_through_mask(&mut pm, &mask, 4, (0, 0), 0.5);
        let half = pm.get(1, 1);
        assert!((half.a as i32 - 128).abs() <= 1, "half-opacity erased {}", half.a);
        assert_eq!(half.r, 10, "erasing changed the colour, not just the alpha");

        erase_through_mask(&mut pm, &mask, 4, (0, 0), 1.0);
        assert_eq!(pm.get(1, 1).a, 0);
    }
}

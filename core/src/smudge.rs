//! The Smudge tool — a finger dragged through wet paint.
//!
//! The others in this group work on a pixel and its immediate neighbours; this
//! one **carries the image with it**. The brush holds a patch of pixels picked up
//! where it last was, lays that patch down where it is now, and picks the result
//! up again for the next dab. Structure gets dragged along the stroke, which is
//! why a smudged edge streaks in the direction of travel instead of merely going
//! soft — and it is the whole difference from Blur.
//!
//! Carrying a *patch* rather than one colour is what makes that work. A single
//! averaged colour would only ever spread a flat smear (that is closer to what
//! the Mixer Brush does with Mix at 100%); pulling a whole neighbourhood along is
//! what lets a smudge trail detail behind it.
//!
//! **Strength** is how much of the carried patch is laid down per dab: at 0 the
//! finger never touches the canvas, at 100 it replaces what it passes over
//! entirely and the stroke drags its starting pixels the whole way. **Finger
//! Painting** loads the finger with the foreground colour first, so the stroke
//! starts by dragging *paint* rather than what was already there.

use crate::blend::{blend_rgb, BlendMode};
use crate::brush::Brush;
use crate::buffer::{Pixmap, Rect, Rgba8};

/// The Smudge tool's options bar.
#[derive(Clone, Copy, Debug)]
pub struct SmudgeOptions {
    /// How much of the carried patch each dab lays down, `0.0..=1.0`.
    pub strength: f32,
    /// Which part of the pixel may change. The same cut-down list the focus
    /// tools offer.
    pub mode: BlendMode,
    /// Pick up from the composite rather than the active layer. The smear still
    /// lands on the active layer alone.
    pub sample_all_layers: bool,
    /// Start the stroke with the foreground colour on the finger.
    pub finger_painting: bool,
    /// The layer's Lock Transparent Pixels. Set from the layer, not the bar.
    pub preserve_alpha: bool,
}

impl Default for SmudgeOptions {
    fn default() -> Self {
        // CS6 opens on Strength 50%, Mode Normal, current layer, no finger
        // painting.
        Self {
            strength: 0.5,
            mode: BlendMode::Normal,
            sample_all_layers: false,
            finger_painting: false,
            preserve_alpha: false,
        }
    }
}

/// State carried across one smudge stroke: the patch on the finger.
pub struct Smudge {
    options: SmudgeOptions,
    /// The pixels being dragged, and where they were picked up from. Sized to
    /// the dab, so the patch is exactly what the tip covers.
    carried: Option<Carried>,
    /// The colour a finger-painting stroke starts loaded with.
    paint: Rgba8,
}

struct Carried {
    pixels: Pixmap,
    /// The centre the patch was picked up around, so it can be laid down
    /// aligned to the new dab rather than to the buffer's corner.
    centre: (f32, f32),
}

impl Smudge {
    /// Start a stroke. `paint` is the foreground colour, used only when Finger
    /// Painting is on.
    pub fn new(options: SmudgeOptions, paint: Rgba8) -> Self {
        Self { options, carried: None, paint }
    }

    pub fn options(&self) -> SmudgeOptions {
        self.options
    }

    /// Apply one dab, editing `pixels` in place. Returns the region changed.
    ///
    /// `(cx, cy)` is in the pixmap's own coordinates. `sampled` is what Sample
    /// All Layers picks up from, with its top-left in `pixels`' coordinates.
    pub fn apply_dab(
        &mut self,
        pixels: &mut Pixmap,
        sampled: Option<(&Pixmap, (i32, i32))>,
        brush: &Brush,
        cx: f32,
        cy: f32,
        pressure: f32,
    ) -> Rect {
        let radius = brush.radius() * pressure.clamp(0.05, 1.0);
        if radius <= 0.0 {
            return Rect::default();
        }
        let strength = self.options.strength.clamp(0.0, 1.0);

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

        let dab = Brush { size: radius * 2.0, ..*brush };
        let mut dirty = Rect::default();

        // Lay down what the finger is carrying, aligned to this dab: the pixel
        // taken is the one that sat at the same place under the *previous* dab,
        // which is what drags the image along rather than blurring it in place.
        if strength > 0.0 {
            if let Some(carried) = self.carried.as_ref() {
                let (dx, dy) = (cx - carried.centre.0, cy - carried.centre.1);
                for y in region.y..region.bottom() {
                    for x in region.x..region.right() {
                        let cover = dab.pixel_coverage(
                            x as f32 + 0.5 - cx,
                            y as f32 + 0.5 - cy,
                            brush.angle,
                            brush.roundness,
                        );
                        if cover <= 0.0 {
                            continue;
                        }
                        let mut weight = (cover * strength).clamp(0.0, 1.0);
                        if weight <= 0.0 {
                            continue;
                        }

                        let src = carried
                            .pixels
                            .get(x - dx.round() as i32, y - dy.round() as i32);

                        let dst = pixels.get(x, y);
                        if self.options.preserve_alpha {
                            if dst.a == 0 {
                                continue;
                            }
                            weight *= dst.a as f32 / 255.0;
                        }

                        let target = restrict(
                            dst,
                            src,
                            self.options.mode,
                            self.options.preserve_alpha,
                        );
                        let out = lerp(dst, target, weight);
                        if out != dst {
                            pixels.set(x, y, out);
                            dirty = dirty.union(&Rect::new(x, y, 1, 1));
                        }
                    }
                }
            }
        }

        // Pick up what is here now, ready for the next dab. Done after laying
        // down, so the finger carries the smear it just made — that feedback is
        // what makes a slow stroke drag further than a fast one.
        self.carried = Some(Carried {
            pixels: match self.carried.take() {
                // Finger painting starts the stroke loaded with paint; from the
                // second dab on it behaves as an ordinary smudge, so this only
                // fires once.
                None if self.options.finger_painting => {
                    let mut patch = Pixmap::new(pixels.width(), pixels.height());
                    patch.fill(self.paint);
                    patch
                }
                _ => match sampled {
                    Some((from, origin)) => shift(from, origin, pixels.width(), pixels.height()),
                    None => pixels.clone(),
                },
            },
            centre: (cx, cy),
        });

        dirty
    }
}

/// Copy `source` into a buffer the size of the layer, aligned to the layer's own
/// coordinates. Pixels the buffer does not cover come back transparent, and the
/// dab loop simply finds nothing to drag there.
fn shift(source: &Pixmap, origin: (i32, i32), width: u32, height: u32) -> Pixmap {
    let mut out = Pixmap::new(width.max(1), height.max(1));
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            out.set(x, y, source.get(x - origin.0, y - origin.1));
        }
    }
    out
}

/// Narrow the carried pixel to the part of it `mode` allows through — the same
/// restriction the focus tools apply, and for the same reason.
fn restrict(dst: Rgba8, carried: Rgba8, mode: BlendMode, preserve_alpha: bool) -> Rgba8 {
    if mode == BlendMode::Normal {
        return if preserve_alpha { Rgba8 { a: dst.a, ..carried } } else { carried };
    }
    let d = [dst.r as f32 / 255.0, dst.g as f32 / 255.0, dst.b as f32 / 255.0];
    let s = [
        carried.r as f32 / 255.0,
        carried.g as f32 / 255.0,
        carried.b as f32 / 255.0,
    ];
    let out = blend_rgb(mode, d, s);
    let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    Rgba8::new(c(out[0]), c(out[1]), c(out[2]), dst.a)
}

/// Straight-alpha interpolation.
fn lerp(from: Rgba8, to: Rgba8, t: f32) -> Rgba8 {
    let t = t.clamp(0.0, 1.0);
    let c = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8;
    Rgba8::new(c(from.r, to.r), c(from.g, to.g), c(from.b, to.b), c(from.a, to.a))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brush() -> Brush {
        Brush { size: 16.0, hardness: 1.0, ..Brush::default() }
    }

    /// A black bar down the left, white to the right of it.
    fn bar() -> Pixmap {
        let mut pm = Pixmap::filled(80, 40, Rgba8::WHITE);
        pm.fill_rect(Rect::new(0, 0, 20, 40), Rgba8::BLACK);
        pm
    }

    /// Drag from `from` to `to` along a row, one dab per pixel.
    fn drag(smudge: &mut Smudge, pm: &mut Pixmap, from: i32, to: i32, y: f32) {
        for x in from..=to {
            smudge.apply_dab(pm, None, &brush(), x as f32, y, 1.0);
        }
    }

    #[test]
    fn a_stroke_drags_colour_along_its_path() {
        // The point of the tool: colour picked up at the start is still being
        // laid down well past where it came from.
        let mut pm = bar();
        let mut smudge = Smudge::new(SmudgeOptions { strength: 0.9, ..SmudgeOptions::default() },
                                     Rgba8::WHITE);
        drag(&mut smudge, &mut pm, 12, 50, 20.0);

        let px = pm.get(40, 20);
        assert!(px.r < 240, "no black was carried out past the bar: {px:?}");
    }

    #[test]
    fn the_smear_fades_with_distance() {
        // A finger runs out: what it drags gets weaker the further it goes,
        // rather than painting a flat bar of the source colour.
        let mut pm = bar();
        let mut smudge = Smudge::new(SmudgeOptions { strength: 0.5, ..SmudgeOptions::default() },
                                     Rgba8::WHITE);
        drag(&mut smudge, &mut pm, 12, 60, 20.0);

        let near = pm.get(30, 20).r;
        let far = pm.get(50, 20).r;
        assert!(far > near, "the smear did not fade: {near} then {far}");
    }

    #[test]
    fn zero_strength_leaves_the_image_alone() {
        let mut pm = bar();
        let before = pm.clone();
        let mut smudge = Smudge::new(SmudgeOptions { strength: 0.0, ..SmudgeOptions::default() },
                                     Rgba8::WHITE);
        drag(&mut smudge, &mut pm, 12, 50, 20.0);
        assert_eq!(pm.get(30, 20), before.get(30, 20));
    }

    #[test]
    fn a_stronger_finger_drags_further() {
        let smear_reach = |strength: f32| {
            let mut pm = bar();
            let mut smudge =
                Smudge::new(SmudgeOptions { strength, ..SmudgeOptions::default() }, Rgba8::WHITE);
            drag(&mut smudge, &mut pm, 12, 70, 20.0);
            (20..70).filter(|x| pm.get(*x, 20).r < 240).count()
        };
        assert!(smear_reach(0.9) > smear_reach(0.3),
                "strength did not change how far the smear carried");
    }

    #[test]
    fn the_first_dab_only_picks_up() {
        // There is nothing on the finger yet, so putting it down must not change
        // anything — otherwise a click would stamp a patch of nowhere.
        let mut pm = bar();
        let before = pm.clone();
        let mut smudge = Smudge::new(SmudgeOptions { strength: 1.0, ..SmudgeOptions::default() },
                                     Rgba8::WHITE);
        assert!(smudge.apply_dab(&mut pm, None, &brush(), 30.0, 20.0, 1.0).is_empty());
        assert_eq!(pm.get(30, 20), before.get(30, 20));
    }

    #[test]
    fn finger_painting_drags_the_foreground_colour_in() {
        // Loaded with paint, the stroke starts by laying that down rather than
        // whatever happened to be under the first dab.
        let mut pm = Pixmap::filled(80, 40, Rgba8::WHITE);
        let red = Rgba8::opaque(220, 20, 20);
        let options = SmudgeOptions {
            strength: 0.9,
            finger_painting: true,
            ..SmudgeOptions::default()
        };
        let mut smudge = Smudge::new(options, red);
        drag(&mut smudge, &mut pm, 20, 40, 20.0);

        let px = pm.get(24, 20);
        assert!(px.r > px.g + 40, "the foreground colour was not dragged in: {px:?}");
    }

    #[test]
    fn the_transparency_lock_keeps_the_smear_off_empty_pixels() {
        let mut pm = Pixmap::new(80, 40);
        pm.fill_rect(Rect::new(0, 0, 40, 40), Rgba8::opaque(240, 200, 40));
        let options = SmudgeOptions {
            strength: 1.0,
            preserve_alpha: true,
            ..SmudgeOptions::default()
        };
        let mut smudge = Smudge::new(options, Rgba8::WHITE);
        drag(&mut smudge, &mut pm, 30, 60, 20.0);
        assert_eq!(pm.get(55, 20).a, 0, "the smear spread coverage past the lock");
    }

    #[test]
    fn sample_all_layers_picks_up_from_the_buffer_it_is_given() {
        // The finger picks up from the composite and lays down on the layer. Note
        // what a *fixed* buffer means: the finger reloads from the same unchanged
        // pixels every dab, so the smear reaches one dab past the boundary and no
        // further. In the document the composite is rebuilt per dab, which is
        // what restores the feedback that carries a smear onward.
        let mut layer = Pixmap::filled(80, 40, Rgba8::WHITE);
        let mut composite = Pixmap::filled(80, 40, Rgba8::WHITE);
        composite.fill_rect(Rect::new(0, 0, 20, 40), Rgba8::BLACK);

        let mut smudge = Smudge::new(SmudgeOptions { strength: 0.9, ..SmudgeOptions::default() },
                                     Rgba8::WHITE);
        for x in 12..=40 {
            smudge.apply_dab(&mut layer, Some((&composite, (0, 0))), &brush(), x as f32, 20.0,
                             1.0);
        }
        assert!(layer.get(20, 20).r < 240, "nothing was picked up from the composite");
        // Only where the tip actually went: the first dab reaches back a radius
        // from x=12, and nothing beyond that is touched.
        assert_eq!(layer.get(2, 20), Rgba8::WHITE, "the layer was painted outside the stroke");
    }
}

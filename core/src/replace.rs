//! Colour replacement — the engine behind the Color Replacement Brush.
//!
//! The tool paints the foreground colour, but only onto pixels that already
//! resemble a *sampled* colour, and only the part of them the chosen mode
//! affects. Painting over black hair with Color mode recolours it while keeping
//! every strand's shading, because the pixel's luminosity is preserved and only
//! its hue and saturation are replaced.
//!
//! Unlike an ordinary stroke, this cannot accumulate into a mask and be
//! composited once at the end: what gets replaced depends on what is already
//! there, and with continuous sampling the reference colour changes as the brush
//! moves. So it applies per dab, straight into the layer, with a record of which
//! pixels have already been dealt with so a slow drag does not build up.

use crate::blend::{blend_rgb, BlendMode};
use crate::brush::Brush;
use crate::buffer::{Pixmap, Rect, Rgba8};
use crate::sample;

/// Which part of the pixel the replacement affects. CS6's Mode menu.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(i32)]
pub enum ReplaceMode {
    Hue = 0,
    Saturation = 1,
    /// Hue and saturation, keeping the pixel's own brightness. CS6's default,
    /// and the one that makes the tool useful.
    #[default]
    Color = 2,
    Luminosity = 3,
}

impl ReplaceMode {
    pub fn from_i32(v: i32) -> ReplaceMode {
        match v {
            0 => ReplaceMode::Hue,
            1 => ReplaceMode::Saturation,
            3 => ReplaceMode::Luminosity,
            _ => ReplaceMode::Color,
        }
    }

    fn blend(self) -> BlendMode {
        match self {
            ReplaceMode::Hue => BlendMode::Hue,
            ReplaceMode::Saturation => BlendMode::Saturation,
            ReplaceMode::Color => BlendMode::Color,
            ReplaceMode::Luminosity => BlendMode::Luminosity,
        }
    }
}

/// Sampling and Limits are the same buttons the Background Eraser has, and the
/// same code answers them for both — see [`crate::sample`]. They keep their
/// tool-flavoured names here because that is what the bar calls them.
pub use crate::sample::{Limits as ReplaceLimits, Sampling as ReplaceSampling};

/// Settings for one colour-replacement stroke.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReplaceOptions {
    pub mode: ReplaceMode,
    pub sampling: ReplaceSampling,
    pub limits: ReplaceLimits,
    /// 0-255, how far a pixel may differ per channel and still count as a match.
    pub tolerance: u32,
    /// Feather the edge of the matched region rather than cutting it hard.
    pub antialias: bool,
}

/// State carried across one stroke.
pub struct ColorReplacer {
    options: ReplaceOptions,
    /// The colour being replaced, for the sampling modes that fix it up front.
    reference: Option<Rgba8>,
    /// Pixels already replaced during this stroke.
    ///
    /// Without this, overlapping dabs would blend the replacement in repeatedly
    /// and a slow drag would come out stronger than a fast one — the same reason
    /// ordinary strokes accumulate coverage with a maximum rather than a sum.
    done: Vec<bool>,
    width: u32,
}

impl ColorReplacer {
    /// Start a stroke. `reference` is used for Once and Background Swatch
    /// sampling; Continuous ignores it and reads the layer as it goes.
    pub fn new(width: u32, height: u32, options: ReplaceOptions, reference: Option<Rgba8>) -> Self {
        Self {
            options,
            reference,
            done: vec![false; (width as usize) * (height as usize)],
            width,
        }
    }

    pub fn options(&self) -> ReplaceOptions {
        self.options
    }

    /// Apply one dab, editing `pixels` in place. Returns the region changed.
    ///
    /// `(cx, cy)` is in the pixmap's own coordinates. `replacement` is the colour
    /// being painted — the foreground.
    pub fn apply_dab(
        &mut self,
        pixels: &mut Pixmap,
        brush: &Brush,
        cx: f32,
        cy: f32,
        pressure: f32,
        replacement: Rgba8,
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

        // The colour to match against.
        let centre = (cx.floor() as i32, cy.floor() as i32);
        let reference = match self.options.sampling {
            ReplaceSampling::Continuous => {
                if !pixels.rect().contains(centre.0, centre.1) {
                    return Rect::default();
                }
                pixels.get(centre.0, centre.1)
            }
            _ => match self.reference {
                Some(colour) => colour,
                None => return Rect::default(),
            },
        };

        // Contiguity is resolved within the dab: a flood from the centre, so the
        // replacement cannot jump a boundary even if the far side matches.
        let reachable = match self.options.limits {
            ReplaceLimits::Discontiguous => None,
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
                self.options.antialias,
            )),
        };

        let dab = Brush { size: radius * 2.0, ..*brush };
        let flow = (brush.flow * brush.opacity).clamp(0.0, 1.0);
        let mut dirty = Rect::default();

        for y in region.y..region.bottom() {
            for x in region.x..region.right() {
                let index = (y as usize) * (self.width as usize) + x as usize;
                if self.done[index] {
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

                let existing = pixels.get(x, y);
                let match_strength = sample::match_strength(
                    existing,
                    reference,
                    self.options.tolerance,
                    self.options.antialias,
                );
                if match_strength <= 0.0 {
                    continue;
                }

                let weight = (brush_cover * match_strength * flow).clamp(0.0, 1.0);
                if weight <= 0.0 {
                    continue;
                }

                pixels.set(x, y, mix(existing, replacement, self.options.mode, weight));
                // Fully replaced pixels are finished; partly covered ones stay
                // open so the rest of the stroke can complete them.
                if weight >= 0.995 {
                    self.done[index] = true;
                }
                dirty = dirty.union(&Rect::new(x, y, 1, 1));
            }
        }

        dirty
    }

}

/// Blend `replacement` into `base` under `mode`, at strength `weight`.
fn mix(base: Rgba8, replacement: Rgba8, mode: ReplaceMode, weight: f32) -> Rgba8 {
    // Fully transparent pixels have no colour to keep, so there is nothing to
    // replace — painting them would create colour out of nothing.
    if base.a == 0 {
        return base;
    }

    let to_f = |c: Rgba8| {
        [
            c.r as f32 / 255.0,
            c.g as f32 / 255.0,
            c.b as f32 / 255.0,
        ]
    };
    let blended = blend_rgb(mode.blend(), to_f(base), to_f(replacement));

    let lerp = |from: f32, to: f32| from + (to - from) * weight;
    let out = |channel: usize, original: u8| {
        (lerp(original as f32 / 255.0, blended[channel]) * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Rgba8::new(out(0, base.r), out(1, base.g), out(2, base.b), base.a)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(mode: ReplaceMode) -> ReplaceOptions {
        ReplaceOptions {
            mode,
            sampling: ReplaceSampling::Continuous,
            limits: ReplaceLimits::Discontiguous,
            tolerance: 60,
            antialias: false,
        }
    }

    fn hard_brush(size: f32) -> Brush {
        Brush { size, hardness: 1.0, ..Brush::default() }
    }

    #[test]
    fn color_mode_keeps_the_pixel_s_brightness() {
        // The point of the tool: recolouring shaded material must keep its
        // shading, otherwise it reads as a flat sticker.
        let mut pm = Pixmap::new(40, 40);
        for y in 0..40 {
            for x in 0..40 {
                // A grey ramp: same hue, different brightness.
                let v = (60 + x * 3).min(255) as u8;
                pm.set(x, y, Rgba8::new(v, v, v, 255));
            }
        }
        let before = (pm.get(14, 20), pm.get(24, 20));

        let mut replacer =
            ColorReplacer::new(40, 40, options(ReplaceMode::Color), None);
        // Tolerance wide enough to cover the ramp under the brush.
        replacer.options.tolerance = 200;
        replacer.apply_dab(&mut pm, &hard_brush(24.0), 20.0, 20.0, 1.0,
                           Rgba8::new(200, 40, 40, 255));

        let after = (pm.get(14, 20), pm.get(24, 20));
        // Both went red...
        assert!(after.0.r > after.0.g + 20, "the darker pixel did not take the colour");
        assert!(after.1.r > after.1.g + 20, "the lighter pixel did not take the colour");
        // ...and the darker one is still darker than the lighter one.
        let luma = |c: Rgba8| 0.299 * c.r as f32 + 0.587 * c.g as f32 + 0.114 * c.b as f32;
        assert!(
            luma(after.0) < luma(after.1),
            "the shading was flattened: {:?} then {:?}",
            after.0,
            after.1
        );
        assert!(luma(after.0) - luma(before.0) < 30.0, "brightness shifted too far");
    }

    #[test]
    fn luminosity_mode_keeps_the_pixel_s_colour() {
        let mut pm = Pixmap::filled(30, 30, Rgba8::new(200, 60, 60, 255));
        let mut replacer =
            ColorReplacer::new(30, 30, options(ReplaceMode::Luminosity), None);
        replacer.apply_dab(&mut pm, &hard_brush(16.0), 15.0, 15.0, 1.0,
                           Rgba8::new(30, 30, 30, 255));

        let px = pm.get(15, 15);
        // Darkened, but still recognisably red rather than grey.
        assert!(px.r > px.g + 10, "the hue was lost: {:?}", px);
        assert!(px.r < 200, "the pixel was not darkened: {:?}", px);
    }

    #[test]
    fn only_matching_pixels_are_replaced() {
        // Two halves; painting over the boundary must leave the far half alone.
        let mut pm = Pixmap::new(60, 30);
        for y in 0..30 {
            for x in 0..60 {
                let c = if x < 30 {
                    Rgba8::new(40, 40, 200, 255)
                } else {
                    Rgba8::new(220, 220, 40, 255)
                };
                pm.set(x, y, c);
            }
        }
        let before = pm.get(40, 15);

        let mut replacer =
            ColorReplacer::new(60, 30, options(ReplaceMode::Color), None);
        // Brush straddles the boundary, sampling the blue side.
        replacer.apply_dab(&mut pm, &hard_brush(30.0), 25.0, 15.0, 1.0,
                           Rgba8::new(40, 200, 40, 255));

        assert_ne!(pm.get(20, 15), Rgba8::new(40, 40, 200, 255), "the blue was not replaced");
        assert_eq!(pm.get(40, 15), before, "the yellow side was altered");
    }

    #[test]
    fn tolerance_widens_what_counts_as_a_match() {
        let build = || {
            let mut pm = Pixmap::new(40, 40);
            for y in 0..40 {
                for x in 0..40 {
                    let v = (100 + x * 2).min(255) as u8;
                    pm.set(x, y, Rgba8::new(v, v, v, 255));
                }
            }
            pm
        };

        let count_changed = |tolerance: u32| {
            let base = build();
            let mut pm = build();
            let mut o = options(ReplaceMode::Color);
            o.tolerance = tolerance;
            let mut replacer = ColorReplacer::new(40, 40, o, None);
            replacer.apply_dab(&mut pm, &hard_brush(36.0), 20.0, 20.0, 1.0,
                               Rgba8::new(220, 20, 20, 255));
            let mut n = 0;
            for y in 0..40 {
                for x in 0..40 {
                    if pm.get(x, y) != base.get(x, y) {
                        n += 1;
                    }
                }
            }
            n
        };

        let tight = count_changed(5);
        let wide = count_changed(120);
        assert!(wide > tight * 2, "tolerance had little effect: {} vs {}", tight, wide);
    }

    #[test]
    fn contiguous_limits_stop_at_a_gap() {
        // Two matching stripes with a non-matching one between them. Contiguous
        // sampling from the left stripe must not reach the right one.
        let mut pm = Pixmap::new(60, 20);
        for y in 0..20 {
            for x in 0..60 {
                let c = if x < 20 || x >= 40 {
                    Rgba8::new(50, 50, 50, 255)
                } else {
                    Rgba8::new(240, 240, 240, 255)
                };
                pm.set(x, y, c);
            }
        }
        let far_before = pm.get(45, 10);

        let mut o = options(ReplaceMode::Color);
        o.limits = ReplaceLimits::Contiguous;
        o.tolerance = 40;
        let mut replacer = ColorReplacer::new(60, 20, o, None);
        // Centred on the near stripe's edge, with a radius that genuinely
        // reaches into the far one — otherwise the test proves nothing.
        replacer.apply_dab(&mut pm, &hard_brush(58.0), 19.0, 10.0, 1.0,
                           Rgba8::new(200, 40, 40, 255));

        assert_ne!(pm.get(10, 10), Rgba8::new(50, 50, 50, 255), "the near stripe was skipped");
        assert_eq!(pm.get(45, 10), far_before, "contiguous mode jumped the gap");
    }

    #[test]
    fn discontiguous_limits_reach_across_a_gap() {
        let mut pm = Pixmap::new(60, 20);
        for y in 0..20 {
            for x in 0..60 {
                let c = if x < 20 || x >= 40 {
                    Rgba8::new(50, 50, 50, 255)
                } else {
                    Rgba8::new(240, 240, 240, 255)
                };
                pm.set(x, y, c);
            }
        }

        let mut o = options(ReplaceMode::Color);
        o.limits = ReplaceLimits::Discontiguous;
        o.tolerance = 40;
        let mut replacer = ColorReplacer::new(60, 20, o, None);
        replacer.apply_dab(&mut pm, &hard_brush(58.0), 19.0, 10.0, 1.0,
                           Rgba8::new(200, 40, 40, 255));

        assert_ne!(pm.get(45, 10), Rgba8::new(50, 50, 50, 255),
                   "discontiguous mode did not reach the far stripe");
    }

    #[test]
    fn a_pixel_is_not_replaced_twice_in_one_stroke() {
        // Repeated dabs on the same spot must not compound, or a slow drag would
        // come out stronger than a fast one.
        let mut pm = Pixmap::filled(30, 30, Rgba8::new(120, 120, 120, 255));
        let mut replacer = ColorReplacer::new(30, 30, options(ReplaceMode::Color), None);
        let brush = hard_brush(16.0);

        replacer.apply_dab(&mut pm, &brush, 15.0, 15.0, 1.0, Rgba8::new(220, 30, 30, 255));
        let once = pm.get(15, 15);
        for _ in 0..5 {
            replacer.apply_dab(&mut pm, &brush, 15.0, 15.0, 1.0, Rgba8::new(220, 30, 30, 255));
        }
        assert_eq!(pm.get(15, 15), once, "repeated dabs compounded");
    }

    #[test]
    fn transparent_pixels_are_left_alone() {
        // There is no colour there to replace; painting one would invent it.
        let mut pm = Pixmap::new(30, 30);
        let mut replacer = ColorReplacer::new(30, 30, options(ReplaceMode::Color), None);
        replacer.apply_dab(&mut pm, &hard_brush(20.0), 15.0, 15.0, 1.0,
                           Rgba8::new(200, 40, 40, 255));
        assert_eq!(pm.get(15, 15).a, 0, "a transparent pixel was painted");
    }

    #[test]
    fn background_swatch_sampling_uses_the_given_colour() {
        // Nothing is sampled from the image: only pixels matching the reference
        // are touched, wherever the brush happens to be.
        let mut pm = Pixmap::new(40, 20);
        for y in 0..20 {
            for x in 0..40 {
                let c = if y < 10 {
                    Rgba8::new(20, 200, 20, 255)
                } else {
                    Rgba8::new(200, 20, 20, 255)
                };
                pm.set(x, y, c);
            }
        }

        let mut o = options(ReplaceMode::Color);
        o.sampling = ReplaceSampling::BackgroundSwatch;
        o.tolerance = 40;
        let mut replacer =
            ColorReplacer::new(40, 20, o, Some(Rgba8::new(20, 200, 20, 255)));
        replacer.apply_dab(&mut pm, &hard_brush(30.0), 20.0, 10.0, 1.0,
                           Rgba8::new(20, 20, 200, 255));

        // The green half changed; the red half did not.
        assert_ne!(pm.get(20, 5), Rgba8::new(20, 200, 20, 255), "the green was not replaced");
        assert_eq!(pm.get(20, 15), Rgba8::new(200, 20, 20, 255), "the red was altered");
    }

    #[test]
    fn modes_round_trip_through_their_integers() {
        for (v, mode) in [
            (0, ReplaceMode::Hue),
            (1, ReplaceMode::Saturation),
            (2, ReplaceMode::Color),
            (3, ReplaceMode::Luminosity),
        ] {
            assert_eq!(ReplaceMode::from_i32(v), mode);
            assert_eq!(mode as i32, v);
        }
        assert_eq!(ReplaceMode::from_i32(99), ReplaceMode::Color);
        assert_eq!(ReplaceSampling::from_i32(99), ReplaceSampling::Continuous);
        assert_eq!(ReplaceLimits::from_i32(99), ReplaceLimits::Contiguous);
    }
}

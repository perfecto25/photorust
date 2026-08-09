//! Wet paint — the engine behind the Mixer Brush.
//!
//! The Mixer Brush models a bristle brush carrying a finite amount of paint
//! over a still-wet canvas. Two colours meet at every dab: the **reservoir**,
//! the paint the brush is loaded with, and the **pickup**, the colour it finds
//! under the tip. Four numbers decide what happens between them, and they are
//! exactly CS6's:
//!
//! * **Wet** — how wet the *canvas* is. At zero the paint sits on top and the
//!   tool is an ordinary brush; the higher it goes the more the existing colour
//!   takes part, both in what is deposited and in dragging colour along.
//! * **Load** — how much paint the brush holds. It runs down as the stroke goes
//!   and, once empty, a dry brush stops depositing while a wet one keeps
//!   smearing what is already there.
//! * **Mix** — the ratio between the two colours once the canvas is wet: 0%
//!   deposits reservoir paint alone, 100% deposits pure canvas pickup, which is
//!   a smear with no colour of its own.
//! * **Flow** — how fast each dab deposits, as with any brush.
//!
//! Like colour replacement, and unlike an ordinary stroke, this cannot
//! accumulate into a mask and composite once at the end: every dab reads what
//! the previous ones left, so it applies straight into the layer as it goes.
//! The reservoir survives the stroke, because in Photoshop it survives too —
//! the brush stays loaded until it is cleaned or reloaded.

use crate::brush::Brush;
use crate::buffer::{Pixmap, Rect, Rgba8};

/// Settings for one mixer stroke — CS6's Wet, Load, Mix and Flow, plus the two
/// checkboxes that decide what the brush does between strokes.
#[derive(Clone, Copy, Debug)]
pub struct MixerOptions {
    /// How wet the canvas is, `0.0..=1.0`.
    pub wet: f32,
    /// How much paint the brush starts with, `0.0..=1.0`.
    pub load: f32,
    /// Canvas-to-reservoir ratio, `0.0..=1.0`. Only meaningful when wet.
    pub mix: f32,
    /// Deposit rate per dab, `0.0..=1.0`.
    pub flow: f32,
    /// Pick up colour from every visible layer rather than the active one. The
    /// result is still written to the active layer alone.
    pub sample_all_layers: bool,
    /// The layer's Lock Transparent Pixels: paint may change what is already
    /// there but must not give a transparent pixel any alpha.
    pub preserve_alpha: bool,
}

impl Default for MixerOptions {
    fn default() -> Self {
        // CS6 opens on Wet 0, Load 50, Mix 0, Flow 100 — the "Dry" preset.
        Self {
            wet: 0.0,
            load: 0.5,
            mix: 0.0,
            flow: 1.0,
            sample_all_layers: false,
            preserve_alpha: false,
        }
    }
}

/// How much of the load a fully wet brush uses over one dab's worth of travel at
/// the default 25% spacing.
///
/// Small enough that a well-loaded brush covers a good stroke before running
/// thin, large enough that "Dry, Light Load" runs out almost at once — which is
/// the whole point of that preset. Scaled by the actual spacing below: paint is
/// spent per distance covered, not per dab, or a finely spaced tip would drain
/// the brush in a fraction of the stroke.
const DAB_CONSUMPTION: f32 = 0.02;

/// The spacing `DAB_CONSUMPTION` is quoted at.
const REFERENCE_SPACING: f32 = 0.25;

/// How readily the reservoir takes on the colour it passes over, scaled by wet
/// and mix. This is what carries colour along a smear instead of every dab
/// re-mixing from the same starting paint.
const PICKUP_RATE: f32 = 0.5;

/// Pixels to read the pickup colour from when it is not the layer being
/// painted — what Sample All Layers composites.
///
/// `origin` is where the buffer's top-left sits in the *layer's* coordinates, so
/// only the neighbourhood of a dab need be composited rather than the whole
/// document.
pub struct Sampled<'a> {
    pub pixels: &'a Pixmap,
    pub origin: (i32, i32),
}

/// State carried across one mixer stroke.
pub struct MixerBrush {
    options: MixerOptions,
    /// The paint on the brush. Straight alpha, like everything else.
    reservoir: Rgba8,
    /// What is left of the load, `0.0..=1.0`.
    paint: f32,
}

impl MixerBrush {
    /// Load a brush and start a stroke. `reservoir` is the paint on it — the
    /// foreground colour after a Load Brush, or whatever the last stroke left.
    pub fn new(options: MixerOptions, reservoir: Rgba8) -> Self {
        Self { options, reservoir, paint: options.load.clamp(0.0, 1.0) }
    }

    pub fn options(&self) -> MixerOptions {
        self.options
    }

    /// The paint currently on the brush, for the options bar's load swatch.
    pub fn reservoir(&self) -> Rgba8 {
        self.reservoir
    }

    /// Apply one dab, editing `pixels` in place. Returns the region changed.
    ///
    /// `(cx, cy)` is in the pixmap's own coordinates. `sampled`, when given, is
    /// what Sample All Layers reads the pickup colour from.
    pub fn apply_dab(
        &mut self,
        pixels: &mut Pixmap,
        sampled: Option<Sampled<'_>>,
        brush: &Brush,
        cx: f32,
        cy: f32,
        pressure: f32,
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

        let wet = self.options.wet.clamp(0.0, 1.0);
        let mix = self.options.mix.clamp(0.0, 1.0);
        let flow = (self.options.flow * brush.opacity).clamp(0.0, 1.0);
        if flow <= 0.0 {
            return Rect::default();
        }
        // A dry brush with nothing left on it has nothing to give, and nothing
        // to smear with either.
        if wet <= 0.0 && self.paint <= 0.0 {
            return Rect::default();
        }

        let dab = Brush { size: radius * 2.0, ..*brush };

        // The colour the brush finds under the tip: the dab's own coverage-
        // weighted average, so a soft tip is led by what is under its middle.
        // Averaging over the whole dab rather than reading the centre pixel is
        // what makes the tool blend rather than clone.
        let pickup = match sampled.as_ref() {
            Some(from) => average_under_dab(from.pixels, from.origin, &dab, brush, region, cx, cy),
            None => average_under_dab(pixels, (0, 0), &dab, brush, region, cx, cy),
        };

        // What actually leaves the brush. Dry paints the reservoir as it is; wet
        // paint is thinned with what the canvas offers, in the ratio Mix asks
        // for. Running out of paint slides the balance to pure canvas, which is
        // how an empty wet brush turns into a smudge.
        let strength = if wet <= 0.0 { 1.0 } else { self.paint.min(1.0) };
        let canvas_share = if wet <= 0.0 { 0.0 } else { mix + (1.0 - mix) * (1.0 - strength) };
        let deposit = match pickup {
            Some(found) => lerp_rgba(self.reservoir, found, canvas_share),
            // Nothing but transparency under the tip — there is no canvas
            // colour to mix in, so the brush lays down its own paint.
            None => self.reservoir,
        };

        // Wet canvas resists a little: at Wet 100 the deposit still only carries
        // so far per dab, which is what leaves the streaked, worked look rather
        // than a flat fill.
        let deposit_alpha = flow * (1.0 - wet * 0.5);
        let mut dirty = Rect::default();

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
                let weight = (cover * deposit_alpha).clamp(0.0, 1.0);
                if weight <= 0.0 {
                    continue;
                }
                let existing = pixels.get(x, y);
                let mut mixed = lerp_rgba(existing, deposit, weight);
                if self.options.preserve_alpha {
                    // Lock Transparent Pixels: colour may change, coverage may
                    // not — and a fully transparent pixel is left alone, since
                    // there is nothing there to recolour.
                    if existing.a == 0 {
                        continue;
                    }
                    mixed.a = existing.a;
                }
                pixels.set(x, y, mixed);
                dirty = dirty.union(&Rect::new(x, y, 1, 1));
            }
        }

        // The brush carries what it picked up on to the next dab, and loses a
        // little paint doing it. Both only happen on a wet canvas — a dry brush
        // neither picks up nor runs out, exactly as in CS6, where Load has no
        // effect until Wet is above zero.
        if wet > 0.0 {
            if let Some(found) = pickup {
                let absorb = (wet * mix * PICKUP_RATE).clamp(0.0, 1.0);
                self.reservoir = lerp_rgba(self.reservoir, found, absorb);
            }
            let travel = (brush.spacing.max(0.01) / REFERENCE_SPACING).clamp(0.05, 4.0);
            self.paint = (self.paint - wet * DAB_CONSUMPTION * travel).max(0.0);
        }

        dirty
    }
}

/// The coverage-weighted average colour under a dab, or `None` where the dab
/// finds nothing but transparency.
///
/// Averaging is done on premultiplied values so a nearly transparent pixel
/// contributes its colour only as far as it is actually there — otherwise the
/// black behind cleared pixels would drag the average down.
fn average_under_dab(
    source: &Pixmap,
    origin: (i32, i32),
    dab: &Brush,
    brush: &Brush,
    region: Rect,
    cx: f32,
    cy: f32,
) -> Option<Rgba8> {
    // `region` is in layer coordinates; the buffer may cover only part of it.
    let covered = source.rect();
    let region = region.intersect(&Rect::new(
        covered.x + origin.0,
        covered.y + origin.1,
        covered.width,
        covered.height,
    ));
    if region.is_empty() {
        return None;
    }

    let (mut r, mut g, mut b, mut a) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let mut total = 0.0f32;
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
            let px = source.get(x - origin.0, y - origin.1);
            let alpha = px.a as f32 / 255.0;
            r += px.r as f32 * alpha * cover;
            g += px.g as f32 * alpha * cover;
            b += px.b as f32 * alpha * cover;
            a += alpha * cover;
            total += cover;
        }
    }
    if total <= 0.0 || a <= 1e-6 {
        return None;
    }

    // Back to straight alpha: the colour is the average of what was there, the
    // alpha the average of how much of it there was.
    Some(Rgba8::new(
        (r / a).round().clamp(0.0, 255.0) as u8,
        (g / a).round().clamp(0.0, 255.0) as u8,
        (b / a).round().clamp(0.0, 255.0) as u8,
        ((a / total) * 255.0).round().clamp(0.0, 255.0) as u8,
    ))
}

/// Straight-alpha interpolation from `from` towards `to`.
fn lerp_rgba(from: Rgba8, to: Rgba8, t: f32) -> Rgba8 {
    let t = t.clamp(0.0, 1.0);
    let channel = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8;
    Rgba8::new(
        channel(from.r, to.r),
        channel(from.g, to.g),
        channel(from.b, to.b),
        channel(from.a, to.a),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brush() -> Brush {
        Brush { size: 10.0, hardness: 1.0, ..Brush::default() }
    }

    fn options() -> MixerOptions {
        MixerOptions {
            wet: 0.0,
            load: 1.0,
            mix: 0.0,
            flow: 1.0,
            sample_all_layers: false,
            preserve_alpha: false,
        }
    }

    #[test]
    fn a_dry_brush_paints_its_own_paint() {
        // Wet 0 is CS6's "Dry" preset: the tool behaves as an ordinary brush and
        // the canvas colour has no say at all.
        let mut pixels = Pixmap::filled(32, 32, Rgba8::WHITE);
        let red = Rgba8::opaque(220, 20, 20);
        let mut mixer = MixerBrush::new(options(), red);
        assert!(!mixer.apply_dab(&mut pixels, None, &brush(), 16.0, 16.0, 1.0).is_empty());
        assert_eq!(pixels.get(16, 16), red);
    }

    #[test]
    fn a_full_mix_smears_the_canvas_without_adding_colour() {
        // Mix 100 on a wet canvas deposits pure pickup, so a stroke started on
        // white can only ever leave white behind — no trace of the load.
        let mut pixels = Pixmap::filled(32, 32, Rgba8::WHITE);
        let opts = MixerOptions { wet: 1.0, mix: 1.0, ..options() };
        let mut mixer = MixerBrush::new(opts, Rgba8::opaque(220, 20, 20));
        for step in 0..8 {
            mixer.apply_dab(&mut pixels, None, &brush(), 8.0 + step as f32, 16.0, 1.0);
        }
        assert_eq!(pixels.get(12, 16), Rgba8::WHITE, "the load leaked into a pure smear");
    }

    #[test]
    fn a_wet_brush_drags_colour_along_the_stroke() {
        // The point of the tool: paint picked up at one end of the stroke must
        // still be visible some way past the boundary it was picked up at.
        let mut pixels = Pixmap::filled(64, 32, Rgba8::WHITE);
        pixels.fill_rect(Rect::new(0, 0, 20, 32), Rgba8::opaque(20, 20, 220));
        let opts = MixerOptions { wet: 0.8, mix: 1.0, load: 1.0, ..options() };
        let mut mixer = MixerBrush::new(opts, Rgba8::WHITE);
        for x in 10..40 {
            mixer.apply_dab(&mut pixels, None, &brush(), x as f32, 16.0, 1.0);
        }
        let past = pixels.get(28, 16);
        assert!(past.b > past.r + 10, "no blue was carried past the boundary: {past:?}");
        assert!(past.r > 40, "the smear stayed pure blue instead of thinning out");
    }

    #[test]
    fn an_empty_wet_brush_keeps_smearing() {
        // Load runs out mid-stroke; a wet brush must go on moving colour about
        // rather than stopping dead.
        let mut pixels = Pixmap::filled(64, 32, Rgba8::WHITE);
        pixels.fill_rect(Rect::new(0, 0, 20, 32), Rgba8::BLACK);
        let opts = MixerOptions { wet: 1.0, mix: 0.5, load: 0.02, ..options() };
        let mut mixer = MixerBrush::new(opts, Rgba8::WHITE);
        for x in 10..40 {
            mixer.apply_dab(&mut pixels, None, &brush(), x as f32, 16.0, 1.0);
        }
        assert!(pixels.get(26, 16).r < 250, "the brush stopped depositing once empty");
    }

    #[test]
    fn an_empty_dry_brush_stops() {
        // Dry, Light Load: one or two dabs and the brush is done, so the far end
        // of a drag is untouched.
        let mut pixels = Pixmap::filled(64, 32, Rgba8::WHITE);
        let opts = MixerOptions { wet: 0.0, load: 0.0, mix: 0.0, ..options() };
        let mut mixer = MixerBrush::new(opts, Rgba8::BLACK);
        assert!(mixer.apply_dab(&mut pixels, None, &brush(), 16.0, 16.0, 1.0).is_empty());
        assert_eq!(pixels.get(16, 16), Rgba8::WHITE);
    }

    #[test]
    fn transparency_under_the_tip_does_not_darken_the_mix() {
        // Cleared pixels are black with zero alpha. Averaging them naively drags
        // the pickup towards black and a smear over an empty region comes out
        // grey; premultiplied averaging keeps the colour that is really there.
        let mut pixels = Pixmap::new(32, 32);
        pixels.fill_rect(Rect::new(0, 0, 32, 16), Rgba8::opaque(240, 200, 40));
        let opts = MixerOptions { wet: 1.0, mix: 1.0, load: 1.0, ..options() };
        let mut mixer = MixerBrush::new(opts, Rgba8::TRANSPARENT);
        mixer.apply_dab(&mut pixels, None, &brush(), 16.0, 15.0, 1.0);
        let px = pixels.get(16, 12);
        assert!(px.r > 150 && px.g > 120, "the empty half greyed the pickup: {px:?}");
    }

    #[test]
    fn sample_all_layers_reads_the_given_pixmap() {
        // The pickup comes from the composite, but the paint still lands on the
        // layer that was passed in to be edited.
        let mut pixels = Pixmap::filled(32, 32, Rgba8::WHITE);
        let composite = Pixmap::filled(32, 32, Rgba8::opaque(0, 200, 0));
        let opts = MixerOptions { wet: 1.0, mix: 1.0, load: 1.0, ..options() };
        let mut mixer = MixerBrush::new(opts, Rgba8::WHITE);
        let sampled = Sampled { pixels: &composite, origin: (0, 0) };
        mixer.apply_dab(&mut pixels, Some(sampled), &brush(), 16.0, 16.0, 1.0);
        let px = pixels.get(16, 16);
        assert!(px.g > px.r, "the green from the composite was not picked up: {px:?}");
    }
}

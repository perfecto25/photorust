//! The toning tools: **Dodge**, **Burn** and **Sponge**.
//!
//! They come from the darkroom, and the names still describe what they do there.
//! Dodging was holding something back from the enlarger's light so that part of
//! the print came out lighter; burning was giving one part extra exposure so it
//! came out darker. The Sponge has no darkroom ancestor — it moves colour toward
//! or away from grey.
//!
//! Like the focus tools they work on what is already there rather than painting
//! over it, dab by dab, so dwelling on one spot goes on lightening (or darkening,
//! or draining) it. Unlike them they read one pixel at a time: nothing here needs
//! a neighbourhood, only the pixel's own tone.
//!
//! Two ideas do most of the work:
//!
//! * **Range** — Photoshop applies dodge and burn strongest inside one band of
//!   the tonal scale. Set to Highlights, a burn bites into a bright sky and
//!   barely touches the shadows under it, which is what makes the tools usable on
//!   a photograph rather than a blunt brightness brush.
//! * **Protect Tones** — change the *luminance* and put the pixel's own colour
//!   back, instead of scaling the channels. Scaling channels drags a colour toward
//!   white or black by whichever channel saturates first, which is how dodging
//!   ends up bleaching skin to pink and burning ends up going muddy.

use crate::brush::Brush;
use crate::buffer::{Pixmap, Rect, Rgba8};

/// Which of the three is stroking.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(i32)]
pub enum ToneTool {
    /// Lighter, as holding light back from the print made it.
    #[default]
    Dodge = 0,
    /// Darker, as extra exposure made it.
    Burn = 1,
    /// More or less colourful, depending on its Mode.
    Sponge = 2,
}

impl ToneTool {
    pub fn from_i32(v: i32) -> ToneTool {
        match v {
            1 => ToneTool::Burn,
            2 => ToneTool::Sponge,
            _ => ToneTool::Dodge,
        }
    }
}

/// The band of the tonal scale Dodge and Burn work hardest in — CS6's **Range**.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(i32)]
pub enum ToneRange {
    Shadows = 0,
    /// CS6's default, and the one that behaves like a general-purpose brush.
    #[default]
    Midtones = 1,
    Highlights = 2,
}

impl ToneRange {
    pub fn from_i32(v: i32) -> ToneRange {
        match v {
            0 => ToneRange::Shadows,
            2 => ToneRange::Highlights,
            _ => ToneRange::Midtones,
        }
    }

    /// Where in the tonal scale this range is centred.
    fn centre(self) -> f32 {
        match self {
            ToneRange::Shadows => 0.0,
            ToneRange::Midtones => 0.5,
            ToneRange::Highlights => 1.0,
        }
    }
}

/// Which way the Sponge moves colour — CS6's **Mode**.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(i32)]
pub enum SpongeMode {
    /// Toward grey. CS6's default.
    #[default]
    Desaturate = 0,
    /// Away from it.
    Saturate = 1,
}

impl SpongeMode {
    pub fn from_i32(v: i32) -> SpongeMode {
        match v {
            1 => SpongeMode::Saturate,
            _ => SpongeMode::Desaturate,
        }
    }
}

/// The toning tools' options bar.
#[derive(Clone, Copy, Debug)]
pub struct ToneOptions {
    pub tool: ToneTool,
    /// Dodge and Burn: which band of the tonal scale to work in.
    pub range: ToneRange,
    /// Sponge: which way to move colour.
    pub sponge: SpongeMode,
    /// How much each dab does, `0.0..=1.0`. CS6 calls this **Exposure** on Dodge
    /// and Burn and **Flow** on the Sponge; it is the same number.
    pub amount: f32,
    /// Dodge and Burn: work on luminance and keep the pixel's own colour, rather
    /// than scaling its channels. CS6 ships with this on.
    pub protect_tones: bool,
    /// Sponge: ease off on colours that are already saturated, so the tool lifts
    /// the flat parts of an image without driving the vivid parts to clipping.
    /// CS6 ships with this on too.
    pub vibrance: bool,
    /// The layer's Lock Transparent Pixels. Set from the layer, not the bar.
    pub preserve_alpha: bool,
}

impl Default for ToneOptions {
    fn default() -> Self {
        // CS6 opens Dodge and Burn on Midtones at Exposure 50% with Protect Tones
        // ticked, and the Sponge on Desaturate at Flow 50% with Vibrance ticked.
        Self {
            tool: ToneTool::Dodge,
            range: ToneRange::Midtones,
            sponge: SpongeMode::Desaturate,
            amount: 0.5,
            protect_tones: true,
            vibrance: true,
            preserve_alpha: false,
        }
    }
}

/// How wide a tonal range reaches. Wide enough that the three overlap — a range
/// that only touched its own third would leave seams where they met.
const RANGE_SIGMA: f32 = 0.38;

/// What Exposure 100% is worth in one pass: the fraction of the distance to the
/// end of the scale a fully covered pixel travels.
///
/// Photoshop's Exposure is not a multiplier on anything obvious, and taking it
/// literally — moving a midtone all the way to white at 100% — makes the tool
/// unusable at any setting. Half the remaining distance at full exposure gives a
/// single pass at CS6's default 50% roughly the lift Photoshop's has.
const EXPOSURE_SCALE: f32 = 0.5;

/// One toning stroke.
///
/// The state it carries is the coverage already applied at each pixel, and that
/// is the whole reason this is a struct rather than a free function: **a pass
/// must apply its effect once, not once per overlapping dab.** Dabs are spaced a
/// quarter of a brush width apart, so every pixel is under four of them; applying
/// per dab compounded the effect four times over and left the scalloped bands
/// where the overlap count changed. Taking the *maximum* coverage a pixel has
/// reached and applying only the increment is what the paint brush already does
/// with its stroke mask, and for the same reason.
///
/// Working the same area again — a second stroke, or moving back over it after
/// letting go — starts a fresh stroke and so does deepen, which is what the tool
/// is for.
pub struct ToneStroke {
    options: ToneOptions,
    /// Coverage already applied at each pixel, `0.0..=1.0`.
    applied: Vec<f32>,
    width: u32,
}

impl ToneStroke {
    pub fn new(options: ToneOptions, width: u32, height: u32) -> Self {
        Self {
            options,
            applied: vec![0.0; (width as usize) * (height as usize)],
            width,
        }
    }

    pub fn options(&self) -> ToneOptions {
        self.options
    }

    /// Apply one dab, editing `pixels` in place. Returns the region changed.
    ///
    /// `(cx, cy)` is in the pixmap's own coordinates.
    pub fn apply_dab(
        &mut self,
        pixels: &mut Pixmap,
        brush: &Brush,
        cx: f32,
        cy: f32,
        pressure: f32,
    ) -> Rect {
        apply(self, pixels, brush, cx, cy, pressure)
    }
}

fn apply(
    stroke: &mut ToneStroke,
    pixels: &mut Pixmap,
    brush: &Brush,
    cx: f32,
    cy: f32,
    pressure: f32,
) -> Rect {
    let options = &stroke.options;
    let radius = brush.radius() * pressure.clamp(0.05, 1.0);
    let amount = options.amount.clamp(0.0, 1.0);
    if radius <= 0.0 || amount <= 0.0 {
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

    let dab = Brush { size: radius * 2.0, ..*brush };
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

            // Only the increment over what this stroke has already applied here.
            let index = (y as usize) * (stroke.width as usize) + x as usize;
            let previous = stroke.applied[index];
            if cover <= previous {
                continue;
            }
            let delta = cover - previous;

            let dst = pixels.get(x, y);
            // Nothing to tone where there is nothing there — and the transparency
            // lock, when set, is a stricter version of the same rule.
            if dst.a == 0 {
                continue;
            }
            let mut weight = (delta * amount).clamp(0.0, 1.0);
            if options.preserve_alpha {
                weight *= dst.a as f32 / 255.0;
            }
            if weight <= 0.0 {
                continue;
            }

            let rgb = [
                dst.r as f32 / 255.0,
                dst.g as f32 / 255.0,
                dst.b as f32 / 255.0,
            ];
            let toned = match options.tool {
                ToneTool::Dodge | ToneTool::Burn => tone(rgb, weight, options),
                ToneTool::Sponge => sponge(rgb, weight, options),
            };

            let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            // Toning changes colour, never coverage.
            let out = Rgba8::new(c(toned[0]), c(toned[1]), c(toned[2]), dst.a);
            if out == dst {
                // Too small a step to move an 8-bit channel. Leave the coverage
                // where it was so the increment is offered again next dab rather
                // than being rounded away: dropping it lost the effect wherever
                // the brush's falloff crept up slowly, which showed as a dotted
                // fringe along the edge of a soft stroke.
                continue;
            }
            stroke.applied[index] = cover;
            pixels.set(x, y, out);
            dirty = dirty.union(&Rect::new(x, y, 1, 1));
        }
    }

    dirty
}

/// Dodge and Burn.
fn tone(rgb: [f32; 3], weight: f32, options: &ToneOptions) -> [f32; 3] {
    let l = luminance(rgb);
    // How much this pixel's own tone belongs to the chosen range. EXPOSURE_SCALE
    // belongs here and not in the shared weight above: it calibrates Exposure's
    // meaning specifically — moving a pixel toward the end of the tonal scale —
    // and has nothing to say about the Sponge's Flow, which was getting the same
    // dampening for no reason and left the tool barely visible.
    let k = weight * EXPOSURE_SCALE * range_weight(options.range, l);
    if k <= 0.0 {
        return rgb;
    }

    let lighten = options.tool == ToneTool::Dodge;
    if options.protect_tones {
        // Move the luminance and leave the colour where it is. `l + (1 - l) * k`
        // can never pass white and `l * (1 - k)` can never pass black, so Protect
        // Tones also means the tool cannot clip — half of why CS6 has it on.
        let target = if lighten { l + (1.0 - l) * k } else { l * (1.0 - k) };
        return shift_luminance(rgb, target);
    }
    // Unprotected: scale the channels, and let whichever saturates first pull the
    // hue with it. That drift is the *reason* Protect Tones exists, so it is not
    // a bug to fix here.
    let mut out = [0.0f32; 3];
    for c in 0..3 {
        out[c] = if lighten {
            rgb[c] + (1.0 - rgb[c]) * k
        } else {
            rgb[c] * (1.0 - k)
        };
    }
    out
}

/// The Sponge.
fn sponge(rgb: [f32; 3], weight: f32, options: &ToneOptions) -> [f32; 3] {
    let l = luminance(rgb);
    let saturation = saturation(rgb);

    let mut k = weight;
    if options.vibrance {
        // Ease off where the colour is already vivid. Saturating an already
        // saturated pixel only drives it to clipping, and draining one that is
        // nearly grey has nothing left to take.
        k *= match options.sponge {
            SpongeMode::Saturate => 1.0 - saturation,
            SpongeMode::Desaturate => saturation,
        };
    }
    if k <= 0.0 {
        return rgb;
    }

    // Distance from grey, scaled. Grey is the luminance, so this is one axis.
    let scale = match options.sponge {
        SpongeMode::Desaturate => 1.0 - k,
        SpongeMode::Saturate => 1.0 + k,
    };
    let mut out = [0.0f32; 3];
    for c in 0..3 {
        out[c] = (l + (rgb[c] - l) * scale).clamp(0.0, 1.0);
    }
    out
}

/// How strongly a pixel of luminance `l` belongs to `range`, `0.0..=1.0`.
///
/// A Gaussian rather than a hard band: the ranges have to overlap, or working
/// across a gradient would leave a seam where one range handed over to the next.
fn range_weight(range: ToneRange, l: f32) -> f32 {
    let d = l - range.centre();
    (-(d * d) / (2.0 * RANGE_SIGMA * RANGE_SIGMA)).exp()
}

/// Rec. 601 luma, the same weighting the rest of the engine uses.
fn luminance(rgb: [f32; 3]) -> f32 {
    0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2]
}

/// How far the colour sits from grey, `0.0..=1.0` — HSL's saturation by another
/// name, and cheap enough to compute per pixel.
fn saturation(rgb: [f32; 3]) -> f32 {
    let max = rgb[0].max(rgb[1]).max(rgb[2]);
    let min = rgb[0].min(rgb[1]).min(rgb[2]);
    if max <= 0.0 {
        return 0.0;
    }
    (max - min) / max
}

/// The colour with its luminance moved to `target`, keeping the *amount* of
/// colour it has.
///
/// The distinction that matters, and the one that made an early version of this
/// turn a dodged horse orange: **shift** the channels, do not **scale** them.
/// Scaling keeps the ratios between them, which keeps hue but multiplies the gap
/// between channels — so a dark brown lightened by scaling arrives as a vivid
/// orange, its colour magnified along with its brightness. Shifting moves all
/// three the same distance, keeping the gaps, and a dark brown lightens into a
/// lighter brown, which is what Photoshop does and what "protect tones" ought to
/// mean.
///
/// Where the shift would push a channel past the end of the scale, the rest of
/// the journey is taken toward the target grey instead — losing exactly as much
/// colour as it must and no more, which is how a hard-dodged pixel eventually
/// reaches white rather than sticking below it.
fn shift_luminance(rgb: [f32; 3], target: f32) -> [f32; 3] {
    let l = luminance(rgb);
    let shift = target - l;
    let shifted = [rgb[0] + shift, rgb[1] + shift, rgb[2] + shift];

    let over = shifted[0].max(shifted[1]).max(shifted[2]) - 1.0;
    let under = -shifted[0].min(shifted[1]).min(shifted[2]);
    let excess = over.max(under);
    if excess <= 0.0 {
        return shifted;
    }
    // How far past the end the worst channel went, as a fraction of its distance
    // from the target grey.
    let spread = shifted
        .iter()
        .map(|v| (v - target).abs())
        .fold(0.0f32, f32::max);
    let fade = (excess / spread.max(1e-6)).clamp(0.0, 1.0);

    let mut out = [0.0f32; 3];
    for c in 0..3 {
        out[c] = (shifted[c] + (target - shifted[c]) * fade).clamp(0.0, 1.0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brush() -> Brush {
        Brush { size: 20.0, hardness: 1.0, ..Brush::default() }
    }

    fn options(tool: ToneTool) -> ToneOptions {
        ToneOptions { tool, amount: 1.0, ..ToneOptions::default() }
    }

    fn flat(colour: Rgba8) -> Pixmap {
        Pixmap::filled(40, 40, colour)
    }

    /// One pass of the tool over the middle of `pm`: a fresh stroke, one dab.
    ///
    /// Passes rather than dabs is the unit that matters now — within a stroke a
    /// pixel is toned once however many dabs cover it.
    fn pass(pm: &mut Pixmap, options: &ToneOptions) {
        let (w, h) = (pm.width(), pm.height());
        ToneStroke::new(*options, w, h).apply_dab(pm, &brush(), 20.0, 20.0, 1.0);
    }

    #[test]
    fn dodge_lightens_and_burn_darkens() {
        let grey = Rgba8::opaque(128, 128, 128);
        let mut lighter = flat(grey);
        let mut darker = flat(grey);
        pass(&mut lighter, &options(ToneTool::Dodge));
        pass(&mut darker, &options(ToneTool::Burn));

        assert!(lighter.get(20, 20).r > 128, "dodge did not lighten");
        assert!(darker.get(20, 20).r < 128, "burn did not darken");
    }

    #[test]
    fn a_pass_tones_once_however_many_dabs_cover_a_pixel() {
        // The bug this guards, and it was a bad one: dabs are spaced a quarter of
        // a brush width apart, so every pixel sits under about four of them. The
        // tool applied its full effect per dab, so one ordinary drag lightened a
        // midtone roughly four times over — a dodge at Exposure 50% took a dark
        // flank to near-white and left scalloped bands where the overlap count
        // changed.
        let grey = Rgba8::opaque(90, 90, 90);
        let opts = ToneOptions { amount: 0.5, ..options(ToneTool::Dodge) };

        let mut single = flat(grey);
        pass(&mut single, &opts);

        // The same spot, reached by a drag whose dabs pile up over it.
        let mut dragged = flat(grey);
        let (w, h) = (dragged.width(), dragged.height());
        let mut stroke = ToneStroke::new(opts, w, h);
        for i in 0..9 {
            stroke.apply_dab(&mut dragged, &brush(), 16.0 + i as f32, 20.0, 1.0);
        }

        let a = single.get(20, 20).r as i32;
        let b = dragged.get(20, 20).r as i32;
        assert!((a - b).abs() <= 2,
                "a drag toned {b} where one dab toned {a}: the dabs compounded");
    }

    #[test]
    fn working_an_area_again_deepens_it() {
        // Each pass still builds on the last — that is the tool's whole method,
        // and it is what the per-stroke coverage must not take away.
        let mut pm = flat(Rgba8::opaque(100, 100, 100));
        let opts = options(ToneTool::Dodge);
        pass(&mut pm, &opts);
        let once = pm.get(20, 20).r;
        for _ in 0..3 {
            pass(&mut pm, &opts);
        }
        assert!(pm.get(20, 20).r > once, "four passes were no lighter than one");
    }

    #[test]
    fn a_dodge_at_the_default_exposure_is_gentle() {
        // Calibration against Photoshop: one pass at CS6's default 50% over a
        // midtone is a visible lift, not a bleach. This is the number the tool
        // was judged wrong on, so it is worth pinning.
        let mut pm = flat(Rgba8::opaque(90, 90, 90));
        pass(&mut pm, &ToneOptions { amount: 0.5, ..options(ToneTool::Dodge) });
        let after = pm.get(20, 20).r as i32;
        assert!(after > 95, "the dodge did nothing: {after}");
        assert!(after < 150, "one pass at 50% is still far too strong: {after}");
    }

    #[test]
    fn dodging_a_dark_colour_does_not_make_it_vivid() {
        // The other half of the same complaint: a dodged brown flank came out
        // orange. Protect Tones shifts the channels rather than scaling them, so
        // the gap between them — the amount of colour — stays where it was.
        let brown = Rgba8::opaque(62, 44, 36);
        let mut pm = flat(brown);
        let opts = ToneOptions { amount: 1.0, ..options(ToneTool::Dodge) };
        for _ in 0..3 {
            pass(&mut pm, &opts);
        }
        let px = pm.get(20, 20);
        assert!(px.r > 90, "the dodge did nothing to measure: {px:?}");

        let spread = |c: Rgba8| c.r as i32 - c.b as i32;
        assert!(spread(px) <= spread(brown) + 6,
                "the colour was magnified along with the brightness: {:?} -> {px:?}", brown);
    }

    #[test]
    fn the_range_decides_which_tones_are_touched() {
        // The point of Range: burning the highlights must bite into a bright
        // patch and leave a dark one nearly alone.
        let mut pm = Pixmap::filled(40, 40, Rgba8::opaque(230, 230, 230));
        pm.fill_rect(Rect::new(0, 0, 40, 20), Rgba8::opaque(30, 30, 30));
        let before = pm.clone();

        let highlights = ToneOptions {
            range: ToneRange::Highlights,
            ..options(ToneTool::Burn)
        };
        let (w, h) = (pm.width(), pm.height());
        let mut stroke = ToneStroke::new(highlights, w, h);
        stroke.apply_dab(&mut pm, &brush(), 20.0, 10.0, 1.0);
        stroke.apply_dab(&mut pm, &brush(), 20.0, 30.0, 1.0);

        let dark_moved = (before.get(20, 10).r as i32 - pm.get(20, 10).r as i32).abs();
        let light_moved = (before.get(20, 30).r as i32 - pm.get(20, 30).r as i32).abs();
        assert!(light_moved > dark_moved * 3,
                "Highlights hit the shadows nearly as hard: {light_moved} vs {dark_moved}");
    }

    #[test]
    fn shadows_and_highlights_are_opposite_ends_of_the_same_scale() {
        let dark = Rgba8::opaque(40, 40, 40);
        let light = Rgba8::opaque(215, 215, 215);
        let moved = |range: ToneRange, colour: Rgba8| {
            let mut pm = flat(colour);
            pass(&mut pm, &ToneOptions { range, ..options(ToneTool::Dodge) });
            (pm.get(20, 20).r as i32 - colour.r as i32).abs()
        };
        assert!(moved(ToneRange::Shadows, dark) > moved(ToneRange::Highlights, dark));
        assert!(moved(ToneRange::Highlights, light) > moved(ToneRange::Shadows, light));
    }

    #[test]
    fn protect_tones_keeps_the_colour_while_dodging() {
        // Scaling channels multiplies the gap between them, so a colour lightened
        // that way arrives more saturated than it started. Shifting keeps the gap.
        let dim = Rgba8::opaque(120, 72, 24);
        let mut protected = flat(dim);
        pass(&mut protected, &ToneOptions { amount: 0.6, ..options(ToneTool::Dodge) });
        let px = protected.get(20, 20);
        assert!(px.r > 120, "the dodge did nothing to measure");
        let spread = |c: Rgba8| c.r as i32 - c.b as i32;
        assert!((spread(px) - spread(dim)).abs() <= 4,
                "the amount of colour changed: {:?} -> {px:?}", dim);

        // Unprotected, the channels are scaled and the colour goes with them.
        let mut loose = flat(dim);
        pass(&mut loose, &ToneOptions {
            amount: 0.6,
            protect_tones: false,
            ..options(ToneTool::Dodge)
        });
        assert!(spread(loose.get(20, 20)) < spread(dim),
                "unprotected dodging should wash the colour out, not hold it");
    }

    #[test]
    fn protect_tones_never_clips_to_white_or_black() {
        let colour = Rgba8::opaque(180, 90, 60);
        for tool in [ToneTool::Dodge, ToneTool::Burn] {
            let mut pm = flat(colour);
            let opts = options(tool);
            for _ in 0..40 {
                pass(&mut pm, &opts);
            }
            let px = pm.get(20, 20);
            // It approaches the end of the scale but is never simply flattened
            // onto it: the step is always a fraction of what is left.
            assert!(px.r != 255 || px.g != 255 || px.b != 255, "{tool:?} clipped to white");
            assert!(px.r != 0 || px.g != 0 || px.b != 0, "{tool:?} clipped to black");
        }
    }

    #[test]
    fn a_single_sponge_stroke_at_default_flow_is_clearly_visible() {
        // The bug this guards: EXPOSURE_SCALE was calibrated for Dodge and Burn's
        // Exposure and then applied as a blanket dampener to the Sponge's Flow
        // too, on top of Vibrance's own easing. The two together made one stroke
        // at the default 50% Flow move an ordinary colour by only a handful of
        // levels — invisible in practice, which is what "the sponge tool isn't
        // doing anything" meant.
        let colour = Rgba8::opaque(180, 90, 70);
        let mut pm = flat(colour);
        // CS6's actual default: Flow 50%, Saturate, Vibrance on.
        pass(&mut pm, &ToneOptions {
            tool: ToneTool::Sponge,
            sponge: SpongeMode::Saturate,
            amount: 0.5,
            ..ToneOptions::default()
        });
        let px = pm.get(20, 20);
        let before_spread = colour.r as i32 - colour.b as i32;
        let after_spread = px.r as i32 - px.b as i32;
        assert!(after_spread - before_spread >= 10,
                "one stroke at default Flow only moved the spread from {before_spread} to \
                 {after_spread} — the sponge is too weak to see");
    }

    #[test]
    fn the_sponge_drains_and_lifts_colour() {
        let colour = Rgba8::opaque(200, 80, 80);
        let saturation_after = |mode: SpongeMode, vibrance: bool| {
            let mut pm = flat(colour);
            let opts = ToneOptions {
                sponge: mode,
                vibrance,
                ..options(ToneTool::Sponge)
            };
            for _ in 0..3 {
                pass(&mut pm, &opts);
            }
            let px = pm.get(20, 20);
            (px.r as i32 - px.b as i32).abs()
        };
        let before = 200 - 80;
        assert!(saturation_after(SpongeMode::Desaturate, false) < before, "no colour drained");
        assert!(saturation_after(SpongeMode::Saturate, false) > before, "no colour lifted");
    }

    #[test]
    fn desaturating_all_the_way_leaves_grey() {
        let mut pm = flat(Rgba8::opaque(200, 80, 80));
        let opts = ToneOptions {
            sponge: SpongeMode::Desaturate,
            vibrance: false,
            ..options(ToneTool::Sponge)
        };
        for _ in 0..24 {
            pass(&mut pm, &opts);
        }
        let px = pm.get(20, 20);
        assert!((px.r as i32 - px.g as i32).abs() <= 2 && (px.g as i32 - px.b as i32).abs() <= 2,
                "the sponge did not reach grey: {px:?}");
    }

    #[test]
    fn vibrance_eases_off_on_colour_that_is_already_vivid() {
        let lift = |colour: Rgba8, vibrance: bool| {
            let mut pm = flat(colour);
            let opts = ToneOptions {
                sponge: SpongeMode::Saturate,
                vibrance,
                ..options(ToneTool::Sponge)
            };
            pass(&mut pm, &opts);
            let px = pm.get(20, 20);
            (px.r as i32 - px.b as i32) - (colour.r as i32 - colour.b as i32)
        };
        let vivid = Rgba8::opaque(240, 30, 30);
        let flatish = Rgba8::opaque(150, 120, 120);
        assert!(lift(flatish, true) > 0, "Vibrance refused to lift a flat colour");
        assert!(lift(vivid, true) < lift(vivid, false),
                "Vibrance did not ease off on an already vivid colour");
    }

    #[test]
    fn grey_has_no_colour_for_the_sponge_to_move() {
        let mut pm = flat(Rgba8::opaque(128, 128, 128));
        let opts = ToneOptions {
            sponge: SpongeMode::Saturate,
            vibrance: false,
            ..options(ToneTool::Sponge)
        };
        for _ in 0..5 {
            pass(&mut pm, &opts);
        }
        assert_eq!(pm.get(20, 20), Rgba8::opaque(128, 128, 128));
    }

    #[test]
    fn toning_never_changes_coverage() {
        let mut pm = Pixmap::new(40, 40);
        pm.fill_rect(Rect::new(0, 0, 20, 40), Rgba8::new(200, 100, 50, 128));
        for tool in [ToneTool::Dodge, ToneTool::Burn, ToneTool::Sponge] {
            let mut copy = pm.clone();
            pass(&mut copy, &options(tool));
            assert_eq!(copy.get(10, 20).a, 128, "{tool:?} changed a pixel's alpha");
            assert_eq!(copy.get(30, 20).a, 0, "{tool:?} gave an empty pixel coverage");
        }
    }

    #[test]
    fn nothing_outside_the_dab_is_touched() {
        let mut pm = flat(Rgba8::opaque(128, 128, 128));
        pass(&mut pm, &options(ToneTool::Dodge));
        assert_eq!(pm.get(2, 2), Rgba8::opaque(128, 128, 128));
    }
}

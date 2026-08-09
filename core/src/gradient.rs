//! Gradients — the engine behind the Gradient tool.
//!
//! A gradient is a **ramp** (a list of colour stops) plus a **shape** that says
//! how a pixel's position turns into a place along that ramp. The two are
//! independent, which is why CS6 lets any preset be drawn as any of its five
//! types: the ramp answers "what colour at 40% along", the type answers "how far
//! along is this pixel".
//!
//! Interpolation is done in **straight alpha**, with colour and opacity
//! interpolated separately. That is deliberate and it is what Photoshop does:
//! "Foreground to Transparent" has to keep the foreground *colour* all the way
//! along while only its opacity falls off. Premultiplied interpolation would
//! drag the colour toward black as it faded.

use crate::blend::BlendMode;
use crate::buffer::{Pixmap, Rect, Rgba8};
use crate::compositor;
use crate::selection::Selection;

/// The five shapes CS6 offers, in options-bar order.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(i32)]
pub enum GradientType {
    /// Straight along the drag. The default.
    #[default]
    Linear = 0,
    /// Circles out from where the drag began.
    Radial = 1,
    /// Sweeps around the start point, the drag setting the zero angle.
    Angle = 2,
    /// Linear, mirrored either side of the start.
    Reflected = 3,
    /// Concentric diamonds out from the start.
    Diamond = 4,
}

impl GradientType {
    pub fn from_i32(v: i32) -> GradientType {
        match v {
            1 => GradientType::Radial,
            2 => GradientType::Angle,
            3 => GradientType::Reflected,
            4 => GradientType::Diamond,
            _ => GradientType::Linear,
        }
    }
}

/// One stop on the ramp.
#[derive(Clone, Copy, Debug)]
pub struct GradientStop {
    /// Where it sits, `0.0..=1.0`.
    pub position: f32,
    pub color: Rgba8,
}

impl GradientStop {
    pub const fn new(position: f32, color: Rgba8) -> Self {
        Self { position, color }
    }
}

/// A colour ramp. Stops are held in ascending position order.
#[derive(Clone, Debug)]
pub struct Gradient {
    pub stops: Vec<GradientStop>,
}

impl Gradient {
    /// A ramp from `stops`, sorted and with the ends guaranteed, so `sample` can
    /// assume both.
    pub fn new(mut stops: Vec<GradientStop>) -> Gradient {
        stops.sort_by(|a, b| a.position.partial_cmp(&b.position).unwrap_or(std::cmp::Ordering::Equal));
        if stops.is_empty() {
            stops.push(GradientStop::new(0.0, Rgba8::BLACK));
        }
        Gradient { stops }
    }

    /// A two-stop ramp.
    pub fn two_stop(from: Rgba8, to: Rgba8) -> Gradient {
        Gradient::new(vec![GradientStop::new(0.0, from), GradientStop::new(1.0, to)])
    }

    /// The colour at `t` along the ramp, clamped outside `0.0..=1.0`.
    pub fn sample(&self, t: f32) -> Rgba8 {
        quantise(self.sample_raw(t))
    }

    /// The same, unquantised: channels as floats in `0.0..=255.0`.
    ///
    /// Kept separate so the renderer can dither the *quantisation* rather than
    /// the ramp position. Nudging `t` instead speckles every hard-edged preset,
    /// because a step in the ramp lands either side of the nudge.
    pub fn sample_raw(&self, t: f32) -> [f32; 4] {
        let t = t.clamp(0.0, 1.0);
        let first = self.stops[0];
        if t <= first.position {
            return channels(first.color);
        }
        let last = self.stops[self.stops.len() - 1];
        if t >= last.position {
            return channels(last.color);
        }

        // Small stop counts — a dozen at most — so a scan beats a binary search
        // and keeps this readable.
        let mut lower = first;
        for stop in &self.stops[1..] {
            if stop.position >= t {
                let span = stop.position - lower.position;
                let f = if span <= 1e-6 { 0.0 } else { (t - lower.position) / span };
                return lerp_raw(channels(lower.color), channels(stop.color), f);
            }
            lower = *stop;
        }
        channels(last.color)
    }

    /// Reverse the ramp, as the options bar's **Reverse** does.
    pub fn reversed(&self) -> Gradient {
        Gradient::new(
            self.stops
                .iter()
                .map(|s| GradientStop::new(1.0 - s.position, s.color))
                .collect(),
        )
    }

    /// Render the ramp as a horizontal strip, for the options bar's swatch and
    /// the preset menu. Drawn by the engine so a preview cannot drift from what
    /// the tool actually paints.
    pub fn preview(&self, width: u32, height: u32) -> Pixmap {
        let mut out = Pixmap::new(width.max(1), height.max(1));
        let w = out.width();
        let h = out.height();
        for x in 0..w {
            // Sample at the pixel's centre, so the first and last pixels are not
            // half a step short of the ends of the ramp.
            let t = if w <= 1 { 0.0 } else { (x as f32 + 0.5) / w as f32 };
            let colour = self.sample(t);
            for y in 0..h {
                out.set(x as i32, y as i32, colour);
            }
        }
        out
    }
}

/// Everything the options bar says about how to draw the ramp.
#[derive(Clone, Copy, Debug)]
pub struct GradientOptions {
    pub kind: GradientType,
    pub mode: BlendMode,
    /// Master opacity, `0.0..=1.0`.
    pub opacity: f32,
    /// Draw the ramp end to start.
    pub reverse: bool,
    /// Break up banding with a little noise. Photoshop has this on by default,
    /// because an 8-bit ramp across a wide canvas bands visibly without it.
    pub dither: bool,
    /// Honour the ramp's own alpha. Off, the gradient is drawn fully opaque —
    /// CS6's **Transparency** checkbox.
    pub transparency: bool,
    /// The layer's Lock Transparent Pixels: the gradient may recolour what is
    /// there but must not give an empty pixel any coverage. Set from the layer,
    /// not from the options bar.
    pub preserve_alpha: bool,
}

impl Default for GradientOptions {
    fn default() -> Self {
        Self {
            kind: GradientType::Linear,
            mode: BlendMode::Normal,
            opacity: 1.0,
            reverse: false,
            dither: true,
            transparency: true,
            preserve_alpha: false,
        }
    }
}

/// How far dither may move a channel, in 8-bit levels.
///
/// Half a level: enough that a value sitting between two levels lands on either
/// of them rather than always the nearer one — which is what breaks a band into
/// noise — and never enough to shift a value that is already exact. That last
/// part is why hard-edged presets like Transparent Stripes stay hard.
const DITHER_LEVELS: f32 = 0.5;

/// Draw `gradient` over `pixels`, from `start` to `end` in the pixmap's own
/// coordinates. Returns the region changed.
///
/// `offset` is where the pixmap sits in document space, needed only to ask the
/// selection about a pixel. Everything outside the selection is left alone.
pub fn draw(
    pixels: &mut Pixmap,
    gradient: &Gradient,
    options: &GradientOptions,
    start: (f32, f32),
    end: (f32, f32),
    offset: (i32, i32),
    selection: Option<&Selection>,
) -> Rect {
    let (dx, dy) = (end.0 - start.0, end.1 - start.1);
    let length = (dx * dx + dy * dy).sqrt();
    if length < 1e-3 {
        // A click without a drag has no axis to run along. Photoshop draws
        // nothing rather than flooding the layer with one end of the ramp.
        return Rect::default();
    }

    let ramp = if options.reverse {
        std::borrow::Cow::Owned(gradient.reversed())
    } else {
        std::borrow::Cow::Borrowed(gradient)
    };
    let opacity = options.opacity.clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return Rect::default();
    }
    let selection = selection.filter(|sel| !sel.is_empty());

    // A gradient covers the whole layer: every pixel gets a place on the ramp,
    // even if that place is one of the ends.
    let region = pixels.rect();
    let mut dirty = Rect::default();

    for y in region.y..region.bottom() {
        for x in region.x..region.right() {
            let mut alpha = opacity;
            if let Some(sel) = selection {
                alpha *= sel.coverage_at(x + offset.0, y + offset.1);
                if alpha <= 0.0 {
                    continue;
                }
            }

            // The pixel's own coordinate, not its centre: the drag arrives in
            // these same coordinates, so a drag from one pixel to another puts
            // the exact ends of the ramp on exactly those pixels.
            let px = x as f32;
            let py = y as f32;
            let t = position_at(options.kind, px, py, start, (dx, dy), length);

            let mut raw = ramp.sample_raw(t);
            if options.dither {
                // One noise value for all four channels: per-channel noise would
                // show as colour speckle rather than as luminance grain.
                let n = dither(x, y) * DITHER_LEVELS;
                for v in &mut raw {
                    *v += n;
                }
            }
            let mut src = quantise(raw);
            if !options.transparency {
                // Transparency off means the ramp's own alpha is ignored, so a
                // "to transparent" preset draws as a solid colour.
                src.a = 255;
            }

            let dst = pixels.get(x, y);
            if options.preserve_alpha {
                if dst.a == 0 {
                    continue;
                }
                alpha *= dst.a as f32 / 255.0;
            }
            let out = compositor::blend_pixel(dst, src, alpha, options.mode);
            if out != dst {
                pixels.set(x, y, out);
                dirty = dirty.union(&Rect::new(x, y, 1, 1));
            }
        }
    }

    dirty
}

/// Where `(px, py)` falls along the ramp, `0.0..=1.0` before clamping.
fn position_at(
    kind: GradientType,
    px: f32,
    py: f32,
    start: (f32, f32),
    axis: (f32, f32),
    length: f32,
) -> f32 {
    let (vx, vy) = (px - start.0, py - start.1);
    let (ax, ay) = (axis.0 / length, axis.1 / length);
    // The pixel in the axis's own frame: `along` runs from start to end, `across`
    // is perpendicular to it.
    let along = vx * ax + vy * ay;
    let across = -vx * ay + vy * ax;

    match kind {
        GradientType::Linear => along / length,
        GradientType::Radial => (vx * vx + vy * vy).sqrt() / length,
        // Photoshop sweeps a full turn anticlockwise from the drag's direction.
        // `across` grows downward, so the sweep is negated to turn the *visible*
        // way round.
        GradientType::Angle => {
            let angle = (-across).atan2(along);
            let turns = angle / (2.0 * std::f32::consts::PI);
            if turns < 0.0 {
                turns + 1.0
            } else {
                turns
            }
        }
        GradientType::Reflected => along.abs() / length,
        GradientType::Diamond => (along.abs() + across.abs()) / length,
    }
}

/// A deterministic value in `-1.0..1.0` for a pixel.
///
/// Deterministic because the preview and the commit must agree, and because undo
/// then redo must reproduce the same fill rather than a differently speckled one.
fn dither(x: i32, y: i32) -> f32 {
    // An integer hash: cheap, and with no visible structure at the scale of one
    // pixel — an ordered matrix would leave its own pattern in the ramp.
    let mut h = (x as u32).wrapping_mul(0x9E37_79B9) ^ (y as u32).wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545_F491);
    h ^= h >> 13;
    ((h & 0xFFFF) as f32 / 65_535.0) * 2.0 - 1.0
}

/// Straight-alpha interpolation, colour and opacity independently.
fn lerp_raw(from: [f32; 4], to: [f32; 4], t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    let mut out = [0.0f32; 4];
    for i in 0..4 {
        out[i] = from[i] + (to[i] - from[i]) * t;
    }
    out
}

fn channels(c: Rgba8) -> [f32; 4] {
    [c.r as f32, c.g as f32, c.b as f32, c.a as f32]
}

fn quantise(c: [f32; 4]) -> Rgba8 {
    let v = |i: usize| c[i].round().clamp(0.0, 255.0) as u8;
    Rgba8::new(v(0), v(1), v(2), v(3))
}

// ---------------------------------------------------------------------------
// Presets

/// CS6's default gradient set, in its own order.
///
/// The names are the contract with the shell, exactly as the adjustment names
/// are: the options bar asks for a preset by name and the engine answers with a
/// ramp. The first three depend on the current foreground and background, which
/// is why building a preset needs them passed in.
pub const PRESET_NAMES: [&str; 15] = [
    "Foreground to Background",
    "Foreground to Transparent",
    "Black, White",
    "Red, Green",
    "Violet, Orange",
    "Blue, Red, Yellow",
    "Blue, Yellow, Blue",
    "Orange, Yellow, Orange",
    "Violet, Green, Orange",
    "Yellow, Violet, Orange, Blue",
    "Copper",
    "Chrome",
    "Spectrum",
    "Transparent Rainbow",
    "Transparent Stripes",
];

/// The ramp behind a preset name, or `None` if the name is not one of ours.
pub fn preset(name: &str, foreground: Rgba8, background: Rgba8) -> Option<Gradient> {
    let rgb = Rgba8::opaque;
    let even = |colours: &[Rgba8]| {
        // Evenly spaced stops, which is how all of CS6's multi-colour presets
        // except Copper and Chrome are laid out.
        let last = (colours.len().max(2) - 1) as f32;
        Gradient::new(
            colours
                .iter()
                .enumerate()
                .map(|(i, c)| GradientStop::new(i as f32 / last, *c))
                .collect(),
        )
    };

    Some(match name {
        "Foreground to Background" => Gradient::two_stop(foreground, background),
        "Foreground to Transparent" => Gradient::two_stop(
            foreground,
            Rgba8::new(foreground.r, foreground.g, foreground.b, 0),
        ),
        "Black, White" => Gradient::two_stop(Rgba8::BLACK, Rgba8::WHITE),
        "Red, Green" => Gradient::two_stop(rgb(255, 0, 0), rgb(0, 255, 0)),
        "Violet, Orange" => Gradient::two_stop(rgb(150, 0, 200), rgb(255, 140, 0)),
        "Blue, Red, Yellow" => even(&[rgb(0, 0, 255), rgb(255, 0, 0), rgb(255, 255, 0)]),
        "Blue, Yellow, Blue" => even(&[rgb(0, 0, 255), rgb(255, 255, 0), rgb(0, 0, 255)]),
        "Orange, Yellow, Orange" => {
            even(&[rgb(255, 130, 0), rgb(255, 255, 0), rgb(255, 130, 0)])
        }
        "Violet, Green, Orange" => {
            even(&[rgb(150, 0, 200), rgb(0, 200, 60), rgb(255, 140, 0)])
        }
        "Yellow, Violet, Orange, Blue" => even(&[
            rgb(255, 240, 0),
            rgb(150, 0, 200),
            rgb(255, 140, 0),
            rgb(0, 40, 220),
        ]),
        // Copper and Chrome are metals: their stops are deliberately uneven,
        // which is what gives the sharp highlight rather than a soft ramp.
        "Copper" => Gradient::new(vec![
            GradientStop::new(0.0, rgb(101, 42, 20)),
            GradientStop::new(0.25, rgb(213, 133, 83)),
            GradientStop::new(0.45, rgb(255, 226, 196)),
            GradientStop::new(0.62, rgb(178, 96, 52)),
            GradientStop::new(1.0, rgb(120, 55, 28)),
        ]),
        "Chrome" => Gradient::new(vec![
            GradientStop::new(0.0, rgb(60, 62, 70)),
            GradientStop::new(0.32, rgb(220, 228, 238)),
            GradientStop::new(0.37, rgb(120, 128, 140)),
            GradientStop::new(0.52, rgb(30, 32, 38)),
            GradientStop::new(0.58, rgb(150, 158, 170)),
            GradientStop::new(1.0, rgb(240, 244, 250)),
        ]),
        // The hue wheel, once round.
        "Spectrum" => even(&[
            rgb(255, 0, 0),
            rgb(255, 255, 0),
            rgb(0, 255, 0),
            rgb(0, 255, 255),
            rgb(0, 0, 255),
            rgb(255, 0, 255),
            rgb(255, 0, 0),
        ]),
        "Transparent Rainbow" => Gradient::new(vec![
            GradientStop::new(0.0, Rgba8::new(255, 0, 0, 0)),
            GradientStop::new(0.1, Rgba8::new(255, 0, 0, 255)),
            GradientStop::new(0.3, Rgba8::new(255, 255, 0, 255)),
            GradientStop::new(0.5, Rgba8::new(0, 255, 0, 255)),
            GradientStop::new(0.7, Rgba8::new(0, 160, 255, 255)),
            GradientStop::new(0.9, Rgba8::new(120, 0, 255, 255)),
            GradientStop::new(1.0, Rgba8::new(120, 0, 255, 0)),
        ]),
        // Hard-edged stripes: pairs of stops at the same position, so the ramp
        // steps instead of blending.
        "Transparent Stripes" => {
            let mut stops = Vec::new();
            let bands = 5;
            for i in 0..bands {
                let a = i as f32 / bands as f32;
                let b = (i as f32 + 0.5) / bands as f32;
                let c = (i as f32 + 1.0) / bands as f32;
                stops.push(GradientStop::new(a, foreground));
                stops.push(GradientStop::new(b, foreground));
                stops.push(GradientStop::new(
                    b,
                    Rgba8::new(foreground.r, foreground.g, foreground.b, 0),
                ));
                stops.push(GradientStop::new(
                    c,
                    Rgba8::new(foreground.r, foreground.g, foreground.b, 0),
                ));
            }
            Gradient::new(stops)
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linear(kind: GradientType) -> GradientOptions {
        // Dither off in tests: it moves a pixel by up to one 8-bit step, which is
        // exactly the margin an equality assertion has no room for.
        GradientOptions { kind, dither: false, ..GradientOptions::default() }
    }

    #[test]
    fn a_linear_ramp_runs_from_start_to_end() {
        let mut pm = Pixmap::filled(64, 8, Rgba8::opaque(0, 128, 0));
        let g = Gradient::two_stop(Rgba8::BLACK, Rgba8::WHITE);
        let dirty = draw(&mut pm, &g, &linear(GradientType::Linear), (0.0, 4.0), (63.0, 4.0),
                         (0, 0), None);
        assert!(!dirty.is_empty());
        assert_eq!(pm.get(0, 4), Rgba8::BLACK);
        assert_eq!(pm.get(63, 4), Rgba8::WHITE);
        // Monotonic in between, which is the whole point of a ramp.
        let mid = pm.get(32, 4).r;
        assert!(mid > 100 && mid < 160, "midpoint was {mid}");
    }

    #[test]
    fn everything_before_the_start_and_after_the_end_takes_the_end_colours() {
        // Photoshop extends the ends of the ramp across the rest of the layer
        // rather than leaving it untouched.
        let mut pm = Pixmap::filled(64, 8, Rgba8::opaque(0, 128, 0));
        let g = Gradient::two_stop(Rgba8::BLACK, Rgba8::WHITE);
        draw(&mut pm, &g, &linear(GradientType::Linear), (20.0, 4.0), (40.0, 4.0), (0, 0), None);
        assert_eq!(pm.get(2, 4), Rgba8::BLACK);
        assert_eq!(pm.get(61, 4), Rgba8::WHITE);
    }

    #[test]
    fn reverse_swaps_the_ends() {
        let mut pm = Pixmap::new(32, 4);
        let g = Gradient::two_stop(Rgba8::BLACK, Rgba8::WHITE);
        let options = GradientOptions { reverse: true, ..linear(GradientType::Linear) };
        draw(&mut pm, &g, &options, (0.0, 2.0), (31.0, 2.0), (0, 0), None);
        assert_eq!(pm.get(0, 2), Rgba8::WHITE);
        assert_eq!(pm.get(31, 2), Rgba8::BLACK);
    }

    #[test]
    fn a_radial_ramp_starts_at_the_centre_of_the_drag() {
        let mut pm = Pixmap::new(64, 64);
        let g = Gradient::two_stop(Rgba8::BLACK, Rgba8::WHITE);
        draw(&mut pm, &g, &linear(GradientType::Radial), (32.0, 32.0), (62.0, 32.0), (0, 0),
             None);
        assert_eq!(pm.get(32, 32), Rgba8::BLACK, "the centre is the start of the ramp");
        // Equidistant points match whatever direction they lie in.
        assert_eq!(pm.get(52, 32), pm.get(32, 52));
    }

    #[test]
    fn a_reflected_ramp_mirrors_about_the_start() {
        let mut pm = Pixmap::new(64, 8);
        let g = Gradient::two_stop(Rgba8::BLACK, Rgba8::WHITE);
        draw(&mut pm, &g, &linear(GradientType::Reflected), (32.0, 4.0), (52.0, 4.0), (0, 0),
             None);
        assert_eq!(pm.get(12, 4), pm.get(52, 4), "the two sides differ");
        assert_eq!(pm.get(32, 4), Rgba8::BLACK);
    }

    #[test]
    fn an_angle_ramp_sweeps_a_full_turn() {
        let mut pm = Pixmap::new(64, 64);
        let g = Gradient::two_stop(Rgba8::BLACK, Rgba8::WHITE);
        draw(&mut pm, &g, &linear(GradientType::Angle), (32.0, 32.0), (62.0, 32.0), (0, 0),
             None);
        // Along the drag is the start of the ramp; the far side of the sweep is
        // the end, and the two meet in a seam there.
        assert!(pm.get(60, 32).r < 20, "the zero angle is not the ramp's start");
        // A quarter turn anticlockwise — up the screen — is a quarter along.
        let quarter = pm.get(32, 4).r as i32;
        assert!((quarter - 64).abs() < 12, "a quarter turn read as {quarter}, not ~64");
        // And three quarters the other way round.
        let three = pm.get(32, 60).r as i32;
        assert!((three - 191).abs() < 12, "three quarters read as {three}, not ~191");
    }

    #[test]
    fn a_diamond_ramp_is_square_on_its_diagonals() {
        let mut pm = Pixmap::new(64, 64);
        let g = Gradient::two_stop(Rgba8::BLACK, Rgba8::WHITE);
        draw(&mut pm, &g, &linear(GradientType::Diamond), (32.0, 32.0), (52.0, 32.0), (0, 0),
             None);
        // Manhattan distance, so a point 20 along one axis matches one 10 along
        // each of two.
        assert_eq!(pm.get(52, 32), pm.get(42, 42));
    }

    #[test]
    fn a_click_without_a_drag_draws_nothing() {
        let mut pm = Pixmap::filled(16, 16, Rgba8::WHITE);
        let g = Gradient::two_stop(Rgba8::BLACK, Rgba8::WHITE);
        let dirty = draw(&mut pm, &g, &linear(GradientType::Linear), (8.0, 8.0), (8.0, 8.0),
                         (0, 0), None);
        assert!(dirty.is_empty());
        assert_eq!(pm.get(8, 8), Rgba8::WHITE);
    }

    #[test]
    fn fading_to_transparent_keeps_the_colour() {
        // The reason interpolation is done in straight alpha: premultiplied would
        // slide the colour toward black as the opacity fell.
        let orange = Rgba8::opaque(255, 140, 0);
        let g = Gradient::two_stop(orange, Rgba8::new(255, 140, 0, 0));
        let mid = g.sample(0.5);
        assert_eq!((mid.r, mid.g, mid.b), (255, 140, 0));
        assert!((mid.a as i32 - 128).abs() <= 2, "opacity did not fade: {}", mid.a);
    }

    #[test]
    fn transparency_off_draws_the_ramp_solid() {
        let mut pm = Pixmap::new(32, 4);
        let g = Gradient::two_stop(Rgba8::opaque(255, 0, 0), Rgba8::new(255, 0, 0, 0));
        let options = GradientOptions { transparency: false, ..linear(GradientType::Linear) };
        draw(&mut pm, &g, &options, (0.0, 2.0), (31.0, 2.0), (0, 0), None);
        assert_eq!(pm.get(31, 2), Rgba8::opaque(255, 0, 0), "the fade was not ignored");
    }

    #[test]
    fn the_transparency_lock_confines_the_gradient_to_existing_pixels() {
        let mut pm = Pixmap::new(32, 32);
        pm.fill_rect(Rect::new(0, 0, 32, 16), Rgba8::opaque(200, 200, 200));
        let g = Gradient::two_stop(Rgba8::BLACK, Rgba8::BLACK);
        let options = GradientOptions { preserve_alpha: true, ..linear(GradientType::Linear) };
        draw(&mut pm, &g, &options, (0.0, 16.0), (31.0, 16.0), (0, 0), None);
        assert_eq!(pm.get(16, 4), Rgba8::BLACK, "the opaque half was not drawn on");
        assert_eq!(pm.get(16, 24).a, 0, "the gradient gave a transparent pixel coverage");
    }

    #[test]
    fn a_selection_confines_the_gradient() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::WHITE);
        let mut sel = Selection::new(32, 32);
        sel.apply_rect(Rect::new(0, 0, 16, 32), crate::selection::SelectionOp::Replace);
        let g = Gradient::two_stop(Rgba8::BLACK, Rgba8::BLACK);
        draw(&mut pm, &g, &linear(GradientType::Linear), (0.0, 16.0), (31.0, 16.0), (0, 0),
             Some(&sel));
        assert_eq!(pm.get(8, 16), Rgba8::BLACK, "nothing was drawn inside the selection");
        assert_eq!(pm.get(24, 16), Rgba8::WHITE, "the gradient escaped the selection");
    }

    #[test]
    fn dither_breaks_a_stretched_ramp_without_being_visible_as_noise() {
        // Banding appears when a ramp is stretched over more pixels than it has
        // levels — 1024 pixels of a 256-level ramp means four pixels a band. A
        // pixel may move by one level, never more.
        const W: i32 = 1024;
        let mut plain = Pixmap::new(W as u32, 2);
        let mut dithered = Pixmap::new(W as u32, 2);
        let g = Gradient::two_stop(Rgba8::BLACK, Rgba8::WHITE);
        let ends = ((0.0, 1.0), ((W - 1) as f32, 1.0));
        draw(&mut plain, &g, &linear(GradientType::Linear), ends.0, ends.1, (0, 0), None);
        draw(&mut dithered, &g, &GradientOptions::default(), ends.0, ends.1, (0, 0), None);

        let mut differed = 0;
        for x in 0..W {
            let d = (plain.get(x, 1).r as i32 - dithered.get(x, 1).r as i32).abs();
            assert!(d <= 1, "dither moved x={x} by {d} levels");
            if d > 0 {
                differed += 1;
            }
        }
        // Roughly the pixels whose exact value sits between two levels, which on
        // a smooth ramp is most of them.
        assert!(differed > W / 8, "dither changed almost nothing: {differed} pixels");
    }

    #[test]
    fn dither_leaves_hard_edged_presets_hard() {
        // The bug this guards: dithering the ramp *position* rather than the
        // quantisation turned every stripe boundary into a band of speckle.
        let g = preset("Transparent Stripes", Rgba8::BLACK, Rgba8::WHITE).unwrap();
        let mut pm = Pixmap::new(200, 8);
        draw(&mut pm, &g, &GradientOptions::default(), (0.0, 4.0), (199.0, 4.0), (0, 0), None);

        // Every pixel is either fully in a stripe or fully out of one; nothing
        // in between, and no pixel differs from the row above it.
        for x in 0..200 {
            let a = pm.get(x, 2).a;
            assert!(a == 0 || a == 255, "x={x} came out half-covered: {a}");
            assert_eq!(a, pm.get(x, 6).a, "x={x} differs between rows: speckle");
        }
    }

    #[test]
    fn every_preset_name_resolves() {
        for name in PRESET_NAMES {
            let g = preset(name, Rgba8::BLACK, Rgba8::WHITE)
                .unwrap_or_else(|| panic!("no ramp for preset {name}"));
            assert!(!g.stops.is_empty());
            // A preview of it must be drawable, since the options bar shows one
            // for every entry.
            assert_eq!(g.preview(24, 6).width(), 24);
        }
        assert!(preset("Not A Preset", Rgba8::BLACK, Rgba8::WHITE).is_none());
    }

    #[test]
    fn the_foreground_presets_follow_the_current_colours() {
        let fg = Rgba8::opaque(10, 20, 30);
        let bg = Rgba8::opaque(200, 210, 220);
        let g = preset("Foreground to Background", fg, bg).unwrap();
        assert_eq!(g.sample(0.0), fg);
        assert_eq!(g.sample(1.0), bg);
    }

    #[test]
    fn transparent_stripes_step_rather_than_blend() {
        let fg = Rgba8::BLACK;
        let g = preset("Transparent Stripes", fg, Rgba8::WHITE).unwrap();
        // Five bands: opaque for the first half of each, clear for the second.
        assert_eq!(g.sample(0.05).a, 255);
        assert_eq!(g.sample(0.15).a, 0);
        assert_eq!(g.sample(0.25).a, 255);
    }
}

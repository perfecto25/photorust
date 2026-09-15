//! Filter ▸ Render.
//!
//! These do not transform what is on the layer — they *make* something and put
//! it there. Flame is the only one so far, and it is unusual in another way
//! too: it needs a path to draw along, which is a part of the document rather
//! than of the layer, so it cannot go through the plain `Filter` route the
//! rest of the menu uses.

use crate::buffer::{Pixmap, Rgba8};
use rayon::prelude::*;

/// CS6's six Flame Types, in the order its list gives them.
///
/// They differ in where each flame is put and which way it points. The path is
/// a wick in all six; what changes is whether one flame runs the length of it
/// or many stand along it, and whether each leans along the path, all one way,
/// or at angles of their own.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FlameType {
    /// A single flame whose spine is the path itself.
    OneAlongPath,
    /// Many flames, each leaning along the path where it stands.
    #[default]
    MultipleAlongPath,
    /// Many flames, all leaning the same way — the Angle.
    MultipleOneDirection,
    /// Many flames, each leaning away from the path rather than along it.
    MultiplePathDirected,
    /// Many flames at angles of their own.
    MultipleVariousAngle,
    /// Upright, tapered, still — a candle.
    CandleLight,
}

impl FlameType {
    pub fn from_i32(value: i32) -> FlameType {
        match value {
            0 => FlameType::OneAlongPath,
            2 => FlameType::MultipleOneDirection,
            3 => FlameType::MultiplePathDirected,
            4 => FlameType::MultipleVariousAngle,
            5 => FlameType::CandleLight,
            _ => FlameType::MultipleAlongPath,
        }
    }

    fn is_single(self) -> bool {
        self == FlameType::OneAlongPath
    }
}

/// How hard the flame burns — CS6's Flame Style.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FlameStyle {
    #[default]
    Normal,
    Violent,
    Flat,
}

impl FlameStyle {
    pub fn from_i32(value: i32) -> FlameStyle {
        match value {
            1 => FlameStyle::Violent,
            2 => FlameStyle::Flat,
            _ => FlameStyle::Normal,
        }
    }

    /// How much the style multiplies the sway, and how sharply the tongue
    /// narrows towards its tip.
    fn sway_and_taper(self) -> (f32, f32) {
        match self {
            FlameStyle::Normal => (1.0, 1.0),
            // Thrown about far more, and torn to a finer point.
            FlameStyle::Violent => (2.2, 1.6),
            // Barely moving, and hardly tapering — a gas burner.
            FlameStyle::Flat => (0.35, 0.5),
        }
    }
}

/// The outline a tongue of flame is cut to — CS6's Flame Shape.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FlameShape {
    #[default]
    Parallel,
    ToTheCentre,
    Spread,
    Oval,
    Pointing,
}

impl FlameShape {
    pub fn from_i32(value: i32) -> FlameShape {
        match value {
            1 => FlameShape::ToTheCentre,
            2 => FlameShape::Spread,
            3 => FlameShape::Oval,
            4 => FlameShape::Pointing,
            _ => FlameShape::Parallel,
        }
    }

    /// Half-width of the tongue at height `v`, as a fraction of the full
    /// width, where `v` runs from 0 at the wick to 1 at the tip.
    fn half_width(self, v: f32) -> f32 {
        match self {
            // Straight sides until it closes over near the top.
            FlameShape::Parallel => (1.0 - v * v * v).max(0.0),
            // Drawn in to a spike.
            FlameShape::ToTheCentre => (1.0 - v).max(0.0),
            // Opening out as it rises, like a torch in wind.
            FlameShape::Spread => (0.45 + v * 0.85) * (1.0 - v * v * v * v).max(0.0),
            // Fattest in the middle.
            FlameShape::Oval => (v * (1.0 - v) * 4.0).sqrt().max(0.0),
            // A long thin tongue.
            FlameShape::Pointing => (1.0 - v).powf(1.8).max(0.0),
        }
    }
}

/// Everything CS6's Flame dialog collects, across both its tabs.
#[derive(Clone, Copy, Debug)]
pub struct FlameOptions {
    pub kind: FlameType,
    /// Height of a tongue, as a percentage. 100 is about a hundred pixels.
    pub length: f32,
    pub randomize_length: bool,
    /// Width of a tongue, as a percentage, on the same scale as the length.
    pub width: f32,
    /// Which way the flames lean, in degrees, for the types that use one.
    pub angle: f32,
    /// Spacing between tongues, as a percentage of their width.
    pub interval: f32,
    /// Space the tongues so that a whole number of them fits a closed path,
    /// leaving no gap or overlap where it joins up.
    pub adjust_interval_for_loops: bool,
    /// Burn in this colour rather than in fire colours.
    pub custom_color: Option<Rgba8>,
    /// 0 draft to 4 very high: how finely each tongue is sampled.
    pub quality: u8,
    /// How much the tongues are thrown about, 0 to 100.
    pub turbulent: f32,
    /// Roughness on top of the sway — torn edges rather than smooth ones.
    pub jag: f32,
    /// How solid a single tongue is, 0 to 100. Low values let tongues build
    /// up where they overlap, which is what makes a bank of fire.
    pub opacity: f32,
    /// How many strands a tongue is made of, 1 to 30.
    pub complexity: f32,
    /// How far up the tongue the fire reaches full strength, 0 to 100.
    pub bottom_alignment: f32,
    pub style: FlameStyle,
    pub shape: FlameShape,
    pub randomize_shapes: bool,
    /// Which shuffle of the randomness to use — CS6's Arrangement. The same
    /// number gives the same fire, which is what lets it be previewed, undone
    /// and redone.
    pub arrangement: u32,
}

impl Default for FlameOptions {
    fn default() -> Self {
        Self {
            kind: FlameType::MultipleOneDirection,
            length: 100.0,
            randomize_length: false,
            width: 100.0,
            angle: 0.0,
            interval: 30.0,
            adjust_interval_for_loops: true,
            custom_color: None,
            quality: 2,
            turbulent: 15.0,
            jag: 0.0,
            opacity: 25.0,
            complexity: 10.0,
            bottom_alignment: 30.0,
            style: FlameStyle::Normal,
            shape: FlameShape::Parallel,
            randomize_shapes: false,
            arrangement: 1,
        }
    }
}

impl FlameOptions {
    /// Read the options out of the flat list of numbers the dialog collects,
    /// in the order it lists them. Anything the caller left off takes the
    /// default, so a shorter list is a valid request rather than a corrupt
    /// one.
    pub fn from_params(p: &[f32]) -> FlameOptions {
        let mut out = FlameOptions::default();
        let at = |i: usize, fallback: f32| p.get(i).copied().unwrap_or(fallback);

        out.kind = FlameType::from_i32(at(0, 1.0) as i32);
        out.length = at(1, out.length).max(1.0);
        out.randomize_length = at(2, 0.0) != 0.0;
        out.width = at(3, out.width).max(1.0);
        out.angle = at(4, out.angle);
        out.interval = at(5, out.interval).max(1.0);
        out.adjust_interval_for_loops = at(6, 1.0) != 0.0;
        out.custom_color = if at(7, 0.0) != 0.0 {
            Some(Rgba8::new(
                at(8, 255.0).clamp(0.0, 255.0) as u8,
                at(9, 140.0).clamp(0.0, 255.0) as u8,
                at(10, 0.0).clamp(0.0, 255.0) as u8,
                255,
            ))
        } else {
            None
        };
        out.quality = at(11, 2.0).clamp(0.0, 4.0) as u8;
        out.turbulent = at(12, out.turbulent).clamp(0.0, 100.0);
        out.jag = at(13, out.jag).clamp(0.0, 100.0);
        out.opacity = at(14, out.opacity).clamp(1.0, 100.0);
        out.complexity = at(15, out.complexity).clamp(1.0, 30.0);
        out.bottom_alignment = at(16, out.bottom_alignment).clamp(0.0, 100.0);
        out.style = FlameStyle::from_i32(at(17, 0.0) as i32);
        out.shape = FlameShape::from_i32(at(18, 0.0) as i32);
        out.randomize_shapes = at(19, 0.0) != 0.0;
        out.arrangement = at(20, 1.0).max(1.0) as u32;
        out
    }
}

/// Render flames along `polylines` onto `pixmap`.
///
/// Each polyline is a flattened subpath in the pixmap's own coordinates, with
/// a flag saying whether it closes back on itself. Nothing is drawn if there
/// is no path — Photoshop refuses the filter outright in that case, and so
/// does the menu entry that calls this.
pub fn flame(pixmap: &mut Pixmap, polylines: &[(Vec<(f32, f32)>, bool)], options: &FlameOptions) {
    if pixmap.is_empty() || polylines.is_empty() {
        return;
    }

    let mut seed = options.arrangement.wrapping_mul(0x9E3779B1);
    let mut stalks = Vec::new();
    for (points, closed) in polylines {
        if points.len() < 2 {
            continue;
        }
        stalks.extend(stalks_along(points, *closed, options, &mut seed));
    }
    draw(pixmap, &stalks, options);
}

/// One tongue of flame: where it stands, which way it leans, and how big it
/// is.
struct Stalk {
    base: (f32, f32),
    /// Unit vector from the wick towards the tip.
    direction: (f32, f32),
    length: f32,
    width: f32,
    shape: FlameShape,
    seed: u32,
}

/// Where the tongues stand along one subpath, and which way each leans.
fn stalks_along(
    points: &[(f32, f32)],
    closed: bool,
    options: &FlameOptions,
    seed: &mut u32,
) -> Vec<Stalk> {
    // Cumulative distance along the polyline, so a tongue can be placed at a
    // given number of pixels from the start without walking from there.
    let mut lengths = Vec::with_capacity(points.len());
    let mut total = 0.0f32;
    lengths.push(0.0f32);
    for pair in points.windows(2) {
        total += distance(pair[0], pair[1]);
        lengths.push(total);
    }
    if total <= 0.0 {
        return Vec::new();
    }

    let width = options.width;
    let mut spacing = (width * options.interval / 100.0).max(1.0);

    // A closed path that does not take a whole number of tongues has a gap or
    // a double thickness where it joins up. Nudging the spacing hides the
    // seam, which is exactly what CS6's tick box is for.
    if closed && options.adjust_interval_for_loops {
        let count = (total / spacing).round().max(1.0);
        spacing = total / count;
    }

    let mut out = Vec::new();
    let mut walked = 0.0f32;
    // One flame along the path is a different thing: a single tongue standing
    // at the middle of it, as long as the path and leaning along it.
    if options.kind.is_single() {
        let (point, tangent) = sample(points, &lengths, total * 0.5);
        out.push(Stalk {
            base: point,
            direction: lean(tangent, options, next_seed(seed)),
            length: total.max(options.length),
            width,
            shape: options.shape,
            seed: next_seed(seed),
        });
        return out;
    }

    while walked <= total {
        let (point, tangent) = sample(points, &lengths, walked);
        let stalk_seed = next_seed(seed);
        // A tongue that is the same height as its neighbours reads as a fence
        // rather than as fire, so CS6 offers to vary them.
        let length = if options.randomize_length {
            options.length * (0.45 + noise01(stalk_seed, 11) * 1.1)
        } else {
            options.length
        };
        let shape = if options.randomize_shapes {
            FlameShape::from_i32((noise01(stalk_seed, 13) * 5.0) as i32)
        } else {
            options.shape
        };
        out.push(Stalk {
            base: point,
            direction: lean(tangent, options, stalk_seed),
            length,
            width,
            shape,
            seed: stalk_seed,
        });
        walked += spacing;
    }
    out
}

/// Which way one tongue leans, given the path's direction where it stands.
fn lean(tangent: (f32, f32), options: &FlameOptions, seed: u32) -> (f32, f32) {
    let from_angle = |degrees: f32| {
        // Screen y counts downwards, so an angle of zero has to point *up*
        // for a flame to behave like one.
        let radians = degrees.to_radians();
        (radians.sin(), -radians.cos())
    };
    match options.kind {
        FlameType::OneAlongPath | FlameType::MultipleAlongPath => tangent,
        FlameType::MultipleOneDirection => from_angle(options.angle),
        // Away from the path rather than along it: the normal, turned by the
        // angle so the dialog's setting still does something.
        FlameType::MultiplePathDirected => {
            let normal = (tangent.1, -tangent.0);
            rotate(normal, options.angle.to_radians())
        }
        FlameType::MultipleVariousAngle => {
            rotate(from_angle(options.angle), (noise01(seed, 3) - 0.5) * 2.4)
        }
        FlameType::CandleLight => (0.0, -1.0),
    }
}

fn rotate(v: (f32, f32), radians: f32) -> (f32, f32) {
    let (sin, cos) = radians.sin_cos();
    (v.0 * cos - v.1 * sin, v.0 * sin + v.1 * cos)
}

fn distance(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// The point `along` pixels down the polyline, and the unit direction there.
fn sample(
    points: &[(f32, f32)],
    lengths: &[f32],
    along: f32,
) -> ((f32, f32), (f32, f32)) {
    let mut i = 0;
    while i + 2 < points.len() && lengths[i + 1] < along {
        i += 1;
    }
    let (a, b) = (points[i], points[i + 1]);
    let span = (lengths[i + 1] - lengths[i]).max(1e-6);
    let t = ((along - lengths[i]) / span).clamp(0.0, 1.0);
    let point = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
    let len = distance(a, b).max(1e-6);
    (point, ((b.0 - a.0) / len, (b.1 - a.1) / len))
}

/// Render a cloud-like fractal noise pattern onto `pixmap`.
///
/// When `difference` is false, replaces all pixel colours with the grayscale
/// pattern (CS6's Clouds). When true, blends the pattern as a difference into
/// existing content so repeating the filter darkens the result (Difference
/// Clouds).
pub fn clouds(pixmap: &mut Pixmap, difference: bool) {
    if pixmap.is_empty() {
        return;
    }

    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;

    // Scale factors so the noise reads as clouds rather than fine grain.
    // Multiple octaves at different frequencies produce the characteristic
    // cloud look: large soft masses with smaller detail layered on top.
    let octaves: [(f32, f32); 5] = [
        (1.0, 0.50),
        (2.0, 0.25),
        (4.0, 0.125),
        (8.0, 0.0625),
        (16.0, 0.03125),
    ];

    let stride = pixmap.stride();
    let data = pixmap.as_bytes_mut();

    for y in 0..height {
        for x in 0..width {
            let mut value = 0.0f32;
            for &(freq, amp) in &octaves {
                value += smooth_noise(x as f32 * freq / 128.0, y as f32 * freq / 128.0) * amp;
            }

            // Normalize to 0..255 and convert to grayscale.
            let gray = (value * 255.0).clamp(0.0, 255.0) as u8;

            let offset = (y as usize * stride) + (x as usize * 4);
            if difference {
                // Difference Clouds: blend the noise as a difference into
                // existing pixels. Each pass darkens the image, which is the
                // CS6 behavior.
                let base_r = data[offset];
                let base_g = data[offset + 1];
                let base_b = data[offset + 2];
                let base_a = data[offset + 3];

                let diff_r = (base_r as i32 - gray as i32).abs() as u8;
                let diff_g = (base_g as i32 - gray as i32).abs() as u8;
                let diff_b = (base_b as i32 - gray as i32).abs() as u8;

                data[offset] = ((diff_r as u16 + base_r as u16) >> 1) as u8;
                data[offset + 1] = ((diff_g as u16 + base_g as u16) >> 1) as u8;
                data[offset + 2] = ((diff_b as u16 + base_b as u16) >> 1) as u8;
                data[offset + 3] = base_a;
            } else {
                data[offset] = gray;
                data[offset + 1] = gray;
                data[offset + 2] = gray;
                // Preserve existing alpha.
            }
        }
    }
}

/// Smooth noise in 0..1 over a plane, using hash values at each lattice
/// point and smooth interpolation between them.
fn smooth_noise(x: f32, y: f32) -> f32 {
    let (xi, yi) = (x.floor(), y.floor());
    let (fx, fy) = (x - xi, y - yi);
    // Smoothstep for interpolation (same as flame's noise2).
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));

    let hash = |i: i32, j: i32| -> f32 {
        let mut h = (i as u32).wrapping_mul(0x9E3779B1) ^ (j as u32).wrapping_mul(0x85EBCA77);
        h ^= h >> 15;
        h = h.wrapping_mul(0x2545F491);
        h ^= h >> 13;
        (h % 65521) as f32 / 65521.0
    };

    let (i, j) = (xi as i32, yi as i32);
    let top = hash(i, j) + (hash(i + 1, j) - hash(i, j)) * sx;
    let bottom = hash(i, j + 1) + (hash(i + 1, j + 1) - hash(i, j + 1)) * sx;
    top + (bottom - top) * sy
}

fn next_seed(seed: &mut u32) -> u32 {
    *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
    *seed
}

/// A hash in 0..1. Everything random about a flame comes through here, so that
/// the same Arrangement burns the same way every time — which is what lets it
/// be previewed, applied, undone and redone without changing.
fn noise01(seed: u32, salt: u32) -> f32 {
    let mut h = seed.wrapping_mul(0x9E3779B1) ^ salt.wrapping_mul(0x85EBCA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545F491);
    h ^= h >> 13;
    (h % 65521) as f32 / 65521.0
}

/// Smooth noise in 0..1 over a plane.
///
/// A flame needs its randomness to vary *across* a tongue as well as along it.
/// Noise that varies only with height slides the whole tongue sideways as one
/// piece, which reads as a wobbling blob; it is the variation across the width
/// that breaks a tongue into separate licks with gaps between them, and that
/// is most of the difference between something that looks like fire and
/// something that looks like an airbrushed smear.
fn noise2(seed: u32, x: f32, y: f32) -> f32 {
    let (xi, yi) = (x.floor(), y.floor());
    let (fx, fy) = (x - xi, y - yi);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let at = |i: i32, j: i32| {
        noise01(
            seed,
            (i as u32).wrapping_mul(73856093) ^ (j as u32).wrapping_mul(19349663),
        )
    };
    let (i, j) = (xi as i32, yi as i32);
    let top = at(i, j) + (at(i + 1, j) - at(i, j)) * sx;
    let bottom = at(i, j + 1) + (at(i + 1, j + 1) - at(i, j + 1)) * sx;
    top + (bottom - top) * sy
}

/// Several octaves of [`noise2`] laid over one another, which is what gives
/// fire detail at every size at once — broad licks with smaller ones on their
/// edges.
fn fbm(seed: u32, x: f32, y: f32, octaves: u32) -> f32 {
    let mut sum = 0.0;
    let mut weight = 0.5;
    let mut scale = 1.0;
    let mut total = 0.0;
    for octave in 0..octaves.max(1) {
        sum += noise2(seed.wrapping_add(octave * 7919), x * scale, y * scale) * weight;
        total += weight;
        weight *= 0.5;
        scale *= 2.0;
    }
    sum / total
}

/// Smooth noise in 0..1 at a point on a line, for the sway of a tongue.
fn wave(seed: u32, salt: u32, t: f32) -> f32 {
    let i = t.floor();
    let f = t - i;
    // Smoothstep between lattice values, so the sway has no corners in it.
    let f = f * f * (3.0 - 2.0 * f);
    let a = noise01(seed, salt.wrapping_add(i as u32 & 0xffff));
    let b = noise01(seed, salt.wrapping_add((i as u32).wrapping_add(1) & 0xffff));
    a + (b - a) * f
}

/// The colour of fire at a given strength, from the dull red at its edge to
/// the near-white at its heart.
///
/// A ramp rather than a blend between two colours: fire is not one hue made
/// brighter, it is red through orange through yellow to white, and skipping
/// that makes the result look like a coloured cloud rather than a flame.
fn fire(strength: f32, custom: Option<Rgba8>) -> (f32, f32, f32) {
    let s = strength.clamp(0.0, 1.0);
    if let Some(colour) = custom {
        // The same shape of ramp, but towards whatever colour was asked for:
        // dark at the edge, the colour itself through the body, white at the
        // heart, so it still reads as something burning.
        let (r, g, b) = (colour.r as f32, colour.g as f32, colour.b as f32);
        return if s < 0.65 {
            let t = s / 0.65;
            (r * t, g * t, b * t)
        } else {
            let t = (s - 0.65) / 0.35;
            (r + (255.0 - r) * t, g + (255.0 - g) * t, b + (255.0 - b) * t)
        };
    }

    // Stops along the fire ramp, in strength order.
    const STOPS: [(f32, f32, f32, f32); 6] = [
        (0.00, 40.0, 0.0, 0.0),
        (0.20, 150.0, 25.0, 0.0),
        (0.45, 226.0, 88.0, 8.0),
        (0.70, 250.0, 160.0, 30.0),
        (0.88, 255.0, 222.0, 96.0),
        (1.00, 255.0, 252.0, 214.0),
    ];
    for pair in STOPS.windows(2) {
        let (lo, lr, lg, lb) = pair[0];
        let (hi, hr, hg, hb) = pair[1];
        if s <= hi {
            let t = ((s - lo) / (hi - lo)).clamp(0.0, 1.0);
            return (lr + (hr - lr) * t, lg + (hg - lg) * t, lb + (hb - lb) * t);
        }
    }
    (255.0, 252.0, 214.0)
}

/// Render fibres between the foreground and background colours onto `pixmap`.
///
/// CS6's fibres are noise **stretched down the picture**, and they are two
/// separate things layered: broad soft *clumps*, light and dark wands tens of
/// pixels across running a long way down, with crisp *hairs* a pixel or two
/// across and much shorter standing on top of them. Judge it at 500%, where
/// the hairs are still single crisp pixels over smooth clump gradients — a
/// whole-picture view hides everything that matters here.
///
/// Four mistakes, all of them made on the way to this:
///
/// - Isotropic noise smeared downwards is soft grey blobs. The smear kills
///   the contrast, and the grain is as coarse across as it is long.
/// - A strand tone that is a property of its *column*, unable to vary with
///   `y`, is right at low variance and wrong above it. By 32 the picture is
///   short dashes in clumps, not full-height wands.
/// - One run of octaves from wide to narrow cannot give clumps and hairs at
///   once. Weighted wide it loses the hairs, weighted narrow it is even fur
///   with no structure in it.
/// - Hard edges at *every* octave is a mosaic of blocks. Only the octaves
///   whose lattice lands on each pixel should come out crisp.
///
/// `variance` (0–64) is how far the fibres break up: near zero a few broad
/// soft bands running the whole height, by the middle a thicket of hairs over
/// clumps, at 64 short near-black-and-white spatter. It shortens both clumps
/// and hairs and shifts the balance towards the hairs at the same time, which
/// is the pair of things CS6's slider is seen doing. `strength` (1–64)
/// stretches them lengthwise again. `seed` reproduces one exact arrangement
/// so the Randomize button's result can be previewed, undone and redone.
pub fn fibers(
    pixmap: &mut Pixmap,
    variance: f32,
    strength: f32,
    seed: u32,
    foreground: Rgba8,
    background: Rgba8,
) {
    if pixmap.is_empty() {
        return;
    }
    let strength = strength.clamp(1.0, 64.0);
    let variance = variance.clamp(0.0, 64.0);
    if variance <= 0.0 {
        // Variance zero is an even blend of the two colours: nothing to
        // randomise.
        let mid = crate::buffer::Rgba8::new(
            ((foreground.r as u32 + background.r as u32) / 2) as u8,
            ((foreground.g as u32 + background.g as u32) / 2) as u8,
            ((foreground.b as u32 + background.b as u32) / 2) as u8,
            ((foreground.a as u32 + background.a as u32) / 2) as u8,
        );
        for px in pixmap.as_bytes_mut().chunks_exact_mut(4) {
            px[0] = mid.r;
            px[1] = mid.g;
            px[2] = mid.b;
            // The layer's own alpha stands, as with any nonzero variance.
        }
        return;
    }

    let width = pixmap.width() as usize;
    let height = pixmap.height() as usize;

    // Two things are going on in a CS6 fibre picture at once, and they want
    // separate handles. Broad soft *clumps* — light and dark wands tens of
    // pixels across, running a long way down — and crisp *hairs* a pixel or
    // two across and much shorter. One run of octaves from wide to narrow
    // cannot give both: weight it towards the wide end and the hairs vanish,
    // towards the narrow end and the picture is even fur with no structure in
    // it.
    //
    // The hair octaves' lattices land on every pixel or every other one, so
    // they come out hard-edged, while the clump octaves stay smooth
    // gradients. That is exactly what CS6 looks like at 500% — crisp hairs
    // over soft clumps — and making every octave hard-edged instead turns the
    // picture into a mosaic of blocks.
    const CLUMP_OCTAVES: usize = 3;
    const OCTAVES: usize = 5;
    /// How wide each octave is, in pixels: three clumps, then two hairs. The
    /// gap between eight and two is deliberate — CS6's clumps and hairs are
    /// separate things, not one continuous spread of sizes.
    const BANDS: [f32; OCTAVES] = [48.0, 20.0, 8.0, 2.0, 1.0];

    // How far each runs before its tone shifts, before the per-hair jitter
    // below. Variance shortens both, strength stretches both.
    let clump_run = (60.0 / (1.0 + variance * 0.02)) * (strength / 4.0).sqrt();
    let hair_run = (28.0 / (1.0 + variance * 0.04)) * (strength / 4.0).sqrt();

    // How much of the picture is hairs rather than clumps — the other half of
    // what variance does. Low is a few broad soft wands, high is a thicket.
    let hair_share = 0.20 + (variance / 64.0).powf(0.6) * 0.45;

    // Where each hair starts and how long its own tone holds. Without this
    // every hair in the picture changes tone at the same rows, on the same
    // rhythm, and the result is corduroy — regular, and nothing like wool.
    // One pass per column rather than per pixel: it only depends on `x`.
    let mut shift = vec![(0.0f32, 0.0f32); width * OCTAVES];
    shift
        .par_chunks_exact_mut(OCTAVES)
        .enumerate()
        .for_each(|(x, octaves)| {
            for (octave, cell) in octaves.iter_mut().enumerate() {
                // Drawn from the same lattice as the octave it belongs to, so
                // that it varies per pixel where that octave does and smoothly
                // where it doesn't. A phase that jumped at every band edge
                // would put back the hard step this octave is meant not to
                // have.
                let along = x as f32 / BANDS[octave];
                let phase = noise2(seed ^ 0x27d4_eb2d, along, 0.0) * 512.0;
                // Three fifths to eight fifths of the nominal length, so some
                // hairs are stubble and others run on.
                let length = 0.6 + noise2(seed ^ 0x1656_67b1, along, 7.0);
                *cell = (phase, length);
            }
        });

    let mut field = vec![0.0f32; width * height];
    field
        .par_chunks_exact_mut(width)
        .enumerate()
        .for_each(|(y, line)| {
            for (x, cell) in line.iter_mut().enumerate() {
                let mut part = |from: usize, to: usize, run: f32, falloff: f32| {
                    let (mut sum, mut weight, mut total) = (0.0, 1.0, 0.0);
                    for octave in from..to {
                        let band = BANDS[octave];
                        let (phase, length) = shift[x * OCTAVES + octave];
                        sum += noise2(
                            seed.wrapping_add(octave as u32 * 7919),
                            x as f32 / band,
                            (y as f32 + phase) / (run * band.sqrt() * length),
                        ) * weight;
                        total += weight;
                        weight *= falloff;
                    }
                    sum / total
                };
                // Within each, the wider octave leads; between them it is
                // `hair_share` that decides.
                let clumps = part(0, CLUMP_OCTAVES, clump_run, 0.6);
                let hairs = part(CLUMP_OCTAVES, OCTAVES, hair_run, 1.7);
                *cell = clumps * (1.0 - hair_share) + hairs * hair_share;
            }
        });

    // A sum of noises sits in a narrow band around the middle of the range;
    // left as is that is the grey mush, not fibres. Stretch what is actually
    // there out to the full range, letting the extremes clip the way CS6's
    // do, and let variance decide how hard.
    let count = field.len() as f32;
    let mean = field.iter().sum::<f32>() / count;
    let spread = (field.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / count)
        .sqrt()
        .max(f32::EPSILON);
    let contrast = 0.55 + (variance / 64.0).sqrt() * 0.95;
    // Two and a bit deviations either side of the mean fills the range.
    let stretch = contrast / (4.4 * spread);

    let (fr, fg, fbc) = (
        foreground.r as f32,
        foreground.g as f32,
        foreground.b as f32,
    );
    let (br, bg, bb) = (
        background.r as f32,
        background.g as f32,
        background.b as f32,
    );

    let stride = pixmap.stride();
    let bytes = pixmap.as_bytes_mut();
    bytes
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            for x in 0..width {
                let value = field[row * width + x];
                let value = (0.5 + (value - mean) * stretch).clamp(0.0, 1.0);
                let i = x * 4;
                out[i] = (br + (fr - br) * value) as u8;
                out[i + 1] = (bg + (fg - bg) * value) as u8;
                out[i + 2] = (bb + (fbc - bb) * value) as u8;
                // The layer's alpha is left standing, as CS6 Fibers does.
            }
        });
}

/// CS6's four lenses, in the order its Lens Flare dialog lists them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LensType {
    /// The busiest of the four, and CS6's default: a modest core, a long
    /// string of coloured ghosts down the axis, and the hexagonal aperture
    /// showing in them.
    #[default]
    Zoom50To300,
    /// A wide lens: a big soft core with long rays and few ghosts.
    Prime35,
    /// A long lens: a tight hot core, short rays, barely any ghosts.
    Prime105,
    /// Anamorphic — the horizontal blue streak a cinema lens throws.
    MoviePrime,
}

impl LensType {
    pub fn from_i32(value: i32) -> LensType {
        match value {
            1 => LensType::Prime35,
            2 => LensType::Prime105,
            3 => LensType::MoviePrime,
            _ => LensType::Zoom50To300,
        }
    }
}

/// One of the discs of light strung along the axis of a flare.
///
/// They are the reflections between the elements of the lens, which is why
/// they line up: each is an image of the aperture, thrown back through the
/// middle of the frame to the far side.
struct Ghost {
    /// Where it sits on the line from the flare through the middle of the
    /// frame — 0 is the flare itself, 1 the centre of the picture, 2 the
    /// point opposite.
    at: f32,
    /// Its size, as a fraction of the half-diagonal.
    radius: f32,
    tint: (f32, f32, f32),
    strength: f32,
    /// A ring of light rather than a filled disc, which is what the larger
    /// ones in a real flare are.
    ring: bool,
}

/// Everything that distinguishes one of CS6's four lenses from the others.
struct Lens {
    /// The hot core and the soft glow around it, as fractions of the
    /// half-diagonal.
    core: f32,
    glow: f32,
    glow_strength: f32,
    /// How many rays come off the core, how far they reach and how much
    /// light they carry.
    rays: u32,
    ray_length: f32,
    ray_strength: f32,
    /// The anamorphic streak: how far it runs sideways, how thick it is, and
    /// how bright. Zero for the three stills lenses.
    streak: f32,
    streak_strength: f32,
    /// The ring thrown around the core.
    halo: f32,
    halo_strength: f32,
    ghosts: &'static [Ghost],
}

const ZOOM_GHOSTS: &[Ghost] = &[
    Ghost { at: -0.22, radius: 0.030, tint: (1.00, 0.86, 0.55), strength: 0.30, ring: false },
    Ghost { at: 0.34, radius: 0.020, tint: (0.55, 0.85, 1.00), strength: 0.24, ring: false },
    Ghost { at: 0.56, radius: 0.046, tint: (0.60, 1.00, 0.75), strength: 0.18, ring: false },
    Ghost { at: 0.78, radius: 0.028, tint: (1.00, 0.70, 0.45), strength: 0.26, ring: false },
    Ghost { at: 1.00, radius: 0.062, tint: (0.70, 0.75, 1.00), strength: 0.14, ring: true },
    Ghost { at: 1.22, radius: 0.034, tint: (1.00, 0.55, 0.55), strength: 0.22, ring: false },
    Ghost { at: 1.46, radius: 0.092, tint: (0.50, 0.80, 1.00), strength: 0.12, ring: true },
    Ghost { at: 1.72, radius: 0.050, tint: (1.00, 0.85, 0.40), strength: 0.16, ring: false },
];

const PRIME35_GHOSTS: &[Ghost] = &[
    Ghost { at: 0.45, radius: 0.036, tint: (0.65, 0.90, 1.00), strength: 0.18, ring: false },
    Ghost { at: 0.95, radius: 0.078, tint: (0.85, 0.70, 1.00), strength: 0.12, ring: true },
    Ghost { at: 1.38, radius: 0.056, tint: (1.00, 0.75, 0.50), strength: 0.16, ring: false },
];

const PRIME105_GHOSTS: &[Ghost] = &[
    Ghost { at: 0.62, radius: 0.022, tint: (0.70, 0.95, 1.00), strength: 0.14, ring: false },
    Ghost { at: 1.12, radius: 0.040, tint: (1.00, 0.80, 0.60), strength: 0.10, ring: true },
];

const MOVIE_GHOSTS: &[Ghost] = &[
    Ghost { at: 0.70, radius: 0.030, tint: (0.60, 0.80, 1.00), strength: 0.16, ring: false },
    Ghost { at: 1.26, radius: 0.062, tint: (0.55, 0.75, 1.00), strength: 0.10, ring: true },
];

impl LensType {
    fn lens(self) -> Lens {
        match self {
            LensType::Zoom50To300 => Lens {
                core: 0.013,
                glow: 0.17,
                glow_strength: 0.55,
                rays: 6,
                ray_length: 0.55,
                ray_strength: 0.22,
                streak: 0.0,
                streak_strength: 0.0,
                halo: 0.30,
                halo_strength: 0.12,
                ghosts: ZOOM_GHOSTS,
            },
            LensType::Prime35 => Lens {
                core: 0.019,
                glow: 0.27,
                glow_strength: 0.78,
                rays: 8,
                ray_length: 0.85,
                ray_strength: 0.32,
                streak: 0.0,
                streak_strength: 0.0,
                halo: 0.22,
                halo_strength: 0.10,
                ghosts: PRIME35_GHOSTS,
            },
            LensType::Prime105 => Lens {
                core: 0.010,
                glow: 0.12,
                glow_strength: 0.62,
                rays: 4,
                ray_length: 0.34,
                ray_strength: 0.12,
                streak: 0.0,
                streak_strength: 0.0,
                halo: 0.18,
                halo_strength: 0.08,
                ghosts: PRIME105_GHOSTS,
            },
            LensType::MoviePrime => Lens {
                core: 0.012,
                glow: 0.14,
                glow_strength: 0.50,
                rays: 0,
                ray_length: 0.0,
                ray_strength: 0.0,
                // The signature of the four: light smeared right across the
                // frame in a thin blue bar.
                streak: 1.30,
                streak_strength: 0.55,
                halo: 0.16,
                halo_strength: 0.06,
                ghosts: MOVIE_GHOSTS,
            },
        }
    }
}

/// Throw a lens flare onto `pixmap` — Filter ▸ Render ▸ Lens Flare.
///
/// `center` is where the sun is, in fractions of the width and height, which
/// is how the dialog's draggable crosshair hands it over and what keeps it
/// meaning the same thing on a proxy as on the full image. `brightness` is
/// CS6's 10–300%.
///
/// This is light *added* to the picture rather than a filter of it: nothing
/// here reads the pixel it is writing to except to add to it. Alpha is left
/// alone — a flare does not make a transparent layer opaque.
///
/// Deliberately not on the GPU. It is per-pixel work of exactly the shape a
/// shader wants, but it runs once when the user presses OK, and the result is
/// needed straight back on the CPU to become the layer — so the upload and
/// readback would be most of the time spent, which is the same trap
/// documented for compositing in `docs/gpu-migration.md`. The dialog's live
/// preview goes through a proxy a few hundred pixels across, which is far
/// below `MIN_GPU_PIXELS` anyway.
pub fn lens_flare(pixmap: &mut Pixmap, center: (f32, f32), brightness: f32, kind: LensType) {
    if pixmap.is_empty() {
        return;
    }
    let lens = kind.lens();
    let gain = brightness.clamp(10.0, 300.0) / 100.0;

    let width = pixmap.width() as f32;
    let height = pixmap.height() as f32;
    // Every size in a lens is a fraction of this, so that a flare on a proxy
    // and the same flare on the full image are the same picture.
    let span = 0.5 * (width * width + height * height).sqrt();

    let flare = (center.0 * width, center.1 * height);
    let middle = (width * 0.5, height * 0.5);
    // The axis the ghosts are strung along: from the flare through the middle
    // of the frame and out the other side.
    let axis = (middle.0 - flare.0, middle.1 - flare.1);

    let stride = pixmap.stride();
    let row_width = pixmap.width() as usize;
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let py = row as f32 + 0.5;
            for x in 0..row_width {
                let px = x as f32 + 0.5;
                let (dx, dy) = (px - flare.0, py - flare.1);
                let distance = (dx * dx + dy * dy).sqrt();

                let mut light = (0.0f32, 0.0f32, 0.0f32);

                // The core, and the glow it sits in. An inverse square rather
                // than a bell curve: it has to be blinding in the middle and
                // still faintly there a long way out, which a bell curve
                // reaches zero far too quickly to do.
                let core_radius = lens.core * span;
                let core = 1.0 / (1.0 + (distance / core_radius).powi(2));
                add(&mut light, (1.00, 0.98, 0.94), core);
                let glow = (-(distance / (lens.glow * span)).powi(2)).exp();
                add(&mut light, (1.00, 0.95, 0.86), glow * lens.glow_strength);

                // The ring thrown around it.
                let ring = (distance - lens.halo * span) / (0.08 * span);
                add(&mut light, (0.92, 0.76, 1.00), (-ring * ring).exp() * lens.halo_strength);

                // Rays. One atan2 for all of them, and none at all for the
                // lens that has none.
                if lens.rays > 0 {
                    let angle = dy.atan2(dx);
                    let spokes = lens.rays as f32;
                    // How far round we are between one ray and the next, as
                    // -0.5..0.5 — the cheap way to ask "how near a ray is
                    // this" without walking the list of them.
                    let between =
                        (angle * spokes / std::f32::consts::TAU + 0.5).fract() - 0.5;
                    let near = (-(between * 14.0).powi(2)).exp();
                    let reach = (-(distance / (lens.ray_length * span)).powi(2)).exp();
                    add(&mut light, (1.00, 0.93, 0.80), near * reach * lens.ray_strength);
                }

                // The anamorphic streak: long sideways, thin the other way.
                if lens.streak_strength > 0.0 {
                    let along = (-(dx / (lens.streak * span)).powi(2)).exp();
                    let across = (-(dy / (0.012 * span)).powi(2)).exp();
                    add(&mut light, (0.55, 0.72, 1.00), along * across * lens.streak_strength);
                }

                for ghost in lens.ghosts {
                    let at = (flare.0 + axis.0 * ghost.at, flare.1 + axis.1 * ghost.at);
                    let (gx, gy) = (px - at.0, py - at.1);
                    let radius = ghost.radius * span;
                    if ghost.ring {
                        let edge = ((gx * gx + gy * gy).sqrt() - radius) / (0.3 * radius);
                        add(&mut light, ghost.tint, (-edge * edge).exp() * ghost.strength);
                    } else {
                        // Shaped by the aperture, not round: an iris is a
                        // hexagon and its reflections show it.
                        let shape = aperture(gx, gy) / radius;
                        let disc = (1.0 - shape * shape).max(0.0);
                        add(&mut light, ghost.tint, disc * disc * ghost.strength);
                    }
                }

                let i = x * 4;
                for (channel, amount) in [light.0, light.1, light.2].into_iter().enumerate() {
                    let lit = out[i + channel] as f32 + amount * gain * 255.0;
                    out[i + channel] = lit.clamp(0.0, 255.0) as u8;
                }
                // Alpha stands.
            }
        });
}

fn add(light: &mut (f32, f32, f32), tint: (f32, f32, f32), amount: f32) {
    light.0 += tint.0 * amount;
    light.1 += tint.1 * amount;
    light.2 += tint.2 * amount;
}

/// Distance from the middle of a hexagonal aperture, measured so that the six
/// flat sides are all one unit away.
///
/// Three dot products rather than the `atan2` the shape suggests: this runs
/// for every ghost at every pixel, and a ghost being slightly hexagonal is
/// not worth an inverse trig call eight times a pixel.
fn aperture(x: f32, y: f32) -> f32 {
    // Normals of the three pairs of parallel sides, at 0°, 60° and 120°.
    const COS60: f32 = 0.5;
    const SIN60: f32 = 0.866_025_4;
    let flat = x.abs();
    let left = (x * COS60 + y * SIN60).abs();
    let right = (x * COS60 - y * SIN60).abs();
    // Rounded off a little towards a circle: a real ghost's corners are soft.
    let hex = flat.max(left).max(right);
    let round = (x * x + y * y).sqrt();
    hex * 0.65 + round * 0.35
}

/// The three lamps CS6's Lighting Effects offers, in the order its Light Type
/// list gives them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LightType {
    /// A cone thrown at the picture from off to one side: a bright middle
    /// falling away to nothing at the edge of an ellipse.
    #[default]
    Spot,
    /// A bulb hung over the picture, throwing light out in every direction
    /// and falling off with distance.
    Point,
    /// The sun: parallel rays from one direction, the same everywhere, with
    /// no falloff at all.
    Infinite,
}

impl LightType {
    pub fn from_i32(value: i32) -> LightType {
        match value {
            1 => LightType::Point,
            2 => LightType::Infinite,
            _ => LightType::Spot,
        }
    }
}

/// Which channel is read as a height map — CS6's Texture list.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TextureChannel {
    #[default]
    None,
    Red,
    Green,
    Blue,
}

impl TextureChannel {
    pub fn from_i32(value: i32) -> TextureChannel {
        match value {
            1 => TextureChannel::Red,
            2 => TextureChannel::Green,
            3 => TextureChannel::Blue,
            _ => TextureChannel::None,
        }
    }
}

/// Everything CS6's Lighting Effects Properties panel collects, for one lamp.
///
/// One lamp, not a list of them: CS6 keeps a Lights panel and will stack
/// several, and this is the part of that feature not built — see the note on
/// [`lighting_effects`]. A list here would have to give up `Copy` on `Filter`,
/// which every caller of a filter relies on, so it is a change to make when
/// the panel that needs it arrives rather than in advance of it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Lighting {
    pub kind: LightType,
    /// The lamp's own colour, and how hard it burns (-100 to 100). Negative
    /// intensity takes light *away*, as CS6's does.
    pub color: Rgba8,
    pub intensity: f32,
    /// How much of a spot's cone is at full brightness before it starts to
    /// fall away (-100 to 100). Read only for a spot.
    pub hotspot: f32,
    /// The colour of the light that is there without any lamp, and how much
    /// of it (-100 to 100).
    pub colorize: Rgba8,
    pub ambience: f32,
    /// A stop control over the whole result (-100 to 100).
    pub exposure: f32,
    /// How sharp the highlight is (-100 matte to 100 shiny) and whose colour
    /// it takes (-100 plastic, the lamp's, to 100 metallic, the surface's).
    pub gloss: f32,
    pub metallic: f32,
    /// The channel read as a height map, and how tall it stands (0 to 100).
    pub texture: TextureChannel,
    pub height: f32,
    /// Where the lamp is and how far it reaches, as fractions of the frame,
    /// and which way it points in degrees. In CS6 these are the handles you
    /// drag on the canvas rather than numbers in the panel.
    pub center: (f32, f32),
    pub size: f32,
    pub angle: f32,
}

impl Default for Lighting {
    fn default() -> Self {
        // CS6's defaults for a new Spot light.
        Self {
            kind: LightType::Spot,
            color: Rgba8::WHITE,
            intensity: 25.0,
            hotspot: 44.0,
            colorize: Rgba8::WHITE,
            ambience: 0.0,
            exposure: 0.0,
            gloss: 0.0,
            metallic: 0.0,
            texture: TextureChannel::None,
            height: 50.0,
            center: (0.5, 0.5),
            size: 0.45,
            angle: 45.0,
        }
    }
}

/// Light the picture as though a lamp were shining on it — Filter ▸ Render ▸
/// Lighting Effects.
///
/// This is not light *added* to the picture the way a flare is. The picture is
/// treated as a surface and re-lit: what a pixel comes out as is what it
/// reflects, so an unlit corner goes black however bright it started, and
/// that is the whole character of the filter.
///
/// The shading is the textbook one — ambient, diffuse by `N·L`, and a
/// specular highlight — with the normal `N` read off whichever channel the
/// Texture list names. With no texture the surface is flat and only the
/// lamp's own falloff shapes the light.
///
/// **What is not built**: CS6 runs this as a whole workspace — a Lights panel
/// holding several lamps at once, handles dragged on the canvas to aim them,
/// and a Presets list. This is one lamp, set from a dialog. The shading below
/// would take a list of lamps with no change to its shape; it is the panel
/// and the on-canvas handles that are missing.
///
/// Deliberately not on the GPU, for the same reason as [`lens_flare`]: it is
/// per-pixel work of the shape a shader likes, but it runs once on OK and the
/// result is wanted straight back on the CPU, so the trip would cost more
/// than the arithmetic. See `docs/gpu-migration.md`.
pub fn lighting_effects(pixmap: &mut Pixmap, opt: Lighting) {
    if pixmap.is_empty() {
        return;
    }
    let width = pixmap.width() as usize;
    let height = pixmap.height() as usize;
    let (fw, fh) = (width as f32, height as f32);

    // Sizes are fractions of the half-diagonal, as with the flare, so that
    // the dialog's shrunk preview is the same picture as the full render.
    let span = 0.5 * (fw * fw + fh * fh).sqrt();
    let reach = (opt.size.clamp(0.02, 3.0)) * span;
    let at = (opt.center.0 * fw, opt.center.1 * fh);
    let heading = opt.angle.to_radians();

    // Where the lamp hangs, in three dimensions: the picture is the z=0
    // plane and the lamp is above it. A spot is pushed off to one side as
    // well, which is what gives it a lit near edge and a dark far one.
    let lamp = match opt.kind {
        LightType::Spot => (
            at.0 + heading.cos() * reach * 0.55,
            at.1 + heading.sin() * reach * 0.55,
            reach * 0.85,
        ),
        LightType::Point => (at.0, at.1, reach * 0.7),
        // An infinite light has no position at all; this is never read.
        LightType::Infinite => (at.0, at.1, reach),
    };
    // The one direction an infinite light comes from, worked out once.
    let sun = {
        let elevation = 50.0f32.to_radians();
        let flat = elevation.cos();
        normalize((
            -heading.cos() * flat,
            -heading.sin() * flat,
            elevation.sin(),
        ))
    };

    // How much of the cone is at full brightness before it falls away.
    let hotspot = ((opt.hotspot.clamp(-100.0, 100.0) + 100.0) / 200.0).clamp(0.0, 0.95);

    // CS6's 25 is a lamp that leaves the picture about as bright as it found
    // it, which is what makes 25 the default rather than 100.
    let strength = opt.intensity.clamp(-100.0, 100.0) / 25.0;
    let ambient = opt.ambience.clamp(-100.0, 100.0) / 100.0;
    // Exposure in stops: fifty points either way is twice or half the light.
    let exposure = (opt.exposure.clamp(-100.0, 100.0) / 50.0).exp2();

    // A highlight only at all once Gloss is positive. At CS6's default of
    // zero, a flat untextured surface shows none — every pixel would face
    // the lamp equally and the picture would come back washed white.
    let shine = (opt.gloss.clamp(-100.0, 100.0) / 100.0).max(0.0).powf(1.5);
    let tightness = 2.0f32.powf(1.0 + (opt.gloss.clamp(-100.0, 100.0) + 100.0) / 200.0 * 6.0);
    // Plastic reflects the lamp's colour, metal its own.
    let metal = ((opt.metallic.clamp(-100.0, 100.0) + 100.0) / 200.0).clamp(0.0, 1.0);

    let lamp_color = (
        opt.color.r as f32 / 255.0,
        opt.color.g as f32 / 255.0,
        opt.color.b as f32 / 255.0,
    );
    let fill_color = (
        opt.colorize.r as f32 / 255.0,
        opt.colorize.g as f32 / 255.0,
        opt.colorize.b as f32 / 255.0,
    );

    // The height map, read out before anything is written: a normal needs the
    // neighbours of the pixel it belongs to, and those are about to change.
    let relief = (opt.texture != TextureChannel::None).then(|| {
        let channel = match opt.texture {
            TextureChannel::Red => 0,
            TextureChannel::Green => 1,
            _ => 2,
        };
        let mut map = vec![0.0f32; width * height];
        for (i, cell) in map.iter_mut().enumerate() {
            *cell = pixmap.as_bytes()[i * 4 + channel] as f32 / 255.0;
        }
        map
    });
    // How steeply the height map stands. Scaled against the frame so that a
    // texture keeps its relief on the dialog's proxy as well as full size.
    let relief_scale = opt.height.clamp(0.0, 100.0) / 100.0 * span * 0.05;

    let stride = pixmap.stride();
    let bytes = pixmap.as_bytes_mut();
    bytes
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let py = row as f32 + 0.5;
            for x in 0..width {
                let px = x as f32 + 0.5;

                // The surface normal. Flat unless a channel is standing in
                // as a height map, in which case the slope either side of
                // this pixel tilts it.
                let normal = match &relief {
                    None => (0.0, 0.0, 1.0),
                    Some(map) => {
                        let at = |sx: usize, sy: usize| map[sy * width + sx];
                        let (left, right) = (x.saturating_sub(1), (x + 1).min(width - 1));
                        let (up, down) = (row.saturating_sub(1), (row + 1).min(height - 1));
                        normalize((
                            (at(left, row) - at(right, row)) * relief_scale,
                            (at(x, up) - at(x, down)) * relief_scale,
                            1.0,
                        ))
                    }
                };

                // Where the light is coming from, and how much of it reaches
                // here.
                let (to_lamp, falloff) = match opt.kind {
                    LightType::Infinite => (sun, 1.0),
                    LightType::Point => {
                        let away = ((px - at.0).powi(2) + (py - at.1).powi(2)).sqrt() / reach;
                        (
                            normalize((lamp.0 - px, lamp.1 - py, lamp.2)),
                            (1.0 - away * away).max(0.0),
                        )
                    }
                    LightType::Spot => {
                        let away = ((px - at.0).powi(2) + (py - at.1).powi(2)).sqrt() / reach;
                        // Flat across the hotspot, then easing off to nothing
                        // at the edge of the cone rather than stopping dead.
                        let edge = if away <= hotspot {
                            1.0
                        } else {
                            let t = ((1.0 - away) / (1.0 - hotspot)).clamp(0.0, 1.0);
                            t * t * (3.0 - 2.0 * t)
                        };
                        (normalize((lamp.0 - px, lamp.1 - py, lamp.2)), edge)
                    }
                };

                let facing = dot(normal, to_lamp).max(0.0);
                let diffuse = facing * falloff * strength;

                // Halfway between the lamp and the eye, which looks straight
                // down at the picture.
                let highlight = if shine > 0.0 && falloff > 0.0 {
                    let half = normalize((to_lamp.0, to_lamp.1, to_lamp.2 + 1.0));
                    dot(normal, half).max(0.0).powf(tightness) * falloff * shine * strength.abs()
                } else {
                    0.0
                };

                let i = x * 4;
                let surface = (
                    out[i] as f32 / 255.0,
                    out[i + 1] as f32 / 255.0,
                    out[i + 2] as f32 / 255.0,
                );
                let lit = [
                    (fill_color.0 * ambient + lamp_color.0 * diffuse, surface.0, lamp_color.0),
                    (fill_color.1 * ambient + lamp_color.1 * diffuse, surface.1, lamp_color.1),
                    (fill_color.2 * ambient + lamp_color.2 * diffuse, surface.2, lamp_color.2),
                ];
                for (channel, (light, own, lamp_channel)) in lit.into_iter().enumerate() {
                    // Plastic at one end of Metallic, metal at the other.
                    let spec = highlight * (lamp_channel * (1.0 - metal) + own * metal);
                    let value = (own * light + spec) * exposure;
                    out[i + channel] = (value * 255.0).clamp(0.0, 255.0) as u8;
                }
                // Alpha stands: relighting a layer does not change its shape.
            }
        });
}

fn dot(a: (f32, f32, f32), b: (f32, f32, f32)) -> f32 {
    a.0 * b.0 + a.1 * b.1 + a.2 * b.2
}

fn normalize(v: (f32, f32, f32)) -> (f32, f32, f32) {
    let length = (v.0 * v.0 + v.1 * v.1 + v.2 * v.2).sqrt();
    if length <= f32::EPSILON {
        return (0.0, 0.0, 1.0);
    }
    (v.0 / length, v.1 / length, v.2 / length)
}

/// A tongue's contribution at one point, from 0 (nothing) to 1 (its heart).
///
/// Split out from the drawing so that the picture can be built a pixel at a
/// time — every tongue that reaches a pixel is asked, and the answers added —
/// rather than a tongue at a time. That is what lets the whole thing run
/// across threads: tongues overlap, so painting them one after another means
/// writing to the same pixels from several places at once.
fn tongue_at(stalk: &Stalk, options: &FlameOptions, px: f32, py: f32) -> f32 {
    let (sway_scale, taper) = options.style.sway_and_taper();
    let sway = options.turbulent / 100.0 * sway_scale;
    let jag = options.jag / 100.0;
    let half_width = stalk.width * 0.5;
    let strands = options.complexity.max(1.0);

    let across = (stalk.direction.1, -stalk.direction.0);
    let dx = px - stalk.base.0;
    let dy = py - stalk.base.1;
    let along = dx * stalk.direction.0 + dy * stalk.direction.1;
    let side = dx * across.0 + dy * across.1;

    let v = along / stalk.length;
    if !(0.0..=1.0).contains(&v) {
        return 0.0;
    }
    let u = side / half_width;
    if u.abs() > 2.5 {
        return 0.0;
    }

    // Domain warp: the tongue is pushed sideways more the higher up it is,
    // which is what makes it look carried by its own heat rather than merely
    // wobbling. Sampled across the width as well as up it, so different parts
    // of the same tongue go different ways and it splits into licks.
    let warp = fbm(stalk.seed, u * 1.6, v * strands * 0.55, 3) - 0.5;
    let rough = fbm(stalk.seed ^ 0x51ed, u * 5.0, v * strands * 2.2, 2) - 0.5;
    let pushed = u - (warp * 3.4 + rough * 2.0 * jag) * sway * v.powf(1.2);

    let edge = stalk.shape.half_width(v).powf(taper);
    if edge <= 0.0 {
        return 0.0;
    }

    // Hot right across the tongue and falling off quickly at its edge, rather
    // than a smooth dome: fire has an edge to it.
    let body = (1.0 - (pushed.abs() / edge).min(2.0)).max(0.0).powf(0.55);
    if body <= 0.0 {
        return 0.0;
    }

    // The strands themselves, which are what the eye reads as flame rather
    // than as a painted shape.
    let grain = fbm(stalk.seed ^ 0x9a17, pushed * strands * 0.5, v * strands * 1.1, 3);

    // Ragged at the top, and ragged *across* the top: the cut-off varies with
    // the width as well as the height, so the tips are torn rather than
    // trimmed to one line.
    let cut = 0.70 + fbm(stalk.seed ^ 0x2c05, pushed * 1.3, 4.0, 2) * 0.55;
    let height = ((cut - v) / (cut * 0.42)).clamp(0.0, 1.0);

    // Eased in at the bottom so the wick does not show as a hard line.
    let foot = options.bottom_alignment / 100.0;
    let base = if foot <= 0.0 {
        1.0
    } else {
        (v / (foot * 0.5 + 0.02)).clamp(0.0, 1.0)
    };

    (body * (0.35 + grain * 1.1) * height * base).clamp(0.0, 1.0)
}

/// The box a tongue can reach, so that a pixel need only ask the few tongues
/// that could possibly touch it.
fn tongue_bounds(stalk: &Stalk, options: &FlameOptions) -> (f32, f32, f32, f32) {
    let (sway_scale, _) = options.style.sway_and_taper();
    let sway = options.turbulent / 100.0 * sway_scale;
    let reach = stalk.width * (1.4 + sway * 3.0);
    let tip = (
        stalk.base.0 + stalk.direction.0 * stalk.length,
        stalk.base.1 + stalk.direction.1 * stalk.length,
    );
    (
        stalk.base.0.min(tip.0) - reach,
        stalk.base.1.min(tip.1) - reach,
        stalk.base.0.max(tip.0) + reach,
        stalk.base.1.max(tip.1) + reach,
    )
}

/// Paint the whole fire onto the layer.
fn draw(pixmap: &mut Pixmap, stalks: &[Stalk], options: &FlameOptions) {
    if stalks.is_empty() {
        return;
    }
    let boxes: Vec<(f32, f32, f32, f32)> =
        stalks.iter().map(|s| tongue_bounds(s, options)).collect();

    // Quality is how many samples a pixel is worth. A flame is noise stacked
    // on noise, so a single sample per pixel speckles; more of them smooth it
    // without changing what is drawn.
    let samples = match options.quality {
        0 => 1u32,
        1 => 2,
        2 => 4,
        3 => 8,
        _ => 16,
    };
    let opacity = options.opacity / 100.0;
    let width = pixmap.width() as i32;
    let stride = pixmap.stride();
    let boxes = &boxes;

    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            for x in 0..width {
                // Light, not strength, is what accumulates. Each tongue is
                // coloured for how hot *it* is and then added in; summing the
                // strengths first and colouring once would take a place where
                // four faint tongues overlap and render it as white-hot,
                // which turns a ring of fire into a solid band.
                let mut light = [0.0f32; 4];
                for s in 0..samples {
                    // Spread the samples over the pixel rather than stacking
                    // them on its middle, or they would all return the same
                    // answer.
                    let px = x as f32
                        + noise01(s.wrapping_mul(2654435761), (x as u32) ^ (y as u32) << 16);
                    let py = y as f32
                        + noise01(s.wrapping_mul(40503) ^ 0x9e37, (y as u32) ^ (x as u32) << 16);
                    for (stalk, bounds) in stalks.iter().zip(boxes) {
                        if px < bounds.0 || py < bounds.1 || px > bounds.2 || py > bounds.3 {
                            continue;
                        }
                        let strength = tongue_at(stalk, options, px, py);
                        if strength <= 0.004 {
                            continue;
                        }
                        let (r, g, b) = fire(strength, options.custom_color);
                        // Opacity is how solid one tongue is on its own. At
                        // CS6's default of 25 a tongue is about as bright as
                        // its own colour, and turning it up burns the overlaps
                        // out to white — which is why it is a low number and
                        // not a weak effect.
                        let alpha = (strength * (0.5 + opacity * 2.0)).clamp(0.0, 1.0);
                        light[0] += r * alpha;
                        light[1] += g * alpha;
                        light[2] += b * alpha;
                        light[3] += 255.0 * alpha;
                    }
                }

                let i = x as usize * 4;
                for c in 0..4 {
                    let value = light[c] / samples as f32;
                    if value > 0.0 {
                        out[i + c] = (out[i + c] as f32 + value).min(255.0) as u8;
                    }
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A short straight path across the middle of a black square.
    fn wick(size: u32) -> (Pixmap, Vec<(Vec<(f32, f32)>, bool)>) {
        let px = Pixmap::filled(size, size, Rgba8::BLACK);
        let y = size as f32 * 0.75;
        let line = vec![(size as f32 * 0.2, y), (size as f32 * 0.8, y)];
        (px, vec![(line, false)])
    }

    fn lit(px: &Pixmap) -> usize {
        px.as_bytes()
            .chunks_exact(4)
            .filter(|p| p[0] as u32 + p[1] as u32 + p[2] as u32 > 30)
            .count()
    }

    #[test]
    fn nothing_burns_without_a_path() {
        // The one thing Photoshop refuses this filter for. Drawing nothing
        // quietly would be worse than refusing: the user would be left
        // wondering which of twenty settings was at fault.
        let mut px = Pixmap::filled(64, 64, Rgba8::BLACK);
        flame(&mut px, &[], &FlameOptions::default());
        assert_eq!(lit(&px), 0);

        // ...and a "path" of one point is not a path either.
        let mut px = Pixmap::filled(64, 64, Rgba8::BLACK);
        flame(&mut px, &[(vec![(32.0, 32.0)], false)], &FlameOptions::default());
        assert_eq!(lit(&px), 0);
    }

    #[test]
    fn the_fire_stands_on_the_path_and_rises_from_it() {
        // Flames go up from the wick, not down from it: an angle of zero has
        // to point against the way screen coordinates count, and getting that
        // backwards gives a filter that works and looks upside down.
        let (mut px, path) = wick(200);
        flame(
            &mut px,
            &path,
            &FlameOptions {
                kind: FlameType::MultipleOneDirection,
                angle: 0.0,
                ..FlameOptions::default()
            },
        );

        let band = |from: i32, to: i32| {
            (from..to)
                .flat_map(|y| (0..200).map(move |x| (x, y)))
                .filter(|&(x, y)| {
                    let p = px.get(x, y);
                    p.r as u32 + p.g as u32 + p.b as u32 > 30
                })
                .count()
        };
        let above = band(60, 145);
        let below = band(155, 200);
        assert!(above > below * 4, "the fire went downwards: {} up, {} down", above, below);
    }

    #[test]
    fn the_same_arrangement_burns_the_same_way() {
        // Everything random in a flame comes from the Arrangement number, so
        // that the preview, the applied filter, the undo and the redo are all
        // the same fire. A running random source would give four.
        let render = |arrangement| {
            let (mut px, path) = wick(120);
            flame(
                &mut px,
                &path,
                &FlameOptions {
                    arrangement,
                    ..FlameOptions::default()
                },
            );
            px
        };
        assert_eq!(render(3).as_bytes(), render(3).as_bytes());
        assert_ne!(render(3).as_bytes(), render(4).as_bytes());
    }

    #[test]
    fn a_longer_flame_reaches_further() {
        let reach = |length| {
            let (mut px, path) = wick(240);
            flame(
                &mut px,
                &path,
                &FlameOptions {
                    length,
                    ..FlameOptions::default()
                },
            );
            // The highest row with any fire in it.
            (0..240)
                .find(|&y| {
                    (0..240).any(|x| {
                        let p = px.get(x, y);
                        p.r as u32 + p.g as u32 + p.b as u32 > 30
                    })
                })
                .unwrap_or(240)
        };
        assert!(reach(150.0) < reach(60.0), "Length did not change how far the fire reached");
    }

    #[test]
    fn a_custom_colour_is_burned_in_instead_of_fire_colours() {
        // The tick box is easy to read and forget to act on, and the result
        // still looks like a flame — just not the one that was asked for.
        let render = |custom| {
            let (mut px, path) = wick(160);
            flame(
                &mut px,
                &path,
                &FlameOptions {
                    custom_color: custom,
                    ..FlameOptions::default()
                },
            );
            px
        };
        let orange = render(None);
        let blue = render(Some(Rgba8::new(40, 90, 255, 255)));
        assert_ne!(orange.as_bytes(), blue.as_bytes());

        // A blue flame has to be bluer than it is red, which fire never is.
        let total = |px: &Pixmap, channel: usize| {
            px.as_bytes()
                .chunks_exact(4)
                .map(|p| p[channel] as u64)
                .sum::<u64>()
        };
        assert!(total(&blue, 2) > total(&blue, 0), "the custom colour was ignored");
        assert!(total(&orange, 0) > total(&orange, 2), "fire came out blue");
    }

    #[test]
    fn a_loop_is_spaced_so_its_seam_does_not_show() {
        // Adjust Interval for Loops. A closed path that does not take a whole
        // number of tongues has a gap or a double thickness where it joins up.
        let ring: Vec<(f32, f32)> = (0..=96)
            .map(|i| {
                let t = i as f32 / 96.0 * std::f32::consts::TAU;
                (120.0 + 70.0 * t.cos(), 120.0 + 70.0 * t.sin())
            })
            .collect();
        let total: f32 = ring.windows(2).map(|p| distance(p[0], p[1])).sum();

        let mut seed = 1u32;
        let options = FlameOptions {
            adjust_interval_for_loops: true,
            width: 100.0,
            interval: 30.0,
            ..FlameOptions::default()
        };
        let stalks = stalks_along(&ring, true, &options, &mut seed);

        // Evenly spaced all the way round means the count divides the length.
        let spacing = total / stalks.len() as f32;
        let plain = options.width * options.interval / 100.0;
        assert!(
            (spacing - plain).abs() < plain * 0.25,
            "the spacing was nudged from {:.1} to {:.1}, which is more than closing the seam \
             should need",
            plain,
            spacing
        );
    }

    #[test]
    fn clouds_is_deterministic() {
        let make = || {
            let mut pm = Pixmap::filled(64, 64, Rgba8::new(128, 128, 128, 255));
            clouds(&mut pm, false);
            pm
        };
        assert_eq!(make().as_bytes(), make().as_bytes());
    }

    #[test]
    fn clouds_is_grayscale() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(0, 0, 0, 255));
        clouds(&mut pm, false);
        for y in 0..32 {
            for x in 0..32 {
                let p = pm.get(x as i32, y as i32);
                assert_eq!(p.r, p.g);
                assert_eq!(p.g, p.b);
            }
        }
    }

    #[test]
    fn clouds_preserves_alpha() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(100, 100, 100, 200));
        clouds(&mut pm, false);
        for y in 0..32 {
            for x in 0..32 {
                assert_eq!(pm.get(x as i32, y as i32).a, 200);
            }
        }
    }

    #[test]
    fn clouds_produces_variety() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(128, 128, 128, 255));
        clouds(&mut pm, false);
        let values: std::collections::HashSet<u8> = pm.as_bytes()
            .chunks_exact(4)
            .map(|p| p[0])
            .collect();
        assert!(values.len() > 10, "clouds produced too few distinct values: {}", values.len());
    }

    #[test]
    fn difference_clouds_differs_from_clouds() {
        let mut pm_clouds = Pixmap::filled(32, 32, Rgba8::new(128, 128, 128, 255));
        let mut pm_diff = Pixmap::filled(32, 32, Rgba8::new(128, 128, 128, 255));
        clouds(&mut pm_clouds, false);
        clouds(&mut pm_diff, true);
        assert_ne!(pm_clouds.as_bytes(), pm_diff.as_bytes());
    }

    #[test]
    fn empty_pixmap_clouds_does_not_panic() {
        let mut pm = Pixmap::new(0, 0);
        clouds(&mut pm, false);
        clouds(&mut pm, true);
    }

    /// Same seed, same fibres — the property the Randomize button and undo
    /// both depend on.
    #[test]
    fn fibers_are_deterministic_per_seed() {
        let render = |seed| {
            let mut pm = Pixmap::filled(48, 48, Rgba8::BLACK);
            fibers(&mut pm, 32.0, 4.0, seed, Rgba8::new(255, 0, 0, 255), Rgba8::new(0, 0, 255, 255));
            pm
        };
        assert_eq!(render(7).as_bytes(), render(7).as_bytes());
        assert_ne!(render(7).as_bytes(), render(8).as_bytes());
    }

    #[test]
    fn fibers_blend_between_the_two_swatch_colours() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::BLACK);
        fibers(&mut pm, 32.0, 8.0, 3, Rgba8::new(255, 0, 0, 255), Rgba8::new(0, 0, 255, 255));
        for p in pm.as_bytes().chunks_exact(4) {
            // A blend of red and blue is always at least as blue as it is
            // green: any green in there would mean the swatches were ignored.
            assert!(p[2] >= p[1], "a fibre pixel came out green");
            assert!(p[0] >= p[1] || p[2] >= p[1], "a pixel fell outside the red-blue blend");
        }
    }

    /// Strands stand beside each other with a clean edge between them rather
    /// than fading into one another. This is what holds up under a 500% zoom,
    /// where CS6's fibres are still crisp single pixels: interpolating across
    /// as well as along, which is what ordinary lattice noise does, leaves a
    /// soft smear that only passes at a distance.
    #[test]
    fn fibers_have_hard_edges_between_strands() {
        let mut pm = Pixmap::filled(256, 256, Rgba8::BLACK);
        fibers(&mut pm, 32.0, 4.0, 3, Rgba8::BLACK, Rgba8::WHITE);
        let (mut steep, mut steps) = (0u32, 0u32);
        for y in 0..256 {
            for x in 0..255 {
                let (a, b) = (pm.get(x, y).r as i32, pm.get(x + 1, y).r as i32);
                if (a - b).abs() > 64 {
                    steep += 1;
                }
                steps += 1;
            }
        }
        assert!(
            steep * 5 > steps,
            "strands are smeared into one another: only {steep} of {steps} \
             sideways steps are a clean jump"
        );
    }

    /// And variance shortens them. Low down the slider a strand runs the
    /// whole height; by the top the picture is short broken dashes. Getting
    /// this backwards — a strand whose tone cannot vary down its column at
    /// all — leaves every setting looking like the bottom of the slider.
    #[test]
    fn fibers_break_up_as_variance_climbs() {
        let column_travel = |variance: f32| {
            let mut pm = Pixmap::filled(96, 192, Rgba8::BLACK);
            fibers(&mut pm, variance, 4.0, 11, Rgba8::BLACK, Rgba8::WHITE);
            let mut diffs = 0u64;
            for x in 0..96 {
                for y in 0..191 {
                    let (a, b) = (pm.get(x, y).r as i32, pm.get(x, y + 1).r as i32);
                    diffs += (a - b).unsigned_abs() as u64;
                }
            }
            diffs
        };
        let (low, high) = (column_travel(8.0), column_travel(48.0));
        assert!(
            high > low * 2,
            "variance did not break the fibres up: {low} at 8 vs {high} at 48"
        );
    }

    #[test]
    fn fibers_stretch_with_strength() {
        // Higher strength: a column of pixels stays the same colour for
        // longer. Measured as how much a randomly-picked column varies down
        // its length — more strength, less variation along it.
        let column_travel = |strength: f32| {
            let mut pm = Pixmap::filled(64, 128, Rgba8::BLACK);
            fibers(&mut pm, 48.0, strength, 5, Rgba8::BLACK, Rgba8::WHITE);
            let mut diffs = 0u64;
            for y in 0..127 {
                let a = pm.get(10, y);
                let b = pm.get(10, y + 1);
                diffs += (a.r as i32 - b.r as i32).abs() as u64;
            }
            diffs
        };
        assert!(
            column_travel(48.0) < column_travel(1.0),
            "strength did not stretch the fibres lengthwise"
        );
    }

    /// A strand runs the whole height of the picture: a step sideways changes
    /// the tone far more than a step down does. Isotropic noise, which is what
    /// this filter used to lay down, scores about even on the two and reads as
    /// grey cloud rather than as fibres.
    #[test]
    fn fibers_run_down_the_picture_not_across_it() {
        let mut pm = Pixmap::filled(128, 128, Rgba8::BLACK);
        fibers(&mut pm, 12.0, 4.0, 5, Rgba8::BLACK, Rgba8::WHITE);
        let (mut across, mut down) = (0u64, 0u64);
        for y in 0..127i32 {
            for x in 0..127i32 {
                let here = pm.get(x, y).r as i32;
                across += (here - pm.get(x + 1, y).r as i32).unsigned_abs() as u64;
                down += (here - pm.get(x, y + 1).r as i32).unsigned_abs() as u64;
            }
        }
        assert!(
            across > down * 4,
            "fibres are not lengthwise: {across} across vs {down} down"
        );
    }

    /// And they go the whole way between the two colours. A field of noise
    /// sits in a narrow band about its mean unless it is stretched, and an
    /// unstretched one is the grey mush this filter used to produce.
    #[test]
    fn fibers_reach_both_colours() {
        let mut pm = Pixmap::filled(128, 128, Rgba8::BLACK);
        fibers(&mut pm, 12.0, 4.0, 2, Rgba8::BLACK, Rgba8::WHITE);
        let tones: Vec<u8> = pm.as_bytes().chunks_exact(4).map(|p| p[0]).collect();
        assert!(tones.iter().copied().min().unwrap() < 16, "no fibre went dark");
        assert!(tones.iter().copied().max().unwrap() > 239, "no fibre went light");
    }

    #[test]
    fn fibers_preserve_alpha() {
        let mut pm = Pixmap::filled(16, 16, Rgba8::new(0, 0, 0, 123));
        fibers(&mut pm, 32.0, 4.0, 1, Rgba8::BLACK, Rgba8::WHITE);
        assert!(pm.as_bytes().chunks_exact(4).all(|p| p[3] == 123));
    }

    #[test]
    fn fibers_on_an_empty_pixmap_do_nothing() {
        let mut pm = Pixmap::new(0, 0);
        fibers(&mut pm, 32.0, 4.0, 1, Rgba8::BLACK, Rgba8::WHITE);
    }

    /// A flare on a dark frame, for the tests below to read.
    fn flare(width: u32, height: u32, at: (f32, f32), brightness: f32, lens: LensType) -> Pixmap {
        let mut pm = Pixmap::filled(width, height, Rgba8::new(20, 20, 20, 255));
        lens_flare(&mut pm, at, brightness, lens);
        pm
    }

    #[test]
    fn a_flare_burns_brightest_where_it_is_put() {
        let pm = flare(240, 180, (0.5, 0.5), 100.0, LensType::Zoom50To300);
        assert_eq!(pm.get(120, 90).r, 255, "the middle of the flare is not blown out");
        // The far corner is off the axis, so no ghost lands on it either.
        assert!(pm.get(4, 174).r < 90, "the flare lit the whole frame");
    }

    #[test]
    fn a_flare_goes_where_it_is_told() {
        let pm = flare(240, 180, (0.25, 0.25), 100.0, LensType::Zoom50To300);
        let here = pm.get(60, 45).r;
        // The opposite corner along the diagonal is where the ghosts fall, so
        // compare against a corner off that axis instead.
        let elsewhere = pm.get(180, 45).r;
        assert!(here > elsewhere + 100, "{here} at the flare, {elsewhere} away from it");
    }

    #[test]
    fn brightness_scales_the_light_a_flare_adds() {
        // Read off the axis and away from the blown-out core, where there is
        // room for the difference to show.
        let sample = |brightness| flare(240, 180, (0.2, 0.2), brightness, LensType::Prime35)
            .get(150, 40)
            .r as i32;
        let (dim, bright) = (sample(25.0), sample(300.0));
        assert!(bright > dim, "brightness did not raise the flare: {dim} then {bright}");
    }

    /// The ghosts are reflections thrown back through the middle of the frame,
    /// so they land on the line from the flare through the centre and out the
    /// far side — not scattered anywhere else.
    #[test]
    fn the_ghosts_line_up_through_the_middle_of_the_frame() {
        // On black, so what is measured is the light the flare added and
        // nothing else.
        let mut pm = Pixmap::filled(400, 300, Rgba8::BLACK);
        lens_flare(&mut pm, (0.12, 0.5), 100.0, LensType::Zoom50To300);
        // The right-hand half of the picture, along the axis and well off it.
        let brightness = |row: i32| -> u64 {
            (200..400).map(|x| pm.get(x, row).r as u64).sum()
        };
        let along = brightness(150);
        let across = brightness(30);
        assert!(
            along > across * 2,
            "the ghosts did not follow the axis: {along} along it, {across} off it"
        );
    }

    #[test]
    fn the_four_lenses_throw_different_flares() {
        let each: Vec<Vec<u8>> = [
            LensType::Zoom50To300,
            LensType::Prime35,
            LensType::Prime105,
            LensType::MoviePrime,
        ]
        .into_iter()
        .map(|lens| flare(160, 120, (0.5, 0.4), 100.0, lens).as_bytes().to_vec())
        .collect();
        for (i, one) in each.iter().enumerate() {
            for other in each.iter().skip(i + 1) {
                assert_ne!(one, other, "two lenses threw the same flare");
            }
        }
    }

    /// A flare is light added to the picture, not a picture of its own: it
    /// must not make a transparent layer opaque.
    #[test]
    fn a_flare_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(10, 10, 10, 77));
        lens_flare(&mut pm, (0.5, 0.5), 300.0, LensType::Prime35);
        assert!(pm.as_bytes().chunks_exact(4).all(|p| p[3] == 77));
    }

    /// The dialog's preview filters a shrunk proxy rather than the layer, which
    /// is only honest because a flare is placed and sized as a fraction of the
    /// frame. If that ever stops being true the preview starts lying about
    /// where the flare will land.
    #[test]
    fn a_flare_is_the_same_picture_at_any_size() {
        let big = flare(600, 400, (0.7, 0.3), 100.0, LensType::Zoom50To300);
        let small = flare(150, 100, (0.7, 0.3), 100.0, LensType::Zoom50To300);
        let mut worst = 0i32;
        for y in 0..100 {
            for x in 0..150 {
                // A proxy pixel stands for the four-by-four block of full-size
                // pixels under it, so that is what it has to agree with —
                // comparing single pixels would only measure how steeply the
                // core falls off between one sample and the next.
                let mut block = 0i32;
                let mut blown = false;
                for dy in 0..4 {
                    for dx in 0..4 {
                        let level = big.get(x * 4 + dx, y * 4 + dy).r as i32;
                        blown |= level >= 250;
                        block += level;
                    }
                }
                // Where the flare has blown out, averaging sixteen clipped
                // pixels is not the same thing as clipping their average, and
                // no amount of care about the geometry will make it so. That
                // is white either way; it is everywhere else that has to
                // agree.
                if blown {
                    continue;
                }
                let mean = block / 16;
                worst = worst.max((mean - small.get(x, y).r as i32).abs());
            }
        }
        // Twenty rather than a handful: the core and the rays are peaked
        // enough that a mean of sixteen samples sits measurably above a single
        // one taken through the middle of them, whatever the geometry does.
        // A flare that was sized in pixels rather than in fractions of the
        // frame would be out by ten times this.
        assert!(worst <= 20, "proxy and full size disagree by {worst} levels");
    }

    #[test]
    fn a_flare_on_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        lens_flare(&mut pm, (0.5, 0.5), 100.0, LensType::Zoom50To300);
    }

    /// An evenly-toned frame, so that what comes back is the lighting and
    /// nothing the picture brought with it.
    fn under(light: Lighting) -> Pixmap {
        let mut pm = Pixmap::filled(240, 180, Rgba8::new(160, 160, 160, 255));
        lighting_effects(&mut pm, light);
        pm
    }

    /// A lamp does not *add* light the way a flare does — it decides what the
    /// picture reflects. Outside a spot's cone, with no ambient light, there
    /// is nothing to reflect and the picture goes black however bright it
    /// started.
    #[test]
    fn a_spot_lights_its_cone_and_leaves_the_rest_dark() {
        let pm = under(Lighting::default());
        assert!(pm.get(120, 90).r > 120, "the middle of the cone is not lit");
        assert!(pm.get(4, 4).r < 12, "outside the cone did not go dark");
    }

    /// Ambience is the light that is there without any lamp, so it is what
    /// brings the unlit corners back.
    #[test]
    fn ambience_lifts_what_the_lamp_does_not_reach() {
        let dark = under(Lighting::default()).get(4, 4).r;
        let lifted = under(Lighting {
            ambience: 50.0,
            ..Lighting::default()
        })
        .get(4, 4)
        .r;
        assert!(lifted > dark + 40, "{dark} unlit, {lifted} with ambience");
    }

    /// The sun reaches everywhere equally: no falloff, no hotspot, no dark
    /// corners.
    #[test]
    fn an_infinite_light_falls_evenly_across_the_frame() {
        let pm = under(Lighting {
            kind: LightType::Infinite,
            ..Lighting::default()
        });
        let middle = pm.get(120, 90).r as i32;
        let corner = pm.get(4, 4).r as i32;
        assert!((middle - corner).abs() <= 1, "{middle} in the middle, {corner} in the corner");
    }

    /// A point light falls off with distance from where it hangs; a spot has
    /// a flat hotspot before it starts to. Reading across the two at the same
    /// place is what tells them apart.
    #[test]
    fn a_point_light_falls_off_where_a_spots_hotspot_is_still_flat() {
        let point = under(Lighting {
            kind: LightType::Point,
            ..Lighting::default()
        });
        let spot = under(Lighting {
            hotspot: 80.0,
            ..Lighting::default()
        });
        let fade = |pm: &Pixmap| pm.get(120, 90).r as i32 - pm.get(120, 130).r as i32;
        assert!(
            fade(&point) > fade(&spot),
            "the point light did not fall off faster than the spot's hotspot"
        );
    }

    #[test]
    fn intensity_and_exposure_both_turn_the_light_up() {
        let middle = |light: Lighting| under(light).get(120, 90).r as i32;
        let base = Lighting {
            intensity: 10.0,
            ..Lighting::default()
        };
        assert!(middle(Lighting { intensity: 20.0, ..base }) > middle(base));
        assert!(middle(Lighting { exposure: 50.0, ..base }) > middle(base));
        // And negative intensity takes light away rather than adding it.
        assert!(middle(Lighting { intensity: -25.0, ..base }) < middle(base));
    }

    /// Without a texture every pixel faces the lamp alike, so a highlight
    /// would be a flat white wash over the whole cone. CS6 shows none at its
    /// default Gloss, and neither does this.
    #[test]
    fn a_texture_is_what_gives_the_light_something_to_catch() {
        let shiny = Lighting {
            gloss: 80.0,
            intensity: 40.0,
            ..Lighting::default()
        };
        let flat = under(shiny);
        // A raised channel, for the same light to catch on.
        let mut bumpy = Pixmap::filled(240, 180, Rgba8::new(160, 160, 160, 255));
        for y in 0..180 {
            for x in 0..240 {
                let ridge = if (x / 8) % 2 == 0 { 40 } else { 200 };
                bumpy.set(x, y, Rgba8::new(160, ridge, 160, 255));
            }
        }
        lighting_effects(
            &mut bumpy,
            Lighting {
                texture: TextureChannel::Green,
                height: 90.0,
                ..shiny
            },
        );
        // Flat: neighbouring pixels in the cone are shaded alike. Bumpy: the
        // slopes catch the light and the ridges do not.
        let spread = |pm: &Pixmap| {
            let (mut low, mut high) = (255i32, 0i32);
            for x in 100..140 {
                let level = pm.get(x, 90).g as i32;
                low = low.min(level);
                high = high.max(level);
            }
            high - low
        };
        assert!(
            spread(&bumpy) > spread(&flat) + 30,
            "the texture did not shape the light: {} flat, {} bumpy",
            spread(&flat),
            spread(&bumpy)
        );
    }

    #[test]
    fn lighting_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(200, 200, 200, 90));
        lighting_effects(&mut pm, Lighting::default());
        assert!(pm.as_bytes().chunks_exact(4).all(|p| p[3] == 90));
    }

    /// Same reasoning as the flare: the dialog previews a shrunk proxy, so
    /// the lamp has to be placed and sized as a fraction of the frame.
    #[test]
    fn a_lamp_is_the_same_light_at_any_size() {
        let render = |w: u32, h: u32| {
            let mut pm = Pixmap::filled(w, h, Rgba8::new(160, 160, 160, 255));
            lighting_effects(
                &mut pm,
                Lighting {
                    center: (0.35, 0.6),
                    ..Lighting::default()
                },
            );
            pm
        };
        let (big, small) = (render(600, 400), render(150, 100));
        let mut worst = 0i32;
        for y in 0..100 {
            for x in 0..150 {
                let mut block = 0i32;
                for dy in 0..4 {
                    for dx in 0..4 {
                        block += big.get(x * 4 + dx, y * 4 + dy).r as i32;
                    }
                }
                worst = worst.max((block / 16 - small.get(x, y).r as i32).abs());
            }
        }
        assert!(worst <= 8, "proxy and full size disagree by {worst} levels");
    }

    #[test]
    fn lighting_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        lighting_effects(&mut pm, Lighting::default());
    }
}

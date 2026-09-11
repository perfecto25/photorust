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
}

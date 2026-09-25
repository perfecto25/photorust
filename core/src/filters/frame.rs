//! Filter ▸ Render ▸ Picture Frame.
//!
//! A decorative border drawn over the picture: vines, flowers and leaves laid
//! round the edge of the canvas, or a ruled or moulded frame. CS6's dialog
//! lists 47 frames, each with its own pattern of the same three ingredients —
//! a **vine** in one colour, **flowers** chosen from 22 shapes in another,
//! and **leaves** from 23 in a third — so the frames are recipes and the
//! shapes a shared catalogue.
//!
//! The Advanced tab's **Number of Lines**, **Thickness**, **Angle**, **Fade**
//! and **Invert** are applied to every frame they make sense for; see
//! [`FrameOptions`].
//!
//! Everything is drawn as filled outlines, anti-aliased, straight onto the
//! layer — CS6 renders the frame into the pixels rather than as vectors.
//!
//! No GPU path. The work is a few hundred small shapes scan-converted one at
//! a time, each touching a patch a few dozen pixels across — the case §7 of
//! CLAUDE.md names as not fitting: small regions, and nothing per-pixel over
//! the whole image to speak of.

use crate::buffer::{Pixmap, Rgba8};
use std::f32::consts::{PI, TAU};

/// CS6's ranges for the Basic tab's sliders.
pub const MARGIN: std::ops::RangeInclusive<u32> = 0..=100;
pub const SIZE: std::ops::RangeInclusive<u32> = 1..=100;
pub const ARRANGEMENT: std::ops::RangeInclusive<u32> = 1..=20;
pub const FLOWER_SIZE: std::ops::RangeInclusive<u32> = 1..=100;
pub const LEAF_SIZE: std::ops::RangeInclusive<u32> = 1..=100;

/// CS6's ranges for the Advanced tab's sliders, read off where its slider
/// thumbs sit: Thickness 29 is a seventh of the way along, Number of Lines 15
/// half way.
pub const LINES: std::ops::RangeInclusive<u32> = 1..=30;
pub const THICKNESS: std::ops::RangeInclusive<u32> = 1..=200;
pub const ANGLE: std::ops::RangeInclusive<u32> = 0..=360;
pub const FADE: std::ops::RangeInclusive<u32> = 0..=100;

/// CS6's frames, by its number.
pub const FRAMES: std::ops::RangeInclusive<u32> = 1..=47;

/// CS6's frame names, in its order.
pub const FRAME_NAMES: [&str; 47] = [
    "Happy Vine",
    "Pretty Vine",
    "Smoke Signals",
    "Party",
    "Big Curls",
    "Tilde",
    "Romance",
    "Curly Dance",
    "Wisps",
    "Spring Weed",
    "Eyelash",
    "Magic Smoke",
    "Curly Vine",
    "Chainmail",
    "Fun Event",
    "Bush",
    "Simple Lace",
    "Pulse",
    "Root",
    "Snakes",
    "Mustache",
    "Circle Sprinkle",
    "Aligned Flowers",
    "Little Flowers",
    "Snowflakes",
    "Flurry",
    "Check Marks",
    "Focused Lines",
    "Focused Parallel Lines",
    "Focused Vibration",
    "Zen Garden",
    "Spikes",
    "Anemone",
    "Pinwheel",
    "Spacing",
    "Line Box",
    "Rounded Corners",
    "Inverse Rounded Corners 1",
    "Inverse Rounded Corners 2",
    "Dual Rounded Corners 1",
    "Dual Rounded Corners 2",
    "Art Frame",
    "Rounded Art Frame",
    "Inverse Rounded Art Frame 1",
    "Inverse Rounded Art Frame 2",
    "Dual Rounded Art Frame 1",
    "Dual Rounded Art Frame 2",
];

/// CS6's flowers, 1 to 22. 0 is None.
pub const FLOWER_NAMES: [&str; 22] = [
    "Small Circle",
    "Circle",
    "Star Flower",
    "Small Flower",
    "Orbit",
    "Pinwheel",
    "Petals",
    "Sunflower Gate",
    "Rose",
    "Grass Circle",
    "Sun",
    "Broken Line Circle",
    "Magical Door",
    "Twinkle",
    "Shiny Star",
    "Star Cloud",
    "Round Plus",
    "Plus Mark",
    "Heart",
    "Star",
    "Snow Flake",
    "Cat's Footprint",
];

/// CS6's leaves, 1 to 23. 0 is None.
pub const LEAF_NAMES: [&str; 23] = [
    "Circle",
    "Square",
    "Drop 1",
    "Leaf 1",
    "Drop 2",
    "Drop 3",
    "Leaf 2",
    "Puff",
    "Leaf 3",
    "Ginkgo leaf",
    "Triangle",
    "Leaf 4",
    "Star 1",
    "Oval",
    "Trapezoid",
    "Lollipop 1",
    "Leaf 5",
    "Star 2",
    "Lollipop 2",
    "Cross",
    "Heart",
    "T",
    "Star 3",
];

/// Everything the Basic tab collects.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameOptions {
    /// CS6's frame number, 18 to 47.
    pub frame: u32,
    pub vine: Rgba8,
    pub margin: u32,
    pub size: u32,
    /// Which of CS6's random arrangements: a seed, not a quantity.
    pub arrangement: u32,
    /// 0 for none, else 1 to 22.
    pub flower: u32,
    pub flower_colour: Rgba8,
    pub flower_size: u32,
    /// 0 for none, else 1 to 23.
    pub leaf: u32,
    pub leaf_colour: Rgba8,
    pub leaf_size: u32,
    /// Advanced: how densely the line frames — Chainmail and the three
    /// Focused frames — lay their lines, 15 being CS6's default.
    pub lines: u32,
    /// Advanced: how thick the vines and lines are, and how wide the ruled
    /// and moulded frames' bands; 29 is CS6's default.
    pub thickness: u32,
    /// Advanced: in degrees, how far every flower and leaf is turned, and how
    /// far off square the slanted line frames lean.
    pub angle: u32,
    /// Advanced: how far the frame fades out from the corners to the middle
    /// of each side, 0 not at all and 100 to nothing.
    pub fade: u32,
    /// Advanced: the frame turned inside out — motifs that lean in lean out,
    /// and a moulding lit from the other side.
    pub invert: bool,
}

impl Default for FrameOptions {
    /// CS6's defaults, off its own dialog.
    fn default() -> FrameOptions {
        FrameOptions {
            frame: 34,
            vine: Rgba8::new(60, 84, 34, 255),
            margin: 14,
            size: 20,
            arrangement: 1,
            flower: 2,
            flower_colour: Rgba8::new(0, 88, 218, 255),
            flower_size: 20,
            leaf: 4,
            leaf_colour: Rgba8::new(104, 170, 60, 255),
            leaf_size: 20,
            lines: 15,
            thickness: 29,
            angle: 10,
            fade: 0,
            invert: false,
        }
    }
}

/// Which of the three ingredients a frame draws. CS6 greys out the controls
/// for the ones it does not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Uses {
    pub vine: bool,
    pub flower: bool,
    pub leaf: bool,
}

/// What `frame` draws with.
pub fn uses(frame: u32) -> Uses {
    let (vine, flower, leaf) = match frame {
        5 | 11 | 12 | 13 | 16 => (true, true, true),
        1..=17 => (true, true, false),
        18 | 19 => (true, true, true),
        20 | 21 => (true, true, false),
        22 | 23 => (false, true, false),
        24..=26 => (false, true, true),
        27..=32 => (true, true, false),
        33 => (false, true, false),
        34 => (true, true, false),
        35 => (false, true, false),
        // The ruled and moulded frames are drawn in the vine colour alone.
        _ => (true, false, false),
    };
    Uses { vine, flower, leaf }
}

/// Whether `frame` is drawn in lines that Number of Lines sets the density
/// of. CS6 greys the slider out for the rest.
pub fn uses_lines(frame: u32) -> bool {
    matches!(frame, 14 | 28 | 29 | 30)
}

// ------------------------------------------------------------ rasterising --

type Contour = Vec<(f32, f32)>;

/// Sub-scanlines per pixel row: vertical anti-aliasing. Horizontal comes from
/// fractional coverage at the ends of each span.
const SUBSAMPLES: usize = 4;

/// Twice the signed area: positive for one winding, negative for the other.
fn area(c: &[(f32, f32)]) -> f32 {
    let mut a = 0.0;
    for i in 0..c.len() {
        let (x0, y0) = c[i];
        let (x1, y1) = c[(i + 1) % c.len()];
        a += x0 * y1 - x1 * y0;
    }
    a
}

/// The contour wound the filling way.
fn solid(mut c: Contour) -> Contour {
    if area(&c) < 0.0 {
        c.reverse();
    }
    c
}

/// The contour wound the cutting way: under the non-zero rule it takes back
/// out what one solid contour around it put in.
fn hole(mut c: Contour) -> Contour {
    if area(&c) > 0.0 {
        c.reverse();
    }
    c
}

/// Add `weight` of coverage between `left` and `right`, with the end pixels
/// taking the fraction of them the span covers.
fn add_span(row: &mut [f32], left: f32, right: f32, weight: f32) {
    let n = row.len() as f32;
    let (l, r) = (left.max(0.0), right.min(n));
    if r <= l {
        return;
    }
    let (li, rf) = (l.floor() as usize, r.floor() as usize);
    if li == rf || (li + 1 == rf && r == rf as f32) {
        row[li.min(row.len() - 1)] += (r - l) * weight;
        return;
    }
    row[li] += (li as f32 + 1.0 - l) * weight;
    let end = rf.min(row.len());
    for v in &mut row[li + 1..end] {
        *v += weight;
    }
    if rf < row.len() {
        row[rf] += (r - rf as f32) * weight;
    }
}

/// Fill `contours` under the non-zero rule with `colour`, `alpha` of the way.
fn fill(pixmap: &mut Pixmap, contours: &[Contour], colour: Rgba8, alpha: f32) {
    let edges: Vec<(f32, f32, f32, f32)> = contours
        .iter()
        .filter(|c| c.len() >= 3)
        .flat_map(|c| {
            (0..c.len()).map(move |i| {
                let (x0, y0) = c[i];
                let (x1, y1) = c[(i + 1) % c.len()];
                (x0, y0, x1, y1)
            })
        })
        // A point that is not a number would throw the winding count off for
        // the rest of the row and flood it.
        .filter(|&(x0, y0, x1, y1)| x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite())
        .collect();
    if edges.is_empty() || alpha <= 0.0 {
        return;
    }
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &(x0, y0, x1, y1) in &edges {
        min_x = min_x.min(x0.min(x1));
        max_x = max_x.max(x0.max(x1));
        min_y = min_y.min(y0.min(y1));
        max_y = max_y.max(y0.max(y1));
    }
    let left = (min_x.floor() as i32).max(0);
    let top = (min_y.floor() as i32).max(0);
    let right = (max_x.ceil() as i32 + 1).min(pixmap.width() as i32);
    let bottom = (max_y.ceil() as i32 + 1).min(pixmap.height() as i32);
    if right <= left || bottom <= top {
        return;
    }
    let width = (right - left) as usize;
    let mut row = vec![0.0f32; width];
    let mut crossings: Vec<(f32, i32)> = Vec::new();
    let weight = 1.0 / SUBSAMPLES as f32;
    for y in top..bottom {
        row.fill(0.0);
        for s in 0..SUBSAMPLES {
            let sy = y as f32 + (s as f32 + 0.5) / SUBSAMPLES as f32;
            crossings.clear();
            for &(x0, y0, x1, y1) in &edges {
                // Half-open in y, so a vertex on the sub-scanline counts once.
                if (y0 <= sy) == (y1 <= sy) {
                    continue;
                }
                let t = (sy - y0) / (y1 - y0);
                crossings.push((x0 + t * (x1 - x0) - left as f32, if y0 <= sy { 1 } else { -1 }));
            }
            crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut winding = 0;
            for pair in crossings.windows(2) {
                winding += pair[0].1;
                if winding != 0 {
                    add_span(&mut row, pair[0].0, pair[1].0, weight);
                }
            }
        }
        for (i, &c) in row.iter().enumerate() {
            let a = c.min(1.0) * alpha;
            if a <= 0.0 {
                continue;
            }
            let x = left + i as i32;
            let dst = pixmap.get(x, y);
            pixmap.set(x, y, crate::brush::source_over(dst, colour, a));
        }
    }
}

// --------------------------------------------------------------- geometry --

fn circle(cx: f32, cy: f32, r: f32) -> Contour {
    ellipse(cx, cy, r, r, 0.0)
}

fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32, turn: f32) -> Contour {
    let n = ((rx.max(ry) * 1.5) as usize).clamp(10, 96);
    let (s, c) = turn.sin_cos();
    solid(
        (0..n)
            .map(|i| {
                let t = i as f32 / n as f32 * TAU;
                let (x, y) = (rx * t.cos(), ry * t.sin());
                (cx + x * c - y * s, cy + x * s + y * c)
            })
            .collect(),
    )
}

fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Contour {
    solid(vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
}

/// A closed curve round the origin, `radius` of each angle, in `n` steps.
fn polar(n: usize, radius: impl Fn(f32) -> f32) -> Contour {
    solid(
        (0..n)
            .map(|i| {
                let t = i as f32 / n as f32 * TAU;
                let r = radius(t);
                (r * t.cos(), r * t.sin())
            })
            .collect(),
    )
}

/// A star of `points` points between `outer` and `inner`, one pointing up.
fn star(points: usize, outer: f32, inner: f32) -> Contour {
    solid(
        (0..points * 2)
            .map(|i| {
                let r = if i % 2 == 0 { outer } else { inner };
                let t = i as f32 / (points * 2) as f32 * TAU - PI / 2.0;
                (r * t.cos(), r * t.sin())
            })
            .collect(),
    )
}

/// A petal along `angle` from `r0` to `r1` from the origin, `width` across
/// at its widest; `bulge` below 1 fattens it towards the tip.
fn petal(angle: f32, r0: f32, r1: f32, width: f32, bulge: f32) -> Contour {
    let n = 16;
    let (s, c) = angle.sin_cos();
    let mut side: Vec<(f32, f32)> = Vec::with_capacity(2 * n + 2);
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let along = r0 + t * (r1 - r0);
        let half = width * 0.5 * (PI * t.powf(bulge)).sin().max(0.0);
        side.push((along, half));
    }
    let mut out: Vec<(f32, f32)> = side.iter().map(|&(a, h)| (a, h)).collect();
    out.extend(side.iter().rev().skip(1).take(n - 1).map(|&(a, h)| (a, -h)));
    solid(out.into_iter().map(|(a, h)| (a * c - h * s, a * s + h * c)).collect())
}

/// A band round the origin from `inner` to `outer`, from angle `a0` to `a1`.
fn arc_band(cx: f32, cy: f32, inner: f32, outer: f32, a0: f32, a1: f32) -> Contour {
    let n = (((a1 - a0).abs() * outer * 0.8) as usize).clamp(6, 64);
    let mut out = Vec::with_capacity(2 * n + 2);
    for i in 0..=n {
        let t = a0 + (a1 - a0) * i as f32 / n as f32;
        out.push((cx + outer * t.cos(), cy + outer * t.sin()));
    }
    for i in (0..=n).rev() {
        let t = a0 + (a1 - a0) * i as f32 / n as f32;
        out.push((cx + inner * t.cos(), cy + inner * t.sin()));
    }
    solid(out)
}

/// A ring cut into `dashes` dashes, each `share` of its step long.
fn dashed_ring(inner: f32, outer: f32, dashes: usize, share: f32) -> Vec<Contour> {
    let step = TAU / dashes as f32;
    (0..dashes)
        .map(|i| {
            let a = i as f32 * step - PI / 2.0;
            arc_band(0.0, 0.0, inner, outer, a - step * share / 2.0, a + step * share / 2.0)
        })
        .collect()
}

/// A line of `width` along `points`, with round joins and ends, as outlines
/// that union under the non-zero rule.
fn stroke(points: &[(f32, f32)], width: f32) -> Vec<Contour> {
    let half = width * 0.5;
    let mut out = Vec::with_capacity(points.len() * 2);
    for pair in points.windows(2) {
        let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len = (dx * dx + dy * dy).sqrt();
        if len <= 1e-4 {
            continue;
        }
        let (nx, ny) = (-dy / len * half, dx / len * half);
        out.push(solid(vec![
            (x0 + nx, y0 + ny),
            (x1 + nx, y1 + ny),
            (x1 - nx, y1 - ny),
            (x0 - nx, y0 - ny),
        ]));
    }
    // Round joins, which also round the two ends. Only worth it once the line
    // is wide enough for a corner to show.
    if half > 0.6 {
        for &(x, y) in points {
            out.push(circle(x, y, half));
        }
    }
    out
}

/// Points along a quadratic Bézier curve.
fn bezier(a: (f32, f32), b: (f32, f32), c: (f32, f32), n: usize) -> Vec<(f32, f32)> {
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let u = 1.0 - t;
            (
                u * u * a.0 + 2.0 * u * t * b.0 + t * t * c.0,
                u * u * a.1 + 2.0 * u * t * b.1 + t * t * c.1,
            )
        })
        .collect()
}

/// A spiral curling in from `(x, y)`: it starts heading along `heading`,
/// turns `turns` times round with its radius shrinking from `radius`, the way
/// a tendril does. `sense` is 1 to curl clockwise and -1 against.
fn curl(x: f32, y: f32, heading: f32, radius: f32, turns: f32, sense: f32) -> Vec<(f32, f32)> {
    let n = ((turns * radius * 2.0) as usize).clamp(12, 120);
    // The centre is a radius to the side of the start, so the curve sets
    // off along `heading`.
    let side = heading + sense * PI / 2.0;
    let (cx, cy) = (x + radius * side.cos(), y + radius * side.sin());
    let start = side + PI;
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let r = radius * (1.0 - 0.75 * t);
            let a = start + sense * t * turns * TAU;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

/// Scale by `scale`, turn by `turn` and move to `(x, y)`.
fn place(contours: &[Contour], x: f32, y: f32, scale: f32, turn: f32) -> Vec<Contour> {
    let (s, c) = turn.sin_cos();
    contours
        .iter()
        .map(|contour| {
            contour
                .iter()
                .map(|&(px, py)| {
                    let (px, py) = (px * scale, py * scale);
                    (x + px * c - py * s, y + px * s + py * c)
                })
                .collect()
        })
        .collect()
}

// ---------------------------------------------------------------- flowers --

/// Flower `index`, 1 to 22, about the origin with a radius of 1.
fn flower_shape(index: u32) -> Vec<Contour> {
    let around = |n: usize, f: &dyn Fn(f32) -> Contour| -> Vec<Contour> {
        (0..n).map(|i| f(i as f32 / n as f32 * TAU - PI / 2.0)).collect()
    };
    match index {
        1 => vec![circle(0.0, 0.0, 0.35)],
        2 => vec![circle(0.0, 0.0, 1.0)],
        3 => {
            let mut out = vec![circle(0.0, 0.0, 0.3)];
            out.extend(around(5, &|a| petal(a, 0.42, 1.0, 0.62, 0.75)));
            out
        }
        4 => {
            let mut out = vec![circle(0.0, 0.0, 0.2)];
            out.extend(around(8, &|a| petal(a, 0.3, 1.0, 0.26, 1.0)));
            out
        }
        5 => {
            let mut out = vec![circle(0.0, 0.0, 0.6)];
            out.extend(dashed_ring(0.78, 0.95, 8, 0.7));
            out
        }
        6 => {
            // A disc with a hole and eight swirling points.
            let mut out = vec![circle(0.0, 0.0, 0.6), hole(circle(0.0, 0.0, 0.28))];
            out.extend(around(8, &|a| {
                solid(vec![
                    (0.55 * (a - 0.3).cos(), 0.55 * (a - 0.3).sin()),
                    (0.55 * (a + 0.3).cos(), 0.55 * (a + 0.3).sin()),
                    ((a + 0.45).cos(), (a + 0.45).sin()),
                ])
            }));
            out
        }
        7 => around(4, &|a| petal(a, 0.12, 1.0, 0.84, 0.6)),
        8 => {
            let mut out = vec![circle(0.0, 0.0, 0.2)];
            for i in 0..8 {
                let a = i as f32 / 8.0 * TAU;
                out.push(circle(0.42 * a.cos(), 0.42 * a.sin(), 0.08));
                out.push(petal(a, 0.55, 1.0, 0.14, 1.0));
                out.push(petal(a + PI / 8.0, 0.55, 0.78, 0.12, 1.0));
            }
            out
        }
        9 => {
            // A rose: five rounded lobes, with the folds of the petals cut
            // out of it in arcs spiralling in.
            let mut out = vec![polar(80, |t| 0.8 + 0.14 * (2.5 * t).cos().abs())];
            for (r, a0, span) in
                [(0.64, 0.3, 2.4), (0.64, 3.4, 2.2), (0.46, 1.6, 2.6), (0.46, 4.6, 2.0), (0.27, 0.8, 3.4)]
            {
                out.push(hole(arc_band(0.0, 0.0, r - 0.04, r + 0.04, a0, a0 + span)));
            }
            out
        }
        10 => {
            let mut out = vec![circle(0.0, 0.0, 0.2), hole(circle(0.0, 0.0, 0.1))];
            out.extend(around(16, &|a| petal(a, 0.16, 1.0, 0.11, 1.0)));
            out
        }
        11 => {
            let mut out = vec![circle(0.0, 0.0, 0.42)];
            out.extend(around(8, &|a| {
                solid(vec![
                    (0.52 * (a - 0.3).cos(), 0.52 * (a - 0.3).sin()),
                    (0.52 * (a + 0.3).cos(), 0.52 * (a + 0.3).sin()),
                    (a.cos(), a.sin()),
                ])
            }));
            out
        }
        12 => {
            let mut out = vec![circle(0.0, 0.0, 0.5)];
            out.extend(dashed_ring(0.68, 0.95, 10, 0.62));
            out
        }
        13 => {
            // A cross with its arms curling back at the ends.
            let mut out = vec![rect(-0.08, -1.0, 0.08, 1.0), rect(-1.0, -0.08, 1.0, 0.08)];
            for (sx, sy) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
                out.push(arc_band(0.45 * sx, 0.45 * sy, 0.2, 0.3, 0.0, TAU * 0.7));
            }
            out
        }
        14 => vec![polar(96, |t| {
            // Four points, the upright ones longer: an astroid.
            let (c, s) = (t.cos(), t.sin());
            let r = (c.abs().powf(2.0 / 3.0) / 0.7f32.powf(2.0 / 3.0) + s.abs().powf(2.0 / 3.0))
                .powf(-1.5);
            r.max(0.06)
        })],
        15 => {
            let mut out = vec![circle(0.0, 0.0, 0.12)];
            for i in 0..8 {
                let a = i as f32 / 8.0 * TAU - PI / 2.0;
                let length = if i % 2 == 0 { 1.0 } else { 0.75 };
                out.push(petal(a, 0.0, length, 0.14, 1.0));
            }
            out
        }
        16 => vec![
            polar(96, |t| 0.86 + 0.14 * (4.0 * t).cos().abs()),
            hole(circle(0.0, 0.0, 0.72)),
            star(5, 0.55, 0.22),
        ],
        17 => around(4, &|a| petal(a, 0.0, 1.0, 0.8, 1.6)),
        18 => vec![rect(-0.25, -1.0, 0.25, 1.0), rect(-1.0, -0.25, 1.0, 0.25)],
        19 => vec![heart()],
        20 => vec![star(5, 1.0, 0.4)],
        21 => {
            let mut out = vec![polar(6, |_| 0.2)];
            for i in 0..6 {
                let a = i as f32 / 6.0 * TAU - PI / 2.0;
                let (c, s) = (a.cos(), a.sin());
                out.extend(stroke(&[(0.0, 0.0), (0.95 * c, 0.95 * s)], 0.1));
                for (at, reach) in [(0.45, 0.28), (0.7, 0.22)] {
                    let (bx, by) = (at * c, at * s);
                    for side in [-0.8f32, 0.8] {
                        let b = a + side;
                        out.extend(stroke(&[(bx, by), (bx + reach * b.cos(), by + reach * b.sin())], 0.08));
                    }
                }
            }
            out
        }
        22 => vec![
            ellipse(0.0, 0.42, 0.45, 0.38, 0.0),
            ellipse(-0.62, -0.12, 0.15, 0.21, -0.4),
            ellipse(-0.24, -0.55, 0.16, 0.22, -0.12),
            ellipse(0.24, -0.55, 0.16, 0.22, 0.12),
            ellipse(0.62, -0.12, 0.15, 0.21, 0.4),
        ],
        _ => Vec::new(),
    }
}

fn heart() -> Contour {
    solid(
        (0..64)
            .map(|i| {
                let t = i as f32 / 64.0 * TAU;
                let x = 16.0 * t.sin().powi(3);
                let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
                (x / 17.0, -y / 17.0 - 0.1)
            })
            .collect(),
    )
}

// ----------------------------------------------------------------- leaves --

/// A closed curve from the top point round to the bottom and back, with
/// `half(t)` the half-width at each height, `t` from 0 at the top to 1.
fn profile(half: impl Fn(f32) -> f32) -> Contour {
    let n = 40;
    let mut out = Vec::with_capacity(2 * n);
    // `max` also turns the NaN a fractional power of sin(π)'s rounding
    // error gives back into the zero it should be.
    let half = |t: f32| half(t).max(0.0);
    for i in 0..=n {
        let t = i as f32 / n as f32;
        out.push((half(t), -1.0 + 2.0 * t));
    }
    for i in (1..n).rev() {
        let t = i as f32 / n as f32;
        out.push((-half(t), -1.0 + 2.0 * t));
    }
    solid(out)
}

/// Leaf `index`, 1 to 23, as CS6's list draws it: hanging from its stalk at
/// (0, -1) and reaching down to y = 1.
fn leaf_shape(index: u32) -> Vec<Contour> {
    let mirrored = |half: &[(f32, f32)]| -> Contour {
        let mut out: Vec<(f32, f32)> = half.to_vec();
        out.extend(half.iter().rev().filter(|p| p.0 != 0.0).map(|&(x, y)| (-x, y)));
        solid(out)
    };
    match index {
        1 => vec![circle(0.0, 0.0, 0.95)],
        2 => vec![rect(-0.9, -0.9, 0.9, 0.9)],
        3 => vec![profile(|t| 0.8 * (PI * t).sin() * (PI * t / 2.0).sin())],
        4 => vec![profile(|t| 0.5 * (PI * t).sin().powf(1.4))],
        5 => vec![profile(|t| 0.7 * (PI * t).sin() * (PI * (1.0 - t) / 2.0).sin())],
        6 => vec![profile(|t| 0.62 * (PI * t).sin() * (PI * t / 2.0).sin().powf(1.3))],
        7 => vec![mirrored(&[(0.0, -1.0), (0.3, -0.35), (0.75, 0.55), (0.25, 0.35), (0.0, 1.0)])],
        8 => vec![
            circle(0.0, -0.25, 0.55),
            circle(-0.55, -0.15, 0.42),
            circle(0.55, -0.15, 0.42),
            circle(0.0, 0.55, 0.3),
        ],
        9 => vec![mirrored(&[(0.0, -1.0), (0.8, -0.3), (0.22, -0.35), (0.16, 0.2), (0.0, 1.0)])],
        10 => {
            let mut half: Vec<(f32, f32)> = vec![(0.0, -0.9)];
            for i in 1..=10 {
                let t = i as f32 / 10.0;
                half.push((0.9 * t * t, -0.9 + 1.4 * t));
            }
            for i in 1..=8 {
                let t = i as f32 / 8.0;
                half.push((0.9 * (1.0 - t), 0.5 + 0.3 * (PI * t / 2.0).sin()));
            }
            half.pop();
            half.push((0.0, 0.8));
            vec![mirrored(&half)]
        }
        11 => vec![solid(vec![(0.0, -0.8), (0.85, 0.8), (-0.85, 0.8)])],
        12 => vec![mirrored(&[(0.0, -1.0), (0.85, -0.05), (0.3, 0.1), (0.0, 0.9)])],
        13 => vec![polar(96, |t| {
            let (c, s) = (t.cos(), t.sin());
            (c.abs().powf(2.0 / 3.0) / 0.55f32.powf(2.0 / 3.0) + s.abs().powf(2.0 / 3.0))
                .powf(-1.5)
                .max(0.05)
        })],
        14 => vec![ellipse(0.0, 0.0, 0.42, 0.95, 0.0)],
        15 => vec![solid(vec![(-0.35, -0.6), (0.35, -0.6), (0.95, 0.6), (-0.95, 0.6)])],
        16 => vec![rect(-0.08, -1.0, 0.08, 0.3), circle(0.0, 0.55, 0.45)],
        17 => vec![profile(|t| 0.75 * (PI * t).sin().powf(0.7) * (1.0 - 0.3 * (1.0 - t)))],
        18 => vec![polar(96, |t| {
            let (c, s) = (t.cos(), t.sin());
            (c.abs().powf(2.0 / 3.0) + s.abs().powf(2.0 / 3.0)).powf(-1.5).max(0.08) * 0.95
        })],
        19 => vec![
            rect(-0.12, -1.0, 0.12, -0.2),
            solid(vec![(-0.12, -0.35), (0.12, -0.35), (0.42, 0.25), (-0.42, 0.25)]),
            circle(0.0, 0.45, 0.5),
        ],
        20 => vec![rect(-0.2, -0.9, 0.2, 0.9), rect(-0.9, -0.2, 0.9, 0.2)],
        21 => vec![heart()],
        22 => vec![rect(-0.18, -1.0, 0.18, 0.55), rect(-0.9, 0.5, 0.9, 0.9)],
        23 => vec![star(5, 1.0, 0.4)],
        _ => Vec::new(),
    }
}

// ------------------------------------------------------------------ track --

/// The rounded rectangle the motifs are laid along, measured clockwise from
/// the top left.
#[derive(Clone, Copy)]
struct Track {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    radius: f32,
}

impl Track {
    fn sides(&self) -> (f32, f32) {
        ((self.x1 - self.x0 - 2.0 * self.radius).max(0.0), (self.y1 - self.y0 - 2.0 * self.radius).max(0.0))
    }

    fn length(&self) -> f32 {
        let (w, h) = self.sides();
        2.0 * (w + h) + TAU * self.radius
    }

    /// Where `s` along the track is, and which way it runs there.
    fn at(&self, s: f32) -> ((f32, f32), (f32, f32)) {
        let (w, h) = self.sides();
        let r = self.radius;
        let quarter = PI / 2.0 * r;
        let mut s = s.rem_euclid(self.length());
        let (x0, y0, x1, y1) = (self.x0, self.y0, self.x1, self.y1);
        // Each side, then the corner after it.
        let pieces: [((f32, f32), (f32, f32), f32, (f32, f32), f32); 4] = [
            ((x0 + r, y0), (1.0, 0.0), w, (x1 - r, y0 + r), -PI / 2.0),
            ((x1, y0 + r), (0.0, 1.0), h, (x1 - r, y1 - r), 0.0),
            ((x1 - r, y1), (-1.0, 0.0), w, (x0 + r, y1 - r), PI / 2.0),
            ((x0, y1 - r), (0.0, -1.0), h, (x0 + r, y0 + r), PI),
        ];
        for (start, dir, len, centre, a0) in pieces {
            if s <= len {
                return ((start.0 + dir.0 * s, start.1 + dir.1 * s), dir);
            }
            s -= len;
            if s <= quarter && r > 0.0 {
                let a = a0 + s / r;
                return ((centre.0 + r * a.cos(), centre.1 + r * a.sin()), (-a.sin(), a.cos()));
            }
            s -= quarter;
        }
        ((x0 + r, y0), (1.0, 0.0))
    }

    /// Positions spread evenly round the track, about `spacing` apart, the
    /// first half a step in so the corners come out alike.
    fn spread(&self, spacing: f32) -> Vec<f32> {
        let n = (self.length() / spacing.max(1.0)).round().max(4.0) as usize;
        let step = self.length() / n as f32;
        (0..n).map(|i| (i as f32 + 0.5) * step).collect()
    }
}

/// A small, fast, seeded generator: the arrangement is a seed, and the same
/// seed has to give the same frame every time or undo could not replay it.
struct Rng(u64);

impl Rng {
    fn new(seed: u32, frame: u32) -> Rng {
        Rng(0x9E37_79B9_7F4A_7C15 ^ ((seed as u64) << 32 | frame as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9))
    }

    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }
}

// ----------------------------------------------------------------- frames --

/// How big everything is, in pixels, worked out from the canvas and the
/// sliders. Everything scales with the canvas's shorter side, so a frame
/// sits the same on a thumbnail as on a poster.
#[derive(Clone, Copy)]
struct Scale {
    /// How far in from the canvas edge the frame runs.
    inset: f32,
    /// A motif's reach: a pinwheel's radius, a spray's spread.
    unit: f32,
    /// A flower's radius.
    flower: f32,
    /// A leaf's length.
    leaf: f32,
    /// The width of a vine.
    vine: f32,
    /// Thickness as a multiple of CS6's default.
    thickness: f32,
}

/// Margin, per step, as a share of the canvas's shorter side; and the base
/// it starts from. Fitted to CS6's own preview, where Margin 14 puts the
/// pinwheels' centres 7% of the way in.
const MARGIN_BASE: f32 = 0.015;
const MARGIN_PER_STEP: f32 = 0.004;

/// A motif's reach, a flower's radius and a leaf's length, each at 100 on its
/// slider, as a share of the canvas's shorter side. At CS6's defaults of 20 a
/// pinwheel spans about a seventh of the side, and its flowers are dots a
/// fortieth across.
const UNIT_AT_FULL: f32 = 0.36;
const FLOWER_AT_FULL: f32 = 0.065;
const LEAF_AT_FULL: f32 = 0.1;

/// A vine's width as a share of a motif's reach, at CS6's default Thickness.
const VINE_SHARE: f32 = 0.045;
const THICKNESS_DEFAULT: f32 = 29.0;

fn scale_for(options: &FrameOptions, width: u32, height: u32) -> Scale {
    let side = width.min(height) as f32;
    let size = options.size.clamp(*SIZE.start(), *SIZE.end()) as f32 / 100.0;
    let unit = side * size * UNIT_AT_FULL;
    let thickness =
        options.thickness.clamp(*THICKNESS.start(), *THICKNESS.end()) as f32 / THICKNESS_DEFAULT;
    Scale {
        inset: side * (MARGIN_BASE + MARGIN_PER_STEP * options.margin.min(*MARGIN.end()) as f32),
        unit,
        flower: side * options.flower_size.clamp(*FLOWER_SIZE.start(), *FLOWER_SIZE.end()) as f32 / 100.0
            * FLOWER_AT_FULL,
        leaf: side * options.leaf_size.clamp(*LEAF_SIZE.start(), *LEAF_SIZE.end()) as f32 / 100.0
            * LEAF_AT_FULL,
        vine: (unit * VINE_SHARE * thickness).max(0.8),
        thickness,
    }
}

/// What a motif frame draws, collected by colour so each colour goes down in
/// one pass: vines under leaves under flowers, as CS6 layers them.
#[derive(Default)]
struct Layers {
    vine: Vec<Contour>,
    leaves: Vec<Contour>,
    flowers: Vec<Contour>,
    /// Flowers laid down half see-through, for the frames CS6 draws soft.
    soft: Vec<Contour>,
    /// The Advanced tab's Angle, added to every flower and leaf.
    turn: f32,
}

impl Layers {
    fn flower(&mut self, shape: &[Contour], x: f32, y: f32, radius: f32, turn: f32) {
        self.flowers.extend(place(shape, x, y, radius, turn + self.turn));
    }

    fn soft_flower(&mut self, shape: &[Contour], x: f32, y: f32, radius: f32, turn: f32) {
        self.soft.extend(place(shape, x, y, radius, turn + self.turn));
    }

    /// A leaf with its stalk at `(x, y)`, reaching `length` towards `angle`.
    fn leaf(&mut self, shape: &[Contour], x: f32, y: f32, length: f32, angle: f32) {
        // The shape hangs from (0, -1) down to (0, 1): turn +y onto the
        // angle, and put the middle half a length out so the stalk lands on
        // the point.
        let half = length * 0.5;
        let angle = angle + self.turn;
        let (cx, cy) = (x + half * angle.cos(), y + half * angle.sin());
        self.leaves.extend(place(shape, cx, cy, half, angle - PI / 2.0));
    }

    fn line(&mut self, points: &[(f32, f32)], width: f32) {
        // Never finer than a pixel: thinner, a stem breaks up into dots.
        self.vine.extend(stroke(points, width.max(1.0)));
    }
}

/// Filter ▸ Render ▸ Picture Frame.
///
/// The frame is drawn on a clear sheet first and laid over the picture after,
/// so that Fade can thin it as a whole: faded shape by shape, two flowers
/// overlapping would show darker where they cross.
pub fn picture_frame(pixmap: &mut Pixmap, options: &FrameOptions) {
    if pixmap.is_empty() {
        return;
    }
    let mut sheet = Pixmap::new(pixmap.width(), pixmap.height());
    draw_frame(&mut sheet, options);

    let fade = options.fade.min(*FADE.end()) as f32 / 100.0;
    let (w, h) = (pixmap.width() as f32, pixmap.height() as f32);
    let width = pixmap.width() as usize;
    let sheet = &sheet;
    let stride = pixmap.stride();
    use rayon::prelude::*;
    pixmap.as_bytes_mut().par_chunks_exact_mut(stride).enumerate().for_each(|(y, out)| {
        let py = y as f32 + 0.5;
        for (x, px) in out.chunks_exact_mut(4).take(width).enumerate() {
            let top = sheet.get(x as i32, y as i32);
            if top.a == 0 {
                continue;
            }
            // How far along its side from the nearer corner to the middle a
            // pixel is, 0 at a corner and 1 half way: by the nearer edge, so
            // a pixel on the top side is measured across and one on the left
            // side down.
            let px_ = x as f32 + 0.5;
            let (ex, ey) = (px_.min(w - px_), py.min(h - py));
            let along = if ey < ex { ex / (w / 2.0) } else { ey / (h / 2.0) };
            let alpha = 1.0 - fade * along.clamp(0.0, 1.0);
            let dst = Rgba8::new(px[0], px[1], px[2], px[3]);
            let out = crate::brush::source_over(dst, top, alpha);
            px.copy_from_slice(&[out.r, out.g, out.b, out.a]);
        }
    });
}

fn draw_frame(pixmap: &mut Pixmap, options: &FrameOptions) {
    let frame = options.frame.clamp(*FRAMES.start(), *FRAMES.end());
    let (width, height) = (pixmap.width(), pixmap.height());
    let scale = scale_for(options, width, height);
    if frame >= 36 {
        moulding(pixmap, frame, options.vine, &scale, options.invert);
        return;
    }

    let flower = flower_shape(options.flower);
    let leaf = leaf_shape(options.leaf);
    let mut rng = Rng::new(options.arrangement, frame);
    let mut layers = Layers {
        turn: options.angle.min(*ANGLE.end()) as f32 * PI / 180.0,
        ..Layers::default()
    };
    let (w, h) = (width as f32, height as f32);
    let inset = scale.inset;
    let track = |radius: f32| Track {
        x0: inset,
        y0: inset,
        x1: w - inset,
        y1: h - inset,
        radius: radius.min((w.min(h) * 0.5 - inset).max(0.0)),
    };
    let centre = (w / 2.0, h / 2.0);
    let (u, f, l, v) = (scale.unit, scale.flower, scale.leaf, scale.vine);
    // The inward normal of a track running clockwise — or, inverted, the
    // outward one, which is all Invert does to a motif frame: whatever leant
    // in towards the picture leans out towards the edge.
    let flip = if options.invert { -1.0 } else { 1.0 };
    let inward = |t: (f32, f32)| (-t.1 * flip, t.0 * flip);
    // Number of Lines as a density against CS6's default of 15.
    let density = options.lines.clamp(*LINES.start(), *LINES.end()) as f32 / 15.0;
    // Angle as a lean, for the slanted line frames.
    let lean = layers.turn;
    // A vine waving `amplitude` either side of the track, a full wave every
    // `wave` along it: the points, and where each crest falls.
    let wavy = |path: &Track, wave: f32, amplitude: f32, phase: f32| -> Vec<(f32, f32)> {
        let n = (path.length() / wave).round().max(4.0);
        let wave = path.length() / n;
        let steps = (path.length() / 1.5) as usize;
        (0..=steps)
            .map(|i| {
                let s = i as f32 / steps as f32 * path.length();
                let (p, t) = path.at(s);
                let (nx, ny) = inward(t);
                let off = amplitude * (TAU * s / wave + phase).sin();
                (p.0 + nx * off, p.1 + ny * off)
            })
            .collect()
    };
    // The same wave's crests, with which way the vine runs there.
    let crests = |path: &Track, wave: f32, phase: f32| -> Vec<(f32, bool)> {
        let n = (path.length() / wave).round().max(4.0) as usize;
        let wave = path.length() / n as f32;
        let quarter = (0.25 - phase / TAU).rem_euclid(1.0);
        (0..n * 2)
            .map(|k| ((k as f32 * 0.5 + quarter) * wave, k % 2 == 0))
            .collect()
    };

    match frame {
        // Happy Vine: a thin vine waving along the edge, a tendril curling
        // off every swing, and flowers on the crests.
        1 => {
            let path = track(u * 0.6);
            let (wave, amp) = (u * 1.6, u * 0.14);
            layers.line(&wavy(&path, wave, amp, 0.0), v * 0.7);
            for (s, out) in crests(&path, wave, 0.0) {
                let (p, t) = path.at(s);
                let (nx, ny) = inward(t);
                let sign = if out { 1.0 } else { -1.0 };
                let (x, y) = (p.0 + nx * amp * sign, p.1 + ny * amp * sign);
                let heading = t.1.atan2(t.0) + sign * 0.9;
                layers.line(&curl(x, y, heading, u * 0.09, 1.2, -sign), v * 0.5);
                layers.flower(&flower, x, y, f * rng.range(0.6, 1.2), rng.range(0.0, TAU));
                if rng.next() < 0.5 {
                    let off = rng.range(0.25, 0.4) * u * sign;
                    layers.flower(&flower, p.0 + nx * off, p.1 + ny * off, f * 0.45, 0.0);
                }
            }
        }
        // Pretty Vine: a faint vine under loose clusters of flowers, some
        // solid and some see-through, of every size.
        2 => {
            let path = track(u * 0.6);
            layers.line(&wavy(&path, u * 2.0, u * 0.08, 0.0), v * 0.45);
            for s in path.spread(u * 0.8) {
                for _ in 0..(2 + (rng.next() * 3.0) as usize) {
                    let (p, t) = path.at(s + rng.range(-0.35, 0.35) * u);
                    let (nx, ny) = inward(t);
                    let off = rng.range(-0.25, 0.25) * u;
                    let (x, y) = (p.0 + nx * off, p.1 + ny * off);
                    let size = f * rng.range(0.4, 1.3);
                    if rng.next() < 0.45 {
                        layers.soft_flower(&flower, x, y, size, rng.range(0.0, TAU));
                    } else {
                        layers.flower(&flower, x, y, size, rng.range(0.0, TAU));
                    }
                }
            }
        }
        // Smoke Signals: flowers along the edge, each sending up a wisp that
        // winds away and ends in a curl.
        3 => {
            let path = track(u * 0.5);
            for s in path.spread(u * 1.1) {
                let (p, t) = path.at(s);
                let (nx, ny) = inward(t);
                let rise = u * rng.range(0.6, 0.9);
                let top = (p.0 + nx * rise + t.0 * rise * 0.3, p.1 + ny * rise + t.1 * rise * 0.3);
                let bend = (p.0 + nx * rise * 0.5 - t.0 * rise * 0.4, p.1 + ny * rise * 0.5 - t.1 * rise * 0.4);
                let mut wisp = bezier(p, bend, top, 14);
                let heading = (top.1 - bend.1).atan2(top.0 - bend.0);
                wisp.extend(curl(top.0, top.1, heading, u * 0.08, 1.1, 1.0).into_iter().skip(1));
                layers.line(&wisp, v * 0.5);
                layers.flower(&flower, p.0, p.1, f * rng.range(1.0, 1.4), rng.range(0.0, TAU));
                layers.flower(&flower, bend.0, bend.1, f * 0.45, 0.0);
            }
        }
        // Party: bunches of flowers with curled streamers flying off them.
        4 => {
            let path = track(u * 0.5);
            for s in path.spread(u * 1.4) {
                let (p, t) = path.at(s);
                let (nx, ny) = inward(t);
                for _ in 0..3 {
                    let a = rng.range(0.0, TAU);
                    let reach = u * rng.range(0.2, 0.4);
                    let end = (p.0 + reach * a.cos(), p.1 + reach * a.sin());
                    let mut streamer = vec![p, end];
                    streamer.extend(curl(end.0, end.1, a, u * 0.07, 1.0, if rng.next() < 0.5 { 1.0 } else { -1.0 }).into_iter().skip(1));
                    layers.line(&streamer, v * 0.45);
                }
                for _ in 0..(3 + (rng.next() * 3.0) as usize) {
                    let (ox, oy) = (rng.range(-0.3, 0.3) * u, rng.range(-0.25, 0.25) * u);
                    layers.flower(&flower, p.0 + t.0 * ox + nx * oy, p.1 + t.1 * ox + ny * oy, f * rng.range(0.5, 1.2), rng.range(0.0, TAU));
                }
            }
        }
        // Big Curls: a vine swinging wide, looping at every crest, with leaves
        // down its length and a flower in each loop.
        5 => {
            let path = track(u * 0.6);
            let (wave, amp) = (u * 1.8, u * 0.2);
            layers.line(&wavy(&path, wave, amp, 0.0), v);
            for (s, out) in crests(&path, wave, 0.0) {
                let (p, t) = path.at(s);
                let (nx, ny) = inward(t);
                let sign = if out { 1.0 } else { -1.0 };
                let (x, y) = (p.0 + nx * amp * sign, p.1 + ny * amp * sign);
                let heading = t.1.atan2(t.0);
                let loop_ = curl(x, y, heading, u * 0.16, 0.9, -sign);
                let centre = loop_[loop_.len() / 2];
                layers.line(&loop_, v * 0.8);
                layers.flower(&flower, centre.0, centre.1, f, rng.range(0.0, TAU));
            }
            for (k, s) in path.spread(u * 0.45).into_iter().enumerate() {
                let (p, t) = path.at(s);
                let (nx, ny) = inward(t);
                let off = amp * (TAU * s / wave).sin();
                let along = t.1.atan2(t.0);
                let side = if k % 2 == 0 { 1.0 } else { -1.0 };
                layers.leaf(&leaf, p.0 + nx * off, p.1 + ny * off, l, along + side * 1.0);
            }
        }
        // Tilde: little "~" strokes one after another, each with a flower on
        // its end.
        6 => {
            let path = track(u * 0.5);
            for s in path.spread(u * 1.2) {
                let (p, t) = path.at(s);
                let (nx, ny) = inward(t);
                let length = u * 0.8;
                let amp = u * 0.1;
                let points: Vec<(f32, f32)> = (0..=24)
                    .map(|i| {
                        let q = i as f32 / 24.0;
                        let a = (q - 0.5) * length;
                        let o = amp * (TAU * q).sin();
                        (p.0 + t.0 * a + nx * o, p.1 + t.1 * a + ny * o)
                    })
                    .collect();
                layers.line(&points, v * 0.6);
                let end = points[points.len() - 1];
                layers.flower(&flower, end.0, end.1, f, 0.0);
            }
        }
        // Romance: flowers in pairs and threes, each hanging from a curl like
        // a question mark.
        7 => {
            let path = track(u * 0.5);
            for s in path.spread(u * 0.9) {
                let (p, t) = path.at(s + rng.range(-0.2, 0.2) * u);
                let (nx, ny) = inward(t);
                let off = rng.range(-0.2, 0.2) * u;
                let base = (p.0 + nx * off, p.1 + ny * off);
                let up = (nx, ny);
                let reach = u * rng.range(0.25, 0.4);
                let top = (base.0 + up.0 * reach, base.1 + up.1 * reach);
                let mut stem = vec![base, top];
                stem.extend(curl(top.0, top.1, up.1.atan2(up.0), u * 0.08, 0.8, if rng.next() < 0.5 { 1.0 } else { -1.0 }).into_iter().skip(1));
                layers.line(&stem, v * 0.5);
                layers.flower(&flower, base.0, base.1, f * rng.range(0.9, 1.3), rng.range(0.0, TAU));
                if rng.next() < 0.6 {
                    let a = rng.range(0.0, TAU);
                    layers.flower(&flower, base.0 + a.cos() * f * 2.0, base.1 + a.sin() * f * 2.0, f * 0.6, 0.0);
                }
            }
        }
        // Curly Dance: a vine of arches, looping at the foot of each, a
        // flower on every arch.
        8 => {
            let path = track(u * 0.6);
            let arch = u * 1.1;
            let n = (path.length() / arch).round().max(4.0);
            let arch = path.length() / n;
            let height = u * 0.25;
            let steps = (path.length() / 1.5) as usize;
            let points: Vec<(f32, f32)> = (0..=steps)
                .map(|i| {
                    let s = i as f32 / steps as f32 * path.length();
                    let (p, t) = path.at(s);
                    let (nx, ny) = inward(t);
                    let off = -height * (PI * s / arch).sin().abs() + height * 0.5;
                    (p.0 + nx * off, p.1 + ny * off)
                })
                .collect();
            layers.line(&points, v * 0.6);
            for k in 0..n as usize {
                let (p, t) = path.at(k as f32 * arch);
                let (nx, ny) = inward(t);
                let foot = (p.0 + nx * height * 0.5, p.1 + ny * height * 0.5);
                layers.line(&curl(foot.0, foot.1, t.1.atan2(t.0), u * 0.08, 1.0, 1.0), v * 0.5);
                let (p, t) = path.at((k as f32 + 0.5) * arch);
                let (nx, ny) = inward(t);
                layers.flower(&flower, p.0 - nx * height * 0.5, p.1 - ny * height * 0.5, f * rng.range(1.0, 1.4), rng.range(0.0, TAU));
            }
        }
        // Wisps: two fine strands twisting round each other, small flowers
        // where they cross.
        9 => {
            let path = track(u * 0.6);
            let (wave, amp) = (u * 1.4, u * 0.12);
            layers.line(&wavy(&path, wave, amp, 0.0), v * 0.4);
            layers.line(&wavy(&path, wave, amp, PI), v * 0.4);
            for (k, s) in path.spread(wave * 0.5).into_iter().enumerate() {
                let (p, _) = path.at(s - wave * 0.25);
                layers.flower(&flower, p.0, p.1, f * if k % 2 == 0 { 0.9 } else { 0.55 }, 0.0);
            }
        }
        // Spring Weed: tufts of fine blades springing up off the edge, some
        // tipped with a flower.
        10 => {
            let path = track(u * 0.4);
            for s in path.spread(u * 0.6) {
                let (p, t) = path.at(s + rng.range(-0.15, 0.15) * u);
                let (nx, ny) = inward(t);
                let blades = 3 + (rng.next() * 3.0) as usize;
                for b in 0..blades {
                    let lean = (b as f32 / (blades - 1) as f32 - 0.5) * 1.4 + rng.range(-0.2, 0.2);
                    let (c, sn) = (lean.cos(), lean.sin());
                    let dir = (nx * c - ny * sn, nx * sn + ny * c);
                    let reach = u * rng.range(0.2, 0.42);
                    let tip = (p.0 + dir.0 * reach, p.1 + dir.1 * reach);
                    let bend = (p.0 + dir.0 * reach * 0.5 + t.0 * reach * 0.2 * lean.signum(), p.1 + dir.1 * reach * 0.5 + t.1 * reach * 0.2 * lean.signum());
                    layers.line(&bezier(p, bend, tip, 10), v * 0.45);
                    if rng.next() < 0.45 {
                        layers.flower(&flower, tip.0, tip.1, f * rng.range(0.5, 0.9), rng.range(0.0, TAU));
                    }
                }
            }
        }
        // Eyelash: curved sprays of leaves, each rooted in a knot of flowers,
        // curving one way and then the other.
        11 => {
            let path = track(u * 0.5);
            for (k, s) in path.spread(u * 1.3).into_iter().enumerate() {
                let (p, t) = path.at(s);
                let (nx, ny) = inward(t);
                let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
                let reach = u * 0.55;
                let tip = (p.0 + t.0 * reach * sign + nx * reach * 0.2, p.1 + t.1 * reach * sign + ny * reach * 0.2);
                let ctrl = (p.0 + t.0 * reach * 0.5 * sign + nx * reach * 0.55, p.1 + t.1 * reach * 0.5 * sign + ny * reach * 0.55);
                let stem = bezier(p, ctrl, tip, 16);
                layers.line(&stem, v * 0.5);
                for i in (3..stem.len()).step_by(3) {
                    let (a, b) = (stem[i - 1], stem[i]);
                    let along = (b.1 - a.1).atan2(b.0 - a.0);
                    layers.leaf(&leaf, b.0, b.1, l * 0.8, along - sign * 0.8);
                }
                for _ in 0..3 {
                    let (ox, oy) = (rng.range(-0.12, 0.12) * u, rng.range(-0.1, 0.1) * u);
                    layers.flower(&flower, p.0 + ox, p.1 + oy, f * rng.range(0.7, 1.1), rng.range(0.0, TAU));
                }
            }
        }
        // Magic Smoke: short wavering stems, each with a pair of leaves and a
        // few flowers at its top.
        12 => {
            let path = track(u * 0.5);
            for s in path.spread(u * 1.1) {
                let (p, t) = path.at(s);
                let (nx, ny) = inward(t);
                let reach = u * 0.4;
                let lean = rng.range(-0.5, 0.5);
                let top = (p.0 + nx * reach + t.0 * reach * lean, p.1 + ny * reach + t.1 * reach * lean);
                let ctrl = (p.0 + nx * reach * 0.5 - t.0 * reach * 0.3, p.1 + ny * reach * 0.5 - t.1 * reach * 0.3);
                let stem = bezier(p, ctrl, top, 12);
                layers.line(&stem, v * 0.5);
                let mid = stem[stem.len() / 2];
                let up = ny.atan2(nx);
                layers.leaf(&leaf, mid.0, mid.1, l, up + 0.9);
                layers.leaf(&leaf, mid.0, mid.1, l, up - 0.9);
                for _ in 0..(2 + (rng.next() * 2.0) as usize) {
                    let (ox, oy) = (rng.range(-0.1, 0.1) * u, rng.range(-0.1, 0.1) * u);
                    layers.flower(&flower, top.0 + ox, top.1 + oy, f * rng.range(0.7, 1.1), rng.range(0.0, TAU));
                }
            }
        }
        // Curly Vine: a leafy vine, thick with leaves both sides and curling
        // tendrils, a flower here and there.
        13 => {
            let path = track(u * 0.6);
            let (wave, amp) = (u * 1.5, u * 0.1);
            layers.line(&wavy(&path, wave, amp, 0.0), v);
            for (k, s) in path.spread(l * 0.4).into_iter().enumerate() {
                let (p, t) = path.at(s);
                let (nx, ny) = inward(t);
                let off = amp * (TAU * s / wave).sin();
                let (x, y) = (p.0 + nx * off, p.1 + ny * off);
                let along = t.1.atan2(t.0);
                let side = if k % 2 == 0 { 1.0 } else { -1.0 };
                layers.leaf(&leaf, x, y, l * rng.range(0.7, 1.2), along + side * rng.range(0.5, 1.3));
                if k % 5 == 2 {
                    layers.line(&curl(x, y, along - side * 1.0, u * 0.08, 1.2, side), v * 0.5);
                }
            }
            for s in path.spread(u * 1.2) {
                let (p, _) = path.at(s + rng.range(-0.3, 0.3) * u);
                layers.flower(&flower, p.0, p.1, f * rng.range(0.7, 1.0), rng.range(0.0, TAU));
            }
        }
        // Chainmail: two rows of flowers along the band's edges, laced
        // together by parallel slanting lines.
        14 => {
            let path = track(u * 0.3);
            let half = u * 0.22;
            let slant = 0.5 + lean;
            for s in path.spread(u * 0.2 / density) {
                let (p, t) = path.at(s);
                let (nx, ny) = inward(t);
                let shift = half * slant.tan().clamp(-3.0, 3.0);
                let a = (p.0 - nx * half - t.0 * shift, p.1 - ny * half - t.1 * shift);
                let b = (p.0 + nx * half + t.0 * shift, p.1 + ny * half + t.1 * shift);
                layers.line(&[a, b], v * 0.35);
            }
            for side in [-1.0f32, 1.0] {
                for s in path.spread(u * 0.3) {
                    let (p, t) = path.at(s + rng.range(-0.08, 0.08) * u);
                    let (nx, ny) = inward(t);
                    let off = side * half * 1.1;
                    layers.flower(&flower, p.0 + nx * off, p.1 + ny * off, f * rng.range(0.55, 1.0), rng.range(0.0, TAU));
                }
            }
        }
        // Fun Event: bunches of flowers on curling stems, with a pair of
        // small ones between.
        15 => {
            let path = track(u * 0.5);
            for s in path.spread(u * 1.2) {
                let (p, t) = path.at(s);
                let (nx, ny) = inward(t);
                for k in 0..3 {
                    let a = ny.atan2(nx) + (k as f32 - 1.0) * 0.8 + rng.range(-0.2, 0.2);
                    let reach = u * rng.range(0.2, 0.35);
                    let tip = (p.0 + reach * a.cos(), p.1 + reach * a.sin());
                    let mut stem = vec![p, tip];
                    stem.extend(curl(tip.0, tip.1, a, u * 0.05, 0.9, if k % 2 == 0 { 1.0 } else { -1.0 }).into_iter().skip(1));
                    layers.line(&stem, v * 0.45);
                    layers.flower(&flower, tip.0, tip.1, f * rng.range(0.7, 1.0), rng.range(0.0, TAU));
                }
                layers.flower(&flower, p.0, p.1, f * 1.2, 0.0);
                let (q, _) = path.at(s + u * 0.6);
                layers.flower(&flower, q.0 - f, q.1, f * 0.5, 0.0);
                layers.flower(&flower, q.0 + f, q.1 + f * 0.5, f * 0.5, 0.0);
            }
        }
        // Bush: clumps of leaves bursting up off a grassy line, flowers in
        // among them, big clumps and small by turns.
        16 => {
            let path = track(u * 0.5);
            let steps = (path.length() / 2.0) as usize;
            let ground: Vec<(f32, f32)> = (0..=steps).map(|i| path.at(i as f32 / steps as f32 * path.length()).0).collect();
            layers.line(&ground, v * 0.5);
            for (k, s) in path.spread(u * 1.0).into_iter().enumerate() {
                let (p, t) = path.at(s);
                let (nx, ny) = inward(t);
                let big = k % 2 == 0;
                let up = ny.atan2(nx);
                // Big enough to read as a bush and not a scatter of leaves:
                // CS6's clumps stand taller than its flowers by far.
                let count = if big { 18 } else { 9 };
                for _ in 0..count {
                    let a = up + rng.range(-1.4, 1.4);
                    let ox = rng.range(-0.3, 0.3) * u;
                    let length = l * rng.range(1.0, if big { 2.2 } else { 1.5 });
                    layers.leaf(&leaf, p.0 + t.0 * ox, p.1 + t.1 * ox, length, a);
                }
                for _ in 0..if big { 3 } else { 1 } {
                    let (ox, oy) = (rng.range(-0.2, 0.2) * u, rng.range(0.05, 0.3) * u);
                    layers.flower(&flower, p.0 + t.0 * ox + nx * oy, p.1 + t.1 * ox + ny * oy, f * rng.range(0.6, 1.0), rng.range(0.0, TAU));
                }
            }
        }
        // Simple Lace: a fine waving line, flowers on the crests, large and
        // small by turns.
        17 => {
            let path = track(u * 0.6);
            let (wave, amp) = (u * 1.4, u * 0.1);
            layers.line(&wavy(&path, wave, amp, 0.0), v * 0.45);
            for (k, (s, out)) in crests(&path, wave, 0.0).into_iter().enumerate() {
                let (p, t) = path.at(s);
                let (nx, ny) = inward(t);
                let sign = if out { 1.0 } else { -1.0 };
                layers.flower(&flower, p.0 + nx * amp * sign, p.1 + ny * amp * sign, f * if k % 2 == 0 { 1.2 } else { 0.8 }, 0.0);
            }
        }
        // Pulse: a vine waving in and out, a flower at every outer swing and
        // a pair of leaves where it crosses the line.
        18 => {
            let path = track(u * 0.6);
            let wave = u * 1.8;
            let n = (path.length() / wave).round().max(4.0);
            let wave = path.length() / n;
            let amplitude = u * 0.28;
            let steps = (path.length() / 2.0) as usize;
            let points: Vec<(f32, f32)> = (0..=steps)
                .map(|i| {
                    let s = i as f32 / steps as f32 * path.length();
                    let (p, t) = path.at(s);
                    let (nx, ny) = inward(t);
                    let off = amplitude * (TAU * s / wave).sin();
                    (p.0 + nx * off, p.1 + ny * off)
                })
                .collect();
            layers.line(&points, v);
            for k in 0..n as usize {
                let s = (k as f32 + 0.75) * wave;
                let (p, t) = path.at(s);
                let (nx, ny) = inward(t);
                layers.flower(&flower, p.0 - nx * amplitude, p.1 - ny * amplitude, f, rng.range(0.0, TAU));
                let s = (k as f32 + 0.5) * wave;
                let (p, t) = path.at(s);
                let along = t.1.atan2(t.0);
                layers.leaf(&leaf, p.0, p.1, l, along - 0.9);
                layers.leaf(&leaf, p.0, p.1, l, along + PI + 0.9);
            }
        }
        // Root: a garland — a gently swaying vine thick with leaves on both
        // sides, and flowers tucked in among them.
        19 => {
            let path = track(u * 0.8);
            let steps = (path.length() / 2.0) as usize;
            let sway = |s: f32| u * 0.12 * (s / (u * 1.4)).sin();
            let points: Vec<(f32, f32)> = (0..=steps)
                .map(|i| {
                    let s = i as f32 / steps as f32 * path.length();
                    let (p, t) = path.at(s);
                    let (nx, ny) = inward(t);
                    (p.0 + nx * sway(s), p.1 + ny * sway(s))
                })
                .collect();
            layers.line(&points, v * 1.3);
            for (k, s) in path.spread(l * 0.45).into_iter().enumerate() {
                let (p, t) = path.at(s);
                let (nx, ny) = inward(t);
                let (x, y) = (p.0 + nx * sway(s), p.1 + ny * sway(s));
                let along = t.1.atan2(t.0);
                let side = if k % 2 == 0 { 1.0 } else { -1.0 };
                let spread = rng.range(0.6, 1.1);
                layers.leaf(&leaf, x, y, l * rng.range(0.8, 1.15), along + side * spread);
            }
            for s in path.spread(u * 1.1) {
                let (p, t) = path.at(s + rng.range(-0.3, 0.3) * u);
                let (nx, ny) = inward(t);
                let off = rng.range(-0.25, 0.25) * u;
                layers.flower(&flower, p.0 + nx * off, p.1 + ny * off, f * rng.range(0.7, 1.1), rng.range(0.0, TAU));
            }
        }
        // Snakes: curling S-shaped stems, each with a flower at its head.
        20 => {
            let path = track(u * 0.5);
            for s in path.spread(u * 1.5) {
                let (p, t) = path.at(s);
                let (nx, ny) = inward(t);
                let reach = u * rng.range(0.45, 0.7);
                let a = t.1.atan2(t.0) + rng.range(-0.8, 0.8);
                let (dx, dy) = (a.cos() * reach, a.sin() * reach);
                let head = (p.0 + dx, p.1 + dy);
                let tail = (p.0 - dx, p.1 - dy);
                let bend = reach * 0.9;
                let first = bezier(tail, (p.0 - dx * 0.5 + nx * bend, p.1 - dy * 0.5 + ny * bend), p, 12);
                let second = bezier(p, (p.0 + dx * 0.5 - nx * bend, p.1 + dy * 0.5 - ny * bend), head, 12);
                let mut curve = first;
                curve.extend(second.into_iter().skip(1));
                layers.line(&curve, v);
                layers.flower(&flower, head.0, head.1, f * rng.range(0.9, 1.6), rng.range(0.0, TAU));
            }
        }
        // Mustache: flowers joined dot to dot by fine lines, zigzagging
        // across the line.
        21 => {
            let path = track(u * 0.5);
            let mut chain: Vec<(f32, f32)> = Vec::new();
            for (k, s) in path.spread(u * 0.7).into_iter().enumerate() {
                let (p, t) = path.at(s);
                let (nx, ny) = inward(t);
                let off = if k % 2 == 0 { 1.0 } else { -1.0 } * u * rng.range(0.15, 0.4);
                chain.push((p.0 + nx * off, p.1 + ny * off));
            }
            if let Some(&first) = chain.first() {
                chain.push(first);
            }
            layers.line(&chain, (v * 0.6).max(0.8));
            for &(x, y) in &chain[..chain.len().saturating_sub(1)] {
                layers.flower(&flower, x, y, f * rng.range(0.5, 1.1), rng.range(0.0, TAU));
            }
        }
        // Circle Sprinkle: flowers of every size scattered along the band.
        22 => {
            let path = track(u * 0.5);
            for s in path.spread(u * 0.45) {
                let (p, t) = path.at(s + rng.range(-0.2, 0.2) * u);
                let (nx, ny) = inward(t);
                let off = rng.range(-0.45, 0.45) * u;
                layers.flower(&flower, p.0 + nx * off, p.1 + ny * off, f * rng.range(0.4, 1.7), rng.range(0.0, TAU));
            }
        }
        // Aligned Flowers: flowers in a row, large and small by turns, with
        // a small one set off to the side now and then.
        23 => {
            let path = track(u * 0.3);
            for (k, s) in path.spread(u * 0.9).into_iter().enumerate() {
                let (p, t) = path.at(s);
                let big = k % 2 == 0;
                layers.flower(&flower, p.0, p.1, f * if big { 1.3 } else { 0.8 }, 0.0);
                if k % 3 == 0 {
                    let (nx, ny) = inward(t);
                    let off = u * 0.35 * if rng.next() < 0.5 { 1.0 } else { -1.0 };
                    layers.flower(&flower, p.0 + nx * off, p.1 + ny * off, f * 0.45, 0.0);
                }
            }
        }
        // Little Flowers: single flowers each with a pair of leaves, soft.
        24 => {
            let path = track(u * 0.5);
            for s in path.spread(u * 1.3) {
                let (p, t) = path.at(s);
                let along = t.1.atan2(t.0);
                // Long enough to reach out from under the flower.
                layers.leaf(&leaf, p.0, p.1, l * 1.8, along + PI / 2.0 + 0.7);
                layers.leaf(&leaf, p.0, p.1, l * 1.8, along + PI / 2.0 - 0.7);
                layers.flower(&flower, p.0, p.1, f * rng.range(1.0, 1.5), rng.range(0.0, TAU));
            }
        }
        // Snowflakes: a drift of small flowers and leaves along the band.
        25 => {
            let path = track(u * 0.5);
            for s in path.spread(u * 0.22) {
                let (p, t) = path.at(s);
                let (nx, ny) = inward(t);
                let off = rng.range(-0.5, 0.5) * u;
                let (x, y) = (p.0 + nx * off, p.1 + ny * off);
                if rng.next() < 0.3 {
                    layers.leaf(&leaf, x, y, l * rng.range(0.5, 0.8), rng.range(0.0, TAU));
                } else {
                    layers.flower(&flower, x, y, f * rng.range(0.35, 0.8), rng.range(0.0, TAU));
                }
            }
        }
        // Flurry: tight clusters of flowers with a leaf or two.
        26 => {
            let path = track(u * 0.5);
            for s in path.spread(u * 1.0) {
                let (p, t) = path.at(s);
                let (nx, ny) = inward(t);
                for _ in 0..2 {
                    let a = rng.range(0.0, TAU);
                    layers.leaf(&leaf, p.0, p.1, l * rng.range(0.6, 0.9), a);
                }
                for _ in 0..(3 + (rng.next() * 3.0) as usize) {
                    let (ox, oy) = (rng.range(-0.3, 0.3) * u, rng.range(-0.25, 0.25) * u);
                    layers.flower(
                        &flower,
                        p.0 + t.0 * ox + nx * oy,
                        p.1 + t.1 * ox + ny * oy,
                        f * rng.range(0.6, 1.1),
                        rng.range(0.0, TAU),
                    );
                }
            }
        }
        // Check Marks: ticks drawn in the vine colour, a flower on the end of
        // every other one.
        27 => {
            let path = track(u * 0.5);
            for (k, s) in path.spread(u * 0.8).into_iter().enumerate() {
                let (p, t) = path.at(s);
                let (nx, ny) = inward(t);
                let arm = u * 0.35;
                let a = (p.0 - t.0 * arm * 0.3 - nx * arm * 0.6, p.1 - t.1 * arm * 0.3 - ny * arm * 0.6);
                let b = (p.0 + nx * arm * 0.4, p.1 + ny * arm * 0.4);
                let c = (p.0 + t.0 * arm - nx * arm * 0.9, p.1 + t.1 * arm - ny * arm * 0.9);
                layers.line(&[a, b, c], (v * 0.7).max(0.8));
                if k % 2 == 1 {
                    layers.flower(&flower, c.0, c.1, f * 1.2, 0.0);
                }
            }
        }
        // Focused Lines: short lines all aimed at the middle of the picture,
        // each ending in a flower on the inside.
        28 => {
            let path = track(u * 0.5);
            for (k, s) in path.spread(u * 0.7 / density).into_iter().enumerate() {
                let (p, _) = path.at(s);
                let (dx, dy) = (centre.0 - p.0, centre.1 - p.1);
                let (dx, dy) = (dx * flip, dy * flip);
                let d = (dx * dx + dy * dy).sqrt().max(1.0);
                let length = u * if k % 2 == 0 { 0.55 } else { 0.3 } * rng.range(0.8, 1.2);
                let end = (p.0 + dx / d * length, p.1 + dy / d * length);
                let start = (p.0 - dx / d * length * 0.3, p.1 - dy / d * length * 0.3);
                layers.line(&[start, end], (v * 0.6).max(0.8));
                layers.flower(&flower, end.0, end.1, f * if k % 2 == 0 { 1.2 } else { 0.8 }, 0.0);
            }
        }
        // Focused Parallel Lines: the same, but every line on one slant.
        29 => {
            let path = track(u * 0.5);
            // Angle leans the lines on from CS6's 45°.
            let slant = PI / 4.0 + lean;
            let (dx, dy) = (slant.cos() * flip, slant.sin() * flip);
            for (k, s) in path.spread(u * 0.75 / density).into_iter().enumerate() {
                let (p, _) = path.at(s);
                let length = u * if k % 2 == 0 { 0.5 } else { 0.3 };
                let start = (p.0 - dx * length * 0.5, p.1 - dy * length * 0.5);
                let end = (p.0 + dx * length * 0.5, p.1 + dy * length * 0.5);
                layers.line(&[start, end], (v * 0.6).max(0.8));
                layers.flower(&flower, start.0, start.1, f * if k % 2 == 0 { 1.4 } else { 0.8 }, 0.0);
            }
        }
        // Focused Vibration: lines standing straight off the edge, their
        // lengths jumping about like a trace, each tipped with a flower.
        30 => {
            let path = track(0.0);
            for s in path.spread(u * 0.45 / density) {
                let (p, t) = path.at(s);
                let (nx, ny) = inward(t);
                let length = u * rng.range(0.1, 0.6);
                let end = (p.0 + nx * length, p.1 + ny * length);
                layers.line(&[p, end], (v * 0.5).max(0.8));
                layers.flower(&flower, end.0, end.1, f * 0.8, 0.0);
            }
        }
        // Zen Garden: each flower set in rings, like a stone in raked sand.
        31 => {
            let path = track(u * 0.5);
            for s in path.spread(u * 1.1) {
                let (p, t) = path.at(s);
                let (nx, ny) = inward(t);
                let off = rng.range(-0.2, 0.2) * u;
                let (x, y) = (p.0 + nx * off, p.1 + ny * off);
                for ring in 1..=3 {
                    let r = u * 0.16 * ring as f32 * rng.range(0.9, 1.1);
                    let n = ((r * 0.8) as usize).clamp(16, 96);
                    let points: Vec<(f32, f32)> = (0..=n)
                        .map(|i| {
                            let a = i as f32 / n as f32 * TAU;
                            (x + r * a.cos(), y + r * a.sin())
                        })
                        .collect();
                    layers.line(&points, (v * 0.4).max(0.7));
                }
                layers.flower(&flower, x, y, f, rng.range(0.0, TAU));
            }
        }
        // Spikes: small flowers along the line, each with a burst of fine
        // spikes round it.
        32 => {
            let path = track(u * 0.5);
            for s in path.spread(u * 1.0) {
                let (p, _) = path.at(s);
                let turn = rng.range(0.0, TAU);
                for i in 0..6 {
                    let a = turn + i as f32 / 6.0 * TAU;
                    let reach = u * if i % 2 == 0 { 0.4 } else { 0.25 };
                    layers.line(&[p, (p.0 + reach * a.cos(), p.1 + reach * a.sin())], (v * 0.35).max(0.7));
                }
                layers.flower(&flower, p.0, p.1, f * 0.8, 0.0);
            }
        }
        // Anemone: flowers crowded all the way round, jostling in and out of
        // line, on a well-rounded track.
        33 => {
            // In clumps of two or three, not one by one.
            let path = track(u * 1.2);
            for s in path.spread(f * 3.4) {
                let (p, t) = path.at(s + rng.range(-0.4, 0.4) * f);
                let (nx, ny) = inward(t);
                let off = rng.range(-0.9, 0.9) * f;
                for _ in 0..(2 + (rng.next() * 2.0) as usize) {
                    let (ax, ay) = (rng.range(-0.8, 0.8) * f, rng.range(-0.6, 0.6) * f);
                    layers.flower(
                        &flower,
                        p.0 + nx * (off + ay) + t.0 * ax,
                        p.1 + ny * (off + ay) + t.1 * ax,
                        f * rng.range(1.1, 1.45),
                        rng.range(0.0, TAU),
                    );
                }
            }
        }
        // Pinwheel: sprays of curved stems, each tipped with a flower,
        // swirling about a centre; set symmetrically, with a tight bunch at
        // each corner and a double spray in the middle of the long sides.
        34 => {
            let path = track(0.0);
            let spray = |layers: &mut Layers, x: f32, y: f32, reach: f32, arms: usize, turn: f32, swirl: f32| {
                for i in 0..arms {
                    let a = turn + i as f32 / arms as f32 * TAU;
                    let tip = (x + reach * a.cos(), y + reach * a.sin());
                    let bend = a + swirl;
                    let ctrl = (x + reach * 0.55 * bend.cos(), y + reach * 0.55 * bend.sin());
                    layers.line(&bezier((x, y), ctrl, tip, 12), v * 0.8);
                    layers.flower(&flower, tip.0, tip.1, f, 0.0);
                }
            };
            // The corners.
            for (x, y) in [(path.x0, path.y0), (path.x1, path.y0), (path.x0, path.y1), (path.x1, path.y1)] {
                for k in 0..9 {
                    let a = k as f32 / 9.0 * TAU + rng.range(-0.2, 0.2);
                    let r = u * 0.45 * rng.range(0.4, 1.0);
                    let tip = (x + r * a.cos(), y + r * a.sin());
                    layers.line(&[(x, y), tip], v * 0.8);
                    layers.flower(&flower, tip.0, tip.1, f * 1.2, 0.0);
                }
            }
            // Along each side, mirrored about its middle.
            // CS6's long side carries seven: a corner, a large spray, a small
            // one, the double in the middle, and back again. The short sides
            // have half as many gaps, whatever their length.
            let long_is_width = path.x1 - path.x0 >= path.y1 - path.y0;
            let long_length = (path.x1 - path.x0).max(path.y1 - path.y0);
            let gaps_long = ((long_length / (u * 3.0)).round() as usize).max(2);
            let side = |layers: &mut Layers, rng: &mut Rng, a: (f32, f32), b: (f32, f32), long: bool| {
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let n = if long { gaps_long } else { (gaps_long / 2).max(1) };
                for i in 1..n {
                    let t = i as f32 / n as f32;
                    let (x, y) = (a.0 + dx * t, a.1 + dy * t);
                    let middle = long && n % 2 == 0 && i == n / 2;
                    let small = !middle && long && i % 2 == 0;
                    let reach = u * if small { 0.55 } else { 1.0 };
                    let turn = rng.range(0.0, TAU);
                    // Swirling one way on the left half and the other on the
                    // right, so the side reads as a mirror image.
                    let swirl = if t < 0.5 { 1.1 } else { -1.1 };
                    spray(layers, x, y, reach, 6, turn, swirl);
                    if middle {
                        spray(layers, x, y, reach, 6, turn + PI / 6.0, -swirl);
                    }
                }
            };
            side(&mut layers, &mut rng, (path.x0, path.y0), (path.x1, path.y0), long_is_width);
            side(&mut layers, &mut rng, (path.x0, path.y1), (path.x1, path.y1), long_is_width);
            side(&mut layers, &mut rng, (path.x0, path.y0), (path.x0, path.y1), !long_is_width);
            side(&mut layers, &mut rng, (path.x1, path.y0), (path.x1, path.y1), !long_is_width);
        }
        // Spacing: flowers in a repeating run of large, medium and small.
        _ => {
            let path = track(u * 0.3);
            for (k, s) in path.spread(u * 0.6).into_iter().enumerate() {
                let (p, _) = path.at(s);
                let size = [1.3, 0.85, 0.45][k % 3];
                layers.flower(&flower, p.0, p.1, f * size, 0.0);
            }
        }
    }

    let used = uses(frame);
    if used.vine {
        fill(pixmap, &layers.vine, options.vine, 1.0);
    }
    if used.leaf && options.leaf != 0 {
        // Little Flowers and Snowflakes are soft, as CS6 draws them.
        let alpha = if matches!(frame, 24 | 25) { 0.7 } else { 1.0 };
        fill(pixmap, &layers.leaves, options.leaf_colour, alpha);
    }
    if used.flower && options.flower != 0 {
        fill(pixmap, &layers.soft, options.flower_colour, 0.45);
        let alpha = if matches!(frame, 24 | 25) { 0.8 } else { 1.0 };
        fill(pixmap, &layers.flowers, options.flower_colour, alpha);
    }
}

// --------------------------------------------------------------- moulding --

/// How wide a ruled or moulded frame is, and how far its corner shapes reach,
/// each at Size 100 as a share of the canvas's shorter side.
const BAND_AT_FULL: f32 = 0.25;
const CORNER_PER_BAND: f32 = 2.4;

/// The six corners the ruled frames (36 to 41) and the moulded ones (42 to 47)
/// share, in CS6's order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Corner {
    Square,
    Rounded,
    Inverse,
    InverseSmall,
    Dual,
    DualSmall,
}

/// How deep a point is inside the frame's outline, `(u, v)` being its
/// distance in from the nearer vertical and horizontal edges of the outline:
/// the distance to the outline, positive inside. Worked out on one corner and
/// folded onto the other three, so the frame is symmetrical by construction.
fn depth(corner: Corner, u: f32, v: f32, reach: f32) -> f32 {
    let straight = u.min(v);
    match corner {
        Corner::Square => straight,
        Corner::Rounded => {
            if u < reach && v < reach {
                reach - ((reach - u).powi(2) + (reach - v).powi(2)).sqrt()
            } else {
                straight
            }
        }
        Corner::Inverse => straight.min((u * u + v * v).sqrt() - reach),
        Corner::InverseSmall => straight.min((u * u + v * v).sqrt() - reach * 0.55),
        Corner::Dual | Corner::DualSmall => {
            // Two scallops, one off each edge, meeting in a point on the
            // diagonal.
            let r = if corner == Corner::Dual { reach * 0.75 } else { reach * 0.45 };
            let c = r * 0.93;
            straight
                .min(((u - c).powi(2) + v * v).sqrt() - r)
                .min((u * u + (v - c).powi(2)).sqrt() - r)
        }
    }
}

/// The ruled frames (36 to 41) — lines following the outline, one heavy and
/// one fine — and the moulded ones (42 to 47), a bevelled band lit from the
/// top left, both in the vine colour.
///
/// Drawn from a distance field rather than as outlines: every line and every
/// step of the moulding is a fixed depth inside the same outline, so the
/// corners stay parallel however elaborate they are, which offsetting an
/// outline would not guarantee.
fn moulding(pixmap: &mut Pixmap, frame: u32, colour: Rgba8, scale: &Scale, invert: bool) {
    let corner = match (frame - 36) % 6 {
        0 => Corner::Square,
        1 => Corner::Rounded,
        2 => Corner::Inverse,
        3 => Corner::InverseSmall,
        4 => Corner::Dual,
        _ => Corner::DualSmall,
    };
    let art = frame >= 42;
    // The corner shapes follow Size; the band's own width follows Thickness
    // as well.
    let reach = scale.unit / UNIT_AT_FULL * BAND_AT_FULL * CORNER_PER_BAND;
    let band = (scale.unit / UNIT_AT_FULL * BAND_AT_FULL * scale.thickness).min(reach * 1.5);
    let (w, h) = (pixmap.width() as f32, pixmap.height() as f32);
    let inset = scale.inset;
    let deep = |x: f32, y: f32| {
        let u = x.min(w - x) - inset;
        let v = y.min(h - y) - inset;
        depth(corner, u, v, reach)
    };
    let heavy = (band * 0.28).max(1.0);
    let fine = (band * 0.1).max(1.0);
    let base = [colour.r as f32, colour.g as f32, colour.b as f32];
    let width = pixmap.width() as i32;
    let reach_in = (inset + band + reach + 2.0) as i32;

    for y in 0..pixmap.height() as i32 {
        let near_y = y < reach_in || y >= pixmap.height() as i32 - reach_in;
        for x in 0..width {
            // Only the border can be touched; skip the middle outright.
            if !near_y && x >= reach_in && x < width - reach_in {
                continue;
            }
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let d = deep(px, py);
            if d < -1.0 || d > band + 1.0 {
                continue;
            }
            let (tone, cover) = if art {
                // The moulding's profile, outside in: a dark outer edge, a
                // rounded bead lit by its slope, a pale flat and a dark lip.
                let cover = (d + 0.5).clamp(0.0, 1.0) * (band - d + 0.5).clamp(0.0, 1.0);
                // Inverted, the profile runs the other way across the band,
                // so what was lit is in shadow.
                let t = (d / band).clamp(0.0, 1.0);
                let t = if invert { 1.0 - t } else { t };
                let tone = if t < 0.1 {
                    0.55
                } else if t < 0.7 {
                    let slope = (PI * (t - 0.1) / 0.6).cos();
                    // Which way the band faces: the gradient of the depth.
                    let e = 0.5;
                    let gx = deep(px + e, py) - deep(px - e, py);
                    let gy = deep(px, py + e) - deep(px, py - e);
                    let g = (gx * gx + gy * gy).sqrt().max(1e-3);
                    // The bead rises going in, so its outer slope faces out
                    // and its inner slope faces in; the light is at the top
                    // left, facing down and right.
                    let facing = -(gx / g * -0.7 + gy / g * -0.7) * slope;
                    1.0 + 0.45 * facing
                } else if t < 0.86 {
                    1.2
                } else {
                    0.6
                };
                (tone, cover)
            } else {
                let line = |at: f32, width: f32| (width * 0.5 - (d - at).abs() + 0.5).clamp(0.0, 1.0);
                // Inverted, the fine line goes outside and the heavy one in.
                let (outer, inner) = if invert { (fine, heavy) } else { (heavy, fine) };
                let cover = line(outer * 0.5, outer).max(line(outer + band * 0.3 + inner * 0.5, inner));
                (1.0, cover)
            };
            if cover <= 0.0 {
                continue;
            }
            let lit = Rgba8::new(
                (base[0] * tone).round().clamp(0.0, 255.0) as u8,
                (base[1] * tone).round().clamp(0.0, 255.0) as u8,
                (base[2] * tone).round().clamp(0.0, 255.0) as u8,
                255,
            );
            let dst = pixmap.get(x, y);
            pixmap.set(x, y, crate::brush::source_over(dst, lit, cover));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn white(w: u32, h: u32) -> Pixmap {
        Pixmap::filled(w, h, Rgba8::new(255, 255, 255, 255))
    }

    fn changed(a: &Pixmap, b: &Pixmap) -> usize {
        a.as_bytes().chunks_exact(4).zip(b.as_bytes().chunks_exact(4)).filter(|(p, q)| p != q).count()
    }

    #[test]
    fn every_frame_draws_something_and_leaves_the_middle_alone() {
        for frame in FRAMES {
            let before = white(300, 200);
            let mut px = before.clone();
            picture_frame(&mut px, &FrameOptions { frame, ..FrameOptions::default() });
            assert!(changed(&before, &px) > 200, "frame {frame} drew next to nothing");
            // The middle of the picture is the picture.
            for y in 80..120 {
                for x in 120..180 {
                    assert_eq!(px.get(x, y), Rgba8::new(255, 255, 255, 255), "frame {frame} reached {x},{y}");
                }
            }
        }
    }

    #[test]
    fn every_flower_and_leaf_has_a_shape() {
        for flower in 1..=22 {
            let mut px = white(64, 64);
            fill(&mut px, &place(&flower_shape(flower), 32.0, 32.0, 28.0, 0.0), Rgba8::BLACK, 1.0);
            let ink = px.as_bytes().chunks_exact(4).filter(|p| p[0] < 128).count();
            assert!(ink > 100 && ink < 64 * 64 * 9 / 10, "flower {flower} covers {ink} pixels");
        }
        for leaf in 1..=23 {
            let mut px = white(64, 64);
            fill(&mut px, &place(&leaf_shape(leaf), 32.0, 32.0, 28.0, 0.0), Rgba8::BLACK, 1.0);
            let ink = px.as_bytes().chunks_exact(4).filter(|p| p[0] < 128).count();
            assert!(ink > 100 && ink < 64 * 64 * 9 / 10, "leaf {leaf} covers {ink} pixels");
        }
    }

    #[test]
    fn a_hole_is_cut_out_of_the_shape_around_it() {
        // Star Cloud is a ring with a star inside it: its middle, between the
        // two, is left bare.
        let mut px = white(100, 100);
        fill(&mut px, &place(&flower_shape(16), 50.0, 50.0, 45.0, 0.0), Rgba8::BLACK, 1.0);
        assert_eq!(px.get(50, 50).r, 0, "the star is missing");
        assert_eq!(px.get(50, 50 + 29).r, 255, "the ring is filled in");
        assert_eq!(px.get(50, 50 + 42).r, 0, "the ring is missing");
    }

    #[test]
    fn the_frame_draws_in_the_colours_it_is_given() {
        let mut px = white(300, 200);
        let options = FrameOptions {
            frame: 34,
            vine: Rgba8::new(200, 0, 0, 255),
            flower_colour: Rgba8::new(0, 0, 200, 255),
            ..FrameOptions::default()
        };
        picture_frame(&mut px, &options);
        let has = |c: Rgba8| px.as_bytes().chunks_exact(4).any(|p| p[..3] == [c.r, c.g, c.b]);
        assert!(has(options.vine), "no vine in the vine colour");
        assert!(has(options.flower_colour), "no flowers in the flower colour");
    }

    #[test]
    fn a_frame_without_leaves_ignores_the_leaf() {
        let run = |leaf| {
            let mut px = white(240, 160);
            picture_frame(&mut px, &FrameOptions { frame: 34, leaf, ..FrameOptions::default() });
            px
        };
        assert_eq!(run(4).as_bytes(), run(20).as_bytes());
    }

    #[test]
    fn a_bigger_margin_moves_the_frame_in() {
        let first_ink = |margin| {
            let mut px = white(300, 200);
            picture_frame(&mut px, &FrameOptions { frame: 36, margin, ..FrameOptions::default() });
            (0..100).find(|&y| px.get(150, y).r < 255).unwrap()
        };
        assert!(first_ink(40) > first_ink(5));
    }

    #[test]
    fn the_arrangement_reshuffles_the_frame() {
        let run = |arrangement| {
            let mut px = white(240, 160);
            picture_frame(&mut px, &FrameOptions { frame: 22, arrangement, ..FrameOptions::default() });
            px
        };
        assert_eq!(run(3).as_bytes(), run(3).as_bytes());
        assert_ne!(run(3).as_bytes(), run(4).as_bytes());
    }

    #[test]
    fn fade_thins_the_middle_of_each_side_and_spares_the_corners() {
        let ink = |fade, x0: i32, x1: i32| {
            let mut px = white(400, 300);
            picture_frame(&mut px, &FrameOptions { frame: 36, fade, ..FrameOptions::default() });
            (x0..x1).flat_map(|x| (0..40).map(move |y| (x, y))).map(|(x, y)| 255 - px.get(x, y).g as u32).sum::<u32>()
        };
        // The top side's middle, and its left end.
        let (middle, corner) = ((180, 220), (8, 48));
        assert!(ink(80, middle.0, middle.1) * 3 < ink(0, middle.0, middle.1), "the middle did not fade");
        let (plain, faded) = (ink(0, corner.0, corner.1), ink(80, corner.0, corner.1));
        assert!(faded * 10 > plain * 7, "the corner faded from {plain} to {faded}");
    }

    #[test]
    fn thickness_widens_the_lines() {
        let ink = |thickness| {
            let mut px = white(300, 200);
            picture_frame(&mut px, &FrameOptions { frame: 17, thickness, ..FrameOptions::default() });
            changed(&white(300, 200), &px)
        };
        assert!(ink(120) > ink(10));
    }

    #[test]
    fn number_of_lines_packs_the_lines_closer() {
        let crossings = |lines| {
            let mut px = white(400, 300);
            let options = FrameOptions { frame: 14, lines, flower: 0, ..FrameOptions::default() };
            picture_frame(&mut px, &options);
            // Along the middle of the top band.
            let y = (0..60).max_by_key(|&y| (0..400).filter(|&x| px.get(x, y).g < 200).count()).unwrap();
            (1..400).filter(|&x| px.get(x, y).g < 200 && px.get(x - 1, y).g >= 200).count()
        };
        assert!(crossings(15) > crossings(5) * 2);
    }

    #[test]
    fn invert_turns_a_frame_inside_out() {
        for frame in [1, 20, 38, 44] {
            let run = |invert| {
                let mut px = white(240, 160);
                picture_frame(&mut px, &FrameOptions { frame, invert, ..FrameOptions::default() });
                px
            };
            assert_ne!(run(false).as_bytes(), run(true).as_bytes(), "frame {frame}");
        }
    }

    #[test]
    fn the_moulding_is_lit_from_the_top_left() {
        // The bead's light and dark sides swap between the top edge and the
        // bottom one, which is what makes it read as raised.
        let mut px = white(300, 300);
        let colour = Rgba8::new(150, 100, 50, 255);
        picture_frame(&mut px, &FrameOptions { frame: 42, vine: colour, ..FrameOptions::default() });
        let column = |ys: std::ops::Range<i32>| -> Vec<u8> { ys.map(|y| px.get(150, y).r).collect() };
        let top = column(0..60);
        let bottom: Vec<u8> = column(240..300).into_iter().rev().collect();
        assert_ne!(top, bottom);
    }
}

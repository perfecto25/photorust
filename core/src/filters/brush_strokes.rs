//! Filter ▸ Brush Strokes.
//!
//! CS6 keeps this family in the Filter Gallery, as it does Artistic. The
//! Gallery is not built (docs/ROADMAP.md), so the filters live under a
//! Filter ▸ Brush Strokes submenu instead. Accented Edges, Angled Strokes,
//! Crosshatch, Dark Strokes, Ink Outlines and Spatter are built; the other two
//! are listed in the menu and disabled.

use crate::buffer::Pixmap;
use crate::filters::artistic::{blur_field, noise};
use rayon::prelude::*;

/// CS6's ranges for Accented Edges, which its three sliders run over.
pub const ACCENT_WIDTH: std::ops::RangeInclusive<u32> = 1..=14;
pub const ACCENT_BRIGHTNESS: std::ops::RangeInclusive<u32> = 0..=50;
pub const ACCENT_SMOOTHNESS: std::ops::RangeInclusive<u32> = 1..=15;

/// Where Edge Brightness turns from dark accents to light ones. Below it the
/// edges are inked; above it they are chalked, harder the further off it.
const ACCENT_NEUTRAL: f32 = 25.0;

/// How far the picture is smoothed before its edges are found, in pixels per
/// step of Smoothness: what washes out the small detail, so only the broader
/// boundaries are accented.
const ACCENT_SMOOTH_PER_STEP: f32 = 0.45;

/// How much of that smoothing the picture itself keeps under the accents,
/// as a fraction of it.
const ACCENT_BASE_SMOOTH: f32 = 0.35;

/// Which edges are accented, in Sobel units of the smoothed brightness: an
/// edge below the first is left alone, one above the second is accented in
/// full, and between the two the accent rises smoothly. A threshold rather than
/// a gain is what makes CS6's accents crisp lines instead of a haze.
const ACCENT_FROM: f32 = 10.0;
const ACCENT_FULL: f32 = 45.0;

/// How much lower a wide accent sets that threshold, as a fraction per step of
/// Edge Width: a heavy brush picks up boundaries a fine one passes over.
const ACCENT_WIDE_REACH: f32 = 0.04;

/// How far an accent spreads per step of Edge Width, as a radius in pixels of
/// the strongest nearby edge, and how far that is softened.
const ACCENT_SPREAD_PER_STEP: f32 = 0.3;
const ACCENT_SOFT: f32 = 0.6;

/// How a chalk accent lights the picture at full strength: a gain on the
/// colour, which keeps its hue and pushes it towards a vivid, lighter version
/// of itself — the lime of a lit stem, the hot pink of a lit petal — and a
/// lift in levels on top.
const ACCENT_CHALK_GAIN: f32 = 1.6;
const ACCENT_CHALK_LIFT: f32 = 25.0;
const ACCENT_CHALK_SATURATION: f32 = 0.9;

/// Filter ▸ Brush Strokes ▸ Accented Edges: the picture's boundaries drawn
/// over it in light or dark.
///
/// The picture is smoothed by **Smoothness**, its edges are found on the
/// smoothed brightness and thresholded into crisp lines, and each line is
/// thickened to **Edge Width**. Then the lines are lit when **Edge
/// Brightness** is above the middle of its range — chalk, which keeps the
/// colour and makes it vivid — and inked black when it is below. Away from the
/// lines the picture is left as it was.
///
/// Alpha is left alone.
///
/// No GPU path. The edge field is floats, spread by a max filter, and each
/// stage would upload and read back (docs/gpu-migration.md).
pub fn accented_edges(pixmap: &mut Pixmap, width: u32, brightness: u32, smoothness: u32) {
    if pixmap.is_empty() {
        return;
    }
    let edge_width = width.clamp(*ACCENT_WIDTH.start(), *ACCENT_WIDTH.end()) as f32;
    let brightness = brightness.clamp(*ACCENT_BRIGHTNESS.start(), *ACCENT_BRIGHTNESS.end()) as f32;
    let smoothness = smoothness.clamp(*ACCENT_SMOOTHNESS.start(), *ACCENT_SMOOTHNESS.end()) as f32;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);

    let smooth = smoothness * ACCENT_SMOOTH_PER_STEP;
    let mut base = pixmap.clone();
    crate::filters::convolve::gaussian_blur_accelerated(&mut base, smooth * ACCENT_BASE_SMOOTH);

    let mut tone: Vec<f32> = pixmap
        .as_bytes()
        .par_chunks_exact(4)
        .map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
        .collect();
    blur_field(&mut tone, w, h, smooth);

    let mut edges = sobel(&tone, w, h);
    let lower = (1.0 - (edge_width - 1.0) * ACCENT_WIDE_REACH).max(0.3);
    let (from, full) = (ACCENT_FROM * lower, ACCENT_FULL * lower);
    edges.par_iter_mut().for_each(|e| {
        let t = ((*e - from) / (full - from)).clamp(0.0, 1.0);
        *e = t * t * (3.0 - 2.0 * t);
    });
    let spread = (edge_width * ACCENT_SPREAD_PER_STEP).round() as usize;
    widest_nearby(&mut edges, w, h, spread);
    blur_field(&mut edges, w, h, ACCENT_SOFT);

    // -1 is full ink, +1 full chalk.
    let tint = (brightness - ACCENT_NEUTRAL) / ACCENT_NEUTRAL;
    let edges = &edges;
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .zip(base.as_bytes().par_chunks_exact(stride))
        .enumerate()
        .for_each(|(y, (out, base))| {
            for (x, (px, b)) in out.chunks_exact_mut(4).zip(base.chunks_exact(4)).enumerate() {
                let accent = edges[y * w + x] * tint.abs();
                let grey = 0.299 * b[0] as f32 + 0.587 * b[1] as f32 + 0.114 * b[2] as f32;
                let vivid = if tint >= 0.0 { 1.0 + ACCENT_CHALK_SATURATION * accent } else { 1.0 };
                for c in 0..3 {
                    let v = grey + (b[c] as f32 - grey) * vivid;
                    let v = if tint >= 0.0 {
                        v * (1.0 + ACCENT_CHALK_GAIN * accent) + ACCENT_CHALK_LIFT * accent
                    } else {
                        v * (1.0 - accent)
                    };
                    px[c] = v.round().clamp(0.0, 255.0) as u8;
                }
                // Alpha stands: accenting the picture does not change the
                // layer's shape.
            }
        });
}

/// How far Accented Edges reaches, in pixels.
pub fn accented_edges_reach(width: u32, smoothness: u32) -> u32 {
    let width = width.clamp(*ACCENT_WIDTH.start(), *ACCENT_WIDTH.end()) as f32;
    let smoothness = smoothness.clamp(*ACCENT_SMOOTHNESS.start(), *ACCENT_SMOOTHNESS.end()) as f32;
    let spread = (width * ACCENT_SPREAD_PER_STEP).round();
    ((smoothness * ACCENT_SMOOTH_PER_STEP + ACCENT_SOFT) * 3.0 + spread).ceil() as u32 + 2
}

/// CS6's ranges for Angled Strokes, which its three sliders run over.
pub const ANGLED_BALANCE: std::ops::RangeInclusive<u32> = 0..=100;
pub const ANGLED_LENGTH: std::ops::RangeInclusive<u32> = 3..=50;
pub const ANGLED_SHARPNESS: std::ops::RangeInclusive<u32> = 0..=10;

/// How far along its stroke a pixel takes its colour from, at most, as a
/// fraction of the stroke; and over how much of the stroke's length the
/// colour is then settled. It is settled by a median along the stroke rather
/// than an average: a median lays each stroke as a flat slab with a crisp end
/// and leaves a smooth sky smooth, where an average blurs both.
const ANGLED_DRAG: f32 = 0.15;
const ANGLED_SMEAR: f32 = 0.7;

/// How wide a stroke is, as a blur in pixels across the noise that drags it.
const ANGLED_WIDTH: f32 = 1.6;

/// The straight lines each stroke leaves, in levels of brightness, in the
/// light and in the dark, and how thick they are as a blur across them.
/// Faint in the light, where the paint is thin; in the dark they are heavy
/// and mostly lighter than the paint round them, as a pastel stick drags pale
/// streaks over a dark ground.
const ANGLED_BRISTLES_LIGHT: f32 = 0.0;
const ANGLED_BRISTLES_DARK: f32 = 7.0;
const ANGLED_BRISTLE_WIDTH: f32 = 0.5;

/// How wide the change-over from one direction to the other is, in
/// lightness, and how far the lightness it is judged on is smoothed, in
/// pixels, so the two directions meet in patches rather than in speckle.
const ANGLED_SWITCH: f32 = 0.08;
const ANGLED_JUDGE: f32 = 2.0;

/// Sharpness: an unsharp mask over the strokes, its scale in pixels, its
/// strength per step, the difference in levels below which it leaves a pixel
/// alone, and the most it moves one.
///
/// Taken on brightness and laid on all three channels alike. Per channel, it
/// amplifies the small colour differences between neighbouring strokes into
/// orange and yellow specks; and without the threshold it turns every ridge
/// into a sawtooth. What CS6's Sharpness crisps is where one stroke ends and
/// the next begins.
const ANGLED_SHARP_SCALE: f32 = 1.2;
const ANGLED_SHARP_PER_STEP: f32 = 0.15;
const ANGLED_SHARP_FROM: f32 = 6.0;
const ANGLED_SHARP_MOST: f32 = 40.0;

/// Filter ▸ Brush Strokes ▸ Angled Strokes: the picture repainted in diagonal
/// strokes, the light parts one way and the dark parts the other.
///
/// Each pixel's colour is dragged a little way along a diagonal and smeared
/// along it, with the fine straight lines of the bristles over it. The light
/// parts of the picture run down to the right, the dark parts up to the
/// right. **Direction Balance** moves the line between them: at 50 it is
/// mid-grey, towards 100 nearly everything runs up to the right, towards 0
/// down. **Stroke Length** is how long the strokes are, and
/// **Sharpness** how crisp.
///
/// Alpha is left alone.
///
/// No GPU path. The strokes are sampled along lines, which would fit, but the
/// two directions and the noise that drags them would each upload and read
/// back, and nothing downstream stays on the device.
pub fn angled_strokes(pixmap: &mut Pixmap, balance: u32, length: u32, sharpness: u32) {
    if pixmap.is_empty() {
        return;
    }
    let balance = balance.clamp(*ANGLED_BALANCE.start(), *ANGLED_BALANCE.end()) as f32 / 100.0;
    let length = length.clamp(*ANGLED_LENGTH.start(), *ANGLED_LENGTH.end()) as f32;
    let sharpness = sharpness.clamp(*ANGLED_SHARPNESS.start(), *ANGLED_SHARPNESS.end()) as f32;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);

    let picture = pixmap.clone();
    let stroke = |angle: f32, salt: usize| lay_strokes(&picture, angle, length, salt);
    let rising = stroke(45.0, 21);
    let falling = stroke(-45.0, 22);
    let bristles = |angle: f32, salt: usize| bristle_lines(w, h, angle, length, salt);
    let (rising_lines, falling_lines) = (bristles(45.0, 23), bristles(-45.0, 24));
    let (rising_lines, falling_lines) = (&rising_lines, &falling_lines);

    let mut judged: Vec<f32> = picture
        .as_bytes()
        .par_chunks_exact(4)
        .map(|p| (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) / 255.0)
        .collect();
    blur_field(&mut judged, w, h, ANGLED_JUDGE);
    let split = balance;
    let judged = &judged;

    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .zip(rising.as_bytes().par_chunks_exact(stride))
        .zip(falling.as_bytes().par_chunks_exact(stride))
        .enumerate()
        .for_each(|(y, ((out, up), down))| {
            for (x, ((px, u), d)) in out
                .chunks_exact_mut(4)
                .zip(up.chunks_exact(4))
                .zip(down.chunks_exact(4))
                .enumerate()
            {
                let t = ((judged[y * w + x] - split) / ANGLED_SWITCH * 0.5 + 0.5).clamp(0.0, 1.0);
                // Light falls to the right, dark rises.
                let fall = t * t * (3.0 - 2.0 * t);
                let i = y * w + x;
                let n = rising_lines[i] * (1.0 - fall) + falling_lines[i] * fall;
                let dark = 1.0 - judged[i];
                // In the dark the streaks lean pale: chalk over a dark ground.
                let n = n + dark * n.abs() * 0.5;
                let line = n * (ANGLED_BRISTLES_LIGHT + (ANGLED_BRISTLES_DARK - ANGLED_BRISTLES_LIGHT) * dark * dark);
                for c in 0..3 {
                    let v = d[c] as f32 * fall + u[c] as f32 * (1.0 - fall) + line;
                    px[c] = v.round().clamp(0.0, 255.0) as u8;
                }
                // Alpha stands: restroking the picture does not change the
                // layer's shape.
            }
        });

    let gain = sharpness * ANGLED_SHARP_PER_STEP;
    if gain > 0.0 {
        let sharp = pixmap.clone();
        let mut soft = pixmap.clone();
        crate::filters::convolve::gaussian_blur_accelerated(&mut soft, ANGLED_SHARP_SCALE);
        pixmap
            .as_bytes_mut()
            .par_chunks_exact_mut(4)
            .zip(sharp.as_bytes().par_chunks_exact(4))
            .zip(soft.as_bytes().par_chunks_exact(4))
            .for_each(|((px, s), b)| {
                let luma = |p: &[u8]| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
                let diff = luma(s) - luma(b);
                let over = (diff.abs() - ANGLED_SHARP_FROM).max(0.0) * diff.signum();
                let lift = (over * gain).clamp(-ANGLED_SHARP_MOST, ANGLED_SHARP_MOST);
                for c in 0..3 {
                    px[c] = (s[c] as f32 + lift).round().clamp(0.0, 255.0) as u8;
                }
            });
    }
}

/// The picture laid in strokes along `angle`, `length` long: each pixel's
/// colour dragged a little way along the stroke and then settled by a median
/// along it. Laid by where on the canvas a pixel is; `salt` keeps two sets of
/// strokes apart.
fn lay_strokes(picture: &Pixmap, angle: f32, length: f32, salt: usize) -> Pixmap {
    let (w, h) = (picture.width() as usize, picture.height() as usize);
    let radians = angle.to_radians();
    let along = (radians.cos(), -radians.sin());
    let mut drag = crate::filters::artistic::streaked_noise(w, h, length + 3.0, salt, along);
    // Neighbours share a drag, so a stroke is a few pixels wide rather than a
    // hairline.
    blur_field(&mut drag, w, h, ANGLED_WIDTH);
    unit_spread(&mut drag);
    let mut dragged = picture.clone();
    dragged
        .as_bytes_mut()
        .par_chunks_exact_mut(w * 4)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let offset = drag[y * w + x].clamp(-2.0, 2.0) * length * ANGLED_DRAG * 0.5;
                let sx = (x as f32 + along.0 * offset).round().clamp(0.0, (w - 1) as f32) as usize;
                let sy = (y as f32 + along.1 * offset).round().clamp(0.0, (h - 1) as f32) as usize;
                let from = (sy * w + sx) * 4;
                px[..3].copy_from_slice(&picture.as_bytes()[from..from + 3]);
            }
        });
    median_along(&dragged, along, (length * ANGLED_SMEAR).max(1.0))
}

/// Fine straight lines along `angle`, about a stroke and a half long, a
/// little over a pixel thick, with a spread of one.
fn bristle_lines(w: usize, h: usize, angle: f32, length: f32, salt: usize) -> Vec<f32> {
    bristle_lines_width(w, h, angle, length, salt, ANGLED_BRISTLE_WIDTH)
}

fn bristle_lines_width(w: usize, h: usize, angle: f32, length: f32, salt: usize, width: f32) -> Vec<f32> {
    let radians = angle.to_radians();
    let mut lines =
        crate::filters::artistic::streaked_noise(w, h, length * 1.5, salt, (radians.cos(), -radians.sin()));
    if width > 0.0 {
        blur_field(&mut lines, w, h, width);
    }
    unit_spread(&mut lines);
    lines
}

pub(crate) fn unit_spread(field: &mut [f32]) {
    let spread = (field.par_iter().map(|v| v * v).sum::<f32>() / field.len().max(1) as f32).sqrt();
    if spread > 0.0 {
        field.par_iter_mut().for_each(|v| *v /= spread);
    }
}

/// CS6's ranges for Crosshatch, which its three sliders run over.
pub const HATCH_LENGTH: std::ops::RangeInclusive<u32> = 3..=50;
pub const HATCH_SHARPNESS: std::ops::RangeInclusive<u32> = 0..=20;
pub const HATCH_STRENGTH: std::ops::RangeInclusive<u32> = 1..=3;

/// How strong the hatching lines are, in levels, where the picture is busy.
///
/// **The hatching follows texture, not tone.** CS6 lays it plainly over
/// spray and broken water, faintly over the dark horse, and not at all over a
/// clear sky: pencil catches where there is something to catch. So the lines'
/// strength is set by how much detail is round a pixel — [`HATCH_DETAIL`] —
/// and muted in the dark, where a line has little to show against.
const HATCH_LINES: f32 = 45.0;

/// How much detail, in levels of difference from a small blur averaged over a
/// few pixels, counts as fully busy; the scale of that blur and of the
/// average; and how much line a completely smooth area still gets.
const HATCH_DETAIL: f32 = 10.0;
const HATCH_DETAIL_SCALE: f32 = 1.5;
const HATCH_DETAIL_REACH: f32 = 4.0;
const HATCH_DETAIL_FLOOR: f32 = 0.05;

/// How much of the lines' strength the darkest tone keeps.
const HATCH_DARK_MUTE: f32 = 0.35;

/// What share of the diagonal segments carry a line, and how much longer than
/// Stroke Length a line is.
const HATCH_DENSITY: f32 = 0.22;
const HATCH_LINE_LENGTH: f32 = 1.6;

/// How many pixels apart the three channels' lines are at the top of
/// Sharpness.
const HATCH_FRINGE: f32 = 1.0;

/// How much stronger the lines are on each pass after the first.
const HATCH_PER_PASS: f32 = 0.3;

/// How much heavier the lines are per step of Sharpness.
const HATCH_LINES_PER_SHARP: f32 = 0.05;

/// Sharpness: a per-channel unsharp mask, its scale in pixels and strength
/// per step. Per channel on purpose: CS6's high settings, repeated by
/// Strength, amplify the colour differences between lines into the rainbow
/// hatching its screenshots show.
const HATCH_SHARP_SCALE: f32 = 1.0;
const HATCH_SHARP_PER_STEP: f32 = 0.08;

/// The difference in levels below which Sharpness leaves a channel alone, so
/// the faint ridges of a smooth sky are not sharpened into hatching.
const HATCH_SHARP_FROM: f32 = 5.0;

/// Filter ▸ Brush Strokes ▸ Crosshatch: the picture drawn over in strokes
/// running both ways at once.
///
/// Each pass lays the picture in strokes along both diagonals — the same
/// strokes as Angled Strokes — takes the mean of the two, draws fine lines
/// both ways over it, and sharpens it. **Stroke Length** is how long the
/// strokes are, **Sharpness** how hard they are sharpened, and **Strength**
/// how many passes: each one works over what the last one left, so at 3 with
/// a high Sharpness the picture is lost under the hatching.
///
/// Alpha is left alone.
///
/// No GPU path, for Angled Strokes' reasons.
pub fn crosshatch(pixmap: &mut Pixmap, length: u32, sharpness: u32, strength: u32) {
    if pixmap.is_empty() {
        return;
    }
    let length = length.clamp(*HATCH_LENGTH.start(), *HATCH_LENGTH.end()) as f32;
    let sharpness = sharpness.clamp(*HATCH_SHARPNESS.start(), *HATCH_SHARPNESS.end()) as f32;
    let strength = strength.clamp(*HATCH_STRENGTH.start(), *HATCH_STRENGTH.end());
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);

    for pass in 0..strength as usize {
        let salt = 40 + pass * 4;
        let rising = lay_strokes(pixmap, 45.0, length, salt);
        let falling = lay_strokes(pixmap, -45.0, length, salt + 1);
        // Hairlines, unsoftened, and cut hard: pen, not charcoal.
        let up = pen_lines(w, h, true, length, salt + 2);
        let down = pen_lines(w, h, false, length, salt + 3);
        // At high Sharpness each channel reads its lines a pixel or so off
        // the others, and the lines come apart into CS6's colour fringes.
        let fringe = (sharpness / *HATCH_SHARPNESS.end() as f32 * HATCH_FRINGE).round() as isize;
        let detail = busyness(pixmap);
        let pass_gain = 1.0 + pass as f32 * HATCH_PER_PASS;
        let (up, down, detail) = (&up, &down, &detail);
        let stride = pixmap.stride();
        pixmap
            .as_bytes_mut()
            .par_chunks_exact_mut(stride)
            .zip(rising.as_bytes().par_chunks_exact(stride))
            .zip(falling.as_bytes().par_chunks_exact(stride))
            .enumerate()
            .for_each(|(y, ((out, r), f))| {
                for (x, ((px, r), f)) in out
                    .chunks_exact_mut(4)
                    .zip(r.chunks_exact(4))
                    .zip(f.chunks_exact(4))
                    .enumerate()
                {
                    let mut mean = [0.0f32; 3];
                    for c in 0..3 {
                        mean[c] = (r[c] as f32 + f[c] as f32) * 0.5;
                    }
                    let lightness = (0.299 * mean[0] + 0.587 * mean[1] + 0.114 * mean[2]) / 255.0;
                    let i = y * w + x;
                    let busy = HATCH_DETAIL_FLOOR + (1.0 - HATCH_DETAIL_FLOOR) * detail[i];
                    let amp = HATCH_LINES
                        * busy
                        * (HATCH_DARK_MUTE + (1.0 - HATCH_DARK_MUTE) * lightness)
                        * (1.0 + sharpness * HATCH_LINES_PER_SHARP)
                        * pass_gain;
                    // Each line has a pale side and a dark side. In the light
                    // the dark side shows, as pencil on paper; in the dark,
                    // the pale side, as chalk.
                    for c in 0..3 {
                        // Shifted across the lines, which run diagonally, so
                        // a shift along x moves them.
                        let shift = (c as isize - 1) * fringe;
                        let xs = (x as isize + shift).clamp(0, w as isize - 1) as usize;
                        let j = y * w + xs;
                        let pale = up[j].max(0.0) + down[j].max(0.0);
                        let shade = -(up[j].min(0.0) + down[j].min(0.0));
                        let line = (pale * (1.0 - lightness) - shade * lightness) * amp;
                        px[c] = (mean[c] + line).round().clamp(0.0, 255.0) as u8;
                    }
                    // Alpha stands: hatching the picture does not change the
                    // layer's shape.
                }
            });

        let gain = sharpness * HATCH_SHARP_PER_STEP;
        // The threshold falls away as Sharpness rises: at the top of the
        // slider every difference is sharpened, and the picture goes hard.
        let threshold = (HATCH_SHARP_FROM * (1.0 - sharpness / *HATCH_SHARPNESS.end() as f32)).max(2.0);
        if gain > 0.0 {
            let sharp = pixmap.clone();
            let mut soft = pixmap.clone();
            crate::filters::convolve::gaussian_blur_accelerated(&mut soft, HATCH_SHARP_SCALE);
            pixmap
                .as_bytes_mut()
                .par_chunks_exact_mut(4)
                .zip(sharp.as_bytes().par_chunks_exact(4))
                .zip(soft.as_bytes().par_chunks_exact(4))
                .for_each(|((px, s), b)| {
                    for c in 0..3 {
                        let diff = s[c] as f32 - b[c] as f32;
                        let over = (diff.abs() - threshold).max(0.0) * diff.signum();
                        let v = s[c] as f32 + over * gain;
                        px[c] = v.round().clamp(0.0, 255.0) as u8;
                    }
                });
        }
    }
}

/// Pen lines: one pixel wide, straight, along the rising (`/`) or falling
/// (`\`) diagonal, each a segment of a diagonal chosen at random, so they
/// stand apart with gaps between — rather than a streak on every diagonal,
/// which reads as a screen. Positive values are one side of the pen, negative
/// the other; 0 is no line.
fn pen_lines(w: usize, h: usize, rising: bool, length: f32, salt: usize) -> Vec<f32> {
    let span = (length * HATCH_LINE_LENGTH).max(3.0);
    let hash = |a: i64, b: i64| {
        let mut v = (a as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (b as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
            ^ (salt as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
        v ^= v >> 31;
        v = v.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        v ^= v >> 29;
        (v >> 11) as f32 / (1u64 << 53) as f32
    };
    let mut out = vec![0.0f32; w * h];
    out.par_chunks_exact_mut(w).enumerate().for_each(|(y, row)| {
        for (x, slot) in row.iter_mut().enumerate() {
            let (x, y) = (x as i64, y as i64);
            let (diagonal, along) = if rising { (x + y, x - y) } else { (x - y, x + y) };
            // Each diagonal is cut at its own offset, so the ends do not line
            // up across the picture.
            let offset = hash(diagonal, -1) * span;
            let segment = ((along as f32 + offset) / span).floor() as i64;
            let pick = hash(diagonal, segment);
            if pick < HATCH_DENSITY {
                let side = if hash(diagonal, segment + 7919) < 0.5 { -1.0 } else { 1.0 };
                *slot = side * (0.6 + 0.4 * pick / HATCH_DENSITY);
            }
        }
    });
    out
}

/// How busy the picture is round each pixel, 0 smooth to 1 fully busy: its
/// difference from a small blur, averaged over a few pixels.
fn busyness(picture: &Pixmap) -> Vec<f32> {
    let (w, h) = (picture.width() as usize, picture.height() as usize);
    let mut soft = picture.clone();
    crate::filters::convolve::gaussian_blur_accelerated(&mut soft, HATCH_DETAIL_SCALE);
    let luma = |p: &[u8]| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
    let mut detail: Vec<f32> = picture
        .as_bytes()
        .par_chunks_exact(4)
        .zip(soft.as_bytes().par_chunks_exact(4))
        .map(|(p, s)| (luma(p) - luma(s)).abs())
        .collect();
    blur_field(&mut detail, w, h, HATCH_DETAIL_REACH);
    detail.par_iter_mut().for_each(|d| *d = (*d / HATCH_DETAIL).min(1.0));
    detail
}

/// CS6's ranges for Dark Strokes, which its three sliders run over.
pub const DARK_BALANCE: std::ops::RangeInclusive<u32> = 0..=10;
pub const DARK_BLACK: std::ops::RangeInclusive<u32> = 0..=10;
pub const DARK_WHITE: std::ops::RangeInclusive<u32> = 0..=10;

/// The strokes: long ones in the light, short ones in the dark, in pixels.
const DARK_LONG: f32 = 14.0;
const DARK_SHORT: f32 = 5.0;

/// Where the picture goes to black, as a fraction of white: a floor, and how
/// far each step of Black Intensity and of Balance raises it; and how soft
/// the fall is. At the top of both, everything short of the sky and the spray
/// is black, as CS6's is.
const DARK_BLACK_FROM: f32 = 0.12;
const DARK_BLACK_PER_STEP: f32 = 0.037;
const DARK_BALANCE_PER_STEP: f32 = 0.028;
const DARK_BLACK_SOFT: f32 = 0.08;

/// Where the lights start going to white, as a fraction of white at White
/// Intensity 0, how far down each step takes it, and how far towards white
/// the lightest tone is carried at 10.
const DARK_WHITE_FROM: f32 = 0.85;
const DARK_WHITE_PER_STEP: f32 = 0.03;
const DARK_WHITE_LIFT: f32 = 0.85;

/// How much each step of Black Intensity deepens the midtones, as a power.
const DARK_DEEPEN_PER_STEP: f32 = 0.4;

/// How much the tones between are pushed apart: a contrast gain at full
/// Black and White Intensity together.
const DARK_CONTRAST: f32 = 0.35;

/// Filter ▸ Brush Strokes ▸ Dark Strokes: the dark parts of the picture
/// painted in short strokes and driven to black, the light parts in long
/// strokes the other way and driven to white.
///
/// The strokes are Angled Strokes': the dark run down to the right, short;
/// the light run up to the right, long. Then **Black Intensity** and
/// **Balance** decide how much of the picture goes to black — the line rises
/// with both — and **White Intensity** how far the lights are carried to
/// white. Because the black is decided on the stroked picture, its edge is
/// cut into strokes, which is the ragged diagonal edge CS6 shows.
///
/// Alpha is left alone.
///
/// No GPU path, for Angled Strokes' reasons.
pub fn dark_strokes(pixmap: &mut Pixmap, balance: u32, black: u32, white: u32) {
    if pixmap.is_empty() {
        return;
    }
    let balance = balance.clamp(*DARK_BALANCE.start(), *DARK_BALANCE.end()) as f32;
    let black = black.clamp(*DARK_BLACK.start(), *DARK_BLACK.end()) as f32;
    let white = white.clamp(*DARK_WHITE.start(), *DARK_WHITE.end()) as f32;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);

    let light_strokes = lay_strokes(pixmap, 45.0, DARK_LONG, 61);
    let dark_strokes = lay_strokes(pixmap, -45.0, DARK_SHORT, 62);

    let luma = |p: &[u8]| (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) / 255.0;
    let mut judged: Vec<f32> = pixmap.as_bytes().par_chunks_exact(4).map(luma).collect();
    blur_field(&mut judged, w, h, ANGLED_JUDGE);
    let judged = &judged;

    let black_from = DARK_BLACK_FROM + black * DARK_BLACK_PER_STEP + balance * DARK_BALANCE_PER_STEP;
    let white_from = DARK_WHITE_FROM - white * DARK_WHITE_PER_STEP;
    let lift = white / 10.0 * DARK_WHITE_LIFT;
    let contrast = 1.0 + (black + white) / 20.0 * DARK_CONTRAST;
    let pivot = (black_from + white_from) * 0.5 * 255.0;

    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .zip(light_strokes.as_bytes().par_chunks_exact(stride))
        .zip(dark_strokes.as_bytes().par_chunks_exact(stride))
        .enumerate()
        .for_each(|(y, ((out, l), d))| {
            for (x, ((px, l), d)) in out
                .chunks_exact_mut(4)
                .zip(l.chunks_exact(4))
                .zip(d.chunks_exact(4))
                .enumerate()
            {
                let t = ((judged[y * w + x] - 0.5) / ANGLED_SWITCH * 0.5 + 0.5).clamp(0.0, 1.0);
                let lit = t * t * (3.0 - 2.0 * t);
                let mut paint = [0.0f32; 3];
                for c in 0..3 {
                    let v = l[c] as f32 * lit + d[c] as f32 * (1.0 - lit);
                    paint[c] = ((v - pivot) * contrast + pivot).clamp(0.0, 255.0);
                }
                let level = (0.299 * paint[0] + 0.587 * paint[1] + 0.114 * paint[2]) / 255.0;
                // The midtones deepened on the way down: a power on the
                // tone, fading out towards the lights.
                let below = (1.0 - level / white_from.max(0.01)).clamp(0.0, 1.0);
                let deepen = level.max(0.01).powf(black * DARK_DEEPEN_PER_STEP * below);
                for v in paint.iter_mut() {
                    *v *= deepen;
                }
                let level = level * deepen;
                let k = ((level - black_from) / DARK_BLACK_SOFT * 0.5 + 0.5).clamp(0.0, 1.0);
                let kept = k * k * (3.0 - 2.0 * k);
                let u = ((level - white_from) / (1.0 - white_from).max(0.01)).clamp(0.0, 1.0);
                let whiten = u * lift;
                for c in 0..3 {
                    let v = paint[c] * kept;
                    px[c] = (v + (255.0 - v) * whiten).round().clamp(0.0, 255.0) as u8;
                }
                // Alpha stands: restroking the picture does not change the
                // layer's shape.
            }
        });
}

/// CS6's ranges for Ink Outlines, which its three sliders run over.
pub const INK_LENGTH: std::ops::RangeInclusive<u32> = 0..=50;
pub const INK_DARK: std::ops::RangeInclusive<u32> = 0..=50;
pub const INK_LIGHT: std::ops::RangeInclusive<u32> = 0..=50;

/// How long a stroke is, in pixels: a floor and a step of Stroke Length.
const INK_LINE: f32 = 4.0;
const INK_LINE_PER_STEP: f32 = 0.5;

/// The scale in pixels at which the picture's detail is read as something to
/// draw, and how wide a reach decides whether a place is in shadow.
const INK_DETAIL_SCALE: f32 = 1.3;
const INK_BROAD_SCALE: f32 = 10.0;

/// Which way the strokes run, in degrees anticlockwise from the horizontal.
/// Light and dark run the opposite way to each other — see [`ink_outlines`].
const INK_LIGHT_ANGLE: f32 = 45.0;
const INK_DARK_ANGLE: f32 = -45.0;

/// Which marks are drawn at all, in levels of detail: below the first nothing
/// is drawn, above the second the mark is drawn in full.
const INK_MARK_FROM: f32 = 1.0;
const INK_MARK_FULL: f32 = 5.0;

/// How much of a full mark a step of Dark Intensity inks, and a step of Light
/// Intensity draws in white.
const INK_PER_STEP: f32 = 0.04;
const INK_CHALK_PER_STEP: f32 = 0.02;

/// The outline: how far the tone is softened before its edges are found — a
/// pixel or less, so the line comes out thin — how strong an edge must be, in
/// Sobel units, before a line is drawn round it and where it is drawn in
/// full, and how black it is drawn at Dark Intensity 0 and per step.
const INK_OUTLINE_SOFT: f32 = 0.8;
const INK_OUTLINE_FROM: f32 = 6.0;
const INK_OUTLINE_FULL: f32 = 26.0;
const INK_OUTLINE_FLOOR: f32 = 0.55;
const INK_OUTLINE_PER_STEP: f32 = 0.009;

/// The rim: how white it is at Light Intensity 0 and per step, and how wide
/// the ring outside a filled shape is, in pixels.
const INK_RIM_FLOOR: f32 = 0.6;
const INK_RIM_PER_STEP: f32 = 0.011;
const INK_RIM_WIDTH: usize = 2;
const INK_RIM_CLOSE: usize = 26;

/// How wide a reach decides whether a place is within a dark mass. Wider
/// than the shadow's own: a gloss can be twenty pixels across and still be a
/// gloss on something black.
const INK_WITHIN_SCALE: f32 = 26.0;

/// How far the paint is taken down inside a dark mass, so that what is lit
/// within it reads as a mark on black rather than as a hole in it.
const INK_INSIDE_DAMP: f32 = 0.95;

/// How much darker again the broad reach must be for a place to count as a
/// dark mass rather than a dark line.
const INK_BROAD_ALLOW: f32 = 0.06;

/// How dark a place must be, as a fraction of white, before it fills solid:
/// at Dark Intensity 0 and per step. The shadow is judged over a wide reach,
/// so a lit flank inside a dark body fills with the body.
const INK_BLACK_FROM: f32 = 0.08;
const INK_BLACK_PER_STEP: f32 = 0.008;
const INK_EDGE_SOFT: f32 = 0.10;

/// How far the two Intensity sliders together drive the picture's contrast at
/// the top of both, and the tone it turns about.
const INK_CONTRAST: f32 = 1.1;
const INK_PIVOT: f32 = 140.0;

/// Filter ▸ Brush Strokes ▸ Ink Outlines: the picture redrawn in fine narrow
/// diagonal pen lines.
///
/// Adobe's own account of it is the shape of this: *"repaints lighter and
/// darker areas using strokes that move in opposite, diagonal directions"*,
/// with fine narrow diagonal lines drawn over the original detail.
///
/// 1. **The repaint.** The picture is laid in strokes twice, up to the right
///    and down to the right, and each pixel takes the one its tone calls for:
///    the light parts one way, the dark parts the other. That is the paint
///    under the pen, and it is why the sea comes back in diagonal bands.
/// 2. **The pen.** The detail the repaint left over is drawn along those same
///    diagonals as narrow lines: dark where the picture falls away, by
///    **Dark Intensity**, and white where it rises, by **Light Intensity**.
/// 3. **The fill.** What is deeply in shadow fills solid, so a dark subject
///    reads as a black shape with a drawn edge.
///
/// **Stroke Length** is how long the strokes and the lines are.
///
/// Alpha is left alone.
///
/// No GPU path, for Angled Strokes' reasons.
pub fn ink_outlines(pixmap: &mut Pixmap, length: u32, dark: u32, light: u32) {
    if pixmap.is_empty() {
        return;
    }
    let length = length.clamp(*INK_LENGTH.start(), *INK_LENGTH.end()) as f32;
    let dark = dark.clamp(*INK_DARK.start(), *INK_DARK.end()) as f32;
    let light = light.clamp(*INK_LIGHT.start(), *INK_LIGHT.end()) as f32;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    let stroke = INK_LINE + length * INK_LINE_PER_STEP;

    let luma = |p: &[u8]| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
    let tone: Vec<f32> = pixmap.as_bytes().par_chunks_exact(4).map(luma).collect();
    let mut judged = tone.clone();
    blur_field(&mut judged, w, h, ANGLED_JUDGE);
    // How dark it is hereabouts, over a wide reach.
    let mut broad = tone.clone();
    blur_field(&mut broad, w, h, INK_BROAD_SCALE);

    // 1: the repaint, in strokes running opposite ways.
    let rising = lay_strokes(pixmap, INK_LIGHT_ANGLE, stroke, 81);
    let falling = lay_strokes(pixmap, INK_DARK_ANGLE, stroke, 82);

    // 2: the detail the repaint left over, drawn along each diagonal.
    let mut soft = tone.clone();
    blur_field(&mut soft, w, h, INK_DETAIL_SCALE);
    let detail: Vec<f32> = tone.iter().zip(&soft).map(|(t, s)| t - s).collect();
    let up = streak_field(&detail, w, h, INK_LIGHT_ANGLE, stroke);
    let down = streak_field(&detail, w, h, INK_DARK_ANGLE, stroke);

    // The outline proper: where the picture turns, not where it is merely
    // uneven. The fine detail above draws the hatching inside a shape; this
    // draws the line round it, which is what the filter is named for.
    // Found on a lightly softened tone and left unsoftened afterwards: a
    // blurred edge map draws a smear where CS6 draws a line. This is the
    // filter's name, so it is drawn at nearly full strength whatever the
    // sliders say — they set how much *else* is drawn.
    let mut edges = tone.clone();
    blur_field(&mut edges, w, h, INK_OUTLINE_SOFT);
    let outline = sobel(&edges, w, h);

    let ink_gain = dark * INK_PER_STEP;
    let outline_gain = (INK_OUTLINE_FLOOR + dark * INK_OUTLINE_PER_STEP).min(1.0);
    let rim_gain = (INK_RIM_FLOOR + light * INK_RIM_PER_STEP).min(1.0);
    let chalk_gain = light * INK_CHALK_PER_STEP;
    let black_from = INK_BLACK_FROM + dark * INK_BLACK_PER_STEP;
    let contrast = 1.0 + (dark + light) / 100.0 * INK_CONTRAST;
    // The shape the ink fills, and the ring of ground just outside it. CS6
    // traces a white line right round a filled shape, and nothing tells where
    // that line goes as plainly as the shape's own edge: judged on tone
    // alone, a lit flank inside the shape looks exactly like the ground
    // beside it.
    // The shape the ink fills. Judged on the sharp tone, not on a blur of
    // it: a blurred tone gives a soft, spreading edge where CS6 cuts a crisp
    // silhouette. The holes the lit parts of a dark subject leave are stopped
    // up afterwards, by spreading the mask and pulling it back, which leaves
    // the outside edge exactly where it was.
    let dark_mask = |t: f32, from: f32| {
        let s = ((t / 255.0 - from) / INK_EDGE_SOFT * 0.5 + 0.5).clamp(0.0, 1.0);
        1.0 - s * s * (3.0 - 2.0 * s)
    };
    // Dark here *and* dark hereabouts. The sharp tone puts the edge exactly
    // where the subject's edge is; the broad tone says whether this is a dark
    // mass at all, which a thin dark band of sea is not.
    let shape: Vec<f32> = judged
        .par_iter()
        .zip(broad.par_iter())
        .map(|(t, b)| dark_mask(*t, black_from).min(dark_mask(*b, black_from + INK_BROAD_ALLOW)))
        .collect();
    let shape = close_gaps(&shape, w, h, INK_RIM_CLOSE as f32);
    // Whatever is enclosed by the shape belongs to it, however brightly it is
    // lit: a gloss in the middle of a black flank is not a hole in the horse.
    // Nothing local can tell that — the gloss is broad and bright on every
    // measure — so it is settled by reaching in from the frame instead.
    let shape = fill_holes(&shape, w, h);
    // How much of a dark mass a place is part of, however brightly it is lit
    // itself. A gloss on a black flank is still black flank, and CS6 keeps it
    // as a thin light mark on the black rather than as a white blot.
    let mut wider = judged.clone();
    blur_field(&mut wider, w, h, INK_WITHIN_SCALE);
    let within: Vec<f32> = wider
        .par_iter()
        .map(|b| dark_mask(*b, black_from + INK_BROAD_ALLOW))
        .collect();
    let within = &within;

    // The rim: the ring just inside the shape's edge, drawn white. Outside
    // it would sit on pale water and never be seen.
    let mut inner = shape.clone();
    inner.par_iter_mut().for_each(|v| *v = -*v);
    let eroded = widest_nearby_field(&inner, w, h, INK_RIM_WIDTH);
    let ring: Vec<f32> = shape
        .par_iter()
        .zip(eroded.par_iter())
        .map(|(s, e)| (s - (-e)).max(0.0))
        .collect();

    let (judged, broad, up, down, outline) = (&judged, &broad, &up, &down, &outline);
    let (shape, ring) = (&shape, &ring);

    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .zip(rising.as_bytes().par_chunks_exact(stride))
        .zip(falling.as_bytes().par_chunks_exact(stride))
        .enumerate()
        .for_each(|(y, ((out, r), f))| {
            for (x, ((px, r), f)) in out
                .chunks_exact_mut(4)
                .zip(r.chunks_exact(4))
                .zip(f.chunks_exact(4))
                .enumerate()
            {
                let i = y * w + x;
                // Light takes the rising stroke, dark the falling one.
                let t = ((judged[i] / 255.0 - 0.5) / ANGLED_SWITCH * 0.5 + 0.5).clamp(0.0, 1.0);
                let lit = t * t * (3.0 - 2.0 * t);

                let mark = |v: f32| {
                    let t = ((v - INK_MARK_FROM) / (INK_MARK_FULL - INK_MARK_FROM)).clamp(0.0, 1.0);
                    t * t * (3.0 - 2.0 * t)
                };
                // The pen follows the stroke the paint was laid with: the
                // dark line down to the right, the light line up to the left.
                let edge = ((outline[i] - INK_OUTLINE_FROM)
                    / (INK_OUTLINE_FULL - INK_OUTLINE_FROM))
                    .clamp(0.0, 1.0);
                let drawn_edge = edge * edge * (3.0 - 2.0 * edge);
                // Which side of the boundary this pixel is on decides which
                // pen draws it: the dark side is inked, the light side is
                // drawn in white. That is CS6's crisp white rim round a black
                // shape, and the black line round a bright one.
                let side = judged[i] - broad[i];
                let dark_side = if side < 0.0 { drawn_edge } else { 0.0 };
                let light_side = if side > 0.0 { drawn_edge } else { 0.0 };
                let inked = ((mark(-down[i]) * ink_gain).max(dark_side * outline_gain)).min(1.0);
                // White on the ring outside a filled shape — the rim — and
                // wherever the pen drew a light mark on light ground.
                let rim = ring[i] * rim_gain;
                // Held back inside a dark mass, both of them: the lit edge of
                // a gloss on a black flank is a strong edge like any other,
                // and drawn white it puts a white blot in the middle of the
                // silhouette.
                let outside = 1.0 - within[i];
                let chalked = ((mark(up[i]) * chalk_gain * outside)
                    .max(light_side * rim_gain * outside)
                    .max(rim))
                    .min(1.0);
                let filled = 1.0 - shape[i];

                let damped = 1.0 - INK_INSIDE_DAMP * within[i];
                for c in 0..3 {
                    let paint = r[c] as f32 * lit + f[c] as f32 * (1.0 - lit);
                    let paint = ((paint - INK_PIVOT) * contrast + INK_PIVOT).clamp(0.0, 255.0);
                    let paint = paint * damped;
                    let v = paint * filled * (1.0 - inked);
                    // The rim is blocked inside a filled shape: it belongs on
                    // the water beside the horse, not on the horse's own lit
                    // flanks, which CS6 leaves solid black.
                    px[c] = (v + (255.0 - v) * chalked).round().clamp(0.0, 255.0) as u8;
                }
                // Alpha stands: drawing over the picture does not change the
                // layer's shape.
            }
        });
}

/// CS6's ranges for Spatter, which its two sliders run over.
pub const SPATTER_RADIUS: std::ops::RangeInclusive<u32> = 0..=25;
pub const SPATTER_SMOOTHNESS: std::ops::RangeInclusive<u32> = 1..=15;

/// How far the spray throws a pixel, in pixels per step of Spray Radius.
///
/// Well under a pixel a step: the slider's top of 25 is meant to be drastic,
/// not unrecognisable, and a throw as wide as the slider number scrambles a
/// wave into fog long before it gets there.
const SPATTER_THROW: f32 = 0.45;

/// How large the clustering window is, in pixels either side: a floor and a
/// step of Smoothness. Small windows leave the grit sharp; large ones gather
/// the scattered pixels into rounded droplets.
///
/// It has to be a good fraction of the throw or the picture turns to mush:
/// the median is what puts a hard edge back round each droplet, and a window
/// much smaller than the spray never finds a dominant colour to settle on.
/// It barely widens with Smoothness. It is tempting to make Smoothness size
/// this window, but CS6's high settings come out *finer* than its low ones,
/// not blobbier — a wide median eats the bridle and the eye, which CS6 still
/// shows at 12. Smoothness is spent on [`SPATTER_COHERENCE`] instead.
const SPATTER_CLUSTER: f32 = 1.0;
const SPATTER_CLUSTER_PER_STEP: f32 = 0.08;

/// How far the throw is softened so that neighbouring pixels are thrown
/// together rather than each its own way, in pixels: a floor and a step of
/// Smoothness.
///
/// This is Smoothness. Near the bottom the two fields are nearly white noise
/// and every pixel goes its own way, which is the sharp, jagged, gritty end
/// of the slider; near the top the throw varies slowly across the picture and
/// whole clumps move together, which is the soft, organic, droplet end.
const SPATTER_COHERENCE: f32 = 0.4;
const SPATTER_COHERENCE_PER_STEP: f32 = 0.15;

/// How far the picture is softened before its edges are measured, and how
/// much a full edge holds the spray back. Some hold keeps a subject's
/// silhouette readable; too much and the outline never breaks up, which is
/// the one thing the filter is for.
const SPATTER_EDGE_SCALE: f32 = 2.0;
const SPATTER_EDGE_FROM: f32 = 10.0;
const SPATTER_EDGE_FULL: f32 = 60.0;
const SPATTER_EDGE_HOLD: f32 = 0.3;

/// Filter ▸ Brush Strokes ▸ Spatter: the picture as an airbrush would spatter
/// it.
///
/// Three stages, which are the filter as Adobe describes it:
///
/// 1. **The spray.** Each pixel takes its colour from another one thrown off
///    it, up to **Spray Radius** in each direction. The two fields that say
///    which way are softened first, by **Smoothness**, so that neighbours are
///    thrown together: at the bottom of the slider they are near white noise
///    and the picture comes apart into sharp grit, at the top they vary
///    slowly and whole clumps of colour move as one, which is what tears an
///    outline into the tongues and shards the filter is named for.
/// 2. **The clustering.** The sprayed pixels are settled by a median over a
///    small window, which puts a hard edge back round each droplet. A *whole*
///    pixel, not a median per channel — see [`median_pixel`].
/// 3. **Holding the edges.** The throw is damped where the picture has a
///    strong boundary, so the spray stylises an outline instead of eating it.
///
/// At Spray Radius 0 the picture is left exactly as it was.
///
/// Alpha is left alone.
///
/// No GPU path: the clustering is a median, sequential along each row.
pub fn spatter(pixmap: &mut Pixmap, radius: u32, smoothness: u32) {
    if pixmap.is_empty() {
        return;
    }
    let radius = radius.clamp(*SPATTER_RADIUS.start(), *SPATTER_RADIUS.end()) as f32;
    if radius == 0.0 {
        // No spray, so nothing to gather: Smoothness alone is not a filter.
        // Running the clustering anyway would quietly median the picture at
        // the bottom of the slider, where CS6 leaves it alone.
        return;
    }
    let smoothness = smoothness.clamp(*SPATTER_SMOOTHNESS.start(), *SPATTER_SMOOTHNESS.end()) as f32;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);

    let luma = |p: &[u8]| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
    let mut tone: Vec<f32> = pixmap.as_bytes().par_chunks_exact(4).map(luma).collect();
    blur_field(&mut tone, w, h, SPATTER_EDGE_SCALE);
    let edges = sobel(&tone, w, h);

    // Which way each pixel is thrown. Not independent per pixel: a pixel that
    // goes its own way leaves a one-pixel fuzz, where CS6 throws *clumps* —
    // whole tongues of colour several pixels wide torn off an edge. Softening
    // the two fields makes neighbours agree over a few pixels, which is what
    // gives the spray its shards, and the softening widens with Smoothness.
    let coherence = SPATTER_COHERENCE + smoothness * SPATTER_COHERENCE_PER_STEP;
    let mut aside: Vec<f32> = (0..w * h)
        .into_par_iter()
        .map(|i| noise((i % w) as i32, (i / w) as i32) * 2.0 - 1.0)
        .collect();
    let mut down: Vec<f32> = (0..w * h)
        .into_par_iter()
        .map(|i| noise((i % w) as i32, (i / w + h + 977) as i32) * 2.0 - 1.0)
        .collect();
    for field in [&mut aside, &mut down] {
        blur_field(field, w, h, coherence);
        // The blur flattens the field towards nothing; this puts its spread
        // back, so Smoothness changes the size of the clumps and not how far
        // the spray reaches.
        unit_spread(field);
    }

    let throw = radius * SPATTER_THROW;
    let source = pixmap.clone();
    let (edges, src) = (&edges, &source);
    let (aside, down) = (&aside, &down);
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(w * 4)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let i = y * w + x;
                let edge = ((edges[i] - SPATTER_EDGE_FROM)
                    / (SPATTER_EDGE_FULL - SPATTER_EDGE_FROM))
                    .clamp(0.0, 1.0);
                let reach = throw * (1.0 - SPATTER_EDGE_HOLD * edge);
                let away = |v: f32| v.clamp(-2.0, 2.0) * reach;
                let sx = (x as f32 + away(aside[i])).round().clamp(0.0, (w - 1) as f32) as usize;
                let sy = (y as f32 + away(down[i])).round().clamp(0.0, (h - 1) as f32) as usize;
                let from = (sy * w + sx) * 4;
                px[..3].copy_from_slice(&src.as_bytes()[from..from + 3]);
                // Alpha stands: spattering the picture does not change the
                // layer's shape.
            }
        });

    let cluster = (SPATTER_CLUSTER + smoothness * SPATTER_CLUSTER_PER_STEP).round() as i32;
    if cluster > 0 {
        let gathered = median_pixel(pixmap, cluster);
        for (out, from) in pixmap.as_bytes_mut().chunks_exact_mut(4).zip(gathered.as_bytes().chunks_exact(4)) {
            out[..3].copy_from_slice(&from[..3]);
        }
    }
}

/// CS6's ranges for Sprayed Strokes, which its two sliders run over.
pub const SPRAYED_LENGTH: std::ops::RangeInclusive<u32> = 0..=20;
pub const SPRAYED_RADIUS: std::ops::RangeInclusive<u32> = 0..=25;

/// Which way the spray runs, in the order CS6's Stroke Direction lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrokeDirection {
    #[default]
    RightDiagonal,
    Horizontal,
    LeftDiagonal,
    Vertical,
}

impl StrokeDirection {
    pub fn from_i32(value: i32) -> StrokeDirection {
        match value {
            1 => StrokeDirection::Horizontal,
            2 => StrokeDirection::LeftDiagonal,
            3 => StrokeDirection::Vertical,
            _ => StrokeDirection::RightDiagonal,
        }
    }

    /// The angle the strokes run along, in degrees anticlockwise from the
    /// horizontal — the convention [`lay_strokes`] and [`streak_field`] use.
    pub(crate) fn angle(self) -> f32 {
        match self {
            StrokeDirection::RightDiagonal => 45.0,
            StrokeDirection::Horizontal => 0.0,
            StrokeDirection::LeftDiagonal => 135.0,
            StrokeDirection::Vertical => 90.0,
        }
    }
}

/// How far the spray throws a pixel along the stroke, in pixels per step of
/// Spray Radius. It is thrown *along* the axis and not across it: that is
/// what makes Vertical comb the picture into long fingers rather than simply
/// roughening it.
const SPRAYED_SCATTER: f32 = 0.4;

/// How wide a tooth of the comb is, as a blur across the noise that throws
/// it. Much narrower than Angled Strokes' brush — CS6's spray separates into
/// fine threads, not into slabs.
const SPRAYED_TOOTH: f32 = 0.5;

/// How long the noise stays with itself along the stroke, as a multiple of
/// Stroke Length with a floor in pixels. A pixel and its neighbour up the
/// stroke are thrown together; its neighbour across the stroke is not.
const SPRAYED_RUN: f32 = 1.0;
const SPRAYED_RUN_FLOOR: f32 = 5.0;

/// How far the colour is then settled along the stroke, as a fraction of
/// Stroke Length. A median, as Angled Strokes uses: it lays each stroke as a
/// flat slab with a crisp end where an average would blur both.
const SPRAYED_SMEAR: f32 = 0.8;

/// Filter ▸ Brush Strokes ▸ Sprayed Strokes: the picture repainted in
/// angled, scattered strokes of its own dominant colours.
///
/// 1. **The spray.** Each pixel takes its colour from one thrown along the
///    stroke axis, up to **Spray Radius**. The field that throws it runs with
///    the stroke and changes across it, so the picture combs into threads
///    along **Stroke Direction** instead of simply roughening.
/// 2. **The strokes.** The sprayed colour is settled by a median along the
///    same axis, **Stroke Length** long, which gathers the threads into flat
///    strokes of the local dominant colour with crisp ends.
///
/// **Stroke Direction** is one of CS6's four: right diagonal, horizontal,
/// left diagonal, vertical.
///
/// At Spray Radius 0 and Stroke Length 0 the picture is left as it was.
///
/// Alpha is left alone.
///
/// No GPU path, for Angled Strokes' reasons: the settling is a median along a
/// line, and nothing downstream stays on the device.
pub fn sprayed_strokes(pixmap: &mut Pixmap, length: u32, radius: u32, direction: StrokeDirection) {
    if pixmap.is_empty() {
        return;
    }
    let length = length.clamp(*SPRAYED_LENGTH.start(), *SPRAYED_LENGTH.end()) as f32;
    let radius = radius.clamp(*SPRAYED_RADIUS.start(), *SPRAYED_RADIUS.end()) as f32;
    if length == 0.0 && radius == 0.0 {
        // No spray and no stroke: nothing to repaint with.
        return;
    }
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);

    let radians = direction.angle().to_radians();
    let along = (radians.cos(), -radians.sin());

    // Coherent along the stroke, near white noise across it — the comb.
    let run = (length * SPRAYED_RUN).max(SPRAYED_RUN_FLOOR);
    let mut throw = crate::filters::artistic::streaked_noise(w, h, run, 91, along);
    blur_field(&mut throw, w, h, SPRAYED_TOOTH);
    unit_spread(&mut throw);

    let source = pixmap.clone();
    let scatter = radius * SPRAYED_SCATTER;
    let (src, throw) = (&source, &throw);
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(w * 4)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let offset = throw[y * w + x].clamp(-2.0, 2.0) * scatter;
                let sx = (x as f32 + along.0 * offset).round().clamp(0.0, (w - 1) as f32) as usize;
                let sy = (y as f32 + along.1 * offset).round().clamp(0.0, (h - 1) as f32) as usize;
                let from = (sy * w + sx) * 4;
                px[..3].copy_from_slice(&src.as_bytes()[from..from + 3]);
                // Alpha stands: repainting the picture does not change the
                // layer's shape.
            }
        });

    if length > 0.0 {
        let settled = median_along(pixmap, along, (length * SPRAYED_SMEAR).max(1.0));
        for (out, from) in pixmap
            .as_bytes_mut()
            .chunks_exact_mut(4)
            .zip(settled.as_bytes().chunks_exact(4))
        {
            out[..3].copy_from_slice(&from[..3]);
        }
    }
}
/// CS6's ranges for Sumi-e, which its three sliders run over.
pub const SUMI_WIDTH: std::ops::RangeInclusive<u32> = 3..=15;
pub const SUMI_PRESSURE: std::ops::RangeInclusive<u32> = 0..=15;
pub const SUMI_CONTRAST: std::ops::RangeInclusive<u32> = 0..=40;

/// The paper and the ink.
///
/// Sumi is a warm black, never the flat `#000` a threshold gives, and the
/// paper it sits on is unbleached — a cool grey-white picture on pure white
/// is the clearest sign of an ink wash that was done with a Levels slider.
const SUMI_PAPER: [f32; 3] = [245.0, 242.0, 231.0];
const SUMI_INK: [f32; 3] = [28.0, 30.0, 36.0];

/// How far the picture is softened before it is read as tone, in pixels: a
/// floor and a step of Stroke Width. A brush carries tone, not detail.
const SUMI_WASH: f32 = 0.6;
const SUMI_WASH_PER_STEP: f32 = 0.22;

/// How far the picture is flattened into slabs of one tone, as a median
/// radius in pixels per step of Stroke Width.
const SUMI_FLATTEN_PER_STEP: f32 = 0.6;

/// How many washes there are between bare paper and full black.
///
/// A painter loads the brush a few times, not two hundred: the whole look
/// rests on there being *few* tones, each one flat. A continuous ramp from
/// the photograph's own luminance is a grey photograph, however well it is
/// blurred.
const SUMI_LEVELS: f32 = 4.0;

/// How far the washes are biased towards bare paper, in fractions of a wash.
/// Without it every faint tone in the photograph picks up the lightest wash
/// and the whole sheet goes muddy, where a painting leaves it empty.
const SUMI_DROP: f32 = 0.3;

/// Where the paper is left bare, as a fraction of white: a floor and a step
/// of Contrast. Everything lighter than this is untouched paper — the empty
/// space that is most of any of these paintings.
const SUMI_PAPER_FROM: f32 = 0.62;
const SUMI_PAPER_PER_STEP: f32 = 0.006;

/// Where the ink goes solid, as a fraction of white: a floor and a step of
/// Contrast. Everything darker than this is simply black.
///
/// A painting has a bottom to its range as well as a top. Scaling the ink by
/// the photograph's own darkest tone instead leaves the deepest shadow at
/// three quarters of a wash — dark grey, never black — and the whole sheet
/// reads as a faded photocopy rather than as ink.
const SUMI_INK_FULL: f32 = 0.15;
const SUMI_INK_FULL_PER_STEP: f32 = 0.004;

/// How much ink the brush carries: a floor and a step of Stroke Pressure.
///
/// The floor is high enough that even a light touch blacks out the deepest
/// shadow — ink is black, and a load that scales the whole range leaves the
/// darkest wash at three quarters, which is dark grey. What Pressure buys is
/// how far the ink reaches *up* into the midtones.
const SUMI_LOAD: f32 = 0.95;
const SUMI_LOAD_PER_STEP: f32 = 0.045;

/// How hard the washes separate from one another, per step of Contrast.
const SUMI_HARD_PER_STEP: f32 = 0.03;

/// How far the ink bleeds into the paper, in pixels: a floor and a step of
/// Stroke Width.
const SUMI_BLEED: f32 = 0.7;
const SUMI_BLEED_PER_STEP: f32 = 0.14;

/// How far the ink creeps along the paper's fibres, in pixels, and how
/// coarse that creep is.
///
/// This is what keeps a wash's edge from being the shape of the photograph
/// underneath it. Ink on paper wanders — it follows the fibres, runs further
/// in one place than the next — and an edge that does not wander reads as a
/// selection that has been feathered.
const SUMI_CREEP: f32 = 2.2;
const SUMI_CREEP_GRAIN: f32 = 1.6;

/// How much darker the rim of a wet wash dries than its middle.
///
/// Water carries the pigment outward as it dries and strands it at the
/// boundary. It is the single most recognisable thing about ink on paper,
/// and nothing else in a tonal filter produces it.
const SUMI_POOL: f32 = 0.22;

/// Which colours survive: nothing below the first, all of it above the
/// second.
///
/// A threshold, not a slope. Scaled straight off saturation, a pale blue sky
/// counts as a colour and picks up a wash of itself — which is how an empty
/// sheet ends up grey.
///
/// Most of these paintings are ink alone; the ones that are not put a few
/// deliberate colours on the same bare ground — a pink blossom, a yellow
/// bird. Keeping colour in proportion to how saturated the photograph
/// already was reproduces both: a beach comes out monochrome, a flower keeps
/// its petals.
const SUMI_COLOUR_FROM: f32 = 0.45;
const SUMI_COLOUR_FULL: f32 = 0.9;

/// The most of itself a colour may keep. There is always ink in the brush:
/// a wash that is purely the photograph's own green is a green photograph,
/// not a painting, and every colour in these pictures is muted by the ink
/// it is mixed with.
const SUMI_COLOUR_MOST: f32 = 0.6;

/// How strongly the paper's own fibre shows through, in levels.
const SUMI_FIBRE: f32 = 3.5;

/// Filter ▸ Brush Strokes ▸ Sumi-e: the picture repainted as a Japanese ink
/// wash — bare warm paper, a few flat washes of warm black, edges that bleed
/// and rims that pool.
///
/// **This is the painting, not CS6's filter.** CS6's Sumi-e lays a diagonal
/// hatch over the photograph and drives the darks down; it is named after the
/// tradition but does not look much like it. What is built here is the
/// tradition — asked for deliberately, and a knowing departure from the
/// parity the rest of this file keeps.
///
/// 1. **The wash.** The picture is softened and flattened into slabs, then
///    read as tone alone and cut into [`SUMI_LEVELS`] washes. Anything
///    lighter than **Contrast**'s threshold is left as bare paper.
/// 2. **The bleed.** Each wash's edge is made to creep along the paper's
///    fibres and then softened, so it wanders instead of tracing whatever was
///    in the photograph.
/// 3. **The pooling.** Where a wash ends, the ink dries darker — the rim that
///    water leaves as it retreats.
/// 4. **The paper.** What is left bare takes the paper's warm white and its
///    fibre. Colour survives only where the photograph was strongly
///    saturated, so most pictures come out in ink alone.
///
/// **Stroke Width** is how wide the brush is and so how far the ink spreads,
/// **Stroke Pressure** how much it carries, and **Contrast** how sharply the
/// washes separate and how much paper is left bare.
///
/// Alpha is left alone.
///
/// No GPU path: the flattening is a median, sequential along each row.
pub fn sumi_e(pixmap: &mut Pixmap, width: u32, pressure: u32, contrast: u32) {
    if pixmap.is_empty() {
        return;
    }
    let width = width.clamp(*SUMI_WIDTH.start(), *SUMI_WIDTH.end()) as f32;
    let pressure = pressure.clamp(*SUMI_PRESSURE.start(), *SUMI_PRESSURE.end()) as f32;
    let contrast = contrast.clamp(*SUMI_CONTRAST.start(), *SUMI_CONTRAST.end()) as f32;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);

    // 1: the wash. Tone, not detail — and only a few tones of it.
    let mut washed = pixmap.clone();
    crate::filters::convolve::gaussian_blur_accelerated(
        &mut washed,
        SUMI_WASH + width * SUMI_WASH_PER_STEP,
    );
    let flatten = (width * SUMI_FLATTEN_PER_STEP).round() as i32;
    if flatten > 0 {
        washed = median_pixel(&washed, flatten);
    }

    let paper_from = (SUMI_PAPER_FROM + contrast * SUMI_PAPER_PER_STEP).min(0.95);
    let ink_full = (SUMI_INK_FULL + contrast * SUMI_INK_FULL_PER_STEP).min(paper_from - 0.08);
    let hard = 1.0 + contrast * SUMI_HARD_PER_STEP;
    let load = SUMI_LOAD + pressure * SUMI_LOAD_PER_STEP;
    let mut ink: Vec<f32> = washed
        .as_bytes()
        .par_chunks_exact(4)
        .map(|p| {
            let tone =
                (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) / 255.0;
            let d = ((paper_from - tone) / (paper_from - ink_full)).clamp(0.0, 1.0);
            let d = (d.powf(1.0 / hard) * load).clamp(0.0, 1.0);
            // Cut into a painter's handful of washes.
            ((d * SUMI_LEVELS - SUMI_DROP).round().max(0.0) / SUMI_LEVELS).min(1.0)
        })
        .collect();

    // 2: the bleed. The wash creeps along the fibres, then softens.
    let grain = |salt: usize| {
        let mut field: Vec<f32> = (0..w * h)
            .into_par_iter()
            .map(|i| noise((i % w) as i32, (i / w + salt) as i32) * 2.0 - 1.0)
            .collect();
        blur_field(&mut field, w, h, SUMI_CREEP_GRAIN);
        unit_spread(&mut field);
        field
    };
    let (aside, down) = (grain(0), grain(h + 613));
    let crept: Vec<f32> = (0..w * h)
        .into_par_iter()
        .map(|i| {
            let (x, y) = ((i % w) as f32, (i / w) as f32);
            let away = |v: f32| v.clamp(-2.0, 2.0) * SUMI_CREEP;
            let sx = (x + away(aside[i])).round().clamp(0.0, (w - 1) as f32) as usize;
            let sy = (y + away(down[i])).round().clamp(0.0, (h - 1) as f32) as usize;
            ink[sy * w + sx]
        })
        .collect();
    ink = crept;
    let bleed = SUMI_BLEED + width * SUMI_BLEED_PER_STEP;
    blur_field(&mut ink, w, h, bleed);

    // 3: the pooling. A wash dries darker where it ends.
    let rim = sobel(&ink, w, h);

    // 4: the paper.
    let (ink, rim) = (&ink, &rim);
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .zip(washed.as_bytes().par_chunks_exact(stride))
        .enumerate()
        .for_each(|(y, (out, tone))| {
            for (x, (px, t)) in out.chunks_exact_mut(4).zip(tone.chunks_exact(4)).enumerate() {
                let i = y * w + x;
                // How much of the picture's own colour the brush kept.
                let (mut most, mut least) = (0.0f32, 255.0f32);
                for c in 0..3 {
                    most = most.max(t[c] as f32);
                    least = least.min(t[c] as f32);
                }
                let saturation = if most > 1.0 { (most - least) / most } else { 0.0 };
                let k = ((saturation - SUMI_COLOUR_FROM)
                    / (SUMI_COLOUR_FULL - SUMI_COLOUR_FROM))
                    .clamp(0.0, 1.0);
                let keep = k * k * (3.0 - 2.0 * k) * SUMI_COLOUR_MOST;

                // How much pigment there is comes from tone alone, never
                // from colour. Giving a saturated area a body of its own
                // lays a flat field of itself over the paper — a green
                // background stops being empty space and becomes a green
                // wall. Colour tints the pigment; it does not summon any.
                let body = (ink[i] + rim[i] * SUMI_POOL).clamp(0.0, 1.0);

                let fibre = (noise(x as i32, (y + 7919) as i32) * 2.0 - 1.0) * SUMI_FIBRE;
                for c in 0..3 {
                    let pigment = SUMI_INK[c] * (1.0 - keep) + t[c] as f32 * 0.85 * keep;
                    let v = SUMI_PAPER[c] * (1.0 - body) + pigment * body + fibre;
                    px[c] = v.round().clamp(0.0, 255.0) as u8;
                }
                // Alpha stands: painting the picture does not change the
                // layer's shape.
            }
        });
}

/// For each pixel, the colour of the neighbour whose brightness is the median
/// of those in a square window `reach` either side.
///
/// A whole pixel, not a median per channel: taking each channel's own median
/// mixes colours that were never next to each other and turns the spray to
/// mush. Choosing one of the sprayed pixels keeps the picture's own palette,
/// which is what gives each droplet a hard edge.
fn median_pixel(source: &Pixmap, reach: i32) -> Pixmap {
    let (w, h) = (source.width() as i32, source.height() as i32);
    let mut out = source.clone();
    let stride = out.stride();
    out.as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, row)| {
            let y = y as i32;
            let (top, bottom) = ((y - reach).max(0), (y + reach).min(h - 1));
            let mut samples: Vec<(u32, usize)> =
                Vec::with_capacity(((2 * reach + 1) * (2 * reach + 1)) as usize);
            for x in 0..w {
                samples.clear();
                let (left, right) = ((x - reach).max(0), (x + reach).min(w - 1));
                for sy in top..=bottom {
                    for sx in left..=right {
                        let i = (sy * w + sx) as usize * 4;
                        let p = &source.as_bytes()[i..i + 3];
                        let luma = 299 * p[0] as u32 + 587 * p[1] as u32 + 114 * p[2] as u32;
                        samples.push((luma, i));
                    }
                }
                let mid = samples.len() / 2;
                let (_, i) = *samples.select_nth_unstable_by_key(mid, |s| s.0).1;
                let o = x as usize * 4;
                row[o..o + 3].copy_from_slice(&source.as_bytes()[i..i + 3]);
            }
        });
    out
}

/// Average a field along a straight line at `angle`, `length` long.
fn streak_field(field: &[f32], w: usize, h: usize, angle: f32, length: f32) -> Vec<f32> {
    let radians = angle.to_radians();
    let (dx, dy) = (radians.cos(), -radians.sin());
    let steps = (length.round() as i32).max(1) / 2;
    let mut out = vec![0.0f32; w * h];
    out.par_chunks_exact_mut(w).enumerate().for_each(|(y, row)| {
        for (x, slot) in row.iter_mut().enumerate() {
            let (mut total, mut count) = (0.0f32, 0.0f32);
            for step in -steps..=steps {
                let sx = (x as f32 + dx * step as f32).round() as i32;
                let sy = (y as f32 + dy * step as f32).round() as i32;
                if sx < 0 || sy < 0 || sx >= w as i32 || sy >= h as i32 {
                    continue;
                }
                total += field[sy as usize * w + sx as usize];
                count += 1.0;
            }
            *slot = total / count.max(1.0);
        }
    });
    out
}

/// For each pixel, the colour of the sample whose brightness is the median of
/// those along a line through it, `length` long.
fn median_along(source: &Pixmap, along: (f32, f32), length: f32) -> Pixmap {
    let (w, h) = (source.width() as i32, source.height() as i32);
    let steps = (length.round() as i32).max(1) / 2;
    let offsets: Vec<(i32, i32)> = (-steps..=steps)
        .map(|s| ((along.0 * s as f32).round() as i32, (along.1 * s as f32).round() as i32))
        .collect();
    let mut out = source.clone();
    let stride = out.stride();
    out.as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, row)| {
            let mut samples: Vec<(u32, usize)> = Vec::with_capacity(offsets.len());
            for x in 0..w {
                samples.clear();
                for &(dx, dy) in &offsets {
                    let (sx, sy) = (x + dx, y as i32 + dy);
                    if sx < 0 || sy < 0 || sx >= w || sy >= h {
                        continue;
                    }
                    let i = (sy * w + sx) as usize * 4;
                    let p = &source.as_bytes()[i..i + 3];
                    let luma = 299 * p[0] as u32 + 587 * p[1] as u32 + 114 * p[2] as u32;
                    samples.push((luma, i));
                }
                let mid = samples.len() / 2;
                let (_, i) = *samples.select_nth_unstable_by_key(mid, |s| s.0).1;
                let o = x as usize * 4;
                row[o..o + 3].copy_from_slice(&source.as_bytes()[i..i + 3]);
            }
        });
    out
}

/// Sobel magnitude of a field, clamped at the edges.
pub(crate) fn sobel(field: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; w * h];
    out.par_chunks_exact_mut(w).enumerate().for_each(|(y, row)| {
        let (up, down) = (y.saturating_sub(1), (y + 1).min(h - 1));
        for (x, slot) in row.iter_mut().enumerate() {
            let (left, right) = (x.saturating_sub(1), (x + 1).min(w - 1));
            let at = |xx: usize, yy: usize| field[yy * w + xx];
            let gx = at(right, up) + 2.0 * at(right, y) + at(right, down)
                - at(left, up) - 2.0 * at(left, y) - at(left, down);
            let gy = at(left, down) + 2.0 * at(x, down) + at(right, down)
                - at(left, up) - 2.0 * at(x, up) - at(right, up);
            *slot = (gx * gx + gy * gy).sqrt();
        }
    });
    out
}

/// Fill whatever the mask encloses: everything outside it that cannot be
/// reached from the edge of the frame is inside it.
fn fill_holes(field: &[f32], w: usize, h: usize) -> Vec<f32> {
    let inside = |v: f32| v > 0.5;
    let mut ground = vec![false; w * h];
    let mut queue: Vec<usize> = Vec::new();
    let mut open = |i: usize, ground: &mut Vec<bool>, queue: &mut Vec<usize>| {
        if !ground[i] && !inside(field[i]) {
            ground[i] = true;
            queue.push(i);
        }
    };
    for x in 0..w {
        open(x, &mut ground, &mut queue);
        open((h - 1) * w + x, &mut ground, &mut queue);
    }
    for y in 0..h {
        open(y * w, &mut ground, &mut queue);
        open(y * w + w - 1, &mut ground, &mut queue);
    }
    while let Some(i) = queue.pop() {
        let (x, y) = (i % w, i / w);
        if x > 0 {
            open(i - 1, &mut ground, &mut queue);
        }
        if x + 1 < w {
            open(i + 1, &mut ground, &mut queue);
        }
        if y > 0 {
            open(i - w, &mut ground, &mut queue);
        }
        if y + 1 < h {
            open(i + w, &mut ground, &mut queue);
        }
    }
    field
        .par_iter()
        .zip(ground.par_iter())
        .map(|(v, out)| if *out { *v } else { v.max(1.0) })
        .collect()
}

/// Stop up the gaps in a mask that are narrower than `reach`, leaving its
/// outside edge where it was.
///
/// By blurring and cutting rather than by spreading and pulling back over a
/// square window: the square leaves square corners, and they show as
/// rectangular blocks wherever a shape has a hole in it. What is left is
/// taken together with the mask it started from, so the edge stays crisp.
fn close_gaps(field: &[f32], w: usize, h: usize, reach: f32) -> Vec<f32> {
    let mut spread = field.to_vec();
    blur_field(&mut spread, w, h, reach * 0.5);
    spread
        .par_iter_mut()
        .zip(field.par_iter())
        .for_each(|(v, f)| {
            let filled = if *v > CLOSE_CUT { 1.0 } else { 0.0 };
            *v = f.max(filled);
        });
    spread
}

/// How much of a neighbourhood must be inside the mask for a gap in it to
/// count as stopped up.
const CLOSE_CUT: f32 = 0.5;

/// [`widest_nearby`] over a plain field.
fn widest_nearby_field(field: &[f32], w: usize, h: usize, radius: usize) -> Vec<f32> {
    let mut out = field.to_vec();
    widest_nearby(&mut out, w, h, radius);
    out
}

/// Replace each value with the largest within `radius` of it, across then
/// down.
pub(crate) fn widest_nearby(field: &mut [f32], w: usize, h: usize, radius: usize) {
    if radius == 0 {
        return;
    }
    let scratch = field.to_vec();
    field.par_chunks_exact_mut(w).enumerate().for_each(|(y, row)| {
        let line = &scratch[y * w..(y + 1) * w];
        for (x, slot) in row.iter_mut().enumerate() {
            let (from, to) = (x.saturating_sub(radius), (x + radius + 1).min(w));
            // Seeded at negative infinity, not zero: a field with negative
            // values in it — an inverted mask, say — would otherwise come
            // back as all zeroes.
            *slot = line[from..to].iter().copied().fold(f32::NEG_INFINITY, f32::max);
        }
    });
    let scratch = field.to_vec();
    field.par_chunks_exact_mut(w).enumerate().for_each(|(y, row)| {
        let (from, to) = (y.saturating_sub(radius), (y + radius + 1).min(h));
        for (x, slot) in row.iter_mut().enumerate() {
            *slot = (from..to).map(|yy| scratch[yy * w + x]).fold(f32::NEG_INFINITY, f32::max);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::{Rect, Rgba8};

    fn step() -> Pixmap {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(60, 60, 60, 255));
        pm.fill_rect(Rect::new(32, 0, 32, 64), Rgba8::new(180, 180, 180, 255));
        pm
    }

    /// High brightness chalks the edge, low brightness inks it, and a flat
    /// field far from the edge is left as it was either way.
    #[test]
    fn brightness_decides_chalk_or_ink() {
        let mut chalk = step();
        accented_edges(&mut chalk, 2, 50, 3);
        let mut ink = step();
        accented_edges(&mut ink, 2, 0, 3);
        assert!(chalk.get(31, 32).r > 150, "no chalk: {}", chalk.get(31, 32).r);
        assert!(ink.get(32, 32).r < 80, "no ink: {}", ink.get(32, 32).r);
        assert_eq!(chalk.get(4, 32).r, 60);
        assert_eq!(ink.get(60, 32).r, 180);
    }

    /// A wider edge reaches further from the boundary.
    #[test]
    fn edge_width_widens_the_accent() {
        let reach = |width| {
            let mut pm = step();
            accented_edges(&mut pm, width, 50, 3);
            pm.get(24, 32).r
        };
        assert!(reach(14) > reach(1) + 40, "{} against {}", reach(14), reach(1));
    }

    #[test]
    fn leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        accented_edges(&mut pm, 2, 38, 5);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    /// Strokes are laid along the diagonal their tone runs on, so a thin line
    /// on that diagonal survives and one across it is painted out: in the
    /// dark, lines rising to the right survive; in the light, falling ones;
    /// and at Direction Balance 100 the light rises too.
    #[test]
    fn angled_strokes_run_by_tone() {
        let kept = |line: u8, ground: u8, rising: bool, balance| {
            let mut pm = Pixmap::filled(80, 80, Rgba8::new(ground, ground, ground, 255));
            for d in -30..30 {
                let y = if rising { 40 - d } else { 40 + d };
                pm.set(40 + d, y, Rgba8::new(line, line, line, 255));
            }
            angled_strokes(&mut pm, balance, 15, 0);
            let y = if rising { 38 } else { 42 };
            (pm.get(42, y).r as i32 - ground as i32).unsigned_abs()
        };
        assert!(kept(70, 20, true, 50) > kept(70, 20, false, 50) + 15, "dark did not rise");
        assert!(kept(170, 230, false, 50) > kept(170, 230, true, 50) + 15, "light did not fall");
        assert!(kept(170, 230, true, 100) > kept(170, 230, false, 100) + 15, "balance did not turn it");
    }

    #[test]
    fn angled_strokes_leave_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        angled_strokes(&mut pm, 50, 15, 3);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn angled_strokes_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        angled_strokes(&mut pm, 50, 15, 3);
    }

    /// Hatching follows texture: a busy field is hatched far more than a
    /// smooth one of the same tone, and more so with more Strength.
    #[test]
    fn crosshatch_hatches_texture_not_smooth_areas() {
        let busy_field = || {
            let mut pm = Pixmap::new(64, 64);
            for y in 0..64 {
                for x in 0..64 {
                    let v = if (x * 7 + y * 13) % 5 < 2 { 150 } else { 110 };
                    pm.set(x, y, Rgba8::new(v, v, v, 255));
                }
            }
            pm
        };
        let hatched = |mut pm: Pixmap, strength| {
            crosshatch(&mut pm, 9, 6, strength);
            restless(&pm)
        };
        let smooth = hatched(Pixmap::filled(64, 64, Rgba8::new(126, 126, 126, 255)), 1);
        let busy = hatched(busy_field(), 1);
        assert!(busy > smooth * 3, "texture was not favoured: {busy} against {smooth}");
        assert!(hatched(busy_field(), 3) > busy, "more strength did not hatch harder");
    }

    /// How much a picture changes from one pixel to the next, across.
    fn restless(pm: &Pixmap) -> u32 {
        (0..64)
            .flat_map(|y| (1..64).map(move |x| (x, y)))
            .map(|(x, y)| (pm.get(x, y).r as i32 - pm.get(x - 1, y).r as i32).unsigned_abs())
            .sum()
    }

    #[test]
    fn crosshatch_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        crosshatch(&mut pm, 9, 6, 2);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn crosshatch_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        crosshatch(&mut pm, 9, 6, 1);
    }

    /// Black Intensity and Balance together take more of the picture to
    /// black; White Intensity carries the light towards white.
    #[test]
    fn dark_strokes_push_the_tones_apart() {
        let tone = |value, balance, black, white| {
            let mut pm = Pixmap::filled(32, 32, Rgba8::new(value, value, value, 255));
            dark_strokes(&mut pm, balance, black, white);
            pm.get(16, 16).r
        };
        assert!(tone(100, 0, 2, 2) > 60, "a mid tone went black at low settings");
        assert!(tone(100, 10, 10, 10) < 10, "a mid tone survived the top settings");
        assert!(tone(20, 0, 2, 2) < 10, "a deep shadow did not go black");
        assert!(tone(225, 5, 5, 10) > tone(225, 5, 5, 0) + 10, "white intensity did nothing");
    }

    #[test]
    fn dark_strokes_leave_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        dark_strokes(&mut pm, 5, 5, 5);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn dark_strokes_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        dark_strokes(&mut pm, 5, 5, 5);
    }

    /// Ink darkens the dark side of an edge and white lights the light side,
    /// each only when its slider is up.
    #[test]
    fn ink_outlines_ink_the_dark_side_and_chalk_the_light() {
        let drawn = |dark, light| {
            let mut pm = Pixmap::filled(64, 64, Rgba8::new(120, 120, 120, 255));
            pm.fill_rect(Rect::new(32, 0, 32, 64), Rgba8::new(190, 190, 190, 255));
            ink_outlines(&mut pm, 4, dark, light);
            // A band either side: the ink lands along the stroke, which runs
            // diagonally, so it need not fall on the pixel beside the edge.
            let darkest = (27..32).map(|x| pm.get(x, 32).r).min().unwrap();
            let lightest = (33..38).map(|x| pm.get(x, 32).r).max().unwrap();
            (darkest, lightest)
        };
        let (dark_side, light_side) = drawn(40, 0);
        assert!(dark_side < 90, "the dark side was not inked: {dark_side}");
        let (_, light_side_lit) = drawn(0, 40);
        assert!(light_side_lit > light_side, "the light side was not chalked");
    }

    #[test]
    fn ink_outlines_leave_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        ink_outlines(&mut pm, 4, 20, 10);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn ink_outlines_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        ink_outlines(&mut pm, 4, 20, 10);
    }

    /// The spray throws colour about, further as Spray Radius rises, and at
    /// 0 it leaves the picture where it was.
    #[test]
    fn spatter_throws_further_as_the_radius_rises() {
        let bleed = |radius| {
            let mut pm = Pixmap::filled(80, 80, Rgba8::new(40, 40, 40, 255));
            pm.fill_rect(Rect::new(0, 0, 40, 80), Rgba8::new(220, 220, 220, 255));
            spatter(&mut pm, radius, 1);
            // How far the light half has thrown pixels into the dark half.
            (40..70)
                .map(|x| (0..80).filter(|&y| pm.get(x, y).r > 120).count() as u32)
                .sum::<u32>()
        };
        assert_eq!(bleed(0), 0);
        assert!(bleed(20) > bleed(5), "{} against {}", bleed(20), bleed(5));
    }

    /// Smoothness gathers the grit: the result changes less from pixel to
    /// pixel as it rises.
    #[test]
    fn spatter_smoothness_gathers_the_grit() {
        let grit = |smoothness| {
            let mut pm = Pixmap::filled(80, 80, Rgba8::new(40, 40, 40, 255));
            pm.fill_rect(Rect::new(0, 0, 40, 80), Rgba8::new(220, 220, 220, 255));
            spatter(&mut pm, 12, smoothness);
            (1..80)
                .flat_map(|y| (1..80).map(move |x| (x, y)))
                .map(|(x, y)| (pm.get(x, y).r as i32 - pm.get(x - 1, y).r as i32).unsigned_abs())
                .sum::<u32>()
        };
        // Comfortably under three quarters — it measures about 0.57 of it.
        // The bar is the direction and a clear margin, not an exact ratio:
        // how much grit a given picture has left is a property of the
        // picture, not something the filter promises.
        assert!(grit(15) * 4 < grit(1) * 3, "{} against {}", grit(15), grit(1));
    }

    #[test]
    fn spatter_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        spatter(&mut pm, 10, 5);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    /// At the bottom of Spray Radius there is no spray, and Smoothness on its
    /// own must not quietly median the picture.
    #[test]
    fn spatter_at_radius_zero_leaves_the_picture_alone() {
        let mut pm = Pixmap::filled(40, 40, Rgba8::new(40, 40, 40, 255));
        pm.fill_rect(Rect::new(0, 0, 20, 40), Rgba8::new(220, 220, 220, 255));
        pm.fill_rect(Rect::new(30, 30, 1, 1), Rgba8::new(0, 255, 0, 255));
        let before = pm.clone();
        for smoothness in [1, 8, 15] {
            spatter(&mut pm, 0, smoothness);
            assert_eq!(pm.as_bytes(), before.as_bytes());
        }
    }

    #[test]
    fn spatter_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        spatter(&mut pm, 10, 5);
    }

    /// A noisy square, and how much it changes from pixel to pixel across and
    /// down. Smearing along an axis makes neighbours along that axis agree,
    /// so the count in that direction falls.
    fn sprayed_grain(direction: StrokeDirection) -> (u32, u32) {
        let mut pm = Pixmap::new(64, 64);
        for y in 0..64i32 {
            for x in 0..64i32 {
                let v = ((x * 37 + y * 101) % 256) as u8;
                pm.set(x, y, Rgba8::new(v, v, v, 255));
            }
        }
        sprayed_strokes(&mut pm, 12, 10, direction);
        let across = (1..64)
            .flat_map(|y| (1..64).map(move |x| (x, y)))
            .map(|(x, y)| (pm.get(x, y).r as i32 - pm.get(x - 1, y).r as i32).unsigned_abs())
            .sum::<u32>();
        let down = (1..64)
            .flat_map(|y| (1..64).map(move |x| (x, y)))
            .map(|(x, y)| (pm.get(x, y).r as i32 - pm.get(x, y - 1).r as i32).unsigned_abs())
            .sum::<u32>();
        (across, down)
    }

    /// Stroke Direction is the axis the paint is laid along: Vertical settles
    /// the picture up and down, Horizontal settles it side to side.
    #[test]
    fn sprayed_strokes_run_along_the_chosen_direction() {
        let (across, down) = sprayed_grain(StrokeDirection::Vertical);
        assert!(down < across, "vertical: {} down against {} across", down, across);
        let (across, down) = sprayed_grain(StrokeDirection::Horizontal);
        assert!(across < down, "horizontal: {} across against {} down", across, down);
    }

    /// The two diagonals are not the same picture, and neither is either of
    /// the axes — the dropdown does something for all four.
    #[test]
    fn sprayed_strokes_directions_differ_from_one_another() {
        let painted = |direction| {
            let mut pm = Pixmap::filled(48, 48, Rgba8::new(30, 60, 90, 255));
            pm.fill_rect(Rect::new(10, 10, 28, 28), Rgba8::new(230, 200, 40, 255));
            sprayed_strokes(&mut pm, 10, 12, direction);
            pm.as_bytes().to_vec()
        };
        let all = [
            painted(StrokeDirection::RightDiagonal),
            painted(StrokeDirection::Horizontal),
            painted(StrokeDirection::LeftDiagonal),
            painted(StrokeDirection::Vertical),
        ];
        for (i, one) in all.iter().enumerate() {
            for other in &all[i + 1..] {
                assert_ne!(one, other);
            }
        }
    }

    /// The dropdown's order is CS6's, and anything else falls back to its
    /// first entry rather than to a silent nothing.
    #[test]
    fn sprayed_stroke_directions_follow_the_dropdown() {
        assert_eq!(StrokeDirection::from_i32(0), StrokeDirection::RightDiagonal);
        assert_eq!(StrokeDirection::from_i32(1), StrokeDirection::Horizontal);
        assert_eq!(StrokeDirection::from_i32(2), StrokeDirection::LeftDiagonal);
        assert_eq!(StrokeDirection::from_i32(3), StrokeDirection::Vertical);
        assert_eq!(StrokeDirection::from_i32(-1), StrokeDirection::RightDiagonal);
        assert_eq!(StrokeDirection::from_i32(99), StrokeDirection::RightDiagonal);
    }

    /// A longer stroke settles the picture further along its axis.
    #[test]
    fn sprayed_strokes_lengthen_with_stroke_length() {
        let along = |length| {
            let mut pm = Pixmap::new(64, 64);
            for y in 0..64i32 {
                for x in 0..64i32 {
                    let v = ((x * 37 + y * 101) % 256) as u8;
                    pm.set(x, y, Rgba8::new(v, v, v, 255));
                }
            }
            sprayed_strokes(&mut pm, length, 8, StrokeDirection::Horizontal);
            (1..64)
                .flat_map(|y| (1..64).map(move |x| (x, y)))
                .map(|(x, y)| (pm.get(x, y).r as i32 - pm.get(x - 1, y).r as i32).unsigned_abs())
                .sum::<u32>()
        };
        assert!(along(20) < along(4), "{} against {}", along(20), along(4));
    }

    /// At the bottom of both sliders there is neither spray nor stroke, and
    /// the picture is left exactly as it was.
    #[test]
    fn sprayed_strokes_at_zero_leave_the_picture_alone() {
        let mut pm = Pixmap::filled(40, 40, Rgba8::new(40, 90, 140, 255));
        pm.fill_rect(Rect::new(5, 5, 12, 20), Rgba8::new(220, 210, 60, 255));
        let before = pm.clone();
        sprayed_strokes(&mut pm, 0, 0, StrokeDirection::RightDiagonal);
        assert_eq!(pm.as_bytes(), before.as_bytes());
    }

    #[test]
    fn sprayed_strokes_leave_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        sprayed_strokes(&mut pm, 12, 10, StrokeDirection::Vertical);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn sprayed_strokes_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        sprayed_strokes(&mut pm, 12, 10, StrokeDirection::Vertical);
    }

    /// A picture to paint: a dark subject on a lighter, broken ground.
    fn sumi_subject() -> Pixmap {
        let mut pm = Pixmap::new(64, 64);
        for y in 0..64i32 {
            for x in 0..64i32 {
                // Broken, but never dark: this stands for the sky and the
                // lit water, which the brush is meant to leave as paper.
                let wave = (((x * 3 + y * 7) as f32 * 0.4).sin() * 18.0) as i32;
                let at = |base: i32| (base + wave).clamp(0, 255) as u8;
                pm.set(x, y, Rgba8::new(at(150), at(180), at(230), 255));
            }
        }
        pm.fill_rect(Rect::new(16, 16, 32, 32), Rgba8::new(28, 24, 22, 255));
        pm
    }

    fn sumi_mean(pm: &Pixmap) -> f32 {
        let b = pm.as_bytes();
        b.chunks_exact(4)
            .map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
            .sum::<f32>()
            / (b.len() / 4) as f32
    }

    /// How near a pixel is to the paper, and to full ink.
    fn is_paper(p: Rgba8) -> bool {
        (p.r as i32 - SUMI_PAPER[0] as i32).abs() <= 6
            && (p.g as i32 - SUMI_PAPER[1] as i32).abs() <= 6
            && (p.b as i32 - SUMI_PAPER[2] as i32).abs() <= 6
    }
    fn is_ink(p: Rgba8) -> bool {
        (p.r as i32 - SUMI_INK[0] as i32).abs() <= 12
            && (p.g as i32 - SUMI_INK[1] as i32).abs() <= 12
            && (p.b as i32 - SUMI_INK[2] as i32).abs() <= 12
    }

    /// The light ground is left as bare paper, and the dark subject goes to
    /// full ink.
    ///
    /// Both halves matter. A wash that never reaches the paper leaves the
    /// sheet grey all over — the empty space is most of any of these
    /// paintings — and one that never reaches full ink leaves the subject a
    /// dark grey, which reads as a faded photograph rather than as sumi.
    #[test]
    fn sumi_e_leaves_bare_paper_and_reaches_full_ink() {
        let mut pm = sumi_subject();
        sumi_e(&mut pm, 7, 3, 12);
        // Well clear of the subject, and well inside it.
        for (x, y) in [(3, 3), (60, 4), (4, 60)] {
            assert!(is_paper(pm.get(x, y)), "({}, {}) is {:?}, not paper", x, y, pm.get(x, y));
        }
        for (x, y) in [(32, 32), (26, 38)] {
            assert!(is_ink(pm.get(x, y)), "({}, {}) is {:?}, not ink", x, y, pm.get(x, y));
        }
    }

    /// The paper is warm, not the white the picture happened to contain.
    #[test]
    fn sumi_e_paints_on_warm_paper() {
        let mut pm = sumi_subject();
        sumi_e(&mut pm, 7, 3, 12);
        let paper = pm.get(3, 3);
        assert!(paper.r > paper.b, "{:?} is not a warm white", paper);
    }

    /// Stroke Pressure is how much ink the brush carries, so more of it puts
    /// more of the picture under a wash.
    #[test]
    fn sumi_e_pressure_lays_more_ink() {
        let inked = |pressure| {
            let mut pm = sumi_subject();
            sumi_e(&mut pm, 7, pressure, 12);
            sumi_mean(&pm)
        };
        assert!(inked(15) < inked(0), "{} against {}", inked(15), inked(0));
    }

    /// Contrast carries the ink further up into the midtones, so less of the
    /// sheet is left bare.
    #[test]
    fn sumi_e_contrast_spreads_the_ink() {
        let bare = |contrast| {
            let mut pm = sumi_subject();
            sumi_e(&mut pm, 7, 3, contrast);
            pm.as_bytes()
                .chunks_exact(4)
                .filter(|p| is_paper(Rgba8::new(p[0], p[1], p[2], p[3])))
                .count()
        };
        assert!(bare(40) < bare(0), "{} against {}", bare(40), bare(0));
    }

    /// A wide brush on wet paper spreads, so the picture comes back softer.
    #[test]
    fn sumi_e_width_softens_the_picture() {
        let grain = |width| {
            let mut pm = sumi_subject();
            sumi_e(&mut pm, width, 3, 12);
            (1..64)
                .flat_map(|y| (1..64).map(move |x| (x, y)))
                .map(|(x, y)| (pm.get(x, y).r as i32 - pm.get(x - 1, y).r as i32).unsigned_abs())
                .sum::<u32>()
        };
        assert!(grain(15) < grain(3), "{} against {}", grain(15), grain(3));
    }

    /// Only a strong colour survives, and even then the ink mutes it.
    ///
    /// Most of these paintings are ink alone, and the ones that are not put a
    /// few deliberate colours on the same bare ground. Carried straight off
    /// saturation instead, every faintly tinted thing in a photograph — a
    /// pale blue sky above all — lays down a wash of itself, and the sheet is
    /// never empty.
    #[test]
    fn sumi_e_keeps_only_a_strong_colour() {
        // Same tone in both, so only the saturation differs.
        let painted = |colour: Rgba8| {
            let mut pm = Pixmap::filled(48, 48, Rgba8::new(240, 240, 238, 255));
            pm.fill_rect(Rect::new(12, 12, 24, 24), colour);
            sumi_e(&mut pm, 7, 3, 12);
            pm.get(24, 24)
        };
        let vivid = painted(Rgba8::new(190, 20, 90, 255));
        let faint = painted(Rgba8::new(120, 104, 100, 255));
        assert!(
            vivid.r as i32 - vivid.g as i32 > 12,
            "{:?} lost a strong colour entirely",
            vivid
        );
        // Muted by the ink rather than reproduced.
        assert!(vivid.r < 190, "{:?} is the photograph's own colour", vivid);
        let spread = |p: Rgba8| p.r.max(p.g).max(p.b) as i32 - p.r.min(p.g).min(p.b) as i32;
        assert!(
            spread(faint) <= 8,
            "{:?} kept a colour it should not have",
            faint
        );
    }

    #[test]
    fn sumi_e_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        sumi_e(&mut pm, 7, 3, 12);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn sumi_e_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        sumi_e(&mut pm, 7, 3, 12);
    }

    #[test]
    fn over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        accented_edges(&mut pm, 2, 38, 5);
    }
}

//! Filter ▸ Artistic.
//!
//! CS6 keeps this whole family in the Filter Gallery rather than in the Filter
//! menu. The Gallery is not built (docs/ROADMAP.md), so the filters live under
//! a Filter ▸ Artistic submenu instead, which is where the Gallery's own
//! category list would have put them. Colored Pencil, Cutout, Dry Brush, Film
//! Grain, Fresco, Neon Glow, Paint Daubs and Palette Knife are built; the other
//! seven are listed in the menu and disabled.

use crate::buffer::{Pixmap, Rgba8};
use rayon::prelude::*;

/// CS6's ranges for Colored Pencil, which its three sliders run over.
pub const PENCIL_WIDTH: std::ops::RangeInclusive<u32> = 1..=24;
pub const PENCIL_PRESSURE: std::ops::RangeInclusive<u32> = 0..=15;
pub const PENCIL_PAPER: std::ops::RangeInclusive<u32> = 0..=50;

/// CS6's ranges for Cutout, which its three sliders run over.
pub const CUTOUT_LEVELS: std::ops::RangeInclusive<u32> = 2..=8;
pub const CUTOUT_SIMPLICITY: std::ops::RangeInclusive<u32> = 0..=10;
pub const CUTOUT_FIDELITY: std::ops::RangeInclusive<u32> = 1..=3;

/// CS6's ranges for Dry Brush, which its three sliders run over.
pub const BRUSH_SIZE: std::ops::RangeInclusive<u32> = 0..=10;
pub const BRUSH_DETAIL: std::ops::RangeInclusive<u32> = 0..=10;
pub const BRUSH_TEXTURE: std::ops::RangeInclusive<u32> = 1..=3;

/// CS6's ranges for Palette Knife, which its three sliders run over.
pub const KNIFE_SIZE: std::ops::RangeInclusive<u32> = 1..=50;
pub const KNIFE_DETAIL: std::ops::RangeInclusive<u32> = 1..=3;
pub const KNIFE_SOFTNESS: std::ops::RangeInclusive<u32> = 0..=10;

/// CS6's ranges for Paint Daubs. The third control is a list of brushes rather
/// than a slider.
pub const DAUB_SIZE: std::ops::RangeInclusive<u32> = 1..=50;
pub const DAUB_SHARPNESS: std::ops::RangeInclusive<u32> = 0..=40;

/// CS6's six Paint Daubs brushes, in the order its list gives them.
///
/// All six paint the same daubs. What differs is what is then done with the
/// detail the daubing left over — see [`paint_daubs`] — which is why a list of
/// brushes with nothing in common as *brushes* behaves like one family.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DaubBrush {
    /// The daubs and nothing else, with what Sharpness asks for on top.
    #[default]
    Simple,
    /// The leftover detail put back hard, keeping only what lightens: the
    /// picture comes back scrubbed and glaring, and its smooth parts break
    /// into contours.
    LightRough,
    /// The same, keeping only what darkens.
    DarkRough,
    /// A broader daub, and the detail put back sharply enough to draw a line
    /// where two daubs meet.
    WideSharp,
    /// A broader daub, softened afterwards, with the detail left out.
    WideBlurry,
    /// Both of the Rough brushes at once and harder than either: the picture
    /// is scrubbed light *and* dark until what was smooth breaks into ribbons
    /// and the edges blow out. Both directions, because the reference's
    /// background swirls light and dark alike — one-sided, it comes back as a
    /// silhouette rather than as a texture.
    Sparkle,
}

impl DaubBrush {
    pub fn from_i32(value: i32) -> DaubBrush {
        match value {
            1 => DaubBrush::LightRough,
            2 => DaubBrush::DarkRough,
            3 => DaubBrush::WideSharp,
            4 => DaubBrush::WideBlurry,
            5 => DaubBrush::Sparkle,
            _ => DaubBrush::Simple,
        }
    }
}

/// CS6's ranges for Neon Glow. The third control is a colour swatch rather
/// than a slider, and the two the picture is rendered between are the
/// document's own, so neither has a range here.
pub const NEON_SIZE: std::ops::RangeInclusive<i32> = -24..=24;
pub const NEON_BRIGHTNESS: std::ops::RangeInclusive<u32> = 0..=50;

/// CS6's ranges for Film Grain, which its three sliders run over.
pub const FILM_GRAIN: std::ops::RangeInclusive<u32> = 0..=20;
pub const FILM_HIGHLIGHT: std::ops::RangeInclusive<u32> = 0..=20;
pub const FILM_INTENSITY: std::ops::RangeInclusive<u32> = 0..=10;

/// The Stroke Pressure at which the pencil lays colour at life size — CS6's
/// default, and the figure everything else is measured against.
const NEUTRAL_PRESSURE: f32 = 8.0;

/// The scale, in pixels, below which the picture counts as *detail* — the
/// veins, the stipple, the boundaries. Measured as what a blur of this radius
/// takes away. Fixed rather than scaled by the pencil: it describes the
/// picture, not the hand.
const DETAIL_SCALE: f32 = 2.0;

/// How far, in pencil widths, the hand looks around itself to find detail to
/// thicken a line with.
const SPREAD: f32 = 1.3;

/// The most, in pixels, that the two reaches below are allowed to grow to.
///
/// Both scale with the pencil, and both would otherwise run away at the top of
/// CS6's range: a 24-wide pencil would look a whole 48 pixels around itself
/// and draw 60 beyond what it found, which stops being "the hand thickens the
/// edge" and becomes a stain spreading out of it.
const MAX_LOOK: f32 = 8.0;
const MAX_FILL: f32 = 8.0;

/// How far, in pencil widths, the drawn edge is then spread. Detail sits in a
/// one-pixel seam along a boundary, and a hand draws a line there rather than
/// the seam itself; this is what makes it a stroke a person would make.
const FILL: f32 = 2.5;

/// The hatch's spacing, in pencil widths. Strokes a single pixel apart read as
/// noise rather than as strokes, so the hand's marks are a couple of widths
/// apart however fine the pencil is.
const HATCH_SPACING: f32 = 1.3;

/// The local detail, in levels, below which it adds nothing to the stroke and
/// above which it adds all of it. Between the two it fades in.
///
/// An out-of-focus background carries a level or so of detail — its own grain,
/// and nothing else, because a blur is by definition what has no fine detail
/// left in it — and so falls under the floor and takes only the flat base
/// stroke. A petal with veins in it carries several, and is drawn in full.
const NOTHING_TO_DRAW: f32 = 1.5;
const DRAWN_IN_FULL: f32 = 4.0;

/// How far the picture is smoothed before the pencil lays it, in pixels. A
/// pencil puts down washes: enough to lose the stipple, not enough to lose
/// the shapes.
const WASH: f32 = 0.8;

/// How much of a full stroke is laid where the picture has nothing in
/// particular to say — a flat wash of colour, or a patch the same level as the
/// paper.
///
/// CS6's Colored Pencil draws the whole picture, not its edges: "the solid
/// colour background shows through the diagonal strokes". A flat pink petal is
/// therefore covered in pink strokes with paper between them, while a flat grey
/// field — which is what the backing colour usually is — disappears into the
/// paper because there is no contrast left to see. This is the number that
/// makes a large flat shape draw at all; without it the hand only ever traces
/// edges and large shapes come out hollow. Detail then adds the rest on top.
///
/// It is small, and deliberately: it is the floor under a part of the picture
/// with *nothing in it*, and in CS6 such a part comes back as bare paper — an
/// out-of-focus background goes flat grey rather than keeping a ghost of its
/// own colour. A base large enough to be seen greys the whole page towards the
/// photograph instead of leaving the drawn shapes to carry it.
const STROKE_BASE: f32 = 0.12;

/// How much of the laid colour the hatch can take back where a stroke thins
/// out, at neutral pressure.
///
/// This is the single most important number for the filter's look, and it cuts
/// both ways. A pencil leaves paper showing between its strokes, so it cannot
/// be nothing; but the hatch is narrow — [`STROKE_SHARPNESS`] keeps the strokes
/// to about a quarter of the page — so whatever this takes back, it takes back
/// from three quarters of every drawn shape. Set near 1 it does not read as
/// paper between strokes at all: a fully drawn petal keeps barely a third of
/// its own pink and the picture comes out as a washed photograph, which is the
/// opposite of the mistake it looks like it is guarding against. CS6 keeps the
/// colour and lets the paper streak across it, so this is a third rather than
/// nearly all. Pressing harder closes the gaps the rest of the way — see where
/// it is used.
const HATCH_DEPTH: f32 = 0.35;

/// How narrow a stroke is across its own width. A ridge shaped by an exponent
/// of one is a triangle that is half on and half off; raising it thins the
/// stroke so the paper between strokes stays the larger part of the page,
/// which is what makes the hatch read as separate pencil lines rather than as
/// a solid fill. This is the second of the two numbers that decide the look.
const STROKE_SHARPNESS: f32 = 2.5;

/// How dark the hatch itself lies on otherwise bare paper, in levels. This is
/// what leaves the faint diagonal strokes across an empty corner of the page,
/// where there was nothing to draw but the hand went over it anyway.
const HATCH_INK: f32 = 12.0;

/// How much darker the pencil goes along a boundary, where the hand presses
/// hardest. This is what outlines a shape.
const EDGE_DARKENING: f32 = 0.9;

/// How thick that outline is, in pencil widths — a wider pencil draws a
/// heavier line, which is the most visible thing Pencil Width does.
const OUTLINE: f32 = 0.5;

/// Filter ▸ Artistic ▸ Colored Pencil: the picture redrawn in pencil on paper.
///
/// The sheet is laid first, a flat grey at Paper Brightness, and the picture is
/// then drawn on it in coloured pencil. Every pixel gets a stroke — CS6 draws
/// the whole picture and lets the paper show through the gaps — and what the
/// picture's own content decides is how *much* stroke, in two steps:
///
/// * **How much fine detail sits here** — what a blur of [`DETAIL_SCALE`] takes
///   away. Not the plain gradient: a soft, out-of-focus background has a
///   perfectly good gradient running across it and no detail in it, while a
///   petal full of veins has plenty. Detail is what earns the darker,
///   cross-hatched strokes along a boundary, so it is the picture's edges that
///   come through most strongly — CS6's "important edges are retained and given
///   a rough crosshatch appearance".
/// * **The answer is averaged over the pencil's reach**, not taken pixel by
///   pixel. An average, not a maximum: one noisy pixel in an empty sky must
///   not earn a stroke for everything around it, and a petal full of veins
///   must earn one for the whole petal rather than for the veins alone. That
///   is what thickens an edge into a drawn line instead of a one-pixel seam.
///
/// What is then laid down is the picture's own colour, lightly washed, crossed
/// by the hatch and darkened along the boundaries where the hand presses
/// hardest. Pencil Width sets the hatch's spacing, how far the hand looks, and
/// how heavy the outlines are; Stroke Pressure is the gain on all of it, so at
/// 0 the page stays blank; Paper Brightness is the sheet.
///
/// The hatch is seeded from each pixel's coordinates rather than from a RNG,
/// so a preview, the commit behind it and an undo/redo replay all draw the
/// same strokes.
///
/// No GPU path. It is per-pixel and would fit the shader shape, but like the
/// rest of the filter stack it would upload its input and read the result
/// straight back, which the measurements in docs/gpu-migration.md say rarely
/// pays for the trip. The blurs inside it do go through the backend.
pub fn colored_pencil(pixmap: &mut Pixmap, width: u32, pressure: u32, paper_brightness: u32) {
    if pixmap.is_empty() {
        return;
    }
    let width_setting = width.clamp(*PENCIL_WIDTH.start(), *PENCIL_WIDTH.end());
    let pressure = pressure.clamp(*PENCIL_PRESSURE.start(), *PENCIL_PRESSURE.end());
    let paper_brightness = paper_brightness.clamp(*PENCIL_PAPER.start(), *PENCIL_PAPER.end());

    // CS6's slider runs 0..50 over the whole range from black paper to white.
    let paper = paper_brightness as f32 / *PENCIL_PAPER.end() as f32 * 255.0;
    let gain = pressure as f32 / NEUTRAL_PRESSURE;
    // What the hand does scales with the pressure, so at 0 the page stays
    // blank however much there was to draw. Past life size it is the gaps that
    // close up rather than the strokes that grow: a stroke cannot carry more
    // than its colour, and what "pressing harder" looks like from there is the
    // paper between the strokes disappearing. Never quite all of it: at the top
    // of the range the gaps are narrow, but a drawing with no paper left in it
    // is a photograph, so the slope here is gentle enough that 15 still shows
    // its strokes.
    let press = gain.min(1.0);
    let gap = if gain > 0.0 {
        (HATCH_DEPTH + (1.0 - gain) * 0.2).clamp(0.0, 0.97)
    } else {
        0.97
    };

    // How much fine detail sits at each pixel...
    let detail = detail_map(pixmap);
    // ...and how much of it there is about, which is what thickens an edge
    // into a drawn line rather than a one-pixel seam...
    let mut drawn_map = detail.clone();
    crate::filters::convolve::gaussian_blur_accelerated(
        &mut drawn_map,
        (width_setting as f32 * SPREAD).min(MAX_LOOK),
    );
    // ...hardened into how much extra stroke the detail earns, and then spread
    // by a pencil width so a wide hand draws a wide line.
    drawn_map
        .as_bytes_mut()
        .par_chunks_exact_mut(4)
        .for_each(|px| {
            let drawn = fade(px[0] as f32 * gain, NOTHING_TO_DRAW, DRAWN_IN_FULL);
            px[0..3].copy_from_slice(&[(drawn * 255.0) as u8; 3]);
        });
    let mut drawn_map = crate::filters::convolve::dilate(
        &drawn_map,
        (((width_setting as f32 * FILL).min(MAX_FILL)).round() as i32).max(1),
    );
    crate::filters::convolve::gaussian_blur_accelerated(
        &mut drawn_map,
        (width_setting as f32 * 0.3).min(MAX_FILL / 2.0),
    );
    // The same detail thickened to the pencil's own width, which is what lays
    // the outline along a boundary.
    let outline = crate::filters::convolve::dilate(
        &detail,
        ((width_setting as f32 * OUTLINE).round() as i32).max(1),
    );
    // The colour the pencil lays: washed, so a petal is a wash of pink with
    // the hatch over it rather than a photograph of a petal.
    let mut wash = pixmap.clone();
    crate::filters::convolve::gaussian_blur_accelerated(&mut wash, WASH);
    let (drawn_map, outline, wash) = (&drawn_map, &outline, &wash);

    let w = pixmap.width() as i32;
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            for x in 0..w {
                // How much extra the detail here earns over the flat base.
                let detail_drawn = drawn_map.get(x, y).r as f32 / 255.0;
                let stroke = STROKE_BASE + (1.0 - STROKE_BASE) * detail_drawn;

                let hatch = hatch(x, y, width_setting);
                // The hatch thins the stroke where it thins, and leaves a
                // faint stroke of its own on bare paper. A hard hand fills
                // the gaps between its strokes and a light one leaves the
                // paper showing between them, which is most of what Stroke
                // Pressure looks like.
                let coverage = press * stroke * (1.0 - gap * (1.0 - hatch));
                // The stroke's own faint lead, strongest down its middle and
                // nothing in the gap, which is what leaves the diagonal marks
                // across a patch of bare paper.
                let ink = ((hatch - 0.6).max(0.0) / 0.4) * HATCH_INK * gain;
                // The harder the hand presses, the darker the lead lies.
                let pressed = (outline.get(x, y).r as f32 / 255.0 * gain).min(1.0);
                let darken = 1.0 - EDGE_DARKENING * pressed;

                let colour = wash.get(x, y);
                let i = x as usize * 4;
                for (c, value) in [colour.r, colour.g, colour.b].into_iter().enumerate() {
                    let lead = value as f32 * darken;
                    out[i + c] = (paper + (lead - paper) * coverage - ink)
                        .clamp(0.0, 255.0)
                        .round() as u8;
                }
                // Alpha stands: drawing on paper does not change the layer's
                // shape.
            }
        });
}

/// How much fine detail sits at each pixel, in levels, as a picture in its own
/// right: how far it is from what a blur of [`DETAIL_SCALE`] leaves.
fn detail_map(pixmap: &Pixmap) -> Pixmap {
    let mut smoothed = pixmap.clone();
    crate::filters::convolve::gaussian_blur_accelerated(&mut smoothed, DETAIL_SCALE);
    let (sharp, soft) = (&*pixmap, &smoothed);
    let w = pixmap.width() as i32;

    let mut out = Pixmap::new(pixmap.width(), pixmap.height());
    let stride = out.stride();
    out.as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            for x in 0..w {
                let lost = (luma(sharp.get(x, y)) - luma(soft.get(x, y)))
                    .abs()
                    .min(255.0) as u8;
                let i = x as usize * 4;
                out[i..i + 3].copy_from_slice(&[lost; 3]);
                out[i + 3] = 255;
            }
        });
    out
}

/// A smooth 0..1 ramp between two thresholds — nothing below `from`, all of it
/// above `to`, and no hard line anywhere for the eye to find.
fn fade(value: f32, from: f32, to: f32) -> f32 {
    let t = ((value - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn luma(c: Rgba8) -> f32 {
    0.299 * c.r as f32 + 0.587 * c.g as f32 + 0.114 * c.b as f32
}

/// How much pencil this pixel has on it, from 0 for bare paper to 1 for the
/// middle of a stroke.
///
/// Two hatches at right angles — the second at half strength, which is what
/// makes it read as a crosshatch rather than as two equal grids — each broken
/// into dashes along its length so that no line runs the width of the picture.
fn hatch(x: i32, y: i32, width: u32) -> f32 {
    let spacing = width.max(1) as f32 * HATCH_SPACING;
    // The two diagonals. `across` counts stripes, `along` runs down a stroke.
    let one = stroke(
        (x + y) as f32,
        (x - y) as f32,
        spacing,
        0,
    );
    let other = stroke(
        (x - y) as f32,
        (x + y) as f32,
        spacing,
        1,
    );
    one.max(other * 0.45)
}

/// One direction of the hatch.
fn stroke(across: f32, along: f32, spacing: f32, seed: u32) -> f32 {
    // Which stripe this is, and where in it we are.
    let stripe = (across / spacing).floor();
    // A dash is a few stripe-widths of stroke with a gap after it; jittering
    // the phase per stripe keeps the dashes from lining up into a grid.
    let jitter = noise(stripe as i32, seed as i32) * spacing;
    let t = ((across + jitter) / spacing).fract().abs();
    // A ridge across the stripe: strongest down its middle, nothing at its
    // edges. The exponent narrows it, so the stroke covers well under half
    // the stripe and the paper shows between one line and the next — without
    // it the hatch is a solid fill and the picture reads as a wash.
    let ridge = (1.0 - (2.0 * t - 1.0).abs()).powf(STROKE_SHARPNESS);

    let dash = noise((along / (spacing * 3.0)).floor() as i32, stripe as i32 + 31);
    ridge * (0.35 + 0.65 * dash)
}

/// Deterministic 0..1 noise from two whole numbers.
fn noise(a: i32, b: i32) -> f32 {
    let mut h = (a as u32)
        .wrapping_mul(0x27d4_eb2d)
        ^ (b as u32).wrapping_mul(0x1656_67b1)
        ^ 0x9e37_79b9;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545_f491);
    h ^= h >> 13;
    (h % 1024) as f32 / 1023.0
}

/// The smallest median the picture is flattened by, and how much further each
/// step of Edge Simplicity takes it, in pixels.
///
/// A median is the right tool for cutting paper: it rubs out anything smaller
/// than half its window outright rather than fading it, and leaves the
/// boundaries it keeps as sharp as it found them. A blur would do the opposite
/// of both. The floor is there because Edge Simplicity 0 still asks for shapes
/// rather than for the photograph's own grain.
const SIMPLIFY_FLOOR: u32 = 1;
const SIMPLIFY_PER_STEP: u32 = 1;

/// Filter ▸ Artistic ▸ Cutout: the picture rebuilt out of pieces of coloured
/// paper.
///
/// CS6 describes it as a picture "made from roughly cut-out pieces of coloured
/// paper", and that is what the implementation is: the picture is flattened
/// into areas, cut along their boundaries, and each piece is then painted one
/// flat colour.
///
/// * **The flattening** is a median whose reach grows with Edge Simplicity. It
///   is what decides how much detail survives to be cut around at all — the
///   veins in a petal, the grain of a leaf — because a median smaller than a
///   feature keeps it and one larger rubs it out.
/// * **The cut** puts each channel into one of `levels` bands and takes a piece
///   to be a run of pixels agreeing on all three. Bands per channel rather than
///   bands of brightness: a pink petal and a green leaf can be exactly as
///   bright as each other, and cutting on brightness alone would join them into
///   one piece and paint the pair some average mud. Number of Levels is
///   therefore how finely the picture is cut, and the size of the pieces falls
///   away as it rises.
/// * **The colour** of a piece is the mean of what the picture had underneath
///   it. This is the difference between Cutout and Posterize, which is the
///   thing it is most often mistaken for: Posterize snaps every pixel to a
///   fixed grid of values, so a photograph comes back in colours it never
///   contained, while cut paper is chosen to match what it stands for. The
///   result is a palette of the picture's own muted colours rather than of
///   primaries.
/// * **Edge Fidelity** is how closely a piece is allowed to follow the picture.
///   At 3 the cut is taken exactly where the bands fall; below that the map is
///   passed through a majority vote first, which rounds the corners off a piece
///   and drops the single-pixel fringe along its boundary — scissors rather
///   than a scalpel.
///
/// Alpha is left alone: cutting the picture up does not change the layer's
/// shape.
///
/// No GPU path, and this one is not close. Finding the pieces is a flood fill,
/// which is the standing example in CLAUDE.md §7 of what does not fit a shader:
/// it is inherently sequential, each step depending on where the last one got
/// to. The median in front of it is already accelerated on its own account.
pub fn cutout(pixmap: &mut Pixmap, levels: u32, simplicity: u32, fidelity: u32) {
    if pixmap.is_empty() {
        return;
    }
    let levels = levels.clamp(*CUTOUT_LEVELS.start(), *CUTOUT_LEVELS.end());
    let simplicity = simplicity.clamp(*CUTOUT_SIMPLICITY.start(), *CUTOUT_SIMPLICITY.end());
    let fidelity = fidelity.clamp(*CUTOUT_FIDELITY.start(), *CUTOUT_FIDELITY.end());

    // Flatten the picture into areas worth cutting around...
    let mut flat = pixmap.clone();
    let reach = SIMPLIFY_FLOOR + simplicity * SIMPLIFY_PER_STEP;
    crate::filters::convolve::median_filter(&mut flat, reach);
    // ...band it, so that "the same colour" becomes a question with a yes or
    // no answer...
    let mut bands = band_map(&flat, levels);
    // ...and let the scissors round off what the bands left ragged. Fidelity 3
    // is the scalpel and skips the vote entirely.
    let vote = (*CUTOUT_FIDELITY.end() - fidelity) as i32;
    if vote > 0 {
        bands = tidy(&bands, vote);
    }

    paint_pieces(pixmap, &bands);
}

/// Which band each channel of each pixel falls in, as a picture in its own
/// right — `r`, `g` and `b` hold band numbers rather than colours.
fn band_map(source: &Pixmap, levels: u32) -> Pixmap {
    let mut out = Pixmap::new(source.width(), source.height());
    let stride = out.stride();
    out.as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .zip(source.as_bytes().par_chunks_exact(stride))
        .for_each(|(out, src)| {
            for (out, src) in out.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                for c in 0..3 {
                    // 256 rather than 255, so that every band is the same
                    // width and only 255 itself would overflow the top one.
                    out[c] = ((src[c] as u32 * levels) / 256).min(levels - 1) as u8;
                }
                out[3] = 255;
            }
        });
    out
}

/// The band map with each pixel replaced by whichever banding is commonest
/// within `radius` of it — the majority vote behind Edge Fidelity.
///
/// A tie is settled in favour of the pixel's own banding, so a vote can round a
/// corner off or rub out a fringe but never moves a boundary that both sides
/// agree on.
fn tidy(bands: &Pixmap, radius: i32) -> Pixmap {
    let (w, h) = (bands.width() as i32, bands.height() as i32);
    let mut out = bands.clone();
    let stride = out.stride();
    out.as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            // At most 5×5 candidates, so a linear scan beats any map.
            let mut seen: Vec<(u32, u32)> = Vec::with_capacity(25);
            for x in 0..w {
                seen.clear();
                let mine = packed(bands.get(x, y));
                for yy in (y - radius).max(0)..=(y + radius).min(h - 1) {
                    for xx in (x - radius).max(0)..=(x + radius).min(w - 1) {
                        let key = packed(bands.get(xx, yy));
                        match seen.iter_mut().find(|(k, _)| *k == key) {
                            Some((_, n)) => *n += 1,
                            None => seen.push((key, 1)),
                        }
                    }
                }
                let best = seen.iter().map(|&(_, n)| n).max().unwrap_or(0);
                let won = seen
                    .iter()
                    .find(|&&(k, n)| n == best && k == mine)
                    .or_else(|| seen.iter().find(|&&(_, n)| n == best))
                    .map(|&(k, _)| k)
                    .unwrap_or(mine);
                let i = x as usize * 4;
                out[i] = (won >> 16) as u8;
                out[i + 1] = (won >> 8) as u8;
                out[i + 2] = won as u8;
            }
        });
    out
}

/// One pixel's banding as a single number, so that "the same piece of paper"
/// is one comparison rather than three.
fn packed(c: Rgba8) -> u32 {
    (c.r as u32) << 16 | (c.g as u32) << 8 | c.b as u32
}

/// Cut `pixmap` along the boundaries in `bands` and paint each piece the mean
/// of what it covers.
///
/// A piece is a run of pixels reachable from one another through neighbours
/// with the same banding — four-connected, so that two areas touching only at a
/// corner are two pieces, which is what a pair of scissors would leave.
///
/// The fill walks whole rows at a time rather than single pixels. A piece is
/// routinely most of the picture — a sky, or the background this filter is
/// usually pointed at — and a stack holding one entry per pixel in it would be
/// both slower and, on a photograph, hundreds of megabytes.
fn paint_pieces(pixmap: &mut Pixmap, bands: &Pixmap) {
    let (w, h) = (pixmap.width() as i32, pixmap.height() as i32);
    let count = (w * h) as usize;
    const UNCUT: u32 = u32::MAX;

    let key = |x: i32, y: i32| packed(bands.get(x, y));
    let mut piece = vec![UNCUT; count];
    // Per piece: the running total of the colours under it, and how many
    // pixels that is.
    let mut totals: Vec<([u64; 3], u64)> = Vec::new();
    let mut runs: Vec<i32> = Vec::new();

    for seed in 0..count {
        if piece[seed] != UNCUT {
            continue;
        }
        let id = totals.len() as u32;
        totals.push(([0; 3], 0));
        let ([r, g, b], n) = &mut totals[id as usize];
        let mine = key(seed as i32 % w, seed as i32 / w);
        runs.push(seed as i32);

        while let Some(start) = runs.pop() {
            let (y, x0) = (start / w, start % w);
            // Another run may have reached this one between being noted and
            // being taken up.
            if piece[start as usize] != UNCUT {
                continue;
            }
            // How far the run reaches either way along its row.
            let mut lo = x0;
            while lo > 0 && key(lo - 1, y) == mine {
                lo -= 1;
            }
            let mut hi = x0;
            while hi + 1 < w && key(hi + 1, y) == mine {
                hi += 1;
            }
            for x in lo..=hi {
                let i = (y * w + x) as usize;
                piece[i] = id;
                let px = pixmap.get(x, y);
                *r += px.r as u64;
                *g += px.g as u64;
                *b += px.b as u64;
                *n += 1;
            }
            // The rows above and below, which the run may have opened up:
            // note the start of each stretch of them that belongs to this
            // piece and has not been taken yet.
            for row in [y - 1, y + 1] {
                if row < 0 || row >= h {
                    continue;
                }
                let ours =
                    |x: i32| piece[(row * w + x) as usize] == UNCUT && key(x, row) == mine;
                let mut x = lo;
                while x <= hi {
                    if ours(x) {
                        runs.push(row * w + x);
                        while x <= hi && ours(x) {
                            x += 1;
                        }
                    } else {
                        x += 1;
                    }
                }
            }
        }
    }

    let colours: Vec<[u8; 3]> = totals
        .iter()
        .map(|&([r, g, b], n)| {
            let n = n.max(1);
            [(r / n) as u8, (g / n) as u8, (b / n) as u8]
        })
        .collect();

    let stride = pixmap.stride();
    let (colours, piece) = (&colours, &piece);
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let first = row * w as usize;
            for (x, out) in out.chunks_exact_mut(4).enumerate() {
                out[0..3].copy_from_slice(&colours[piece[first + x] as usize]);
                // Alpha stands: cutting the picture up does not change the
                // layer's shape.
            }
        });
}

/// How many colours the paint is mixed from at Brush Detail 0, and how many
/// more each step of the slider adds.
///
/// CS6 says Dry Brush "simplifies an image by reducing its range of colours to
/// areas of common colour", and this is that range. At the bottom of the
/// slider the picture is painted out of a handful of colours and comes back
/// poster-like; at the top the steps are finer than the eye can find and what
/// is left is the brushwork alone.
const PAINT_FLOOR: u32 = 4;
const PAINT_PER_STEP: u32 = 6;

/// How much relief each step of Texture raises the brushwork into.
///
/// Texture is **not** grain sprinkled over the picture. Sprinkling is what it
/// looks like it ought to be and the result is unmistakable — it reads as
/// sensor noise, not as paint — and setting it low enough to stop reading as
/// noise leaves a slider that does nothing at all. Both were tried.
///
/// What the slider does in CS6 is give the paint *body*: at 3 the same picture
/// comes back crunchier than at 1, with the strokes standing out from each
/// other and the surfaces rougher, which is what paint laid on thickly looks
/// like. So it is worked as relief — every facet the brush left is pushed
/// further from its neighbours, at the brush's own scale — rather than as
/// anything added on top.
const RELIEF_PER_STEP: f32 = 0.5;

/// The most, in levels, that the relief may move any one pixel.
///
/// Without it the slider grows a white rim around every dark background: the
/// step from a petal to the grass behind it is a hundred levels, and half of
/// that added back is a bloom along the whole outline. The steps between
/// facets, which are what the relief is *for*, are a few levels each and never
/// come near this. So the cap costs the effect nothing and takes the halo off
/// it — the same bargain as Unsharp Mask's threshold, from the other end.
const RELIEF_CAP: f32 = 14.0;

/// How deep the canvas's own tooth bites, in levels, and how many pixels across
/// one tooth of it is. The faint surface under the paint, there at every
/// setting: it is what the picture is painted *on*, not what Texture does.
const TOOTH_DEPTH: f32 = 1.2;
const TOOTH_SCALE: i32 = 3;

/// Filter ▸ Artistic ▸ Dry Brush: the picture repainted with a stiff, half-dry
/// brush — CS6 puts it "between oil and watercolour".
///
/// The brush is the whole filter. A dab of a dry brush picks up one load of
/// colour and puts it down flat, so a stroke lays a patch of a single colour
/// rather than a blend of everything under it — and a painter working up to a
/// boundary loads the brush from *one side* of it, never from across it. That
/// is what keeps a painting's edges crisp while its surfaces go flat, and it
/// is the behaviour to reproduce.
///
/// So each pixel looks at the four square areas that have it at a corner, asks
/// which of them the picture is calmest over, and takes that one's average
/// colour. On a flat surface all four answer much the same and the texture
/// averages away; against a boundary the two quadrants lying across it are in
/// uproar and lose to the two that do not, so the pixel is painted from its own
/// side and the edge stays where it was. Both questions are answered from box
/// blurs — the colour from one over the picture, the calmness from one over how
/// far the picture strays from its own local average — so the cost per pixel
/// does not grow with the brush.
///
/// **Brush Size** is how wide that load is. **Brush Detail** is how many
/// colours the paint is mixed from, which is CS6's own account of the slider —
/// see [`PAINT_FLOOR`]. **Texture** is how much body the paint is laid on with,
/// worked as relief on the brushwork the pass above left — see
/// [`RELIEF_PER_STEP`]. Under all of it is the canvas's own faint tooth, seeded
/// from each pixel's coordinates rather than from a RNG, so a preview, the
/// commit behind it and an undo/redo replay all show the same surface.
///
/// Alpha is left alone: repainting the picture does not change the layer's
/// shape.
///
/// No GPU path. It is per-neighbourhood and uniform, so unlike Cutout it would
/// fit a shader — but it is four box blurs and a choice between four lookups,
/// and like the rest of the filter stack it would upload its input and read the
/// result straight back, which docs/gpu-migration.md says rarely pays for the
/// trip.
pub fn dry_brush(pixmap: &mut Pixmap, size: u32, detail: u32, texture: u32) {
    if pixmap.is_empty() {
        return;
    }
    let size = size.clamp(*BRUSH_SIZE.start(), *BRUSH_SIZE.end());
    let detail = detail.clamp(*BRUSH_DETAIL.start(), *BRUSH_DETAIL.end());
    let texture = texture.clamp(*BRUSH_TEXTURE.start(), *BRUSH_TEXTURE.end());

    // Brush Size is the half-width of one load, and also how far the four of
    // them sit from the pixel — so the brush considers `4 * size + 1` across
    // and the facets it leaves are about a load wide. Size 0 is no brush at
    // all rather than the smallest one: every load is then the pixel itself
    // and nothing is painted, which is what the bottom of a slider should
    // mean.
    let reach = size;
    paint_in_dabs(pixmap, reach, false);

    // Lay it on thickly, at the scale of the brush that put it there — the
    // relief is of the facets, so anything finer than one would raise the
    // picture's own grain instead of the paint.
    raise_the_paint(
        pixmap,
        reach.max(1) as f32,
        (texture - *BRUSH_TEXTURE.start()) as f32 * RELIEF_PER_STEP,
    );
    finish(pixmap, PAINT_FLOOR + detail * PAINT_PER_STEP, TOOTH_DEPTH);
}

/// Repaint the picture a dab at a time, each dab one flat load of colour taken
/// from whichever side of itself the picture is calmest over.
///
/// Shared by Dry Brush and Fresco, which CS6 gives the same three sliders and
/// which differ in what happens *after* the painting rather than in the
/// painting. `round` is the shape of the dab: square for a stiff flat brush,
/// circular for the rounded dabs a fresco is laid in.
fn paint_in_dabs(pixmap: &mut Pixmap, reach: u32, round: bool) {
    if reach == 0 {
        return;
    }
    let mut load = pixmap.clone();
    if round {
        crate::filters::convolve::disc_blur(&mut load, reach);
    } else {
        crate::filters::convolve::box_blur(&mut load, reach);
    }
    let calm = roughness(pixmap, reach);
    lay_the_paint(pixmap, &load, &calm, reach as i32);
}

/// How hard the plaster takes the pigment down, as the exponent of a curve over
/// the tonal range.
///
/// This is the whole difference between Fresco and Dry Brush, which CS6 gives
/// the same three sliders and the same coarse dabs. Pigment laid into wet
/// plaster sinks in and dries dark, and the filter is emphatic about it: the
/// grass behind these flowers goes from a middling green to very nearly black,
/// which is a square of the tone it had.
///
/// [`BURNISH`] is the other end of the same curve. The darks going down is only
/// half of what the reference shows — its petals come back *brighter* than the
/// photograph's, not merely deeper — so the range is carried a little past
/// white before the curve is taken. What was already light is lifted and
/// everything below it still sinks. A plain gamma gives the blacks and loses
/// the flowers.
const PLASTER: f32 = 2.0;
const BURNISH: f32 = 1.1;

/// Filter ▸ Artistic ▸ Fresco: the picture laid into wet plaster.
///
/// CS6 paints it "in a coarse style using short, rounded, and hastily applied
/// dabs", and gives it Dry Brush's three sliders because it is Dry Brush's
/// brush — the same load of flat colour taken from whichever side of a boundary
/// the picture is calm over, which is what keeps a painting's edges while its
/// surfaces go flat. Two things differ, and the second is the one that matters:
///
/// * **The dab is round**, not square. A fresco is laid in with the tip.
/// * **The plaster takes the pigment down.** See [`PLASTER`]: the darks go very
///   dark, the lights stay where they are, and the picture comes back with the
///   weight the reference has. Without it this is Dry Brush with a different
///   name on the menu.
///
/// **Brush Size**, **Brush Detail** and **Texture** mean exactly what they mean
/// in [`dry_brush`], down to the ranges, because in CS6 they are the same three
/// sliders.
///
/// Alpha is left alone, and there is no GPU path, for the same reasons as
/// [`dry_brush`].
pub fn fresco(pixmap: &mut Pixmap, size: u32, detail: u32, texture: u32) {
    if pixmap.is_empty() {
        return;
    }
    let size = size.clamp(*BRUSH_SIZE.start(), *BRUSH_SIZE.end());
    let detail = detail.clamp(*BRUSH_DETAIL.start(), *BRUSH_DETAIL.end());
    let texture = texture.clamp(*BRUSH_TEXTURE.start(), *BRUSH_TEXTURE.end());

    // A coarser hand than Dry Brush's at the same setting — "coarse" and
    // "hastily applied" are CS6's own words for it, and the reference's dabs
    // are plainly larger than the same number gives there. Size 0 is still no
    // brush at all.
    let reach = size * 2;
    paint_in_dabs(pixmap, reach, true);
    sink_into_the_plaster(pixmap);
    raise_the_paint(
        pixmap,
        reach.max(1) as f32,
        (texture - *BRUSH_TEXTURE.start()) as f32 * RELIEF_PER_STEP,
    );
    finish(pixmap, PAINT_FLOOR + detail * PAINT_PER_STEP, TOOTH_DEPTH);
}

/// Take the picture's darks down the way wet plaster takes pigment down.
///
/// Per channel rather than on brightness, so that what a colour loses is its
/// weakest channel first — which is what deepens a colour instead of merely
/// dimming it, and why the reference's greens go black while its pinks only go
/// redder.
fn sink_into_the_plaster(pixmap: &mut Pixmap) {
    // 256 entries is cheaper than a `powf` per channel per pixel, and exact:
    // there are only 256 answers.
    let sunk: [u8; 256] = std::array::from_fn(|v| {
        let carried = (v as f32 / 255.0 * BURNISH).min(1.0);
        (255.0 * carried.powf(PLASTER)).round() as u8
    });
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(4)
        .for_each(|px| {
            for c in 0..3 {
                px[c] = sunk[px[c] as usize];
            }
            // Alpha stands: laying the picture into plaster does not change
            // the layer's shape.
        });
}

/// How wide a daub is, in pixels per step of Brush Size, and how different two
/// colours may be before the brush treats them as different things.
///
/// The second is what makes these daubs rather than a blur: a neighbour only
/// goes into the daub if it is within this of the pixel being painted, so a
/// daub spreads through a petal and stops dead at its outline however wide the
/// brush is set.
const DAUB_WIDTH: f32 = 0.9;
const DAUB_REGION: u32 = 65;

/// How much broader the two Wide brushes are than the rest.
const WIDE_DAUB: f32 = 1.6;

/// What a step of Sharpness is worth to a brush that paints and to one that
/// scrubs.
///
/// The two are not the same measurement, and conflating them is the trap here.
/// On the painting brushes Sharpness is **definition between one daub and the
/// next** — relief at the daub's own scale, exactly as Dry Brush's Texture
/// works. It must not be built out of the *photograph's* detail: putting that
/// back is undoing the daubing, and a mid-slider setting then returns most of
/// the picture and leaves the filter looking like it did nothing.
///
/// On the Rough brushes it is how hard what the daubing threw away is scrubbed
/// back on, which is a different thing entirely and an order of magnitude
/// larger. That is where the reference's texture comes from, and — where the
/// picture was smooth and the leftovers are a level or two of gradient — its
/// contour ribbons.
const SHARPNESS_PER_STEP: f32 = 0.03;
const ROUGH_PER_STEP: f32 = 0.3;

/// What a step of Sharpness is worth to Wide Blurry, which lays neither
/// definition nor a scrub but the grain of the paint.
const STIPPLE_PER_STEP: f32 = 0.2;

/// The scale, in pixels, at which the picture counts as *grain* — what a blur
/// this small takes away is the finest thing it has, and nothing of its shapes.
const GRAIN_SCALE: f32 = 1.2;

/// How much wider than the daub the Rough brushes look when they ask what the
/// broad shading here is. Wide enough that the shading itself is part of what
/// they scrub back on, which is where the ribbons come from.
const BROAD_SCALE: f32 = 2.0;

/// Filter ▸ Artistic ▸ Paint Daubs: the picture repainted in daubs of one
/// colour, with whatever the daubing could not hold put back on top.
///
/// Two passes, and every one of CS6's six brushes is a setting of the second.
///
/// 1. **The daubs.** Each pixel is averaged with the neighbours within Brush
///    Size *that are near enough in colour to belong to the same thing* — see
///    [`DAUB_REGION`]. A petal's inside washes together into one soft daub and
///    its outline survives untouched, which is what the reference's Simple
///    brush is: creamy, edgeless within a shape, and crisp at every boundary.
/// 2. **What the daubs could not hold.** The difference between the picture and
///    its daubs is every fine thing the first pass threw away — the grain, the
///    veins, the stamens. Putting a little back is definition. Putting a great
///    deal back is what the Rough brushes do, and it is worth understanding
///    *why they look the way they do*: where the picture was smooth, the
///    leftovers are a level or two of gradient, and multiplying that by twenty
///    turns a gentle background into bands and ribbons. The wiggling contours
///    all over CS6's Dark Rough and Sparkle are not a texture pasted on. They
///    are the photograph's own gradients, amplified until they band.
///
/// **Brush Size** is the daub. **Sharpness** is how much goes back on. **Brush
/// Type** decides *which* of it goes back: all of it, only what lightens, only
/// what darkens, or none.
///
/// Alpha is left alone: repainting the picture does not change the layer's
/// shape.
///
/// No GPU path. The daubing is [`crate::filters::convolve::surface_blur`],
/// which is a sliding histogram — sequential along each row by construction,
/// and the reason it is fast enough to offer a fifty-pixel brush at all.
pub fn paint_daubs(pixmap: &mut Pixmap, size: u32, sharpness: u32, brush: DaubBrush) {
    if pixmap.is_empty() {
        return;
    }
    let size = size.clamp(*DAUB_SIZE.start(), *DAUB_SIZE.end());
    let sharpness = sharpness.clamp(*DAUB_SHARPNESS.start(), *DAUB_SHARPNESS.end());

    let wide = matches!(brush, DaubBrush::WideSharp | DaubBrush::WideBlurry);
    let width = size as f32 * DAUB_WIDTH * if wide { WIDE_DAUB } else { 1.0 };

    let picture = pixmap.clone();
    crate::filters::convolve::surface_blur(pixmap, (width.round() as u32).max(1), DAUB_REGION);

    let sharp = sharpness as f32;
    match brush {
        // The painting brushes: definition where two daubs meet, and nothing
        // of the photograph put back.
        DaubBrush::Simple => raise_the_paint(pixmap, width, sharp * SHARPNESS_PER_STEP),
        DaubBrush::WideSharp => raise_the_paint(pixmap, width, sharp * SHARPNESS_PER_STEP * 2.0),
        // A wide daub with a soft edge, which is what shows the grain of the
        // paint rather than the shapes in it: only the finest thing the
        // picture had goes back on, so the surface is stippled and the veins
        // and stamens the daub took out stay out.
        DaubBrush::WideBlurry => {
            let mut grain = picture.clone();
            crate::filters::convolve::gaussian_blur_accelerated(&mut grain, GRAIN_SCALE);
            put_back(
                pixmap,
                &picture,
                &grain,
                sharp * STIPPLE_PER_STEP,
                Leftovers::All,
            );
        }
        // The scrubbing brushes, which measure against the *broad* shading
        // rather than against the daub. See BROAD_SCALE.
        DaubBrush::LightRough => scrub(pixmap, &picture, width, sharp, Leftovers::Lightening, 1.0),
        DaubBrush::DarkRough => scrub(pixmap, &picture, width, sharp, Leftovers::Darkening, 1.0),
        DaubBrush::Sparkle => scrub(pixmap, &picture, width, sharp, Leftovers::All, 2.0),
    }
}

/// Which half of the leftover detail a brush is allowed to put back.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Leftovers {
    All,
    /// Only where the picture was lighter than what it is measured against, so
    /// the brush can scrub a surface white but never dirty it — and the other
    /// way about.
    Lightening,
    Darkening,
}

/// Scrub the picture back on over its daubs, measured against its *broad*
/// shading rather than against the daubs themselves.
///
/// Which of the two it is measured against decides what the brush looks like,
/// and it is not a detail. Measured against the daub, the leftover in a smooth
/// part of the picture is the grain and nothing else — the daub of an
/// out-of-focus background *is* that background — so the brush lays speckle
/// there and the flowing ribbons that cover CS6's Rough and Sparkle never
/// appear. Measured against a blur wide enough to lose the shading too, the
/// leftover in that same smooth part is a level or two of gradient running
/// across the frame, and multiplying that by twenty is exactly what turns it
/// into ribbons. The grain is still in there; it is now the smaller half of
/// what the brush is scrubbing rather than the whole of it.
fn scrub(
    pixmap: &mut Pixmap,
    picture: &Pixmap,
    width: f32,
    sharpness: f32,
    keep: Leftovers,
    hardness: f32,
) {
    // Blurred from the picture rather than from the daubs under it: the daubs
    // are flat patches with steps between them, and at these gains those steps
    // come back as blocks. The picture's own shading has no steps in it.
    let mut broad = picture.clone();
    crate::filters::convolve::gaussian_blur_accelerated(&mut broad, width * BROAD_SCALE);
    put_back(
        pixmap,
        picture,
        &broad,
        sharpness * ROUGH_PER_STEP * hardness,
        keep,
    );
}

/// Add what `over` has and `under` does not to whatever is in `pixmap`.
fn put_back(pixmap: &mut Pixmap, over: &Pixmap, under: &Pixmap, gain: f32, keep: Leftovers) {
    if gain <= 0.0 {
        return;
    }
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .zip(over.as_bytes().par_chunks_exact(stride))
        .zip(under.as_bytes().par_chunks_exact(stride))
        .for_each(|((out, over), under)| {
            for ((out, over), under) in out
                .chunks_exact_mut(4)
                .zip(over.chunks_exact(4))
                .zip(under.chunks_exact(4))
            {
                for c in 0..3 {
                    let left_over = over[c] as f32 - under[c] as f32;
                    let allowed = match keep {
                        Leftovers::All => left_over,
                        Leftovers::Lightening => left_over.max(0.0),
                        Leftovers::Darkening => left_over.min(0.0),
                    };
                    out[c] = (out[c] as f32 + allowed * gain).clamp(0.0, 255.0).round() as u8;
                }
                // Alpha stands: repainting the picture does not change the
                // layer's shape.
            }
        });
}

/// How wide a stroke of the knife is, in pixels per step of Stroke Size.
///
/// This is the size of one **cell**, and that word is the whole of what this
/// filter is. A knife does not spread the picture about the way a brush or a
/// blade does: it puts down one flat mass of colour where it is set down and
/// another beside it, and what the two leave between them is a straight join.
/// So the picture comes back as a mosaic of irregular polygons, exactly the
/// structure Filter ▸ Pixelate ▸ Crystallize builds, which is what the
/// implementation borrows.
///
/// Anything that averages over a window instead — a median, a blur, a
/// quantisation — gives flat *areas* with wandering, rounded boundaries, and
/// they are not the same picture at all. The reference's background is plainly
/// cellular: five- and six-sided, each one flat, meeting along straight lines.
const KNIFE_WIDTH: f32 = 0.4;

/// How far the picture is flattened before the knife touches it, in pixels per
/// step of Stroke Size, how different two colours may be and still be flattened
/// together, and how much more of it a step down in Stroke Detail asks for.
///
/// **Flattened, not blurred, and the difference is the whole filter.** Two
/// things have to be true of the reference at once: a petal comes back as a
/// smooth mass with its veins and stamens gone, *and* the outline of that petal
/// is still crisp enough to tell it from the one behind it. A blur cannot do
/// both — the radius that loses the veins loses the outline with them, and what
/// comes back is a mush. So the flattening only averages neighbours near enough
/// in colour to belong to the same thing, which takes the veins out of a petal
/// and leaves its edge exactly where it was.
///
/// Once that is done, one uniform pass of cells gives the two completely
/// different-looking results the reference shows: a flattened petal is nearly
/// one colour, so its cells all come out alike and it reads as smooth, while
/// the out-of-focus background keeps its broad range of greens and *its* cells
/// come out plainly different from one another and read as a mosaic.
const KNIFE_SMOOTH: f32 = 0.25;
const KNIFE_REGION: u32 = 30;
const KNIFE_DETAIL_PER_STEP: f32 = 0.3;

/// How far Softness carries, in pixels per step.
///
/// Small, because it is the joins between cells being eased and not the
/// picture: at the top of CS6's slider the reference is still crisp everywhere
/// the picture had anything to show.
const KNIFE_SOFTNESS_SCALE: f32 = 0.15;

/// Filter ▸ Artistic ▸ Palette Knife: the picture spread with a knife.
///
/// Three passes, one per slider.
///
/// * **Stroke Size** is how wide one load of the knife is — see
///   [`KNIFE_WIDTH`]. The picture comes back as a mosaic of flat polygonal
///   cells, each the average of what it covered, which is what a knife set down
///   and lifted actually leaves. This is the difference between it and every
///   other painting filter in this module: they average over a window and leave
///   rounded, wandering shapes, and a knife leaves straight joins.
/// * **Stroke Detail** is how much of the picture the knife keeps before it
///   starts — see [`KNIFE_SMOOTH`], which is where this filter is won or lost.
///   Everything is smoothed down first, and even at the top of the slider the
///   veins of a petal do not survive it.
/// * **Softness** is the blade's edge, which eases the joins between one cell
///   and the next.
///
/// Alpha is left alone: spreading the picture does not change the layer's
/// shape.
///
/// No GPU path. The cells come from a jittered lattice of seeds, each pixel
/// hunting the nearest — the same argument as Crystallize, which this borrows.
pub fn palette_knife(pixmap: &mut Pixmap, size: u32, detail: u32, softness: u32) {
    if pixmap.is_empty() {
        return;
    }
    let size = size.clamp(*KNIFE_SIZE.start(), *KNIFE_SIZE.end());
    let detail = detail.clamp(*KNIFE_DETAIL.start(), *KNIFE_DETAIL.end());
    let softness = softness.clamp(*KNIFE_SOFTNESS.start(), *KNIFE_SOFTNESS.end());

    // Take the picture down to masses first, keeping every boundary where it
    // stands. Everything the filter looks like follows from this rather than
    // from the cells; see KNIFE_SMOOTH.
    let held_back = 1.0 + (*KNIFE_DETAIL.end() - detail) as f32 * KNIFE_DETAIL_PER_STEP;
    crate::filters::convolve::surface_blur(
        pixmap,
        ((size as f32 * KNIFE_SMOOTH * held_back).round() as u32).max(1),
        KNIFE_REGION,
    );

    let load = ((size as f32 * KNIFE_WIDTH).round() as u32).max(2);
    crate::filters::pixelate::crystallize(pixmap, load);

    if softness > 0 {
        crate::filters::convolve::gaussian_blur_accelerated(
            pixmap,
            softness as f32 * KNIFE_SOFTNESS_SCALE,
        );
    }
    // Alpha stands throughout: spreading the picture does not change the
    // layer's shape. Every pass above leaves it alone.
}

/// Where on the Glow Brightness slider the lamp's light exactly reaches white.
///
/// Below it the picture lights up; above it the light carries the brightest
/// part *past* white and out the other side, and what was the lightest thing in
/// the frame comes back at the foreground colour. That fold is not a mistake to
/// be clamped away — it is the reason the same filter gives a green flower on a
/// blue ground at one end of the slider and a blue flower on a green ground at
/// the other, which is what CS6 does and what the reference shows.
const NEON_NEUTRAL: f32 = 25.0;

/// How much of the picture's own light the tube picks up and carries, as the
/// power the spread light is raised to.
///
/// A square rather than a straight line, and this is the number that decides
/// whether the filter looks like CS6's. The tube lights *the objects in the
/// picture* — Adobe's own wording — not the frame it stands in, so a dark
/// ground has to come back the colour it was rendered in and nothing else. Taken
/// straight, a ground at a quarter of the range still picks up a quarter of the
/// light and the whole picture goes off in the glow colour, which is a blue
/// photograph rather than a blue glow. Squared, that quarter becomes a
/// sixteenth and the ground stays where it belongs while the flowers light up
/// as hard as before.
const NEON_PICKUP: i32 = 2;

/// Filter ▸ Artistic ▸ Neon Glow: the picture lit by a tube of one colour.
///
/// Three things go in and CS6 asks the dialog for only one of them. **Glow
/// Color** is the swatch. The other two are the **document's foreground and
/// background colours**, which is what the picture is rendered between — so a
/// black-and-white pair gives the grey-and-neon look the filter is known for,
/// and a coloured foreground tints everything the tube does not reach. The
/// dialog does not ask because CS6 does not; the bridge fills them in, as it
/// does for Pointillize and Fibers.
///
/// What happens is one idea carried through:
///
/// 1. **The picture's own light is spread** by Glow Size, so a bright thing
///    lights what is near it. A *negative* size — CS6's slider runs to -24 —
///    lights the shadows instead, and the frame glows inwards from its dark
///    parts.
/// 2. **The lamp is turned up** by Glow Brightness, and the light is added to
///    what the picture already had.
/// 3. **What that comes to is folded back at white**. See [`NEON_NEUTRAL`]:
///    below the middle of the slider the picture lights up in the ordinary way,
///    and above it the brightest parts are driven past white and return as the
///    foreground colour. This is the whole of why the filter's two extremes
///    look like negatives of one another.
/// 4. **The result is rendered between the two swatches**, and carried towards
///    the glow colour by how lit it is.
///
/// Alpha is left alone: lighting the picture does not change the layer's shape.
///
/// No GPU path: per pixel over a blur, and the same round-trip argument as the
/// rest of the filter stack. The blur itself goes through the backend.
pub fn neon_glow(
    pixmap: &mut Pixmap,
    size: i32,
    brightness: u32,
    glow: Rgba8,
    foreground: Rgba8,
    background: Rgba8,
) {
    if pixmap.is_empty() {
        return;
    }
    let size = size.clamp(*NEON_SIZE.start(), *NEON_SIZE.end());
    let brightness = brightness.clamp(*NEON_BRIGHTNESS.start(), *NEON_BRIGHTNESS.end());
    let strength = brightness as f32 / NEON_NEUTRAL;

    // The lamp: the picture's own light, spread by Glow Size. Wound below
    // zero it is the shadows that light up instead.
    let mut lamp = lightness(pixmap, size < 0);
    crate::filters::convolve::gaussian_blur_accelerated(&mut lamp, size.unsigned_abs() as f32);

    let w = pixmap.width() as i32;
    let stride = pixmap.stride();
    let lamp = &lamp;
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            for x in 0..w {
                let i = x as usize * 4;
                let own = luma(Rgba8::new(out[i], out[i + 1], out[i + 2], 255)) / 255.0;
                let lit = lamp.get(x, y).r as f32 / 255.0;
                // How much of the lamp's light lands here. See NEON_PICKUP:
                // what was already dark picks up almost none of it.
                let light = lit.powi(NEON_PICKUP) * strength;
                // What the picture had, plus what the lamp adds, folded back
                // where it runs past white.
                let exposed = own + light;
                let mix = (1.0 - (1.0 - exposed).abs()).clamp(0.0, 1.0);
                // The tube's colour goes where its light *stayed*. Both terms
                // are needed: the light alone would colour what the fold has
                // already driven past white and back to the foreground, which
                // is the one part of the picture that must come back
                // untinted.
                let tint = (mix * light).clamp(0.0, 1.0);
                for (c, (dark, pale)) in [
                    (foreground.r, background.r),
                    (foreground.g, background.g),
                    (foreground.b, background.b),
                ]
                .into_iter()
                .enumerate()
                {
                    let between = dark as f32 + (pale as f32 - dark as f32) * mix;
                    let tube = [glow.r, glow.g, glow.b][c] as f32;
                    let value = between + (tube - between) * tint;
                    out[i + c] = value.clamp(0.0, 255.0).round() as u8;
                }
                // Alpha stands: lighting the picture does not change the
                // layer's shape.
            }
        });
}

/// How light each pixel is, as a picture in its own right — `r`, `g` and `b`
/// all carry the same brightness. `inverted` gives its negative, which is what
/// asks "how *dark* is it here" without a second code path.
fn lightness(pixmap: &Pixmap, inverted: bool) -> Pixmap {
    let w = pixmap.width() as i32;
    let mut out = Pixmap::new(pixmap.width(), pixmap.height());
    let stride = out.stride();
    out.as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            for x in 0..w {
                let l = luma(pixmap.get(x, y)) as u8;
                let i = x as usize * 4;
                out[i..i + 3].copy_from_slice(&[if inverted { 255 - l } else { l }; 3]);
                out[i + 3] = 255;
            }
        });
    out
}

/// How restless the picture is around each pixel, as a picture in its own
/// right — the mean of how far brightness strays from its own local average
/// over a window of `reach`.
///
/// A variance would be the textbook answer and this is its cheaper cousin, the
/// mean absolute deviation. It answers the only question being asked of it —
/// *which of these four areas is the calmest* — with the same ordering, and it
/// stays in the 0..255 the rest of the engine is built around instead of
/// needing a plane of floats per channel.
fn roughness(pixmap: &Pixmap, reach: u32) -> Pixmap {
    let lum = lightness(pixmap, false);
    let stride = lum.stride();
    let mut mean = lum.clone();
    crate::filters::convolve::box_blur(&mut mean, reach);
    let mut strays = lum;
    strays
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .zip(mean.as_bytes().par_chunks_exact(stride))
        .for_each(|(out, mean)| {
            for (out, mean) in out.chunks_exact_mut(4).zip(mean.chunks_exact(4)) {
                let stray = (out[0] as i32 - mean[0] as i32).unsigned_abs() as u8;
                out[0..3].copy_from_slice(&[stray; 3]);
            }
        });
    crate::filters::convolve::box_blur(&mut strays, reach);
    strays
}

/// Paint each pixel from whichever of its four brush loads the picture is
/// calmest over.
///
/// **Picked outright, not blended between.** Leaning on all four by how calm
/// each is looks like the gentler, better-behaved thing to do, and it undoes
/// the filter: the four loads either side of a boundary are equally calm, so
/// the lean is even and the edge averages into a smear, and on a flat surface
/// the blend of four overlapping means is just the picture again, smoothed.
/// What is left is the photograph. The whole of the brushwork — the flat facet,
/// the hard step from one to the next — is in the committing to one load and
/// discarding the other three, so that is what this does.
fn lay_the_paint(pixmap: &mut Pixmap, load: &Pixmap, calm: &Pixmap, reach: i32) {
    let (w, h) = (pixmap.width() as i32, pixmap.height() as i32);
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            for x in 0..w {
                // The four areas with this pixel at a corner, named by where
                // their middles are. Clamped, so a pixel at the frame's edge
                // dips its brush inside the picture rather than off it.
                let i = x as usize * 4;
                let mine = luma(Rgba8::new(out[i], out[i + 1], out[i + 2], 255));
                let mut best = (u32::MAX, 0i32, 0i32);
                for (dx, dy) in [(-reach, -reach), (reach, -reach), (-reach, reach), (reach, reach)]
                {
                    let (sx, sy) = ((x + dx).clamp(0, w - 1), (y + dy).clamp(0, h - 1));
                    let stray = calm.get(sx, sy).r as u32;
                    // Two loads can be exactly as calm as each other — one
                    // either side of a boundary usually are, and over a weave
                    // or a field of grass all four routinely are. Settle it on
                    // which came back with a colour nearer the pixel's own,
                    // which is the side of the boundary this pixel is on;
                    // otherwise the answer turns on the order they happen to
                    // be listed in and flickers from pixel to pixel.
                    let apart = (mine - luma(load.get(sx, sy))).abs() as u32;
                    let score = stray << 9 | apart.min(511);
                    if score < best.0 {
                        best = (score, sx, sy);
                    }
                }
                let colour = load.get(best.1, best.2);
                out[i] = colour.r;
                out[i + 1] = colour.g;
                out[i + 2] = colour.b;
                // Alpha stands: repainting the picture does not change the
                // layer's shape.
            }
        });
}

/// Stand every facet further from its neighbours — the body of the paint.
///
/// What is added back is what a blur of the brush's own scale takes away, which
/// on a picture the brush has just been over is the facets and the steps
/// between them and nothing else: there is no finer detail left for it to find.
/// So this raises the brushwork rather than the photograph, which is the
/// difference between paint laid on thickly and a sharpened snapshot.
fn raise_the_paint(pixmap: &mut Pixmap, scale: f32, amount: f32) {
    if amount <= 0.0 || pixmap.is_empty() {
        return;
    }
    let mut flattened = pixmap.clone();
    crate::filters::convolve::gaussian_blur_accelerated(&mut flattened, scale);
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .zip(flattened.as_bytes().par_chunks_exact(stride))
        .for_each(|(out, flat)| {
            for (out, flat) in out.chunks_exact_mut(4).zip(flat.chunks_exact(4)) {
                for c in 0..3 {
                    let lift = ((out[c] as f32 - flat[c] as f32) * amount)
                        .clamp(-RELIEF_CAP, RELIEF_CAP);
                    out[c] = (out[c] as f32 + lift).clamp(0.0, 255.0).round() as u8;
                }
            }
        });
}

/// How coarse the grain goes at the top of the Grain slider — the heaviest
/// clump of silver it can lay, in levels.
///
/// Over half the tonal range, which looks far too much written down and is not:
/// [`speck`] bunches its draws towards the middle, so the *typical* deposit is
/// nearer a third of this and only the rare clump gets anywhere near it. That
/// is the shape of the thing being reproduced — a fast film is mostly faint
/// with the odd heavy grain — and the number to compare against CS6 is what a
/// mid setting looks like, not what the arithmetic allows at the end.
const GRAIN_MAX: f32 = 140.0;

/// How gradually a value becomes a highlight once it is over the line.
///
/// The line itself runs the whole way: at Highlight Area 0 it sits at white, so
/// nothing in the picture is over it and the filter is its grain and nothing
/// else — which is CS6's default, and what its own screenshot of the default
/// shows — and at 20 it sits at black and everything is.
const HIGHLIGHT_SPREAD: f32 = 48.0;

/// How far towards white the lit part of the picture is carried at Intensity
/// 10, as a fraction of what is left between it and white.
const HIGHLIGHT_PULL: f32 = 0.5;

/// How much of its grain the lit part gives up, and how far the grain it keeps
/// drifts from grey into colour.
///
/// Both come straight out of CS6's one-line account of the filter — "a
/// smoother, more saturated pattern is added to the image's lighter areas" —
/// and they are what stops Film Grain being Add Noise with extra steps. Smooth
/// and coloured in the light, even and grey in the shadows and midtones.
const HIGHLIGHT_SMOOTHING: f32 = 0.7;
const HIGHLIGHT_COLOUR: f32 = 0.6;

/// Filter ▸ Artistic ▸ Film Grain: the picture as a fast film would have taken
/// it.
///
/// CS6 describes it in one line — "applies an even pattern to the shadow tones
/// and midtones; a smoother, more saturated pattern is added to the image's
/// lighter areas" — and both halves of that sentence are load-bearing. An even
/// pattern everywhere is Filter ▸ Noise ▸ Add Noise, which this is not.
///
/// * **Grain** is how coarse the silver is, in levels. It is laid as one offset
///   across all three channels rather than three separate ones, because that is
///   what grain in a film emulsion is: clumps of silver that are there or not
///   there, which lighten and darken without tinting.
/// * **Highlight Area** is how far down the range counts as "the lighter
///   areas". At 0 the line sits at white and nothing is above it, which is why
///   CS6's default settings come back as the picture with grain on it and
///   nothing else. Wound up, the line drops into the midtones and takes more
///   and more of the picture with it.
///
///   **Each channel is asked separately**, not the pixel's brightness, and this
///   is the single decision that makes the filter look like CS6's rather than
///   like a wash. A dark green — the grass behind these flowers is (40, 90, 30)
///   — has one channel over the line and two under it, so green alone is
///   carried up and what comes back is a *vivid* green, not a paler one.
///   Testing the brightness instead lifts all three together, which is the
///   definition of washing a colour out. It is also what "a more saturated
///   pattern in the lighter areas" means: the saturation is not added, it falls
///   out of asking each channel where it stands.
/// * **Intensity** is how hard that lighter part is then carried towards white.
///   It has nothing to work on until Highlight Area gives it something, which
///   is the pair's whole relationship and is worth knowing before wondering why
///   a slider at 10 is doing nothing.
///
/// Where the two overlap, the grain also goes *smoother* and *more coloured* —
/// a highlight on film is a thinner, finer deposit, and what grain is left in
/// it sits in the dye layers rather than in the silver.
///
/// The grain is seeded from each pixel's coordinates rather than from a RNG, so
/// a preview, the commit behind it and an undo/redo replay all show the same
/// film.
///
/// Alpha is left alone: exposing the picture differently does not change the
/// layer's shape.
///
/// No GPU path. It is the most shader-shaped operation in this module — per
/// pixel, no neighbourhood at all — but it is also one pass over the picture
/// doing a dozen operations per pixel, so the upload and the read straight back
/// would cost more than the arithmetic they carried. docs/gpu-migration.md has
/// the measurements.
pub fn film_grain(pixmap: &mut Pixmap, grain: u32, highlight_area: u32, intensity: u32) {
    if pixmap.is_empty() {
        return;
    }
    let grain = grain.clamp(*FILM_GRAIN.start(), *FILM_GRAIN.end());
    let highlight_area = highlight_area.clamp(*FILM_HIGHLIGHT.start(), *FILM_HIGHLIGHT.end());
    let intensity = intensity.clamp(*FILM_INTENSITY.start(), *FILM_INTENSITY.end());

    let coarseness = grain as f32 / *FILM_GRAIN.end() as f32 * GRAIN_MAX;
    // Where the highlights start. At white when the area is shut, so nothing
    // is one; at black when it is fully open, so everything is.
    let line = 255.0 * (1.0 - highlight_area as f32 / *FILM_HIGHLIGHT.end() as f32);
    let pull = intensity as f32 / *FILM_INTENSITY.end() as f32 * HIGHLIGHT_PULL;

    let w = pixmap.width() as i32;
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            for x in 0..w {
                let i = x as usize * 4;
                let silver = speck(x, y, 0);
                for c in 0..3 {
                    // Where this channel — not this pixel — stands against the
                    // line.
                    let lit = fade(out[i + c] as f32, line, line + HIGHLIGHT_SPREAD);
                    // Thinner deposit in the light, so less of it and finer.
                    let amount = coarseness * (1.0 - HIGHLIGHT_SMOOTHING * lit);
                    // Grey in the shadows and midtones, drifting into the dye
                    // layers as the picture lightens.
                    let deposit = silver + lit * HIGHLIGHT_COLOUR * (speck(x, y, c + 1) - silver);
                    let value = out[i + c] as f32 + deposit * amount;
                    let exposed = value + (255.0 - value) * lit * pull;
                    out[i + c] = exposed.clamp(0.0, 255.0).round() as u8;
                }
                // Alpha stands: exposing the picture differently does not
                // change the layer's shape.
            }
        });
}

/// One clump of silver, from -1 to 1.
///
/// Two draws averaged rather than one, which bunches the result towards the
/// middle. Grain that is uniformly anything between its extremes reads as
/// television static; a film's is mostly faint with the odd heavy clump, and
/// two draws is the cheapest thing that looks like that.
fn speck(x: i32, y: i32, layer: usize) -> f32 {
    let salt = layer as i32 * 977;
    noise(x + salt, y - salt) + noise(y * 3 + salt, x * 5 + salt) - 1.0
}

/// The canvas's tooth at a pixel, from -1 in a dip to 1 on a rise.
///
/// Value noise on a grid of [`TOOTH_SCALE`], carried smoothly between the
/// corners rather than held flat across each cell. The interpolation is the
/// whole point: noise taken straight off a grid is squares, and noise taken per
/// pixel is static, while a weave is neither — it has a size, and it runs into
/// itself.
fn weave(x: i32, y: i32) -> f32 {
    let (cx, cy) = (x.div_euclid(TOOTH_SCALE), y.div_euclid(TOOTH_SCALE));
    let across = |d: i32| {
        let t = d as f32 / TOOTH_SCALE as f32;
        t * t * (3.0 - 2.0 * t)
    };
    let (sx, sy) = (across(x.rem_euclid(TOOTH_SCALE)), across(y.rem_euclid(TOOTH_SCALE)));
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let top = lerp(noise(cx, cy), noise(cx + 1, cy), sx);
    let bottom = lerp(noise(cx, cy + 1), noise(cx + 1, cy + 1), sx);
    lerp(top, bottom, sy) * 2.0 - 1.0
}

/// Mix the paint from `levels` colours per channel, and lay the canvas's tooth
/// over what it made.
///
/// The tooth goes on after the mixing rather than before, because it is the
/// surface the picture was painted on and not one of the colours it was
/// painted with — quantising it away and then wondering where the texture went
/// is the obvious way to get this wrong.
fn finish(pixmap: &mut Pixmap, levels: u32, tooth: f32) {
    let w = pixmap.width() as i32;
    let steps = levels.max(2);
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            for x in 0..w {
                let grain = if tooth > 0.0 { weave(x, y) * tooth } else { 0.0 };
                let i = x as usize * 4;
                for c in 0..3 {
                    // 256 rather than 255 so that every step is the same
                    // width, then back out over the range so that black stays
                    // black and white stays white.
                    let step = (out[i + c] as u32 * steps / 256).min(steps - 1);
                    let mixed = (step * 255) as f32 / (steps - 1) as f32;
                    out[i + c] = (mixed + grain).clamp(0.0, 255.0).round() as u8;
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture with detail in one end and a featureless grey in the other:
    /// the pencil should draw on the one and leave the other as paper.
    ///
    /// The flat half is the colour of the paper (Paper Brightness 25), because
    /// the filter draws the whole picture — a flat *coloured* field takes
    /// pencil like anything else, and only a field with nothing in it at all,
    /// which is what a backing the colour of the sheet is, comes back bare.
    fn half_detailed() -> Pixmap {
        let mut pm = Pixmap::filled(160, 64, Rgba8::new(128, 128, 128, 255));
        for y in 0..64 {
            for x in 0..64 {
                // Fine stripes — veins for the pencil to follow.
                if x % 3 == 0 {
                    pm.set(x, y, Rgba8::new(200, 60, 90, 255));
                }
            }
        }
        pm
    }

    /// How much colour the pencil left over a band of the picture.
    ///
    /// Measured as colourfulness rather than as distance from the paper: the
    /// paper is grey, and anything the pencil drew carries the picture's own
    /// colour, so the two are told apart by whether the channels differ — and
    /// a wash that happens to average out near the paper's own level is not
    /// mistaken for bare paper.
    fn ink(pm: &Pixmap, xs: std::ops::Range<i32>) -> u32 {
        let columns = xs.len() as u32;
        xs.map(|x| {
            (0..64)
                .map(|y| {
                    let p = pm.get(x, y);
                    (p.r as i32 - p.g as i32).unsigned_abs()
                        + (p.g as i32 - p.b as i32).unsigned_abs()
                        + (p.r as i32 - p.b as i32).unsigned_abs()
                })
                .sum::<u32>()
        })
        .sum::<u32>()
            / columns
    }

    #[test]
    fn the_pencil_draws_the_detail_and_leaves_the_flat_parts_as_paper() {
        let mut pm = half_detailed();
        colored_pencil(&mut pm, 2, 8, 25);
        let detailed = ink(&pm, 0..64);
        let flat = ink(&pm, 144..160);
        assert!(
            detailed > flat * 4,
            "the flat wash took nearly as much pencil as the detail: {detailed} vs {flat}"
        );
    }

    /// A drawn shape keeps its own colour: the paper streaks across a petal,
    /// it does not swallow it. CS6's flowers come back pink.
    ///
    /// This is the regression the hatch is one number away from at all times.
    /// The strokes cover about a quarter of the page, so a hatch that takes
    /// nearly everything back in its gaps leaves a third of the colour at best
    /// — everywhere, detail or no detail — and the filter turns into a washed
    /// photograph on grey rather than a drawing.
    #[test]
    fn a_drawn_shape_keeps_most_of_its_colour() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(200, 60, 90, 255));
        // Fine texture, so the whole field counts as something to draw — what
        // the veins in a petal do — without moving the colour itself much.
        for y in 0..64 {
            for x in (0..64).step_by(3) {
                pm.set(x, y, Rgba8::new(220, 75, 105, 255));
            }
        }
        let before = ink(&pm, 0..64);
        let mut drawn = pm.clone();
        colored_pencil(&mut drawn, 2, 8, 25);
        let after = ink(&drawn, 0..64);
        assert!(
            after * 2 > before,
            "the petal lost most of its colour to the paper: {after} of {before}"
        );
    }

    /// Paper Brightness sets the level of the ground. A flat wash has no
    /// detail to draw, so the pencil leaves it as a light stroke of its own
    /// colour over the sheet — which pulls it towards the paper rather than
    /// leaving it at the picture's own level. 0 is black paper and 50 is
    /// white.
    #[test]
    fn paper_brightness_sets_the_ground() {
        // Well away from the middle, so "towards the paper" is unambiguous at
        // every setting.
        let source = Rgba8::new(60, 80, 100, 255);
        for (setting, paper) in [(0u32, 0i32), (25, 128), (50, 255)] {
            let mut pm = Pixmap::filled(32, 32, source);
            colored_pencil(&mut pm, 4, 8, setting);
            let got = pm.get(16, 16).r as i32;
            assert!(
                (got - paper).abs() < (got - source.r as i32).abs(),
                "paper brightness {setting}: {got} was not pulled towards {paper}"
            );
        }
    }

    /// A field the colour of the sheet has nothing to show, so it comes back
    /// as the paper — within the hatch's own faint stroke, which the hand
    /// leaves even where it had nothing to draw.
    #[test]
    fn a_field_the_colour_of_the_paper_stays_paper() {
        for (setting, paper) in [(0u32, 0i32), (25, 128), (50, 255)] {
            let mut pm = Pixmap::filled(32, 32, Rgba8::new(paper as u8, paper as u8, paper as u8, 255));
            colored_pencil(&mut pm, 4, 8, setting);
            let got = pm.get(16, 16).r as i32;
            assert!(
                (got - paper).abs() <= HATCH_INK as i32,
                "paper brightness {setting}: a paper-coloured field came back at {got}"
            );
        }
    }

    /// Stroke Pressure is the gain on everything the hand does, the hatch
    /// included, so at zero the page stays blank however much there was to
    /// draw.
    #[test]
    fn no_pressure_leaves_the_page_blank() {
        let mut pm = half_detailed();
        colored_pencil(&mut pm, 2, 0, 25);
        assert!(pm.as_bytes().chunks_exact(4).all(|p| p[0] == 128));
    }

    /// More pressure, more pencil.
    #[test]
    fn pressure_lays_more_colour() {
        let laid = |pressure| {
            let mut pm = half_detailed();
            colored_pencil(&mut pm, 2, pressure, 25);
            ink(&pm, 0..64)
        };
        assert!(laid(14) > laid(4), "pressing harder laid no more colour");
    }

    /// The hatch is seeded from where each pixel is, so an undo/redo replay
    /// draws the same strokes.
    #[test]
    fn the_same_picture_is_drawn_the_same_way_twice() {
        let mut first = half_detailed();
        colored_pencil(&mut first, 6, 8, 25);
        let mut second = half_detailed();
        colored_pencil(&mut second, 6, 8, 25);
        assert_eq!(first.as_bytes(), second.as_bytes());
    }

    /// A wider pencil is a coarser hatch — the same drawing in fewer, bigger
    /// strokes, so neighbouring pixels agree with each other more often.
    #[test]
    fn a_wider_pencil_makes_a_coarser_hatch() {
        let roughness = |width| {
            let mut pm = half_detailed();
            colored_pencil(&mut pm, width, 12, 25);
            (0..32)
                .map(|x| {
                    (0..64)
                        .map(|y| {
                            (pm.get(x, y).r as i32 - pm.get(x + 1, y).r as i32).unsigned_abs()
                        })
                        .sum::<u32>()
                })
                .sum::<u32>()
        };
        assert!(
            roughness(20) < roughness(2),
            "a wide pencil changed as often across the page as a fine one"
        );
    }

    #[test]
    fn colored_pencil_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        colored_pencil(&mut pm, 4, 8, 25);
        assert!(pm.as_bytes().chunks_exact(4).all(|p| p[3] == 77));
    }

    #[test]
    fn colored_pencil_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        colored_pencil(&mut pm, 4, 8, 25);
    }

    /// A green ground with a pink disc on it, which is the shape of the
    /// picture Cutout is usually pointed at: two areas, each with a little
    /// grain in it for the flattening to rub out.
    fn disc_on_a_ground() -> Pixmap {
        let mut pm = Pixmap::filled(96, 96, Rgba8::new(40, 90, 30, 255));
        for y in 0..96 {
            for x in 0..96 {
                let (dx, dy) = (x as f32 - 48.0, y as f32 - 48.0);
                let px = if dx * dx + dy * dy < 30.0 * 30.0 {
                    Rgba8::new(210, 100, 150, 255)
                } else {
                    Rgba8::new(40, 90, 30, 255)
                };
                // Grain, kept well inside a band so that it cannot cut a piece
                // of its own — it is there to be averaged away.
                let grain = ((x * 7 + y * 13) % 5) as u8 * 2;
                pm.set(x, y, Rgba8::new(px.r + grain, px.g + grain, px.b + grain, 255));
            }
        }
        pm
    }

    /// How many different colours a picture is made of.
    fn shades(pm: &Pixmap) -> std::collections::HashSet<[u8; 3]> {
        pm.as_bytes()
            .chunks_exact(4)
            .map(|p| [p[0], p[1], p[2]])
            .collect()
    }

    /// The point of the filter: what comes back is a handful of flat areas,
    /// not a photograph.
    #[test]
    fn cutout_leaves_a_few_flat_pieces() {
        let mut pm = disc_on_a_ground();
        let before = shades(&pm).len();
        cutout(&mut pm, 4, 4, 2);
        let after = shades(&pm).len();
        assert!(before >= 10, "the test picture was already flat: {before}");
        assert!(
            after <= 4,
            "a two-colour picture came back in {after} shades"
        );
    }

    /// A piece is painted the colour the picture had under it, not a value off
    /// a fixed grid — the difference between Cutout and Posterize. The disc
    /// stays the pink it was; the ground stays the green it was.
    #[test]
    fn a_piece_keeps_the_colour_it_covered() {
        let mut pm = disc_on_a_ground();
        cutout(&mut pm, 4, 4, 2);
        // The grain averages out to four levels over the picture's own colour.
        for (at, want) in [((48, 48), [214, 104, 154]), ((4, 4), [44, 94, 34])] {
            let got = pm.get(at.0, at.1);
            for (c, want) in [got.r, got.g, got.b].into_iter().zip(want) {
                assert!(
                    (c as i32 - want as i32).abs() <= 8,
                    "a piece came back {got:?} where the picture was {want:?}"
                );
            }
        }
    }

    /// Cutting more finely leaves more pieces, which is what Number of Levels
    /// is for.
    #[test]
    fn more_levels_cut_more_pieces() {
        let gradient = || {
            let mut pm = Pixmap::new(128, 32);
            for y in 0..32 {
                for x in 0..128 {
                    let v = (x * 2) as u8;
                    pm.set(x, y, Rgba8::new(v, v / 2, 255 - v, 255));
                }
            }
            pm
        };
        let pieces = |levels| {
            let mut pm = gradient();
            cutout(&mut pm, levels, 0, 3);
            shades(&pm).len()
        };
        assert!(
            pieces(8) > pieces(2),
            "eight levels cut no more finely than two"
        );
    }

    /// Edge Simplicity rubs out what is too small to cut around. A speck a few
    /// pixels across survives a light hand and not a heavy one.
    #[test]
    fn edge_simplicity_rubs_out_the_small_stuff() {
        let speck = || {
            let mut pm = Pixmap::filled(64, 64, Rgba8::new(40, 90, 30, 255));
            pm.fill_rect(crate::buffer::Rect::new(30, 30, 3, 3), Rgba8::new(230, 40, 60, 255));
            pm
        };
        let survives = |simplicity| {
            let mut pm = speck();
            cutout(&mut pm, 4, simplicity, 3);
            pm.get(31, 31).r > 150
        };
        assert!(survives(0), "a light hand cut round the speck anyway");
        assert!(!survives(8), "a heavy hand left the speck standing");
    }

    /// Two areas exactly as bright as each other are still two pieces. Cutting
    /// on brightness alone would join them and paint the pair some average
    /// mud, which is the thing this filter must not do to a flower on grass.
    #[test]
    fn equally_bright_colours_are_cut_apart() {
        // Matched to within a level of each other by the usual weighting.
        let (pink, green) = (Rgba8::new(214, 100, 160, 255), Rgba8::new(40, 208, 60, 255));
        assert!((luma(pink) - luma(green)).abs() < 1.0);
        let mut pm = Pixmap::filled(64, 32, pink);
        pm.fill_rect(crate::buffer::Rect::new(32, 0, 32, 32), green);
        cutout(&mut pm, 4, 2, 3);
        for (at, want) in [((8, 16), pink), ((56, 16), green)] {
            let got = pm.get(at.0, at.1);
            assert!(
                (got.r as i32 - want.r as i32).abs() <= 8
                    && (got.g as i32 - want.g as i32).abs() <= 8,
                "{got:?} where the picture was {want:?} — the two were cut as one piece"
            );
        }
    }

    /// Areas touching only at a corner are two pieces, as a pair of scissors
    /// would leave them, and each takes its own colour.
    #[test]
    fn areas_meeting_at_a_corner_are_two_pieces() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(20, 20, 20, 255));
        let red = Rgba8::new(220, 40, 40, 255);
        pm.fill_rect(crate::buffer::Rect::new(0, 0, 16, 16), red);
        pm.fill_rect(crate::buffer::Rect::new(16, 16, 16, 16), red);
        // No flattening and no vote, so the corner is left exactly as drawn.
        cutout(&mut pm, 8, 0, 3);
        assert_eq!(pm.get(4, 4).r, pm.get(20, 20).r);
    }

    #[test]
    fn cutout_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        cutout(&mut pm, 4, 4, 2);
        assert!(pm.as_bytes().chunks_exact(4).all(|p| p[3] == 77));
    }

    /// Every pixel belongs to exactly one piece, whatever the settings — the
    /// fill must leave none of the picture uncut.
    #[test]
    fn cutout_covers_the_whole_picture() {
        for (levels, simplicity, fidelity) in
            [(2, 0, 1), (8, 10, 3), (4, 4, 2), (2, 10, 1), (8, 0, 3)]
        {
            let mut pm = disc_on_a_ground();
            cutout(&mut pm, levels, simplicity, fidelity);
            assert!(pm.as_bytes().chunks_exact(4).all(|p| p[3] == 255));
        }
    }

    #[test]
    fn cutout_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        cutout(&mut pm, 4, 4, 2);
    }

    /// Two noisy fields with a hard boundary between them: the thing a
    /// painting filter has to get right is flattening the fields *without*
    /// softening what divides them.
    fn two_noisy_fields() -> Pixmap {
        let mut pm = Pixmap::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                let base = if x < 32 { 60 } else { 200 };
                let n = ((x * 7 + y * 13) % 5) as i32 * 5 - 10;
                let v = (base + n).clamp(0, 255) as u8;
                pm.set(x, y, Rgba8::new(v, v, v, 255));
            }
        }
        pm
    }

    /// How much a band of the picture jitters from pixel to pixel.
    fn restlessness(pm: &Pixmap, xs: std::ops::Range<i32>) -> u32 {
        xs.map(|x| {
            (0..63)
                .map(|y| (pm.get(x, y).r as i32 - pm.get(x, y + 1).r as i32).unsigned_abs())
                .sum::<u32>()
        })
        .sum()
    }

    /// The brush lays flat colour — the field it paints over comes back
    /// calmer than it was.
    #[test]
    fn the_brush_flattens_what_it_paints_over() {
        let before = two_noisy_fields();
        let mut after = before.clone();
        dry_brush(&mut after, 6, 10, 1);
        let (was, now) = (restlessness(&before, 4..28), restlessness(&after, 4..28));
        assert!(
            now * 3 < was,
            "the brush left the surface as restless as it found it: {now} against {was}"
        );
    }

    /// ...and it loads from one side of a boundary, never across it, so the
    /// boundary is exactly where it was.
    #[test]
    fn the_brush_does_not_paint_across_an_edge() {
        let mut pm = two_noisy_fields();
        dry_brush(&mut pm, 6, 10, 1);
        // The step from one side to the other, a pixel either way.
        let step = pm.get(32, 32).r as i32 - pm.get(31, 32).r as i32;
        assert!(step > 120, "the edge came back softened to {step} levels");
    }

    /// A wider brush lays broader patches. Stripes eight pixels apart stand up
    /// to a brush narrower than they are and are painted over by one wider than
    /// they are, which is the whole of what Brush Size does.
    #[test]
    fn a_bigger_brush_paints_broader_patches() {
        let stripes = || {
            let mut pm = Pixmap::new(64, 64);
            for y in 0..64 {
                for x in 0..64 {
                    let v = if (x / 8) % 2 == 0 { 60 } else { 200 };
                    pm.set(x, y, Rgba8::new(v, v, v, 255));
                }
            }
            pm
        };
        // How much of the stripes' contrast is left across the middle of the
        // picture, away from the frame's edges.
        let surviving = |size| {
            let mut pm = stripes();
            dry_brush(&mut pm, size, 10, 1);
            let band: Vec<i32> = (16..48).map(|x| pm.get(x, 32).r as i32).collect();
            band.iter().max().unwrap() - band.iter().min().unwrap()
        };
        assert!(
            surviving(10) * 2 < surviving(2),
            "a wide brush left as much standing as a narrow one: {} against {}",
            surviving(10),
            surviving(2)
        );
    }

    /// Brush Detail is how many colours the paint is mixed from.
    #[test]
    fn brush_detail_sets_how_many_colours_the_paint_is_mixed_from() {
        let mixed = |detail| {
            let mut pm = two_noisy_fields();
            dry_brush(&mut pm, 2, detail, 1);
            shades(&pm).len()
        };
        assert!(
            mixed(10) > mixed(0),
            "the top of the slider mixed no more colours than the bottom"
        );
    }

    /// Texture is the body of the paint: laid on thickly, the facets the brush
    /// left stand further from one another.
    ///
    /// Measured on a picture with brushwork in it, because that is what the
    /// slider works on. A flat wash has no facets to raise and comes back the
    /// same at every setting, which is the honest answer — there is nothing
    /// there to lay on thickly.
    #[test]
    fn texture_is_the_body_of_the_paint() {
        let laid = |texture| {
            let mut pm = two_noisy_fields();
            dry_brush(&mut pm, 4, 10, texture);
            restlessness(&pm, 4..60)
        };
        assert!(
            laid(3) > laid(1),
            "paint laid on thickly stood no further out: {} against {}",
            laid(3),
            laid(1)
        );
    }

    /// The canvas is seeded from where each pixel is, so an undo/redo replay
    /// paints on the same one.
    #[test]
    fn the_same_picture_is_painted_the_same_way_twice() {
        let mut first = two_noisy_fields();
        dry_brush(&mut first, 4, 8, 2);
        let mut second = two_noisy_fields();
        dry_brush(&mut second, 4, 8, 2);
        assert_eq!(first.as_bytes(), second.as_bytes());
    }

    #[test]
    fn dry_brush_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        dry_brush(&mut pm, 4, 8, 2);
        assert!(pm.as_bytes().chunks_exact(4).all(|p| p[3] == 77));
    }

    /// Brush Size 0 is no brush: nothing is painted, and what comes back is
    /// the picture with the paint mixed and the canvas under it.
    #[test]
    fn no_brush_paints_nothing() {
        let source = two_noisy_fields();
        let mut pm = source.clone();
        dry_brush(&mut pm, 0, 10, 1);
        for (a, b) in pm.as_bytes().chunks_exact(4).zip(source.as_bytes().chunks_exact(4)) {
            assert!(
                (a[0] as i32 - b[0] as i32).abs() <= 8,
                "size 0 moved a pixel from {} to {}",
                b[0],
                a[0]
            );
        }
    }

    #[test]
    fn dry_brush_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        dry_brush(&mut pm, 2, 8, 2);
    }

    /// A cell is flat, so a ramp — where no two neighbours were ever equal —
    /// comes back as runs of one colour.
    #[test]
    fn the_knife_lays_flat_cells_on_a_smooth_ramp() {
        let mut pm = ramp();
        let flat = |pm: &Pixmap| {
            (0..63)
                .filter(|&x| pm.get(x, 32).r == pm.get(x + 1, 32).r)
                .count()
        };
        assert_eq!(flat(&pm), 0, "the test ramp was not a ramp");
        palette_knife(&mut pm, 25, 3, 0);
        assert!(
            flat(&pm) > 40,
            "only {} of 63 neighbours came back flat",
            flat(&pm)
        );
    }

    /// A wide knife works in fewer, bigger masses of colour than a fine one.
    #[test]
    fn a_wider_knife_carries_fewer_colours() {
        let carried = |size| {
            let mut pm = ramp();
            palette_knife(&mut pm, size, 3, 0);
            shades(&pm).len()
        };
        assert!(
            carried(50) < carried(2),
            "a wide knife carried as many colours as a fine one: {} against {}",
            carried(50),
            carried(2)
        );
    }

    /// The knife spreads a surface and stops at a boundary.
    #[test]
    fn the_knife_stops_at_a_boundary() {
        let mut pm = two_noisy_fields();
        palette_knife(&mut pm, 10, 3, 0);
        // The two fields are 140 levels apart. What crosses between them is
        // one step, not a gradient.
        let step = pm.get(32, 32).r as i32 - pm.get(31, 32).r as i32;
        assert!(step > 100, "the knife spread across the edge: {step} levels");
    }

    /// Stroke Detail is how much of the picture the knife keeps before it
    /// starts. Not *whether* it keeps any: even at 3 the picture is taken down
    /// to masses first, which is the thing about this filter that took the
    /// longest to see.
    #[test]
    fn stroke_detail_is_how_much_the_knife_keeps() {
        // Stripes near enough in colour to be flattened together, and wide
        // enough that how far the flattening reaches decides how much of them
        // is left. Anything finer is gone at either setting, which is what the
        // first version of this test measured and why it measured nothing.
        let kept = |detail| {
            let mut pm = Pixmap::new(64, 64);
            for y in 0..64 {
                for x in 0..64 {
                    let v = if (x / 6) % 2 == 0 { 120 } else { 145 };
                    pm.set(x, y, Rgba8::new(v, v, v, 255));
                }
            }
            palette_knife(&mut pm, 12, detail, 0);
            let band: Vec<i32> = (16..48).map(|x| pm.get(x, 32).r as i32).collect();
            band.iter().max().unwrap() - band.iter().min().unwrap()
        };
        assert!(
            kept(3) > kept(1),
            "the knife kept as little at full detail as at none: {} against {}",
            kept(3),
            kept(1)
        );
    }

    /// Softness is the blade's edge: wound up, the cells stop meeting at a hard
    /// line.
    ///
    /// Measured on a ramp, which has no fine detail anywhere — so the cells
    /// stand over the whole of it and there is a join to ease. On a picture
    /// with detail in it the picture shows through and there is nothing for
    /// this slider to do, which is the point of applying it where it is.
    #[test]
    fn softness_eases_the_joins_between_cells() {
        let hardest_join = |softness| {
            let mut pm = ramp();
            palette_knife(&mut pm, 10, 3, softness);
            (0..63)
                .map(|x| (pm.get(x, 32).r as i32 - pm.get(x + 1, 32).r as i32).abs())
                .max()
                .unwrap_or(0)
        };
        assert!(
            hardest_join(10) * 2 < hardest_join(0),
            "the soft blade cut as hard as the sharp one: {} against {}",
            hardest_join(10),
            hardest_join(0)
        );
    }

    #[test]
    fn palette_knife_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        palette_knife(&mut pm, 25, 3, 5);
        assert!(pm.as_bytes().chunks_exact(4).all(|p| p[3] == 77));
    }

    #[test]
    fn palette_knife_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        palette_knife(&mut pm, 25, 3, 0);
    }

    /// The daubs wash a surface together and stop dead at a boundary, which is
    /// the whole of the first pass.
    #[test]
    fn a_daub_washes_a_surface_together_and_stops_at_a_boundary() {
        let before = two_noisy_fields();
        let mut after = before.clone();
        paint_daubs(&mut after, 10, 0, DaubBrush::Simple);
        assert!(
            restlessness(&after, 4..28) * 4 < restlessness(&before, 4..28),
            "the daub left the surface as it found it"
        );
        let step = after.get(32, 32).r as i32 - after.get(31, 32).r as i32;
        assert!(step > 120, "the daub washed across the edge: {step} levels");
    }

    /// A bigger brush lays a bigger daub, and anything smaller than the daub
    /// goes into it. A mark a few pixels across survives a small brush and is
    /// painted over by a large one — as long as it is near enough in colour to
    /// belong to the same thing, which is the next test's business.
    #[test]
    fn a_bigger_brush_lays_a_bigger_daub() {
        let survives = |size| {
            let mut pm = Pixmap::filled(64, 64, Rgba8::new(120, 120, 120, 255));
            pm.fill_rect(crate::buffer::Rect::new(30, 30, 6, 6), Rgba8::new(150, 150, 150, 255));
            paint_daubs(&mut pm, size, 0, DaubBrush::Simple);
            pm.get(32, 32).r as i32 - 120
        };
        assert!(survives(2) > 20, "a small brush painted over the mark anyway");
        assert!(survives(20) < 8, "a large brush left the mark standing");
    }

    /// The daub alone, which the brushes are measured against below.
    fn daubed() -> Pixmap {
        let mut pm = two_noisy_fields();
        paint_daubs(&mut pm, 8, 0, DaubBrush::Simple);
        pm
    }

    /// The Rough brushes scrub what the daubing threw away back on, and each
    /// is allowed one direction only: Light Rough can scrub a surface brighter
    /// and never dirty it, Dark Rough the other way about. Sparkle is Light
    /// Rough taken further.
    #[test]
    fn the_rough_brushes_scrub_in_one_direction_only() {
        let scrubbed = |brush| {
            let mut pm = two_noisy_fields();
            paint_daubs(&mut pm, 8, 20, brush);
            pm
        };
        let (plain, light, dark) = (
            daubed(),
            scrubbed(DaubBrush::LightRough),
            scrubbed(DaubBrush::DarkRough),
        );
        for y in 0..64 {
            for x in 0..64 {
                let (p, l, d) = (plain.get(x, y).r, light.get(x, y).r, dark.get(x, y).r);
                assert!(l >= p, "Light Rough darkened {p} to {l} at {x},{y}");
                assert!(d <= p, "Dark Rough lightened {p} to {d} at {x},{y}");
            }
        }
    }

    /// ...and they scrub hard. This is what separates them from the painting
    /// brushes, where the same slider is worth a fraction as much.
    #[test]
    fn the_rough_brushes_scrub_harder_than_the_painting_ones() {
        let texture = |brush| {
            let mut pm = two_noisy_fields();
            paint_daubs(&mut pm, 8, 20, brush);
            restlessness(&pm, 4..28)
        };
        assert!(
            texture(DaubBrush::LightRough) > texture(DaubBrush::Simple) * 3,
            "the rough brush laid no more texture than the plain one"
        );
    }

    /// The two Wide brushes lay the same daub and differ in what goes on top:
    /// Wide Sharp lays definition where two daubs meet, Wide Blurry lays the
    /// grain of the paint. So on a surface that is nothing but grain — no
    /// shapes for either to find — the blurry brush is the rougher of the two.
    #[test]
    fn wide_blurry_lays_the_grain_where_wide_sharp_lays_definition() {
        let grainy = || {
            let mut pm = Pixmap::new(64, 64);
            for y in 0..64 {
                for x in 0..64 {
                    let v = 120 + ((x * 7 + y * 13) % 5) as i32 * 4 - 8;
                    pm.set(x, y, Rgba8::new(v as u8, v as u8, v as u8, 255));
                }
            }
            pm
        };
        let texture = |brush| {
            let mut pm = grainy();
            paint_daubs(&mut pm, 12, 20, brush);
            restlessness(&pm, 4..28)
        };
        assert!(
            texture(DaubBrush::WideBlurry) > texture(DaubBrush::WideSharp) * 2,
            "the blurry brush laid no more grain than the sharp one: {} against {}",
            texture(DaubBrush::WideBlurry),
            texture(DaubBrush::WideSharp)
        );
    }

    #[test]
    fn paint_daubs_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        paint_daubs(&mut pm, 8, 7, DaubBrush::DarkRough);
        assert!(pm.as_bytes().chunks_exact(4).all(|p| p[3] == 77));
    }

    #[test]
    fn paint_daubs_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        paint_daubs(&mut pm, 8, 7, DaubBrush::Simple);
    }

    /// Black and white, which is what the swatches usually are, and a blue
    /// tube.
    const TUBE: Rgba8 = Rgba8::new(0, 0, 255, 255);
    fn lit(pm: &mut Pixmap, size: i32, brightness: u32) {
        neon_glow(pm, size, brightness, TUBE, Rgba8::BLACK, Rgba8::WHITE);
    }

    /// The same with a white tube, for the tests that are about where a tone
    /// lands rather than what colour the lamp is — a coloured tube pulls every
    /// channel about and would be measuring two things at once.
    fn lit_plainly(pm: &mut Pixmap, size: i32, brightness: u32) {
        neon_glow(pm, size, brightness, Rgba8::WHITE, Rgba8::BLACK, Rgba8::WHITE);
    }

    /// The picture is rendered between the two swatches: what was dark comes
    /// back as the foreground colour, what was light as the background.
    #[test]
    fn the_picture_is_rendered_between_the_swatches() {
        let mut pm = ramp();
        // No spread and a lamp turned right down, so nothing but the mapping
        // is being measured.
        lit_plainly(&mut pm, 0, 0);
        assert!(pm.get(1, 32).r < 12, "the dark end did not go to black");
        assert!(pm.get(62, 32).r > 200, "the light end did not go to white");
    }

    /// The lit end is carried towards the glow colour, so a blue tube leaves
    /// the lights blue and the darks alone.
    #[test]
    fn the_tube_tints_what_it_lights() {
        let mut pm = ramp();
        lit(&mut pm, 0, 10);
        let light = pm.get(60, 32);
        assert!(
            light.b > light.r + 60,
            "the lit end came back untinted: {light:?}"
        );
        let dark = pm.get(2, 32);
        assert!(dark.b < 40, "the tube reached the shadows: {dark:?}");
    }

    /// Glow Size is how far the picture's own light carries into what is dark.
    #[test]
    fn glow_size_is_how_far_the_light_carries() {
        let carried = |size| {
            let mut pm = Pixmap::filled(96, 96, Rgba8::BLACK);
            pm.fill_rect(crate::buffer::Rect::new(0, 0, 48, 96), Rgba8::WHITE);
            neon_glow(&mut pm, size, 40, TUBE, Rgba8::BLACK, Rgba8::WHITE);
            // A band well inside the dark half, where the only thing that can
            // have reached is light that carried.
            (58..78)
                .map(|x| (0..96).map(|y| pm.get(x, y).b as u32).sum::<u32>())
                .sum::<u32>()
        };
        assert!(
            carried(20) > carried(4) * 4,
            "a wide glow carried no further than a narrow one: {} against {}",
            carried(20),
            carried(4)
        );
    }

    /// Wound below zero it is the shadows that light up instead, which is what
    /// the negative half of CS6's slider is for.
    #[test]
    fn a_negative_glow_size_lights_the_shadows() {
        let ground = |size| {
            let mut pm = Pixmap::filled(96, 96, Rgba8::BLACK);
            pm.fill_rect(crate::buffer::Rect::new(32, 32, 32, 32), Rgba8::WHITE);
            neon_glow(&mut pm, size, 12, TUBE, Rgba8::BLACK, Rgba8::WHITE);
            pm.get(8, 8).b as i32
        };
        assert!(
            ground(-12) > ground(12) + 60,
            "the dark ground did not light up: {} against {}",
            ground(-12),
            ground(12)
        );
    }

    /// Past the middle of the Glow Brightness slider the light carries the
    /// brightest part of the picture past white and back out the other side,
    /// so it returns as the foreground colour. This is what makes the filter's
    /// two extremes look like negatives of one another, and it is deliberate.
    #[test]
    fn a_lamp_turned_past_white_folds_back_to_the_foreground() {
        let white_end = |brightness| {
            let mut pm = ramp();
            lit_plainly(&mut pm, 0, brightness);
            pm.get(62, 32).r as i32
        };
        assert!(white_end(10) > 180, "the lamp did not light the picture");
        assert!(
            white_end(50) < 40,
            "the lamp never folded: the light end came back at {}",
            white_end(50)
        );
    }

    #[test]
    fn neon_glow_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        lit(&mut pm, 5, 15);
        assert!(pm.as_bytes().chunks_exact(4).all(|p| p[3] == 77));
    }

    #[test]
    fn neon_glow_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        lit(&mut pm, 5, 15);
    }

    /// The plaster takes the darks down and leaves the lights where they are.
    /// That is the whole of what separates Fresco from Dry Brush, so it is
    /// measured against Dry Brush at the same settings rather than in the
    /// abstract.
    #[test]
    fn the_plaster_takes_the_picture_down() {
        let source = Pixmap::filled(64, 64, Rgba8::new(60, 100, 80, 255));
        let (mut painted, mut plastered) = (source.clone(), source.clone());
        dry_brush(&mut painted, 2, 8, 1);
        fresco(&mut plastered, 2, 8, 1);
        assert!(
            (plastered.get(32, 32).g as i32) * 2 < painted.get(32, 32).g as i32,
            "the plaster left the picture where the brush did: {:?} against {:?}",
            plastered.get(32, 32),
            painted.get(32, 32)
        );
    }

    /// ...and it takes a colour's weakest channel down hardest, which deepens
    /// the colour rather than merely dimming it.
    #[test]
    fn the_plaster_deepens_rather_than_dims() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(60, 180, 60, 255));
        fresco(&mut pm, 0, 10, 1);
        let got = pm.get(32, 32);
        let (was, now) = (180.0 / 60.0, got.g as f32 / (got.r as f32).max(1.0));
        assert!(
            now > was * 1.5,
            "the colour came back as flat as it went in: {got:?}"
        );
    }

    /// White is plaster's one exception: the top of the range stays put, so a
    /// highlight is still a highlight.
    #[test]
    fn the_lights_stay_where_they_are() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(255, 255, 250, 255));
        fresco(&mut pm, 0, 10, 1);
        assert!(pm.get(16, 16).r > 240, "the plaster took the highlight down");
    }

    /// Fresco paints in the same dabs as Dry Brush, so it flattens a surface
    /// and keeps the boundary through it.
    #[test]
    fn fresco_paints_in_dabs_like_the_brush_it_shares() {
        let mut pm = two_noisy_fields();
        fresco(&mut pm, 6, 10, 1);
        let step = pm.get(32, 32).r as i32 - pm.get(31, 32).r as i32;
        assert!(step > 60, "the edge came back softened to {step} levels");
    }

    #[test]
    fn fresco_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        fresco(&mut pm, 2, 8, 1);
        assert!(pm.as_bytes().chunks_exact(4).all(|p| p[3] == 77));
    }

    #[test]
    fn fresco_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        fresco(&mut pm, 2, 8, 1);
    }

    /// A ramp from black to white, for asking what a filter does to each end
    /// of the tonal range separately.
    fn ramp() -> Pixmap {
        let mut pm = Pixmap::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                let v = (x * 4) as u8;
                pm.set(x, y, Rgba8::new(v, v, v, 255));
            }
        }
        pm
    }

    /// How far the picture strays from the ramp it was, in levels per pixel,
    /// over a band of it.
    fn scatter(pm: &Pixmap, xs: std::ops::Range<i32>) -> f32 {
        let n = xs.len() as f32 * 64.0;
        xs.map(|x| {
            (0..64)
                .map(|y| (pm.get(x, y).r as i32 - (x * 4)).abs() as f32)
                .sum::<f32>()
        })
        .sum::<f32>()
            / n
    }

    /// Grain is grain: more of it, more silver on the picture.
    #[test]
    fn grain_lays_silver_on_the_picture() {
        let laid = |grain| {
            let mut pm = ramp();
            film_grain(&mut pm, grain, 0, 10);
            scatter(&pm, 8..56)
        };
        assert!(laid(0) < 0.5, "no grain still moved the picture");
        assert!(laid(20) > laid(10) && laid(10) > laid(4), "the slider did nothing");
    }

    /// CS6's defaults are Grain 4, Highlight Area 0, Intensity 10, and what
    /// they give is the picture with grain on it and nothing else — the
    /// highlight pair has nothing to work on until Highlight Area opens it up.
    #[test]
    fn with_no_highlight_area_intensity_has_nothing_to_do() {
        let lit = |highlight| {
            let mut pm = ramp();
            film_grain(&mut pm, 0, highlight, 10);
            // The light end of the ramp, where any lifting would show.
            (48..64).map(|x| pm.get(x, 32).r as i32 - x * 4).sum::<i32>()
        };
        assert_eq!(lit(0), 0, "the highlights moved with the area shut");
        assert!(lit(20) > 200, "opening the area lifted nothing");
    }

    /// Intensity is how hard the lit part is carried towards white, so it does
    /// nothing at 0 and everything at 10 — once there is an area to carry.
    #[test]
    fn intensity_carries_the_lit_part_towards_white() {
        let lifted = |intensity| {
            let mut pm = ramp();
            film_grain(&mut pm, 0, 20, intensity);
            pm.get(60, 32).r as i32
        };
        assert_eq!(lifted(0), 240, "intensity 0 lifted the highlight anyway");
        assert!(lifted(10) > lifted(5) && lifted(5) > lifted(0));
    }

    /// What is under the line is left where it was. A film's shadows do not
    /// glow.
    #[test]
    fn what_is_under_the_line_is_left_alone() {
        let mut pm = ramp();
        film_grain(&mut pm, 0, 10, 10);
        for x in 0..8 {
            assert_eq!(pm.get(x, 32).r as i32, x * 4, "a shadow was lifted");
        }
    }

    /// Each channel is asked where *it* stands, not the pixel's brightness. So
    /// a colour with one channel over the line and two under it comes back
    /// more vivid rather than washed out — CS6's grass goes lime, not grey.
    #[test]
    fn a_colour_with_one_channel_in_the_light_goes_vivid() {
        let grass = Rgba8::new(40, 90, 30, 255);
        let mut pm = Pixmap::filled(32, 32, grass);
        film_grain(&mut pm, 0, 16, 10);
        let got = pm.get(16, 16);
        assert_eq!(
            (got.r, got.b),
            (grass.r, grass.b),
            "a channel under the line was carried anyway"
        );
        assert!(
            got.g > grass.g + 40,
            "the channel over the line was not carried: {got:?}"
        );
    }

    /// "A smoother pattern is added to the image's lighter areas" — the grain
    /// thins out where the picture is light, rather than lying evenly over
    /// everything the way Add Noise would.
    #[test]
    fn the_grain_is_smoother_in_the_light() {
        let mut pm = ramp();
        // Half open, so the light end of the ramp is over the line and the
        // dark end is under it. Intensity 0, so nothing has been lifted and
        // the only difference between the two ends is how much silver landed
        // on them.
        film_grain(&mut pm, 20, 10, 0);
        let (shadow, highlight) = (scatter(&pm, 4..20), scatter(&pm, 44..60));
        assert!(
            highlight * 2.0 < shadow,
            "the light end took as much grain as the dark: {highlight} against {shadow}"
        );
    }

    /// ...and the grain it keeps there is coloured rather than grey, which is
    /// the other half of CS6's sentence.
    #[test]
    fn the_grain_in_the_light_is_coloured() {
        let colourfulness = |xs: std::ops::Range<i32>| {
            let mut pm = ramp();
            film_grain(&mut pm, 20, 10, 0);
            ink(&pm, xs)
        };
        assert!(
            colourfulness(44..60) > colourfulness(4..20),
            "the light end's grain was as grey as the dark end's"
        );
    }

    /// The film is seeded from where each pixel is, so an undo/redo replay
    /// exposes the same frame.
    #[test]
    fn the_same_picture_takes_the_same_grain_twice() {
        let mut first = ramp();
        film_grain(&mut first, 8, 10, 5);
        let mut second = ramp();
        film_grain(&mut second, 8, 10, 5);
        assert_eq!(first.as_bytes(), second.as_bytes());
    }

    #[test]
    fn film_grain_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        film_grain(&mut pm, 8, 10, 5);
        assert!(pm.as_bytes().chunks_exact(4).all(|p| p[3] == 77));
    }

    #[test]
    fn film_grain_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        film_grain(&mut pm, 8, 10, 5);
    }

    /// The brush reaches past the frame at every edge and has to dip inside
    /// it instead.
    #[test]
    fn dry_brush_handles_a_picture_smaller_than_its_brush() {
        for (w, h) in [(1, 1), (3, 40), (40, 3)] {
            let mut pm = Pixmap::filled(w, h, Rgba8::new(90, 120, 30, 255));
            dry_brush(&mut pm, 10, 10, 1);
            assert!((pm.get(0, 0).g as i32 - 120).abs() <= 12);
        }
    }

    /// A single row and a single column still cut — the fill's walk above and
    /// below the run has nowhere to go.
    #[test]
    fn cutout_handles_a_one_pixel_picture() {
        for (w, h) in [(1, 32), (32, 1), (1, 1)] {
            let mut pm = Pixmap::filled(w, h, Rgba8::new(90, 120, 30, 255));
            cutout(&mut pm, 4, 4, 2);
            assert_eq!(pm.get(0, 0).g, 120);
        }
    }
}


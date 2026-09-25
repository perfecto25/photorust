//! Filter ▸ Artistic.
//!
//! CS6 keeps this whole family in the Filter Gallery rather than in the Filter
//! menu. The Gallery is not built (docs/ROADMAP.md), so the filters live under
//! a Filter ▸ Artistic submenu instead, which is where the Gallery's own
//! category list would have put them. Colored Pencil, Cutout, Dry Brush, Film
//! Grain, Fresco, Neon Glow, Paint Daubs, Palette Knife, Plastic Wrap, Poster
//! Edges, Rough Pastels, Smudge Stick, Sponge, Underpainting and Watercolor —
//! the whole of CS6's Artistic group — are built.

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

/// CS6's ranges for Plastic Wrap, which its three sliders run over.
pub const WRAP_HIGHLIGHT: std::ops::RangeInclusive<u32> = 0..=20;
pub const WRAP_DETAIL: std::ops::RangeInclusive<u32> = 1..=15;
pub const WRAP_SMOOTHNESS: std::ops::RangeInclusive<u32> = 1..=15;

/// CS6's ranges for Poster Edges, which its three sliders run over.
pub const POSTER_THICKNESS: std::ops::RangeInclusive<u32> = 0..=10;
pub const POSTER_INTENSITY: std::ops::RangeInclusive<u32> = 0..=10;
pub const POSTER_LEVELS: std::ops::RangeInclusive<u32> = 0..=6;

/// CS6's ranges for Rough Pastels' two stroke sliders. Its texture controls
/// are [`crate::filters::texture`]'s.
pub const PASTEL_LENGTH: std::ops::RangeInclusive<u32> = 0..=40;
pub const PASTEL_DETAIL: std::ops::RangeInclusive<u32> = 1..=20;

/// CS6's ranges for Smudge Stick, which its three sliders run over.
pub const SMUDGE_LENGTH: std::ops::RangeInclusive<u32> = 0..=10;
pub const SMUDGE_HIGHLIGHT: std::ops::RangeInclusive<u32> = 0..=20;
pub const SMUDGE_INTENSITY: std::ops::RangeInclusive<u32> = 0..=10;

/// CS6's ranges for Sponge, which its three sliders run over.
pub const SPONGE_SIZE: std::ops::RangeInclusive<u32> = 0..=10;
pub const SPONGE_DEFINITION: std::ops::RangeInclusive<u32> = 0..=25;
pub const SPONGE_SMOOTHNESS: std::ops::RangeInclusive<u32> = 1..=15;

/// CS6's ranges for Underpainting's two sliders. Its texture controls are
/// [`crate::filters::texture`]'s.
pub const UNDERPAINT_SIZE: std::ops::RangeInclusive<u32> = 0..=40;
pub const UNDERPAINT_COVERAGE: std::ops::RangeInclusive<u32> = 0..=40;

/// CS6's ranges for Watercolor, which its three sliders run over.
pub const WATER_DETAIL: std::ops::RangeInclusive<u32> = 1..=14;
pub const WATER_SHADOW: std::ops::RangeInclusive<u32> = 0..=10;
pub const WATER_TEXTURE: std::ops::RangeInclusive<u32> = 1..=3;

/// CS6's ranges for Paint Daubs. The third control is a list of brushes rather
/// than a slider.
pub const DAUB_SIZE: std::ops::RangeInclusive<u32> = 1..=50;
pub const DAUB_SHARPNESS: std::ops::RangeInclusive<u32> = 0..=40;

/// CS6's six Paint Daubs brushes, in the order its list gives them. See
/// [`paint_daubs`] for what each does.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DaubBrush {
    /// A round daub, sharpened.
    #[default]
    Simple,
    /// A textured daub with a light halo round every shape.
    LightRough,
    /// A textured daub with a heavy dark outline round every shape.
    DarkRough,
    /// A daub stretched sideways, sharpened hard.
    WideSharp,
    /// A daub stretched sideways and softened.
    WideBlurry,
    /// A bright, gritty daub with contour lines swirling through it.
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
pub(crate) fn noise(a: i32, b: i32) -> f32 {
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

/// How far a daub reaches, in pixels either side per step of Brush Size, and
/// how much longer than tall the two Wide brushes lay it.
///
/// The daub is a **median**, not an average, and that is what CS6's Simple
/// brush looks like: a median throws away anything narrower than half its
/// window — the stamens, the veins, the points of the petals — and leaves
/// everything broader exactly where it was, edge and all. So a petal comes
/// back as a rounded, creamy lobe with a crisp outline, which an average
/// (blurred edges) or an edge-preserving blur (the picture untouched, since
/// Surface Blur keeps every fine thing that differs enough) does not give.
const DAUB_REACH: f32 = 0.45;
const WIDE_STRETCH: f32 = 2.0;

/// Sharpness on the painting brushes: an unsharp mask over the daubs, its
/// scale in pixels and its strength per step. This is what draws the thin
/// dark line and light rim round each shape in CS6's Simple.
const EDGE_SCALE: f32 = 2.0;
const EDGE_PER_STEP: f32 = 0.2;

/// Sharpness on the Rough brushes: the same mask taken on brightness, wider
/// and much harder, and only in one direction. Dark Rough's halo on the dark
/// side of every boundary is what becomes CS6's heavy black outline.
const ROUGH_SCALE: f32 = 3.0;
const ROUGH_PER_STEP: f32 = 0.5;

/// The texture of the Rough brushes, in levels and in pixels of speck.
///
/// Two coats. The *tooth* goes on under the daubs (except Sparkle's), so the median turns it
/// into blotches the daubs' own shape and the mask above then works on them.
/// The *grit* goes on last, colour by colour, and is what gives CS6's rough
/// petals their scatter of lilac, white and deeper pink.
const DAUB_TOOTH: f32 = 15.0;
const DAUB_TOOTH_SCALE: f32 = 2.5;
const DAUB_GRIT: f32 = 14.0;
const DAUB_GRIT_SCALE: f32 = 2.0;

/// How much of the daub Wide Blurry softens it by, as a blur of this fraction
/// of its reach.
pub const WIDE_BLUR: f32 = 0.3;

/// Filter ▸ Artistic ▸ Paint Daubs: the picture repainted in daubs, then
/// sharpened, with the brush deciding the shape of the daub and what the
/// sharpening looks like.
///
/// **Brush Size** is the daub. **Sharpness** is the sharpening laid over it.
/// **Brush Type**: Simple is the two as they are; Wide Sharp and Wide Blurry
/// stretch the daub sideways and sharpen harder or soften it; Light Rough and
/// Dark Rough lay a tooth and keep only the light halo or only the dark one;
/// Sparkle draws contour lines and lights the picture up — see [`sparkle`].
///
/// Alpha is left alone: repainting the picture does not change the layer's
/// shape.
///
/// No GPU path. The daubing is a median over a sliding histogram —
/// sequential along each row by construction, and the reason a fifty-pixel
/// brush is fast enough to offer at all.
pub fn paint_daubs(pixmap: &mut Pixmap, size: u32, sharpness: u32, brush: DaubBrush) {
    if pixmap.is_empty() {
        return;
    }
    let size = size.clamp(*DAUB_SIZE.start(), *DAUB_SIZE.end());
    let sharpness = sharpness.clamp(*DAUB_SHARPNESS.start(), *DAUB_SHARPNESS.end()) as f32;
    let (across, down) = daub_reach(size, brush);
    let rough = brush.is_rough();

    // Not under Sparkle: its contour lines need the smooth shading the tooth
    // would break up.
    if rough && brush != DaubBrush::Sparkle {
        lay_tooth(pixmap, DAUB_TOOTH, DAUB_TOOTH_SCALE, 0);
    }
    let mut daubs = crate::filters::convolve::median_of(pixmap, across, down);
    for (daub, original) in daubs
        .as_bytes_mut()
        .chunks_exact_mut(4)
        .zip(pixmap.as_bytes().chunks_exact(4))
    {
        daub[3] = original[3];
    }
    *pixmap = daubs;

    let edge = sharpness * EDGE_PER_STEP;
    let halo = sharpness * ROUGH_PER_STEP;
    match brush {
        DaubBrush::Simple => sharpen(pixmap, edge),
        DaubBrush::WideSharp => sharpen(pixmap, edge * 2.0),
        DaubBrush::WideBlurry => {
            crate::filters::convolve::gaussian_blur_accelerated(pixmap, down as f32 * 2.0 * WIDE_BLUR);
            sharpen(pixmap, edge * 0.5);
        }
        DaubBrush::LightRough => rough_edge(pixmap, halo, Halo::Light),
        DaubBrush::DarkRough => rough_edge(pixmap, halo, Halo::Dark),
        DaubBrush::Sparkle => sparkle(pixmap, sharpness),
    }
    if rough {
        lay_tooth(pixmap, DAUB_GRIT, DAUB_GRIT_SCALE, 3);
    }
}

impl DaubBrush {
    /// Whether the brush lays a tooth, which is laid by where on the canvas a
    /// pixel is.
    pub fn is_rough(self) -> bool {
        matches!(self, DaubBrush::LightRough | DaubBrush::DarkRough | DaubBrush::Sparkle)
    }
}

/// How far the daub reaches across and down, in pixels either side.
pub fn daub_reach(size: u32, brush: DaubBrush) -> (u32, u32) {
    let reach = (size as f32 * DAUB_REACH).round().max(1.0);
    match brush {
        DaubBrush::WideSharp | DaubBrush::WideBlurry => (
            (reach * WIDE_STRETCH) as u32,
            (reach / WIDE_STRETCH).max(1.0) as u32,
        ),
        _ => (reach as u32, reach as u32),
    }
}

/// Noise in each colour channel, blurred to specks `scale` pixels across and
/// centred on 127.5. `salt` keeps two uses from being the same noise.
pub(crate) fn blurred_specks(width: u32, height: u32, scale: f32, salt: usize) -> Pixmap {
    let mut specks = Pixmap::new(width, height);
    specks
        .as_bytes_mut()
        .par_chunks_exact_mut(width as usize * 4)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                for c in 0..3 {
                    px[c] = ((speck(x as i32, y as i32, c + salt) * 0.5 + 0.5) * 255.0) as u8;
                }
                px[3] = 255;
            }
        });
    crate::filters::convolve::gaussian_blur_accelerated(&mut specks, scale);
    specks
}

/// What [`blurred_specks`] at `scale` must be multiplied by to spread about
/// one level. A blur of σ leaves white noise about 1/(2√π·σ) of its spread,
/// and `speck` starts at about 52 levels.
pub(crate) fn speck_gain(scale: f32) -> f32 {
    2.0 * std::f32::consts::PI.sqrt() * scale / 52.0
}

/// Add colour noise of about `levels` spread, in specks `scale` pixels
/// across, each channel on its own. `salt` keeps two coats from being the
/// same noise.
fn lay_tooth(pixmap: &mut Pixmap, levels: f32, scale: f32, salt: usize) {
    let tooth = blurred_specks(pixmap.width(), pixmap.height(), scale, salt);
    let gain = levels * speck_gain(scale);
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .zip(tooth.as_bytes().par_chunks_exact(stride))
        .for_each(|(out, tooth)| {
            for (out, tooth) in out.chunks_exact_mut(4).zip(tooth.chunks_exact(4)) {
                for c in 0..3 {
                    let v = out[c] as f32 + (tooth[c] as f32 - 127.5) * gain;
                    out[c] = v.clamp(0.0, 255.0).round() as u8;
                }
            }
        });
}

/// Sparkle's contour lines: how many levels of brightness apart they are
/// drawn, how far and how broadly the wobble pushes them about, how thin they
/// are, and how bright they are per step of Sharpness. The dark lines are
/// drawn at a fraction of the light ones'.
const CONTOUR_STEP: f32 = 8.0;
const CONTOUR_SOFTEN: f32 = 2.0;
const CONTOUR_WOBBLE: f32 = 12.0;
const CONTOUR_WOBBLE_SCALE: f32 = 12.0;
const CONTOUR_THIN: f32 = 8.0;
const CONTOUR_PER_STEP: f32 = 3.0;
const CONTOUR_DARK: f32 = 0.5;

/// Sparkle's sharpening: scale in pixels and strength per step.
const SPARKLE_SCALE: f32 = 1.5;
const SPARKLE_PER_STEP: f32 = 0.1;

/// Sparkle's brilliance: an overall lift (gain after a gamma), then a push
/// towards white that starts at [`BRILLIANCE_FROM`] and leaves the darks alone.
const BRILLIANCE_GAMMA: f32 = 0.85;
const BRILLIANCE_GAIN: f32 = 1.1;
const BRILLIANCE_FROM: f32 = 0.3;
const BRILLIANCE_PUSH: f32 = 0.5;

/// Sparkle: the daubs with contour lines drawn through them, sharpened, and
/// lit up.
///
/// The swirling lines all over CS6's Sparkle, which are densest in the
/// out-of-focus background, are iso-lines of brightness: where the picture
/// shades slowly they are far apart and follow the shading, and a slow random
/// wobble added to the brightness first keeps them from reading as a
/// topographic map. Light lines are drawn stronger than dark ones, which is
/// half of the brilliance; the other half is a tone curve that sends the light
/// parts towards white channel by channel, so the petals go pale rather than
/// merely brighter while the background stays dark.
fn sparkle(pixmap: &mut Pixmap, sharpness: f32) {
    use std::f32::consts::TAU;

    let mut field = pixmap.clone();
    crate::filters::convolve::gaussian_blur_accelerated(&mut field, CONTOUR_SOFTEN);
    let wobble = blurred_specks(pixmap.width(), pixmap.height(), CONTOUR_WOBBLE_SCALE, 7);
    let wobble_gain = CONTOUR_WOBBLE * speck_gain(CONTOUR_WOBBLE_SCALE);
    let amp = sharpness * CONTOUR_PER_STEP;

    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .zip(field.as_bytes().par_chunks_exact(stride))
        .zip(wobble.as_bytes().par_chunks_exact(stride))
        .for_each(|((out, field), wobble)| {
            for ((out, f), wb) in out
                .chunks_exact_mut(4)
                .zip(field.chunks_exact(4))
                .zip(wobble.chunks_exact(4))
            {
                let lum = 0.299 * f[0] as f32 + 0.587 * f[1] as f32 + 0.114 * f[2] as f32;
                let level = lum + (wb[0] as f32 - 127.5) * wobble_gain;
                let wave = (level / CONTOUR_STEP * TAU).cos();
                let line = wave.max(0.0).powf(CONTOUR_THIN)
                    - CONTOUR_DARK * (-wave).max(0.0).powf(CONTOUR_THIN);
                for c in 0..3 {
                    out[c] = (out[c] as f32 + line * amp).clamp(0.0, 255.0).round() as u8;
                }
            }
        });

    let sharp = pixmap.clone();
    let mut soft = pixmap.clone();
    crate::filters::convolve::gaussian_blur_accelerated(&mut soft, SPARKLE_SCALE);
    put_back(pixmap, &sharp, &soft, sharpness * SPARKLE_PER_STEP);

    let brilliant: [u8; 256] = std::array::from_fn(|v| {
        let x = ((v as f32 / 255.0).powf(BRILLIANCE_GAMMA) * BRILLIANCE_GAIN).min(1.0);
        let k = ((x - BRILLIANCE_FROM) / (1.0 - BRILLIANCE_FROM)).clamp(0.0, 1.0);
        let k = k * k * (3.0 - 2.0 * k);
        let x = x + BRILLIANCE_PUSH * k * (1.0 - x);
        (x * 255.0).round() as u8
    });
    pixmap.as_bytes_mut().par_chunks_exact_mut(4).for_each(|px| {
        for c in 0..3 {
            px[c] = brilliant[px[c] as usize];
        }
    });
}

/// An unsharp mask at [`EDGE_SCALE`], channel by channel.
fn sharpen(pixmap: &mut Pixmap, gain: f32) {
    if gain <= 0.0 {
        return;
    }
    let sharp = pixmap.clone();
    let mut soft = pixmap.clone();
    crate::filters::convolve::gaussian_blur_accelerated(&mut soft, EDGE_SCALE);
    put_back(pixmap, &sharp, &soft, gain);
}

/// Which side of a boundary a Rough brush draws its halo on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Halo {
    Light,
    Dark,
}

/// The Rough brushes' halo, taken on brightness and laid on all three
/// channels alike, so that a dark halo goes to black rather than to a deeper
/// shade of whatever it was.
fn rough_edge(pixmap: &mut Pixmap, gain: f32, keep: Halo) {
    if gain <= 0.0 {
        return;
    }
    let mut soft = pixmap.clone();
    crate::filters::convolve::gaussian_blur_accelerated(&mut soft, ROUGH_SCALE);
    let lum = |p: &[u8]| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .zip(soft.as_bytes().par_chunks_exact(stride))
        .for_each(|(out, soft)| {
            for (out, soft) in out.chunks_exact_mut(4).zip(soft.chunks_exact(4)) {
                let halo = lum(out) - lum(soft);
                let halo = match keep {
                    Halo::Light => halo.max(0.0),
                    Halo::Dark => halo.min(0.0),
                } * gain;
                for c in 0..3 {
                    out[c] = (out[c] as f32 + halo).clamp(0.0, 255.0).round() as u8;
                }
            }
        });
}

/// Add what `over` has and `under` does not to whatever is in `pixmap`.
fn put_back(pixmap: &mut Pixmap, over: &Pixmap, under: &Pixmap, gain: f32) {
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
                    let lift = (over[c] as f32 - under[c] as f32) * gain;
                    out[c] = (out[c] as f32 + lift).clamp(0.0, 255.0).round() as u8;
                }
            }
        });
}

/// How broad a swell counts as the picture's shading rather than its relief,
/// in pixels. The relief is the picture less a blur this wide, so a bright
/// shape stands up out of the wrap and the wrap dips into a trough around it
/// before it settles — and the far wall of that trough is what catches the
/// light in the ring CS6 draws a little way outside every petal.
const WRAP_SHADING: f32 = 8.0;

/// How far the relief is smoothed before it is lit, in pixels: a floor, and
/// a step of Smoothness. Wrap laid over a surface does not follow its every
/// grain; the smoother it is, the broader and fewer its folds.
const WRAP_SMOOTH_FLOOR: f32 = 1.0;
const WRAP_SMOOTH_PER_STEP: f32 = 0.7;

/// How hard the relief is pushed up before it is lit: a floor, and a step of
/// Detail. It goes through `tanh`, so strong relief is capped and faint relief
/// is what the slider really raises — which is what brings up the crinkles
/// in the background as Detail goes up.
const WRAP_DETAIL_FLOOR: f32 = 0.5;
const WRAP_DETAIL_PER_STEP: f32 = 0.08;

/// How steep the relief stands, as a multiplier on its slope.
const WRAP_DEPTH: f32 = 250.0;

/// Where the light is — up and to the left, the convention relief is read
/// by — and how tight the highlight is. High, because plastic is glossy: the
/// highlights are thin bright streaks, not a sheen.
const WRAP_LIGHT: [f32; 3] = [-1.0, -1.0, 1.2];
const WRAP_SHINE: f32 = 50.0;

/// How bright the highlights are at the top of Highlight Strength, and how
/// sharply that grows along the slider. CS6's low settings are barely there,
/// so the growth is steeper than a straight line.
const WRAP_GLOSS: f32 = 5.0;
const WRAP_GLOSS_CURVE: f32 = 1.6;

/// How much the wrap dulls the picture under it at the top of Highlight
/// Strength.
const WRAP_DULL: f32 = 0.25;

/// Filter ▸ Artistic ▸ Plastic Wrap: the picture shrink-wrapped in glossy
/// plastic.
///
/// The picture's brightness is read as a surface — light things stand up,
/// dark things sink — and the surface is lit by one light with a tight
/// specular highlight. Only the highlight is laid back on, towards white, over
/// a slightly dulled copy of the picture; the plastic itself is clear.
///
/// **Highlight Strength** is how bright the highlights are. **Detail** is how
/// much of the picture's faint relief the wrap picks up. **Smoothness** is how
/// broad its folds are.
///
/// Alpha is left alone: wrapping the picture does not change the layer's
/// shape.
///
/// No GPU path. The work is three blurs of a floating-point height field —
/// in bytes the relief is a level or two deep and its slopes would come back
/// as steps — and the backend's blur is over bytes. The lighting is one pass
/// per pixel, which would upload its input and read it straight back.
pub fn plastic_wrap(pixmap: &mut Pixmap, highlight: u32, detail: u32, smoothness: u32) {
    if pixmap.is_empty() {
        return;
    }
    let highlight = highlight.clamp(*WRAP_HIGHLIGHT.start(), *WRAP_HIGHLIGHT.end()) as f32;
    let detail = detail.clamp(*WRAP_DETAIL.start(), *WRAP_DETAIL.end()) as f32;
    let smoothness = smoothness.clamp(*WRAP_SMOOTHNESS.start(), *WRAP_SMOOTHNESS.end()) as f32;
    let (width, height) = (pixmap.width() as usize, pixmap.height() as usize);

    let brightness: Vec<f32> = pixmap
        .as_bytes()
        .par_chunks_exact(4)
        .map(|p| (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) / 255.0)
        .collect();
    let mut shading = brightness.clone();
    blur_field(&mut shading, width, height, WRAP_SHADING);
    let mut relief: Vec<f32> = brightness.iter().zip(&shading).map(|(b, s)| b - s).collect();
    blur_field(
        &mut relief,
        width,
        height,
        WRAP_SMOOTH_FLOOR + smoothness * WRAP_SMOOTH_PER_STEP,
    );
    let push = WRAP_DETAIL_FLOOR + detail * WRAP_DETAIL_PER_STEP;
    relief.par_iter_mut().for_each(|r| *r = (*r * push).tanh());

    // The half-way vector between the light and a viewer straight above.
    let light = normalise(WRAP_LIGHT);
    let half = normalise([light[0], light[1], light[2] + 1.0]);
    let gloss = (highlight / *WRAP_HIGHLIGHT.end() as f32).powf(WRAP_GLOSS_CURVE) * WRAP_GLOSS;
    let dull = 1.0 - WRAP_DULL * highlight / *WRAP_HIGHLIGHT.end() as f32;

    let relief = &relief;
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, out)| {
            // Central differences, one-sided at the edges.
            let (up, down) = (y.saturating_sub(1), (y + 1).min(height - 1));
            for (x, px) in out.chunks_exact_mut(4).enumerate() {
                let (left, right) = (x.saturating_sub(1), (x + 1).min(width - 1));
                let dx = (relief[y * width + right] - relief[y * width + left])
                    / (right - left).max(1) as f32;
                let dy = (relief[down * width + x] - relief[up * width + x])
                    / (down - up).max(1) as f32;
                let normal = normalise([-dx * WRAP_DEPTH, -dy * WRAP_DEPTH, 1.0]);
                let facing = (normal[0] * half[0] + normal[1] * half[1] + normal[2] * half[2])
                    .max(0.0);
                let shine = (facing.powf(WRAP_SHINE) * gloss).min(1.0);
                for c in 0..3 {
                    let under = px[c] as f32 * dull;
                    px[c] = (under + (255.0 - under) * shine).round().clamp(0.0, 255.0) as u8;
                }
                // Alpha stands: wrapping the picture does not change the
                // layer's shape.
            }
        });
}

fn normalise(v: [f32; 3]) -> [f32; 3] {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / length, v[1] / length, v[2] / length]
}

/// A Gaussian blur of a floating-point field, with the edge pixels standing
/// in for what is past them.
pub(crate) fn blur_field(field: &mut [f32], width: usize, height: usize, sigma: f32) {
    if sigma <= 0.0 {
        return;
    }
    let taps = (sigma * 3.0).ceil() as i32;
    let kernel = crate::filters::convolve::gaussian_kernel_1d(sigma, taps);
    let kernel = &kernel;
    let mut scratch = field.to_vec();
    field
        .par_chunks_exact_mut(width)
        .zip(scratch.par_chunks_exact(width))
        .for_each(|(out, row)| {
            for (x, slot) in out.iter_mut().enumerate() {
                *slot = kernel
                    .iter()
                    .enumerate()
                    .map(|(k, w)| {
                        let at = (x as i32 + k as i32 - taps).clamp(0, width as i32 - 1);
                        w * row[at as usize]
                    })
                    .sum();
            }
        });
    scratch.copy_from_slice(field);
    let scratch = &scratch;
    field
        .par_chunks_exact_mut(width)
        .enumerate()
        .for_each(|(y, out)| {
            for (x, slot) in out.iter_mut().enumerate() {
                *slot = kernel
                    .iter()
                    .enumerate()
                    .map(|(k, w)| {
                        let at = (y as i32 + k as i32 - taps).clamp(0, height as i32 - 1);
                        w * scratch[at as usize * width + x]
                    })
                    .sum();
            }
        });
}

/// How far the wrap reaches, in pixels: the blur that takes the shading out
/// and the one that smooths the relief, three sigma each, and the slope's
/// one pixel either side.
pub fn plastic_wrap_reach(smoothness: u32) -> u32 {
    let smoothness = smoothness.clamp(*WRAP_SMOOTHNESS.start(), *WRAP_SMOOTHNESS.end()) as f32;
    ((WRAP_SHADING + WRAP_SMOOTH_FLOOR + smoothness * WRAP_SMOOTH_PER_STEP) * 3.0).ceil() as u32 + 2
}

/// How far the picture is smoothed before it is posterized, in pixels. It is
/// what gives the bands blotchy, rounded outlines rather than the ragged ones
/// a photograph's grain would cut.
const POSTER_SETTLE: f32 = 2.0;

/// How many bands of brightness Posterization gives: a floor, plus a step,
/// plus a step that grows with the slider. CS6 is harsh at the bottom of the
/// slider — three bands, so the darkest parts of the background go black — and
/// by the top the banding is barely there.
const POSTER_BANDS: f32 = 3.0;
const POSTER_BANDS_CURVE: f32 = 0.25;

/// Where between two bands a brightness is rounded up rather than down. Over
/// a half, so that a band only goes to black when it is well into the dark:
/// CS6's lowest setting turns a mid-green background bright green with black
/// only in its deepest shadow.
const POSTER_ROUND: f32 = 0.64;

/// The edges: how far round each pixel the ink looks, as a floor and a step
/// of Edge Thickness; how much darker than that a pixel must be before it is
/// inked, in levels; and how much ink a level of darkness is worth, as a floor
/// and a step of Edge Intensity.
///
/// Ink goes where a pixel is darker than what is round it, which is the dark
/// side of every boundary — the background just outside a petal — and along
/// anything thin and dark, like a vein. That is where CS6 draws.
const POSTER_EDGE_SOFTEN: f32 = 0.5;
const POSTER_REACH: f32 = 1.5;
const POSTER_REACH_PER_STEP: f32 = 0.5;
const POSTER_INK_FROM: f32 = 2.0;
const POSTER_INK: f32 = 12.0;
const POSTER_INK_PER_STEP: f32 = 4.0;

/// Filter ▸ Artistic ▸ Poster Edges: the picture posterized, with its edges
/// inked in black.
///
/// **Posterization** bands the picture's brightness and keeps its colour: each
/// pixel is scaled to its band's brightness, so a background comes back as a
/// few flat greens rather than as the few flat primaries per-channel
/// posterizing would give. **Edge Thickness** is how broad the ink is, and
/// **Edge Intensity** how much of the picture it picks up — at the top, every
/// vein is hatched in.
///
/// Alpha is left alone: posterizing the picture does not change the layer's
/// shape.
///
/// No GPU path. What costs is the blurs — the settling one already goes
/// through the backend, and the one the ink measures against is over floats —
/// and the rest is one pass that would upload its input and read it straight
/// back.
pub fn poster_edges(pixmap: &mut Pixmap, thickness: u32, intensity: u32, posterization: u32) {
    if pixmap.is_empty() {
        return;
    }
    let thickness = thickness.clamp(*POSTER_THICKNESS.start(), *POSTER_THICKNESS.end()) as f32;
    let intensity = intensity.clamp(*POSTER_INTENSITY.start(), *POSTER_INTENSITY.end()) as f32;
    let posterization =
        posterization.clamp(*POSTER_LEVELS.start(), *POSTER_LEVELS.end()) as f32;
    let (width, height) = (pixmap.width() as usize, pixmap.height() as usize);
    let brightness_of = |p: &[u8]| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;

    let mut settled = pixmap.clone();
    crate::filters::convolve::gaussian_blur_accelerated(&mut settled, POSTER_SETTLE);

    let mut softened = pixmap.clone();
    crate::filters::convolve::gaussian_blur_accelerated(&mut softened, POSTER_EDGE_SOFTEN);
    let brightness: Vec<f32> = softened.as_bytes().par_chunks_exact(4).map(brightness_of).collect();
    let mut around = brightness.clone();
    blur_field(
        &mut around,
        width,
        height,
        POSTER_REACH + thickness * POSTER_REACH_PER_STEP,
    );

    let steps = POSTER_BANDS + posterization + posterization * posterization * POSTER_BANDS_CURVE - 1.0;
    let ink_gain = (POSTER_INK + intensity * POSTER_INK_PER_STEP) / 255.0;
    let (brightness, around) = (&brightness, &around);
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .zip(settled.as_bytes().par_chunks_exact(stride))
        .enumerate()
        .for_each(|(y, (out, settled))| {
            for (x, (px, from)) in out.chunks_exact_mut(4).zip(settled.chunks_exact(4)).enumerate() {
                let level = brightness_of(from);
                let band = (level / 255.0 * steps + POSTER_ROUND).floor().min(steps) * 255.0 / steps;
                let scale = band / level.max(1.0);

                let i = y * width + x;
                let darker = around[i] - brightness[i] - POSTER_INK_FROM;
                let bare = 1.0 - (darker * ink_gain).clamp(0.0, 1.0);

                for c in 0..3 {
                    px[c] = ((from[c] as f32 * scale).min(255.0) * bare).round() as u8;
                }
                // Alpha stands: posterizing the picture does not change the
                // layer's shape.
            }
        });
}

/// How far Poster Edges reaches, in pixels: the wider of the settling blur
/// and the one the ink measures against, three sigma each, with the ink's own
/// softening on top.
pub fn poster_edges_reach(thickness: u32) -> u32 {
    let thickness = thickness.clamp(*POSTER_THICKNESS.start(), *POSTER_THICKNESS.end()) as f32;
    let widest = POSTER_SETTLE.max(POSTER_REACH + thickness * POSTER_REACH_PER_STEP);
    ((widest + POSTER_EDGE_SOFTEN) * 3.0).ceil() as u32 + 1
}

/// How long a pastel stroke is, in pixels: a floor, so that even Stroke
/// Length 0 is drawn in strokes as CS6's is, and a step of the slider.
///
/// This is the length of the chalk's *grain*. The picture itself is smeared
/// far less — [`PASTEL_SMEAR`] — because CS6's pastel keeps the picture sharp
/// at any Stroke Length: the long streaks are chalk laid over it, not the
/// picture dragged out. Smearing the picture by the stroke's full length
/// turns it into a motion blur.
const PASTEL_STROKE: f32 = 6.0;
const PASTEL_STROKE_PER_STEP: f32 = 0.6;
const PASTEL_SMEAR: f32 = 2.0;
const PASTEL_SMEAR_PER_STEP: f32 = 0.1;

/// Which way the strokes run: up and to the right, as CS6's do, in degrees
/// anticlockwise from the horizontal.
const PASTEL_ANGLE: f32 = 45.0;

/// How far along itself a stroke drags its colour, in stroke lengths, at its
/// furthest. Each stroke takes its colour from a little way up or down its own
/// line, which is what breaks an outline into the ragged, overlapping marks of
/// chalk rather than a clean smear.
const PASTEL_DRAG: f32 = 0.12;

/// How much of the picture's fine detail each stroke carries, as a floor and
/// a step of Stroke Detail, and the scale in pixels below which it counts as
/// detail. High detail is what lays CS6's bright scratches over the horse's
/// highlights.
const PASTEL_DETAIL_SCALE: f32 = 2.0;
const PASTEL_DETAIL_FLOOR: f32 = 0.5;
const PASTEL_DETAIL_PER_STEP: f32 = 0.15;

/// The grain of the chalk, in levels of spread. It is strongest in the dark,
/// where chalk goes on thin and catches only on the tooth of the paper, and
/// faintest in the light, where it is laid thick.
const PASTEL_GRAIN: f32 = 22.0;
const PASTEL_GRAIN_DARK: f32 = 1.2;

/// How much lighter than the picture the pastel is, as a gamma: chalk is
/// opaque and pale, and CS6's pastel is a washed-out copy of the photograph.
const PASTEL_PALE: f32 = 0.8;

/// Filter ▸ Artistic ▸ Rough Pastels: the picture drawn in coloured chalk on
/// a textured surface.
///
/// Two stages, the second shared with the other textured filters.
///
/// 1. **The strokes.** The picture is dragged into diagonal strokes, each
///    carrying some of the fine detail under it, with the grain of the chalk
///    over the top, and then paled. **Stroke Length** is how long the strokes
///    are; **Stroke Detail** how much of the picture they carry.
/// 2. **The surface.** The strokes are lit as if drawn on the chosen texture —
///    see [`crate::filters::texture::apply_relief`] for Texture, Scaling,
///    Relief, Light and Invert.
///
/// Alpha is left alone.
///
/// No GPU path. The strokes are sampled along a line, which is a fit, but
/// every stage wants the previous one's result on the CPU and each would
/// upload and read back — the per-call cost compositing already showed is not
/// worth paying (docs/gpu-migration.md).
#[allow(clippy::too_many_arguments)]
pub fn rough_pastels(
    pixmap: &mut Pixmap,
    length: u32,
    detail: u32,
    texture: crate::filters::texture::Texture,
    scaling: u32,
    relief: u32,
    light: crate::filters::texture::Light,
    invert: bool,
) {
    if pixmap.is_empty() {
        return;
    }
    let length = length.clamp(*PASTEL_LENGTH.start(), *PASTEL_LENGTH.end()) as f32;
    let detail = detail.clamp(*PASTEL_DETAIL.start(), *PASTEL_DETAIL.end()) as f32;
    let (width, height) = (pixmap.width() as usize, pixmap.height() as usize);
    let stroke = PASTEL_STROKE + length * PASTEL_STROKE_PER_STEP;
    let (along_x, along_y) = {
        let radians = PASTEL_ANGLE.to_radians();
        (radians.cos(), -radians.sin())
    };

    // Where each stroke takes its colour from.
    let drag = streaked_noise(width, height, stroke * 2.0 + 3.0, 11, (along_x, along_y));
    let picture = pixmap.clone();
    let mut dragged = picture.clone();
    dragged
        .as_bytes_mut()
        .par_chunks_exact_mut(width * 4)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let offset = drag[y * width + x].clamp(-2.0, 2.0) * stroke * PASTEL_DRAG;
                let sx = (x as f32 + along_x * offset).round().clamp(0.0, (width - 1) as f32);
                let sy = (y as f32 + along_y * offset).round().clamp(0.0, (height - 1) as f32);
                let from = (sy as usize * width + sx as usize) * 4;
                px[..3].copy_from_slice(&picture.as_bytes()[from..from + 3]);
            }
        });
    let mut strokes = dragged;
    let smear = PASTEL_SMEAR + length * PASTEL_SMEAR_PER_STEP;
    crate::filters::convolve::motion_blur(&mut strokes, PASTEL_ANGLE, smear);

    // The picture's fine detail, drawn along the stroke: a streak of the
    // picture less a streak of its blur, which is the streak of the detail.
    let carried = smear;
    let mut sharp = picture.clone();
    crate::filters::convolve::motion_blur(&mut sharp, PASTEL_ANGLE, carried);
    let mut soft = picture.clone();
    crate::filters::convolve::gaussian_blur_accelerated(&mut soft, PASTEL_DETAIL_SCALE);
    crate::filters::convolve::motion_blur(&mut soft, PASTEL_ANGLE, carried);
    let carry = PASTEL_DETAIL_FLOOR + detail * PASTEL_DETAIL_PER_STEP;

    let grain = streaked_noise(width, height, stroke * 1.5 + 3.0, 12, (along_x, along_y));
    let pale: [f32; 256] = std::array::from_fn(|v| 255.0 * (v as f32 / 255.0).powf(PASTEL_PALE));

    let stride = pixmap.stride();
    let (sharp, soft, grain) = (&sharp, &soft, &grain);
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .zip(strokes.as_bytes().par_chunks_exact(stride))
        .enumerate()
        .for_each(|(y, (out, strokes))| {
            let (sharp, soft) = (sharp.row(y as u32), soft.row(y as u32));
            for x in 0..width {
                let i = x * 4;
                let mut chalk = [0.0f32; 3];
                for c in 0..3 {
                    chalk[c] = strokes[i + c] as f32
                        + (sharp[i + c] as f32 - soft[i + c] as f32) * carry;
                }
                let lightness = (0.299 * chalk[0] + 0.587 * chalk[1] + 0.114 * chalk[2]) / 255.0;
                let tooth = grain[y * width + x] * PASTEL_GRAIN * (PASTEL_GRAIN_DARK - lightness);
                for c in 0..3 {
                    let v = (chalk[c] + tooth).clamp(0.0, 255.0);
                    out[i + c] = pale[v.round() as usize].round() as u8;
                }
                // Alpha stands: drawing the picture does not change the
                // layer's shape.
            }
        });

    crate::filters::texture::apply_relief(pixmap, texture, scaling, relief, light, invert);
}

/// White noise averaged along `along` over `length` pixels, then scaled to a
/// spread of one: streaks, each about as long as a stroke. Laid by where on
/// the canvas a pixel is.
pub(crate) fn streaked_noise(width: usize, height: usize, length: f32, salt: usize, along: (f32, f32)) -> Vec<f32> {
    let noise: Vec<f32> = (0..width * height)
        .into_par_iter()
        .map(|i| speck((i % width) as i32, (i / width) as i32, salt))
        .collect();
    let steps = length.round().max(1.0) as i32;
    let mut streaks = vec![0.0f32; width * height];
    streaks
        .par_chunks_exact_mut(width)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, slot) in row.iter_mut().enumerate() {
                let (mut total, mut count) = (0.0f32, 0.0f32);
                for step in -steps / 2..=steps / 2 {
                    let sx = (x as f32 + along.0 * step as f32).round() as i32;
                    let sy = (y as f32 + along.1 * step as f32).round() as i32;
                    if sx < 0 || sy < 0 || sx >= width as i32 || sy >= height as i32 {
                        continue;
                    }
                    total += noise[sy as usize * width + sx as usize];
                    count += 1.0;
                }
                *slot = total / count.max(1.0);
            }
        });
    let spread = (streaks.par_iter().map(|v| v * v).sum::<f32>() / streaks.len() as f32).sqrt();
    if spread > 0.0 {
        streaks.par_iter_mut().for_each(|v| *v /= spread);
    }
    streaks
}

/// How long a smudge is, in pixels: a floor and a step of Stroke Length.
const SMUDGE_STROKE: f32 = 8.0;
const SMUDGE_STROKE_PER_STEP: f32 = 2.0;

/// How big a smudged patch is, in pixels either side: a floor and a step of
/// Stroke Length. CS6's smudging is soft paint with crisp edges between
/// patches, not a streak — a long streak reads as motion blur.
const SMUDGE_PATCH: f32 = 1.0;
const SMUDGE_PATCH_PER_STEP: f32 = 0.35;

/// The fine streaks: how much of the picture's own detail, dragged along the
/// stroke, goes back on, and how long the drag is as a fraction of the stroke;
/// and a light grain along the strokes in the darks, in levels. This is what
/// CS6's smudging is made of close up — dense, thin diagonal streaks over
/// crisp patches. Without them the smear reads as a blur.
const SMUDGE_STREAK: f32 = 0.8;
const SMUDGE_STREAK_LENGTH: f32 = 0.5;
const SMUDGE_GRAIN: f32 = 7.0;

/// Which way the smudges run, in degrees anticlockwise from the horizontal.
const SMUDGE_ANGLE: f32 = 45.0;

/// How much the smudge favours the darker of the picture and its streak —
/// the stick drags dark into light, not light into dark — and how far the
/// smudging is confined to the dark: a power on darkness, so the lights are
/// barely touched.
const SMUDGE_DARK_DRAG: f32 = 0.8;
const SMUDGE_DARK_ONLY: f32 = 0.4;

/// How much of the smear goes over the patches at most. Short of all of it,
/// so a bridle or an eye still reads through the strokes.
const SMUDGE_MIX: f32 = 0.5;

/// How much the tones below the highlights are deepened, most in the
/// midtones and fading to nothing at black and at the highlights. The stick lays dark as well as
/// lifting light: CS6's shadows and midtones come back heavier than the
/// photograph's at any Intensity.
const SMUDGE_DEEPEN: f32 = 0.4;

/// Where the highlights start, as a fraction of white: at Highlight Area 0,
/// and how far down each step takes it. And how soft the start is.
const SMUDGE_HIGHLIGHT_FROM: f32 = 0.9;
const SMUDGE_HIGHLIGHT_PER_STEP: f32 = 0.02;
const SMUDGE_HIGHLIGHT_SOFT: f32 = 0.15;

/// At Intensity 10: how far the highlights are carried towards white, and how
/// much the contrast is raised.
const SMUDGE_LIFT: f32 = 0.9;
const SMUDGE_CONTRAST: f32 = 0.2;

/// Filter ▸ Artistic ▸ Smudge Stick: the picture's darks smeared along short
/// diagonal strokes, and its lights brightened towards white.
///
/// **Stroke Length** is how long the smudges are. **Highlight Area** is how
/// far down the tones the brightening reaches — at 20 most of a sunlit
/// picture burns out. **Intensity** is how hard the lights are brightened and
/// the contrast raised.
///
/// Alpha is left alone.
///
/// The streaks are mostly the picture's own detail, dragged; the grain along
/// them is kept light, because a heavy one reads as pencil hatching, which is
/// Rough Pastels' look, not this one.
///
/// No GPU path. The smudge is a streak along a line and the rest one pass per
/// pixel, and each would upload its input and read it straight back.
pub fn smudge_stick(pixmap: &mut Pixmap, length: u32, highlight: u32, intensity: u32) {
    if pixmap.is_empty() {
        return;
    }
    let length = length.clamp(*SMUDGE_LENGTH.start(), *SMUDGE_LENGTH.end()) as f32;
    let highlight = highlight.clamp(*SMUDGE_HIGHLIGHT.start(), *SMUDGE_HIGHLIGHT.end()) as f32;
    let intensity = intensity.clamp(*SMUDGE_INTENSITY.start(), *SMUDGE_INTENSITY.end()) as f32
        / *SMUDGE_INTENSITY.end() as f32;
    let width = pixmap.width() as usize;
    let stroke = SMUDGE_STROKE + length * SMUDGE_STROKE_PER_STEP;

    // Patches first — a median keeps their edges and loses the fine detail
    // inside them — then a short drag along the stroke.
    let patch = (SMUDGE_PATCH + length * SMUDGE_PATCH_PER_STEP).round() as u32;
    let patches = crate::filters::convolve::median_of(pixmap, patch, patch);
    let mut streak = patches.clone();
    crate::filters::convolve::motion_blur(&mut streak, SMUDGE_ANGLE, stroke);
    // The picture's detail — what the patches lost — dragged along the
    // stroke, held about mid-grey so it survives in bytes.
    let mut detail = pixmap.clone();
    for (d, p) in detail.as_bytes_mut().chunks_exact_mut(4).zip(patches.as_bytes().chunks_exact(4)) {
        for c in 0..3 {
            d[c] = (128 + (d[c] as i32 - p[c] as i32) / 2).clamp(0, 255) as u8;
        }
    }
    crate::filters::convolve::motion_blur(&mut detail, SMUDGE_ANGLE, stroke * SMUDGE_STREAK_LENGTH);
    let along = {
        let radians = SMUDGE_ANGLE.to_radians();
        (radians.cos(), -radians.sin())
    };
    let grain = streaked_noise(width, pixmap.height() as usize, stroke * SMUDGE_STREAK_LENGTH + 3.0, 13, along);
    let mut darkest = darkest_along(&patches, SMUDGE_ANGLE, stroke);
    crate::filters::convolve::motion_blur(&mut darkest, SMUDGE_ANGLE, stroke * 0.5);

    let from = (SMUDGE_HIGHLIGHT_FROM - highlight * SMUDGE_HIGHLIGHT_PER_STEP) * 255.0;
    let soft = SMUDGE_HIGHLIGHT_SOFT * 255.0;
    let contrast = 1.0 + intensity * SMUDGE_CONTRAST;
    let lift = intensity * SMUDGE_LIFT;
    let luma = |p: [f32; 3]| 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2];

    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .zip(streak.as_bytes().par_chunks_exact(stride))
        .zip(darkest.as_bytes().par_chunks_exact(stride))
        .zip(patches.as_bytes().par_chunks_exact(stride))
        .zip(detail.as_bytes().par_chunks_exact(stride))
        .enumerate()
        .for_each(|(y, ((((out, streak), darkest), patches), detail))| {
            for x in 0..width {
                let i = x * 4;
                let picture = [out[i] as f32, out[i + 1] as f32, out[i + 2] as f32];
                let dragged = [darkest[i] as f32, darkest[i + 1] as f32, darkest[i + 2] as f32];
                // The darker of the pixel and the darkest along its stroke, so
                // dark dragged into a light is smudged there too — the dark
                // streaks CS6 lays through spray next to a dark leg.
                let darkness = 1.0 - luma(picture).min(luma(dragged)) / 255.0;
                let own_darkness = 1.0 - luma(picture) / 255.0;
                let smudged = darkness.powf(SMUDGE_DARK_ONLY) * SMUDGE_MIX;
                let mut paint = [0.0f32; 3];
                for c in 0..3 {
                    let s = streak[i + c] as f32;
                    // The darkest along the stroke, so dark is carried along
                    // it as a mark rather than averaged away — and taken from
                    // the patches, not the picture, whose own darks are sharp
                    // and would come back as speckle.
                    let d = darkest[i + c] as f32;
                    let smear = s + (d.min(s) - s) * SMUDGE_DARK_DRAG;
                    // Over the patches, which keep the shapes readable under
                    // the smear, and not over the picture's own grain.
                    let under = picture[c] + (patches[i + c] as f32 - picture[c]) * own_darkness.powf(SMUDGE_DARK_ONLY);
                    let streaks = (detail[i + c] as f32 - 128.0) * 2.0 * SMUDGE_STREAK
                        + grain[y * width + x] * SMUDGE_GRAIN * own_darkness;
                    let v = under + (smear - under) * smudged + streaks;
                    paint[c] = (v - 128.0) * contrast + 128.0;
                }
                let below = (1.0 - luma(paint) / from.max(1.0)).clamp(0.0, 1.0);
                // Heaviest in the midtones: the blacks are black already, and
                // taking them further only loses what is in them.
                let deepen = 1.0 - SMUDGE_DEEPEN * 4.0 * below * (1.0 - below);
                for v in paint.iter_mut() {
                    *v *= deepen;
                }
                let over = ((luma(paint) - from) / soft).clamp(0.0, 1.0);
                let lit = over * over * (3.0 - 2.0 * over) * lift;
                for c in 0..3 {
                    let v = paint[c] + (255.0 - paint[c]) * lit;
                    out[i + c] = v.clamp(0.0, 255.0).round() as u8;
                }
                // Alpha stands: smudging the picture does not change the
                // layer's shape.
            }
        });
}

/// The darkest value along a line through each pixel, channel by channel.
fn darkest_along(source: &Pixmap, angle: f32, length: f32) -> Pixmap {
    let steps = length.round().max(1.0) as i32;
    let radians = angle.to_radians();
    let (dx, dy) = (radians.cos(), -radians.sin());
    let (width, height) = (source.width() as i32, source.height() as i32);
    let mut out = source.clone();
    let stride = out.stride();
    out.as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, row)| {
            for x in 0..width {
                let i = x as usize * 4;
                for step in -steps / 2..=steps / 2 {
                    let sx = (x as f32 + dx * step as f32).round() as i32;
                    let sy = (y as f32 + dy * step as f32).round() as i32;
                    if sx < 0 || sy < 0 || sx >= width || sy >= height {
                        continue;
                    }
                    let line = source.row(sy as u32);
                    let j = sx as usize * 4;
                    for c in 0..3 {
                        row[i + c] = row[i + c].min(line[j + c]);
                    }
                }
            }
        });
    out
}

/// How far Smudge Stick reaches, in pixels: the patch, and half its stroke
/// either way.
pub fn smudge_stick_reach(length: u32) -> u32 {
    let length = length.clamp(*SMUDGE_LENGTH.start(), *SMUDGE_LENGTH.end()) as f32;
    let patch = (SMUDGE_PATCH + length * SMUDGE_PATCH_PER_STEP).round();
    (patch + (SMUDGE_STROKE + length * SMUDGE_STROKE_PER_STEP) / 2.0).ceil() as u32 + 1
}

/// How big the sponge's blotches are, in pixels of blur on the noise they are
/// cut from: a floor and a step of Brush Size.
const SPONGE_BLOTCH: f32 = 2.0;
const SPONGE_BLOTCH_PER_STEP: f32 = 0.6;

/// How hard the blotches are cut, as a gain on the noise before `tanh`: high
/// at Smoothness 1, where their edges are crisp, and lower as it rises.
const SPONGE_CUT: f32 = 3.0;
const SPONGE_CUT_PER_STEP: f32 = 0.12;

/// How many levels lighter or darker a blotch is per step of Definition.
const SPONGE_LEVELS: f32 = 1.4;

/// How much the picture under the blotches is settled: a median, whose reach
/// grows with Brush Size, then a blur that grows with Smoothness.
const SPONGE_SETTLE: f32 = 1.0;
const SPONGE_SETTLE_PER_STEP: f32 = 0.2;
const SPONGE_SOFTEN_PER_STEP: f32 = 0.12;

/// Filter ▸ Artistic ▸ Sponge: the picture dabbed on with a sponge.
///
/// The picture is settled into soft patches, and a pattern of blotches is
/// laid over it, each a little lighter or a little darker than what is under
/// it — the open and closed cells of the sponge. The blotches are smooth
/// noise cut hard, so they come out as rounded, irregular spots rather than
/// as grain.
///
/// **Brush Size** is how big the blotches are. **Definition** is how much
/// lighter or darker they are. **Smoothness** is how soft their edges are,
/// and how soft the picture under them.
///
/// Alpha is left alone.
///
/// No GPU path. The work is a median, which is sequential along each row, and
/// the blurs, which already go through the backend.
pub fn sponge(pixmap: &mut Pixmap, size: u32, definition: u32, smoothness: u32) {
    if pixmap.is_empty() {
        return;
    }
    let size = size.clamp(*SPONGE_SIZE.start(), *SPONGE_SIZE.end()) as f32;
    let definition = definition.clamp(*SPONGE_DEFINITION.start(), *SPONGE_DEFINITION.end()) as f32;
    let smoothness = smoothness.clamp(*SPONGE_SMOOTHNESS.start(), *SPONGE_SMOOTHNESS.end()) as f32;

    let settle = (SPONGE_SETTLE + size * SPONGE_SETTLE_PER_STEP).round() as u32;
    let mut settled = crate::filters::convolve::median_of(pixmap, settle, settle);
    crate::filters::convolve::gaussian_blur_accelerated(&mut settled, smoothness * SPONGE_SOFTEN_PER_STEP);

    let scale = SPONGE_BLOTCH + size * SPONGE_BLOTCH_PER_STEP;
    let blotches = blurred_specks(pixmap.width(), pixmap.height(), scale, 31);
    let spread = speck_gain(scale);
    let cut = (SPONGE_CUT - (smoothness - 1.0) * SPONGE_CUT_PER_STEP).max(0.5);
    let levels = definition * SPONGE_LEVELS;

    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .zip(settled.as_bytes().par_chunks_exact(stride))
        .zip(blotches.as_bytes().par_chunks_exact(stride))
        .for_each(|((out, settled), blotches)| {
            for ((px, from), b) in out
                .chunks_exact_mut(4)
                .zip(settled.chunks_exact(4))
                .zip(blotches.chunks_exact(4))
            {
                let cell = ((b[0] as f32 - 127.5) * spread * cut).tanh() * levels;
                for c in 0..3 {
                    px[c] = (from[c] as f32 + cell).clamp(0.0, 255.0).round() as u8;
                }
                // Alpha stands: dabbing the picture does not change the
                // layer's shape.
            }
        });
}

/// How far the underpainting is settled, per step of Brush Size: a median
/// reach, which gives the soft patches, and a blur, which washes them
/// together.
const UNDERPAINT_PATCH_PER_STEP: f32 = 0.6;
const UNDERPAINT_WASH_PER_STEP: f32 = 0.8;

/// How far the texture's slope bends where the paint is read from, per step
/// of Texture Coverage, and how far the surface is softened first so a hard
/// texture bends it over a pixel or two rather than in a single jump.
const UNDERPAINT_PUSH_PER_STEP: f32 = 3.5;
const UNDERPAINT_BEVEL: f32 = 0.7;

/// How much Relief each step of Texture Coverage adds.
const UNDERPAINT_RELIEF_PER_STEP: f32 = 1.0;

/// How much less the surface shows in the light than in the dark. See
/// [`crate::filters::texture::apply_relief_weighted`].
const UNDERPAINT_DARK_BIAS: f32 = 0.4;

/// How hard the surface glints: see
/// [`crate::filters::texture::Finish::crisp`].
const UNDERPAINT_CRISP: f32 = 0.75;

/// The shadow in the surface's low parts, and how much of the surface fades
/// out in broad soft patches. See [`crate::filters::texture::Finish`].
const UNDERPAINT_OCCLUSION: f32 = 70.0;
const UNDERPAINT_PATCHY: f32 = 0.7;

/// Filter ▸ Artistic ▸ Underpainting: the picture laid in broadly on a
/// textured surface, as the first coat of a painting is.
///
/// 1. **Brush Size** settles the picture into soft washes of colour.
/// 2. **Texture Coverage** is how much the surface shows through: the paint is
///    bent about by the texture's slope, so edges take on its pattern, and
///    the surface stands deeper.
/// 3. The surface is lit — see [`crate::filters::texture::apply_relief`] for
///    Texture, Scaling, Relief, Light and Invert.
///
/// Alpha is left alone.
///
/// No GPU path. The settling is a median, which is sequential along each row,
/// and every later stage wants the one before back on the CPU.
#[allow(clippy::too_many_arguments)]
pub fn underpainting(
    pixmap: &mut Pixmap,
    size: u32,
    coverage: u32,
    texture: crate::filters::texture::Texture,
    scaling: u32,
    relief: u32,
    light: crate::filters::texture::Light,
    invert: bool,
) {
    if pixmap.is_empty() {
        return;
    }
    let size = size.clamp(*UNDERPAINT_SIZE.start(), *UNDERPAINT_SIZE.end()) as f32;
    let coverage = coverage.clamp(*UNDERPAINT_COVERAGE.start(), *UNDERPAINT_COVERAGE.end()) as f32;
    let (width, height) = (pixmap.width() as usize, pixmap.height() as usize);

    let patch = (size * UNDERPAINT_PATCH_PER_STEP).round() as u32;
    let mut paint = if patch > 0 {
        crate::filters::convolve::median_of(pixmap, patch, patch)
    } else {
        pixmap.clone()
    };
    for (p, o) in paint.as_bytes_mut().chunks_exact_mut(4).zip(pixmap.as_bytes().chunks_exact(4)) {
        p[3] = o[3];
    }
    crate::filters::convolve::gaussian_blur_accelerated(&mut paint, size * UNDERPAINT_WASH_PER_STEP);

    let mut surface = crate::filters::texture::height_map(texture, pixmap.width(), pixmap.height(), scaling);
    if invert {
        surface.par_iter_mut().for_each(|h| *h = 1.0 - *h);
    }
    blur_field(&mut surface, width, height, UNDERPAINT_BEVEL);
    let push = coverage * UNDERPAINT_PUSH_PER_STEP;
    let (lx, ly) = light.towards();
    let (paint, surface) = (&paint, &surface);
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, out)| {
            let (up, down) = (y.saturating_sub(1), (y + 1).min(height - 1));
            for (x, px) in out.chunks_exact_mut(4).enumerate() {
                // Read the paint from where the surface's slope bends it, as
                // light through rippled glass: flat surface, no change; the
                // side of a ridge, a jump. That is what cuts an edge into the
                // texture's own jagged pattern.
                let (left, right) = (x.saturating_sub(1), (x + 1).min(width - 1));
                let dx = (surface[y * width + right] - surface[y * width + left]) / 2.0;
                let dy = (surface[down * width + x] - surface[up * width + x]) / 2.0;
                // Along the light only: the paint slides down the slopes the
                // light shows, so with the light above, edges break into the
                // horizontal dashes CS6 gives and not into a grid of cells.
                let along = (dx * lx + dy * ly) * push
                    * crate::filters::texture::patchiness(x, y, UNDERPAINT_PATCHY);
                // Sampled between pixels, so the bend is smooth rather than a
                // pixel's jump.
                let fx = (x as f32 + lx * along).clamp(0.0, (width - 1) as f32);
                let fy = (y as f32 + ly * along).clamp(0.0, (height - 1) as f32);
                let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
                let (x1, y1) = ((x0 + 1).min(width - 1), (y0 + 1).min(height - 1));
                let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
                let (r0, r1) = (paint.row(y0 as u32), paint.row(y1 as u32));
                for c in 0..3 {
                    let top = r0[x0 * 4 + c] as f32 * (1.0 - tx) + r0[x1 * 4 + c] as f32 * tx;
                    let bottom = r1[x0 * 4 + c] as f32 * (1.0 - tx) + r1[x1 * 4 + c] as f32 * tx;
                    px[c] = (top * (1.0 - ty) + bottom * ty).round() as u8;
                }
                // Alpha stands: laying the picture in does not change the
                // layer's shape.
            }
        });

    // Coverage is surface as well as push: CS6's texture stands out plainly
    // at Relief 4 once the coverage is up.
    let relief = relief + (coverage * UNDERPAINT_RELIEF_PER_STEP).round() as u32;
    crate::filters::texture::apply_relief_weighted(
        pixmap,
        texture,
        scaling,
        relief,
        light,
        invert,
        crate::filters::texture::Finish {
            dark_bias: UNDERPAINT_DARK_BIAS,
            bevel: UNDERPAINT_BEVEL,
            crisp: UNDERPAINT_CRISP,
            occlusion: UNDERPAINT_OCCLUSION,
            patchy: UNDERPAINT_PATCHY,
            gain: 1.0,
        },
    );
}

/// How far the picture is washed into patches, in pixels of median reach:
/// the reach at Brush Detail 1, and how much each step of detail takes off.
const WATER_WASH: f32 = 3.6;
const WATER_WASH_PER_STEP: f32 = 0.2;

/// The smart blur that smooths the gradients inside a wash and keeps its
/// boundary: reach in pixels, and how many levels apart two tones may be and
/// still be the same wash.
const WATER_FLATTEN_REACH: u32 = 2;
const WATER_FLATTEN_LEVELS: u32 = 16;

/// How many pools of tone the picture is quantised into: at Brush Detail 1,
/// and how many more each step adds. Each pixel keeps its own colour and is
/// taken to its pool's brightness, so a pool is one flat wash of paint.
const WATER_POOLS: f32 = 8.0;
const WATER_POOLS_PER_STEP: f32 = 1.0;

/// How much of the pooled picture is laid over the smooth wash — the cutout
/// layer's opacity in the stack. All of it reads as a poster.
const WATER_POOL_MIX: f32 = 0.35;

/// The drawing under the paint: an edge finder over the pooled picture, and
/// how dark a line a unit of edge draws, multiplied in. The scale in pixels
/// it is found at keeps the line a pixel or two wide rather than a crisp
/// single-pixel trace.
const WATER_LINE_SCALE: f32 = 0.8;
const WATER_LINE: f32 = 0.0025;

/// Local contrast in the light, before the shadows: the scale in pixels a
/// pixel is compared against, and how far its difference is pushed. What
/// gives CS6's white spray its dark flecks and blue-grey pools.
const WATER_LOCAL_SCALE: f32 = 3.0;
const WATER_LOCAL: f32 = 7.0;

/// Where Shadow Intensity starts taking tones to black, as a fraction of
/// white: at 0, and how far each step raises it; and how soft the fall is.
/// At 5 it reaches the sea and the whole of it goes dark, as CS6's does.
const WATER_SHADOW_FROM: f32 = 0.12;
const WATER_SHADOW_PER_STEP: f32 = 0.085;
const WATER_SHADOW_SOFT: f32 = 0.1;

/// How much richer the colour is than the photograph's.
const WATER_SATURATION: f32 = 1.25;

/// How deep the colour is laid at any setting: watercolour is richer in the
/// dark than the photograph, as a gamma.
const WATER_DEPTH: f32 = 1.15;

/// The granulation of the pigment: levels at Texture 1 and per step, and the
/// size of a speck in pixels. Laid on before the wash, in brightness only, so
/// the washing turns it into a faint mottle inside the pools — laid on after,
/// it is noise and reads as film grain.
const WATER_GRAIN: f32 = 2.0;
const WATER_GRAIN_SCALE: f32 = 2.0;

/// What each step of Texture above 1 adds: a fine grit laid on at the end,
/// in levels, strongest in the dark and fading out in the light. Laid on
/// before the washing it is enlarged into blotches that cover the sky.
const WATER_GRIT_PER_STEP: f32 = 9.0;
const WATER_GRIT_SCALE: f32 = 0.7;

/// Filter ▸ Artistic ▸ Watercolor: the picture washed in with a wet brush.
///
/// The classic stack, in order:
///
/// 1. **Dry brush** — Dry Brush's own dabs, their size set by **Brush
///    Detail**: blotches with ragged edges, where a median would leave smooth
///    rounded shapes.
/// 2. **Smart blur** — the gradients inside each shape smoothed away, its
///    boundary kept.
/// 3. **Cutout** — the brightness pulled part of the way to a handful of
///    pools, each pixel keeping its colour.
/// 4. **Find edges** — the pools' boundaries drawn in as thin dark lines and
///    multiplied over the paint.
/// 5. **Shadow Intensity** takes the darker tones to black: at 1 only the
///    deepest shadows, by 5 everything below the midtones. **Texture** is how
///    much the pigment granulates.
///
/// Alpha is left alone.
///
/// No GPU path. The dabs and the smart blur are sequential along each row.
pub fn watercolor(pixmap: &mut Pixmap, detail: u32, shadow: u32, texture: u32) {
    if pixmap.is_empty() {
        return;
    }
    let detail = detail.clamp(*WATER_DETAIL.start(), *WATER_DETAIL.end()) as f32;
    let shadow = shadow.clamp(*WATER_SHADOW.start(), *WATER_SHADOW.end()) as f32;
    let texture = texture.clamp(*WATER_TEXTURE.start(), *WATER_TEXTURE.end()) as f32;
    let (width, height) = (pixmap.width() as usize, pixmap.height() as usize);
    let luma = |p: &[u8]| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;

    // Granulation, under everything else.
    let grain = blurred_specks(pixmap.width(), pixmap.height(), WATER_GRAIN_SCALE, 61);
    let grain_gain = WATER_GRAIN * speck_gain(WATER_GRAIN_SCALE);
    let grit = blurred_specks(pixmap.width(), pixmap.height(), WATER_GRIT_SCALE, 62);
    let grit_gain = (texture - 1.0) * WATER_GRIT_PER_STEP * speck_gain(WATER_GRIT_SCALE);
    let mut grained = pixmap.clone();
    grained
        .as_bytes_mut()
        .par_chunks_exact_mut(4)
        .zip(grain.as_bytes().par_chunks_exact(4))
        .for_each(|(px, g)| {
            let speck = (g[0] as f32 - 127.5) * grain_gain;
            for c in 0..3 {
                px[c] = (px[c] as f32 + speck).round().clamp(0.0, 255.0) as u8;
            }
        });

    // 1 and 2: dry brush, smart blur.
    let reach = (WATER_WASH - (detail - 1.0) * WATER_WASH_PER_STEP).round().max(1.0) as u32;
    // Dabs, not a median: a median leaves smooth, rounded shapes, where a
    // brush leaves blotches with ragged edges — Dry Brush's own dabs.
    let mut wash = grained;
    paint_in_dabs(&mut wash, reach, true);
    crate::filters::convolve::surface_blur(&mut wash, WATER_FLATTEN_REACH, WATER_FLATTEN_LEVELS);

    // 3: cutout — pools of tone, then their edges rounded.
    let steps = (WATER_POOLS + (detail - 1.0) * WATER_POOLS_PER_STEP - 1.0).max(1.0);
    wash.as_bytes_mut().par_chunks_exact_mut(4).for_each(|px| {
        let level = luma(px);
        let pool = (level / 255.0 * steps).round() / steps * 255.0;
        // Less in the light: a pale wash is thin and has no hard pools, and a
        // clear sky banded into contours reads as a map.
        let mix = WATER_POOL_MIX * (1.0 - level / 255.0);
        let scale = 1.0 + (pool / level.max(1.0) - 1.0) * mix;
        for c in 0..3 {
            px[c] = (px[c] as f32 * scale).round().clamp(0.0, 255.0) as u8;
        }
    });

    // 4: find edges, on the pools' brightness.
    let mut tone: Vec<f32> = wash.as_bytes().par_chunks_exact(4).map(luma).collect();
    blur_field(&mut tone, width, height, WATER_LINE_SCALE);
    let mut around = tone.clone();
    blur_field(&mut around, width, height, WATER_LOCAL_SCALE);
    let (tone, around) = (&tone, &around);

    let from = WATER_SHADOW_FROM + shadow * WATER_SHADOW_PER_STEP;
    let depth: [f32; 256] = std::array::from_fn(|v| 255.0 * (v as f32 / 255.0).powf(WATER_DEPTH));

    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .zip(wash.as_bytes().par_chunks_exact(stride))
        .zip(grit.as_bytes().par_chunks_exact(stride))
        .enumerate()
        .for_each(|(y, ((out, wash), grit))| {
            let (up, down) = (y.saturating_sub(1), (y + 1).min(height - 1));
            for (x, ((px, w), g)) in out
                .chunks_exact_mut(4)
                .zip(wash.chunks_exact(4))
                .zip(grit.chunks_exact(4))
                .enumerate()
            {
                let (left, right) = (x.saturating_sub(1), (x + 1).min(width - 1));
                let at = |xx: usize, yy: usize| tone[yy * width + xx];
                // Sobel.
                let gx = at(right, up) + 2.0 * at(right, y) + at(right, down)
                    - at(left, up) - 2.0 * at(left, y) - at(left, down);
                let gy = at(left, down) + 2.0 * at(x, down) + at(right, down)
                    - at(left, up) - 2.0 * at(x, up) - at(right, up);
                let line = 1.0 - ((gx * gx + gy * gy).sqrt() * WATER_LINE).min(1.0);

                let mut paint = [0.0f32; 3];
                for c in 0..3 {
                    paint[c] = depth[w[c] as usize] * line;
                }
                // Local contrast in the light: a speck of spray a little
                // darker than the white round it is pulled down further, so
                // it is caught by the shadows and dries as a dark fleck.
                let i = y * width + x;
                let pale = around[i] / 255.0;
                let lift = (tone[i] - around[i]) * WATER_LOCAL * pale * pale;
                let grey = 0.299 * paint[0] + 0.587 * paint[1] + 0.114 * paint[2];
                let lifted = (grey + lift).max(0.0);
                let ratio = lifted / grey.max(1.0);
                for v in paint.iter_mut() {
                    *v = (lifted + (*v * ratio - lifted) * WATER_SATURATION).clamp(0.0, 255.0);
                }
                let grey = lifted.min(255.0);
                let t = ((grey / 255.0 - (from - WATER_SHADOW_SOFT)) / (2.0 * WATER_SHADOW_SOFT))
                    .clamp(0.0, 1.0);
                let kept = t * t * (3.0 - 2.0 * t);
                let speck = (g[0] as f32 - 127.5) * grit_gain * (1.0 - grey / 255.0);
                for c in 0..3 {
                    px[c] = (paint[c] * kept + speck).round().clamp(0.0, 255.0) as u8;
                }
                // Alpha stands: washing the picture in does not change the
                // layer's shape.
            }
        });
}

/// How big one stroke of the knife is, as slack given to the segmentation per
/// square pixel of Stroke Size.
///
/// **This filter works on regions, not on windows, and that is the whole of
/// it.** Everything else in this module looks at a fixed neighbourhood around
/// each pixel, so a petal and a blade of grass are the same size of thing to
/// it. Adobe's own description of Palette Knife is not that: it segments the
/// picture into contiguous areas of like colour, holds the boundaries between
/// them, and flattens what is inside each one. So a petal that happens to be
/// one colour across four hundred pixels comes back as *one* flat mass, while
/// the busy grass behind it comes back as a dozen — from a single uniform pass,
/// which is the thing about the reference that no window operation explains.
///
/// It is a spacing, in pixels per step of Stroke Size: see
/// [`segment::regions`](crate::filters::segment::regions), where it sets how
/// far apart the regions start. Small — smaller than the masses that come back
/// — because the regions are not the masses. They are cut, and then the palette
/// below fuses them into masses; a boundary in the reference is the edge of a
/// run of regions that happened to round the same way, which is why it wanders
/// and steps about instead of curving the way one region's edge would.
const KNIFE_WIDTH: f32 = 1.0;

/// How tightly a region is held to its own patch of canvas.
///
/// This is `compactness` in [`segment::regions`](crate::filters::segment::regions):
/// how much being near counts against being alike. Low, so the regions follow
/// what is in the picture rather than tiling it — what makes the masses coarse
/// is the palette, not the shape of the regions underneath.
const KNIFE_HOLD: f32 = 12.0;

/// How coarse the palette is, in levels per channel, and how much coarser a
/// step down in Stroke Detail makes it.
///
/// **The colours are compressed, and this is the part that took longest to
/// see.** The reference is not a photograph reduced to averages — it is a
/// photograph reduced to a *handful of colours*, half a dozen greens and four
/// or five pinks, each sitting flat over a large area with a hard, ragged edge
/// against the next. Regions alone never give that: they give a hundred
/// slightly different greens and boundaries no eye can find. Rounding what each
/// region came to onto a coarse palette is what fuses them.
///
/// Stroke Detail is how sensitive the knife is to the smaller colour breaks
/// inside a mass, which is exactly a count of rungs: wound up the palette is
/// fine and a petal keeps its shading, wound down it is coarse and the whole
/// flower goes over to two or three pinks.
const KNIFE_PALETTE: u32 = 16;
const KNIFE_DETAIL_PER_STEP: f32 = 0.6;

/// How far the picture is settled before the cut, in pixels per step of Stroke
/// Size.
///
/// Two jobs, and the second is the one that sets the size. A photograph's grain
/// is the enemy of a segmentation in a way it is not of a blur — two pixels of
/// the same petal a few levels apart will be pulled into different regions —
/// and a little blur fixes that at any radius.
///
/// The size comes from the dark ring around the flowers, which drove four wrong
/// mechanisms before this one. Blurring widens the step from pink to near-black
/// green into a band of the colours in between, and the palette then rounds the
/// middle of that band onto one rung: a flat plum ring with a hard edge on both
/// sides, following the outline all the way round. How wide the blur is, is how
/// wide the ring is. Nothing looks for the outline and nothing draws it.
///
/// Mostly floor, and barely rising with Stroke Size, because the ring in the
/// reference is about as wide at the top of the slider as at the middle — what
/// a wide stroke widens is the *masses*, which is the spacing's business. Made
/// proportional it swallows the flower: a blur wide enough to matter at Stroke
/// Size 50 pulls the background into the petals and the whole picture comes
/// back dull and fattened.
const KNIFE_SETTLE_FLOOR: f32 = 1.0;
const KNIFE_SETTLE: f32 = 0.03;

/// How much of the picture's small change is thrown away before the cut, in
/// pixels of median per step of Stroke Size.
///
/// This is where Stroke Size gets its *abstraction* from, and it has to be a
/// median rather than more blur for the reason above: at the top of the slider
/// the reference has lost the veins of a petal and the individual blades of
/// grass entirely, but the flower is still exactly the shape and the colour it
/// was. A median throws away whatever is narrower than its window and leaves
/// everything else where it stood; a blur wide enough to do the same would
/// drag the background into the flower.
const KNIFE_SIMPLIFY: f32 = 0.2;

/// How much longer than wide one stroke is.
///
/// A knife is a blade, and what a blade leaves is longer than it is wide —
/// every stroke in a real palette-knife painting is a slab dragged in one
/// direction, not a dab. Round masses come back reading as cobbles however
/// well their colours are judged, which is what this was added to fix.
///
/// Which way each one runs is worked out from the picture rather than chosen:
/// see [`segment::regions`](crate::filters::segment::regions).
const KNIFE_STROKE: f32 = 2.2;

/// How many coats go on, and how much finer each is than the one under it.
///
/// **Oil laid on with a knife is built up, not laid down.** A painting of one
/// is a broad coat that covers the canvas and gets the masses right, and then
/// smaller strokes worked into the parts of it that had something to say —
/// the poppies over the field, the light on the water. One coat, however well
/// cut, gives a picture where a petal and a whole hillside are the same size of
/// thing, because they were both painted at the same size of stroke.
///
/// Two coats, because the third adds work and very little paint: by then what
/// the second missed is a pixel here and there. Where each coat goes is
/// [`where_it_matters`].
const COATS: usize = 2;
const KNIFE_FINER: f32 = 0.45;

/// Under a pixel, to take the stairs off a torn edge without softening the
/// tear. See where it is used for why a torn edge has stairs at all.
const KNIFE_NO_STAIRS: f32 = 0.7;

/// How far a boundary is allowed to wander off where it really is, in pixels
/// per step of Stroke Size.
///
/// Without this the masses come back with smooth, rounded, curving edges and
/// the result reads as a median filter rather than as a knife — which is
/// exactly the note this constant was added on. The reference's edges step and
/// kink; a knife is a straight blade dragged through wet paint and it leaves a
/// torn edge, not a drawn one. See
/// [`segment::flatten`](crate::filters::segment::flatten) for how little it
/// costs: displacing where a pixel reads its colour from changes nothing inside
/// a flat mass and everything at its edge.
const KNIFE_RAGGED: f32 = 0.12;

/// How far each step of Softness carries, in pixels per step of Stroke Size.
///
/// Nothing at the bottom of the slider: the joins in the reference at Softness
/// 0 are *hard*, and ragged, and that is most of what makes it look scraped on
/// rather than painted. This only eases them.
const KNIFE_SOFTNESS_SCALE: f32 = 0.03;

/// How thickly the paint stands, how far its edge falls away, how much the
/// load varies from one stroke to the next, and where the light comes from.
///
/// **This is what makes it a palette knife and not a poster.** A real knife
/// painting is not flat colour: every stroke is a slab of paint standing a
/// millimetre or two off the canvas, so it has a lit edge on one side, a shadow
/// on the other, and a ridge of surplus paint where the blade lifted. Take the
/// relief away and the same picture reads as printed rather than laid on, which
/// is the note this pass was added on.
///
/// The light comes from the upper left because that is where it comes from in
/// every painting of one: it is the convention a viewer reads relief by, and
/// lighting from below makes the strokes read as dents instead.
///
/// The shoulder is a fraction of the stroke rather than a fixed distance —
/// a wide stroke carries more paint and its edge falls away further — and the
/// load is what stops the surface reading as machined: a knife picks up a
/// different amount of paint every time it goes back to the palette.
const PAINT_THICK: f32 = 0.16;
const PAINT_SHOULDER: f32 = 0.3;
const PAINT_LOAD: f32 = 0.5;
const PAINT_LIGHT: (f32, f32) = (-0.7, -0.7);

/// How deep the ridges the blade drags through a stroke are, and how far apart.
///
/// The pitch is a fraction of the stroke's width, so a wide stroke gets a few
/// broad ridges rather than a wide stroke's worth of fine ones. They run
/// *along* the stroke, which is why the orientation of each mass has to be
/// worked out before they can be laid: ridges running the wrong way across a
/// stroke read as corrugation, not as paint.
const PAINT_DRAG: f32 = 0.15;
const PAINT_DRAG_PITCH: f32 = 0.5;

/// How different a coat has to have left the picture before another one is
/// worked into it, in levels, and how different before it is worked in fully.
///
/// Low enough that anything the broad coat plainly lost gets gone back over,
/// high enough that the whole canvas does not — a second coat laid everywhere
/// buries the first and there was no point laying it.
const PAINT_MISS_LOW: f32 = 10.0;
const PAINT_MISS_HIGH: f32 = 17.0;

/// Three box passes make a good enough Gaussian, and this one is over a height
/// field that nothing but the lighting will ever see.
fn soften(field: &mut [f32], width: usize, height: usize, radius: usize) {
    if radius == 0 {
        return;
    }
    let mut scratch = vec![0.0f32; field.len()];
    for _ in 0..3 {
        // Across, then down. A box blur is separable for the same reason a
        // Gaussian is.
        scratch.copy_from_slice(field);
        field
            .par_chunks_exact_mut(width)
            .enumerate()
            .for_each(|(y, line)| {
                let row = &scratch[y * width..(y + 1) * width];
                for (x, slot) in line.iter_mut().enumerate() {
                    let from = x.saturating_sub(radius);
                    let to = (x + radius + 1).min(width);
                    *slot = row[from..to].iter().sum::<f32>() / (to - from) as f32;
                }
            });
        scratch.copy_from_slice(field);
        field
            .par_chunks_exact_mut(width)
            .enumerate()
            .for_each(|(y, line)| {
                let from = y.saturating_sub(radius);
                let to = (y + radius + 1).min(height);
                for (x, slot) in line.iter_mut().enumerate() {
                    let mut total = 0.0;
                    for row in from..to {
                        total += scratch[row * width + x];
                    }
                    *slot = total / (to - from) as f32;
                }
            });
    }
}

/// Smooth noise along one number, for the ridges the blade drags.
fn furrow(t: f32, salt: u32) -> f32 {
    let cell = t.floor();
    let step = t - cell;
    let ease = step * step * (3.0 - 2.0 * step);
    let at = |c: f32| noise(c as i32, salt as i32) * 2.0 - 1.0;
    at(cell) + (at(cell + 1.0) - at(cell)) * ease
}

/// Stand the paint off the canvas and light it.
///
/// Every mass becomes a slab: thickest in the middle, falling away to nothing
/// at its edge, standing a little higher or lower than its neighbours depending
/// on how much paint the knife had on it, and furrowed along its own length by
/// the blade. Then the whole surface is lit from the upper left, which is the
/// only step that touches the colours — everything above it is building a
/// height field that is never itself seen.
///
/// The orientation of each mass comes from its own second moments. A knife
/// stroke is longer than it is wide and the drag runs the long way; measuring
/// it is cheaper and steadier than guessing at the picture's own direction,
/// because the masses are already the shape the strokes are.
fn slab_of_paint(
    labels: &[u32],
    count: usize,
    width: usize,
    height: usize,
    stroke: f32,
) -> Vec<f32> {
    if count == 0 {
        return vec![0.0; width * height];
    }

    // Where each mass lies and which way it runs, from its own moments.
    let mut tally = vec![0f64; count];
    let mut sums = vec![[0f64; 5]; count];
    for (p, &label) in labels.iter().enumerate() {
        let k = label as usize;
        let (x, y) = ((p % width) as f64, (p / width) as f64);
        tally[k] += 1.0;
        sums[k][0] += x;
        sums[k][1] += y;
        sums[k][2] += x * x;
        sums[k][3] += x * y;
        sums[k][4] += y * y;
    }
    let lie: Vec<(f32, f32, f32)> = (0..count)
        .map(|k| {
            let n = tally[k].max(1.0);
            let (mx, my) = (sums[k][0] / n, sums[k][1] / n);
            let xx = sums[k][2] / n - mx * mx;
            let xy = sums[k][3] / n - mx * my;
            let yy = sums[k][4] / n - my * my;
            // The long axis of the mass. A round mass has no long axis and the
            // angle it gives is arbitrary, which is harmless: the furrows have
            // to run *some* way and on a round mass no way is wrong.
            let angle = 0.5 * (2.0 * xy).atan2(xx - yy);
            let load = noise(k as i32, 0x5bd1) - 0.5;
            (angle.cos() as f32, angle.sin() as f32, load)
        })
        .collect();

    // A slab per mass: full thickness inside, falling away at the edge. Built
    // as an interior mask and then softened, which costs one blur and needs no
    // distance transform.
    let mut paint = vec![0f32; width * height];
    paint
        .par_chunks_exact_mut(width)
        .enumerate()
        .for_each(|(y, line)| {
            for (x, slot) in line.iter_mut().enumerate() {
                let p = y * width + x;
                let mine = labels[p];
                let same = (x == 0 || labels[p - 1] == mine)
                    && (x + 1 == width || labels[p + 1] == mine)
                    && (y == 0 || labels[p - width] == mine)
                    && (y + 1 == height || labels[p + width] == mine);
                *slot = if same { 1.0 } else { 0.0 };
            }
        });
    soften(
        &mut paint,
        width,
        height,
        ((stroke * PAINT_SHOULDER).round() as usize).max(1),
    );

    // Then the load each stroke was carrying, and the furrows the blade left
    // along it. Both are held down at the stroke's edge by the slab itself —
    // there is no paint out there to stand proud or to be dragged.
    let pitch = (stroke * PAINT_DRAG_PITCH).max(1.0);
    paint
        .par_chunks_exact_mut(width)
        .enumerate()
        .for_each(|(y, line)| {
            for (x, slot) in line.iter_mut().enumerate() {
                let k = labels[y * width + x] as usize;
                let (across, along, load) = lie[k];
                // Measured across the stroke, so the furrows run along it.
                let over = (x as f32 * -along + y as f32 * across) / pitch;
                let drag = furrow(over, k as u32 & 0xffff);
                *slot *= 1.0 + load * PAINT_LOAD;
                *slot += drag * PAINT_DRAG * *slot;
            }
        });
    soften(&mut paint, width, height, 1);
    paint
}

/// Where the coat that has just gone on missed the picture, and so where
/// another one is worth laying.
///
/// This is how a painter works and it is the point of laying the paint in more
/// than one coat: the first is a broad one that covers the canvas and gets the
/// big shapes down, and it is *wrong* in the places where the picture had
/// something small to say. Those are the places that get worked into, and the
/// broad coat is left standing everywhere else. Laying the second coat
/// everywhere instead gives back the fine cut on its own, with the coarse one
/// buried and nothing gained from having laid it.
fn where_it_matters(subject: &Pixmap, canvas: &Pixmap, spread: f32) -> Vec<f32> {
    let width = subject.width() as usize;
    let height = subject.height() as usize;
    let (was, is) = (subject.as_bytes(), canvas.as_bytes());
    let mut missed = vec![0f32; width * height];
    missed
        .par_chunks_exact_mut(width)
        .enumerate()
        .for_each(|(y, line)| {
            for (x, slot) in line.iter_mut().enumerate() {
                let i = (y * width + x) * 4;
                *slot = (0..3)
                    .map(|c| (was[i + c] as i32 - is[i + c] as i32).abs())
                    .max()
                    .unwrap_or(0) as f32;
            }
        });
    // Over an area rather than a pixel: a stroke is worked into or it is not,
    // and half of one cannot be.
    soften(
        &mut missed,
        width,
        height,
        ((spread * 0.25).round() as usize).max(1),
    );
    for v in missed.iter_mut() {
        *v = ((*v - PAINT_MISS_LOW) / (PAINT_MISS_HIGH - PAINT_MISS_LOW)).clamp(0.0, 1.0);
    }
    missed
}

/// Light the paint. The only step in the whole filter that turns a height back
/// into a colour; everything before it was building the height.
fn light_the_paint(pixmap: &mut Pixmap, depth: &[f32], lift: f32) {
    if lift <= 0.0 {
        return;
    }
    let width = pixmap.width() as usize;
    let height = pixmap.height() as usize;
    let (lx, ly) = PAINT_LIGHT;
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(width * 4)
        .enumerate()
        .for_each(|(y, line)| {
            for (x, chunk) in line.chunks_exact_mut(4).enumerate() {
                let at = |dx: usize, dy: usize| depth[dy * width + dx];
                let dx = at((x + 1).min(width - 1), y) - at(x.saturating_sub(1), y);
                let dy = at(x, (y + 1).min(height - 1)) - at(x, y.saturating_sub(1));
                let shade = (1.0 + (dx * lx + dy * ly) * lift).clamp(0.7, 1.35);
                for c in 0..3 {
                    chunk[c] = (chunk[c] as f32 * shade).clamp(0.0, 255.0) as u8;
                }
            }
        });
}

/// Filter ▸ Artistic ▸ Palette Knife: the picture spread with a knife.
///
/// The picture is settled, cut into contiguous areas of like colour, and each
/// area filled with its own average rounded onto a coarse palette. Both halves
/// are needed and neither is enough: the cut is what makes the areas follow
/// what is in the picture, and the palette is what fuses them into a few flat
/// masses with the hard ragged edges between them that a knife leaves.
///
/// * **Stroke Size** is how big a mass of colour the knife works in. It sets
///   both the spacing of the cut ([`KNIFE_WIDTH`]) and how far the picture is
///   settled first ([`KNIFE_SETTLE`]), which is what makes a wide stroke lose
///   the small things entirely rather than merely enlarging them.
/// * **Stroke Detail** is how sensitive the knife is to the smaller colour
///   breaks inside a mass — the number of rungs on the palette, see
///   [`KNIFE_PALETTE`]. Wound down, a whole flower goes over to two or three
///   pinks.
/// * **Softness** is the blade's edge, which eases the joins between one mass
///   and the next. At 0 they are left hard, as the reference has them.
///
/// Alpha is left alone: spreading the picture does not change the layer's
/// shape.
///
/// No GPU path, and there will not be one: the cut is a sequential walk over
/// the joins between pixels in order, each step depending on every step before
/// it — the same argument as flood fill. See
/// [`segment`](crate::filters::segment).
pub fn palette_knife(pixmap: &mut Pixmap, size: u32, detail: u32, softness: u32) {
    if pixmap.is_empty() {
        return;
    }
    let size = size.clamp(*KNIFE_SIZE.start(), *KNIFE_SIZE.end());
    let detail = detail.clamp(*KNIFE_DETAIL.start(), *KNIFE_DETAIL.end());
    let softness = softness.clamp(*KNIFE_SOFTNESS.start(), *KNIFE_SOFTNESS.end());

    // Settle the picture before deciding what belongs with what — and paint
    // back from the settled picture too, not from the original. The band of
    // in-between colours this lays along every strong edge is not an artefact
    // to be tolerated; it is where the ring comes from. See KNIFE_SETTLE.
    crate::filters::convolve::median_filter(
        pixmap,
        ((size as f32 * KNIFE_SIMPLIFY).round() as u32).max(1),
    );
    crate::filters::convolve::gaussian_blur_accelerated(
        pixmap,
        KNIFE_SETTLE_FLOOR + size as f32 * KNIFE_SETTLE,
    );

    let rungs = (KNIFE_PALETTE as f32
        / (1.0 + (*KNIFE_DETAIL.end() - detail) as f32 * KNIFE_DETAIL_PER_STEP))
        .round()
        .max(2.0) as u32;
    let width = pixmap.width() as usize;
    let height = pixmap.height() as usize;

    // What is being painted *from* stays as it was settled. Every coat is cut
    // from it and coloured from it, so a coat laid over another is a fresh
    // reading of the picture rather than a reading of the paint already down —
    // which is what a painter does, and is also the only way the second coat
    // can put back what the first one lost.
    let subject = pixmap.clone();
    let mut depth = vec![0f32; width * height];
    let broad = size as f32 * KNIFE_WIDTH;

    for coat in 0..COATS {
        let stroke = broad * KNIFE_FINER.powi(coat as i32);
        let (labels, count) =
            crate::filters::segment::regions(&subject, stroke, KNIFE_HOLD, KNIFE_STROKE);

        let mut wet = subject.clone();
        crate::filters::segment::flatten(
            &mut wet,
            &labels,
            count,
            rungs,
            stroke * KNIFE_RAGGED / KNIFE_WIDTH,
        );

        // The first coat covers the canvas; every one after it goes on only
        // where the one before missed.
        let worked = if coat == 0 {
            vec![1.0f32; width * height]
        } else {
            where_it_matters(&subject, pixmap, stroke)
        };

        let fresh = wet.as_bytes().to_vec();
        pixmap
            .as_bytes_mut()
            .par_chunks_exact_mut(width * 4)
            .enumerate()
            .for_each(|(y, line)| {
                for (x, chunk) in line.chunks_exact_mut(4).enumerate() {
                    let p = y * width + x;
                    let over = worked[p];
                    for c in 0..3 {
                        let under = chunk[c] as f32;
                        chunk[c] = (under + (fresh[p * 4 + c] as f32 - under) * over) as u8;
                    }
                }
            });

        // And the paint stacks up where it went on. A finer stroke carries
        // less paint than a broad one, so it stands proportionally less proud
        // — otherwise the accents shout over the coat they were laid on.
        let slab = slab_of_paint(&labels, count, width, height, stroke);
        let carried = stroke / broad;
        depth
            .par_iter_mut()
            .zip(slab.par_iter().zip(worked.par_iter()))
            .for_each(|(total, (&this, &over))| *total += this * over * carried);
    }

    // Take the stairs off the edges. Displacing where a pixel reads its colour
    // from is done in whole pixels, so a torn boundary comes back climbing in
    // single-pixel steps — ragged at arm's length and *pixelated* up close,
    // which is the note this was added on. Under a pixel of blur reads as a
    // torn edge rather than as a stepped one and costs nothing else: there is
    // nothing this small anywhere else in the picture by now.
    crate::filters::convolve::gaussian_blur(pixmap, KNIFE_NO_STAIRS);

    if softness > 0 {
        crate::filters::convolve::gaussian_blur_accelerated(
            pixmap,
            size as f32 * softness as f32 * KNIFE_SOFTNESS_SCALE,
        );
    }

    // Last, because it is the only pass that is about the paint rather than
    // about the picture. Softness thins the paint as well as easing the joins:
    // a stroke laid on thin has no edge to catch the light.
    let left = 1.0 - softness as f32 / *KNIFE_SOFTNESS.end() as f32;
    light_the_paint(pixmap, &depth, left * PAINT_THICK * broad);
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

    /// A ramp — where every step was the same size as the last — comes back as
    /// masses: long stretches that barely climb at all, and hard joins between
    /// them.
    ///
    /// Not as *runs of one colour*, which is what this measured before the
    /// paint was given any thickness. One mass is one colour of pigment, but
    /// the paint it is made of is a slab with a lit side and a shaded one (see
    /// [`lay_the_paint_on`]), so the values inside it drift by a level or two.
    /// What survives the lighting is the shape of the climb: flat-ish, then a
    /// step, then flat-ish again.
    #[test]
    fn the_knife_lays_a_smooth_ramp_on_in_masses() {
        let mut pm = ramp();
        let steps = |pm: &Pixmap| -> Vec<i32> {
            (0..63)
                .map(|x| (pm.get(x + 1, 32).r as i32 - pm.get(x, 32).r as i32).abs())
                .collect()
        };
        // The ramp climbs by 4 a pixel, everywhere, with no joins in it.
        assert!(steps(&pm).iter().all(|&d| d == 4));

        palette_knife(&mut pm, 25, 3, 0);
        let after = steps(&pm);
        // Afterwards the climb is not spread evenly: most of it happens at a
        // few hard joins. Measured against the total rather than as a count of
        // flat neighbours, because the lighting puts a slope on every mass and
        // a count of flat ones counts the lighting instead of the paint.
        let joins: i32 = after.iter().filter(|&&d| d > 12).sum();
        let total: i32 = after.iter().sum();
        assert!(
            joins * 2 > total,
            "the ramp came back climbing evenly: {joins} of {total} levels crossed at a join"
        );
        let hardest = *after.iter().max().unwrap();
        assert!(
            hardest > 20,
            "the masses met at a step of only {hardest} levels"
        );
    }

    /// A wide knife works in bigger masses of colour than a fine one, and so
    /// leaves the picture further from where it started.
    ///
    /// Measured as how far it strays rather than as a count of masses, of
    /// colours or of joins, all three of which were tried. Colours count the
    /// lighting now that the paint has thickness. Joins count nothing at all at
    /// the fine end: a knife this small follows a ramp so closely that it
    /// leaves no hard join anywhere, and the honest comparison came out 14
    /// against 0 the wrong way round.
    #[test]
    fn a_wider_knife_strays_further_from_the_picture() {
        let strayed = |size| {
            let before = ramp();
            let mut pm = before.clone();
            palette_knife(&mut pm, size, 3, 0);
            (0..64)
                .map(|y| {
                    (0..64)
                        .map(|x| (pm.get(x, y).r as i32 - before.get(x, y).r as i32).abs() as u32)
                        .sum::<u32>()
                })
                .sum::<u32>()
        };
        assert!(
            strayed(50) > strayed(2),
            "a wide knife stayed as close to the picture as a fine one: {} against {}",
            strayed(50),
            strayed(2)
        );
    }

    /// The paint stands off the canvas. A field of one flat colour has nothing
    /// in it to find and still comes back with light and shade in it, because
    /// what is being lit is the paint and not the picture.
    ///
    /// This is the difference between a knife painting and a poster, and it is
    /// the one thing no amount of work on the *colours* was ever going to give.
    #[test]
    fn the_paint_stands_off_the_canvas() {
        let range = |softness| {
            let mut pm = Pixmap::filled(96, 96, Rgba8::new(140, 140, 140, 255));
            palette_knife(&mut pm, 12, 3, softness);
            let band: Vec<i32> = (16..80).map(|x| pm.get(x, 48).r as i32).collect();
            band.iter().max().unwrap() - band.iter().min().unwrap()
        };
        assert!(
            range(0) > 20,
            "a flat field came back flat: {} levels across it",
            range(0)
        );
        // Laid on thin, it has no edge to catch the light.
        assert!(
            range(10) < range(0),
            "thin paint caught as much light as thick: {} against {}",
            range(10),
            range(0)
        );
    }

    /// The knife spreads a surface and stops at a boundary.
    ///
    /// Stops, with the one exception the filter is built on: what lies between
    /// two masses is neither of them but a band of its own, flat, with a hard
    /// edge on each side. That is the dark ring around everything in the
    /// reference — see [`KNIFE_SETTLE`] — and this is the test that says it is
    /// a band and not a gradient, which for four attempts it wrongly was.
    #[test]
    fn the_knife_stops_at_a_boundary() {
        let mut pm = two_noisy_fields();
        palette_knife(&mut pm, 10, 3, 0);
        let left = pm.get(20, 32).r as i32;
        let right = pm.get(44, 32).r as i32;
        assert!(
            left < 90 && right > 170,
            "the knife carried one field into the other: {left} and {right}"
        );
        // Whatever is neither field is the ring, and there is not much of it:
        // the two fields meet inside the width of one stroke.
        let between: Vec<i32> = (0..64)
            .map(|x| pm.get(x, 32).r as i32)
            .filter(|&v| v > 90 && v < 170)
            .collect();
        assert!(
            between.len() <= 12,
            "the knife spread the edge over {} pixels",
            between.len()
        );
        // And it is made of flat rungs rather than of every level in between.
        let mut rungs = between.clone();
        rungs.sort_unstable();
        rungs.dedup();
        assert!(
            rungs.len() <= 4,
            "the ring came back as a gradient of {} shades, not as a band",
            rungs.len()
        );
    }

    /// Stroke Detail is how much of the picture the masses keep to. Wound up,
    /// a mass will reach for whatever matches it and the picture comes back
    /// close to what it was; wound down, the masses are held to their own patch
    /// of canvas and cut across it.
    #[test]
    fn stroke_detail_is_how_much_the_knife_keeps() {
        // A diagonal edge, which no mass held to a patch of canvas can follow:
        // every one of them that lands on the edge has to average the two sides
        // and comes back as neither. Measuring what survives of a *mark*
        // instead measures nothing, which two earlier versions of this test did
        // — a region the colour of the mark is a region at either setting, so
        // the mark comes back whole either way. What the setting changes is the
        // shape of the masses, not whether a colour survives at all.
        let strayed = |detail| {
            let mut pm = Pixmap::new(96, 96);
            for y in 0..96 {
                for x in 0..96 {
                    let v = if x < y { 70 } else { 190 };
                    pm.set(x, y, Rgba8::new(v, v, v, 255));
                }
            }
            let before = pm.clone();
            palette_knife(&mut pm, 12, detail, 0);
            (0..96)
                .map(|y| {
                    (0..96)
                        .map(|x| (pm.get(x, y).r as i32 - before.get(x, y).r as i32).abs() as u32)
                        .sum::<u32>()
                })
                .sum::<u32>()
        };
        assert!(
            strayed(3) < strayed(1),
            "the knife kept as little at full detail as at none: {} against {}",
            strayed(3),
            strayed(1)
        );
    }

    /// Softness is the blade's edge: wound up, the masses stop meeting at a
    /// hard line.
    ///
    /// Measured on a ramp, which has no fine detail anywhere — so the masses
    /// stand over the whole of it and every join is one this slider can ease.
    /// At the bottom of the slider they are left hard, which is most of what
    /// makes the reference look scraped on rather than painted.
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
            hardest_join(10) < hardest_join(0),
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
        assert!(step >= 100, "the daub washed across the edge: {step} levels");
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

    /// Dark Rough draws a dark outline along the dark side of a boundary —
    /// darker than either field — and Light Rough a light one along the
    /// bright side.
    #[test]
    fn the_rough_brushes_draw_their_halo_on_one_side() {
        let halo = |brush| {
            let mut pm = Pixmap::filled(64, 64, Rgba8::new(60, 60, 60, 255));
            pm.fill_rect(crate::buffer::Rect::new(32, 0, 32, 64), Rgba8::new(200, 200, 200, 255));
            paint_daubs(&mut pm, 8, 20, brush);
            let mean = |x: i32| (0..64).map(|y| pm.get(x, y).r as u32).sum::<u32>() / 64;
            (mean(30), mean(33))
        };
        let (dark_side, _) = halo(DaubBrush::DarkRough);
        assert!(dark_side < 30, "Dark Rough drew no outline: {dark_side}");
        let (dark_side, light_side) = halo(DaubBrush::LightRough);
        assert!(light_side > 230, "Light Rough drew no rim: {light_side}");
        assert!(dark_side > 40, "Light Rough darkened the dark side: {dark_side}");
    }

    /// Sparkle draws contour lines across a slow gradient, which the plain
    /// brush leaves smooth, and lifts the light parts of the picture.
    #[test]
    fn sparkle_draws_contours_and_lights_the_picture_up() {
        let ramp = || {
            let mut pm = Pixmap::new(64, 64);
            for y in 0..64 {
                for x in 0..64 {
                    let v = (60 + x * 2) as u8;
                    pm.set(x, y, Rgba8::new(v, v, v, 255));
                }
            }
            pm
        };
        let across = |pm: &Pixmap| {
            (1..64)
                .map(|x| (pm.get(x, 32).r as i32 - pm.get(x - 1, 32).r as i32).unsigned_abs())
                .sum::<u32>()
        };
        let (mut plain, mut sparkle) = (ramp(), ramp());
        paint_daubs(&mut plain, 4, 17, DaubBrush::Simple);
        paint_daubs(&mut sparkle, 4, 17, DaubBrush::Sparkle);
        assert!(
            across(&sparkle) > across(&plain) * 3,
            "no contours: {} against {}",
            across(&sparkle),
            across(&plain)
        );

        let mut light = Pixmap::filled(32, 32, Rgba8::new(200, 200, 200, 255));
        paint_daubs(&mut light, 4, 0, DaubBrush::Sparkle);
        assert!(light.get(16, 16).r > 220, "not lit up: {}", light.get(16, 16).r);
    }

    /// The Rough brushes lay texture where the painting ones leave a surface
    /// smooth.
    #[test]
    fn the_rough_brushes_lay_texture() {
        let texture = |brush| {
            let mut pm = Pixmap::filled(64, 64, Rgba8::new(120, 120, 120, 255));
            paint_daubs(&mut pm, 8, 10, brush);
            restlessness(&pm, 4..60)
        };
        assert_eq!(texture(DaubBrush::Simple), 0);
        assert!(texture(DaubBrush::DarkRough) > 500, "no texture: {}", texture(DaubBrush::DarkRough));
    }

    /// The Wide brushes stretch the daub sideways, so a horizontal stripe a
    /// round daub of the same size paints over survives them.
    #[test]
    fn the_wide_brushes_lay_a_wide_daub() {
        let stripe = |brush| {
            let mut pm = Pixmap::filled(64, 64, Rgba8::new(60, 60, 60, 255));
            pm.fill_rect(crate::buffer::Rect::new(0, 30, 64, 4), Rgba8::new(200, 200, 200, 255));
            paint_daubs(&mut pm, 12, 0, brush);
            pm.get(32, 31).r
        };
        assert!(stripe(DaubBrush::Simple) < 70, "the round daub kept the stripe");
        assert!(stripe(DaubBrush::WideSharp) > 190, "the wide daub lost the stripe");
    }

    #[test]
    fn paint_daubs_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(crate::buffer::Rect::new(0, 0, 3, 3), Rgba8::new(120, 140, 160, 200));
        let before = pm.clone();
        paint_daubs(&mut pm, 8, 7, DaubBrush::DarkRough);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    /// Relief catches the light: a bright disc on a dark ground comes back
    /// with highlights round it, and a flat surface comes back with none.
    #[test]
    fn plastic_wrap_highlights_relief_and_not_a_flat_surface() {
        let mut flat = Pixmap::filled(48, 48, Rgba8::new(90, 90, 90, 255));
        plastic_wrap(&mut flat, 20, 9, 7);
        let brightest = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[0]).max().unwrap();
        assert!(brightest(&flat) <= 90, "the flat surface shone: {}", brightest(&flat));

        let mut disc = Pixmap::filled(64, 64, Rgba8::new(40, 40, 40, 255));
        for y in 0..64 {
            for x in 0..64 {
                if (x - 32) * (x - 32) + (y - 32) * (y - 32) < 12 * 12 {
                    disc.set(x, y, Rgba8::new(200, 200, 200, 255));
                }
            }
        }
        plastic_wrap(&mut disc, 20, 9, 7);
        assert!(brightest(&disc) > 240, "the relief did not shine: {}", brightest(&disc));
    }

    /// Highlight Strength is how bright the highlights are, and at 0 there
    /// are none.
    #[test]
    fn plastic_wrap_highlight_strength_sets_the_gloss() {
        let lit = |strength| {
            let mut pm = Pixmap::new(64, 64);
            for y in 0..64 {
                for x in 0..64 {
                    let v = if (x / 8 + y / 8) % 2 == 0 { 60 } else { 180 };
                    pm.set(x, y, Rgba8::new(v, v, v, 255));
                }
            }
            let before = pm.clone();
            plastic_wrap(&mut pm, strength, 9, 7);
            pm.as_bytes()
                .chunks_exact(4)
                .zip(before.as_bytes().chunks_exact(4))
                .map(|(a, b)| (a[0] as i32 - b[0] as i32).max(0) as u32)
                .sum::<u32>()
        };
        assert_eq!(lit(0), 0);
        assert!(lit(20) > lit(8) * 2, "{} against {}", lit(20), lit(8));
    }

    #[test]
    fn plastic_wrap_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(crate::buffer::Rect::new(4, 4, 8, 8), Rgba8::new(250, 250, 250, 200));
        let before = pm.clone();
        plastic_wrap(&mut pm, 15, 9, 7);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn plastic_wrap_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        plastic_wrap(&mut pm, 15, 9, 7);
    }

    /// The dark side of a boundary is inked, and the light side and a flat
    /// field are not.
    #[test]
    fn poster_edges_inks_the_dark_side_of_a_boundary() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(60, 60, 60, 255));
        pm.fill_rect(crate::buffer::Rect::new(32, 0, 32, 64), Rgba8::new(200, 200, 200, 255));
        poster_edges(&mut pm, 2, 1, 6);
        assert!(pm.get(31, 32).r < 15, "no ink on the dark side: {}", pm.get(31, 32).r);
        assert!(pm.get(33, 32).r > 150, "ink on the light side: {}", pm.get(33, 32).r);
        assert!(pm.get(8, 32).r > 40, "ink on a flat field: {}", pm.get(8, 32).r);
    }

    /// Posterization bands the brightness and keeps the colour: a ramp comes
    /// back in few values, and a green stays green.
    #[test]
    fn poster_edges_bands_brightness_and_keeps_colour() {
        let mut ramp = Pixmap::new(256, 8);
        for y in 0..8 {
            for x in 0..256 {
                ramp.set(x, y, Rgba8::new(x as u8, x as u8, x as u8, 255));
            }
        }
        poster_edges(&mut ramp, 0, 0, 0);
        let mut values: Vec<u8> = (0..256).map(|x| ramp.get(x, 4).r).collect();
        // A level either way is rounding, not another band.
        values.dedup_by(|a, b| a.abs_diff(*b) <= 2);
        assert!(values.len() <= 8, "the ramp was not banded: {values:?}");

        let mut green = Pixmap::filled(32, 32, Rgba8::new(50, 100, 20, 255));
        poster_edges(&mut green, 2, 1, 0);
        let g = green.get(16, 16);
        assert!(g.g > g.r * 3 / 2 && g.g > g.b * 3, "the green lost its colour: {g:?}");
    }

    #[test]
    fn poster_edges_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(crate::buffer::Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        poster_edges(&mut pm, 2, 1, 2);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn poster_edges_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        poster_edges(&mut pm, 2, 1, 2);
    }

    fn pastel(pm: &mut Pixmap, length: u32, detail: u32, relief: u32) {
        use crate::filters::texture::{Light, Texture};
        rough_pastels(pm, length, detail, Texture::Canvas, 100, relief, Light::Bottom, false);
    }

    /// A longer stroke carries a mark further along the diagonal — but only
    /// a little: the picture stays sharp, and the long streaks are grain.
    #[test]
    fn rough_pastels_strokes_run_diagonally_and_lengthen() {
        let reach = |length| {
            let mut pm = Pixmap::filled(80, 80, Rgba8::new(40, 40, 40, 255));
            pm.fill_rect(crate::buffer::Rect::new(38, 38, 4, 4), Rgba8::new(255, 255, 255, 255));
            pastel(&mut pm, length, 1, 0);
            // Up and to the right of the mark, and straight to its right.
            let diagonal: u32 = (3..5).map(|d| pm.get(41 + d, 38 - d).r as u32).sum();
            let across: u32 = (4..7).map(|d| pm.get(41 + d, 40).r as u32).sum::<u32>() * 2 / 3;
            (diagonal, across)
        };
        let (short, _) = reach(0);
        let (long, across) = reach(40);
        assert!(long > short, "a longer stroke did not reach further: {long} against {short}");
        assert!(long > across, "the stroke did not run diagonally: {long} against {across}");
    }

    /// The pastel is paler than the picture, and grained where it was flat.
    #[test]
    fn rough_pastels_pales_and_grains_the_picture() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(100, 100, 100, 255));
        pastel(&mut pm, 6, 4, 0);
        let mean = pm.as_bytes().chunks_exact(4).map(|p| p[0] as u32).sum::<u32>() / (64 * 64);
        assert!(mean > 110, "not paler: {mean}");
        assert!(restlessness(&pm, 4..60) > 300, "no grain: {}", restlessness(&pm, 4..60));
    }

    #[test]
    fn rough_pastels_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(crate::buffer::Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        pastel(&mut pm, 6, 4, 20);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn rough_pastels_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        pastel(&mut pm, 6, 4, 20);
    }

    /// The lights are carried towards white, further as Highlight Area goes
    /// up, and not at all at Intensity 0 beyond the smudge.
    #[test]
    fn smudge_stick_brightens_the_lights() {
        let lit = |highlight, intensity| {
            let mut pm = Pixmap::filled(32, 32, Rgba8::new(170, 170, 170, 255));
            smudge_stick(&mut pm, 2, highlight, intensity);
            pm.get(16, 16).r
        };
        assert!(lit(0, 10) < 200, "a mid-light burnt out at Highlight Area 0: {}", lit(0, 10));
        assert!(lit(20, 10) > 240, "Highlight Area 20 did not burn it out: {}", lit(20, 10));
        assert!(lit(20, 0) < 180, "Intensity 0 still brightened: {}", lit(20, 0));
    }

    /// Dark is smeared along the diagonal into light, and light is not
    /// smeared into dark.
    #[test]
    fn smudge_stick_drags_dark_along_the_diagonal() {
        // Straight edges, which the median keeps; a corner it rounds away.
        // A diagonal stroke smears both a band and a stripe; a horizontal or
        // vertical one only smears one of them.
        let smeared = |rect| {
            let mut pm = Pixmap::filled(64, 64, Rgba8::new(150, 150, 150, 255));
            pm.fill_rect(rect, Rgba8::new(0, 0, 0, 255));
            smudge_stick(&mut pm, 10, 0, 0);
            pm
        };
        let band = smeared(crate::buffer::Rect::new(0, 20, 64, 20));
        let stripe = smeared(crate::buffer::Rect::new(20, 0, 20, 64));
        assert!(band.get(30, 17).r < 140, "nothing smeared over the band: {}", band.get(30, 17).r);
        assert!(stripe.get(43, 30).r < 140, "nothing smeared past the stripe: {}", stripe.get(43, 30).r);
        assert!(band.get(30, 30).r < 30, "the dark was washed out: {}", band.get(30, 30).r);
    }

    #[test]
    fn smudge_stick_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(crate::buffer::Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        smudge_stick(&mut pm, 2, 10, 10);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn smudge_stick_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        smudge_stick(&mut pm, 2, 0, 10);
    }

    /// A flat field comes back blotched, lighter and darker, and more so as
    /// Definition rises; at Definition 0 it stays flat.
    #[test]
    fn sponge_blotches_a_flat_field() {
        let spread = |definition| {
            let mut pm = Pixmap::filled(64, 64, Rgba8::new(128, 128, 128, 255));
            sponge(&mut pm, 2, definition, 5);
            let values: Vec<u8> = pm.as_bytes().chunks_exact(4).map(|p| p[0]).collect();
            (values.iter().min().copied().unwrap(), values.iter().max().copied().unwrap())
        };
        assert_eq!(spread(0), (128, 128));
        let (lo, hi) = spread(12);
        assert!(lo < 118 && hi > 138, "not blotched: {lo}..{hi}");
        let (lo2, hi2) = spread(25);
        assert!(hi2 - lo2 > hi - lo, "more Definition did not blotch harder");
    }

    /// A bigger brush lays bigger blotches: fewer changes from one pixel to
    /// the next.
    #[test]
    fn sponge_brush_size_sizes_the_blotches() {
        let busy = |size| {
            let mut pm = Pixmap::filled(96, 96, Rgba8::new(128, 128, 128, 255));
            sponge(&mut pm, size, 20, 5);
            restlessness(&pm, 4..92)
        };
        assert!(busy(0) > busy(10) * 2, "{} against {}", busy(0), busy(10));
    }

    #[test]
    fn sponge_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(crate::buffer::Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        sponge(&mut pm, 2, 12, 5);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn sponge_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        sponge(&mut pm, 2, 12, 5);
    }

    fn underpaint(pm: &mut Pixmap, size: u32, coverage: u32, relief: u32) {
        use crate::filters::texture::{Light, Texture};
        underpainting(pm, size, coverage, Texture::Burlap, 100, relief, Light::Top, false);
    }

    /// A bigger brush washes a sharp edge out further.
    #[test]
    fn underpainting_brush_size_softens_the_picture() {
        let edge = |size| {
            let mut pm = Pixmap::filled(64, 64, Rgba8::new(40, 40, 40, 255));
            pm.fill_rect(crate::buffer::Rect::new(32, 0, 32, 64), Rgba8::new(220, 220, 220, 255));
            underpaint(&mut pm, size, 0, 0);
            pm.get(29, 32).r
        };
        assert!(edge(20) > edge(0) + 30, "{} against {}", edge(20), edge(0));
    }

    /// Texture Coverage breaks a straight edge into the texture's pattern.
    #[test]
    fn underpainting_coverage_breaks_an_edge() {
        let ragged = |coverage| {
            let mut pm = Pixmap::filled(64, 64, Rgba8::new(40, 40, 40, 255));
            pm.fill_rect(crate::buffer::Rect::new(32, 0, 32, 64), Rgba8::new(220, 220, 220, 255));
            underpaint(&mut pm, 0, coverage, 0);
            restlessness(&pm, 26..38)
        };
        assert!(ragged(40) > ragged(0) + 500, "{} against {}", ragged(40), ragged(0));
    }

    #[test]
    fn underpainting_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(crate::buffer::Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        underpaint(&mut pm, 6, 16, 4);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn underpainting_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        underpaint(&mut pm, 6, 16, 4);
    }

    /// Shadow Intensity takes a mid-dark tone to black once it is up, and
    /// leaves a light one alone.
    #[test]
    fn watercolor_shadow_intensity_blackens_the_darker_tones() {
        let tone = |value, shadow| {
            let mut pm = Pixmap::filled(32, 32, Rgba8::new(value, value, value, 255));
            watercolor(&mut pm, 9, shadow, 1);
            pm.get(16, 16).r
        };
        assert!(tone(100, 0) > 60, "a mid-dark went black at 0: {}", tone(100, 0));
        assert!(tone(100, 6) < 15, "a mid-dark stayed at 6: {}", tone(100, 6));
        assert!(tone(220, 6) > 180, "a light tone went dark: {}", tone(220, 6));
    }

    /// Less detail washes a small mark away; more keeps it.
    #[test]
    fn watercolor_brush_detail_sets_the_wash() {
        let kept = |detail| {
            let mut pm = Pixmap::filled(48, 48, Rgba8::new(200, 200, 200, 255));
            pm.fill_rect(crate::buffer::Rect::new(22, 22, 5, 5), Rgba8::new(90, 90, 90, 255));
            watercolor(&mut pm, detail, 0, 1);
            pm.get(24, 24).r
        };
        assert!(kept(14) < 150, "full detail lost the mark: {}", kept(14));
        assert!(kept(1) > 170, "the broadest wash kept the mark: {}", kept(1));
    }

    #[test]
    fn watercolor_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(crate::buffer::Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        watercolor(&mut pm, 9, 1, 1);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn watercolor_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        watercolor(&mut pm, 9, 1, 1);
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


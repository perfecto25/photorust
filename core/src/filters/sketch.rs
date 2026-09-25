//! Filter ▸ Sketch.
//!
//! CS6 keeps this family in the Filter Gallery, as it does Artistic and Brush
//! Strokes. The Gallery is not built (docs/ROADMAP.md), so the filters live
//! under a Filter ▸ Sketch submenu instead.
//!
//! What binds the family together is that **it paints in the two swatches**,
//! not in the picture's own colours: every one of these but Chrome and Water
//! Paper throws the hue away, works on brightness alone, and maps the answer
//! between the document's foreground and background. The colours are
//! properties of the document rather than of the dialog, so the bridge fills
//! them in — see `Engine::filter_for`.

use crate::buffer::{Pixmap, Rgba8};
use crate::filters::artistic::blur_field;
use crate::filters::brush_strokes::{unit_spread, StrokeDirection};
use crate::filters::texture::{apply_relief_weighted, Finish, Light, Texture};
use rayon::prelude::*;

/// CS6's ranges for Bas Relief, which its two sliders run over.
pub const RELIEF_DETAIL: std::ops::RangeInclusive<u32> = 1..=15;
pub const RELIEF_SMOOTHNESS: std::ops::RangeInclusive<u32> = 1..=15;

/// How far the brightness is softened before the light is run across it, in
/// pixels per step of Smoothness.
///
/// This is the whole of Smoothness: it is what decides whether the carving
/// comes out as crisp chiselled detail or as broad, rounded, half-melted
/// forms, and nothing else in the filter changes that.
const RELIEF_SMOOTH_PER_STEP: f32 = 0.38;

/// How far apart the two samples are taken, in pixels — the depth of the
/// carving. One pixel either side: a relief is shallow, and reading further
/// turns the edges into double lines.
const RELIEF_REACH: f32 = 1.0;

/// How hard the slope is driven into the two swatches, per step of Detail.
///
/// Detail is the gain, not the scale. At the bottom of the slider only the
/// strongest boundaries lift off the flat mid-tone, which is CS6's
/// "generalized shapes"; at the top every small change in the surface is
/// pushed to one swatch or the other.
const RELIEF_GAIN_PER_STEP: f32 = 0.85;

/// Filter ▸ Sketch ▸ Bas Relief: the picture carved in shallow relief and lit
/// from one side, in the two swatches.
///
/// The picture's brightness is read as a height field: what was light stands
/// proud, what was dark is cut away. **Smoothness** softens that surface
/// first — a low value leaves every fleck of the photograph as its own ridge,
/// a high one rounds the whole thing off. Then the surface is lit from
/// **Light**: each pixel is compared with its neighbour on the lit side, and
/// a slope facing the light is driven towards the background colour while one
/// facing away goes to the foreground. **Detail** is how hard it is driven.
///
/// A flat surface catches no light either way, so it comes out at the midpoint
/// of the two swatches — which is why most of the picture is an even tone with
/// the carving standing out of it.
///
/// Alpha is left alone.
///
/// No GPU path. It is two taps and a blend per pixel — the kind of work that
/// would fit, but the upload and read-back would cost more than the
/// arithmetic saves (docs/gpu-migration.md).
pub fn bas_relief(
    pixmap: &mut Pixmap,
    detail: u32,
    smoothness: u32,
    light: Light,
    foreground: Rgba8,
    background: Rgba8,
) {
    if pixmap.is_empty() {
        return;
    }
    let detail = detail.clamp(*RELIEF_DETAIL.start(), *RELIEF_DETAIL.end()) as f32;
    let smoothness = smoothness.clamp(*RELIEF_SMOOTHNESS.start(), *RELIEF_SMOOTHNESS.end()) as f32;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);

    // The surface: the picture's own brightness, softened by Smoothness.
    let mut surface: Vec<f32> = pixmap
        .as_bytes()
        .par_chunks_exact(4)
        .map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
        .collect();
    blur_field(&mut surface, w, h, smoothness * RELIEF_SMOOTH_PER_STEP);

    let (lx, ly) = light.towards();
    let (dx, dy) = (lx * RELIEF_REACH, ly * RELIEF_REACH);
    let gain = detail * RELIEF_GAIN_PER_STEP;
    let ink = [foreground.r as f32, foreground.g as f32, foreground.b as f32];
    let paper = [background.r as f32, background.g as f32, background.b as f32];
    let surface = &surface;
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let at = |ox: f32, oy: f32| {
                    let sx = (x as f32 + ox).round().clamp(0.0, (w - 1) as f32) as usize;
                    let sy = (y as f32 + oy).round().clamp(0.0, (h - 1) as f32) as usize;
                    surface[sy * w + sx]
                };
                // A face is lit when it *falls* towards the light: walking
                // south from a bump's peak with the sun in the south, the
                // ground drops away and the slope you are on is the one
                // facing the sun. Reading the slope the other way up lights
                // every ridge from the opposite side to the one asked for.
                let slope = (at(-dx, -dy) - at(dx, dy)) / 255.0;
                let lit = (0.5 + slope * gain).clamp(0.0, 1.0);
                for (c, (dark, pale)) in ink.iter().zip(paper.iter()).enumerate() {
                    px[c] = (dark + (pale - dark) * lit).round().clamp(0.0, 255.0) as u8;
                }
                // Alpha stands: carving the picture does not change the
                // layer's shape.
            }
        });
}

/// CS6's ranges for Chalk & Charcoal, which its three sliders run over.
pub const CHALK_AREA: std::ops::RangeInclusive<u32> = 0..=20;
pub const CHALK_PRESSURE: std::ops::RangeInclusive<u32> = 0..=5;

/// The mid grey the two media are drawn on.
///
/// A literal neutral grey, not the midpoint of the two swatches: with a blue
/// foreground and a white background the midpoint is pale blue, and CS6's
/// ground stays grey. Adobe calls it "a solid midtone gray foundation", and
/// that is exactly what it is — the paper, not a mixture of the sticks.
const CHALK_GROUND: f32 = 128.0;

/// How far the brightness is smudged before either stick is laid on it, in
/// pixels. Charcoal does not respect single pixels.
const CHALK_SMUDGE: f32 = 1.4;

/// Where the charcoal reaches to, as a fraction of white: a floor and a step
/// of Charcoal Area. Anything darker than this is taken by the foreground.
const CHARCOAL_FROM: f32 = 0.185;
const CHARCOAL_PER_STEP: f32 = 0.019;

/// Where the chalk reaches down to, as a fraction of white: a ceiling and a
/// step of Chalk Area. Anything lighter than this is taken by the background.
const CHALK_FROM: f32 = 0.955;
const CHALK_PER_STEP: f32 = 0.0125;

/// How soft the change from ground to stick is: the widest it gets, and how
/// far each step of Stroke Pressure narrows it.
///
/// This is Stroke Pressure. A light hand leaves the tone grading into the
/// paper; a heavy one lays the stick down flat, and the picture separates
/// into three colours with hard edges between them.
const CHALK_SOFT: f32 = 0.38;
const CHALK_SOFT_PER_STEP: f32 = 0.072;

/// How long the strokes are, in pixels, and how wide. Long coarse drags, not
/// fine hatching: a stick of charcoal is blunt.
const CHALK_STROKE: f32 = 26.0;
const CHALK_STROKE_WIDTH: f32 = 1.1;

/// How far the strokes swing the tone they are read against.
///
/// The swing is what breaks a boundary into separate marks rather than a
/// feathered outline, and it is also what puts texture *inside* an area.
///
/// **It does not change with Stroke Pressure**, and it is worth knowing why,
/// because scaling it looks like the obvious thing to do. The swing and the
/// threshold width together already give both ends of that slider: wound
/// down, the threshold is wide, so the swing only moves coverage a little
/// either way and the picture grades smoothly with no strokes to speak of;
/// wound up, the threshold is a hard line, so ground within a swing of it
/// breaks into separate marks while everything further off lies flat. Making
/// the swing fall as well flattens nothing extra at the top and turns the
/// bottom of the slider — which should be the *smoothest* setting there is —
/// into hatching laid over the whole picture.
const CHALK_SWING: f32 = 0.06;

/// Which way each stick is dragged, in degrees anticlockwise from the
/// horizontal. Opposite diagonals: it is what tells the two media apart where
/// they meet, and the crossing marks are plain in CS6 wherever the ground
/// shows between them.
const CHARCOAL_ANGLE: f32 = -45.0;
const CHALK_ANGLE: f32 = 45.0;

/// Filter ▸ Sketch ▸ Chalk & Charcoal: the picture redrawn in charcoal and
/// chalk over a mid-grey ground.
///
/// Three tones and no more. The dark of the picture is taken by charcoal in
/// the foreground colour, the light by chalk in the background colour, and
/// what neither reaches is left as bare grey paper — which is why the result
/// reads as a drawing rather than as a tinted photograph.
///
/// **Charcoal Area** is how far up the tones the charcoal climbs and **Chalk
/// Area** how far down the chalk comes; wound far enough they meet in the
/// middle and the grey disappears. **Stroke Pressure** is how hard the sticks
/// are pressed: lightly, and each grades into the paper; heavily, and they lie
/// flat with a hard edge.
///
/// Both are dragged along opposite diagonals, and the marks swing the tone
/// they are read against, so an edge breaks into separate strokes instead of
/// being feathered.
///
/// Alpha is left alone.
///
/// No GPU path, for Bas Relief's reasons: the streaked noise each stick is
/// dragged along would upload and read back for a few taps of arithmetic.
pub fn chalk_and_charcoal(
    pixmap: &mut Pixmap,
    charcoal_area: u32,
    chalk_area: u32,
    pressure: u32,
    foreground: Rgba8,
    background: Rgba8,
) {
    if pixmap.is_empty() {
        return;
    }
    let charcoal_area = charcoal_area.clamp(*CHALK_AREA.start(), *CHALK_AREA.end()) as f32;
    let chalk_area = chalk_area.clamp(*CHALK_AREA.start(), *CHALK_AREA.end()) as f32;
    let pressure = pressure.clamp(*CHALK_PRESSURE.start(), *CHALK_PRESSURE.end()) as f32;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);

    let mut tone: Vec<f32> = pixmap
        .as_bytes()
        .par_chunks_exact(4)
        .map(|p| (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) / 255.0)
        .collect();
    blur_field(&mut tone, w, h, CHALK_SMUDGE);

    let marks = |angle: f32, salt: usize| {
        let radians: f32 = angle.to_radians();
        let mut field = crate::filters::artistic::streaked_noise(
            w,
            h,
            CHALK_STROKE,
            salt,
            (radians.cos(), -radians.sin()),
        );
        blur_field(&mut field, w, h, CHALK_STROKE_WIDTH);
        unit_spread(&mut field);
        field
    };
    let charcoal_marks = marks(CHARCOAL_ANGLE, 131);
    let chalk_marks = marks(CHALK_ANGLE, 132);

    let charcoal_edge = CHARCOAL_FROM + charcoal_area * CHARCOAL_PER_STEP;
    let chalk_edge = CHALK_FROM - chalk_area * CHALK_PER_STEP;
    let soft = (CHALK_SOFT - pressure * CHALK_SOFT_PER_STEP).max(0.015);
    let ink = [foreground.r as f32, foreground.g as f32, foreground.b as f32];
    let paper = [background.r as f32, background.g as f32, background.b as f32];
    let (tone, charcoal_marks, chalk_marks) = (&tone, &charcoal_marks, &chalk_marks);

    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let i = y * w + x;
                let stick = |edge: f32, swing: f32, below: bool| {
                    let against = tone[i] + swing.clamp(-2.0, 2.0) * CHALK_SWING;
                    let over = if below { edge - against } else { against - edge };
                    let t = (over / soft * 0.5 + 0.5).clamp(0.0, 1.0);
                    t * t * (3.0 - 2.0 * t)
                };
                let mut dark = stick(charcoal_edge, charcoal_marks[i], true);
                let mut light = stick(chalk_edge, chalk_marks[i], false);
                // Wound far enough the two areas overlap. Neither stick gives
                // way to the other — they mix on the paper.
                let laid = dark + light;
                if laid > 1.0 {
                    dark /= laid;
                    light /= laid;
                }
                let bare = 1.0 - dark - light;
                for (c, (dark_c, pale_c)) in ink.iter().zip(paper.iter()).enumerate() {
                    let v = CHALK_GROUND * bare + dark_c * dark + pale_c * light;
                    px[c] = v.round().clamp(0.0, 255.0) as u8;
                }
                // Alpha stands: redrawing the picture does not change the
                // layer's shape.
            }
        });
}

/// CS6's ranges for Charcoal, which its three sliders run over.
pub const COAL_THICKNESS: std::ops::RangeInclusive<u32> = 1..=7;
pub const COAL_DETAIL: std::ops::RangeInclusive<u32> = 0..=5;
pub const COAL_BALANCE: std::ops::RangeInclusive<u32> = 0..=100;

/// How far the picture is simplified before anything is drawn, in pixels: the
/// most, at Detail 0, and how far each step of Detail claws back.
///
/// Detail runs backwards from the other sliders — it is how much of the
/// photograph's own fine structure survives to be drawn, so *less* of it
/// means *more* blur and broader, emptier shapes.
const COAL_SIMPLIFY: f32 = 2.6;
const COAL_SIMPLIFY_PER_STEP: f32 = 0.44;

/// Where the charcoal fills solid, as a fraction of white: a floor and the
/// span Light/Dark Balance runs over.
///
/// This is the balance. At the bottom only what is nearly black takes any
/// charcoal at all and the sheet is almost bare; at the top the stick reaches
/// most of the way up the tones and very little paper is left.
const COAL_MASS_FROM: f32 = 0.10;
const COAL_MASS_SPAN: f32 = 0.50;

/// How soft the edge of a filled mass is, as a fraction of white.
///
/// A ramp, not a threshold: charcoal is a tonal medium, and a drawing made
/// with it runs from bare paper through every grey to solid black. Cut hard,
/// the picture goes to two colours and the grain that should be reading as
/// shading is left as bare streaks scratched over a silhouette.
///
/// But only just wide enough to shade the subject. Widened past that it
/// reaches up into the midtones and lays a haze over the sea and sky as
/// well — and then there is no drawing left, only an evenly hatched
/// rectangle. The empty paper around the subject is half of what makes this
/// read as a sketch.
const COAL_MASS_SOFT: f32 = 0.22;

/// Which edges are drawn, in Sobel units of the simplified brightness: the
/// threshold at Detail 0, and how far each step of Detail lowers it. A low
/// Detail draws only where the picture really turns; a high one picks up
/// every ripple.
const COAL_EDGE_FROM: f32 = 180.0;
const COAL_EDGE_PER_STEP: f32 = 15.0;
const COAL_EDGE_SPAN: f32 = 30.0;

/// How far a drawn line is softened, and how dark it is allowed to get.
const COAL_EDGE_SOFT: f32 = 0.7;
const COAL_EDGE_WEIGHT: f32 = 0.45;

/// How far an edge is spread, in pixels per step of Charcoal Thickness —
/// CS6's "expands the stroke kernel", and the whole of that slider's effect
/// on the outlines.
const COAL_THICK_PER_STEP: f32 = 0.25;

/// The strokes the stick leaves: which way it is dragged, how long each mark
/// is, and how wide.
///
/// Short. Charcoal hatching is made of many small strokes packed together,
/// not of lines drawn from one side of the picture to the other, and the
/// length here is the length of one of them.
const COAL_ANGLE: f32 = 45.0;
const COAL_STROKE: f32 = 10.0;
const COAL_STROKE_WIDTH: f32 = 0.8;

/// How far the strokes swing the shading either side of what the tone owes.
///
/// Deep enough that the hatch reads as separate marks, shallow enough that
/// they stay grey rather than snapping to paper and ink.
const COAL_GRAIN_DEPTH: f32 = 0.24;

/// How far a thick stick shifts the whole hatch towards ink.
const COAL_THICK_INK: f32 = 0.12;

/// Filter ▸ Sketch ▸ Charcoal: the picture redrawn as a charcoal sketch on
/// bare paper, in the two swatches.
///
/// 1. **The masses.** What is darker than **Light/Dark Balance** fills with
///    charcoal in the foreground colour. Everything else is left as paper in
///    the background colour — there is no middle tone, which is what makes
///    this a sketch rather than a photograph.
/// 2. **The outlines.** The picture's edges are drawn as well, so a shape too
///    light to fill still gets a line round it. **Detail** decides how weak an
///    edge still counts, and **Charcoal Thickness** how far each line is
///    spread — bold heavy outlines at the top of the slider.
/// 3. **The tooth.** Both are dragged along one diagonal and broken up by the
///    grain of the paper, so the white of the sheet streaks through even the
///    densest mass. A thicker stick fills more of it in.
///
/// Alpha is left alone.
///
/// No GPU path, for Bas Relief's reasons.
pub fn charcoal(
    pixmap: &mut Pixmap,
    thickness: u32,
    detail: u32,
    balance: u32,
    foreground: Rgba8,
    background: Rgba8,
) {
    if pixmap.is_empty() {
        return;
    }
    let thickness = thickness.clamp(*COAL_THICKNESS.start(), *COAL_THICKNESS.end()) as f32;
    let detail = detail.clamp(*COAL_DETAIL.start(), *COAL_DETAIL.end()) as f32;
    let balance = balance.clamp(*COAL_BALANCE.start(), *COAL_BALANCE.end()) as f32 / 100.0;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);

    // 1: the picture simplified to what a stick of charcoal can say.
    let mut tone: Vec<f32> = pixmap
        .as_bytes()
        .par_chunks_exact(4)
        .map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
        .collect();
    let simplify = (COAL_SIMPLIFY - detail * COAL_SIMPLIFY_PER_STEP).max(0.3);
    blur_field(&mut tone, w, h, simplify);

    // 2: the outlines, spread by Charcoal Thickness.
    let mut edges = crate::filters::brush_strokes::sobel(&tone, w, h);
    let edge_from = (COAL_EDGE_FROM - detail * COAL_EDGE_PER_STEP).max(3.0);
    edges.par_iter_mut().for_each(|e| {
        let t = ((*e - edge_from) / COAL_EDGE_SPAN).clamp(0.0, 1.0);
        *e = t * t * (3.0 - 2.0 * t);
    });
    let spread = (thickness * COAL_THICK_PER_STEP).round() as usize;
    crate::filters::brush_strokes::widest_nearby(&mut edges, w, h, spread);
    // Softened, so a line lies down into the shading around it rather than
    // being stamped over the top of it in flat black.
    blur_field(&mut edges, w, h, COAL_EDGE_SOFT);
    edges.par_iter_mut().for_each(|e| *e *= COAL_EDGE_WEIGHT);

    // 3: the grain of the paper.
    let radians: f32 = COAL_ANGLE.to_radians();
    let mut grain = crate::filters::artistic::streaked_noise(
        w,
        h,
        COAL_STROKE,
        141,
        (radians.cos(), -radians.sin()),
    );
    blur_field(&mut grain, w, h, COAL_STROKE_WIDTH);
    unit_spread(&mut grain);

    let mass_from = COAL_MASS_FROM + balance * COAL_MASS_SPAN;
    // A heavier stick lays a mark where a finer one would have left paper.
    let bias = thickness / *COAL_THICKNESS.end() as f32 * COAL_THICK_INK;
    let ink = [foreground.r as f32, foreground.g as f32, foreground.b as f32];
    let paper = [background.r as f32, background.g as f32, background.b as f32];
    let (tone, edges, grain) = (&tone, &edges, &grain);

    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let i = y * w + x;
                // How much charcoal this pixel is owed, before the hatch:
                // a smooth ramp from bare paper to solid, plus the outlines.
                let t = ((mass_from - tone[i] / 255.0) / COAL_MASS_SOFT * 0.5 + 0.5)
                    .clamp(0.0, 1.0);
                let mass = t * t * (3.0 - 2.0 * t);
                let owed = mass.max(edges[i]).clamp(0.0, 1.0);

                // The hatch: the strokes swing the shading up and down about
                // what is owed, so a shaded area comes back as many separate
                // marks, each carrying its own grey.
                //
                // The swing closes at both ends of the range. Bare paper has
                // nothing drawn on it and solid black has no gaps left in it;
                // it is only in between that a stick leaves strokes at all.
                // Without that, marks appear on the empty sheet and holes
                // open in the darks. And a *swing* rather than a threshold is
                // what keeps the drawing continuous in tone: compared against
                // a threshold instead, every pixel comes out either paper or
                // ink, and the result is one bit deep and staircased where
                // charcoal is soft and grey.
                // Flattened, so the stick still leaves strokes well down into
                // the darks and up into the lights. Squared off, the swing
                // dies away so fast that anything approaching solid comes
                // back as a flat silhouette with no stroke in it at all.
                let open = (4.0 * owed * (1.0 - owed)).clamp(0.0, 1.0).sqrt();
                let depth = COAL_GRAIN_DEPTH * open;
                let coal = (owed + bias * open + grain[i] * depth).clamp(0.0, 1.0);
                for (c, (dark, pale)) in ink.iter().zip(paper.iter()).enumerate() {
                    px[c] = (pale + (dark - pale) * coal).round().clamp(0.0, 255.0) as u8;
                }
                // Alpha stands: sketching the picture does not change the
                // layer's shape.
            }
        });
}

/// CS6's ranges for Chrome, which its two sliders run over.
pub const CHROME_DETAIL: std::ops::RangeInclusive<u32> = 0..=10;
pub const CHROME_SMOOTHNESS: std::ops::RangeInclusive<u32> = 0..=10;

/// How far the surface is melted before it is polished, in pixels: a floor
/// and a step of Smoothness.
///
/// This is what makes the metal *liquid*. The waveform below draws a band
/// wherever the surface crosses a level, so the shape of those bands is the
/// shape of the surface's contours — and a photograph's contours are ragged
/// until they are smoothed into something that pours.
const CHROME_MELT: f32 = 1.2;
const CHROME_MELT_PER_STEP: f32 = 1.6;

/// How much further the broad surface is poured than the melted one, and how
/// much of the melted surface survives at the bottom of Detail.
const CHROME_BROAD: f32 = 3.0;
const CHROME_DETAIL_FLOOR: f32 = 0.2;

/// What share of the picture is left outside the stretch at each end, so a
/// handful of extreme pixels cannot set the range for all of it.
const CHROME_FLOOR: f32 = 0.005;

/// How far the tones are pulled towards filling the range.
///
/// Part of the way, not all. Stretched fully, a flat expanse like a clear sky
/// is spread across a whole trough of the waveform and comes back as one
/// enormous dark slab, where CS6 leaves it bright; not stretched at all, a
/// photograph in a narrow band of tones barely completes a cycle and the
/// metal has no reflections in it worth the name.
const CHROME_STRETCH: f32 = 0.5;

/// How many times the waveform runs from light to dark across the tonal
/// range: a floor and a step of Detail.
///
/// This is the whole trick. A polished surface does not shade smoothly from
/// dark to light — it mirrors whatever is around it, so the tone runs up to a
/// highlight, breaks, and starts again. Cycling the tone several times over
/// the range is what the eye reads as a horizon reflected in metal.
const CHROME_CYCLES: f32 = 3.5;
const CHROME_CYCLES_PER_STEP: f32 = 0.7;

/// How narrow the dark troughs are.
///
/// A plain cosine spends as much of its run dark as it does light, and the
/// picture comes out a quarter black — banded like corrugated card rather
/// than polished. Metal is bright nearly everywhere; the dark shows only as a
/// thin line where the surface turns right away from the light. Raising this
/// pinches the troughs towards those lines and leaves the rest of the sheet
/// in the light.
const CHROME_TROUGH: f32 = 3.5;

/// How far the sheet is tipped towards the light, as a fraction of a cycle
/// per level of slope.
///
/// A *signed* slope, read along one diagonal: it shifts where the waveform
/// sits rather than adding brightness, so one face of a fold catches the
/// light and the other loses it. Taken as a magnitude instead — which is what
/// an edge detector gives — every edge in the picture gets a bright halo on
/// both sides of it, and the result is a contour map rather than metal.
const CHROME_TIP: f32 = 0.10;

/// Filter ▸ Sketch ▸ Chrome: the picture as a sheet of polished metal.
///
/// 1. **The surface.** The brightness is read as a height field and melted by
///    **Smoothness**, so its contours pour instead of following every ragged
///    edge of the photograph. **Detail** puts back as much of the picture's
///    own fine structure as you ask for.
/// 2. **The polish.** That surface is run through a waveform rather than a
///    straight ramp: the tone climbs to a highlight, clips to white, drops
///    away to near-black and climbs again, several times over the range.
///    Each crossing draws a band, and because the bands follow the surface's
///    contours they read as a horizon reflected in chrome.
///
/// **Detail** also sets how many times the waveform runs, so winding it up
/// gives every ripple a reflection of its own.
///
/// Unlike the rest of this family it does not paint between the two
/// swatches: chrome is grey, and CS6's is grey whatever the swatches are set
/// to.
///
/// Alpha is left alone.
///
/// No GPU path, for Bas Relief's reasons.
pub fn chrome(pixmap: &mut Pixmap, detail: u32, smoothness: u32) {
    if pixmap.is_empty() {
        return;
    }
    let detail = detail.clamp(*CHROME_DETAIL.start(), *CHROME_DETAIL.end()) as f32;
    let smoothness = smoothness.clamp(*CHROME_SMOOTHNESS.start(), *CHROME_SMOOTHNESS.end()) as f32;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);

    // 1: the surface.
    let tone: Vec<f32> = pixmap
        .as_bytes()
        .par_chunks_exact(4)
        .map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
        .collect();
    let mut melted = tone.clone();
    blur_field(&mut melted, w, h, CHROME_MELT + smoothness * CHROME_MELT_PER_STEP);

    // The same surface poured out much further, for Detail to choose
    // against: at the bottom of the slider the chrome runs in a few broad
    // sheets, at the top it keeps every fold the melt left.
    //
    // Chosen *between* two melts rather than by adding the fine structure
    // back on top. Added back, the gain needed at the top of the slider also
    // multiplies the photograph's grain, and the waveform turns that grain
    // into dense speckle — the picture stops being metal and becomes noise.
    let mut broad = tone.clone();
    blur_field(&mut broad, w, h, (CHROME_MELT + smoothness * CHROME_MELT_PER_STEP) * CHROME_BROAD);
    let keep = CHROME_DETAIL_FLOOR
        + detail / *CHROME_DETAIL.end() as f32 * (1.0 - CHROME_DETAIL_FLOOR);
    let surface: Vec<f32> = broad
        .par_iter()
        .zip(melted.par_iter())
        .map(|(b, m)| b + (m - b) * keep)
        .collect();

    // Stretched to fill the range. A photograph's tones sit in whatever
    // narrow band the exposure gave them, and the waveform below runs on a
    // *fraction* of a cycle across such a band — a couple of sheets of metal
    // and no reflection at all. CS6's cycles several times over exactly such
    // a range, which it can only do by measuring the range first. Read off
    // percentiles rather than the outright darkest and lightest pixel, so one
    // speck of blown highlight cannot flatten the whole picture.
    let surface = {
        let mut sorted: Vec<f32> = surface.clone();
        sorted.par_sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
        let at = |f: f32| sorted[((sorted.len() - 1) as f32 * f) as usize];
        let (low, high) = (at(CHROME_FLOOR), at(1.0 - CHROME_FLOOR));
        let span = (high - low).max(1.0);
        surface
            .par_iter()
            .map(|v| {
                let pulled = ((v - low) / span).clamp(0.0, 1.0) * 255.0;
                v + (pulled - v) * CHROME_STRETCH
            })
            .collect::<Vec<f32>>()
    };

    // The slope along one diagonal, signed, for the tip towards the light.
    let mut lit = vec![0.0f32; w * h];
    lit.par_chunks_exact_mut(w).enumerate().for_each(|(y, row)| {
        for (x, slot) in row.iter_mut().enumerate() {
            let at = |ox: i32, oy: i32| {
                let sx = (x as i32 + ox).clamp(0, w as i32 - 1) as usize;
                let sy = (y as i32 + oy).clamp(0, h as i32 - 1) as usize;
                surface[sy * w + sx]
            };
            *slot = (at(-1, -1) - at(1, 1)) / 255.0;
        }
    });

    // 2: the polish.
    let cycles = CHROME_CYCLES + detail * CHROME_CYCLES_PER_STEP;
    let (surface, lit) = (&surface, &lit);
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let i = y * w + x;
                let level = (surface[i] / 255.0).clamp(0.0, 1.0);
                // Up to a highlight, over the edge, and up again. The tip
                // slides the sheet along the waveform rather than brightening
                // it, so a fold lights on one face and darkens on the other.
                let along = level * cycles + lit[i].clamp(-1.0, 1.0) * CHROME_TIP;
                let wave = 0.5 - 0.5 * (along * std::f32::consts::TAU).cos();
                // Squared off, so it holds white and holds black instead of
                // rippling evenly between them.
                let polished = 1.0 - (1.0 - wave).powf(CHROME_TROUGH);
                let v = (polished * 255.0).round().clamp(0.0, 255.0) as u8;
                px[0] = v;
                px[1] = v;
                px[2] = v;
                // Alpha stands: polishing the picture does not change the
                // layer's shape.
            }
        });
}

/// CS6's ranges for Conté Crayon's two level sliders. Its Texture, Scaling,
/// Relief, Light and Invert are the block in [`crate::filters::texture`],
/// shared with Texturizer and the rest.
pub const CONTE_LEVEL: std::ops::RangeInclusive<u32> = 1..=15;

/// How far up the tones a full Foreground Level carries the crayon, and how
/// far down a full Background Level brings the paper — the black point and
/// the white point of the mapping between the two swatches.
const CONTE_BLACK: f32 = 0.28;
const CONTE_WHITE: f32 = 0.45;

/// How hard the paper's weave is brought up: a power on the slope below one
/// lifts the faint threads towards the strong, and the hollows between them
/// are darkened as a groove is.
const CONTE_CRISP: f32 = 0.35;
const CONTE_OCCLUSION: f32 = 1.3;

/// How far the picture is softened before its tones are mapped, in pixels.
///
/// A crayon is blunt. Mapped off the photograph as it stands, every hair of
/// the subject comes back as a hard black line and the drawing is sharper
/// than the thing that supposedly drew it.
const CONTE_BLUNT: f32 = 1.1;

/// Filter ▸ Sketch ▸ Conté Crayon: the picture drawn in a waxy stick on
/// textured paper, in the two swatches.
///
/// The picture's tones are mapped between the two swatches: the dark carried
/// in the foreground colour, the light left as bare paper in the background
/// colour, and **everything between them drawn in the greys between the
/// two**. **Foreground Level** is how far up the tones go solid and
/// **Background Level** how far down the paper stays bare, so together they
/// set the contrast of the drawing. Over that, the paper's own tooth grains
/// the midtones.
///
/// **Texture**, **Scaling**, **Relief**, **Light** and **Invert** are the
/// block CS6 shares with Texturizer: the same paper, read as a height field
/// and lit from one side, laid over the finished drawing.
///
/// The paper reaches the drawing *only* through that lighting, which is why a
/// square weave comes out as horizontal striations under a light from the
/// top: a slope is lit by how far it falls away from the light, so the
/// threads running across the picture catch it and the ones running down it
/// do not. Reading the height field directly instead — on the reasoning that
/// a waxy stick catches the tops of the grain — lays the weave over the
/// picture as a square grid, the same whichever way the light is set, and no
/// Light setting can then look right.
///
/// Alpha is left alone.
///
/// No GPU path, for Bas Relief's reasons.
#[allow(clippy::too_many_arguments)]
pub fn conte_crayon(
    pixmap: &mut Pixmap,
    foreground_level: u32,
    background_level: u32,
    texture: Texture,
    scaling: u32,
    relief: u32,
    light: Light,
    invert: bool,
    foreground: Rgba8,
    background: Rgba8,
) {
    if pixmap.is_empty() {
        return;
    }
    let top = *CONTE_LEVEL.end() as f32 - 1.0;
    let fore = (foreground_level.clamp(*CONTE_LEVEL.start(), *CONTE_LEVEL.end()) - 1) as f32 / top;
    let back = (background_level.clamp(*CONTE_LEVEL.start(), *CONTE_LEVEL.end()) - 1) as f32 / top;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);

    // The picture softened to what a blunt stick can say.
    let mut tones: Vec<f32> = pixmap
        .as_bytes()
        .par_chunks_exact(4)
        .map(|p| (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) / 255.0)
        .collect();
    blur_field(&mut tones, w, h, CONTE_BLUNT);
    let tones = &tones;

    let black_point = CONTE_BLACK * fore;
    let white_point = 1.0 - CONTE_WHITE * back;
    let span = (white_point - black_point).max(0.05);
    let ink = [foreground.r as f32, foreground.g as f32, foreground.b as f32];
    let paper = [background.r as f32, background.g as f32, background.b as f32];

    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                // Where this tone sits between the two swatches: 1 all
                // crayon, 0 all paper, and every grey in between.
                let laid = ((white_point - tones[y * w + x]) / span).clamp(0.0, 1.0);
                for (c, (dark, pale)) in ink.iter().zip(paper.iter()).enumerate() {
                    px[c] = (pale + (dark - pale) * laid).round().clamp(0.0, 255.0) as u8;
                }
                // Alpha stands: drawing over the picture does not change the
                // layer's shape.
            }
        });

    // And the paper itself, lit from one side.
    //
    // Harder than Texturizer lays the same surface. There the paper is a
    // ground the picture sits on and a whisper of it is enough; here the
    // paper *is* the drawing's grain, and CS6 shows the weave plainly at a
    // Relief of 4 where Texturizer at 4 is barely there. The slope is
    // sharpened and the hollows darkened to bring it up without touching what
    // the slider means.
    apply_relief_weighted(
        pixmap,
        texture,
        scaling,
        relief,
        light,
        invert,
        Finish { crisp: CONTE_CRISP, occlusion: CONTE_OCCLUSION, ..Finish::default() },
    );
}

/// CS6's ranges for Graphic Pen, which its two sliders run over.
pub const PEN_LENGTH: std::ops::RangeInclusive<u32> = 1..=15;
pub const PEN_BALANCE: std::ops::RangeInclusive<u32> = 0..=100;

/// How long one stroke is, in pixels: a floor and a step of Stroke Length.
///
/// This is the whole of Stroke Length. The strokes are drawn by a field of
/// noise that stays with itself along **Stroke Direction** for about this far
/// and is uncorrelated across it, so a threshold taken through it comes back
/// as separate marks of this length lying the way the direction says. At the
/// bottom a mark is a pixel — the drawing is dithering, and keeps every fleck
/// of the photograph; at the top it is a long dash, and the texture smooths
/// away.
const PEN_STREAK: f32 = 1.0;
const PEN_STREAK_PER_STEP: f32 = 1.6;

/// How far Light/Dark Balance slides the ink either way, and how hard the tone
/// is driven into it.
///
/// At the middle of the slider the coverage is the tone's own darkness, wound
/// up by the gain so that the darks of a photograph reach solid rather than
/// stopping at stripes. At the bottom nearly everything but the darkest tones
/// is left as bare paper and at the top even a white ground takes ink. CS6
/// opens this filter low, near the bottom of the slider, which is what makes
/// it start as a light sketch rather than as a solid block.
const PEN_BALANCE_SPAN: f32 = 0.9;
const PEN_GAIN: f32 = 1.5;

/// The spread of the threshold field, in units of the noise's own width.
///
/// The coverage is a fraction of the sheet to ink and the threshold field says
/// which pixels those are: where the field is below the coverage the pixel is
/// inked. Widened, the marks separate into distinct strokes with paper between
/// them; narrowed, they clump. This is what keeps a midtone a hatch of strokes
/// rather than an even grey.
const PEN_SPREAD: f32 = 0.22;

/// Filter ▸ Sketch ▸ Graphic Pen: the picture drawn in fine pen strokes lying
/// along **Stroke Direction**, in the two swatches.
///
/// The picture's brightness is read as how much of the sheet takes ink — dark
/// means most of it, light almost none — and a field of noise decides *where*,
/// so a midtone comes back as a hatch of separate strokes rather than as an
/// even tone. The strokes are drawn by noise that is coherent along the
/// direction and uncorrelated across it, which is what makes them lie the way
/// the direction says while falling at random rather than on a printed screen.
/// What is inked takes the foreground colour and the rest the background:
/// there is no middle tone, which is what makes this read as a pen drawing
/// rather than as a tinted photograph.
///
/// **Stroke Length** is how long one stroke is. Short, and the marks are dots
/// and the drawing is dithering that keeps the photograph's detail; long, and
/// they are dashes and the texture smooths away. **Light/Dark Balance** slides
/// the whole thing towards ink or towards paper, and wound well up the darkest
/// areas reach solid black as the strokes run together.
///
/// Alpha is left alone.
///
/// No GPU path, for Bas Relief's reasons: the marks are laid by where on the
/// canvas a pixel is, and the work per pixel is one comparison.
pub fn graphic_pen(
    pixmap: &mut Pixmap,
    stroke_length: u32,
    balance: u32,
    direction: StrokeDirection,
    foreground: Rgba8,
    background: Rgba8,
) {
    if pixmap.is_empty() {
        return;
    }
    let length = stroke_length.clamp(*PEN_LENGTH.start(), *PEN_LENGTH.end()) as f32;
    let balance = balance.clamp(*PEN_BALANCE.start(), *PEN_BALANCE.end()) as f32 / 100.0;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);

    // The strokes run along this direction. The field below stays with itself
    // along it for about Stroke Length and is uncorrelated across it, so a
    // threshold taken through the field comes back as marks of that length
    // lying that way.
    let angle = direction.angle().to_radians();
    let (dx, dy) = (angle.cos(), -angle.sin());
    let streak = (PEN_STREAK + length * PEN_STREAK_PER_STEP).max(1.0);
    let mut hatch = crate::filters::artistic::streaked_noise(w, h, streak, 161, (dx, dy));
    unit_spread(&mut hatch);

    let shift = (balance - 0.5) * PEN_BALANCE_SPAN;
    let ink = [foreground.r as f32, foreground.g as f32, foreground.b as f32];
    let paper = [background.r as f32, background.g as f32, background.b as f32];
    let hatch = &hatch;

    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let i = y * w + x;
                let tone = (0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32)
                    / 255.0;
                // How much of the sheet this tone wants inked: its darkness,
                // slid towards ink or paper by Balance and wound up by the
                // gain so that a dark area reaches solid.
                let coverage = ((1.0 - tone + shift) * PEN_GAIN).clamp(0.0, 1.0);
                // Where the marks fall. Where the field sits under the
                // coverage the pixel is inked; because the field is streaked
                // along the direction those pixels come in strokes, and
                // because it is noise they fall at random rather than on a
                // printed screen.
                let threshold = 0.5 + hatch[i] * PEN_SPREAD;
                let color = if coverage > threshold { &ink } else { &paper };
                for (c, v) in color.iter().enumerate() {
                    px[c] = v.round().clamp(0.0, 255.0) as u8;
                }
                // Alpha stands: drawing over the picture does not change the
                // layer's shape.
            }
        });
}

/// CS6's ranges for Halftone Pattern, which its two sliders run over.
pub const HALFTONE_SIZE: std::ops::RangeInclusive<u32> = 1..=12;
pub const HALFTONE_CONTRAST: std::ops::RangeInclusive<u32> = 0..=50;

/// Which pattern the screen is ruled into, in the order CS6's Pattern Type
/// lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HalftonePattern {
    /// A chessboard of squares, `size` pixels to a square, square to the
    /// picture. CS6 calls it Dot; nothing about it is round.
    #[default]
    Dot,
    /// Concentric rings about the middle of the picture.
    Circle,
    /// Parallel horizontal lines, each growing from the middle of its band.
    Line,
}

impl HalftonePattern {
    pub fn from_i32(value: i32) -> HalftonePattern {
        match value {
            1 => HalftonePattern::Circle,
            2 => HalftonePattern::Line,
            _ => HalftonePattern::Dot,
        }
    }
}

/// The finest cell the Line and Circle screens can be ruled into, in pixels.
///
/// CS6's Size runs down to 1, but a band or a ring one or two pixels across
/// has no room for an edge: it is on or off for the whole cell, so the screen
/// would come back as a plain threshold with no pattern in it. Three is the
/// finest that still rules. The Dot screen needs no floor — its cell is a
/// whole square of the chessboard, and at Size 1 that is one pixel, which is
/// the finest chessboard there is and the pattern CS6 draws there.
const HALFTONE_MIN_CELL: f32 = 3.0;

/// How many steps of Contrast double the steepness of the curve the tone is
/// read through.
///
/// Contrast is not the *edge* of a mark, it is how hard the picture is pushed
/// against the screen. Wound right down the screen only shades the picture and
/// the greys survive; wound up it cuts them away until nothing is left but ink
/// and paper. Five steps to a doubling is what puts CS6's 0–50 over that whole
/// run, and it is the number the picture is most sensitive to. At its default
/// of 5 the curve is exactly twice as steep as the screen, which is the
/// setting at which a mid grey — and only a mid grey — comes back as the full
/// black-and-white chessboard, a lighter tone as a chessboard of white against
/// a light grey, a darker one as black against a dark grey. Steeper than that
/// and whole bands of tone collapse onto the same flat chessboard, which is
/// the posterised sheet CS6 does not draw. It was fitted against CS6's own
/// output on `samples/horse-3.jpg` at Size 1, Contrast 5.
const HALFTONE_CONTRAST_DOUBLING: f32 = 5.0;


/// Filter ▸ Sketch ▸ Halftone Pattern: the picture ruled into a screen of
/// dots, rings or lines, in the two swatches.
///
/// The screen is a **threshold laid over the picture**: every pixel carries a
/// tone the screen asks it to beat — little where a mark falls, much where the
/// paper is meant to show — and how far the picture's own tone clears that
/// threshold is how much ink the pixel takes. Clear it by a long way and the
/// pixel is solid foreground, miss by a long way and it is bare background,
/// and in between it is a blend of the two.
///
/// That in-between is the filter's whole character and the thing it is easy to
/// get wrong. CS6 at a low Contrast does *not* come back as two colours: the
/// screen shades the photograph, the horse keeps its modelling and the sky
/// keeps its greys, and what the pattern adds is a texture over the top.
/// Contrast is what cuts the greys away — by the top of the slider nothing is
/// left but ink and paper, which is the two-tone halftone. A screen that
/// thresholds hard whatever Contrast says throws away nine tenths of the
/// picture and looks nothing like the original.
///
/// The Dot screen is a **chessboard of whole squares, square to the picture**
/// — Photoshop's own transparency grid is the thing it looks like. Nothing
/// grows inside a square: the two squares of the board ask opposite things of
/// the tone, so what changes is their *shade*, from pale squares on white
/// through the black-and-white chessboard a mid grey fuses into, on to black
/// squares with dark grey between them. It is worth being plain about what it
/// is **not**: there is no round dot swelling with the tone, and no screen
/// turned to 45°. That is the printer's halftone, and in CS6 it is a
/// different filter — Pixelate ▸ Color Halftone, in `filters::pixelate`.
///
/// **Pattern Type** is the shape of the screen — the chessboard of squares,
/// concentric rings about the middle of the picture, or parallel horizontal
/// lines. **Size** is how big a square, a ring or a band is, in pixels, so
/// Size 1 gives the finest chessboard there is, of single pixels.
/// **Contrast** is how hard the picture is pushed against the screen, from a
/// shading that leaves the greys to a cut that leaves none.
///
/// Alpha is left alone.
///
/// No GPU path, for Bas Relief's reasons: the screen is laid by where on the
/// canvas a pixel is, and the work per pixel is one pattern sample.
pub fn halftone_pattern(
    pixmap: &mut Pixmap,
    size: u32,
    contrast: u32,
    pattern: HalftonePattern,
    foreground: Rgba8,
    background: Rgba8,
) {
    if pixmap.is_empty() {
        return;
    }
    let size = size.clamp(*HALFTONE_SIZE.start(), *HALFTONE_SIZE.end()) as f32;
    let contrast = contrast.clamp(*HALFTONE_CONTRAST.start(), *HALFTONE_CONTRAST.end()) as f32;
    // How steeply the tone is read against the screen. At the bottom of the
    // slider the curve is a straight line — the screen shades the picture and
    // the greys come through — and every few steps doubles it, so the top of
    // the slider is a cut with nothing between ink and paper.
    let gain = (contrast / HALFTONE_CONTRAST_DOUBLING).exp2();
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    let cell = match pattern {
        // A square of the chessboard, in pixels, straight off the slider —
        // Size 4 is a four-pixel square, and Size 1 a single pixel.
        HalftonePattern::Dot => size,
        HalftonePattern::Line | HalftonePattern::Circle => size.max(HALFTONE_MIN_CELL),
    };
    // Half a pixel's share of the tone. A fine screen holds so few pixels per
    // cell that a mark takes a pixel whole or not at all, and the tones it can
    // actually strike are a short ladder. Reading the tone against the middle
    // of each rung rather than its bottom puts them where they belong:
    // without it the palest grey there is already inks half the sheet, and
    // with the ends left uncompressed black never closes up. A Line's ladder
    // and a Circle's run across the cell, so they have as many rungs as the
    // cell is pixels wide. The Dot screen's has two rungs whatever the Size,
    // because the board is two squares standing for two tones — so its
    // thresholds come out at a quarter and three quarters, and a square is a
    // square of the chessboard from the moment the tone reaches it.
    let rung = match pattern {
        HalftonePattern::Dot => 0.25,
        HalftonePattern::Line | HalftonePattern::Circle => 0.5 / cell,
    };
    let (mid_x, mid_y) = (w as f32 / 2.0, h as f32 / 2.0);

    let ink = [foreground.r as f32, foreground.g as f32, foreground.b as f32];
    let paper = [background.r as f32, background.g as f32, background.b as f32];

    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let tone = (0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32)
                    / 255.0;
                let darkness = 1.0 - tone;
                // The tone this pixel has to beat to take ink: little where a
                // mark falls, much where the paper is meant to show. Where
                // the threshold stands is *how much of the cell* takes ink at
                // or before it, so that a tone laid over a whole cell inks
                // that share of it — the squares and the gaps interlock at a
                // mid grey instead of leaving marks on pale ground. The
                // screen is laid by where on the canvas the pixel is, so the
                // pattern is anchored to the picture rather than to the tone.
                let screen = match pattern {
                    HalftonePattern::Dot => {
                        // A chessboard of squares, square to the picture: one
                        // square of the pair asks little of the tone and
                        // takes ink early, its neighbour asks everything and
                        // holds out. So a light grey comes back as pale
                        // squares on white and a mid grey as the
                        // black-and-white chessboard the eye fuses into that
                        // grey. This is the pattern CS6 draws; a screen with
                        // a dot growing in it — the printer's kind — is the
                        // wrong filter, that is Pixelate ▸ Color Halftone.
                        //
                        // A cosine either way rules the board: half a turn to
                        // a square, so the squares are `size` across and the
                        // board comes back round every two of them. A cosine
                        // and not a step, because CS6's squares are **soft**:
                        // seen close up they are rounded, brightest across
                        // the middle and fading into the join, and a board of
                        // flat tiles reads as pixel art instead. What keeps
                        // them squares rather than round dots is `rung` — the
                        // two squares stand for two tones, so a square is a
                        // square of the chessboard as soon as the tone
                        // reaches it, and only the rim is left in between.
                        use std::f32::consts::PI;
                        let wave = (PI * x as f32 / cell).cos() * (PI * y as f32 / cell).cos();
                        0.5 - 0.5 * wave
                    }
                    HalftonePattern::Line => {
                        let fy = ((y as f32 + 0.5) / cell).fract() - 0.5;
                        fy.abs() * 2.0
                    }
                    HalftonePattern::Circle => {
                        let dx = x as f32 - mid_x;
                        let dy = y as f32 - mid_y;
                        let along = ((dx * dx + dy * dy).sqrt() / cell).fract();
                        (along - 0.5).abs() * 2.0
                    }
                };
                // Stood in the middle of the rung it speaks for (see `rung`),
                // so a fine screen's few steps land where they belong.
                let screen = rung + screen.clamp(0.0, 1.0) * (1.0 - 2.0 * rung);
                // How far the picture clears the screen, read through the
                // contrast curve: a grey that only just clears it stays a
                // grey until Contrast is wound up enough to cut it away.
                let coverage = ((darkness - screen) * gain + 0.5).clamp(0.0, 1.0);
                for (c, (dark, pale)) in ink.iter().zip(paper.iter()).enumerate() {
                    px[c] = (pale + (dark - pale) * coverage).round().clamp(0.0, 255.0) as u8;
                }
                // Alpha stands: ruling the picture does not change the layer's
                // shape.
            }
        });
}

/// CS6's ranges for Note Paper, which its three sliders run over.
pub const NOTE_BALANCE: std::ops::RangeInclusive<u32> = 0..=50;
pub const NOTE_GRAININESS: std::ops::RangeInclusive<u32> = 0..=20;
pub const NOTE_RELIEF: std::ops::RangeInclusive<u32> = 0..=25;

/// How far the picture is softened before it is cut into paper and ink, in
/// pixels. Enough that the cut follows the shapes rather than every speck of
/// the photograph, not so much that CS6's ragged mane comes out as blobs.
const NOTE_SMOOTH: f32 = 1.0;

/// How wide the cut between paper and ink is, as a share of the tone range.
/// A hard step would alias; this is about a pixel of anti-aliasing.
const NOTE_CUT_SOFT: f32 = 0.04;

/// How far the ink is taken from the background towards the foreground.
///
/// Note Paper's ink is not ink: it is the holes in the top sheet, and what
/// shows through them is a mid-light grey, not black. Measured off CS6 on
/// `samples/horse-3.jpg` with black and white swatches — the horse comes out
/// at about 172, which is a third of the way to black.
const NOTE_INK: f32 = 0.33;

/// How softly the edge of a hole is rounded before it is lit, in pixels. This
/// is the width of the shadow line along the top of each hole.
const NOTE_WALL: f32 = 0.9;

/// How deep a hole is, in the same units as the grain.
const NOTE_DEPTH: f32 = 1.0;

/// How tall the paper's fibres stand, per step of Graininess.
const NOTE_GRAIN_PER_STEP: f32 = 0.007;

/// How big a fibre is, in pixels. CS6's are a couple of pixels across and a
/// little longer than they are tall, so the noise is blurred and then drawn
/// out along the row.
const NOTE_FIBRE: f32 = 0.8;

/// How steeply the surface is lit, per step of Relief.
const NOTE_RELIEF_PER_STEP: f32 = 0.18;

/// How much a slope darkens whichever way it faces, as a power of its
/// tilt. A sheet of rough paper is greyer than a flat one — every fibre
/// shades its neighbours — and CS6's paper at a heavy grain sits well below
/// white, with only the flat tops of the fibres catching it. Lambert alone
/// lights the half of the fibres facing the lamp brighter than flat, which
/// leaves the paper white with dark specks instead.
const NOTE_OCCLUSION: f32 = 3.0;

/// Where the light comes from: above and a little to the left, low over the
/// sheet. CS6 has no Light control on this filter, and its holes are shadowed
/// along the top edge and, more faintly, the left.
const NOTE_LIGHT: (f32, f32, f32) = (-0.3, -1.0, 1.2);

/// Filter ▸ Sketch ▸ Note Paper: the picture cut out of a sheet of handmade
/// paper, the dark parts showing through as holes onto a sheet beneath.
///
/// The picture is softened a little and **cut in two** at the tone **Image
/// Balance** asks for: whatever is darker than it becomes a hole, the rest
/// stays paper. Low values leave only the deepest shadows as holes, high
/// ones punch out everything but the highlights. The holes are a light grey
/// — a third of the way from the background to the foreground — and the
/// paper is the background itself.
///
/// The whole sheet is then read as a **surface** — the paper standing a step
/// above the holes, with a fibrous grain laid over both whose height is
/// **Graininess** — and lit from above. **Relief** is how steeply that
/// surface stands: at 0 the sheet is flat and the grain is invisible; as it
/// rises the top edge of every hole falls into shadow and the fibres cast
/// their own. A surface that is bumpy all over catches less light on average
/// than a flat one, so a heavy grain greys the paper as it does in CS6.
///
/// Alpha is left alone.
///
/// No GPU path, for Bas Relief's reasons: a small blur, a threshold, a
/// noise field and one lit normal per pixel — the upload and read-back would
/// cost more than the arithmetic. The grain is laid by where on the canvas a
/// pixel is, so a preview crop cannot be filtered on its own either.
pub fn note_paper(
    pixmap: &mut Pixmap,
    balance: u32,
    graininess: u32,
    relief: u32,
    foreground: Rgba8,
    background: Rgba8,
) {
    if pixmap.is_empty() {
        return;
    }
    let balance = balance.clamp(*NOTE_BALANCE.start(), *NOTE_BALANCE.end()) as f32;
    let graininess = graininess.clamp(*NOTE_GRAININESS.start(), *NOTE_GRAININESS.end()) as f32;
    let relief = relief.clamp(*NOTE_RELIEF.start(), *NOTE_RELIEF.end()) as f32;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);

    // Paper or hole: the softened brightness against the cut. Image Balance
    // runs the cut from black at 0 to white at 50.
    let mut tone: Vec<f32> = pixmap
        .as_bytes()
        .par_chunks_exact(4)
        .map(|p| (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) / 255.0)
        .collect();
    blur_field(&mut tone, w, h, NOTE_SMOOTH);
    let cut = balance / *NOTE_BALANCE.end() as f32;
    let paper: Vec<f32> = tone
        .par_iter()
        .map(|&t| ((t - cut) / NOTE_CUT_SOFT + 0.5).clamp(0.0, 1.0))
        .collect();

    // The surface: the paper a step above the holes, its edges rounded, and
    // the fibres over the top of both.
    let mut surface: Vec<f32> = paper.iter().map(|&p| p * NOTE_DEPTH).collect();
    blur_field(&mut surface, w, h, NOTE_WALL);
    if graininess > 0.0 {
        let mut fibre: Vec<f32> = (0..w * h)
            .into_par_iter()
            .map(|i| crate::filters::artistic::noise((i % w) as i32, (i / w) as i32 + 7919) - 0.5)
            .collect();
        blur_field(&mut fibre, w, h, NOTE_FIBRE);
        // Drawn out along the row, so a fibre is longer than it is tall.
        let mut drawn: Vec<f32> = (0..w * h)
            .into_par_iter()
            .map(|i| {
                let x = i % w;
                let left = if x > 0 { fibre[i - 1] } else { fibre[i] };
                let right = if x + 1 < w { fibre[i + 1] } else { fibre[i] };
                (left + fibre[i] + right) / 3.0
            })
            .collect();
        // Blurring white noise flattens it; `unit_spread` puts it back to a
        // known height so Graininess means the same thing at every blur.
        unit_spread(&mut drawn);
        let height = graininess * NOTE_GRAIN_PER_STEP;
        surface.par_iter_mut().zip(drawn.par_iter()).for_each(|(s, f)| *s += f * height);
    }

    let steep = relief * NOTE_RELIEF_PER_STEP;
    let (lx, ly, lz) = NOTE_LIGHT;
    let length = (lx * lx + ly * ly + lz * lz).sqrt();
    let (lx, ly, lz) = (lx / length, ly / length, lz / length);
    let hole = [
        background.r as f32 + (foreground.r as f32 - background.r as f32) * NOTE_INK,
        background.g as f32 + (foreground.g as f32 - background.g as f32) * NOTE_INK,
        background.b as f32 + (foreground.b as f32 - background.b as f32) * NOTE_INK,
    ];
    let sheet = [background.r as f32, background.g as f32, background.b as f32];
    let (surface, paper) = (&surface, &paper);
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, row)| {
            let up = y.saturating_sub(1);
            let down = (y + 1).min(h - 1);
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let i = y * w + x;
                let left = x.saturating_sub(1);
                let right = (x + 1).min(w - 1);
                let gx = (surface[y * w + right] - surface[y * w + left]) * 0.5;
                let gy = (surface[down * w + x] - surface[up * w + x]) * 0.5;
                // Lambert against the surface's normal, over what a flat
                // sheet would catch, so flat paper is exactly its colour.
                let (nx, ny, nz) = (-gx * steep, -gy * steep, 1.0);
                let tilt = (nx * nx + ny * ny + nz * nz).sqrt();
                let lit = ((nx * lx + ny * ly + nz * lz) / tilt / lz).max(0.0)
                    / tilt.powf(NOTE_OCCLUSION);
                let p = paper[i];
                for c in 0..3 {
                    let base = hole[c] + (sheet[c] - hole[c]) * p;
                    // Past flat, the light has nowhere darker to come from:
                    // a highlight lifts towards white rather than scaling a
                    // colour that is already the paper.
                    let v = if lit <= 1.0 {
                        base * lit
                    } else {
                        base + (255.0 - base) * (lit - 1.0)
                    };
                    px[c] = v.round().clamp(0.0, 255.0) as u8;
                }
                // Alpha stands: cutting the sheet does not change the layer's
                // shape.
            }
        });
}

/// CS6's ranges for Photocopy, which its two sliders run over.
pub const COPY_DETAIL: std::ops::RangeInclusive<u32> = 1..=24;
pub const COPY_DARKNESS: std::ops::RangeInclusive<u32> = 1..=50;

/// How wide the neighbourhood a pixel is compared with is, in pixels of
/// blur: a floor, and how much each step of Detail adds.
///
/// This is the whole of Detail, and it is not what the name suggests. A pixel
/// takes toner when it is darker than the picture *round it*, so a narrow
/// neighbourhood only catches the dark side of every edge — CS6 at Detail 4
/// draws the horse as an outline with its body left white — and a wide one
/// catches whole masses that are darker than their surroundings, which is why
/// at Detail 23 the body comes back solid black.
const COPY_REACH: f32 = 0.6;
const COPY_REACH_PER_STEP: f32 = 1.2;

/// How much toner a level of difference lays, per step of Darkness.
///
/// At the bottom of the slider the difference is shaded in, so the lines are
/// soft and grey and fade into the paper; by the top it is cut, and the copy
/// is toner or paper with nothing between.
const COPY_GAIN_PER_STEP: f32 = 1.1;

/// How much darker than the neighbourhood a pixel has to be before it takes
/// any toner, in levels. A copier does not pick up the faint texture of a
/// flat grey; without this every speck of JPEG noise in a sky comes back as
/// a grey smudge.
const COPY_FLOOR: f32 = 1.5;

/// Filter ▸ Sketch ▸ Photocopy: the picture as a cheap photocopier sees it,
/// in the two swatches.
///
/// A copier does not reproduce tone, it reproduces *change*: a flat area,
/// dark or light, comes back as bare paper, and toner sticks where the
/// picture is darker than what is round it. So each pixel is compared with a
/// blurred copy of the picture, and how far it falls below that is how much
/// toner it takes. **Detail** is how wide that neighbourhood is — narrow
/// draws outlines along the dark side of every edge, wide fills in whole
/// masses that are darker than their surroundings. **Darkness** is how hard
/// the difference is driven, from soft grey shading to a hard cut into
/// toner and paper.
///
/// Toner is the foreground, paper the background. Alpha is left alone.
///
/// No GPU path. The blur is the only real work, and at the top of Darkness a
/// difference of five levels is already full toner, so it has to stay in
/// floating point — the GPU blur works in eight bits, and its rounding would
/// come back as contour lines across every smooth gradient.
pub fn photocopy(
    pixmap: &mut Pixmap,
    detail: u32,
    darkness: u32,
    foreground: Rgba8,
    background: Rgba8,
) {
    if pixmap.is_empty() {
        return;
    }
    let detail = detail.clamp(*COPY_DETAIL.start(), *COPY_DETAIL.end()) as f32;
    let darkness = darkness.clamp(*COPY_DARKNESS.start(), *COPY_DARKNESS.end()) as f32;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);

    let tone: Vec<f32> = pixmap
        .as_bytes()
        .par_chunks_exact(4)
        .map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
        .collect();
    let mut around = tone.clone();
    blur_field(&mut around, w, h, COPY_REACH + detail * COPY_REACH_PER_STEP);

    let gain = darkness * COPY_GAIN_PER_STEP / 255.0;
    let toner = [foreground.r as f32, foreground.g as f32, foreground.b as f32];
    let paper = [background.r as f32, background.g as f32, background.b as f32];
    let (tone, around) = (&tone, &around);
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let i = y * w + x;
                // Only what is darker than its surroundings takes toner; a
                // pixel lighter than them is paper, however dark it is.
                let below = (around[i] - tone[i] - COPY_FLOOR).max(0.0);
                let ink = (below * gain).min(1.0);
                for c in 0..3 {
                    px[c] = (paper[c] + (toner[c] - paper[c]) * ink).round().clamp(0.0, 255.0)
                        as u8;
                }
                // Alpha stands: copying the picture does not change the
                // layer's shape.
            }
        });
}

/// How far Photocopy reaches, in pixels: three sigma of its blur.
pub fn photocopy_reach(detail: u32) -> u32 {
    let detail = detail.clamp(*COPY_DETAIL.start(), *COPY_DETAIL.end()) as f32;
    ((COPY_REACH + detail * COPY_REACH_PER_STEP) * 3.0).ceil() as u32 + 1
}

/// CS6's ranges for Plaster, which its two sliders run over.
pub const PLASTER_BALANCE: std::ops::RangeInclusive<u32> = 0..=50;
pub const PLASTER_SMOOTHNESS: std::ops::RangeInclusive<u32> = 1..=15;

/// How far the picture is softened before it is poured, in pixels: a floor,
/// and how much each step of Smoothness adds. This rounds the outline off —
/// but only the outline's *noise*: CS6 at Smoothness 12 still keeps the
/// mane's spikes and the spray's droplets, and melts only the pixel-level
/// raggedness in between. What makes the top of the slider read as liquid is
/// the wider rim, not a blurrier shape.
const PLASTER_SMOOTH: f32 = 0.6;
const PLASTER_SMOOTH_PER_STEP: f32 = 0.2;

/// How the pools are cut out of the plaster.
///
/// The threshold is taken hard, as a mask of whole pixels, and the mask is
/// then softened by [`PLASTER_EDGE`] and cut again at its middle over
/// [`PLASTER_EDGE_AA`]. Cutting the tone directly leaves the photograph's
/// pixel staircase along every edge; cutting a softened mask puts the edge
/// between pixels, where the shape really runs, and anti-aliases it. The
/// softening is kept small so that the specks CS6 keeps — the white dots on
/// the horse, the spray's droplets — survive it.
const PLASTER_EDGE: f32 = 1.0;
const PLASTER_EDGE_AA: f32 = 0.35;

/// How wide the bevel on every edge is, in pixels of blur, and how much each
/// step of Smoothness widens it — hardly at all: CS6's bevel is about as
/// wide at Smoothness 12 as at 2, and a bevel that grows with the slider
/// turns the top of it into broad, blown-out bands.
///
/// This is the filter's look. The pools stand proud of the plaster, and the
/// mask is blurred into a height field whose shoulders run out past the edge
/// on both sides: eight to twelve pixels of rounded bevel at the bottom of the
/// slider and not much more at the top. Lit, those shoulders are what
/// make CS6's output look poured and three-dimensional rather than outlined.
const PLASTER_BEVEL: f32 = 4.0;
const PLASTER_BEVEL_PER_STEP: f32 = 0.08;

/// How steeply the bevel stands. The blurred mask's slope is only about a
/// tenth of a unit per pixel; this turns it into shoulders tilted most of
/// the way to upright, which is what lets them catch the light hard.
const PLASTER_STEEP: f32 = 14.0;

/// How high the lamp stands over the sheet, as the upward part of the vector
/// towards it — CS6's Light sets only its compass direction.
const PLASTER_ELEVATION: f32 = 0.7;

/// How glossy the plaster is. The highlight is a Blinn–Phong lobe: `SHINE`
/// is its exponent — how tight it is — and `GLOSS` how far it takes a
/// pixel towards the background colour. A wet, glossy bead is what CS6
/// draws; a matte shoulder with diffuse light alone looks like Bas Relief.
const PLASTER_SHINE: f32 = 8.0;
const PLASTER_GLOSS: f32 = 1.4;

/// How hard a shoulder is lifted towards the background colour: `LIFT` by
/// how much more light it catches than flat ground, and `SHEEN` by how
/// steep it is whichever way it faces.
///
/// CS6's lit bevels are broad bands of near-white, six to eight pixels
/// across, not the thin line a specular lobe draws on its own; and they run
/// on round the sides of a shape as well as along the edge squarely facing
/// the lamp — with Light at Top, the horse's legs are bright down their
/// left sides too. The sheen is what carries them round; the shade on the
/// side away from the lamp is what still pulls that side down.
const PLASTER_LIFT: f32 = 2.5;
const PLASTER_SHEEN: f32 = 0.6;

/// How much of the gloss shows on the pools themselves. They are the
/// foreground colour and mostly stay it; CS6 lets a little of the highlight
/// across onto the lip of a pool, and all of it onto a small one.
const PLASTER_POOL_GLOSS: f32 = 0.35;

/// How deep the shade on a shoulder facing away from the lamp goes, towards
/// the foreground colour. Diffuse light alone would take the far side to
/// black; CS6's shade is a darker band on the plaster, not a hole in it.
const PLASTER_SHADE: f32 = 0.6;

/// The thin grey line inside every pool, a few pixels in from its edge: where
/// on the bevel's coordinate it falls (0 deep in a pool, ½ on the edge), how
/// wide it is, and how far it is taken towards the plaster's colour.
const PLASTER_ECHO: (f32, f32) = (0.25, 0.06);
const PLASTER_ECHO_STRENGTH: f32 = 0.3;

/// Filter ▸ Sketch ▸ Plaster: the picture poured in plaster and lit from one
/// side, in the two swatches.
///
/// Whatever is darker than the tone **Image Balance** asks for becomes a pool
/// of the foreground colour, standing proud of the rest, which is the
/// plaster. The mask of the pools is blurred into a height field, so every
/// edge is a wide, rounded **bevel** — see [`PLASTER_BEVEL`] — and the whole
/// surface is lit from **Light** with a glossy highlight: the shoulders that
/// face the lamp catch a bright wet sheen, the ones that face away fall into
/// shade, and together they make the shapes look poured and three-
/// dimensional. **Smoothness** rounds off the outline's pixel noise.
///
/// The plaster is not left flat. CS6 shades it as a single **ramp** across
/// the whole picture, from the background colour at the edge nearest the
/// light to the foreground at the far edge. Measured off CS6 with Light Top,
/// it runs from 253 at the top of the frame to 9 at the bottom, linear in
/// between.
///
/// Alpha is left alone.
///
/// No GPU path, for Bas Relief's reasons: three blurs and a few taps per
/// pixel. The ramp is laid across the whole frame, so a preview crop cannot
/// be filtered on its own.
pub fn plaster(
    pixmap: &mut Pixmap,
    balance: u32,
    smoothness: u32,
    light: Light,
    foreground: Rgba8,
    background: Rgba8,
) {
    if pixmap.is_empty() {
        return;
    }
    let balance = balance.clamp(*PLASTER_BALANCE.start(), *PLASTER_BALANCE.end()) as f32;
    let smoothness =
        smoothness.clamp(*PLASTER_SMOOTHNESS.start(), *PLASTER_SMOOTHNESS.end()) as f32;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);

    // The plaster: the softened brightness against the cut, 1 where it is
    // lighter. Image Balance runs the cut from black to white.
    let mut tone: Vec<f32> = pixmap
        .as_bytes()
        .par_chunks_exact(4)
        .map(|p| (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) / 255.0)
        .collect();
    blur_field(&mut tone, w, h, PLASTER_SMOOTH + smoothness * PLASTER_SMOOTH_PER_STEP);
    let cut = balance / *PLASTER_BALANCE.end() as f32;
    let plaster: Vec<f32> = tone.par_iter().map(|&t| if t >= cut { 1.0 } else { 0.0 }).collect();

    // The edge, anti-aliased: the mask softened a little and cut at its
    // middle, so the outline runs between pixels rather than along them.
    let mut edge = plaster.clone();
    blur_field(&mut edge, w, h, PLASTER_EDGE);
    // The bevel's coordinate: 1 in open plaster, 0 deep in a pool, a half on
    // the edge, running smoothly across the bevel's whole width.
    let mut bevel = plaster;
    blur_field(&mut bevel, w, h, PLASTER_BEVEL + smoothness * PLASTER_BEVEL_PER_STEP);
    // The height. The pools stand proud and their tops are flat right out to
    // the edge: the whole slope lies on the plaster side of it. That is
    // where CS6 draws the relief — a bright band on the plaster along the
    // edges facing the lamp, a shaded one along the edges facing away — and
    // it leaves the pools a flat, clean black. A slope straddling the edge
    // puts half the highlight on the pools and turns them into glossy blobs.
    // Eased at the top, so the edge is a rounded lip rather than a crease.
    let height: Vec<f32> = bevel
        .par_iter()
        .map(|&g| {
            let t = (2.0 * (1.0 - g)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        })
        .collect();

    // The ramp: how far along the line away from the light a pixel is, from
    // 0 at the corner nearest the lamp to 1 at the one furthest from it.
    let (lx, ly) = light.towards();
    let (ax, ay) = (-lx, -ly);
    let corners = [
        0.0,
        ax * (w - 1) as f32,
        ay * (h - 1) as f32,
        ax * (w - 1) as f32 + ay * (h - 1) as f32,
    ];
    let near = corners.iter().copied().fold(f32::INFINITY, f32::min);
    let far = corners.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let span = (far - near).max(1.0);

    // The lamp, and the half-way vector between it and a viewer straight
    // above, for the highlight. A flat surface catches some of the lobe too;
    // that much is taken off so flat plaster is exactly the ramp.
    let lamp = {
        let n = (lx * lx + ly * ly + PLASTER_ELEVATION * PLASTER_ELEVATION).sqrt();
        (lx / n, ly / n, PLASTER_ELEVATION / n)
    };
    let half = {
        let (hx, hy, hz) = (lamp.0, lamp.1, lamp.2 + 1.0);
        let n = (hx * hx + hy * hy + hz * hz).sqrt();
        (hx / n, hy / n, hz / n)
    };
    let flat_sheen = half.2.powf(PLASTER_SHINE);

    let ink = [foreground.r as f32, foreground.g as f32, foreground.b as f32];
    let paper = [background.r as f32, background.g as f32, background.b as f32];
    let (bevel, edge, height) = (&bevel, &edge, &height);
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, row)| {
            let up = y.saturating_sub(1);
            let down = (y + 1).min(h - 1);
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let i = y * w + x;
                let g = bevel[i];
                let e = edge[i];
                let p = ((0.5 - e) / PLASTER_EDGE_AA + 0.5).clamp(0.0, 1.0);
                // The surface's normal, leaning down the slope.
                let gx = (height[y * w + (x + 1).min(w - 1)]
                    - height[y * w + x.saturating_sub(1)])
                    * 0.5;
                let gy = (height[down * w + x] - height[up * w + x]) * 0.5;
                let (nx, ny, nz) = (-gx * PLASTER_STEEP, -gy * PLASTER_STEEP, 1.0);
                let n = (nx * nx + ny * ny + nz * nz).sqrt();
                let (nx, ny, nz) = (nx / n, ny / n, nz / n);
                // Diffuse against what flat ground catches: 1 on the flat,
                // above it on a shoulder facing the lamp, below it on one
                // facing away.
                let diffuse = (nx * lamp.0 + ny * lamp.1 + nz * lamp.2).max(0.0) / lamp.2;
                let shade = ((1.0 - diffuse).max(0.0) * PLASTER_SHADE).min(1.0);
                let lift = ((diffuse - 1.0).max(0.0) * PLASTER_LIFT + (1.0 - nz) * PLASTER_SHEEN)
                    .min(1.0);
                let sheen = (nx * half.0 + ny * half.1 + nz * half.2).max(0.0).powf(PLASTER_SHINE);
                let gloss = ((sheen - flat_sheen).max(0.0) / (1.0 - flat_sheen) * PLASTER_GLOSS)
                    .min(1.0)
                    * (1.0 - p * (1.0 - PLASTER_POOL_GLOSS));
                // The echo, inside the pool only, tailing off to nothing in
                // its middle so a pool far from any edge stays flat.
                let echo = (-((g - PLASTER_ECHO.0) / PLASTER_ECHO.1).powi(2)).exp()
                    * PLASTER_ECHO_STRENGTH
                    * (g * 4.0).min(1.0)
                    * p;

                let along = ((x as f32 * ax + y as f32 * ay) - near) / span;
                for c in 0..3 {
                    let ramp = paper[c] + (ink[c] - paper[c]) * along;
                    let mut v = ramp + (ink[c] - ramp) * p;
                    v += (ramp - v) * echo;
                    v += (ink[c] - v) * shade;
                    v += (paper[c] - v) * lift;
                    v += (paper[c] - v) * gloss;
                    px[c] = v.round().clamp(0.0, 255.0) as u8;
                }
                // Alpha stands: pouring the picture does not change the
                // layer's shape.
            }
        });
}

/// CS6's ranges for Reticulation, which its three sliders run over.
pub const RETIC_DENSITY: std::ops::RangeInclusive<u32> = 0..=50;
pub const RETIC_FOREGROUND: std::ops::RangeInclusive<u32> = 0..=50;
pub const RETIC_BACKGROUND: std::ops::RangeInclusive<u32> = 0..=50;

/// How big a clump of grain is, in pixels of blur, and how much finer each
/// step of Density makes it. CS6's clumps are worms two or three pixels
/// across, packed into a labyrinth: dark worms on white in the highlights,
/// grey worms parted by thin black gaps in the shadows.
///
/// The grain is white noise band-passed — blurred by this, less the same
/// noise blurred by twice this — which is what packs it into worms of one
/// size rather than soft blobs of every size. Plain blurred noise reads as a
/// grey fog next to CS6.
const RETIC_CLUMP: f32 = 1.15;
const RETIC_CLUMP_PER_STEP: f32 = 0.006;

/// How far the grain pushes the tone, as a share of the tone range: a floor,
/// and how much each step of Density adds.
const RETIC_GRAIN: f32 = 0.167;
const RETIC_GRAIN_PER_STEP: f32 = 0.0032;

/// How far Density lifts the darkest tones before the grain is added, per
/// step. Without it a shadow sits below the curve's black point, half of
/// the grain is clipped away, and the horse sets solid black; CS6's shadows
/// keep a lighter web through them that thickens with Density.
const RETIC_LIFT_PER_STEP: f32 = 0.004;

/// The tone curve the grained picture is read through. Foreground Level
/// moves its black point up and bends the midtones down — at 50 the sea sets
/// dark while the sky barely moves — and Background Level brings its white
/// point down, washing the highlights out.
const RETIC_BLACK: f32 = 0.051;
const RETIC_BLACK_PER_STEP: f32 = 0.0086;
const RETIC_BEND: f32 = 0.619;
const RETIC_BEND_PER_STEP: f32 = 0.0542;
const RETIC_WHITE: f32 = 1.127;
const RETIC_WHITE_PER_STEP: f32 = 0.0085;

/// How far towards each swatch the result reaches. CS6's grain never quite
/// sets solid: the darkest clump keeps a trace of the background colour and
/// the palest a trace of the foreground.
const RETIC_DEEPEST: f32 = 0.068;
const RETIC_PALEST: f32 = 0.938;

// Every constant above was fitted together, against the 5th, 25th, 50th,
// 75th and 95th percentiles of CS6's output over the sky, the horse, the sea
// and the sand of `samples/horse-3.jpg` at Density / Foreground / Background
// of 5/10/5, 22/26/24 and 22/50/24 — sixty numbers, matched to about ten
// levels each, with the grain made as described at [`RETIC_CLUMP`]. Change
// one and the others are no longer the fit.

/// Filter ▸ Sketch ▸ Reticulation: the picture as film whose emulsion has
/// clumped, in the two swatches.
///
/// Every pixel's tone is shaken by a **grain** of small, worm-like clumps —
/// white noise blurred to a couple of pixels, so it clumps rather than
/// speckles — and the result is read through a levels curve into the two
/// swatches. The grain rides on the picture rather than replacing it: the
/// sky comes out light with dark worms through it, the horse dark with a
/// lighter web, and midtones a close mesh of the two.
///
/// The picture is read through a tone curve first, and the grain shakes
/// the result. **Density** is how hard the grain shakes it, how fine it is,
/// and how far it lightens the deepest shadows so that a web of grain
/// still shows through them. **Foreground Level** darkens the curve's
/// midtones, setting more of them in the foreground colour. **Background
/// Level** washes the highlights out towards clean background. The result
/// is continuous tone, not two colours — CS6's clumps have soft grey edges,
/// and so does this.
///
/// Alpha is left alone.
///
/// No GPU path, for Bas Relief's reasons: one small blur of noise and a
/// curve per pixel. The grain is laid by where on the canvas a pixel is, so
/// a preview crop cannot be filtered on its own.
pub fn reticulation(
    pixmap: &mut Pixmap,
    density: u32,
    foreground_level: u32,
    background_level: u32,
    foreground: Rgba8,
    background: Rgba8,
) {
    if pixmap.is_empty() {
        return;
    }
    let density = density.clamp(*RETIC_DENSITY.start(), *RETIC_DENSITY.end()) as f32;
    let foreground_level =
        foreground_level.clamp(*RETIC_FOREGROUND.start(), *RETIC_FOREGROUND.end()) as f32;
    let background_level =
        background_level.clamp(*RETIC_BACKGROUND.start(), *RETIC_BACKGROUND.end()) as f32;
    let black = RETIC_BLACK + foreground_level * RETIC_BLACK_PER_STEP;
    let white = RETIC_WHITE - background_level * RETIC_WHITE_PER_STEP;
    let bend = RETIC_BEND + foreground_level * RETIC_BEND_PER_STEP;
    let lift = density * RETIC_LIFT_PER_STEP;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);

    // The grain: white noise band-passed into worms of one size, and put back
    // to a known spread so Density means the same thing however fine the
    // worms are.
    let noise: Vec<f32> = (0..w * h)
        .into_par_iter()
        .map(|i| crate::filters::artistic::noise((i % w) as i32, (i / w) as i32 + 104_729) - 0.5)
        .collect();
    let clump = RETIC_CLUMP - density * RETIC_CLUMP_PER_STEP;
    let mut grain = noise.clone();
    blur_field(&mut grain, w, h, clump);
    let mut broad = noise;
    blur_field(&mut broad, w, h, clump * 2.0);
    grain.par_iter_mut().zip(broad.par_iter()).for_each(|(g, b)| *g -= b);
    unit_spread(&mut grain);
    let amount = RETIC_GRAIN + density * RETIC_GRAIN_PER_STEP;
    let span = (white - black).max(1e-3);

    let ink = [foreground.r as f32, foreground.g as f32, foreground.b as f32];
    let paper = [background.r as f32, background.g as f32, background.b as f32];
    let grain = &grain;
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let tone = (0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32)
                    / 255.0;
                let curved = ((tone - black) / span).clamp(0.0, 1.0).powf(bend);
                let curved = lift + (1.0 - lift) * curved;
                let shaken = (curved + grain[y * w + x] * amount).clamp(0.0, 1.0);
                let light = RETIC_DEEPEST + (RETIC_PALEST - RETIC_DEEPEST) * shaken;
                for c in 0..3 {
                    px[c] = (ink[c] + (paper[c] - ink[c]) * light).round().clamp(0.0, 255.0) as u8;
                }
                // Alpha stands: graining the picture does not change the
                // layer's shape.
            }
        });
}

/// CS6's ranges for Stamp, which its two sliders run over.
pub const STAMP_BALANCE: std::ops::RangeInclusive<u32> = 0..=50;
pub const STAMP_SMOOTHNESS: std::ops::RangeInclusive<u32> = 1..=50;

/// How far the picture is melted before it is cut, in pixels of blur per
/// step of Smoothness. At 5 the horse keeps its mane and the highlights on
/// its flank; at 24 it is a rounded silhouette; by 47 the whole frame is a
/// few great soft shapes.
const STAMP_MELT_PER_STEP: f32 = 0.35;

/// Where the cut falls, as a share of the tone range: at the bottom of
/// Light/Dark Balance, and how far each step raises it.
///
/// The slider covers less of the tone range than it looks: under a quarter
/// of the way up at 0, about two thirds at 50. Read off CS6 on
/// `samples/horse-3.jpg` — at 9 the horse is already mostly ink, with only
/// its highlights left as paper; at 33 the dark streaks of the sea have
/// joined it; at 48 the whole sea has, and the pale sky and the spray are
/// still paper.
const STAMP_CUT: f32 = 0.225;
const STAMP_CUT_PER_STEP: f32 = 0.0083;

/// How hard local contrast is pushed before the melt, and over how wide a
/// neighbourhood, in pixels of blur. A stamp picks up a thin dark line on a
/// light ground — CS6 at Smoothness 5 inks every streak of foam-shadow across
/// the sand — even where the line is not darker than the cut on its own; it
/// is darker than what is round it. Sharpening first is what lets it through.
/// At high Smoothness the melt blurs the sharpening away again, as CS6's
/// great soft shapes show nothing of it.
const STAMP_LOCAL: f32 = 2.0;
const STAMP_LOCAL_REACH: f32 = 6.0;

/// How wide the cut between ink and paper is, as a share of the tone range.
/// Where an edge is steep that is about a pixel of anti-aliasing; where two
/// shapes almost meet the tone is shallow, and the same width becomes the
/// soft grey neck CS6 draws between them.
const STAMP_CUT_SOFT: f32 = 0.015;

/// Filter ▸ Sketch ▸ Stamp: the picture cut as a rubber stamp, in the two
/// swatches.
///
/// The picture's brightness is sharpened a little, so thin dark lines hold
/// — see [`STAMP_LOCAL`] — then melted by **Smoothness** — a Gaussian blur,
/// which is what rounds every shape off the way a stamp's are — and then
/// cut at the tone **Light/Dark Balance** asks for: darker than the cut
/// takes the foreground, lighter the background. Raising the balance moves
/// the cut up the tones and inks more of the picture.
///
/// Alpha is left alone.
///
/// No GPU path. The blur is the only real work and would fit, but the cut
/// is read off it at a width of a few levels, so it has to stay in floating
/// point: the GPU blur works in eight bits, and its rounding would come back
/// as stair-steps along every edge. The blur is local, so a preview crop is
/// filtered with enough margin round it — see `Filter::reach`.
pub fn stamp(
    pixmap: &mut Pixmap,
    balance: u32,
    smoothness: u32,
    foreground: Rgba8,
    background: Rgba8,
) {
    if pixmap.is_empty() {
        return;
    }
    let balance = balance.clamp(*STAMP_BALANCE.start(), *STAMP_BALANCE.end()) as f32;
    let smoothness = smoothness.clamp(*STAMP_SMOOTHNESS.start(), *STAMP_SMOOTHNESS.end()) as f32;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);

    let mut tone: Vec<f32> = pixmap
        .as_bytes()
        .par_chunks_exact(4)
        .map(|p| (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) / 255.0)
        .collect();
    let mut around = tone.clone();
    blur_field(&mut around, w, h, STAMP_LOCAL_REACH);
    tone.par_iter_mut()
        .zip(around.par_iter())
        .for_each(|(t, a)| *t += (*t - a) * STAMP_LOCAL);
    blur_field(&mut tone, w, h, smoothness * STAMP_MELT_PER_STEP);
    let cut = STAMP_CUT + balance * STAMP_CUT_PER_STEP;

    let ink = [foreground.r as f32, foreground.g as f32, foreground.b as f32];
    let paper = [background.r as f32, background.g as f32, background.b as f32];
    let tone = &tone;
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let light = ((tone[y * w + x] - cut) / STAMP_CUT_SOFT + 0.5).clamp(0.0, 1.0);
                for c in 0..3 {
                    px[c] = (ink[c] + (paper[c] - ink[c]) * light).round().clamp(0.0, 255.0) as u8;
                }
                // Alpha stands: stamping the picture does not change the
                // layer's shape.
            }
        });
}

/// How far Stamp reaches, in pixels: three sigma of the sharpening's
/// neighbourhood and of the melt, one after the other.
pub fn stamp_reach(smoothness: u32) -> u32 {
    let smoothness = smoothness.clamp(*STAMP_SMOOTHNESS.start(), *STAMP_SMOOTHNESS.end()) as f32;
    ((STAMP_LOCAL_REACH + smoothness * STAMP_MELT_PER_STEP) * 3.0).ceil() as u32 + 1
}

/// CS6's ranges for Torn Edges, which its three sliders run over.
pub const TORN_BALANCE: std::ops::RangeInclusive<u32> = 0..=50;
pub const TORN_SMOOTHNESS: std::ops::RangeInclusive<u32> = 1..=15;
pub const TORN_CONTRAST: std::ops::RangeInclusive<u32> = 1..=25;

/// Where the cut falls, as a share of the tone range: at the bottom of Image
/// Balance, and how far each step raises it. Read off CS6 on
/// `samples/horse-3.jpg` — at 12 the horse is inked but its highlights are
/// not; at 25 the horse and a few of the darkest streaks of the sea; at 41
/// the whole sea, with the pale sky still paper.
const TORN_CUT: f32 = 0.06;
const TORN_CUT_PER_STEP: f32 = 0.018;

/// How far the edge of the inked mask is spread, in pixels: this over
/// Smoothness. Named backwards from what it looks like — CS6 at Smoothness 3
/// has broad, fuzzy, torn-felt edges, and at 13 crisp ragged ones — because
/// what Smoothness smooths is the *tear*: the higher it is, the less the
/// edge frays out.
const TORN_SPREAD: f32 = 18.0;

/// How hard the paper's grain pulls the edge about. Across the spread-out
/// edge every pixel is pushed in or out of the ink at random, which is what
/// turns a clean boundary into a torn, fibrous one. Deep inside a shape, and
/// far out in the paper, the grain is not strong enough to change anything,
/// so the ink stays solid and the paper clean — which is what CS6 draws at
/// every setting but the very top of Contrast.
const TORN_RAG: f32 = 0.8;

/// How big a grain of the paper is, in pixels of blur over white noise.
/// CS6's fibres and flecks are clumps two or three pixels across.
const TORN_GRAIN: f32 = 0.7;

/// How steeply the frayed mask is cut into ink and paper, as a floor and per
/// step of Contrast. Low, and the torn fringe is a soft grey blur; high, and
/// there is nothing between ink and paper.
const TORN_GAIN: f32 = 2.0;
const TORN_GAIN_PER_STEP: f32 = 0.6;

/// How deep the grain bites holes right through the ink at the top of
/// Contrast. CS6 shows none at 17 and half the ink gone at 25, so the bite
/// comes in steeply: this, times the Contrast slider's position to the power
/// [`TORN_HOLE_ONSET`].
const TORN_HOLE: f32 = 1.2;
const TORN_HOLE_ONSET: i32 = 8;

/// Filter ▸ Sketch ▸ Torn Edges: the picture torn out of paper, in the two
/// swatches.
///
/// Whatever is darker than the tone **Image Balance** asks for is inked, in
/// the foreground colour; the rest is paper, the background. The edge of the
/// inked mask is spread out by a blur that **Smoothness** narrows, and the
/// paper's grain pushes each pixel across it in or out at random — see
/// [`TORN_RAG`] — so the boundary tears into fibres while the shapes stay
/// solid and the paper stays clean. **Contrast** is how cleanly that is cut:
/// a soft grey fringe at the bottom, pure ink and paper higher up, and at the
/// very top the grain bites holes right through the ink.
///
/// Alpha is left alone.
///
/// No GPU path, for Bas Relief's reasons: two small blurs and a curve per
/// pixel. The grain is laid by where on the canvas a pixel is, so a preview
/// crop cannot be filtered on its own.
pub fn torn_edges(
    pixmap: &mut Pixmap,
    balance: u32,
    smoothness: u32,
    contrast: u32,
    foreground: Rgba8,
    background: Rgba8,
) {
    if pixmap.is_empty() {
        return;
    }
    let balance = balance.clamp(*TORN_BALANCE.start(), *TORN_BALANCE.end()) as f32;
    let smoothness = smoothness.clamp(*TORN_SMOOTHNESS.start(), *TORN_SMOOTHNESS.end()) as f32;
    let contrast = contrast.clamp(*TORN_CONTRAST.start(), *TORN_CONTRAST.end()) as f32;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);

    // The inked mask, cut at the balance and frayed by the blur.
    let cut = TORN_CUT + balance * TORN_CUT_PER_STEP;
    let mut mask: Vec<f32> = pixmap
        .as_bytes()
        .par_chunks_exact(4)
        .map(|p| {
            let tone = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) / 255.0;
            if tone < cut {
                1.0
            } else {
                0.0
            }
        })
        .collect();
    blur_field(&mut mask, w, h, TORN_SPREAD / smoothness);

    // The paper's grain: 0..1, blurred into small clumps and stretched back
    // out to its old spread so the bite means the same at any clump size.
    let mut grain: Vec<f32> = (0..w * h)
        .into_par_iter()
        .map(|i| crate::filters::artistic::noise((i % w) as i32, (i / w) as i32 + 15_485) - 0.5)
        .collect();
    blur_field(&mut grain, w, h, TORN_GRAIN);
    unit_spread(&mut grain);
    // White noise over 0..1 has a spread of 1/√12.
    grain.par_iter_mut().for_each(|g| *g = (0.5 + *g * 0.2887).clamp(0.0, 1.0));
    let grain = &grain;

    let top = (contrast - *TORN_CONTRAST.start() as f32)
        / (*TORN_CONTRAST.end() - *TORN_CONTRAST.start()) as f32;
    let hole = TORN_HOLE * top.powi(TORN_HOLE_ONSET);
    let gain = TORN_GAIN + contrast * TORN_GAIN_PER_STEP;
    let ink = [foreground.r as f32, foreground.g as f32, foreground.b as f32];
    let paper = [background.r as f32, background.g as f32, background.b as f32];
    let mask = &mask;
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let i = y * w + x;
                let g = grain[i];
                // The grain pulls only where there is an edge to tear: open
                // paper and the solid middle of a shape have none, so a soft
                // low-Contrast cut leaves neither a grey cast on the paper
                // nor one in the ink.
                let m = mask[i];
                let edge = (4.0 * m * (1.0 - m)).max(0.0).sqrt();
                let torn = m + (g - 0.5) * TORN_RAG * edge - g * hole;
                let inked = ((torn - 0.5) * gain + 0.5).clamp(0.0, 1.0);
                for c in 0..3 {
                    px[c] = (paper[c] + (ink[c] - paper[c]) * inked).round().clamp(0.0, 255.0) as u8;
                }
            // Alpha stands: tearing the picture does not change the
                // layer's shape.
            }
        });
}

/// CS6's ranges for Water Paper, which its three sliders run over.
pub const WATER_FIBER: std::ops::RangeInclusive<u32> = 3..=50;
pub const WATER_BRIGHTNESS: std::ops::RangeInclusive<u32> = 0..=100;
pub const WATER_CONTRAST: std::ops::RangeInclusive<u32> = 0..=100;

/// How far the colour runs along a fibre, in pixels, per step of Fiber
/// Length — the whole run, both ways. Read off CS6 on `samples/horse-3.jpg`:
/// at 15 the mane bleeds in streaks a dozen pixels long, at 30 about twice
/// that.
const WATER_RUN_PER_STEP: f32 = 0.9;

/// How strongly the wettest fibres carry colour over the rest. The fibres'
/// streaked noise has a spread of one, and a pixel's pull along a fibre is
/// `exp` of this times it. Too little and the bleed is an even cross-shaped
/// blur; too much and it is a scribble of dark scratches, which CS6's soft
/// run of colour is not.
const WATER_SOAK: f32 = 0.9;

/// How deep the paper's weave shows through the pigment, in levels at full
/// darkness. The weave shows in the ink and not on bare paper: CS6's dark
/// horse is cross-hatched and its pale sky is smooth.
const WATER_WEAVE: f32 = 18.0;

/// Brightness as a factor of two to the power of its distance from the
/// middle of the slider over this. Below the middle the picture is scaled
/// down by it; above, it is the gamma the picture is lifted by. A lift rather
/// than a scale, because CS6 at 87 turns the brown horse pink and the sky
/// pale blue but leaves the mane black: the mid-tones rise and the tint
/// survives, where scaling would have burnt it all out to white.
const WATER_BRIGHTNESS_DOUBLING: f32 = 30.0;

/// Contrast as a gain about mid-grey: doubling every this many steps from a
/// gain of one at [`WATER_CONTRAST_FLAT`]. CS6 at 23 is flat and murky, at
/// 34 hazy, and at 94 crushes the darks to black.
const WATER_CONTRAST_DOUBLING: f32 = 30.0;
const WATER_CONTRAST_FLAT: f32 = 50.0;

/// Where the highlights start to roll off rather than clip, as a share of the
/// range. At high Contrast CS6's light areas crowd up towards white but keep
/// their colour — pastel, not blown out.
const WATER_SHOULDER: f32 = 0.7;

/// Filter ▸ Sketch ▸ Water Paper: the picture daubed onto damp, fibrous
/// paper, the colour running along the fibres.
///
/// The paper is two sets of fibres, one running down and one across, each
/// streaked noise as long as **Fiber Length** asks. Every pixel's colour is
/// the average of the picture along the fibres through it, weighted by how
/// wet each fibre is — see [`WATER_SOAK`] — so colour bleeds out of a shape in
/// streaks both ways and the picture takes on the grid of the paper. Pigment
/// then settles into the fibres, which gives the dark parts their weave.
/// **Brightness** sinks the whole picture towards black or lifts its
/// mid-tones towards white, and
/// **Contrast** stretches it about mid-grey, in that order, with the
/// highlights rolling off rather than clipping.
///
/// Unlike the rest of the family it keeps the picture's colours: CS6's Water
/// Paper ignores the swatches.
///
/// Alpha is left alone.
///
/// No GPU path, for Bas Relief's reasons: a handful of short one-axis sums
/// and a curve per pixel. The fibres are laid by where on the canvas a pixel
/// is, so a preview crop cannot be filtered on its own.
pub fn water_paper(pixmap: &mut Pixmap, fiber: u32, brightness: u32, contrast: u32) {
    if pixmap.is_empty() {
        return;
    }
    let fiber = fiber.clamp(*WATER_FIBER.start(), *WATER_FIBER.end()) as f32;
    let brightness =
        brightness.clamp(*WATER_BRIGHTNESS.start(), *WATER_BRIGHTNESS.end()) as f32;
    let contrast = contrast.clamp(*WATER_CONTRAST.start(), *WATER_CONTRAST.end()) as f32;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    let run = fiber * WATER_RUN_PER_STEP;
    let reach = (run / 2.0).round().max(1.0) as i32;

    let down = crate::filters::artistic::streaked_noise(w, h, run, 31, (0.0, 1.0));
    let across = crate::filters::artistic::streaked_noise(w, h, run, 32, (1.0, 0.0));
    let wet_down: Vec<f32> = down.par_iter().map(|v| (WATER_SOAK * v).exp()).collect();
    let wet_across: Vec<f32> = across.par_iter().map(|v| (WATER_SOAK * v).exp()).collect();

    // Each pixel's colour, in floats, so the sums below read it cheaply.
    let stride = pixmap.stride();
    let source: Vec<[f32; 3]> = pixmap
        .as_bytes()
        .chunks_exact(4)
        .map(|p| [p[0] as f32, p[1] as f32, p[2] as f32])
        .collect();
    let (source, down, across) = (&source, &down, &across);
    let (wet_down, wet_across) = (&wet_down, &wet_across);

    let lift = 2f32.powf((brightness - 50.0) / WATER_BRIGHTNESS_DOUBLING);
    let gain = 2f32.powf((contrast - WATER_CONTRAST_FLAT) / WATER_CONTRAST_DOUBLING);
    let tone_curve = |v: f32| {
        let v = v.clamp(0.0, 1.0);
        let v = if lift < 1.0 { v * lift } else { v.powf(1.0 / lift) };
        let v = (v - 0.5) * gain + 0.5;
        if v > WATER_SHOULDER {
            let room = 1.0 - WATER_SHOULDER;
            1.0 - room * (-(v - WATER_SHOULDER) / room).exp()
        } else {
            v
        }
    };
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                // The colour carried along both fibres through this pixel.
                let mut total = [0.0f32; 3];
                let mut weight = 0.0f32;
                for k in -reach..=reach {
                    let sy = y as i32 + k;
                    if sy >= 0 && sy < h as i32 {
                        let j = sy as usize * w + x;
                        let wt = wet_down[j];
                        for c in 0..3 {
                            total[c] += wt * source[j][c];
                        }
                        weight += wt;
                    }
                    let sx = x as i32 + k;
                    if sx >= 0 && sx < w as i32 {
                        let j = y * w + sx as usize;
                        let wt = wet_across[j];
                        for c in 0..3 {
                            total[c] += wt * source[j][c];
                        }
                        weight += wt;
                    }
                }
                let i = y * w + x;
                let bled = total.map(|t| t / weight);

                // Pigment settles into the fibres, and there is only as much
                // of it to settle as the pixel is dark.
                let tone = (0.299 * bled[0] + 0.587 * bled[1] + 0.114 * bled[2]) / 255.0;
                let weave = down[i].max(across[i]) * WATER_WEAVE * (1.0 - tone);
                for c in 0..3 {
                    let v = tone_curve((bled[c] - weave) / 255.0);
                    px[c] = (v * 255.0).round().clamp(0.0, 255.0) as u8;
                }
                // Alpha stands: wetting the paper does not change the layer's
                // shape.
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Rect;

    const BLACK: Rgba8 = Rgba8::BLACK;
    const WHITE: Rgba8 = Rgba8::WHITE;

    /// A raised bar on dark ground: a ridge with two faces, one of which the
    /// light strikes and the other of which it cannot reach. A single step
    /// would not do — it has one slope, so it carves one line, and says
    /// nothing about which face takes which swatch.
    const RIDGE: std::ops::Range<i32> = 28..36;
    fn step() -> Pixmap {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(40, 40, 40, 255));
        pm.fill_rect(
            Rect::new(RIDGE.start, 0, RIDGE.len() as u32, 64),
            Rgba8::new(220, 220, 220, 255),
        );
        pm
    }

    /// A surface with no slope catches no light, so it comes out midway
    /// between the two swatches whatever its own brightness was.
    ///
    /// This is most of the picture, and it is what makes the filter read as
    /// carved stone rather than as a photograph with edges drawn on.
    #[test]
    fn bas_relief_leaves_flat_ground_midway_between_the_swatches() {
        let mut pm = step();
        bas_relief(&mut pm, 13, 3, Light::Left, BLACK, WHITE);
        // Well clear of the edge on both sides, and both were flat.
        for (x, y) in [(8, 8), (8, 50), (56, 8), (56, 50)] {
            let p = pm.get(x, y);
            assert!(
                (p.r as i32 - 128).abs() <= 3,
                "({}, {}) is {:?}, not the midpoint",
                x,
                y,
                p
            );
        }
    }

    /// The two faces of a ridge take opposite swatches, and swapping the
    /// light over swaps them — which is the whole of the Light list.
    #[test]
    fn bas_relief_lights_the_two_faces_from_the_chosen_side() {
        let lit = |light| {
            let mut pm = step();
            bas_relief(&mut pm, 13, 1, light, BLACK, WHITE);
            // The ridge's two faces.
            (pm.get(RIDGE.start, 32).r as i32, pm.get(RIDGE.end - 1, 32).r as i32)
        };
        let (near, far) = lit(Light::Left);
        assert!(
            (near - far).abs() > 60,
            "the ridge was not carved: {} against {}",
            near,
            far
        );
        let (flipped_near, flipped_far) = lit(Light::Right);
        assert!(
            (near - far).signum() != (flipped_near - flipped_far).signum(),
            "lighting from the other side did not turn the carving over: \
             {} against {} became {} against {}",
            near,
            far,
            flipped_near,
            flipped_far
        );
    }

    /// Detail is the gain: more of it drives more of the picture off the flat
    /// mid-tone and into the swatches.
    #[test]
    fn bas_relief_detail_deepens_the_carving() {
        let carved = |detail| {
            let mut pm = step();
            bas_relief(&mut pm, detail, 3, Light::Left, BLACK, WHITE);
            pm.as_bytes()
                .chunks_exact(4)
                .map(|p| (p[0] as i32 - 128).abs() as u32)
                .sum::<u32>()
        };
        assert!(carved(15) > carved(1), "{} against {}", carved(15), carved(1));
    }

    /// Smoothness rounds the surface off before it is lit, so the carving
    /// spreads out instead of standing on one crisp line.
    ///
    /// Measured as the width of the carved band, not its depth: at any useful
    /// Detail a hard edge drives both faces clean into the swatches whatever
    /// the Smoothness, so the peak is pinned at the ends of the range and
    /// says nothing. What rounding off does is make the band wider.
    #[test]
    fn bas_relief_smoothness_rounds_the_carving_off() {
        let width = |smoothness| {
            let mut pm = step();
            bas_relief(&mut pm, 13, smoothness, Light::Left, BLACK, WHITE);
            (0..64)
                .filter(|&x| (pm.get(x, 32).r as i32 - 128).abs() > 10)
                .count()
        };
        assert!(width(15) > width(1), "{} against {}", width(15), width(1));
    }

    /// It paints in the swatches, not in the picture's own colours — the one
    /// thing every filter in this family has in common.
    #[test]
    fn bas_relief_paints_in_the_two_swatches() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(30, 160, 60, 255));
        pm.fill_rect(Rect::new(32, 0, 32, 64), Rgba8::new(200, 40, 90, 255));
        let (fore, back) = (Rgba8::new(0, 0, 200, 255), Rgba8::new(255, 255, 255, 255));
        bas_relief(&mut pm, 13, 3, Light::Bottom, fore, back);
        // Nothing green survives: every pixel lies on the line between the
        // two swatches, which for these two means r == g and b at or above
        // both.
        for p in pm.as_bytes().chunks_exact(4) {
            assert_eq!(p[0], p[1], "{:?} is not on the line between the swatches", p);
            assert!(p[2] >= p[0], "{:?} is not on the line between the swatches", p);
        }
    }

    #[test]
    fn bas_relief_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        bas_relief(&mut pm, 13, 3, Light::Bottom, BLACK, WHITE);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn bas_relief_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        bas_relief(&mut pm, 13, 3, Light::Bottom, BLACK, WHITE);
    }

    /// Three flat bands — dark, mid and light — so each stick has somewhere
    /// it must reach and somewhere it must not.
    fn bands() -> Pixmap {
        let mut pm = Pixmap::filled(120, 64, Rgba8::new(128, 128, 128, 255));
        pm.fill_rect(Rect::new(0, 0, 40, 64), Rgba8::new(16, 16, 16, 255));
        pm.fill_rect(Rect::new(80, 0, 40, 64), Rgba8::new(252, 252, 252, 255));
        pm
    }

    /// The mean colour of a band's middle, clear of the smudge at its edges.
    fn band_mean(pm: &Pixmap, from: i32, to: i32) -> [f32; 3] {
        let mut total = [0.0f32; 3];
        let mut count = 0.0;
        for y in 8..56 {
            for x in from..to {
                let p = pm.get(x, y);
                total[0] += p.r as f32;
                total[1] += p.g as f32;
                total[2] += p.b as f32;
                count += 1.0;
            }
        }
        [total[0] / count, total[1] / count, total[2] / count]
    }

    /// The dark goes to the foreground, the light to the background, and the
    /// middle is left as bare paper.
    #[test]
    fn chalk_and_charcoal_gives_the_dark_to_one_stick_and_the_light_to_the_other() {
        let mut pm = bands();
        chalk_and_charcoal(&mut pm, 6, 6, 5, BLACK, WHITE);
        let dark = band_mean(&pm, 8, 32);
        let mid = band_mean(&pm, 52, 68);
        let light = band_mean(&pm, 88, 112);
        assert!(dark[0] < 40.0, "the charcoal did not take the dark: {:?}", dark);
        assert!(light[0] > 215.0, "the chalk did not take the light: {:?}", light);
        assert!(
            (mid[0] - CHALK_GROUND).abs() < 20.0,
            "the middle is not bare paper: {:?}",
            mid
        );
    }

    /// The ground is a neutral grey, not the midpoint of the two swatches.
    ///
    /// Every other filter in this family works between the two colours, so
    /// splitting the difference is the natural thing to write — and with a
    /// blue foreground against white it gives a pale blue paper where CS6's
    /// stays grey. Adobe is explicit that the drawing sits on "a solid
    /// midtone gray foundation".
    #[test]
    fn chalk_and_charcoal_draws_on_grey_paper_whatever_the_swatches() {
        let blue = Rgba8::new(0, 80, 200, 255);
        let mut pm = bands();
        chalk_and_charcoal(&mut pm, 6, 6, 5, blue, WHITE);
        let mid = band_mean(&pm, 52, 68);
        for (c, v) in mid.iter().enumerate() {
            assert!(
                (v - CHALK_GROUND).abs() < 20.0,
                "channel {} of the paper is {}, not grey: {:?}",
                c,
                v,
                mid
            );
        }
    }

    /// Each Area slider is how far its stick climbs into the midtones.
    ///
    /// Probed at a tone the stick can actually reach. Neither reaches a flat
    /// 50% grey even wound to the top — CS6's do not either — so testing both
    /// against the middle of the range only shows that nothing happened.
    #[test]
    fn chalk_and_charcoal_areas_reach_further_into_the_middle() {
        let drawn = |tone: u8, charcoal, chalk| {
            let mut pm = Pixmap::filled(64, 64, Rgba8::new(tone, tone, tone, 255));
            chalk_and_charcoal(&mut pm, charcoal, chalk, 5, BLACK, WHITE);
            band_mean(&pm, 8, 56)[0]
        };
        // A shade below the middle, which the charcoal climbs to but only
        // once the slider is wound well up.
        assert!(
            drawn(115, 20, 6) < drawn(115, 6, 6),
            "the charcoal did not climb: {} against {}",
            drawn(115, 20, 6),
            drawn(115, 6, 6)
        );
        // And a shade above it for the chalk coming down.
        assert!(
            drawn(191, 6, 20) > drawn(191, 6, 6),
            "the chalk did not come down: {} against {}",
            drawn(191, 6, 20),
            drawn(191, 6, 6)
        );
    }

    /// Stroke Pressure is how hard the stick is pressed: lightly, the tone
    /// grades into the paper; heavily, it lies flat and the picture separates
    /// into three colours with hard edges between them.
    #[test]
    fn chalk_and_charcoal_pressure_flattens_the_sticks() {
        let between = |pressure| {
            let mut pm = bands();
            chalk_and_charcoal(&mut pm, 10, 10, pressure, BLACK, WHITE);
            // How many pixels are neither paper nor near one of the swatches.
            pm.as_bytes()
                .chunks_exact(4)
                .filter(|p| {
                    let v = p[0] as f32;
                    v > 30.0 && v < 225.0 && (v - CHALK_GROUND).abs() > 25.0
                })
                .count()
        };
        assert!(between(5) < between(0), "{} against {}", between(5), between(0));
    }

    /// Near a threshold, a light touch grades smoothly and a firm one breaks
    /// the ground into separate marks.
    ///
    /// This is the direction of Stroke Pressure, and it is the opposite of
    /// what it looks like from the top of the slider alone. Wound up, the
    /// threshold is a hard line and ground within a swing of it dithers into
    /// strokes — so the *firm* setting is the one with visible marks on it.
    /// Wound down, the threshold is wide enough that the same swing only
    /// nudges the coverage, and the picture grades like a photograph with
    /// almost no strokes at all. The bottom of this slider is the smoothest
    /// setting there is, not the roughest.
    #[test]
    fn chalk_and_charcoal_pressure_breaks_the_ground_into_marks() {
        // A shade below where the chalk starts at Area 6, so both settings
        // have the same ground to work on.
        let marks = |pressure| {
            let mut pm = Pixmap::filled(96, 96, Rgba8::new(204, 204, 204, 255));
            chalk_and_charcoal(&mut pm, 6, 6, pressure, BLACK, WHITE);
            (1..96)
                .flat_map(|y| (1..96).map(move |x| (x, y)))
                .map(|(x, y)| (pm.get(x, y).r as i32 - pm.get(x - 1, y).r as i32).unsigned_abs())
                .sum::<u32>()
        };
        assert!(
            marks(5) > marks(0) * 5 / 4,
            "a firm hand did not mark the ground more than a light one: {} against {}",
            marks(5),
            marks(0)
        );
    }

    #[test]
    fn chalk_and_charcoal_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        chalk_and_charcoal(&mut pm, 6, 6, 1, BLACK, WHITE);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn chalk_and_charcoal_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        chalk_and_charcoal(&mut pm, 6, 6, 1, BLACK, WHITE);
    }

    /// A dark subject on a light ground, with a fine ripple over the ground:
    /// something for the masses to fill, something for the outlines to trace,
    /// and something faint that only a high Detail should pick up.
    fn sketchable() -> Pixmap {
        let mut pm = Pixmap::new(128, 128);
        for y in 0..128i32 {
            for x in 0..128i32 {
                // Fine and strong: fine enough that Detail 0's simplifying
                // blur erases it, strong enough that Detail 5 draws it.
                let ripple = (((x + y) % 4 < 2) as i32) * 40;
                let v = (214 + ripple).clamp(0, 255) as u8;
                pm.set(x, y, Rgba8::new(v, v, v, 255));
            }
        }
        // Tone 0.219, the same as the subject the sliders were matched on.
        pm.fill_rect(Rect::new(32, 32, 64, 64), Rgba8::new(67, 52, 47, 255));
        pm
    }

    /// How much of the sheet the stick covered.
    fn covered(pm: &Pixmap) -> usize {
        pm.as_bytes()
            .chunks_exact(4)
            .filter(|p| (p[0] as u32 + p[1] as u32 + p[2] as u32) < 384)
            .count()
    }

    /// What share of a square was covered by the stick.
    fn covered_in(pm: &Pixmap, from: i32, to: i32) -> f32 {
        let mut covered = 0.0;
        let mut total = 0.0;
        for y in from..to {
            for x in from..to {
                total += 1.0;
                if pm.get(x, y).r < 128 {
                    covered += 1.0;
                }
            }
        }
        covered / total
    }

    /// The dark subject fills with charcoal and the light ground is left as
    /// bare paper — there is no middle tone at all.
    ///
    /// That is what separates a sketch from a photograph, and it is the one
    /// thing every slider here has to keep true.
    ///
    /// Measured as how much of each was covered, not as the mean tone of it:
    /// a filled mass is *streaked*, because the stick skips over the tooth of
    /// the paper, so its average sits well up towards the middle of the range
    /// even where the fill is working perfectly well. See
    /// [`charcoal_lets_the_paper_show_through_a_filled_mass`], which insists
    /// on exactly that.
    #[test]
    fn charcoal_fills_the_dark_and_leaves_the_light_bare() {
        let mut pm = sketchable();
        charcoal(&mut pm, 3, 2, 45, BLACK, WHITE);
        let inside = covered_in(&pm, 48, 80);
        let outside = covered_in(&pm, 4, 26);
        assert!(inside > 0.4, "the subject did not fill: {:.2} covered", inside);
        assert!(
            outside < 0.05,
            "the ground did not stay bare: {:.2} covered",
            outside
        );
    }

    /// The mean brightness of a whole sheet.
    fn sheet_mean(pm: &Pixmap) -> f32 {
        let b = pm.as_bytes();
        b.chunks_exact(4).map(|p| p[0] as f32).sum::<f32>() / (b.len() / 4) as f32
    }

    /// Light/Dark Balance is how far up the tones the charcoal reaches.
    ///
    /// Read as a mean tone, not as a count of pixels over some cutoff. The
    /// stick shades rather than fills, so a midtone under a firm balance
    /// settles at a genuine mid grey — a count of "dark" pixels lands right
    /// on its own threshold there and answers noise.
    ///
    /// Checked against the tones of the photograph these were matched on: the
    /// subject at 0.219 and the sea at 0.640. Wind the span up until that sea
    /// goes solid and the picture stops being a drawing at the top of the
    /// slider.
    #[test]
    fn charcoal_balance_decides_how_far_the_stick_reaches() {
        let sheet = |tone: u8, balance| {
            let mut pm = Pixmap::filled(96, 96, Rgba8::new(tone, tone, tone, 255));
            charcoal(&mut pm, 3, 2, balance, BLACK, WHITE);
            sheet_mean(&pm)
        };
        // 0.219 — the subject. Much lighter at the bottom of the slider than
        // at 45, and well shaded by 45.
        let low = sheet(56, 1);
        let mid = sheet(56, 45);
        assert!(
            low > mid * 1.5,
            "the bottom of the slider shaded nearly as much as the middle: \
             {} against {}",
            low,
            mid
        );
        assert!(mid < 130.0, "the subject was not shaded at 45: {}", mid);
        // 0.640 — a midtone. Shaded at the top of the slider, never solid.
        let high = sheet(163, 100);
        assert!(
            high > 90.0,
            "a midtone went solid at the top of the slider: {}",
            high
        );
        assert!(high < 235.0, "the top of the slider did nothing: {}", high);
    }

    /// Detail is how weak an edge still gets drawn.
    #[test]
    fn charcoal_detail_picks_up_the_finer_marks() {
        let drawn = |detail| {
            let mut pm = sketchable();
            charcoal(&mut pm, 3, detail, 45, BLACK, WHITE);
            covered(&pm)
        };
        assert!(drawn(5) > drawn(0), "{} against {}", drawn(5), drawn(0));
    }

    /// Charcoal Thickness spreads every line it draws, and fills more of the
    /// paper's tooth back in.
    #[test]
    fn charcoal_thickness_lays_a_bolder_line() {
        let bold = |thickness| {
            let mut pm = sketchable();
            charcoal(&mut pm, thickness, 2, 45, BLACK, WHITE);
            covered(&pm)
        };
        assert!(bold(7) > bold(1), "{} against {}", bold(7), bold(1));
    }

    /// A shaded mass carries the stroke, rather than coming out flat.
    ///
    /// Measured as the spread of tone inside it, not as how much bare paper
    /// is left there. A dense mass is dense — the white inside CS6's subject
    /// is the subject's own markings, not the sheet showing through — so
    /// counting pale pixels asks a question that has no answer once the
    /// shading is working. What must not happen is a flat silhouette.
    #[test]
    fn charcoal_leaves_the_stroke_in_a_shaded_mass() {
        let mut pm = sketchable();
        charcoal(&mut pm, 3, 2, 45, BLACK, WHITE);
        // Inside the subject, well clear of its outline.
        let mut values = Vec::new();
        for y in 44..84 {
            for x in 44..84 {
                values.push(pm.get(x, y).r as f32);
            }
        }
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        let spread =
            (values.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / values.len() as f32)
                .sqrt();
        assert!(
            spread > 10.0,
            "the mass came out flat: tone varies by only {:.1} across it",
            spread
        );
    }

    /// The marks are short strokes, not lines drawn clear across the picture.
    ///
    /// This is the thing that keeps going wrong, and no other test here sees
    /// it: shade an even tone and every measure of *how much* charcoal went
    /// down comes out identical whether the stick laid a hundred short marks
    /// or one unbroken line through the lot of them. Only the length of a run
    /// along the drag tells them apart.
    #[test]
    fn charcoal_lays_short_marks_rather_than_long_lines() {
        // An even midtone, so the whole sheet is shaded the same and any run
        // found is the stroke's own doing.
        let mut pm = Pixmap::filled(200, 200, Rgba8::new(110, 110, 110, 255));
        charcoal(&mut pm, 1, 2, 50, BLACK, WHITE);
        // Walk down the drag, which runs at 45 degrees.
        let mut longest = 0;
        for start in 0..200i32 {
            let mut run = 0;
            for step in 0..(200 - start) {
                let (x, y) = (start + step, 199 - step);
                if pm.get(x, y).r < 128 {
                    run += 1;
                    longest = longest.max(run);
                } else {
                    run = 0;
                }
            }
        }
        assert!(
            longest < 40,
            "the stick drew a line {} pixels long — these should be strokes",
            longest
        );
    }

    /// A shaded area comes back in continuous tone, not one bit deep.
    ///
    /// Charcoal is soft and grey: a stroke carries its own weight and fades
    /// off at its edges. Compare the shading against a threshold instead of
    /// swinging it — which is the natural way to get *separate* marks — and
    /// every pixel lands on either paper or ink, the greys vanish, and the
    /// result is a staircased one-bit picture. Nothing else here sees that:
    /// how much charcoal went down, how long the marks are and where they
    /// fall all come out the same either way.
    #[test]
    fn charcoal_shades_in_greys_rather_than_one_bit() {
        let mut pm = Pixmap::filled(200, 200, Rgba8::new(110, 110, 110, 255));
        charcoal(&mut pm, 1, 2, 50, BLACK, WHITE);
        let greys = pm
            .as_bytes()
            .chunks_exact(4)
            .filter(|p| (40..=215).contains(&p[0]))
            .count();
        let total = pm.as_bytes().len() / 4;
        assert!(
            greys * 4 > total,
            "only {} of {} pixels carry a grey — the shading went one bit deep",
            greys,
            total
        );
    }

    /// It paints in the swatches, not in the picture's own colours.
    #[test]
    fn charcoal_paints_in_the_two_swatches() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(30, 160, 60, 255));
        pm.fill_rect(Rect::new(16, 16, 32, 32), Rgba8::new(200, 40, 90, 255));
        let (fore, back) = (Rgba8::new(0, 0, 200, 255), Rgba8::new(255, 255, 255, 255));
        charcoal(&mut pm, 3, 2, 45, fore, back);
        for p in pm.as_bytes().chunks_exact(4) {
            assert_eq!(p[0], p[1], "{:?} is not on the line between the swatches", p);
            assert!(p[2] >= p[0], "{:?} is not on the line between the swatches", p);
        }
    }

    #[test]
    fn charcoal_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        charcoal(&mut pm, 3, 2, 45, BLACK, WHITE);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn charcoal_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        charcoal(&mut pm, 3, 2, 45, BLACK, WHITE);
    }

    /// A smooth left-to-right ramp through the whole tonal range.
    fn ramp() -> Pixmap {
        let mut pm = Pixmap::new(256, 64);
        for y in 0..64i32 {
            for x in 0..256i32 {
                let v = x as u8;
                pm.set(x, y, Rgba8::new(v, v, v, 255));
            }
        }
        pm
    }

    /// How many times a scanline turns around.
    fn turns(pm: &Pixmap, y: i32) -> usize {
        let row: Vec<i32> = (0..pm.width() as i32).map(|x| pm.get(x, y).r as i32).collect();
        let mut turns = 0;
        let mut rising: Option<bool> = None;
        for pair in row.windows(2) {
            if pair[1] == pair[0] {
                continue;
            }
            let now = pair[1] > pair[0];
            if rising == Some(!now) {
                turns += 1;
            }
            rising = Some(now);
        }
        turns
    }

    /// A straight ramp of tone comes back oscillating: up to a highlight,
    /// over the edge, and up again, several times across the range.
    ///
    /// This is the whole filter. Any ordinary tone mapping — brighten,
    /// darken, threshold, invert — takes a ramp to something that still only
    /// turns around once or not at all, and would pass every other test here:
    /// the output would still be grey, still respond to the sliders, still
    /// leave alpha alone. What makes it read as polished metal rather than as
    /// a photograph is that the tone *cycles*.
    #[test]
    fn chrome_cycles_the_tone_rather_than_mapping_it_straight() {
        let mut pm = ramp();
        chrome(&mut pm, 5, 1);
        let turns = turns(&pm, 32);
        assert!(
            turns >= 4,
            "a straight ramp came back turning only {} times — the tone was \
             mapped, not cycled",
            turns
        );
    }

    /// Detail is how many times it cycles, so more of it puts more bands
    /// across the same range.
    #[test]
    fn chrome_detail_adds_bands() {
        let banded = |detail| {
            let mut pm = ramp();
            chrome(&mut pm, detail, 1);
            turns(&pm, 32)
        };
        assert!(
            banded(10) > banded(0),
            "{} against {}",
            banded(10),
            banded(0)
        );
    }

    /// Smoothness melts the surface, so the picture comes back with less
    /// small structure in it.
    #[test]
    fn chrome_smoothness_melts_the_surface() {
        let rough = |smoothness| {
            let mut pm = Pixmap::new(128, 128);
            for y in 0..128i32 {
                for x in 0..128i32 {
                    let v = (((x * 5 + y * 11) % 64) * 4) as u8;
                    pm.set(x, y, Rgba8::new(v, v, v, 255));
                }
            }
            chrome(&mut pm, 5, smoothness);
            (1..128)
                .flat_map(|y| (1..128).map(move |x| (x, y)))
                .map(|(x, y)| (pm.get(x, y).r as i32 - pm.get(x - 1, y).r as i32).unsigned_abs())
                .sum::<u32>()
        };
        assert!(rough(10) < rough(0), "{} against {}", rough(10), rough(0));
    }

    /// Chrome is grey, whatever colour went in.
    ///
    /// The rest of this family paints between the two swatches; this one
    /// does not take them at all, because polished metal has no colour of its
    /// own to take.
    #[test]
    fn chrome_comes_back_grey() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(30, 160, 60, 255));
        pm.fill_rect(Rect::new(16, 16, 32, 32), Rgba8::new(200, 40, 90, 255));
        chrome(&mut pm, 5, 1);
        for p in pm.as_bytes().chunks_exact(4) {
            assert_eq!(p[0], p[1], "{:?} is not grey", p);
            assert_eq!(p[1], p[2], "{:?} is not grey", p);
        }
    }

    #[test]
    fn chrome_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        chrome(&mut pm, 5, 1);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn chrome_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        chrome(&mut pm, 5, 1);
    }

    /// Conté Crayon over a flat sheet of one tone, with everything but the
    /// two levels left at CS6's defaults.
    fn crayoned(tone: u8, fore: u32, back: u32) -> Pixmap {
        let mut pm = Pixmap::filled(128, 128, Rgba8::new(tone, tone, tone, 255));
        conte_crayon(&mut pm, fore, back, Texture::Canvas, 100, 0, Light::Top, false, BLACK, WHITE);
        pm
    }

    /// What share of a sheet the crayon covered.
    fn crayon_cover(pm: &Pixmap) -> f32 {
        let b = pm.as_bytes();
        b.chunks_exact(4).filter(|p| p[0] < 128).count() as f32 / (b.len() / 4) as f32
    }

    /// What share of a sheet came back a grey rather than either swatch.
    fn crayon_greys(pm: &Pixmap) -> f32 {
        let b = pm.as_bytes();
        b.chunks_exact(4).filter(|p| (40..=215).contains(&p[0])).count() as f32
            / (b.len() / 4) as f32
    }

    /// The dark is carried in the foreground colour and the light left as
    /// bare paper in the background colour.
    #[test]
    fn conte_crayon_carries_the_dark_and_leaves_the_light() {
        assert!(
            crayon_cover(&crayoned(30, 11, 7)) > 0.9,
            "the crayon did not carry the dark"
        );
        assert!(
            crayon_cover(&crayoned(240, 11, 7)) < 0.1,
            "the crayon did not leave the light"
        );
    }

    /// The two levels pull against each other: more foreground carries the
    /// crayon further up the tones, more background brings the paper further
    /// down.
    ///
    /// Read as a mean tone. Counting pixels past a cutoff says nothing here —
    /// a midtone settles near that cutoff by definition, so the count answers
    /// noise rather than the sliders.
    fn crayon_mean(pm: &Pixmap) -> f32 {
        let b = pm.as_bytes();
        b.chunks_exact(4).map(|p| p[0] as f32).sum::<f32>() / (b.len() / 4) as f32
    }

    #[test]
    fn conte_crayon_levels_pull_against_each_other() {
        // A midtone, where both still have something to decide.
        let mid = 150;
        let level = crayon_mean(&crayoned(mid, 8, 8));
        assert!(
            crayon_mean(&crayoned(mid, 15, 8)) < level,
            "more foreground did not carry the crayon further"
        );
        assert!(
            crayon_mean(&crayoned(mid, 8, 15)) > level,
            "more background did not bring the paper further down"
        );
    }

    /// The midtones are drawn in the greys between the two swatches, and the
    /// paper's tooth grains them.
    ///
    /// Both halves matter and neither is seen by anything else here. The
    /// obvious way to put a crayon on textured paper is to decide, per pixel,
    /// whether the stick touched it — and that gives a picture with no greys
    /// at all, every pixel one swatch or the other in a fine spatter. It is a
    /// halftone screen, not a drawing, and it still carries the dark, leaves
    /// the light, answers every slider and keeps alpha, so the rest of these
    /// tests pass on it happily.
    #[test]
    fn conte_crayon_draws_the_midtones_in_greys() {
        let pm = crayoned(150, 8, 8);
        let greys = crayon_greys(&pm);
        assert!(
            greys > 0.9,
            "only {:.2} of a flat midtone came back a grey — the drawing went \
             two-colour",
            greys
        );
    }

    /// At a Relief of 0 the paper is flat, so a flat tone comes back flat —
    /// and at any Relief above it the weave grains the drawing.
    ///
    /// The paper reaches the picture only through its lighting, and an unlit
    /// surface has nothing to show. It is the same reason a square weave
    /// reads as horizontal striations under a light from the top.
    #[test]
    fn conte_crayon_grains_the_drawing_only_where_the_paper_is_lit() {
        let spread = |relief| {
            let mut pm = Pixmap::filled(128, 128, Rgba8::new(150, 150, 150, 255));
            conte_crayon(
                &mut pm, 8, 8, Texture::Canvas, 100, relief, Light::Top, false, BLACK, WHITE,
            );
            let values: Vec<f32> = pm.as_bytes().chunks_exact(4).map(|p| p[0] as f32).collect();
            let mean = values.iter().sum::<f32>() / values.len() as f32;
            (values.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / values.len() as f32)
                .sqrt()
        };
        assert!(spread(0) < 0.5, "flat paper still grained the drawing: {:.1}", spread(0));
        assert!(spread(25) > 4.0, "the lit paper left no grain: {:.1}", spread(25));
    }

    /// Each texture leaves its own grain, and Scaling changes its size.
    #[test]
    fn conte_crayon_grain_follows_the_texture_and_its_scaling() {
        // Lit, because an unlit surface shows nothing at all — see
        // [`conte_crayon_grains_the_drawing_only_where_the_paper_is_lit`].
        let drawn = |texture, scaling| {
            let mut pm = Pixmap::filled(128, 128, Rgba8::new(150, 150, 150, 255));
            conte_crayon(&mut pm, 8, 8, texture, scaling, 25, Light::Top, false, BLACK, WHITE);
            pm.as_bytes().to_vec()
        };
        let canvas = drawn(Texture::Canvas, 100);
        assert_ne!(canvas, drawn(Texture::Brick, 100), "the texture made no difference");
        assert_ne!(canvas, drawn(Texture::Canvas, 200), "the scaling made no difference");
    }

    /// Relief lights the paper, so raising it changes the drawing; and the
    /// Light list decides which side of the grain catches it.
    #[test]
    fn conte_crayon_relief_lights_the_paper() {
        let drawn = |relief, light| {
            let mut pm = Pixmap::filled(128, 128, Rgba8::new(150, 150, 150, 255));
            conte_crayon(&mut pm, 8, 8, Texture::Canvas, 100, relief, light, false, BLACK, WHITE);
            pm.as_bytes().to_vec()
        };
        let flat = drawn(0, Light::Top);
        assert_ne!(flat, drawn(40, Light::Top), "the relief did not light the paper");
        assert_ne!(
            drawn(40, Light::Top),
            drawn(40, Light::Bottom),
            "the light came from the same side either way"
        );
    }

    /// Invert turns the paper inside out, so what took the crayon first now
    /// takes it last.
    #[test]
    fn conte_crayon_invert_turns_the_paper_over() {
        let drawn = |invert| {
            let mut pm = Pixmap::filled(128, 128, Rgba8::new(150, 150, 150, 255));
            conte_crayon(&mut pm, 8, 8, Texture::Canvas, 100, 20, Light::Top, invert, BLACK, WHITE);
            pm.as_bytes().to_vec()
        };
        assert_ne!(drawn(false), drawn(true));
    }

    /// The weave runs the way the Light is set, not the way it is woven.
    ///
    /// Canvas is a square weave, and the obvious thing to do with it — read
    /// the height field and let a waxy stick catch the tops of the grain —
    /// lays that square over the picture as a grid, the same whichever way
    /// the light is set. CS6 shows horizontal striations under a light from
    /// the top, because a slope is lit by how far it falls *away* from the
    /// light: the threads running across the picture catch it and the ones
    /// running down it do not. Nothing else here notices which way round the
    /// grain went.
    #[test]
    fn conte_crayon_weave_runs_across_the_light() {
        // At CS6's own default Relief of 4. Wound higher the directional
        // lighting swamps everything and the measure stops telling the two
        // apart — a square weave laid straight over the picture still comes
        // out looking directional at a Relief of 30.
        let grain_along = |light| {
            let mut pm = Pixmap::filled(128, 128, Rgba8::new(150, 150, 150, 255));
            conte_crayon(
                &mut pm, 8, 8, Texture::Canvas, 100, 4, light, false, BLACK, WHITE,
            );
            let across: u32 = (1..128)
                .flat_map(|y| (1..128).map(move |x| (x, y)))
                .map(|(x, y)| (pm.get(x, y).r as i32 - pm.get(x - 1, y).r as i32).unsigned_abs())
                .sum();
            let down: u32 = (1..128)
                .flat_map(|y| (1..128).map(move |x| (x, y)))
                .map(|(x, y)| (pm.get(x, y).r as i32 - pm.get(x, y - 1).r as i32).unsigned_abs())
                .sum();
            (across, down)
        };
        // Lit from the top: lines run across, so the tone changes going down.
        let (across, down) = grain_along(Light::Top);
        assert!(
            down > across * 3 / 2,
            "a light from the top did not lay the weave across: {} across against {} down",
            across,
            down
        );
        // Lit from the left: the other way about.
        let (across, down) = grain_along(Light::Left);
        assert!(
            across > down * 3 / 2,
            "a light from the left did not lay the weave down: {} across against {} down",
            across,
            down
        );
    }

    /// It draws in the swatches, not in the picture's own colours.
    #[test]
    fn conte_crayon_draws_in_the_two_swatches() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(30, 160, 60, 255));
        pm.fill_rect(Rect::new(16, 16, 32, 32), Rgba8::new(200, 40, 90, 255));
        let (fore, back) = (Rgba8::new(0, 0, 200, 255), Rgba8::new(255, 255, 255, 255));
        // Relief off, so only the crayon itself is being read.
        conte_crayon(&mut pm, 11, 7, Texture::Canvas, 100, 0, Light::Top, false, fore, back);
        for p in pm.as_bytes().chunks_exact(4) {
            assert_eq!(p[0], p[1], "{:?} is not on the line between the swatches", p);
            assert!(p[2] >= p[0], "{:?} is not on the line between the swatches", p);
        }
    }

    #[test]
    fn conte_crayon_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        conte_crayon(&mut pm, 11, 7, Texture::Canvas, 100, 20, Light::Top, false, BLACK, WHITE);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn conte_crayon_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        conte_crayon(&mut pm, 11, 7, Texture::Canvas, 100, 20, Light::Top, false, BLACK, WHITE);
    }

    /// A flat mid-grey sheet, which the screen has half its range to ink.
    fn pen_sheet() -> Pixmap {
        Pixmap::filled(160, 160, Rgba8::new(128, 128, 128, 255))
    }

    /// Whether a pixel came back as ink rather than as paper.
    fn inked(pm: &Pixmap, x: i32, y: i32) -> bool {
        pm.get(x, y).r < 128
    }

    /// What share of a sheet the pen covered.
    fn pen_cover(pm: &Pixmap) -> f32 {
        let b = pm.as_bytes();
        b.chunks_exact(4).filter(|p| p[0] < 128).count() as f32 / (b.len() / 4) as f32
    }

    /// How often two neighbouring pixels, a step apart, are both ink or both
    /// paper. Lines running one way agree along them and alternate across.
    fn agreement(pm: &Pixmap, dx: i32, dy: i32) -> f32 {
        let (mut same, mut total) = (0.0f32, 0.0f32);
        for y in 0..160i32 {
            for x in 0..160i32 {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= 160 || ny >= 160 {
                    continue;
                }
                total += 1.0;
                if inked(pm, x, y) == inked(pm, nx, ny) {
                    same += 1.0;
                }
            }
        }
        same / total.max(1.0)
    }

    /// The dark takes the ink and the light is left as bare paper — there is
    /// no middle tone, which is what makes this read as a pen rather than as a
    /// tinted photograph.
    #[test]
    fn graphic_pen_takes_the_dark_and_leaves_the_light() {
        let mut pm = Pixmap::filled(128, 128, Rgba8::new(20, 20, 20, 255));
        pm.fill_rect(Rect::new(0, 0, 128, 32), Rgba8::new(245, 245, 245, 255));
        graphic_pen(&mut pm, 8, 50, StrokeDirection::Horizontal, BLACK, WHITE);
        // Well inside each band, clear of the boundary between them.
        let light = (4..28)
            .flat_map(|y| (8..120).map(move |x| (x, y)))
            .filter(|&(x, y)| inked(&pm, x, y))
            .count();
        let dark = (48..112)
            .flat_map(|y| (8..120).map(move |x| (x, y)))
            .filter(|&(x, y)| inked(&pm, x, y))
            .count();
        assert!(
            dark > light * 3,
            "the dark did not take the ink: {} against {}",
            dark,
            light
        );
    }

    /// Light/Dark Balance is how far the sheet is slid towards ink. Wound up
    /// it darkens the sheet; wound down it leaves most of it bare.
    #[test]
    fn graphic_pen_balance_moves_the_sheet_towards_ink() {
        let cover = |balance| {
            let mut pm = pen_sheet();
            graphic_pen(&mut pm, 6, balance, StrokeDirection::Horizontal, BLACK, WHITE);
            pen_cover(&pm)
        };
        assert!(
            cover(80) > cover(20) + 0.2,
            "the balance did not slide the sheet: {} against {}",
            cover(80),
            cover(20)
        );
    }

    /// Wound well up, the strokes run together and a dark area goes solid
    /// rather than staying striped — which is what CS6's heavier settings show
    /// and what a plain line screen cannot do.
    #[test]
    fn graphic_pen_dark_areas_run_together_when_the_balance_is_high() {
        let covered = |balance| {
            let mut pm = Pixmap::filled(128, 128, Rgba8::new(30, 30, 30, 255));
            graphic_pen(&mut pm, 8, balance, StrokeDirection::Horizontal, BLACK, WHITE);
            pen_cover(&pm)
        };
        assert!(
            covered(60) > 0.97,
            "a dark area stayed striped at a heavy balance: {:.2} covered",
            covered(60)
        );
    }

    /// The lines run along Stroke Direction: a pixel and its neighbour up the
    /// line agree far more often than a pixel and its neighbour across it.
    ///
    /// This is the whole of the direction list. Any of the four drawn the same
    /// way would still look like a pen drawing, so nothing else here sees it.
    #[test]
    fn graphic_pen_lays_the_lines_the_way_the_direction_says() {
        for (direction, along) in [
            (StrokeDirection::Horizontal, (1, 0)),
            (StrokeDirection::Vertical, (0, 1)),
            (StrokeDirection::RightDiagonal, (1, -1)),
            (StrokeDirection::LeftDiagonal, (1, 1)),
        ] {
            let mut pm = pen_sheet();
            graphic_pen(&mut pm, 12, 50, direction, BLACK, WHITE);
            let with = agreement(&pm, along.0, along.1);
            let across = agreement(&pm, -along.1, along.0);
            assert!(
                with > across + 0.1,
                "{:?}: the lines do not run along the direction ({} with, {} across)",
                direction,
                with,
                across
            );
        }
    }

    /// Stroke Length is how long a mark is: short, and the drawing is
    /// dithering that breaks against every change of tone; long, and the marks
    /// are dashes that carry across the picture.
    ///
    /// Measured as how often a scanline changes from ink to paper, which is
    /// the number of marks it crosses: many short ones, or few long ones.
    #[test]
    fn graphic_pen_stroke_length_lengthens_the_marks() {
        let turns = |length| {
            let mut pm = pen_sheet();
            graphic_pen(&mut pm, length, 50, StrokeDirection::Horizontal, BLACK, WHITE);
            let row: Vec<bool> = (0..160).map(|x| inked(&pm, x, 80)).collect();
            row.windows(2).filter(|pair| pair[0] != pair[1]).count()
        };
        assert!(turns(1) > turns(15), "{} against {}", turns(1), turns(15));
    }

    /// It draws in the swatches, not in the picture's own colours — the one
    /// thing every filter in this family has in common.
    #[test]
    fn graphic_pen_paints_in_the_two_swatches() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(30, 160, 60, 255));
        pm.fill_rect(Rect::new(16, 16, 32, 32), Rgba8::new(200, 40, 90, 255));
        let (fore, back) = (Rgba8::new(0, 0, 200, 255), Rgba8::new(255, 255, 255, 255));
        graphic_pen(&mut pm, 8, 50, StrokeDirection::RightDiagonal, fore, back);
        for p in pm.as_bytes().chunks_exact(4) {
            let is_fore = p[0] == fore.r && p[1] == fore.g && p[2] == fore.b;
            let is_back = p[0] == back.r && p[1] == back.g && p[2] == back.b;
            assert!(is_fore || is_back, "{:?} is neither swatch", p);
        }
    }

    #[test]
    fn graphic_pen_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        graphic_pen(&mut pm, 8, 50, StrokeDirection::Horizontal, BLACK, WHITE);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn graphic_pen_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        graphic_pen(&mut pm, 8, 50, StrokeDirection::Horizontal, BLACK, WHITE);
    }

    /// A flat mid-grey sheet, where the screen has half the range to ink.
    fn screen_sheet() -> Pixmap {
        Pixmap::filled(64, 64, Rgba8::new(128, 128, 128, 255))
    }

    /// How many pixels came back a grey rather than one of the two swatches.
    fn screen_greys(pm: &Pixmap) -> usize {
        pm.as_bytes()
            .chunks_exact(4)
            .filter(|p| (20..=235).contains(&p[0]))
            .count()
    }

    /// Contrast is how hard the picture is pushed against the screen: wound
    /// right down the screen shades it and the greys come through, wound up
    /// they are cut away and nothing is left but the two swatches.
    #[test]
    fn halftone_pattern_contrast_cuts_the_greys_away() {
        let greys = |contrast| {
            let mut pm = screen_sheet();
            halftone_pattern(&mut pm, 6, contrast, HalftonePattern::Dot, BLACK, WHITE);
            screen_greys(&pm)
        };
        // At the top of the slider every pixel is one swatch or the other.
        assert_eq!(greys(50), 0, "contrast 50 left greys in the screen");
        assert!(
            greys(0) > 0,
            "the screen did not soften at the bottom of contrast"
        );
        assert!(greys(0) > greys(10), "contrast did not cut the greys away");
    }

    /// What the screen does to a tone at CS6's default Contrast: a mid grey —
    /// and only a mid grey — comes back as the full black-and-white
    /// chessboard, which is how the eye reads it back as a mid grey. Either
    /// side of that the chessboard is of white against a light grey, or of
    /// black against a dark one, so the photograph's own tones survive as the
    /// *shade* of the chessboard. A curve steeper than this collapses whole
    /// bands of tone onto the same flat chessboard and posterises the sheet.
    #[test]
    fn halftone_pattern_shades_the_chessboard_with_the_tone() {
        // The two swatches of the chessboard at one tone, dark one first.
        let squares = |grey: u8| {
            let mut pm = Pixmap::filled(16, 16, Rgba8::new(grey, grey, grey, 255));
            halftone_pattern(&mut pm, 1, 5, HalftonePattern::Dot, BLACK, WHITE);
            let (a, b) = (pm.get(8, 8).r, pm.get(9, 8).r);
            assert_ne!(a, b, "grey {grey} came back flat, with no chessboard in it");
            (a.min(b), a.max(b))
        };
        let (dark, light) = squares(128);
        assert!(dark < 4 && light > 251, "a mid grey is not the full chessboard: {dark} {light}");
        let (dark, light) = squares(220);
        assert_eq!(light, 255, "a light grey lost its paper");
        assert!((160..250).contains(&dark), "a light grey went to ink, not a light grey: {dark}");
        let (dark, light) = squares(40);
        assert_eq!(dark, 0, "a dark grey lost its ink");
        assert!((10..100).contains(&light), "a dark grey went to paper, not a dark grey: {light}");
    }

    /// The trap this filter falls into: thresholding the picture to two
    /// colours whatever Contrast says. CS6 at a low Contrast comes back as
    /// the *photograph*, shaded by the screen — a ramp still reads as a ramp,
    /// with its own tones in it, and only the top of the slider flattens it
    /// into ink and paper.
    #[test]
    fn halftone_pattern_at_low_contrast_keeps_the_photograph() {
        let ramp = || {
            let mut pm = Pixmap::new(256, 64);
            for y in 0..64 {
                for x in 0..256 {
                    let v = x as u8;
                    pm.set(x, y, Rgba8::new(v, v, v, 255));
                }
            }
            pm
        };
        // How many tones the ramp came back in, and how well what came back
        // still climbs with what went in.
        let levels = |pm: &Pixmap| {
            let mut seen = [false; 256];
            for p in pm.as_bytes().chunks_exact(4) {
                seen[p[0] as usize] = true;
            }
            seen.iter().filter(|&&s| s).count()
        };
        let column_mean = |pm: &Pixmap, x: i32| {
            (0..64).map(|y| pm.get(x, y).r as f32).sum::<f32>() / 64.0
        };

        let mut low = ramp();
        halftone_pattern(&mut low, 4, 5, HalftonePattern::Dot, BLACK, WHITE);
        assert!(
            levels(&low) > 32,
            "the low end of contrast came back in {} tones, not a photograph",
            levels(&low)
        );
        // Dark end inked, light end bare, and the middle in between.
        let (dark, mid, light) = (
            column_mean(&low, 16),
            column_mean(&low, 128),
            column_mean(&low, 240),
        );
        assert!(dark < mid && mid < light, "the ramp did not climb: {dark} {mid} {light}");

        // And at the top of the slider the ramp is flattened into the two
        // swatches — bar the odd pixel sitting on the threshold itself, which
        // the curve passes through however steep it is.
        let mut high = ramp();
        halftone_pattern(&mut high, 4, 50, HalftonePattern::Dot, BLACK, WHITE);
        let pure = high
            .as_bytes()
            .chunks_exact(4)
            .filter(|p| p[0] == 0 || p[0] == 255)
            .count();
        assert!(
            pure * 1000 >= 256 * 64 * 995,
            "the top of contrast left {} of {} pixels grey",
            256 * 64 - pure,
            256 * 64
        );
    }

    /// The dark takes the ink and the light is left as bare paper.
    #[test]
    fn halftone_pattern_takes_the_dark_and_leaves_the_light() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(20, 20, 20, 255));
        pm.fill_rect(Rect::new(0, 0, 64, 24), Rgba8::new(245, 245, 245, 255));
        halftone_pattern(&mut pm, 6, 50, HalftonePattern::Dot, BLACK, WHITE);
        let light = (4..20)
            .flat_map(|y| (4..60).map(move |x| (x, y)))
            .filter(|&(x, y)| inked(&pm, x, y))
            .count();
        let dark = (28..60)
            .flat_map(|y| (4..60).map(move |x| (x, y)))
            .filter(|&(x, y)| inked(&pm, x, y))
            .count();
        assert!(
            dark > light * 3,
            "the dark did not take the ink: {} against {}",
            dark,
            light
        );
    }

    /// The Dot screen is a chessboard of squares `size` pixels across, square
    /// to the picture: it comes back round every two squares either way, a
    /// step of one square along a row or a column lands on the other colour,
    /// and a step of one along the diagonal on the same colour again.
    #[test]
    fn halftone_pattern_dot_rules_a_chessboard_of_squares() {
        let mut pm = screen_sheet();
        halftone_pattern(&mut pm, 6, 50, HalftonePattern::Dot, BLACK, WHITE);
        for y in 0..(64 - 12) {
            for x in 0..(64 - 12) {
                assert_eq!(pm.get(x, y), pm.get(x + 12, y), "not periodic in x at ({x}, {y})");
                assert_eq!(pm.get(x, y), pm.get(x, y + 12), "not periodic in y at ({x}, {y})");
                assert_eq!(pm.get(x, y), pm.get(x + 6, y + 6), "not diagonal at ({x}, {y})");
            }
        }
        // Read at the middle of each square, which is where it is most
        // itself — the squares fade into one another at the join, so a pixel
        // picked there belongs to neither.
        for cy in (0..52).step_by(6) {
            for cx in (0..52).step_by(6) {
                let here = inked(&pm, cx, cy);
                assert_ne!(here, inked(&pm, cx + 6, cy), "squares do not alternate across");
                assert_ne!(here, inked(&pm, cx, cy + 6), "squares do not alternate down");
            }
        }
    }

    /// The squares are soft-edged, not tiles: they are rounded, and fade into
    /// one another rather than meeting at a step — without which a screen
    /// reads as pixel art rather than as a screen. Contrast hardens the rim
    /// along with everything else, so it is off the top of the slider that it
    /// shows.
    #[test]
    fn halftone_pattern_dot_squares_are_soft_edged() {
        let mut pm = screen_sheet();
        halftone_pattern(&mut pm, 6, 7, HalftonePattern::Dot, BLACK, WHITE);
        let soft = screen_greys(&pm);
        assert!(
            soft > 64 * 64 / 8,
            "the squares met at a step, not a fade: {soft} of {} pixels",
            64 * 64
        );
        // Walking from the middle of one square to the middle of the next
        // crosses the join once and in order, through several shades.
        let row: Vec<u8> = (0..=6).map(|x| pm.get(x, 0).r).collect();
        assert!(
            row.windows(2).all(|p| p[0] <= p[1]),
            "the fade does not run from one square to the next: {row:?}"
        );
        let between = row.iter().filter(|&&v| (20..=235).contains(&v)).count();
        assert!(between >= 2, "the join is a step, not a fade: {row:?}");
    }

    /// At half tone the black squares and the white ones are the same size,
    /// which is the chessboard the eye fuses back into that grey. At Size 1 a
    /// square is a single pixel, so the chessboard is the pixel grid itself:
    /// every pixel is the opposite of the four beside it and the same as the
    /// four at its corners, which is the screen CS6 draws there.
    #[test]
    fn halftone_pattern_dot_is_a_chessboard_at_half_tone() {
        let mut pm = screen_sheet();
        halftone_pattern(&mut pm, 1, 50, HalftonePattern::Dot, BLACK, WHITE);
        for y in 1..63 {
            for x in 1..63 {
                let here = inked(&pm, x, y);
                assert_ne!(here, inked(&pm, x + 1, y), "({x}, {y}) matches its neighbour");
                assert_ne!(here, inked(&pm, x, y + 1), "({x}, {y}) matches the one below");
                assert_eq!(here, inked(&pm, x + 1, y + 1), "({x}, {y}) breaks the diagonal");
            }
        }
        // Half the sheet, square for square with the paper, at a coarser Size
        // too — where a square of the board is eight pixels across. Weighed
        // rather than counted, because the join between two squares is a fade
        // (see `halftone_pattern_dot_squares_are_soft_edged`) and stands for
        // the part of a square it covers. Off the top of Contrast, too: the
        // join asks for exactly a mid grey, so at the top of the slider a
        // sheet of exactly that tone is a knife edge and lands wherever the
        // last bit of rounding sends it.
        let mut pm = screen_sheet();
        halftone_pattern(&mut pm, 8, 7, HalftonePattern::Dot, BLACK, WHITE);
        let ink: f32 = pm
            .as_bytes()
            .chunks_exact(4)
            .map(|p| 1.0 - p[0] as f32 / 255.0)
            .sum();
        let sheet = (64 * 64) as f32;
        assert!(
            (0.45..=0.55).contains(&(ink / sheet)),
            "the screen is not half ink: {:.3}",
            ink / sheet
        );
    }

    /// Bare paper stays bare and solid ink closes up: the ends of the tone
    /// range are not lost to the coarse ladder a fine screen can strike.
    #[test]
    fn halftone_pattern_dot_reaches_both_ends() {
        for size in [1, 4, 12] {
            let mut white = Pixmap::filled(32, 32, WHITE);
            halftone_pattern(&mut white, size, 50, HalftonePattern::Dot, BLACK, WHITE);
            assert!(
                (0..32).flat_map(|y| (0..32).map(move |x| (x, y))).all(|(x, y)| !inked(&white, x, y)),
                "paper took ink at size {size}"
            );
            let mut black = Pixmap::filled(32, 32, BLACK);
            halftone_pattern(&mut black, size, 50, HalftonePattern::Dot, BLACK, WHITE);
            assert!(
                (0..32).flat_map(|y| (0..32).map(move |x| (x, y))).all(|(x, y)| inked(&black, x, y)),
                "the solid did not close up at size {size}"
            );
        }
    }

    /// The Line screen rules bands across the picture: every pixel in a row
    /// agrees with its neighbour along it, and the screen repeats down.
    #[test]
    fn halftone_pattern_line_rules_bands() {
        let mut pm = screen_sheet();
        halftone_pattern(&mut pm, 6, 50, HalftonePattern::Line, BLACK, WHITE);
        for y in 0..(64 - 6) {
            for x in 0..(64 - 6) {
                assert_eq!(pm.get(x, y), pm.get(x + 1, y), "the bands do not run across");
                assert_eq!(pm.get(x, y), pm.get(x, y + 6), "not periodic in y at ({x}, {y})");
            }
        }
    }

    /// The Circle screen is radial: points the same distance from the middle
    /// of the picture sit on the same ring.
    #[test]
    fn halftone_pattern_circle_rules_rings() {
        let mut pm = Pixmap::filled(128, 128, Rgba8::new(128, 128, 128, 255));
        halftone_pattern(&mut pm, 8, 50, HalftonePattern::Circle, BLACK, WHITE);
        for r in [9, 17, 26, 41, 55] {
            let right = pm.get(64 + r, 64);
            assert_eq!(right, pm.get(64 - r, 64), "not radial at r={r}");
            assert_eq!(right, pm.get(64, 64 + r), "not radial at r={r}");
            assert_eq!(right, pm.get(64, 64 - r), "not radial at r={r}");
        }
    }

    /// Size is how far apart the cells stand: a wider cell crosses a scanline
    /// fewer times.
    #[test]
    fn halftone_pattern_size_widens_the_cells() {
        let bands = |size| {
            let mut pm = screen_sheet();
            halftone_pattern(&mut pm, size, 50, HalftonePattern::Line, BLACK, WHITE);
            let column: Vec<bool> = (0..64).map(|y| inked(&pm, 32, y)).collect();
            column.windows(2).filter(|pair| pair[0] != pair[1]).count()
        };
        assert!(bands(12) < bands(2), "{} against {}", bands(12), bands(2));
    }

    /// It draws in the swatches, not in the picture's own colours.
    #[test]
    fn halftone_pattern_paints_in_the_two_swatches() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(30, 160, 60, 255));
        pm.fill_rect(Rect::new(16, 16, 32, 32), Rgba8::new(200, 40, 90, 255));
        let (fore, back) = (Rgba8::new(0, 0, 200, 255), Rgba8::new(255, 255, 255, 255));
        halftone_pattern(&mut pm, 6, 50, HalftonePattern::Dot, fore, back);
        for p in pm.as_bytes().chunks_exact(4) {
            let is_fore = p[0] == fore.r && p[1] == fore.g && p[2] == fore.b;
            let is_back = p[0] == back.r && p[1] == back.g && p[2] == back.b;
            assert!(is_fore || is_back, "{:?} is neither swatch", p);
        }
    }

    #[test]
    fn halftone_pattern_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        halftone_pattern(&mut pm, 6, 50, HalftonePattern::Circle, BLACK, WHITE);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn halftone_pattern_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        halftone_pattern(&mut pm, 6, 50, HalftonePattern::Dot, BLACK, WHITE);
    }

    /// With no relief the sheet is flat: every pixel is the paper or the
    /// hole and nothing else, whatever the grain.
    #[test]
    fn note_paper_without_relief_is_two_tones() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(230, 230, 230, 255));
        pm.fill_rect(Rect::new(16, 16, 32, 32), Rgba8::new(20, 20, 20, 255));
        note_paper(&mut pm, 25, 20, 0, BLACK, WHITE);
        let hole = (255.0 * (1.0 - NOTE_INK)).round() as u8;
        let at = |x: usize, y: usize| pm.as_bytes()[(y * 64 + x) * 4];
        assert_eq!(at(2, 2), 255, "the paper is the background");
        assert_eq!(at(32, 32), hole, "the hole is a third of the way to black");
    }

    /// Image Balance is where the cut falls: a mid grey is paper at the
    /// bottom of the slider and a hole at the top.
    #[test]
    fn note_paper_balance_moves_the_cut() {
        let grey = Rgba8::new(128, 128, 128, 255);
        let mut low = Pixmap::filled(32, 32, grey);
        note_paper(&mut low, 5, 0, 0, BLACK, WHITE);
        let mut high = Pixmap::filled(32, 32, grey);
        note_paper(&mut high, 45, 0, 0, BLACK, WHITE);
        assert_eq!(low.as_bytes()[0], 255);
        assert!(high.as_bytes()[0] < 200);
    }

    /// The holes are cut below the paper and lit from above, so the wall
    /// along the top of a hole is in shadow and the one along its bottom
    /// catches the light.
    #[test]
    fn note_paper_shadows_the_top_edge_of_a_hole() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(230, 230, 230, 255));
        pm.fill_rect(Rect::new(0, 24, 64, 16), Rgba8::new(20, 20, 20, 255));
        note_paper(&mut pm, 25, 0, 20, BLACK, WHITE);
        let column = |y: usize| pm.as_bytes()[(y * 64 + 32) * 4];
        let hole = (255.0 * (1.0 - NOTE_INK)).round() as u8;
        let top = (21..27).map(column).min().unwrap();
        let bottom = (37..43).map(column).max().unwrap();
        assert!(top < hole - 30, "top wall {top} is not in shadow");
        assert!(bottom > hole + 30, "bottom wall {bottom} is not lit");
        assert_eq!(column(32), hole, "the floor of the hole is flat");
    }

    /// Graininess lays a texture over flat paper; without it the paper is
    /// flat whatever the relief.
    #[test]
    fn note_paper_graininess_textures_the_sheet() {
        let spread = |grain: u32| {
            let mut pm = Pixmap::filled(64, 64, Rgba8::new(230, 230, 230, 255));
            note_paper(&mut pm, 25, grain, 15, BLACK, WHITE);
            let v: Vec<f32> = pm.as_bytes().chunks_exact(4).map(|p| p[0] as f32).collect();
            let mean = v.iter().sum::<f32>() / v.len() as f32;
            (v.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / v.len() as f32).sqrt()
        };
        assert_eq!(spread(0), 0.0);
        assert!(spread(20) > 5.0);
    }

    /// It draws in the swatches: the paper is the background and the holes
    /// lean towards the foreground.
    #[test]
    fn note_paper_paints_in_the_two_swatches() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(230, 230, 230, 255));
        pm.fill_rect(Rect::new(16, 16, 32, 32), Rgba8::new(20, 20, 20, 255));
        let (fore, back) = (Rgba8::new(0, 0, 200, 255), Rgba8::new(255, 255, 0, 255));
        note_paper(&mut pm, 25, 0, 0, fore, back);
        let at = |x: usize, y: usize| {
            let p = &pm.as_bytes()[(y * 64 + x) * 4..][..3];
            (p[0], p[1], p[2])
        };
        assert_eq!(at(2, 2), (255, 255, 0));
        let (r, g, b) = at(32, 32);
        assert!(r < 255 && g < 255 && b > 0, "the hole ({r}, {g}, {b}) is not towards the foreground");
    }

    #[test]
    fn note_paper_is_deterministic() {
        let mut a = Pixmap::filled(48, 48, Rgba8::new(150, 120, 90, 255));
        a.fill_rect(Rect::new(10, 10, 20, 20), Rgba8::new(30, 30, 30, 255));
        let mut b = a.clone();
        note_paper(&mut a, 25, 10, 11, BLACK, WHITE);
        note_paper(&mut b, 25, 10, 11, BLACK, WHITE);
        assert_eq!(a.as_bytes(), b.as_bytes());
    }

    #[test]
    fn note_paper_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        note_paper(&mut pm, 25, 10, 11, BLACK, WHITE);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn note_paper_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        note_paper(&mut pm, 25, 10, 11, BLACK, WHITE);
    }

    /// A copier reproduces change, not tone: a flat sheet comes back as bare
    /// paper however dark it was.
    #[test]
    fn photocopy_leaves_flat_areas_as_paper() {
        for level in [10, 128, 240] {
            let mut pm = Pixmap::filled(32, 32, Rgba8::new(level, level, level, 255));
            photocopy(&mut pm, 7, 50, BLACK, WHITE);
            assert!(pm.as_bytes().chunks_exact(4).all(|p| p[0] == 255), "flat {level} took toner");
        }
    }

    /// Toner goes on the dark side of an edge and nowhere else.
    #[test]
    fn photocopy_inks_the_dark_side_of_an_edge() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(220, 220, 220, 255));
        pm.fill_rect(Rect::new(32, 0, 32, 64), Rgba8::new(40, 40, 40, 255));
        photocopy(&mut pm, 4, 30, BLACK, WHITE);
        let at = |x: usize| pm.as_bytes()[(32 * 64 + x) * 4];
        assert_eq!(at(33), 0, "the dark side of the edge is not toner");
        assert_eq!(at(30), 255, "the light side of the edge took toner");
        assert_eq!(at(62), 255, "the flat dark far from the edge took toner");
    }

    /// Detail widens the neighbourhood, so a wider band of the dark side is
    /// darker than what is round it.
    #[test]
    fn photocopy_detail_fills_in_more_of_a_dark_mass() {
        let inked = |detail: u32| {
            let mut pm = Pixmap::filled(96, 16, Rgba8::new(220, 220, 220, 255));
            pm.fill_rect(Rect::new(32, 0, 64, 16), Rgba8::new(40, 40, 40, 255));
            photocopy(&mut pm, detail, 30, BLACK, WHITE);
            (32..96).filter(|&x| pm.as_bytes()[(8 * 96 + x) * 4] < 128).count()
        };
        assert!(inked(20) > inked(2) + 5, "{} vs {}", inked(20), inked(2));
    }

    /// Darkness is how hard the difference is driven: soft grey at the
    /// bottom, a cut at the top.
    #[test]
    fn photocopy_darkness_deepens_the_toner() {
        let at = |darkness: u32| {
            let mut pm = Pixmap::filled(64, 16, Rgba8::new(160, 160, 160, 255));
            pm.fill_rect(Rect::new(32, 0, 32, 16), Rgba8::new(130, 130, 130, 255));
            photocopy(&mut pm, 4, darkness, BLACK, WHITE);
            pm.as_bytes()[(8 * 64 + 33) * 4]
        };
        assert!(at(1) > 200, "Darkness 1 gives {}", at(1));
        assert_eq!(at(50), 0);
    }

    #[test]
    fn photocopy_paints_in_the_two_swatches() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(30, 160, 60, 255));
        pm.fill_rect(Rect::new(16, 16, 32, 32), Rgba8::new(200, 40, 90, 255));
        let (fore, back) = (Rgba8::new(0, 0, 200, 255), Rgba8::new(255, 255, 0, 255));
        photocopy(&mut pm, 7, 50, fore, back);
        let mut toner = false;
        for p in pm.as_bytes().chunks_exact(4) {
            let is_fore = p[0] == fore.r && p[1] == fore.g && p[2] == fore.b;
            let is_back = p[0] == back.r && p[1] == back.g && p[2] == back.b;
            toner |= is_fore;
            assert!(is_fore || is_back || p[0] < 255, "{:?} is off the swatches' line", p);
        }
        assert!(toner, "no pixel took the foreground");
    }

    #[test]
    fn photocopy_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        photocopy(&mut pm, 7, 8, BLACK, WHITE);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn photocopy_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        photocopy(&mut pm, 7, 8, BLACK, WHITE);
    }

    /// With nothing dark enough to pour, the sheet is CS6's ramp: the
    /// background at the edge nearest the light, the foreground at the far
    /// one, linear in between.
    #[test]
    fn plaster_shades_the_sheet_as_a_ramp_away_from_the_light() {
        let mut pm = Pixmap::filled(64, 33, Rgba8::new(240, 240, 240, 255));
        plaster(&mut pm, 20, 2, Light::Top, BLACK, WHITE);
        let at = |x: usize, y: usize| pm.as_bytes()[(y * 64 + x) * 4];
        assert_eq!(at(10, 0), 255);
        assert_eq!(at(10, 32), 0);
        assert!((at(10, 16) as i32 - 128).abs() <= 1, "middle is {}", at(10, 16));
        assert_eq!(at(0, 16), at(63, 16), "a Top ramp does not change across a row");

        let mut pm = Pixmap::filled(64, 33, Rgba8::new(240, 240, 240, 255));
        plaster(&mut pm, 20, 2, Light::Left, BLACK, WHITE);
        let at = |x: usize, y: usize| pm.as_bytes()[(y * 64 + x) * 4];
        assert_eq!(at(0, 10), 255);
        assert_eq!(at(63, 10), 0);
    }

    /// What is darker than the cut sets as a pool of the foreground, and
    /// Image Balance moves the cut.
    #[test]
    fn plaster_pours_the_dark_as_foreground() {
        let grey = |balance: u32| {
            let mut pm = Pixmap::filled(64, 64, Rgba8::new(128, 128, 128, 255));
            plaster(&mut pm, balance, 2, Light::Top, Rgba8::new(0, 0, 200, 255), WHITE);
            let p = &pm.as_bytes()[(10 * 64 + 32) * 4..][..3];
            (p[0], p[1], p[2])
        };
        assert_eq!(grey(45), (0, 0, 200), "a mid grey is poured at a high balance");
        assert_ne!(grey(5), (0, 0, 200), "a mid grey is poured at a low balance");
    }

    /// The pools stand proud of the plaster on a bevel that lies on the
    /// plaster's side of the edge. Lit from the top, the shoulder above a
    /// pool faces the lamp and catches a bright band; the one below faces
    /// away and falls into shade; the pool itself stays flat foreground.
    #[test]
    fn plaster_lights_the_shoulder_facing_the_lamp() {
        // Wide enough that the bevel's blur does not reach its middle.
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(240, 240, 240, 255));
        pm.fill_rect(Rect::new(0, 20, 64, 24), Rgba8::new(10, 10, 10, 255));
        let mut flat = Pixmap::filled(64, 64, Rgba8::new(240, 240, 240, 255));
        plaster(&mut pm, 20, 1, Light::Top, BLACK, WHITE);
        plaster(&mut flat, 20, 1, Light::Top, BLACK, WHITE);
        let at = |pm: &Pixmap, y: usize| pm.as_bytes()[(y * 64 + 32) * 4] as i32;
        let diff = |y: usize| at(&pm, y) - at(&flat, y);
        let lit = (8..20).map(diff).max().unwrap();
        // The ramp is already dark down there, so the shade is measured as a
        // share of it rather than in levels.
        let shade = (44..56)
            .map(|y| at(&pm, y) as f32 / at(&flat, y).max(1) as f32)
            .fold(f32::INFINITY, f32::min);
        assert!(lit > 60, "the shoulder facing the lamp is only {lit} above the ramp");
        assert!(shade < 0.85, "the shoulder facing away keeps {shade} of the ramp");
        assert_eq!(at(&pm, 32), 0, "the top of the pool is flat foreground");
    }



    fn mean_tone(pm: &Pixmap) -> f32 {
        let bytes = pm.as_bytes();
        bytes.chunks_exact(4).map(|p| p[0] as f32).sum::<f32>() / (bytes.len() / 4) as f32
    }

    fn spread(pm: &Pixmap) -> f32 {
        let mean = mean_tone(pm);
        let bytes = pm.as_bytes();
        (bytes.chunks_exact(4).map(|p| (p[0] as f32 - mean).powi(2)).sum::<f32>()
            / (bytes.len() / 4) as f32)
            .sqrt()
    }

    /// The grain rides on the picture: a light sheet stays light and a dark
    /// one dark, each with a grain through it — not two swatches thrown down
    /// at random.
    #[test]
    fn reticulation_keeps_the_picture_s_tone() {
        let mut light = Pixmap::filled(96, 96, Rgba8::new(225, 225, 225, 255));
        let mut dark = Pixmap::filled(96, 96, Rgba8::new(35, 35, 35, 255));
        // Moderate levels, so the curve is not what is being tested.
        reticulation(&mut light, 12, 20, 20, BLACK, WHITE);
        reticulation(&mut dark, 12, 20, 20, BLACK, WHITE);
        assert!(mean_tone(&light) > mean_tone(&dark) + 100.0);
        assert!(spread(&light) > 10.0, "no grain in the light");
        assert!(spread(&dark) > 10.0, "no grain in the dark");
    }

    /// Density is how hard the grain shakes the tone.
    #[test]
    fn reticulation_density_strengthens_the_grain() {
        let run = |density: u32| {
            let mut pm = Pixmap::filled(96, 96, Rgba8::new(128, 128, 128, 255));
            reticulation(&mut pm, density, 20, 20, BLACK, WHITE);
            spread(&pm)
        };
        assert!(run(50) > run(0) + 5.0, "{} vs {}", run(50), run(0));
    }

    /// Foreground Level darkens the midtones; Background Level lightens the
    /// highlights.
    #[test]
    fn reticulation_levels_move_the_tone() {
        let run = |tone: u8, fore: u32, back: u32| {
            let mut pm = Pixmap::filled(96, 96, Rgba8::new(tone, tone, tone, 255));
            reticulation(&mut pm, 12, fore, back, BLACK, WHITE);
            mean_tone(&pm)
        };
        assert!(run(140, 50, 5) < run(140, 10, 5) - 40.0, "midtones did not darken");
        assert!(run(220, 20, 40) > run(220, 20, 0) + 15.0, "highlights did not lighten");
    }

    /// It draws in the swatches: a blue ink and a yellow paper mean every
    /// pixel lies between the two.
    #[test]
    fn reticulation_paints_between_the_two_swatches() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(128, 128, 128, 255));
        reticulation(&mut pm, 12, 40, 5, Rgba8::new(0, 0, 255, 255), Rgba8::new(255, 255, 0, 255));
        for p in pm.as_bytes().chunks_exact(4) {
            assert_eq!(p[0], p[1], "{:?} is off the line between the swatches", p);
            assert_eq!(p[0] as u16 + p[2] as u16, 255, "{:?} is off the line", p);
        }
    }

    #[test]
    fn reticulation_is_deterministic_and_leaves_alpha_alone() {
        let mut a = Pixmap::filled(48, 48, Rgba8::new(120, 140, 160, 77));
        a.fill_rect(Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = a.clone();
        let mut b = a.clone();
        reticulation(&mut a, 12, 40, 5, BLACK, WHITE);
        reticulation(&mut b, 12, 40, 5, BLACK, WHITE);
        assert_eq!(a.as_bytes(), b.as_bytes());
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&a), alpha(&before));
    }

    #[test]
    fn reticulation_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        reticulation(&mut pm, 12, 40, 5, BLACK, WHITE);
    }

    /// A stamp is two colours: away from an edge every pixel is one swatch
    /// or the other.
    #[test]
    fn stamp_is_two_tones_away_from_its_edges() {
        let mut pm = Pixmap::filled(96, 96, Rgba8::new(230, 230, 230, 255));
        pm.fill_rect(Rect::new(0, 0, 48, 96), Rgba8::new(20, 20, 20, 255));
        stamp(&mut pm, 25, 1, BLACK, WHITE);
        let at = |x: usize| pm.as_bytes()[(48 * 96 + x) * 4];
        assert_eq!(at(10), 0);
        assert_eq!(at(86), 255);
    }

    /// Light/Dark Balance moves the cut: a mid grey is paper at the bottom
    /// of the slider and ink at the top.
    #[test]
    fn stamp_balance_moves_the_cut() {
        let run = |balance: u32| {
            let mut pm = Pixmap::filled(32, 32, Rgba8::new(110, 110, 110, 255));
            stamp(&mut pm, balance, 5, BLACK, WHITE);
            pm.as_bytes()[(16 * 32 + 16) * 4]
        };
        assert_eq!(run(0), 255);
        assert_eq!(run(50), 0);
    }

    /// Smoothness melts detail away: a fine chequer of ink and paper is
    /// picked out at the bottom of the slider and dissolves into one tone at
    /// the top.
    #[test]
    fn stamp_smoothness_melts_fine_detail() {
        // Wide enough that the middle is clear of the borders even for the
        // widest melt, where the blur repeats the outermost stripe and
        // tips the tone to one side of the cut.
        let changes = |smoothness: u32| {
            let mut pm = Pixmap::filled(256, 16, Rgba8::new(230, 230, 230, 255));
            for i in 0..32 {
                pm.fill_rect(Rect::new(i * 8, 0, 4, 16), Rgba8::new(10, 10, 10, 255));
            }
            stamp(&mut pm, 25, smoothness, BLACK, WHITE);
            let row = &pm.as_bytes()[(8 * 256 + 96) * 4..(8 * 256 + 160) * 4];
            row.chunks_exact(4)
                .zip(row.chunks_exact(4).skip(1))
                .filter(|(a, b)| (a[0] > 127) != (b[0] > 127))
                .count()
        };
        assert!(changes(1) >= 12, "fine stripes lost at Smoothness 1: {}", changes(1));
        assert_eq!(changes(50), 0, "fine stripes survived Smoothness 50");
    }


    /// A thin dark line on a light ground is inked even where it is not
    /// darker than the cut on its own — it is darker than what is round it.
    #[test]
    fn stamp_picks_up_a_thin_line_on_light_ground() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(220, 220, 220, 255));
        pm.fill_rect(Rect::new(0, 31, 64, 2), Rgba8::new(140, 140, 140, 255));
        let mut flat = Pixmap::filled(64, 64, Rgba8::new(140, 140, 140, 255));
        stamp(&mut pm, 25, 1, BLACK, WHITE);
        stamp(&mut flat, 25, 1, BLACK, WHITE);
        assert_eq!(flat.as_bytes()[0], 255, "the line's own tone is above the cut");
        assert_eq!(pm.as_bytes()[(32 * 64 + 32) * 4], 0, "the line was not picked up");
    }

    #[test]
    fn stamp_paints_in_the_swatches_and_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(230, 230, 230, 77));
        pm.fill_rect(Rect::new(0, 0, 32, 64), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        let (fore, back) = (Rgba8::new(0, 0, 200, 255), Rgba8::new(255, 255, 0, 255));
        stamp(&mut pm, 25, 1, fore, back);
        let px = |x: usize| {
            let p = &pm.as_bytes()[(32 * 64 + x) * 4..][..3];
            (p[0], p[1], p[2])
        };
        assert_eq!(px(4), (0, 0, 200));
        assert_eq!(px(60), (255, 255, 0));
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn stamp_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        stamp(&mut pm, 25, 5, BLACK, WHITE);
    }

    fn inked_share(pm: &Pixmap) -> f32 {
        let bytes = pm.as_bytes();
        bytes.chunks_exact(4).filter(|p| p[0] < 128).count() as f32 / (bytes.len() / 4) as f32
    }

    /// Paper stays paper: the grain only takes ink away, so a light sheet is
    /// clean background at every Contrast.
    #[test]
    fn torn_edges_leaves_the_paper_clean() {
        for contrast in [1, 12, 25] {
            let mut pm = Pixmap::filled(64, 64, Rgba8::new(235, 235, 235, 255));
            torn_edges(&mut pm, 25, 11, contrast, BLACK, WHITE);
            assert!(pm.as_bytes().chunks_exact(4).all(|p| p[0] == 255), "Contrast {contrast}");
        }
    }

    /// Image Balance is the cut: a mid grey is paper at the bottom of the
    /// slider and ink at the top.
    #[test]
    fn torn_edges_balance_moves_the_cut() {
        let run = |balance: u32| {
            let mut pm = Pixmap::filled(64, 64, Rgba8::new(120, 120, 120, 255));
            torn_edges(&mut pm, balance, 11, 1, BLACK, WHITE);
            inked_share(&pm)
        };
        assert_eq!(run(0), 0.0);
        assert!(run(50) > 0.9, "only {} inked at 50", run(50));
    }

    /// Contrast is how hard the grain bites: solid ink at the bottom of the
    /// slider, riddled with holes at the top.
    #[test]
    fn torn_edges_contrast_bites_holes_in_the_ink() {
        let run = |contrast: u32| {
            let mut pm = Pixmap::filled(96, 96, Rgba8::new(10, 10, 10, 255));
            torn_edges(&mut pm, 25, 11, contrast, BLACK, WHITE);
            inked_share(&pm)
        };
        assert!(run(1) > 0.95, "{} inked at Contrast 1", run(1));
        assert!(run(25) < 0.75, "{} inked at Contrast 25", run(25));
    }

    /// The edge frays: across a boundary there are pixels of both kinds in a
    /// band, not one clean step — and a low Smoothness frays it wider.
    #[test]
    fn torn_edges_frays_the_boundary() {
        let fringe = |smoothness: u32| {
            let mut pm = Pixmap::filled(128, 64, Rgba8::new(235, 235, 235, 255));
            pm.fill_rect(Rect::new(0, 0, 64, 64), Rgba8::new(10, 10, 10, 255));
            torn_edges(&mut pm, 25, smoothness, 12, BLACK, WHITE);
            // Columns whose average is well clear of both the flecked ink
            // inside the shape and the clean paper outside it.
            let mean = |x: usize| {
                (0..64).map(|y| pm.as_bytes()[(y * 128 + x) * 4] as f32).sum::<f32>() / 64.0
            };
            let inside = mean(8);
            (0..128)
                .filter(|&x| {
                    let m = mean(x);
                    m > inside + 20.0 && m < 235.0
                })
                .count()
        };
        assert!(fringe(15) >= 2, "no fringe at all");
        assert!(fringe(2) > fringe(15), "{} vs {}", fringe(2), fringe(15));
    }

    #[test]
    fn torn_edges_is_deterministic_and_leaves_alpha_alone() {
        let mut a = Pixmap::filled(48, 48, Rgba8::new(120, 140, 160, 77));
        a.fill_rect(Rect::new(4, 4, 20, 20), Rgba8::new(20, 20, 20, 200));
        let before = a.clone();
        let mut b = a.clone();
        torn_edges(&mut a, 25, 11, 17, BLACK, WHITE);
        torn_edges(&mut b, 25, 11, 17, BLACK, WHITE);
        assert_eq!(a.as_bytes(), b.as_bytes());
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&a), alpha(&before));
    }

    #[test]
    fn torn_edges_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        torn_edges(&mut pm, 25, 11, 17, BLACK, WHITE);
    }

    #[test]
    fn plaster_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 140, 160, 77));
        pm.fill_rect(Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = pm.clone();
        plaster(&mut pm, 20, 2, Light::Top, BLACK, WHITE);
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&pm), alpha(&before));
    }

    #[test]
    fn plaster_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        plaster(&mut pm, 20, 2, Light::Top, BLACK, WHITE);
    }

    /// A one-pixel sheet has no ramp to lay; it must not divide by zero.
    #[test]
    fn plaster_over_a_single_pixel_is_finite() {
        let mut pm = Pixmap::filled(1, 1, Rgba8::new(240, 240, 240, 255));
        plaster(&mut pm, 20, 2, Light::TopLeft, BLACK, WHITE);
        assert_eq!(pm.as_bytes()[0], 255);
    }
    /// The colour runs along the fibres: a dark bar bleeds out into the
    /// paper either side of it, down and across, and a flat sheet with
    /// nothing to bleed stays the colour it was, give or take the weave.
    #[test]
    fn water_paper_bleeds_colour_out_of_a_shape() {
        let bar = || {
            let mut pm = Pixmap::filled(96, 96, Rgba8::new(200, 200, 200, 255));
            pm.fill_rect(Rect::new(44, 0, 8, 96), Rgba8::new(20, 20, 20, 255));
            pm
        };
        // Brightness and Contrast at their flat middles, so only the bleed
        // is being tested.
        let mut short = bar();
        water_paper(&mut short, 3, 50, 50);
        let mut long = bar();
        water_paper(&mut long, 50, 50, 50);
        let beside = |pm: &Pixmap| (0..96).map(|y| pm.get(40, y).r as f32).sum::<f32>() / 96.0;
        assert!(
            beside(&long) < beside(&short) - 10.0,
            "longer fibres did not carry the ink further: {} against {}",
            beside(&long),
            beside(&short)
        );
    }

    /// Brightness lifts and sinks the picture; Contrast pulls its ends apart.
    #[test]
    fn water_paper_brightness_and_contrast_move_the_tone() {
        let run = |tone: u8, brightness: u32, contrast: u32| {
            let mut pm = Pixmap::filled(64, 64, Rgba8::new(tone, tone, tone, 255));
            water_paper(&mut pm, 15, brightness, contrast);
            mean_tone(&pm)
        };
        assert!(run(128, 90, 50) > run(128, 50, 50) + 30.0, "brightness did not lift");
        assert!(run(128, 10, 50) < run(128, 50, 50) - 30.0, "brightness did not sink");
        let span = |contrast| run(200, 50, contrast) - run(60, 50, contrast);
        assert!(span(90) > span(20) + 60.0, "{} vs {}", span(90), span(20));
    }

    /// Unlike the rest of the family it keeps the picture's hue: a red sheet
    /// stays red.
    #[test]
    fn water_paper_keeps_the_picture_s_colours() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(200, 40, 40, 255));
        water_paper(&mut pm, 15, 60, 80);
        let p = pm.get(32, 32);
        assert!(p.r as i32 > p.g as i32 + 80 && p.r as i32 > p.b as i32 + 80, "{:?}", p);
    }

    /// The weave shows in the ink, not on bare paper.
    #[test]
    fn water_paper_weave_shows_in_the_dark() {
        let run = |tone: u8| {
            let mut pm = Pixmap::filled(96, 96, Rgba8::new(tone, tone, tone, 255));
            water_paper(&mut pm, 15, 50, 50);
            spread(&pm)
        };
        assert!(run(40) > run(230) + 2.0, "{} vs {}", run(40), run(230));
    }

    #[test]
    fn water_paper_is_deterministic_and_leaves_alpha_alone() {
        let mut a = Pixmap::filled(48, 48, Rgba8::new(120, 140, 160, 77));
        a.fill_rect(Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = a.clone();
        let mut b = a.clone();
        water_paper(&mut a, 15, 60, 80);
        water_paper(&mut b, 15, 60, 80);
        assert_eq!(a.as_bytes(), b.as_bytes());
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&a), alpha(&before));
    }

    #[test]
    fn water_paper_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        water_paper(&mut pm, 15, 60, 80);
    }
}

//! Filter ▸ Sketch.
//!
//! CS6 keeps this family in the Filter Gallery, as it does Artistic and Brush
//! Strokes. The Gallery is not built (docs/ROADMAP.md), so the filters live
//! under a Filter ▸ Sketch submenu instead.
//!
//! What binds the family together is that **it paints in the two swatches**,
//! not in the picture's own colours: every one of these fourteen throws the
//! hue away, works on brightness alone, and maps the answer between the
//! document's foreground and background. The colours are properties of the
//! document rather than of the dialog, so the bridge fills them in — see
//! `Engine::filter_for`.

use crate::buffer::{Pixmap, Rgba8};
use crate::filters::artistic::blur_field;
use crate::filters::brush_strokes::unit_spread;
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
}

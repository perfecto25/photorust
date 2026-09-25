//! The surface textures CS6 lays under several of its filters.
//!
//! Rough Pastels, Underpainting, Conté Crayon, Texturizer and Glass all carry
//! the same block of controls — Texture, Scaling, Relief, Light and Invert —
//! and all do the same thing with it: read a grey texture as a height map and
//! light the picture as if it were printed on that surface. This is that block,
//! once, for all of them. Rough Pastels is the first to use it.
//!
//! CS6 ships its four textures as small bitmaps. These are procedural height
//! maps drawn to match what those look like at 100%, so there is nothing to
//! load and they tile at any size. CS6's "Load Texture…", which reads a
//! Photoshop file as the texture, is not built.
//!
//! The Filter ▸ Texture family lives here too, starting with Craquelure: it
//! lights the picture as a surface in the same way, but draws that surface
//! itself rather than taking one of the four.

use crate::buffer::Pixmap;
use rayon::prelude::*;

/// CS6's ranges for the shared texture controls.
pub const SCALING: std::ops::RangeInclusive<u32> = 50..=200;
pub const RELIEF: std::ops::RangeInclusive<u32> = 0..=50;

/// CS6's four textures, in the order its list gives them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Texture {
    /// Courses of bricks in running bond, with sunken mortar between them.
    Brick,
    /// Coarse sacking: an uneven weave of thick, wandering threads.
    Burlap,
    /// A fine, even weave.
    #[default]
    Canvas,
    /// Gritty stone, lumpy at several scales at once.
    Sandstone,
}

impl Texture {
    pub fn from_i32(value: i32) -> Texture {
        match value {
            0 => Texture::Brick,
            1 => Texture::Burlap,
            3 => Texture::Sandstone,
            _ => Texture::Canvas,
        }
    }
}

/// Where CS6's texture light comes from, in the order its list gives them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Light {
    #[default]
    Bottom,
    BottomLeft,
    Left,
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
}

impl Light {
    pub fn from_i32(value: i32) -> Light {
        match value {
            1 => Light::BottomLeft,
            2 => Light::Left,
            3 => Light::TopLeft,
            4 => Light::Top,
            5 => Light::TopRight,
            6 => Light::Right,
            7 => Light::BottomRight,
            _ => Light::Bottom,
        }
    }

    /// Which way the light points from, as a unit step in image coordinates
    /// (y down).
    pub fn towards(self) -> (f32, f32) {
        let d = std::f32::consts::FRAC_1_SQRT_2;
        match self {
            Light::Bottom => (0.0, 1.0),
            Light::BottomLeft => (-d, d),
            Light::Left => (-1.0, 0.0),
            Light::TopLeft => (-d, -d),
            Light::Top => (0.0, -1.0),
            Light::TopRight => (d, -d),
            Light::Right => (1.0, 0.0),
            Light::BottomRight => (d, d),
        }
    }
}

/// The texture as a height field, 0 low and 1 high, one value per pixel.
/// `scaling` is CS6's percentage: at 200 every feature is twice the size.
///
/// Laid by where on the canvas a pixel is, so it lines up across the whole
/// image — and so a crop of the image gets a different part of it.
pub fn height_map(texture: Texture, width: u32, height: u32, scaling: u32) -> Vec<f32> {
    let zoom = 100.0 / scaling.clamp(*SCALING.start(), *SCALING.end()) as f32;
    let width = width as usize;
    let mut field = vec![0.0f32; width * height as usize];
    field
        .par_chunks_exact_mut(width.max(1))
        .enumerate()
        .for_each(|(y, row)| {
            for (x, h) in row.iter_mut().enumerate() {
                let (u, v) = (x as f32 * zoom, y as f32 * zoom);
                *h = match texture {
                    Texture::Brick => brick(u, v),
                    Texture::Burlap => burlap(u, v),
                    Texture::Canvas => canvas(u, v),
                    Texture::Sandstone => sandstone(u, v),
                };
            }
        });
    field
}

/// How much a unit of slope lights or shades the picture at the top of Relief:
/// some of it in proportion to the colour, most of it as plain light and
/// shadow, so the texture shows as clearly in a black as in a white.
const SHADE_BY_COLOUR: f32 = 0.8;
const SHADE_BY_LIGHT: f32 = 150.0;

/// How far the height field is softened before it is lit, in pixels, so a
/// hard-edged texture casts a bevel rather than a one-pixel line.
const BEVEL: f32 = 0.6;

/// Light the picture as if it were printed on `texture`.
///
/// **Scaling** is the size of the texture. **Relief** is how deep it is.
/// **Light** is where the light comes from, and **Invert** turns the surface
/// inside out, so what stood up is sunk.
///
/// Alpha is left alone.
///
/// No GPU path: the height field is drawn and lit in one pass each, and the
/// result is wanted straight back on the CPU by the filter that asked.
pub fn apply_relief(
    pixmap: &mut Pixmap,
    texture: Texture,
    scaling: u32,
    relief: u32,
    light: Light,
    invert: bool,
) {
    apply_relief_weighted(pixmap, texture, scaling, relief, light, invert, Finish::default());
}

/// How a surface is lit, beyond the controls CS6 shows.
#[derive(Clone, Copy, Debug)]
pub struct Finish {
    /// How much less the surface shows in the light than in the dark: 0 lights
    /// every tone alike; at 1 white is untouched and black takes the full
    /// relief.
    pub dark_bias: f32,
    /// How far the height field is softened before it is lit, in pixels.
    pub bevel: f32,
    /// A power on the slope below 1 lifts faint slopes towards the strong
    /// ones, so the surface reads as hard glints and cracks rather than as a
    /// soft emboss.
    pub crisp: f32,
    /// How many levels the low parts of the surface are darkened by, as the
    /// shadow in a groove is, whichever way the light falls. It is what makes
    /// a joint or a valley show under a light that runs along it.
    pub occlusion: f32,
    /// How much of the surface fades out in broad, random patches: 0 is
    /// even everywhere, 1 lets some patches go smooth. See [`patchiness`].
    pub patchy: f32,
    /// How many times deeper the surface is than Relief alone makes it.
    pub gain: f32,
}

impl Default for Finish {
    fn default() -> Finish {
        Finish { dark_bias: 0.0, bevel: BEVEL, crisp: 1.0, occlusion: 0.0, patchy: 0.0, gain: 1.0 }
    }
}

/// How broad the patches [`patchiness`] fades the surface in are, in pixels.
const PATCH_SCALE: f32 = 36.0;

/// How strongly the surface shows at a pixel, 1 fully and less in broad
/// random patches: `patchy` 0 gives 1 everywhere. CS6's surface is not laid
/// evenly — here and there it thins to nothing, and the picture shows through
/// soft — and a surface laid the same everywhere reads as a machine print.
pub fn patchiness(x: usize, y: usize, patchy: f32) -> f32 {
    if patchy <= 0.0 {
        return 1.0;
    }
    let n = 0.6 * value_noise(x as f32, y as f32, PATCH_SCALE, 51)
        + 0.4 * value_noise(x as f32, y as f32, PATCH_SCALE * 0.45, 52);
    // Most of the canvas keeps its surface; the lowest third fades out.
    let t = ((n - 0.3) / 0.25).clamp(0.0, 1.0);
    1.0 - patchy * (1.0 - t * t * (3.0 - 2.0 * t))
}

/// [`apply_relief`], with the [`Finish`] set by the filter. Underpainting's
/// paint is thin over a pale sky and thick where the picture is dark, and its
/// surface is glassy rather than soft, and CS6 shows it that way.
pub fn apply_relief_weighted(
    pixmap: &mut Pixmap,
    texture: Texture,
    scaling: u32,
    relief: u32,
    light: Light,
    invert: bool,
    finish: Finish,
) {
    let relief = relief.clamp(*RELIEF.start(), *RELIEF.end()) as f32 / *RELIEF.end() as f32;
    if relief <= 0.0 || pixmap.is_empty() {
        return;
    }
    let (width, height) = (pixmap.width() as usize, pixmap.height() as usize);
    let mut field = height_map(texture, pixmap.width(), pixmap.height(), scaling);
    if invert {
        field.par_iter_mut().for_each(|h| *h = 1.0 - *h);
    }
    crate::filters::artistic::blur_field(&mut field, width, height, finish.bevel);

    let (lx, ly) = light.towards();
    let field = &field;
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, out)| {
            let (up, down) = (y.saturating_sub(1), (y + 1).min(height - 1));
            for (x, px) in out.chunks_exact_mut(4).enumerate() {
                let (left, right) = (x.saturating_sub(1), (x + 1).min(width - 1));
                let dx = (field[y * width + right] - field[y * width + left])
                    / (right - left).max(1) as f32;
                let dy = (field[down * width + x] - field[up * width + x])
                    / (down - up).max(1) as f32;
                // A slope that falls away from the light is lit.
                let lightness = (0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32) / 255.0;
                let weight = 1.0 - finish.dark_bias * lightness;
                let slope = -(dx * lx + dy * ly);
                let slope = slope.signum() * slope.abs().powf(finish.crisp);
                let patch = patchiness(x, y, finish.patchy);
                let shade = slope * relief * weight * patch * finish.gain;
                let groove = (0.5 - field[y * width + x]).max(0.0) * 2.0
                    * finish.occlusion * relief * weight * patch;
                for c in 0..3 {
                    let v = px[c] as f32 * (1.0 + shade * SHADE_BY_COLOUR) + shade * SHADE_BY_LIGHT
                        - groove;
                    px[c] = v.clamp(0.0, 255.0).round() as u8;
                }
            }
        });
}

/// Brick: courses 9 pixels high of long bricks, every other course offset by
/// half a brick, with a rough, pitted face.
///
/// What CS6's brick is made of is mostly *face*, not mortar. Lit from below,
/// the deep horizontal joints between courses are what shows — a ruled page of
/// lines. Lit from the side, those joints catch no light at all, the shallow
/// vertical ones barely any, and what is left is the grit of the brick itself:
/// a dense scatter of short vertical ticks, because the pits are taller than
/// they are wide. A regular grid of dashes is what a smooth-faced brick with
/// deep joints gives instead, and it reads as a pattern rather than a surface.
fn brick(u: f32, v: f32) -> f32 {
    const COURSE: f32 = 9.0;
    const LENGTH: f32 = 40.0;
    const MORTAR: f32 = 1.4;
    const JOINT_DEPTH: f32 = 0.35;
    const GRIT: f32 = 0.9;
    let row = (v / COURSE).floor();
    let offset = if row.rem_euclid(2.0) > 0.5 { LENGTH / 2.0 } else { 0.0 };
    let fy = v - row * COURSE;
    let fx = (u + offset).rem_euclid(LENGTH);
    let bed = ((fy.min(COURSE - fy) - MORTAR * 0.5) / MORTAR).clamp(0.0, 1.0);
    let head = ((fx.min(LENGTH - fx) - MORTAR * 0.5) / MORTAR).clamp(0.0, 1.0);
    let joints = bed * (1.0 - JOINT_DEPTH * (1.0 - head));
    // Pits twice as tall as they are wide, at two sizes.
    let grit = 0.6 * value_noise(u * 2.0, v, 3.0, 3) + 0.4 * value_noise(u * 2.0, v, 1.5, 5);
    joints * (1.0 - GRIT + GRIT * grit)
}

/// Burlap: coarse sacking, read as CS6 draws it — threads running across,
/// about 7 pixels to a thread, with a groove between each and the next.
///
/// CS6's burlap is not a round-threaded basket weave. Lit from the top it is
/// dark grooves on a ground that keeps the picture's own tone, and the
/// grooves are not straight: each runs a few pixels, then steps up or down
/// where a thread crossing underneath lifts it, and some runs are shallow
/// enough to break the line into dashes. The threads running down show only
/// as those kinks and a faint dip at each crossing. A soft sine weave both
/// ways — the obvious model — reads as knitting.
///
/// Fitted to CS6's Texturizer over `samples/horse-3.jpg`, registered against
/// the source, on how the pattern repeats down a column — every 7 pixels,
/// and about as strongly every 14 — and how quickly it changes along a row.
/// The step is what sets the first: kinks much smaller than this and the
/// grooves line up into a ruled page, much bigger and the rows dissolve.
fn burlap(u: f32, v: f32) -> f32 {
    const PITCH: f32 = 7.2;
    // How long a groove runs between kinks, how far a kink steps it, and how
    // much of a run the step takes.
    const RUN: f32 = 4.0;
    const KINK: f32 = 1.4;
    const WEAVE: f32 = 0.35;
    const DRIFT: f32 = 12.0;
    const STEP: f32 = 0.45;
    // The groove's half-width, and how deep the crossings dip.
    const GROOVE: f32 = 1.3;
    const CROSSING: f32 = 0.15;
    let smooth = |t: f32| {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let below = (v / PITCH).floor() as i32;
    let mut h = 1.0f32;
    // The groove above the pixel and the one below; a kink never moves a
    // groove far enough for the next one out to reach.
    for row in [below, below + 1] {
        // The threads running down are straight enough that the crossings
        // line up in columns, half a run apart from one groove to the next
        // as the weave goes over and under. The columns wander, slowly enough that neighbouring grooves keep
        // in step but not so slowly that the cloth reads as ruled.
        let wander = (value_noise(u, row as f32 * PITCH, DRIFT, 26) - 0.5) * 2.0;
        let t = u / RUN + 0.5 * row.rem_euclid(2) as f32 + wander;
        let k = t.floor() as i32;
        let blend = smooth((t - k as f32 - (1.0 - STEP)) / STEP);
        let at = |k: i32| {
            // Up at one crossing and down at the next, the other way round
            // in the next groove, and never quite the same twice. That
            // alternation is why CS6's burlap repeats every two threads more
            // strongly than every one.
            let weave = if (k + row).rem_euclid(2) == 0 { 1.0 } else { -1.0 };
            let shift = (WEAVE * weave + (lattice(k, row, 23) - 0.5) * 2.0) * KINK;
            // Most runs are cut deep; one in four or so barely at all.
            let depth = (lattice(k, row, 24) * 1.4 - 0.1).clamp(0.25, 1.0);
            (shift, depth)
        };
        let ((s0, d0), (s1, d1)) = (at(k), at(k + 1));
        let shift = s0 + (s1 - s0) * blend;
        let depth = d0 + (d1 - d0) * blend;
        let d = (v - (row as f32 * PITCH + shift)) / GROOVE;
        h -= depth * (-d * d).exp();
        // The dip where a thread running down passes under, on the thread
        // just below this groove.
        let across = (t - (k + 1) as f32) * RUN;
        let under = (v - row as f32 * PITCH) / PITCH;
        if (0.0..1.0).contains(&under) {
            h -= CROSSING * (-(across * across) / 0.8).exp() * (std::f32::consts::PI * under).sin();
        }
    }
    // Hairy fibre along the threads.
    let fibre = value_noise(u * 0.7, v * 1.4, 1.2, 7);
    (h * (0.75 + 0.25 * fibre)).max(0.0)
}

/// Canvas: a fine plain weave, 3 pixels to a thread, the threads running down
/// standing a little prouder than those running across, over a little grain.
/// The threads wander a pixel or so and vary in thickness, as real canvas
/// does; a perfectly regular weave reads as ruled lines, not as cloth.
fn canvas(u: f32, v: f32) -> f32 {
    use std::f32::consts::PI;
    const PITCH: f32 = 3.0;
    const WANDER: f32 = 0.5;
    let u = u + (value_noise(u, v, 6.0, 41) - 0.5) * 2.0 * WANDER;
    let v = v + (value_noise(u, v, 6.0, 42) - 0.5) * 2.0 * WANDER;
    let down = (u / PITCH * PI).sin().abs();
    let across = (v / PITCH * PI).sin().abs();
    let over = ((u / PITCH).floor() + (v / PITCH).floor()).rem_euclid(2.0);
    let weave = if over > 0.5 { 0.6 * down + 0.3 * across } else { 0.4 * down + 0.5 * across };
    let slub = 0.75 + 0.5 * value_noise(u, v, 3.0, 43);
    (weave * slub + 0.2 * value_noise(u, v, 2.0, 4)) * 0.85
}

/// Sandstone: grain at three scales.
fn sandstone(u: f32, v: f32) -> f32 {
    0.5 * value_noise(u, v, 2.0, 1) + 0.3 * value_noise(u, v, 5.0, 2) + 0.2 * value_noise(u, v, 12.0, 3)
}

/// Smooth 0..1 noise on a lattice `cell` pixels apart.
fn value_noise(u: f32, v: f32, cell: f32, seed: u32) -> f32 {
    let (gu, gv) = (u / cell, v / cell);
    let (iu, iv) = (gu.floor(), gv.floor());
    let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
    let (fu, fv) = (smooth(gu - iu), smooth(gv - iv));
    let at = |du: i32, dv: i32| lattice(iu as i32 + du, iv as i32 + dv, seed);
    let top = at(0, 0) * (1.0 - fu) + at(1, 0) * fu;
    let bottom = at(0, 1) * (1.0 - fu) + at(1, 1) * fu;
    top * (1.0 - fv) + bottom * fv
}

fn lattice(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1)
        ^ (y as u32).wrapping_mul(0x85EB_CA77)
        ^ seed.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545_F491);
    h ^= h >> 13;
    (h & 0xFFFF) as f32 / 65535.0
}

/// CS6's ranges for Craquelure, which its three sliders run over.
pub const CRACK_SPACING: std::ops::RangeInclusive<u32> = 2..=100;
pub const CRACK_DEPTH: std::ops::RangeInclusive<u32> = 0..=10;
pub const CRACK_BRIGHTNESS: std::ops::RangeInclusive<u32> = 0..=10;

/// How wide a plate of paint is, in pixels, per step of Crack Spacing. Read
/// off CS6 on `samples/horse-3.jpg`: at 15 the plates are a dozen pixels
/// across, at 60 the sky is ruled into blocks about fifty wide.
const CRACK_CELL_PER_STEP: f32 = 0.85;

/// How tall a course of plates is, as a share of their width. CS6's plates
/// are laid in rough courses, wider than they are tall: its sky is ruled by
/// cracks running mostly across, broken up by shorter ones running down.
const CRACK_COURSE: f32 = 0.8;

/// How far each crack between courses is pushed up or down from where an
/// even course would put it, as a share of a course, and how wide a stretch
/// of it moves together, as a share of a plate. Evenly spaced courses are
/// brickwork however broken up they are: CS6's rows are uneven, and a crack
/// running across steps up or down every so often rather than running on at
/// one height.
const CRACK_COURSE_JITTER: f32 = 0.42;
const CRACK_COURSE_STEP: f32 = 2.6;

/// How far apart the cracks that break a course run, as a share of a plate.
const CRACK_BREAK: f32 = 1.5;

/// The courses are laid twice over: once at the plate size and again finer,
/// this size, shut more and drawn fainter. One size of plate repeats; CS6's
/// sky has cracks of every length between its long ones.
const CRACK_FINE: f32 = 0.6;
const CRACK_FINE_GAP: f32 = 0.22;
const CRACK_FINE_FAINT: f32 = 1.8;

/// How far the cracks wander off straight: a slow sway and a kink at half a
/// plate, both as shares of a plate, and a fine wobble in pixels but no more
/// than a share of a plate. CS6's cracks run across and down, but none of
/// them is ruled; wobbled as hard as big ones, small plates lose their
/// direction altogether.
const CRACK_WANDER: f32 = 0.18;
const CRACK_KINK: f32 = 0.12;
const CRACK_WOBBLE: f32 = 1.3;
const CRACK_WOBBLE_MAX: f32 = 0.08;

/// The crumple: cracks along the contours of a fractal noise, which meander
/// every which way at every scale at once. CS6's dark horse is crumpled all
/// over like this, with no two chips alike; a Voronoi diagram's chips are
/// all one size and read as a jigsaw. The noise is three octaves from this
/// share of a plate, and a crack runs every [`CRACK_CONTOUR`] of its value.
const CRACK_CRUMPLE: f32 = 1.3;
const CRACK_CONTOUR: f32 = 0.1;
const CRACK_JAG: f32 = 1.2;

/// The jag grows with the plates, as this share of one, over a grain this
/// share of one: without it a wide spacing's contours are the smooth loops
/// of a contour map rather than cracks.
const CRACK_JAG_PER_CELL: f32 = 0.07;
const CRACK_JAG_GRAIN: f32 = 0.15;

/// Where the courses and the crumple hand over, as tones of the picture
/// softened over a share of a plate: the courses fade in above the first
/// pair and the crumple fades down above the second.
const CRACK_LIGHT_FROM: f32 = 0.25;
const CRACK_LIGHT_FULL: f32 = 0.55;
const CRACK_DARK_FROM: f32 = 0.3;
const CRACK_DARK_FULL: f32 = 0.72;
const CRACK_TONE_SOFTEN: f32 = 0.3;

/// How wide a crack is, and how far a plate's edge rounds down into it, in
/// pixels. CS6's plates are flat, with a narrow bevelled rim: rounding them
/// much further turns the surface into bubble wrap. But a crack is a groove
/// with a floor and two walls, not a scratch — a hairline casts no shadow.
const CRACK_WIDTH: f32 = 1.6;
const CRACK_BEVEL: f32 = 1.1;

/// How wide the dark floor at the bottom of a crack is, in pixels. Narrower
/// than the groove: the rest of its width shows as walls, lit on one side
/// and in shadow on the other, and a floor as wide as the groove runs
/// neighbouring cracks together into blots.
const CRACK_FLOOR_WIDTH: f32 = 1.0;

/// How deep a crack runs, as a floor and a spread along its length over a
/// stretch this share of a plate. A crack of one depth all along is a line
/// ruled on the surface; CS6's open wide here and close to nothing there.
const CRACK_DEEP_FLOOR: f32 = 0.45;
const CRACK_DEEP_SPREAD: f32 = 1.1;
const CRACK_DEEP_STRETCH: f32 = 0.6;

/// Pockets: small pits where the paint has flaked away, this share of a
/// plate across (but at least [`CRACK_POCKET_MIN`] pixels), found where a
/// noise rises above [`CRACK_POCKET_FROM`], and this deep. CS6's sea and
/// horse are pocked all over between the cracks, its pale sky hardly at all,
/// so they come with the crumple.
const CRACK_POCKET: f32 = 0.22;
const CRACK_POCKET_MIN: f32 = 2.5;
const CRACK_POCKET_FROM: f32 = 0.66;
const CRACK_POCKET_SOFT: f32 = 0.08;
const CRACK_POCKET_DEPTH: f32 = 0.8;

/// Shadows: how far the light is traced back towards the top left, in
/// pixels, how steeply it falls per pixel in units of the surface's height,
/// and how dark a full shadow is. The lit rim of a crack or a pocket throws
/// its far wall into shade, which is what makes it read as a hole rather
/// than as a line drawn on.
const CRACK_SHADOW_REACH: usize = 4;
const CRACK_SHADOW_FALL: f32 = 0.22;
const CRACK_SHADOW: f32 = 0.55;

/// How much of the crack network never opens. CS6's cracks almost never
/// close into plates: they are runs that meet now and then in a T or an L
/// and stop short, and a network closed all round reads as jigsaw pieces or
/// brickwork. So every run of crack is gated by a noise along its length,
/// this much of which is shut, and the gate opens over [`CRACK_GAP_SOFT`] so
/// a crack tapers out rather than stopping square. Wider spacing shuts more
/// of it, up to [`CRACK_GAP_WIDE`] more by [`CRACK_GAP_FULL`].
const CRACK_GAP: f32 = 0.38;
const CRACK_GAP_SOFT: f32 = 0.12;
const CRACK_GAP_WIDE: f32 = 0.1;
const CRACK_GAP_FULL: f32 = 60.0;

/// How long a dash of crack runs before its gate can shut, as a share of a
/// plate.
const CRACK_DASH: f32 = 0.7;

/// The share of the cracks that break a course that open at all.
const CRACK_BREAKS_OPEN: f32 = 0.55;

/// How much less of the crumple's length the gate shuts than of the
/// courses': CS6's dark horse is crumpled all over, and gated as hard as the
/// sky it is bare.
const CRACK_CRUMPLE_GAP_EASE: f32 = 0.12;

/// How much the picture's own brightness raises the surface, so the relief
/// follows the photograph's contours as well as the cracks.
const CRACK_PICTURE_RELIEF: f32 = 0.9;

/// How steep the relief is lit at the top of Crack Depth, and how much of
/// that the lightest parts of the picture keep. CS6's pale sky is faintly
/// ruled with thin embossed lines, while its dark horse is crumpled, every
/// chip catching a bright rim.
const CRACK_SHADE: f32 = 1.6;
const CRACK_SHADE_IN_LIGHT: f32 = 0.28;

/// A crack's darkness at the bottom of Crack Brightness, as a share of the
/// picture it runs through, and how far each step lifts it. At CS6's default
/// of 9 a crack is barely darker than its plates, and shows by its relief.
const CRACK_FLOOR: f32 = 0.15;
const CRACK_LIFT_PER_STEP: f32 = 0.095;

/// Filter ▸ Texture ▸ Craquelure: the picture painted onto plaster that has
/// dried and cracked into plates.
///
/// The crack network follows the picture. Where it is light the plates are
/// laid in uneven courses, **Crack Spacing** wide, split by cracks at random
/// and laid again finer over the top — see [`CRACK_COURSE`] — which is what
/// makes CS6's cracks across a pale sky run mostly across and down, at every
/// length. Where it is dark the paint has crumpled instead, and the cracks
/// follow the contours of a fractal noise, meandering every which way — see
/// [`CRACK_CRUMPLE`]. The cracks wander, and much of both networks never
/// opens at all, so the plates run into one another and the cracks stop
/// short — see [`CRACK_GAP`]. The
/// surface is the plates, flat with a narrow bevelled rim, plus the
/// picture's own brightness, and it is lit from
/// the top left as deep as **Crack Depth** asks. **Crack Brightness** is how
/// light the bottom of a crack is: near black at 0, the picture's own colour
/// towards 10.
///
/// Alpha is left alone.
///
/// No GPU path: it would fit — a few noises and a slope per pixel — but it
/// runs in tens of milliseconds on the CPU and its result is wanted straight
/// back by the history, so the upload would cost more than it saved.
pub fn craquelure(pixmap: &mut Pixmap, spacing: u32, depth: u32, brightness: u32) {
    if pixmap.is_empty() {
        return;
    }
    let spacing = spacing.clamp(*CRACK_SPACING.start(), *CRACK_SPACING.end()) as f32;
    let depth = depth.clamp(*CRACK_DEPTH.start(), *CRACK_DEPTH.end()) as f32;
    let brightness =
        brightness.clamp(*CRACK_BRIGHTNESS.start(), *CRACK_BRIGHTNESS.end()) as f32;
    let (width, height) = (pixmap.width() as usize, pixmap.height() as usize);
    let cell = (spacing * CRACK_CELL_PER_STEP).max(2.0);
    let gap = CRACK_GAP + CRACK_GAP_WIDE * (spacing / CRACK_GAP_FULL).min(1.0);
    // How open a crack is, from its gate's noise: a crack that is closing
    // has its distance stretched, so it thins and tapers out.
    let open = |gate: f32| ((gate - gap) / CRACK_GAP_SOFT).clamp(0.0, 1.0);

    // The picture's tone, softened so the network follows its broad shapes
    // rather than its detail: that decides which network cracks where.
    let bytes = pixmap.as_bytes();
    let stride = pixmap.stride();
    let lum: Vec<f32> = (0..width * height)
        .into_par_iter()
        .map(|i| {
            let p = &bytes[(i / width) * stride + (i % width) * 4..];
            (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) / 255.0
        })
        .collect();
    let mut tone = lum.clone();
    crate::filters::artistic::blur_field(&mut tone, width, height, cell * CRACK_TONE_SOFTEN);
    let ramp = |v: f32, from: f32, full: f32| ((v - from) / (full - from)).clamp(0.0, 1.0);

    // The crumple's noise, drawn once so its slope can be read off it, over
    // coordinates jagged by a fine wobble.
    let crumple = cell * CRACK_CRUMPLE;
    let noise: Vec<f32> = (0..width * height)
        .into_par_iter()
        .map(|i| {
            let (x, y) = ((i % width) as f32, (i / width) as f32);
            let grain = cell * CRACK_JAG_GRAIN;
            let jag = cell * CRACK_JAG_PER_CELL;
            let u = x
                + (value_noise(x, y, 3.0, 71) - 0.5) * 2.0 * CRACK_JAG
                + (value_noise(x, y, grain, 76) - 0.5) * 2.0 * jag;
            let v = y
                + (value_noise(x, y, 3.0, 72) - 0.5) * 2.0 * CRACK_JAG
                + (value_noise(x, y, grain, 77) - 0.5) * 2.0 * jag;
            0.55 * value_noise(u, v, crumple, 73)
                + 0.3 * value_noise(u, v, crumple * 0.45, 74)
                + 0.15 * value_noise(u, v, crumple * 0.2, 75)
        })
        .collect();

    // How far a point is from the nearest open crack between or across the
    // courses, for courses `size` wide, `seed` apart from any other laying,
    // shut `extra` more than the gap.
    let courses_at = |xf: f32, yf: f32, size: f32, seed: u32, extra: f32| -> f32 {
        let s = seed * 16;
        let wobble = CRACK_WOBBLE.min(size * CRACK_WOBBLE_MAX);
        let u = xf
            + (value_noise(xf, yf, size, 61 + s) - 0.5) * 2.0 * CRACK_WANDER * size
            + (value_noise(xf, yf, size * 0.5, 68 + s) - 0.5) * 2.0 * CRACK_KINK * size
            + (value_noise(xf, yf, 4.0, 62 + s) - 0.5) * 2.0 * wobble;
        let v = yf
            + (value_noise(xf, yf, size, 63 + s) - 0.5) * 2.0 * CRACK_WANDER * size
            + (value_noise(xf, yf, size * 0.5, 69 + s) - 0.5) * 2.0 * CRACK_KINK * size
            + (value_noise(xf, yf, 4.0, 64 + s) - 0.5) * 2.0 * wobble;
        let seed = seed as i32 * 7919;
        let course = size * CRACK_COURSE;
        // The stretch of courses this point is in, and the uneven heights
        // of the cracks between them there.
        let stretch = (u / (size * CRACK_COURSE_STEP)).floor() as i32 + seed;
        let boundary = |k: i32| {
            (k as f32 + (lattice(k, stretch, 85) - 0.5) * 2.0 * CRACK_COURSE_JITTER) * course
        };
        let mut k = (v / course).floor() as i32;
        if v < boundary(k) {
            k -= 1;
        } else if v >= boundary(k + 1) {
            k += 1;
        }
        let dash = size * CRACK_DASH;
        let mut across = f32::MAX;
        for (b, d) in [(k, v - boundary(k)), (k + 1, boundary(k + 1) - v)] {
            let gate = value_noise(u + b as f32 * 97.0, seed as f32, dash, 81);
            let o = open(gate - extra);
            if o > 0.0 {
                across = across.min(d.abs() / o);
            }
        }
        // The cracks that break the course, only some of which open.
        let slab = size * CRACK_BREAK;
        let column = (u / slab).floor() as i32;
        let mut down = f32::MAX;
        for j in column - 1..=column + 1 {
            if lattice(j, k + seed, 82) > CRACK_BREAKS_OPEN - extra {
                continue;
            }
            let at = (j as f32 + 0.15 + 0.7 * lattice(j, k + seed, 65)) * slab;
            down = down.min((u - at).abs());
        }
        across.min(down)
    };

    // How far each pixel is from the nearest crack that is open there. A
    // network that is fading out has its distances stretched, so its cracks
    // thin and go before they vanish rather than switching off.
    let mut crack = vec![0.0f32; width * height];
    let (tone_ref, noise) = (&tone, &noise);
    crack.par_chunks_exact_mut(width).enumerate().for_each(|(y, row)| {
        let (up, down) = (y.saturating_sub(1), (y + 1).min(height - 1));
        for (x, slot) in row.iter_mut().enumerate() {
            let (xf, yf) = (x as f32, y as f32);
            let i = y * width + x;
            let t = tone_ref[i];
            let light = ramp(t, CRACK_LIGHT_FROM, CRACK_LIGHT_FULL);
            let dark = 1.0 - ramp(t, CRACK_DARK_FROM, CRACK_DARK_FULL);

            let mut courses = f32::MAX;
            if light > 0.0 {
                courses = courses_at(xf, yf, cell, 0, 0.0)
                    .min(courses_at(xf, yf, cell * CRACK_FINE, 1, CRACK_FINE_GAP) * CRACK_FINE_FAINT)
                    / light;
            }

            // The crumple: distance to the nearest contour of the noise, as
            // its value's distance over its slope. None of it in the light:
            // faded rather than gone, its contours break up into specks.
            if dark <= 0.0 {
                *slot = courses;
                continue;
            }
            let (left, right) = (x.saturating_sub(1), (x + 1).min(width - 1));
            let gx = (noise[y * width + right] - noise[y * width + left]) / (right - left).max(1) as f32;
            let gy = (noise[down * width + x] - noise[up * width + x]) / (down - up).max(1) as f32;
            let slope = (gx * gx + gy * gy).sqrt().max(1e-4);
            let level = noise[i] / CRACK_CONTOUR;
            let contour = (level - level.round()).abs() * CRACK_CONTOUR / slope;
            let along = open(value_noise(xf, yf, cell * CRACK_DASH, 83) + CRACK_CRUMPLE_GAP_EASE);
            let crumpled = if along > 0.0 { contour / along / dark } else { f32::MAX };

            *slot = courses.min(crumpled);
        }
    });

    // The surface: plates rounded down into their cracks, on top of the
    // picture's own brightness.
    let smooth = |t: f32| {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let mut surface: Vec<f32> = (0..width * height)
        .into_par_iter()
        .map(|i| {
            let (x, y) = ((i % width) as f32, (i / width) as f32);
            // The groove, deeper in some stretches than others.
            let deep = CRACK_DEEP_FLOOR
                + CRACK_DEEP_SPREAD * value_noise(x, y, cell * CRACK_DEEP_STRETCH, 91);
            let groove = deep * (1.0 - smooth((crack[i] - CRACK_WIDTH * 0.5) / CRACK_BEVEL));
            // The pockets, where the paint has crumpled.
            let dark = 1.0 - ramp(tone[i], CRACK_DARK_FROM, CRACK_DARK_FULL);
            let pocket_size = (cell * CRACK_POCKET).max(CRACK_POCKET_MIN);
            let n = 0.65 * value_noise(x, y, pocket_size, 92)
                + 0.35 * value_noise(x, y, pocket_size * 0.5, 93);
            let pocket = smooth((n - CRACK_POCKET_FROM) / CRACK_POCKET_SOFT) * CRACK_POCKET_DEPTH * dark;
            1.0 - groove.max(pocket) + lum[i] * CRACK_PICTURE_RELIEF
        })
        .collect();
    crate::filters::artistic::blur_field(&mut surface, width, height, 0.5);

    let (lx, ly) = Light::TopLeft.towards();
    let shade_gain = depth / *CRACK_DEPTH.end() as f32 * CRACK_SHADE;
    let floor = CRACK_FLOOR + brightness * CRACK_LIFT_PER_STEP;
    let (surface, crack, lum) = (&surface, &crack, &lum);
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, out)| {
            let (up, down) = (y.saturating_sub(1), (y + 1).min(height - 1));
            for (x, px) in out.chunks_exact_mut(4).enumerate() {
                let i = y * width + x;
                let (left, right) = (x.saturating_sub(1), (x + 1).min(width - 1));
                let dx = (surface[y * width + right] - surface[y * width + left])
                    / (right - left).max(1) as f32;
                let dy = (surface[down * width + x] - surface[up * width + x])
                    / (down - up).max(1) as f32;
                // A slope that falls away from the light is lit.
                let lightness = lum[i];
                let shade = -(dx * lx + dy * ly)
                    * shade_gain
                    * (1.0 - (1.0 - CRACK_SHADE_IN_LIGHT) * lightness);
                // The shadow thrown by whatever stands between this pixel and
                // the light: the lit rim of a crack or a pocket.
                let mut shadow = 0.0f32;
                for k in 1..=CRACK_SHADOW_REACH {
                    let sx = x as f32 + lx * k as f32;
                    let sy = y as f32 + ly * k as f32;
                    if sx < 0.0 || sy < 0.0 {
                        break;
                    }
                    let (sx, sy) = (sx.round() as usize, sy.round() as usize);
                    if sx >= width || sy >= height {
                        break;
                    }
                    let above = surface[sy * width + sx] - surface[i] - CRACK_SHADOW_FALL * k as f32;
                    shadow = shadow.max(above);
                }
                let shadow = 1.0 - CRACK_SHADOW * (shadow * shade_gain / CRACK_SHADE).clamp(0.0, 1.0);
                // The bottom of the crack, where the light does not reach.
                let open = 1.0 - smooth(crack[i] / CRACK_FLOOR_WIDTH);
                let tone = (1.0 + (floor - 1.0) * open) * shadow;
                for c in 0..3 {
                    let v = px[c] as f32 * (1.0 + shade * SHADE_BY_COLOUR) + shade * SHADE_BY_LIGHT;
                    px[c] = (v * tone).clamp(0.0, 255.0).round() as u8;
                }
                // Alpha stands: cracking the paint does not change the
                // layer's shape.
            }
        });
}

/// CS6's ranges for Grain, which its two sliders run over.
pub const GRAIN_INTENSITY: std::ops::RangeInclusive<u32> = 0..=100;
pub const GRAIN_CONTRAST: std::ops::RangeInclusive<u32> = 0..=100;

/// CS6's ten kinds of grain, in the order its Grain Type list gives them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum GrainType {
    /// Colour noise, a pixel at a time.
    #[default]
    Regular,
    /// The same, gentler and a little softened.
    Soft,
    /// Specks of the background colour thrown over the picture.
    Sprinkles,
    /// Colour noise gathered into clumps a few pixels across.
    Clumped,
    /// Clumped noise over a picture pushed hard towards black and white.
    Contrasty,
    /// Colour noise in bigger, softer clumps.
    Enlarged,
    /// The picture dithered into the two swatches.
    Stippled,
    /// Dark streaks running across.
    Horizontal,
    /// Dark streaks running down.
    Vertical,
    /// Specks of the foreground colour, thickest in the dark.
    Speckle,
}

impl GrainType {
    pub fn from_i32(value: i32) -> GrainType {
        match value {
            1 => GrainType::Soft,
            2 => GrainType::Sprinkles,
            3 => GrainType::Clumped,
            4 => GrainType::Contrasty,
            5 => GrainType::Enlarged,
            6 => GrainType::Stippled,
            7 => GrainType::Horizontal,
            8 => GrainType::Vertical,
            9 => GrainType::Speckle,
            _ => GrainType::Regular,
        }
    }
}

/// How strong the grain is, as a spread in levels per step of Intensity.
/// Read off CS6 on `samples/horse-3.jpg`: at 40 Regular is a plain but
/// moderate noise over the sky, at 71 the sky is more noise than blue.
const GRAIN_SPREAD_PER_STEP: f32 = 0.8;

/// Contrast is a gain about mid-grey, doubling every this many steps from a
/// gain of one at [`GRAIN_CONTRAST_FLAT`], and it stretches the grain along
/// with the picture: CS6 at 78 turns the horse nearly black and the noise
/// over the sky garish.
const GRAIN_CONTRAST_DOUBLING: f32 = 40.0;
const GRAIN_CONTRAST_FLAT: f32 = 50.0;

/// Each kind's grain: how far the noise is blurred into clumps, in pixels,
/// how strong it is against Regular's, how much harder than the slider it
/// pushes the contrast, and a contrast of its own on top that holds at any
/// slider setting. CS6's Contrasty at the slider's flat middle already has
/// the horse black and the sky bleached, so its contrast cannot all come
/// from the slider. Clumped is harsh too; Enlarged softer; Soft barely there.
struct GrainKind {
    clump: f32,
    strength: f32,
    contrast: f32,
    boost: f32,
}
const GRAIN_REGULAR: GrainKind = GrainKind { clump: 0.0, strength: 1.0, contrast: 1.0, boost: 1.0 };
const GRAIN_SOFT: GrainKind = GrainKind { clump: 0.7, strength: 0.6, contrast: 1.0, boost: 1.0 };
const GRAIN_CLUMPED: GrainKind = GrainKind { clump: 2.0, strength: 0.5, contrast: 1.6, boost: 1.4 };
const GRAIN_CONTRASTY: GrainKind =
    GrainKind { clump: 1.6, strength: 0.4, contrast: 2.0, boost: 1.65 };
const GRAIN_ENLARGED: GrainKind = GrainKind { clump: 2.4, strength: 0.55, contrast: 1.3, boost: 1.15 };

/// Sprinkles: the share of pixels thrown to the background colour at the top
/// of Intensity. CS6 at 51 has salted the dark horse grey with them.
const GRAIN_SPRINKLE: f32 = 0.7;

/// Speckle inks a coarse screen. CS6 at 200% is a square grid about four
/// pixels to a cell: the grid's lines take the foreground wherever the
/// picture is mid-dark, so the brown horse comes out tiled; the darkest
/// parts fill solid; and whatever is darker than its surroundings — an
/// outline, the dark side of a detail — inks in too. The pale sky stays
/// clean. So a pixel is inked when its tone, less how much darker than its
/// neighbourhood it is, falls below a cut that is higher on the grid's lines
/// and shaken by a little noise, all in levels at the top of Intensity.
const GRAIN_MESH: usize = 4;
const GRAIN_SPECKLE_CUT: f32 = 95.0;
const GRAIN_SPECKLE_ON_MESH: f32 = 125.0;
const GRAIN_SPECKLE_SHAKE: f32 = 55.0;
const GRAIN_SPECKLE_EDGE: f32 = 2.5;
const GRAIN_SPECKLE_NEIGHBOURHOOD: f32 = 2.0;

/// How far Speckle lifts and saturates the picture under its specks at the
/// top of Intensity. CS6's brown horse comes out lighter and redder, its sky
/// and sand paler and brighter.
const GRAIN_SPECKLE_LIFT: f32 = 0.7;
const GRAIN_SPECKLE_SATURATE: f32 = 0.5;

/// Stippled: how hard the noise shakes each pixel before it is cut into one
/// swatch or the other, against Regular's. Enough that the mid-tones come
/// out as an even scatter of both.
const GRAIN_STIPPLE: f32 = 2.2;

/// Horizontal and Vertical are printer lines, as on a worn photocopy: each
/// row (or column) gets its own darkness, carried the whole way along it, so
/// the picture is banded rather than scratched. The bands are one to three
/// pixels thick — the row noise is smoothed over [`GRAIN_BAND`] — and fade
/// in and out over [`GRAIN_BAND_FADE`] pixels along their length, a few rows
/// at a time, keeping at least [`GRAIN_BAND_FLOOR`] of their strength. A fine
/// grit rides on top. A band darkens by [`GRAIN_STREAK_STRENGTH`] against
/// Regular's grain and lightens by [`GRAIN_STREAK_LIGHT`] of that.
const GRAIN_BAND: f32 = 1.8;
const GRAIN_BAND_FADE: f32 = 240.0;
const GRAIN_BAND_FLOOR: f32 = 0.3;
const GRAIN_BAND_GRIT: f32 = 0.5;
const GRAIN_STREAK_STRENGTH: f32 = 2.2;
const GRAIN_STREAK_LIGHT: f32 = 0.35;

/// The streaks' contrast of its own, like [`GrainKind::boost`]: CS6's horse
/// under them is near black and its sky bleached, so the lines vanish in the
/// sky and show hardest over the mid-toned sea.
const GRAIN_STREAK_BOOST: f32 = 1.25;

/// How far Sprinkles lifts the picture towards white at the top of
/// Intensity. CS6 pales the whole picture under it, so the specks show.
const GRAIN_SPECK_LIFT: f32 = 0.6;

/// How much of each channel's grain is shared with the other two. CS6's
/// colour grain is pastel — pink, mint and lilac specks, not pure red, green
/// and blue ones — which is what three channels partly moving together give.
const GRAIN_SHARED: f32 = 0.55;

/// White noise from -1 to 1, laid by where on the canvas a pixel is; `salt`
/// keeps two layers from being the same noise.
fn grain_noise(x: usize, y: usize, salt: i32) -> f32 {
    crate::filters::artistic::noise(x as i32 + salt * 7919, y as i32 - salt * 104_729) * 2.0 - 1.0
}

/// Filter ▸ Texture ▸ Grain: the picture as if printed through grain of one
/// of ten kinds.
///
/// **Intensity** is how strong the grain is and **Contrast** stretches the
/// result about mid-grey, grain and all. **Grain Type** decides what the
/// grain is: colour noise pixel by pixel (Regular, Soft) or gathered into
/// clumps (Clumped, Contrasty, Enlarged), streaks (Horizontal, Vertical), or
/// specks and dither in the document's swatches — Sprinkles throws specks of
/// the background colour, Speckle specks of the foreground thickest in the
/// dark, and Stippled cuts the whole picture into the two.
///
/// Alpha is left alone.
///
/// No GPU path: a noise and a curve per pixel, a few milliseconds on the
/// CPU, and the clumps and streaks are one-off blurs of the noise. The upload
/// would cost more than the work.
pub fn grain(
    pixmap: &mut Pixmap,
    intensity: u32,
    contrast: u32,
    kind: GrainType,
    foreground: crate::buffer::Rgba8,
    background: crate::buffer::Rgba8,
) {
    if pixmap.is_empty() {
        return;
    }
    let intensity = intensity.clamp(*GRAIN_INTENSITY.start(), *GRAIN_INTENSITY.end()) as f32;
    let contrast = contrast.clamp(*GRAIN_CONTRAST.start(), *GRAIN_CONTRAST.end()) as f32;
    let (width, height) = (pixmap.width() as usize, pixmap.height() as usize);
    let spread = intensity * GRAIN_SPREAD_PER_STEP;
    let share = intensity / *GRAIN_INTENSITY.end() as f32;

    let colour_kind = match kind {
        GrainType::Regular => Some(&GRAIN_REGULAR),
        GrainType::Soft => Some(&GRAIN_SOFT),
        GrainType::Clumped => Some(&GRAIN_CLUMPED),
        GrainType::Contrasty => Some(&GRAIN_CONTRASTY),
        GrainType::Enlarged => Some(&GRAIN_ENLARGED),
        _ => None,
    };
    let push = colour_kind.map_or(1.0, |k| k.contrast);
    let boost = match kind {
        GrainType::Horizontal | GrainType::Vertical => GRAIN_STREAK_BOOST,
        _ => colour_kind.map_or(1.0, |k| k.boost),
    };
    let gain = boost * 2f32.powf((contrast - GRAIN_CONTRAST_FLAT) / GRAIN_CONTRAST_DOUBLING * push);
    let stretch = |v: f32| ((v - 128.0) * gain + 128.0).clamp(0.0, 255.0);
    let lift = |v: f32| v + (255.0 - v) * share * GRAIN_SPECK_LIFT;

    // The colour kinds' noise, one field per channel, clumped and scaled
    // back to a spread of one so the clumping does not also fade it.
    let fields: Vec<Vec<f32>> = match colour_kind {
        Some(k) => (0..3)
            .map(|c| {
                let mut field: Vec<f32> = (0..width * height)
                    .into_par_iter()
                    .map(|i| {
                        let (x, y) = (i % width, i / width);
                        GRAIN_SHARED * grain_noise(x, y, 10)
                            + (1.0 - GRAIN_SHARED) * grain_noise(x, y, 11 + c)
                    })
                    .collect();
                if k.clump > 0.0 {
                    crate::filters::artistic::blur_field(&mut field, width, height, k.clump);
                }
                crate::filters::brush_strokes::unit_spread(&mut field);
                field
            })
            .collect(),
        None => Vec::new(),
    };
    let streaks: Vec<f32> = match kind {
        GrainType::Horizontal | GrainType::Vertical => (0..width * height)
            .into_par_iter()
            .map(|i| {
                let (x, y) = (i % width, i / width);
                // Along the band, and which band.
                let (along, band) = if kind == GrainType::Horizontal { (x, y) } else { (y, x) };
                let row = value_noise(0.0, band as f32, GRAIN_BAND, 61) * 2.0 - 1.0;
                let fade = value_noise(along as f32, band as f32 * 37.0, GRAIN_BAND_FADE, 62);
                let fade = GRAIN_BAND_FLOOR + (1.0 - GRAIN_BAND_FLOOR) * fade;
                row * fade + grain_noise(x, y, 63) * GRAIN_BAND_GRIT
            })
            .collect(),
        _ => Vec::new(),
    };
    // Speckle's neighbourhood, so what is darker than its surroundings can
    // be inked.
    let around: Vec<f32> = if kind == GrainType::Speckle {
        let bytes = pixmap.as_bytes();
        let stride = pixmap.stride();
        let mut field: Vec<f32> = (0..width * height)
            .into_par_iter()
            .map(|i| {
                let p = &bytes[(i / width) * stride + (i % width) * 4..];
                0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32
            })
            .collect();
        crate::filters::artistic::blur_field(&mut field, width, height, GRAIN_SPECKLE_NEIGHBOURHOOD);
        field
    } else {
        Vec::new()
    };
    let around = &around;
    let ink = [foreground.r as f32, foreground.g as f32, foreground.b as f32];
    let paper = [background.r as f32, background.g as f32, background.b as f32];
    let (fields, streaks) = (&fields, &streaks);

    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let i = y * width + x;
                let lum = 0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32;
                let out: [f32; 3] = match kind {
                    GrainType::Regular
                    | GrainType::Soft
                    | GrainType::Clumped
                    | GrainType::Contrasty
                    | GrainType::Enlarged => {
                        let strength = colour_kind.map_or(1.0, |k| k.strength) * spread;
                        std::array::from_fn(|c| stretch(px[c] as f32 + fields[c][i] * strength))
                    }
                    GrainType::Sprinkles => {
                        let hit = (grain_noise(x, y, 31) + 1.0) * 0.5 < share * GRAIN_SPRINKLE;
                        if hit {
                            paper
                        } else {
                            std::array::from_fn(|c| stretch(lift(px[c] as f32)))
                        }
                    }
                    GrainType::Speckle => {
                        let on_mesh = x % GRAIN_MESH == 0 || y % GRAIN_MESH == 0;
                        let cut = share
                            * (GRAIN_SPECKLE_CUT
                                + if on_mesh { GRAIN_SPECKLE_ON_MESH } else { 0.0 }
                                + grain_noise(x, y, 41) * GRAIN_SPECKLE_SHAKE);
                        let darker = (around[i] - lum).max(0.0) * GRAIN_SPECKLE_EDGE;
                        if lum - darker < cut {
                            ink
                        } else {
                            // Lighter and richer under the specks.
                            let lifted = lum + (255.0 - lum) * share * GRAIN_SPECKLE_LIFT;
                            let rich = 1.0 + share * GRAIN_SPECKLE_SATURATE;
                            let scale = lifted / lum.max(1.0);
                            std::array::from_fn(|c| {
                                let v = lifted + (px[c] as f32 - lum) * scale.min(3.0) * rich;
                                stretch(v)
                            })
                        }
                    }
                    GrainType::Stippled => {
                        let shaken = stretch(lum) + grain_noise(x, y, 51) * spread * GRAIN_STIPPLE;
                        if shaken < 128.0 {
                            ink
                        } else {
                            paper
                        }
                    }
                    GrainType::Horizontal | GrainType::Vertical => {
                        let s = streaks[i];
                        let s = if s < 0.0 { s } else { s * GRAIN_STREAK_LIGHT };
                        let shift = s * spread * GRAIN_STREAK_STRENGTH;
                        std::array::from_fn(|c| stretch(px[c] as f32 + shift))
                    }
                };
                for c in 0..3 {
                    px[c] = out[c].round().clamp(0.0, 255.0) as u8;
                }
                // Alpha stands: grain does not change the layer's shape.
            }
        });
}

/// CS6's ranges for Mosaic Tiles, which its three sliders run over.
pub const TILE_SIZE: std::ops::RangeInclusive<u32> = 2..=100;
pub const TILE_GROUT: std::ops::RangeInclusive<u32> = 1..=15;
pub const TILE_LIGHTEN: std::ops::RangeInclusive<u32> = 0..=10;

/// How far a tile's edge wanders off the grid, in pixels and as a share of
/// the tile, and how coarse the wander is, as a share of the tile but no
/// finer than [`TILE_JAG_GRAIN_MIN`]. CS6's grout lines follow a square grid
/// but are ragged and chunky, like hand-cut tiles: at Tile Size 47 an edge
/// steps in and out by a few pixels every eight or so, at 12 the tiles are
/// lumpy enough to read as pebbles.
const TILE_JAG: f32 = 2.4;
const TILE_JAG_PER_SIZE: f32 = 0.07;
const TILE_JAG_GRAIN: f32 = 0.2;
const TILE_JAG_GRAIN_MIN: f32 = 6.0;

/// The share of the grid's edges that never got grout, so the tiles either
/// side are one. CS6's grid is not complete: here and there two tiles run
/// together into a longer, odd-shaped one.
const TILE_MISSING: f32 = 0.2;

/// How far a tile's edge is rounded over into the grout, in pixels, but no
/// more than this share of the tile and no less than a pixel. CS6's tile
/// faces are flat right up to a narrow rim, whatever their size: rounded
/// further, small tiles are all bevel and read as bubble wrap.
const TILE_BEVEL: f32 = 2.0;
const TILE_BEVEL_PER_SIZE: f32 = 0.06;
const TILE_BEVEL_MIN: f32 = 0.7;

/// How high a tile stands over its grout, in the same units as the picture's
/// relief. A step of its own rather than a share of the picture's brightness:
/// scaled by that, every bright tile stood tall and shaded like a dome.
const TILE_HEIGHT: f32 = 0.55;

/// How much the picture's own brightness raises the surface, so the picture
/// reads as pressed into the tiles, as CS6's embossed horse does.
const TILE_PICTURE_RELIEF: f32 = 1.6;

/// How hard the tiles are lit, and how dark the shadow a tile's edge throws
/// over the grout below and to the right of it is.
const TILE_SHADE: f32 = 1.3;
const TILE_SHADOW: f32 = 0.45;
const TILE_SHADOW_REACH: usize = 3;

/// How wide the grout is, in pixels each side of the grid line, per step of
/// Grout Width and less this, but never under [`TILE_GROUT_MIN`]. CS6's
/// grout at 3 is a line under two pixels wide, rims included, and at 8 a
/// band about seven wide: it widens faster than the slider from a start
/// near nothing.
const TILE_GROUT_SHARE: f32 = 0.55;
const TILE_GROUT_OFFSET: f32 = 0.75;
const TILE_GROUT_MIN: f32 = 0.5;

/// The grout: a grey this light at Lighten Grout 0 and this much lighter per
/// step, laid over the picture this thickly at 0 and this much thicker per
/// step. CS6's grout lets some of the picture through — dark over the
/// horse, pale over the sky — so the picture reads as one thing cut into
/// tiles rather than as squares on a grey ground.
const TILE_GROUT_GREY: f32 = 90.0;
const TILE_GROUT_PER_STEP: f32 = 14.0;
const TILE_GROUT_COVER: f32 = 0.25;
const TILE_GROUT_COVER_PER_STEP: f32 = 0.05;

/// Filter ▸ Texture ▸ Mosaic Tiles: the picture laid in small tiles with
/// grout between them.
///
/// The tiles are a square grid **Tile Size** apart whose lines are jagged by
/// a coarse noise — see [`TILE_JAG`] — and a few of which are missing, so
/// the odd pair of tiles runs together — see [`TILE_MISSING`]. The grout between them is **Grout
/// Width** wide. The grout is grey, as light as **Lighten Grout** asks, with
/// a little of the picture showing through. The tiles are raised, rounded at
/// the edge and lit from the top left, with the picture's own brightness
/// pressed into their faces, and their edges throw shadow into the grout.
///
/// Alpha is left alone.
///
/// No GPU path, for Craquelure's reasons: a few noises and a slope per
/// pixel, tens of milliseconds on the CPU, and the result is wanted straight
/// back.
pub fn mosaic_tiles(pixmap: &mut Pixmap, size: u32, grout: u32, lighten: u32) {
    if pixmap.is_empty() {
        return;
    }
    let size = size.clamp(*TILE_SIZE.start(), *TILE_SIZE.end()) as f32;
    let grout = grout.clamp(*TILE_GROUT.start(), *TILE_GROUT.end()) as f32;
    let lighten = lighten.clamp(*TILE_LIGHTEN.start(), *TILE_LIGHTEN.end()) as f32;
    let (width, height) = (pixmap.width() as usize, pixmap.height() as usize);
    let jag = TILE_JAG + size * TILE_JAG_PER_SIZE;
    let grain = (size * TILE_JAG_GRAIN).max(TILE_JAG_GRAIN_MIN);
    // The grout cannot eat the whole tile.
    let half_grout = (grout * TILE_GROUT_SHARE - TILE_GROUT_OFFSET)
        .max(TILE_GROUT_MIN)
        .min(size * 0.3);
    let bevel = TILE_BEVEL.min(size * TILE_BEVEL_PER_SIZE).max(TILE_BEVEL_MIN);

    let bytes = pixmap.as_bytes();
    let stride = pixmap.stride();
    let lum: Vec<f32> = (0..width * height)
        .into_par_iter()
        .map(|i| {
            let p = &bytes[(i / width) * stride + (i % width) * 4..];
            (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) / 255.0
        })
        .collect();

    // How far each pixel is inside its tile: negative in the grout.
    let inside: Vec<f32> = (0..width * height)
        .into_par_iter()
        .map(|i| {
            let (x, y) = ((i % width) as f32, (i / width) as f32);
            let u = x + (value_noise(x, y, grain, 101) - 0.5) * 2.0 * jag;
            let v = y + (value_noise(x, y, grain, 102) - 0.5) * 2.0 * jag;
            // The nearest line of the grid each way, and whether the stretch
            // of it here got any grout.
            let (col, row) = ((u / size).round(), (v / size).round());
            let (cell_x, cell_y) = ((u / size).floor() as i32, (v / size).floor() as i32);
            let mut across = (u - col * size).abs();
            if lattice(col as i32, cell_y, 103) < TILE_MISSING {
                across = f32::MAX;
            }
            let mut down = (v - row * size).abs();
            if lattice(cell_x, row as i32, 104) < TILE_MISSING {
                down = f32::MAX;
            }
            across.min(down) - half_grout
        })
        .collect();

    let smooth = |t: f32| {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let mut surface: Vec<f32> = (0..width * height)
        .into_par_iter()
        .map(|i| smooth(inside[i] / bevel) * TILE_HEIGHT + lum[i] * TILE_PICTURE_RELIEF)
        .collect();
    crate::filters::artistic::blur_field(&mut surface, width, height, 0.4);

    let (lx, ly) = Light::TopLeft.towards();
    let grey = TILE_GROUT_GREY + lighten * TILE_GROUT_PER_STEP;
    let cover = TILE_GROUT_COVER + lighten * TILE_GROUT_COVER_PER_STEP;
    let (surface, inside) = (&surface, &inside);
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, out)| {
            let (up, down) = (y.saturating_sub(1), (y + 1).min(height - 1));
            for (x, px) in out.chunks_exact_mut(4).enumerate() {
                let i = y * width + x;
                let (left, right) = (x.saturating_sub(1), (x + 1).min(width - 1));
                let dx = (surface[y * width + right] - surface[y * width + left])
                    / (right - left).max(1) as f32;
                let dy = (surface[down * width + x] - surface[up * width + x])
                    / (down - up).max(1) as f32;
                // A slope that falls away from the light is lit.
                let shade = -(dx * lx + dy * ly) * TILE_SHADE;
                // The shadow a tile's edge throws over lower ground.
                let mut shadow = 0.0f32;
                for k in 1..=TILE_SHADOW_REACH {
                    let sx = x as f32 + lx * k as f32;
                    let sy = y as f32 + ly * k as f32;
                    if sx < 0.0 || sy < 0.0 {
                        break;
                    }
                    let (sx, sy) = (sx.round() as usize, sy.round() as usize);
                    if sx >= width || sy >= height {
                        break;
                    }
                    shadow = shadow.max(surface[sy * width + sx] - surface[i]);
                }
                let shadow = 1.0 - TILE_SHADOW * shadow.clamp(0.0, 1.0);
                // Tile or grout, blended across the edge's first pixel.
                let tile = smooth(inside[i] + 0.5);
                for c in 0..3 {
                    let own = px[c] as f32;
                    let grout = own + (grey - own) * cover;
                    let base = grout + (own - grout) * tile;
                    let v = base * (1.0 + shade * SHADE_BY_COLOUR) + shade * SHADE_BY_LIGHT;
                    px[c] = (v * shadow).clamp(0.0, 255.0).round() as u8;
                }
                // Alpha stands: tiling the picture does not change the
                // layer's shape.
            }
        });
}

/// CS6's ranges for Patchwork, which its two sliders run over.
pub const PATCH_SQUARE: std::ops::RangeInclusive<u32> = 0..=10;
pub const PATCH_RELIEF: std::ops::RangeInclusive<u32> = 0..=25;

/// How wide a square is, in pixels: Square Size, plus this. Measured off CS6
/// by fitting `samples/horse-3.jpg` to its preview: at Square Size 4 the
/// squares repeat every 8.9 pixels of the image, at 8 every 13.1.
const PATCH_SQUARE_FROM: u32 = 5;

/// The squares' faces are lifted by this gamma. Measured off CS6 block by
/// block over the whole of `samples/horse-3.jpg`: a source tone of 12 comes
/// out at 37, 62 at 104, 137 at 190 and 237 at 233.
const PATCH_LIFT: f32 = 0.65;

/// How much a square's brightness is pushed at random, as a share of full
/// white. Adobe's account of the filter says it varies the tiles' depth at
/// random "to replicate the highlights and shadows", but only a trace
/// shows: any more and the dark of the horse comes out as a checkerboard,
/// where CS6's runs in even rows.
const PATCH_VARY: f32 = 0.02;

/// The Relief the constants here are set for. Relief scales the heights by
/// its square root: CS6's shadows are near black by 16, but the rest of the
/// picture is hardly darker than at 8.
const PATCH_RELIEF_FULL: f32 = 8.0;

/// The squares are real blocks, and the picture is a render of them. Each
/// stands this many pixels high per unit of brightness over a black one —
/// so the steps between a pale square and a dark one throw shadows — plus
/// [`PATCH_RIM`] pixels that every square has, rounded over at its edges
/// into the joint with the next. The rounding runs this share of a square
/// in from each edge, no less than [`PATCH_ROUND_MIN`] pixels, and is a
/// rounded rectangle, so it rounds the corners too: that is what gives
/// CS6's shadow under each square its crescent shape, thin under the middle
/// and thick at the ends.
const PATCH_STEP: f32 = 10.0;
const PATCH_RIM: f32 = 1.2;
const PATCH_ROUND: f32 = 0.32;
const PATCH_ROUND_MIN: f32 = 1.5;

/// How much tighter the rounding is on a square's left and right sides than
/// on its top and bottom, and how much shallower. CS6's joints down a row
/// are faint lines, at about 88% of the face across its sky, beside the ones
/// between rows at under 60%. The two roundings multiply, so the corners
/// sink furthest — the ends of CS6's crescents.
const PATCH_ROUND_SIDES: f32 = 0.35;
const PATCH_SIDE_DEPTH: f32 = 0.65;

/// How much higher a square's lower edge stands than its upper one, in
/// pixels: the rows lie like shingles, each square's foot standing over the
/// head of the one below and throwing it into shadow. CS6's rows read as
/// stacked one on another, and averaged down a square its shading runs from
/// a shadow at its head, 69%, up to 103% a fifth of the way down, and down
/// again to its foot; a block with a level face lit from above has a bright
/// bevel along its head instead, and reads as a chocolate bar.
const PATCH_TILT: f32 = 2.0;

/// Where the light comes from: its direction across the picture, pointing
/// towards the light, and its height above the horizon in degrees. CS6's is
/// above and a little to the left, and low enough that a step of a couple
/// of pixels throws a shadow several pixels long.
const PATCH_LIGHT: (f32, f32) = (-0.2, -0.98);
const PATCH_ELEVATION: f32 = 28.0;

/// How much of a face's light is ambient, which neither slope nor shadow
/// takes away: the floor that shadows fall to.
const PATCH_AMBIENT: f32 = 0.38;

/// Shadows are traced from each pixel back towards the light in steps of
/// this many pixels, as far as this, and their edge is softened over this
/// many pixels of height, so a shadow deepens into the joint rather than
/// being cut out.
const PATCH_SHADOW_STEP: f32 = 0.5;
const PATCH_SHADOW_REACH: f32 = 14.0;
const PATCH_SHADOW_SOFT: f32 = 1.1;

/// Filter ▸ Texture ▸ Patchwork: the picture as squares of cloth or tile,
/// each one the average colour under it, raised by its brightness.
///
/// The picture is cut into squares **Square Size** and five pixels across,
/// and each is filled with the average of the picture under it. Then the
/// squares are built as blocks — each as high as it is bright, its edges
/// and corners rounded down into the joints — and the picture is a render
/// of those blocks lit from above and a little to the left: slopes facing
/// the light are brighter, slopes facing away darker, and every block
/// throws a shadow onto whatever stands lower beside it. **Relief** is how
/// high it all stands.
///
/// The height is worked out at any point, not per pixel, so slopes and
/// shadows are measured between pixels and their edges come out smooth
/// rather than staircased.
///
/// Alpha is left alone.
///
/// No GPU path: it would fit — a height and a short ray per pixel — but it
/// runs in tens of milliseconds on the CPU and its result is wanted straight
/// back.
pub fn patchwork(pixmap: &mut Pixmap, square: u32, relief: u32) {
    if pixmap.is_empty() {
        return;
    }
    let n = (square.clamp(*PATCH_SQUARE.start(), *PATCH_SQUARE.end()) + PATCH_SQUARE_FROM) as usize;
    // Deepening by the square root: CS6 at Relief 16 has darker joints than
    // at 8, not twice the shading everywhere.
    let relief = (relief.clamp(*PATCH_RELIEF.start(), *PATCH_RELIEF.end()) as f32 / PATCH_RELIEF_FULL).sqrt();
    let (width, height) = (pixmap.width() as usize, pixmap.height() as usize);
    let (cols, rows) = (width.div_ceil(n), height.div_ceil(n));
    let stride = pixmap.stride();

    // Each square's average colour, lifted, and its height.
    let bytes = pixmap.as_bytes();
    let squares: Vec<([f32; 3], f32)> = (0..cols * rows)
        .into_par_iter()
        .map(|k| {
            let (cx, cy) = (k % cols, k / cols);
            let mut total = [0.0f32; 3];
            let mut count = 0.0f32;
            for y in cy * n..((cy + 1) * n).min(height) {
                for x in cx * n..((cx + 1) * n).min(width) {
                    let p = &bytes[y * stride + x * 4..];
                    for c in 0..3 {
                        total[c] += p[c] as f32;
                    }
                    count += 1.0;
                }
            }
            let mean = total.map(|t| t / count.max(1.0));
            let lum = (0.299 * mean[0] + 0.587 * mean[1] + 0.114 * mean[2]) / 255.0;
            // Lifted by brightness rather than channel by channel, so the
            // browns keep their warmth instead of greying.
            let gain = if lum > 0.0 { lum.powf(PATCH_LIFT) / lum } else { 1.0 };
            // The variation is the tiles' depth, so it goes with Relief.
            let vary = (lattice(cx as i32, cy as i32, 111) - 0.5) * 2.0 * PATCH_VARY * 255.0 * relief.min(1.5);
            let face = mean.map(|v| (v * gain + vary).clamp(0.0, 255.0));
            (face, lum)
        })
        .collect();
    let squares = &squares;

    // The blocks' height at any point, in pixels.
    let size = n as f32;
    let round = (size * PATCH_ROUND).max(PATCH_ROUND_MIN).min(size * 0.5);
    let surface = move |x: f32, y: f32| -> f32 {
        let x = x.clamp(0.0, width as f32 - 0.001);
        let y = y.clamp(0.0, height as f32 - 0.001);
        let (cx, cy) = ((x / size) as usize, (y / size) as usize);
        let base = squares[cy * cols + cx].1 * PATCH_STEP;
        let rim = if n >= 3 {
            // A quarter circle over the rounding, in from an edge.
            let quarter = |edge: f32, r: f32| {
                let t = 1.0 - (edge / r).min(1.0);
                (1.0 - t * t).max(0.0).sqrt()
            };
            let (u, v) = (x - cx as f32 * size, y - cy as f32 * size);
            let down = quarter(v.min(size - v), round);
            let across = quarter(u.min(size - u), round * PATCH_ROUND_SIDES);
            down * (1.0 - PATCH_SIDE_DEPTH * (1.0 - across))
        } else {
            1.0
        };
        let tilt = if n >= 3 { (y / size).fract() * PATCH_TILT } else { 0.0 };
        (base + rim * PATCH_RIM + tilt) * relief
    };

    let (lx, ly) = {
        let (x, y) = PATCH_LIGHT;
        let len = (x * x + y * y).sqrt();
        (x / len, y / len)
    };
    let elevation = PATCH_ELEVATION.to_radians();
    let (flat, rise) = (elevation.cos(), elevation.tan());
    let light = [lx * flat, ly * flat, elevation.sin()];
    let diffuse = 1.0 - PATCH_AMBIENT;
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, out)| {
            for (x, px) in out.chunks_exact_mut(4).enumerate() {
                let colour = squares[(y / n) * cols + x / n].0;
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                // The slope, across the pixel, and how squarely it faces
                // the light against a flat face.
                let dx = surface(fx + 0.5, fy) - surface(fx - 0.5, fy);
                let dy = surface(fx, fy + 0.5) - surface(fx, fy - 0.5);
                let norm = (dx * dx + dy * dy + 1.0).sqrt();
                let facing = ((-dx * light[0] - dy * light[1] + light[2]) / norm).max(0.0);
                // The shadow: how far anything between here and the light
                // stands above the ray back to it.
                let here = surface(fx, fy);
                let mut over = 0.0f32;
                let mut t = PATCH_SHADOW_STEP;
                while t <= PATCH_SHADOW_REACH {
                    let h = surface(fx + lx * t, fy + ly * t);
                    over = over.max(h - here - t * rise);
                    t += PATCH_SHADOW_STEP;
                }
                let lit = 1.0 - (over / PATCH_SHADOW_SOFT).clamp(0.0, 1.0);
                let light_here = PATCH_AMBIENT + diffuse * facing * lit;
                // Against a square's own tilted face, so the face keeps the
                // square's colour.
                let slope = PATCH_TILT * relief / size;
                let light_flat = PATCH_AMBIENT
                    + diffuse * ((-slope * light[1] + light[2]) / (slope * slope + 1.0).sqrt());
                let k = light_here / light_flat;
                for c in 0..3 {
                    px[c] = (colour[c] * k).round().clamp(0.0, 255.0) as u8;
                }
                // Alpha stands: patching the picture does not change the
                // layer's shape.
            }
        });
}

/// How Texturizer lights its surface, beyond the controls.
///
/// **Texturizer's relief is far deeper than the other filters'.** Measured on
/// CS6's output over `samples/horse-3.jpg`, registered against the source,
/// its Brick at Relief 24 moves a pixel by 80 levels, one standard deviation,
/// and its Burlap and Sandstone at 16 by 70 and 40 — six to fifteen times
/// what the same Relief does in Rough Pastels. So each texture has its own
/// gain, fitted on those numbers; Canvas, which there was no reference for,
/// is a guess of the same order.
///
/// **And it lights every tone alike.** CS6 moves the horse's blacks as far
/// as the sky's whites — its brick throws white edges across the black
/// horse — so the share of the light that scales with the picture's own
/// brightness is mostly taken back out by the dark bias.
const TEXTURIZER_FINISH: Finish = Finish {
    dark_bias: 0.6,
    bevel: BEVEL,
    crisp: 1.0,
    occlusion: 0.0,
    patchy: 0.0,
    gain: 1.0,
};
const TEXTURIZER_BURLAP_GAIN: f32 = 13.0;
const TEXTURIZER_CANVAS_GAIN: f32 = 20.0;
const TEXTURIZER_SANDSTONE_GAIN: f32 = 15.0;

/// Brick is lit harder still, and crisply: CS6's shows a white lip along the
/// top of each course and a black joint under it, with the face between
/// keeping the picture's tone. The grit on the face slopes almost as steeply
/// as the joints do, so without the crispness — which lifts strong slopes
/// over faint ones — the depth that draws the joints turns every face to
/// speckle. The occlusion is what makes the joint black rather than merely
/// shaded.
const TEXTURIZER_BRICK_GAIN: f32 = 135.0;
const TEXTURIZER_BRICK_CRISP: f32 = 2.2;
const TEXTURIZER_BRICK_OCCLUSION: f32 = 60.0;

/// Filter ▸ Texture ▸ Texturizer: the picture printed on one of the four
/// surfaces, and nothing else done to it.
///
/// It is the texture block Rough Pastels, Underpainting and Conté Crayon
/// carry, on its own: the same controls in the same order — **Texture**,
/// **Scaling**, **Relief**, **Light** and **Invert** — over the same ranges,
/// and the same four surfaces, so a brick here is the brick there. It is lit
/// much more deeply than they are; see [`TEXTURIZER_FINISH`].
///
/// Alpha is left alone.
///
/// No GPU path, for [`apply_relief`]'s reasons.
pub fn texturizer(
    pixmap: &mut Pixmap,
    texture: Texture,
    scaling: u32,
    relief: u32,
    light: Light,
    invert: bool,
) {
    let finish = match texture {
        Texture::Brick => Finish {
            gain: TEXTURIZER_BRICK_GAIN,
            crisp: TEXTURIZER_BRICK_CRISP,
            occlusion: TEXTURIZER_BRICK_OCCLUSION,
            ..TEXTURIZER_FINISH
        },
        Texture::Burlap => Finish { gain: TEXTURIZER_BURLAP_GAIN, ..TEXTURIZER_FINISH },
        Texture::Canvas => Finish { gain: TEXTURIZER_CANVAS_GAIN, ..TEXTURIZER_FINISH },
        Texture::Sandstone => Finish { gain: TEXTURIZER_SANDSTONE_GAIN, ..TEXTURIZER_FINISH },
    };
    apply_relief_weighted(pixmap, texture, scaling, relief, light, invert, finish);
}

/// CS6's ranges for Stained Glass, which its three sliders run over.
pub const GLASS_CELL: std::ops::RangeInclusive<u32> = 2..=50;
pub const GLASS_BORDER: std::ops::RangeInclusive<u32> = 1..=20;
pub const GLASS_LIGHT: std::ops::RangeInclusive<u32> = 0..=10;

/// How far apart the panes' seeds are, in pixels per step of Cell Size.
/// CS6's panes are bigger than the slider reads: across a row its sky
/// crosses a lead every 20 pixels or so at Cell Size 10, and every 49 at 26.
const GLASS_SPACING: f32 = 2.2;

/// How far each seed is nudged off its lattice point, as a share of the
/// lattice step. CS6's panes are irregular but even — five and six sided,
/// none much bigger than the rest — which is a hexagonal lattice shaken a
/// little rather than points thrown down anyhow; a free scatter leaves slivers
/// beside panes three times their size.
const GLASS_JITTER: f32 = 0.6;

/// How wide the lead is, in pixels per step of Border Thickness. CS6's lead
/// is about two and a half pixels at 4 and seven at 10.
const GLASS_LEAD: f32 = 0.62;

/// Light Intensity's glow: how far it reaches from the middle of the image,
/// as a share of the image's size, and how much of the way to white it takes
/// the glass there at full intensity.
///
/// It grows with the **square** of the slider. Measured on CS6's output over
/// `samples/horse-3.jpg`, the glass in the middle goes 85% of the way to
/// white at 7 and hardly a tenth at 2 — a straight line through the first
/// would light the second three times too brightly. At 7 it is still half as
/// strong a fifth of the image out, and has faded to a sixth a third out.
const GLASS_GLOW_REACH: f32 = 0.26;
const GLASS_GLOW: f32 = 1.75;

/// Filter ▸ Texture ▸ Stained Glass: the picture remade as panes of flat
/// colour held in lead.
///
/// The panes are the cells of a Voronoi diagram — every pixel belongs to its
/// nearest seed — laid on a jittered hexagonal lattice whose spacing
/// follows **Cell Size**; see [`GLASS_JITTER`]. Each pane is filled with the
/// average of the picture under it. The lead between them is **Border
/// Thickness** wide and in the **foreground colour**, as CS6's is, and it
/// runs round the edge of the image as well, half as wide there, since the
/// pane on the far side is missing. **Light Intensity** is a glow centred on
/// the image, as if the window were lit from behind at its middle: it takes
/// the glass towards white and leaves the lead alone.
///
/// Alpha is left alone.
///
/// No GPU path, for Crystallize's reasons: each pane's average needs every
/// pixel in it gathered first, and the whole filter is a few tens of
/// milliseconds on the CPU.
pub fn stained_glass(
    pixmap: &mut Pixmap,
    cell_size: u32,
    border: u32,
    light: u32,
    lead: crate::buffer::Rgba8,
) {
    if pixmap.is_empty() {
        return;
    }
    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;
    let step = GLASS_SPACING * cell_size.clamp(*GLASS_CELL.start(), *GLASS_CELL.end()) as f32;
    let row_step = step * 3f32.sqrt() / 2.0;
    let half_lead = 0.5 * GLASS_LEAD * border.clamp(*GLASS_BORDER.start(), *GLASS_BORDER.end()) as f32;

    // Seeds from two steps before the canvas to two after, so a pixel at the
    // edge has neighbours on every side to be nearer to.
    let cols = (width as f32 / step).ceil() as i32 + 5;
    let rows = (height as f32 / row_step).ceil() as i32 + 5;
    let seed_at = |i: i32, j: i32| -> (f32, f32) {
        // Every other row is shifted half a step, which is what makes the
        // lattice hexagonal.
        let shift = 0.5 * (j & 1) as f32;
        (
            (i as f32 + shift + GLASS_JITTER * (lattice(i, j, 41) - 0.5)) * step,
            (j as f32 + GLASS_JITTER * (lattice(i, j, 42) - 0.5)) * row_step,
        )
    };
    let index_of = |i: i32, j: i32| -> usize { ((j + 2) * cols + (i + 2)) as usize };

    // Which pane each pixel is in, and how far it is from the pane's edge —
    // the nearer of the image's edge and the line halfway to a neighbouring
    // seed. Worked out once and kept: both the averages and the painting
    // need it.
    let (w, h) = (width as usize, height as usize);
    let mut owner = vec![0u32; w * h];
    let mut inset = vec![0f32; w * h];
    owner
        .par_chunks_exact_mut(w)
        .zip(inset.par_chunks_exact_mut(w))
        .enumerate()
        .for_each(|(row, (owners, insets))| {
            let py = row as f32 + 0.5;
            let j0 = (py / row_step).floor() as i32;
            for x in 0..w {
                let px = x as f32 + 0.5;
                let i0 = (px / step).floor() as i32;
                let mut near = [(0f32, 0f32); 25];
                let mut n = 0;
                let (mut best, mut best_at, mut best_index) = (f32::MAX, 0, 0usize);
                for j in (j0 - 2).max(-2)..=(j0 + 2).min(rows - 3) {
                    for i in (i0 - 2).max(-2)..=(i0 + 2).min(cols - 3) {
                        let (sx, sy) = seed_at(i, j);
                        let d = (sx - px) * (sx - px) + (sy - py) * (sy - py);
                        if d < best {
                            best = d;
                            best_at = n;
                            best_index = index_of(i, j);
                        }
                        near[n] = (sx, sy);
                        n += 1;
                    }
                }
                let (bx, by) = near[best_at];
                // The distance to the bisector between the nearest seed and
                // each other one, which is the distance to that side of the
                // pane.
                let mut edge = px.min(width as f32 - px).min(py).min(height as f32 - py);
                for (k, &(sx, sy)) in near[..n].iter().enumerate() {
                    if k == best_at {
                        continue;
                    }
                    let d = (sx - px) * (sx - px) + (sy - py) * (sy - py);
                    let apart = ((sx - bx) * (sx - bx) + (sy - by) * (sy - by)).sqrt();
                    if apart > 0.0 {
                        edge = edge.min((d - best) / (2.0 * apart));
                    }
                }
                owners[x] = best_index as u32;
                insets[x] = edge;
            }
        });

    // What each pane averages to, in one sweep: a per-thread tally of every
    // pane would cost more than the image at the smallest cell size.
    let count = (cols * rows) as usize;
    let mut totals = vec![[0u64; 3]; count];
    let mut counts = vec![0u32; count];
    for (i, px) in pixmap.as_bytes().chunks_exact(4).enumerate() {
        let pane = owner[i] as usize;
        for c in 0..3 {
            totals[pane][c] += px[c] as u64;
        }
        counts[pane] += 1;
    }
    let panes: Vec<[f32; 3]> = totals
        .iter()
        .zip(&counts)
        .map(|(t, &n)| {
            let n = n.max(1) as f32;
            [t[0] as f32 / n, t[1] as f32 / n, t[2] as f32 / n]
        })
        .collect();

    let reach = GLASS_GLOW_REACH * (width as f32 * height as f32).sqrt();
    let intensity = light.min(*GLASS_LIGHT.end()) as f32 / *GLASS_LIGHT.end() as f32;
    let peak = (GLASS_GLOW * intensity * intensity).min(1.0);
    let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);
    let lead = [lead.r as f32, lead.g as f32, lead.b as f32];
    let (owner, inset, panes) = (&owner, &inset, &panes);
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let dy = row as f32 + 0.5 - cy;
            for (x, px) in out.chunks_exact_mut(4).take(w).enumerate() {
                let i = row * w + x;
                // A pixel's worth of soft edge, so the lead is not stepped.
                let glass = (inset[i] - half_lead + 0.5).clamp(0.0, 1.0);
                let dx = x as f32 + 0.5 - cx;
                let glow = peak * (-(dx * dx + dy * dy) / (reach * reach)).exp();
                let pane = panes[owner[i] as usize];
                for c in 0..3 {
                    let lit = pane[c] + (255.0 - pane[c]) * glow;
                    px[c] = (lead[c] + (lit - lead[c]) * glass).round().clamp(0.0, 255.0) as u8;
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Rgba8;

    fn glass_at(pixmap: &Pixmap, x: i32, y: i32) -> [u8; 3] {
        let p = pixmap.get(x, y);
        [p.r, p.g, p.b]
    }

    #[test]
    fn stained_glass_leads_in_the_foreground_colour_and_frames_the_image() {
        // The lead is the foreground colour, as CS6's is, and it runs round
        // the edge of the image as well as between the panes.
        let lead = Rgba8::new(200, 20, 30, 255);
        let mut px = Pixmap::filled(120, 90, Rgba8::new(40, 120, 220, 255));
        stained_glass(&mut px, 10, 6, 0, lead);
        for (x, y) in [(0, 0), (119, 0), (0, 89), (119, 89), (60, 0), (0, 45)] {
            assert_eq!(glass_at(&px, x, y), [200, 20, 30], "no lead at {x},{y}");
        }
        // Everything that is not lead is the one colour the picture had:
        // a flat picture makes flat panes.
        let mut glass = 0;
        for y in 0..90 {
            for x in 0..120 {
                let c = glass_at(&px, x, y);
                if c == [40, 120, 220] {
                    glass += 1;
                }
            }
        }
        assert!(glass > 120 * 90 / 2, "only {glass} pixels came back as glass");
    }

    #[test]
    fn stained_glass_panes_are_flat_averages() {
        // A ramp comes back as a handful of flat panes rather than a ramp:
        // many fewer distinct colours than it went in with.
        let mut px = Pixmap::new(200, 60);
        for y in 0..60 {
            for x in 0..200 {
                px.set(x, y, Rgba8::new(x as u8, 255 - x as u8, 128, 255));
            }
        }
        stained_glass(&mut px, 12, 1, 0, Rgba8::BLACK);
        let mut colours = std::collections::HashMap::new();
        for y in 0..60 {
            for x in 0..200 {
                *colours.entry(glass_at(&px, x, y)).or_insert(0) += 1;
            }
        }
        // A colour to a pane, and those cover the picture; what is left is
        // the soft edge of the lead, a pixel here and there.
        let panes: Vec<i32> = colours.values().copied().filter(|&n| n >= 10).collect();
        let covered: i32 = panes.iter().sum();
        assert!(panes.len() < 60, "{} colours each cover ten pixels or more", panes.len());
        assert!(covered > 200 * 60 * 7 / 10, "the panes cover only {covered} pixels");
    }

    #[test]
    fn a_thicker_border_is_more_lead() {
        let lead = |border| {
            let mut px = Pixmap::filled(160, 160, Rgba8::new(255, 255, 255, 255));
            stained_glass(&mut px, 10, border, 0, Rgba8::BLACK);
            px.as_bytes().chunks_exact(4).filter(|p| p[0] < 128).count()
        };
        let thin = lead(2);
        let thick = lead(10);
        assert!(thick > thin * 2, "Border Thickness 10 leads {thick} pixels and 2 leads {thin}");
    }

    #[test]
    fn bigger_cells_are_fewer_panes() {
        // Leads crossed along the middle row.
        let crossings = |cell| {
            let mut px = Pixmap::filled(400, 100, Rgba8::new(255, 255, 255, 255));
            stained_glass(&mut px, cell, 3, 0, Rgba8::BLACK);
            (1..400)
                .filter(|&x| px.get(x, 50).r < 128 && px.get(x - 1, 50).r >= 128)
                .count()
        };
        let small = crossings(5);
        let big = crossings(20);
        assert!(small > big * 2, "Cell Size 5 crossed {small} leads and 20 crossed {big}");
    }

    #[test]
    fn light_intensity_lights_the_middle_and_not_the_lead() {
        // The glow is centred on the image: the glass in the middle goes
        // towards white, the glass in a corner barely moves, and the lead
        // stays the colour it was given.
        let mut px = Pixmap::filled(300, 200, Rgba8::new(60, 60, 60, 255));
        stained_glass(&mut px, 8, 6, 10, Rgba8::BLACK);
        let brightest = |x0: i32, y0: i32| {
            (y0..y0 + 20)
                .flat_map(|y| (x0..x0 + 20).map(move |x| (x, y)))
                .map(|(x, y)| px.get(x, y).r)
                .max()
                .unwrap()
        };
        let middle = brightest(140, 90);
        let corner = brightest(4, 4);
        assert!(middle > 200, "the middle only reached {middle}");
        assert!(corner < 90, "the corner was lit to {corner}");
        assert_eq!(glass_at(&px, 0, 100), [0, 0, 0], "the glow reached the lead");
    }

    #[test]
    fn texturizer_leaves_a_picture_alone_at_no_relief() {
        let mut px = Pixmap::filled(40, 40, Rgba8::new(90, 140, 200, 255));
        let before = px.clone();
        texturizer(&mut px, Texture::Burlap, 100, 0, Light::Top, false);
        assert_eq!(px.as_bytes(), before.as_bytes());
    }

    #[test]
    fn texturizer_shows_as_plainly_in_black_as_in_white() {
        // CS6 lights every tone alike: its brick throws white edges across a
        // black horse as plainly as dark ones across the sky. A surface that
        // only scaled the picture's own brightness would vanish in the black.
        let spread = |level: u8| {
            let mut px = Pixmap::filled(120, 120, Rgba8::new(level, level, level, 255));
            texturizer(&mut px, Texture::Sandstone, 100, 16, Light::Top, false);
            let values: Vec<f32> = px.as_bytes().chunks_exact(4).map(|p| p[0] as f32).collect();
            let mean = values.iter().sum::<f32>() / values.len() as f32;
            (values.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / values.len() as f32).sqrt()
        };
        let dark = spread(30);
        let light = spread(200);
        assert!(dark > 20.0, "the surface barely shows in black: {dark:.1}");
        assert!(dark > light * 0.6, "black shows {dark:.1} of texture and white {light:.1}");
    }

    #[test]
    fn texturizer_brick_draws_its_courses() {
        // Lit from the top, each course has a lit lip and a dark joint: down
        // a column the picture swings far both ways once every course.
        let mut px = Pixmap::filled(80, 90, Rgba8::new(128, 128, 128, 255));
        texturizer(&mut px, Texture::Brick, 100, 24, Light::Top, false);
        let column: Vec<u8> = (0..90).map(|y| px.get(40, y).r).collect();
        let bright = column.iter().filter(|&&v| v > 220).count();
        let dark = column.iter().filter(|&&v| v < 40).count();
        assert!(bright >= 8, "only {bright} lit pixels down a column of ten courses");
        assert!(dark >= 8, "only {dark} dark pixels down a column of ten courses");
    }

    #[test]
    fn texturizer_invert_turns_the_surface_inside_out() {
        let run = |invert| {
            let mut px = Pixmap::filled(60, 60, Rgba8::new(128, 128, 128, 255));
            texturizer(&mut px, Texture::Canvas, 100, 12, Light::Top, invert);
            px
        };
        let (plain, inverted) = (run(false), run(true));
        assert_ne!(plain.as_bytes(), inverted.as_bytes());
    }

    #[test]
    fn stained_glass_is_deterministic() {
        let run = || {
            let mut px = Pixmap::new(90, 70);
            for y in 0..70 {
                for x in 0..90 {
                    px.set(x, y, Rgba8::new((x * 3) as u8, (y * 3) as u8, 90, 255));
                }
            }
            stained_glass(&mut px, 6, 3, 4, Rgba8::BLACK);
            px
        };
        assert_eq!(run().as_bytes(), run().as_bytes());
    }

    #[test]
    fn every_texture_stays_in_range_and_is_not_flat() {
        for texture in [Texture::Brick, Texture::Burlap, Texture::Canvas, Texture::Sandstone] {
            let field = height_map(texture, 64, 64, 100);
            let (lo, hi) = field
                .iter()
                .fold((f32::MAX, f32::MIN), |(lo, hi), &h| (lo.min(h), hi.max(h)));
            assert!(lo >= 0.0 && hi <= 1.2, "{texture:?} ran from {lo} to {hi}");
            assert!(hi - lo > 0.3, "{texture:?} is nearly flat: {lo}..{hi}");
        }
    }

    /// Brick's mortar runs in rows, so at double the scaling the rows are
    /// twice as far apart.
    #[test]
    fn scaling_sizes_the_texture() {
        // Averaged across a row, so the grit of the face evens out and the
        // joints between courses stand out.
        let rows = |scaling| {
            let field = height_map(Texture::Brick, 160, 200, scaling);
            let mean = |y: usize| field[y * 160..(y + 1) * 160].iter().sum::<f32>() / 160.0;
            (1..200).filter(|&y| mean(y) < 0.1 && mean(y - 1) >= 0.1).count()
        };
        let (normal, double) = (rows(100), rows(200));
        assert!(normal >= double * 2 - 1 && normal <= double * 2 + 1, "{normal} rows against {double}");
    }

    /// The same edge is lit from one side and shaded from the other, and
    /// Invert swaps the two.
    #[test]
    fn the_light_and_invert_decide_which_side_is_lit() {
        let lit = |light, invert| {
            let mut pm = Pixmap::filled(40, 40, Rgba8::new(128, 128, 128, 255));
            apply_relief(&mut pm, Texture::Brick, 100, 50, light, invert);
            pm.as_bytes().chunks_exact(4).map(|p| p[0] as i64).collect::<Vec<_>>()
        };
        let (top, bottom, inverted) = (
            lit(Light::Top, false),
            lit(Light::Bottom, false),
            lit(Light::Bottom, true),
        );
        assert_ne!(top, bottom);
        let moved: i64 = top.iter().zip(&inverted).map(|(a, b)| (a - b).abs()).sum();
        assert!(moved < 40 * 40, "inverting the surface did not swap the light: {moved}");
    }

    #[test]
    fn no_relief_changes_nothing() {
        let mut pm = Pixmap::filled(16, 16, Rgba8::new(10, 200, 30, 90));
        let before = pm.clone();
        apply_relief(&mut pm, Texture::Sandstone, 100, 0, Light::Top, false);
        assert_eq!(pm.as_bytes(), before.as_bytes());
    }
    fn crack_tone(pm: &Pixmap) -> Vec<f32> {
        pm.as_bytes().chunks_exact(4).map(|p| p[0] as f32).collect()
    }

    /// A flat sheet comes out cracked: some of it much darker than the rest,
    /// where the cracks are, and the more spacing the fewer of them.
    #[test]
    fn craquelure_cracks_a_flat_sheet_and_spacing_thins_the_cracks() {
        let dark_share = |spacing| {
            let mut pm = Pixmap::filled(160, 160, Rgba8::new(200, 200, 200, 255));
            craquelure(&mut pm, spacing, 6, 0);
            let tone = crack_tone(&pm);
            tone.iter().filter(|&&v| v < 100.0).count() as f32 / tone.len() as f32
        };
        let (tight, wide) = (dark_share(10), dark_share(80));
        assert!(tight > 0.05, "hardly any cracks at spacing 10: {tight}");
        assert!(wide > 0.005, "no cracks at spacing 80: {wide}");
        assert!(tight > wide * 2.0, "{tight} against {wide}");
    }

    /// Crack Brightness lifts the bottom of the cracks towards the picture.
    #[test]
    fn craquelure_brightness_lightens_the_cracks() {
        let darkest = |brightness| {
            let mut pm = Pixmap::filled(96, 96, Rgba8::new(200, 200, 200, 255));
            craquelure(&mut pm, 15, 0, brightness);
            crack_tone(&pm).into_iter().fold(f32::MAX, f32::min)
        };
        assert!(darkest(10) > darkest(0) + 80.0, "{} vs {}", darkest(10), darkest(0));
    }

    /// Crack Depth is how hard the plates are lit: at 0 a plate is flat, and
    /// deeper spreads its tones apart.
    #[test]
    fn craquelure_depth_deepens_the_relief() {
        let spread = |depth| {
            let mut pm = Pixmap::filled(96, 96, Rgba8::new(128, 128, 128, 255));
            // Cracks as light as the plates, so only the lighting spreads.
            craquelure(&mut pm, 15, depth, 10);
            let tone = crack_tone(&pm);
            let mean = tone.iter().sum::<f32>() / tone.len() as f32;
            (tone.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / tone.len() as f32).sqrt()
        };
        assert!(spread(10) > spread(0) + 10.0, "{} vs {}", spread(10), spread(0));
    }

    #[test]
    fn craquelure_is_deterministic_and_leaves_alpha_alone() {
        let mut a = Pixmap::filled(48, 48, Rgba8::new(120, 140, 160, 77));
        a.fill_rect(crate::buffer::Rect::new(4, 4, 8, 8), Rgba8::new(20, 20, 20, 200));
        let before = a.clone();
        let mut b = a.clone();
        craquelure(&mut a, 15, 6, 9);
        craquelure(&mut b, 15, 6, 9);
        assert_eq!(a.as_bytes(), b.as_bytes());
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&a), alpha(&before));
    }

    #[test]
    fn craquelure_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        craquelure(&mut pm, 15, 6, 9);
    }
    /// The network follows the picture: across a light sheet the cracks run
    /// mostly across, so a cracked pixel's neighbour to the side is cracked
    /// more often than the one below it. Across a dark sheet the paint
    /// crumples every which way, and the two come out alike.
    #[test]
    fn craquelure_courses_the_light_and_crumples_the_dark() {
        // How much more often cracks run across than down.
        let across_over_down = |tone: u8| {
            let n = 192usize;
            let mut pm = Pixmap::filled(n as u32, n as u32, Rgba8::new(tone, tone, tone, 255));
            // No lighting and black-bottomed cracks: only the cracks show.
            craquelure(&mut pm, 20, 0, 0);
            let cracked: Vec<bool> =
                crack_tone(&pm).iter().map(|&v| v < tone as f32 * 0.6).collect();
            let (mut across, mut down) = (0, 0);
            for y in 0..n - 1 {
                for x in 0..n - 1 {
                    let i = y * n + x;
                    if cracked[i] {
                        across += cracked[i + 1] as u32;
                        down += cracked[i + n] as u32;
                    }
                }
            }
            across as f32 / down.max(1) as f32
        };
        let (light, dark) = (across_over_down(220), across_over_down(40));
        assert!(light > 1.3, "the light sheet's cracks do not run across: {light}");
        assert!((0.75..1.3).contains(&dark), "the dark sheet's cracks lean one way: {dark}");
    }

    /// The cracks do not close into plates. CS6's are runs that meet now and
    /// then and stop short, so the uncracked paint is mostly one surface
    /// running round their ends; a network closed all round cuts it into
    /// jigsaw pieces, none of them more than a sliver of the whole.
    #[test]
    fn craquelure_cracks_stop_short_rather_than_cutting_out_pieces() {
        for tone in [220u8, 40] {
            let n = 192usize;
            let mut pm = Pixmap::filled(n as u32, n as u32, Rgba8::new(tone, tone, tone, 255));
            craquelure(&mut pm, 20, 0, 0);
            let paint: Vec<bool> =
                crack_tone(&pm).iter().map(|&v| v > tone as f32 * 0.8).collect();
            // The largest run of connected paint, by flood fill.
            let mut seen = vec![false; n * n];
            let mut largest = 0;
            for start in 0..n * n {
                if !paint[start] || seen[start] {
                    continue;
                }
                let (mut stack, mut size) = (vec![start], 0);
                seen[start] = true;
                while let Some(i) = stack.pop() {
                    size += 1;
                    let (x, y) = (i % n, i / n);
                    let mut visit = |j: usize| {
                        if paint[j] && !seen[j] {
                            seen[j] = true;
                            stack.push(j);
                        }
                    };
                    if x > 0 { visit(i - 1); }
                    if x + 1 < n { visit(i + 1); }
                    if y > 0 { visit(i - n); }
                    if y + 1 < n { visit(i + n); }
                }
                largest = largest.max(size);
            }
            let total = paint.iter().filter(|&&p| p).count();
            // The crumple's contours do ring off the odd island, as CS6's
            // crumpled horse does; the courses hardly ever close.
            let share = if tone > 128 { 0.8 } else { 0.5 };
            assert!(
                largest as f32 > total as f32 * share,
                "tone {tone}: the cracks cut the paint into pieces, the largest {largest} of {total}"
            );
        }
    }
    /// The cracks are grooves, and their lit rims throw shadow. With the
    /// crack floors lifted to the paint's own tone, anything darker than the
    /// paint is shade, and there should be plenty of it when the cracks are
    /// deep and none when they are flat.
    #[test]
    fn craquelure_grooves_cast_shadows() {
        let shaded = |depth| {
            let mut pm = Pixmap::filled(160, 160, Rgba8::new(128, 128, 128, 255));
            craquelure(&mut pm, 20, depth, 10);
            let tone = crack_tone(&pm);
            tone.iter().filter(|&&v| v < 128.0 * 0.7).count() as f32 / tone.len() as f32
        };
        let (deep, flat) = (shaded(10), shaded(0));
        assert!(deep > 0.04, "deep cracks threw hardly any shadow: {deep}");
        assert!(flat < 0.001, "flat cracks threw shadow: {flat}");
    }
    fn spread_of(pm: &Pixmap) -> f32 {
        let t = crack_tone(pm);
        let m = t.iter().sum::<f32>() / t.len() as f32;
        (t.iter().map(|v| (v - m).powi(2)).sum::<f32>() / t.len() as f32).sqrt()
    }

    /// Intensity is how strong the grain is: none at 0, plenty at 100.
    #[test]
    fn grain_intensity_strengthens_the_grain() {
        let run = |intensity| {
            let mut pm = Pixmap::filled(96, 96, Rgba8::new(128, 128, 128, 255));
            grain(&mut pm, intensity, 50, GrainType::Regular, Rgba8::BLACK, Rgba8::WHITE);
            spread_of(&pm)
        };
        assert!(run(0) < 0.5, "grain at Intensity 0: {}", run(0));
        assert!(run(100) > 30.0, "hardly any grain at 100: {}", run(100));
    }

    /// Clumped grain is gathered into clumps: neighbouring pixels move
    /// together, where Regular's are independent.
    #[test]
    fn grain_clumped_gathers_the_noise() {
        let neighbour_likeness = |kind| {
            let mut pm = Pixmap::filled(96, 96, Rgba8::new(128, 128, 128, 255));
            grain(&mut pm, 60, 50, kind, Rgba8::BLACK, Rgba8::WHITE);
            let t = crack_tone(&pm);
            let m = t.iter().sum::<f32>() / t.len() as f32;
            let (mut together, mut apart) = (0.0, 0.0);
            for i in 0..t.len() - 1 {
                together += (t[i] - m) * (t[i + 1] - m);
                apart += (t[i] - m).powi(2);
            }
            together / apart
        };
        assert!(neighbour_likeness(GrainType::Regular) < 0.2);
        assert!(neighbour_likeness(GrainType::Clumped) > 0.5);
    }

    /// Stippled cuts the picture into the two swatches, and more of it into
    /// the foreground where the picture is dark.
    #[test]
    fn grain_stippled_is_the_two_swatches() {
        let (ink, paper) = (Rgba8::new(0, 0, 255, 255), Rgba8::new(255, 255, 0, 255));
        let inked = |tone: u8| {
            let mut pm = Pixmap::filled(64, 64, Rgba8::new(tone, tone, tone, 255));
            grain(&mut pm, 50, 50, GrainType::Stippled, ink, paper);
            let mut count = 0;
            for p in pm.as_bytes().chunks_exact(4) {
                assert!(p[..3] == [0, 0, 255] || p[..3] == [255, 255, 0], "{p:?}");
                count += (p[2] == 255 && p[0] == 0) as u32;
            }
            count
        };
        assert!(inked(60) > inked(190) + 1000, "{} vs {}", inked(60), inked(190));
    }

    /// Horizontal grain bands the picture in rows: averaged along each row
    /// the bands stand out, averaged down each column they cancel. Vertical
    /// the other way.
    #[test]
    fn grain_streaks_run_their_way() {
        let lean = |kind| {
            let n = 128usize;
            let mut pm = Pixmap::filled(n as u32, n as u32, Rgba8::new(160, 160, 160, 255));
            grain(&mut pm, 80, 50, kind, Rgba8::BLACK, Rgba8::WHITE);
            let t = crack_tone(&pm);
            let spread = |means: Vec<f32>| {
                let m = means.iter().sum::<f32>() / means.len() as f32;
                (means.iter().map(|x| (x - m).powi(2)).sum::<f32>() / means.len() as f32).sqrt()
            };
            let rows = spread((0..n).map(|y| t[y * n..(y + 1) * n].iter().sum::<f32>() / n as f32).collect());
            let cols = spread((0..n).map(|x| (0..n).map(|y| t[y * n + x]).sum::<f32>() / n as f32).collect());
            rows / cols
        };
        assert!(lean(GrainType::Horizontal) > 3.0, "{}", lean(GrainType::Horizontal));
        assert!(lean(GrainType::Vertical) < 0.33, "{}", lean(GrainType::Vertical));
    }

    /// Speckle throws the foreground over the dark and leaves the light
    /// clean; Sprinkles throws the background.
    #[test]
    fn grain_speckle_and_sprinkles_throw_the_swatches() {
        let ink = Rgba8::new(255, 0, 0, 255);
        let paper = Rgba8::new(0, 255, 0, 255);
        let count = |tone: u8, kind, swatch: Rgba8| {
            let mut pm = Pixmap::filled(64, 64, Rgba8::new(tone, tone, tone, 255));
            grain(&mut pm, 60, 50, kind, ink, paper);
            pm.as_bytes()
                .chunks_exact(4)
                .filter(|p| p[..3] == [swatch.r, swatch.g, swatch.b])
                .count()
        };
        assert!(count(30, GrainType::Speckle, ink) > 800);
        assert!(count(250, GrainType::Speckle, ink) < 20);
        assert!(count(128, GrainType::Sprinkles, paper) > 800);
    }

    #[test]
    fn grain_is_deterministic_and_leaves_alpha_alone() {
        for k in 0..10 {
            let kind = GrainType::from_i32(k);
            let mut a = Pixmap::filled(48, 48, Rgba8::new(120, 140, 160, 77));
            let before = a.clone();
            let mut b = a.clone();
            grain(&mut a, 60, 60, kind, Rgba8::BLACK, Rgba8::WHITE);
            grain(&mut b, 60, 60, kind, Rgba8::BLACK, Rgba8::WHITE);
            assert_eq!(a.as_bytes(), b.as_bytes(), "{kind:?}");
            let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
            assert_eq!(alpha(&a), alpha(&before), "{kind:?}");
        }
        grain(&mut Pixmap::new(0, 0), 40, 50, GrainType::Regular, Rgba8::BLACK, Rgba8::WHITE);
    }
    /// Contrasty is contrasty at the slider's flat middle: a dark sheet goes
    /// darker and a light one lighter than Regular leaves them.
    #[test]
    fn grain_contrasty_pushes_the_tones_apart_on_its_own() {
        let mean = |tone: u8, kind| {
            let mut pm = Pixmap::filled(64, 64, Rgba8::new(tone, tone, tone, 255));
            grain(&mut pm, 40, 50, kind, Rgba8::BLACK, Rgba8::WHITE);
            let t = crack_tone(&pm);
            t.iter().sum::<f32>() / t.len() as f32
        };
        let span = |kind| mean(200, kind) - mean(60, kind);
        assert!(
            span(GrainType::Contrasty) > span(GrainType::Regular) * 1.4,
            "{} vs {}",
            span(GrainType::Contrasty),
            span(GrainType::Regular)
        );
    }

    /// CS6's streaks are long: a row of Horizontal grain stays alike far along
    /// it, not only between neighbours.
    #[test]
    fn grain_streaks_run_long() {
        let mut pm = Pixmap::filled(256, 64, Rgba8::new(140, 140, 140, 255));
        grain(&mut pm, 60, 50, GrainType::Horizontal, Rgba8::BLACK, Rgba8::WHITE);
        let t = crack_tone(&pm);
        let m = t.iter().sum::<f32>() / t.len() as f32;
        // How alike pixels 40 apart along a row are, against 40 apart down.
        let (mut along, mut across, mut own) = (0.0, 0.0, 0.0);
        for y in 0..24 {
            for x in 0..200 {
                let i = y * 256 + x;
                along += (t[i] - m) * (t[i + 40] - m);
                across += (t[i] - m) * (t[i + 40 * 256] - m);
                own += (t[i] - m).powi(2);
            }
        }
        assert!(along / own > 0.3, "the streaks are short: {}", along / own);
        assert!((across / own).abs() < 0.1, "rows move together: {}", across / own);
    }

    /// Speckle's specks sit on a mesh: the rows and columns of the mesh take
    /// far more of them than the pixels between.
    #[test]
    fn grain_speckle_lays_its_specks_on_a_mesh() {
        let mut pm = Pixmap::filled(96, 96, Rgba8::new(90, 90, 90, 255));
        grain(&mut pm, 50, 50, GrainType::Speckle, Rgba8::new(255, 0, 0, 255), Rgba8::WHITE);
        let (mut on, mut off, mut n_on, mut n_off) = (0.0, 0.0, 0.0, 0.0);
        for (i, p) in pm.as_bytes().chunks_exact(4).enumerate() {
            let (x, y) = (i % 96, i / 96);
            let speck = (p[..3] == [255, 0, 0]) as u32 as f32;
            if x % GRAIN_MESH == 0 || y % GRAIN_MESH == 0 {
                on += speck;
                n_on += 1.0;
            } else {
                off += speck;
                n_off += 1.0;
            }
        }
        assert!(on / n_on > 2.0 * off / n_off, "{} vs {}", on / n_on, off / n_off);
    }
    /// How grey each pixel of a flat sheet comes out after tiling, and
    /// whether it is grout: the grout is pulled towards grey, so on a black
    /// sheet it is the lighter part.
    fn tiled(size: u32, grout: u32, lighten: u32) -> Vec<f32> {
        let mut pm = Pixmap::filled(200, 200, Rgba8::new(0, 0, 0, 255));
        mosaic_tiles(&mut pm, size, grout, lighten);
        crack_tone(&pm)
    }

    /// The grout runs in a grid Tile Size apart: across a black sheet, the
    /// rows that are mostly grout come round every Tile Size.
    #[test]
    fn mosaic_tiles_lays_a_grid_tile_size_apart() {
        let lines = |size| {
            let t = tiled(size, 4, 10);
            let row = |y: usize| t[y * 200..(y + 1) * 200].iter().sum::<f32>() / 200.0;
            let rows: Vec<f32> = (0..200).map(row).collect();
            let mean = rows.iter().sum::<f32>() / 200.0;
            (1..200).filter(|&y| rows[y] > mean && rows[y - 1] <= mean).count()
        };
        let (small, big) = (lines(20), lines(40));
        assert!((8..=12).contains(&small), "{small} grout lines at size 20");
        assert!((4..=6).contains(&big), "{big} grout lines at size 40");
    }

    /// Grout Width widens the grout, and Lighten Grout lightens it.
    #[test]
    fn mosaic_tiles_grout_widens_and_lightens() {
        let share = |grout| tiled(30, grout, 10).iter().filter(|&&v| v > 60.0).count();
        assert!(share(12) > share(2) * 2, "{} vs {}", share(12), share(2));
        let lightest = |lighten| tiled(30, 8, lighten).into_iter().fold(0.0f32, f32::max);
        assert!(lightest(10) > lightest(0) + 60.0, "{} vs {}", lightest(10), lightest(0));
    }

    /// The tiles keep the picture's colour: a red sheet's tiles are red.
    #[test]
    fn mosaic_tiles_keep_the_picture_on_the_tiles() {
        let mut pm = Pixmap::filled(120, 120, Rgba8::new(200, 30, 30, 255));
        mosaic_tiles(&mut pm, 40, 3, 9);
        let reds = pm
            .as_bytes()
            .chunks_exact(4)
            .filter(|p| p[0] as i32 > p[1] as i32 + 100)
            .count();
        assert!(reds > 120 * 120 / 2, "only {reds} red pixels");
    }

    #[test]
    fn mosaic_tiles_is_deterministic_and_leaves_alpha_alone() {
        let mut a = Pixmap::filled(48, 48, Rgba8::new(120, 140, 160, 77));
        let before = a.clone();
        let mut b = a.clone();
        mosaic_tiles(&mut a, 12, 3, 9);
        mosaic_tiles(&mut b, 12, 3, 9);
        assert_eq!(a.as_bytes(), b.as_bytes());
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&a), alpha(&before));
        mosaic_tiles(&mut Pixmap::new(0, 0), 12, 3, 9);
    }
    /// At CS6's defaults the tiles are flat and the grout thin, so most of
    /// the picture comes through as it was: tiles rounded into domes, or
    /// grout laid thick, would change nearly all of it.
    #[test]
    fn mosaic_tiles_leave_the_picture_on_flat_faces() {
        let mut pm = Pixmap::filled(160, 160, Rgba8::new(90, 90, 90, 255));
        mosaic_tiles(&mut pm, 12, 3, 9);
        let t = crack_tone(&pm);
        let kept = t.iter().filter(|&&v| (v - 90.0).abs() <= 12.0).count();
        assert!(kept > t.len() / 2, "only {kept} of {} pixels kept their tone", t.len());
    }
    /// Every square is one colour, the average of the picture under it
    /// lifted by [`PATCH_LIFT`]: at Relief 0 there is no light, and a
    /// square's pixels all match. Squares are twice Square Size and one
    /// pixels across.
    #[test]
    fn patchwork_fills_each_square_with_its_average() {
        let mut pm = Pixmap::new(27, 27);
        for y in 0..27 {
            for x in 0..27 {
                let v = if (x + y) % 2 == 0 { 40 } else { 200 };
                pm.set(x, y, Rgba8::new(v, v, v, 255));
            }
        }
        patchwork(&mut pm, 4, 0);
        for y in 0..9 {
            for x in 0..9 {
                assert_eq!(pm.get(x, y), pm.get(0, 0));
            }
        }
        // 41 of one and 40 of the other.
        let mean = (41.0 * 40.0 + 40.0 * 200.0) / 81.0 / 255.0;
        let lifted = 255.0 * f32::powf(mean, PATCH_LIFT);
        assert!((pm.get(0, 0).r as f32 - lifted).abs() <= 3.0, "{:?} vs {lifted}", pm.get(0, 0));
        assert_ne!(pm.get(9, 0), pm.get(8, 0), "the next square began somewhere else");
    }

    /// Relief builds every square into a block: across a flat sheet the
    /// bottom edge of each square is shaded against its face, the more so
    /// the higher Relief is, and the joints between rows are much deeper than
    /// the ones along a row. At Relief 0 the sheet stays flat.
    #[test]
    fn patchwork_relief_rims_every_square() {
        let run = |relief| {
            let mut pm = Pixmap::filled(54, 54, Rgba8::new(160, 160, 160, 255));
            patchwork(&mut pm, 4, relief);
            pm
        };
        // Squares are nine pixels: row 22 is mid-face, row 26 a bottom edge,
        // column 26 a right-hand edge.
        let band = |pm: &Pixmap| pm.get(22, 22).r as i32 - pm.get(22, 26).r as i32;
        let side = |pm: &Pixmap| pm.get(22, 22).r as i32 - pm.get(26, 22).r as i32;
        let (flat, eight, sixteen) = (run(0), run(8), run(16));
        assert_eq!(band(&flat), 0);
        assert!(band(&eight) > 25, "{}", band(&eight));
        assert!(band(&sixteen) > band(&eight), "{} vs {}", band(&sixteen), band(&eight));
        assert!(band(&eight) > 2 * side(&eight).abs(), "{} vs {}", band(&eight), side(&eight));
    }

    /// A bright square standing over a dark one throws a crisp shadow onto
    /// it, as CS6's pale sky does onto the horse.
    #[test]
    fn patchwork_casts_shadow_below_a_raised_square() {
        let mut pm = Pixmap::filled(18, 27, Rgba8::new(60, 60, 60, 255));
        pm.fill_rect(crate::buffer::Rect::new(0, 0, 18, 9), Rgba8::new(230, 230, 230, 255));
        let mut flat = Pixmap::filled(18, 27, Rgba8::new(60, 60, 60, 255));
        patchwork(&mut pm, 4, 8);
        patchwork(&mut flat, 4, 8);
        // Just inside the top of the square under the bright one: a real
        // shadow, not a shade darker.
        let (shadowed, open) = (pm.get(6, 10).r as f32, flat.get(6, 10).r as f32);
        assert!(shadowed < open * 0.7, "{shadowed} vs {open}");
    }

    #[test]
    fn patchwork_is_deterministic_and_leaves_alpha_alone() {
        let mut a = Pixmap::filled(47, 31, Rgba8::new(120, 140, 160, 77));
        let before = a.clone();
        let mut b = a.clone();
        patchwork(&mut a, 4, 8);
        patchwork(&mut b, 4, 8);
        assert_eq!(a.as_bytes(), b.as_bytes());
        let alpha = |pm: &Pixmap| pm.as_bytes().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>();
        assert_eq!(alpha(&a), alpha(&before));
        patchwork(&mut Pixmap::new(0, 0), 4, 8);
        patchwork(&mut Pixmap::filled(3, 3, Rgba8::WHITE), 0, 25);
    }
    /// The joint between two squares follows the step between them, as
    /// CS6's does: where a pale square stands over a dark one below it, the
    /// joint is shaded; where a dark one stands over a pale one, the pale
    /// square's top edge faces the light and the joint is lit.
    #[test]
    fn patchwork_joints_follow_the_step() {
        // The middle row of three: pale above dark, or dark above pale. Scored
        // as the band across the joint against the two faces.
        let joint = |upper: u8, lower: u8| {
            let mut pm = Pixmap::filled(27, 27, Rgba8::new(128, 128, 128, 255));
            pm.fill_rect(crate::buffer::Rect::new(0, 0, 27, 9), Rgba8::new(upper, upper, upper, 255));
            pm.fill_rect(crate::buffer::Rect::new(0, 9, 27, 9), Rgba8::new(lower, lower, lower, 255));
            patchwork(&mut pm, 4, 8);
            let band = (6..12).map(|y| pm.get(13, y).r as f32).sum::<f32>() / 6.0;
            let faces = (pm.get(13, 4).r as f32 + pm.get(13, 13).r as f32) / 2.0;
            band / faces
        };
        let (shaded, lit) = (joint(220, 90), joint(90, 220));
        assert!(lit > shaded + 0.15, "lit {lit} vs shaded {shaded}");
    }
}

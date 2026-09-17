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
}

impl Default for Finish {
    fn default() -> Finish {
        Finish { dark_bias: 0.0, bevel: BEVEL, crisp: 1.0, occlusion: 0.0, patchy: 0.0 }
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
                let shade = slope * relief * weight * patch;
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

/// Burlap: a coarse plain weave, 6 pixels to a thread, the threads wandering
/// and uneven. Each crossing has one thread over the other, alternating, and
/// the thread on top arches over it. Both directions matter: lit from the
/// side, it is the threads running down that show, and from below the ones
/// running across — a weave of one direction only turns into chevrons.
fn burlap(u: f32, v: f32) -> f32 {
    use std::f32::consts::PI;
    const PITCH: f32 = 6.0;
    const WANDER: f32 = 1.5;
    // Each thread wanders along its length.
    let u = u + (value_noise(u, v, 9.0, 21) - 0.5) * 2.0 * WANDER;
    let v = v + (value_noise(u, v, 9.0, 22) - 0.5) * 2.0 * WANDER;
    // Round threads both ways, and which one is on top rising and falling
    // smoothly from one crossing to the next. A hard switch at each crossing
    // cuts the cloth into puzzle pieces.
    let (a, b) = (u / PITCH * PI, v / PITCH * PI);
    let over = 0.5 * a.sin() * b.sin();
    let h = 0.5 + 0.3 * (b.sin().abs() - 0.5) + 0.2 * (a.sin().abs() - 0.5) + 0.3 * over;
    h * (0.7 + 0.3 * value_noise(u, v, 3.0, 7))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Rgba8;

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
}

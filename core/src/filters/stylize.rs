//! Filter ▸ Stylize.
//!
//! These do to a picture what a printmaker's technique does: they keep its
//! shapes and throw away its smoothness. Diffuse, Emboss, Extrude, Find Edges,
//! Solarize, Tiles, Trace Contour and Wind are built — the whole of CS6's
//! submenu — and Glowing Edges, which CS6 keeps in the Filter Gallery.

use crate::buffer::{Pixmap, Rgba8};
use rayon::prelude::*;

/// CS6's four Diffuse modes, in the order its dialog lists them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DiffuseMode {
    /// Every pixel takes a neighbour's colour — the picture is shuffled
    /// about within a pixel of where it was.
    #[default]
    Normal,
    /// The same, but a pixel only gives way to a neighbour darker than
    /// itself, so dark detail creeps outwards and light detail is eaten.
    DarkenOnly,
    /// And the other way about.
    LightenOnly,
    /// Not a shuffle at all: smoothing that runs along an edge and not
    /// across it, which is what leaves the smeared, painted look.
    Anisotropic,
}

impl DiffuseMode {
    pub fn from_i32(value: i32) -> DiffuseMode {
        match value {
            1 => DiffuseMode::DarkenOnly,
            2 => DiffuseMode::LightenOnly,
            3 => DiffuseMode::Anisotropic,
            _ => DiffuseMode::Normal,
        }
    }
}

/// How many passes Anisotropic makes, and so how far a pixel there reaches:
/// each pass reads one step out, and the next pass reads what that left.
/// [`crate::filters::Filter::reach`] has to agree with this, or a preview
/// cropped to a region comes out wrong along its edges.
pub const ANISOTROPIC_REACH: u32 = 4;

/// The eight neighbours of a pixel, in no order that matters.
const AROUND: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

/// Filter ▸ Stylize ▸ Diffuse: shuffle each pixel with one of its neighbours.
///
/// The randomness is drawn from the pixel's own coordinates rather than from a
/// generator, so that a preview, the commit behind it and an undo/redo replay
/// all produce the same picture. CS6 re-rolls on every apply, which is the one
/// difference — but running it twice here still goes on diffusing, because the
/// second pass reads what the first one left.
pub fn diffuse(pixmap: &mut Pixmap, mode: DiffuseMode) {
    if pixmap.is_empty() {
        return;
    }
    if mode == DiffuseMode::Anisotropic {
        // Diffusion is iterative. One pass over the neighbours barely moves a
        // picture — on fine grain it only halves it, because half the
        // neighbours of a speckle are the same speckle — and the smeared look
        // CS6 gives is several steps of it.
        for _ in 0..ANISOTROPIC_REACH {
            let source = pixmap.clone();
            let (width, height) = (source.width() as i32, source.height() as i32);
            let stride = pixmap.stride();
            pixmap
                .as_bytes_mut()
                .par_chunks_exact_mut(stride)
                .enumerate()
                .for_each(|(row, out)| {
                    for x in 0..width {
                        let colour = along_the_edge(&source, x, row as i32, width, height);
                        let i = x as usize * 4;
                        out[i] = colour.r;
                        out[i + 1] = colour.g;
                        out[i + 2] = colour.b;
                        out[i + 3] = colour.a;
                    }
                });
        }
        return;
    }

    // Every output pixel reads its neighbours as they *were*: writing in
    // place would let a pixel already shuffled this pass be shuffled again by
    // the one next to it, which smears the result along each row.
    let source = pixmap.clone();
    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;

    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            for x in 0..width {
                let here = source.get(x, y);
                // Clamped rather than wrapped or left transparent: a
                // neighbour off the edge of the layer would otherwise punch
                // holes along it.
                let (dx, dy) = AROUND[(hash(x as u32, y as u32) % 8) as usize];
                let there =
                    source.get((x + dx).clamp(0, width - 1), (y + dy).clamp(0, height - 1));
                let colour = match mode {
                    DiffuseMode::Normal => there,
                    // Compared by brightness and then taken whole, rather
                    // than channel by channel: the darker of two colours per
                    // channel is a third colour that was never in the
                    // picture, and it shows as a shift in hue along every
                    // edge.
                    DiffuseMode::DarkenOnly => {
                        if luma(there) < luma(here) { there } else { here }
                    }
                    DiffuseMode::LightenOnly => {
                        if luma(there) > luma(here) { there } else { here }
                    }
                    // Handled above, in its own iterated pass.
                    DiffuseMode::Anisotropic => here,
                };
                let i = x as usize * 4;
                out[i] = colour.r;
                out[i + 1] = colour.g;
                out[i + 2] = colour.b;
                out[i + 3] = colour.a;
            }
        });
}

/// One step of diffusion that flows along an edge rather than across it.
///
/// Each neighbour is weighted by how much it differs from this pixel, so
/// neighbours on the same side of an edge are averaged in and ones on the far
/// side are barely counted. That is the whole difference between this and a
/// blur: a blur would take the edge with it.
fn along_the_edge(source: &Pixmap, x: i32, y: i32, width: i32, height: i32) -> Rgba8 {
    let here = source.get(x, y);
    // How big a colour difference counts as an edge, in levels. Small enough
    // that a real edge survives, large enough that the grain within a flat
    // region is smoothed away.
    const EDGE: f32 = 28.0;

    let mut total = 1.0f32;
    let mut sum = (
        here.r as f32,
        here.g as f32,
        here.b as f32,
        here.a as f32,
    );
    for (dx, dy) in AROUND {
        let there = source.get((x + dx).clamp(0, width - 1), (y + dy).clamp(0, height - 1));
        let difference = ((there.r as f32 - here.r as f32).powi(2)
            + (there.g as f32 - here.g as f32).powi(2)
            + (there.b as f32 - here.b as f32).powi(2))
        .sqrt();
        let weight = (-(difference / EDGE).powi(2)).exp();
        sum.0 += there.r as f32 * weight;
        sum.1 += there.g as f32 * weight;
        sum.2 += there.b as f32 * weight;
        sum.3 += there.a as f32 * weight;
        total += weight;
    }
    Rgba8::new(
        (sum.0 / total).round().clamp(0.0, 255.0) as u8,
        (sum.1 / total).round().clamp(0.0, 255.0) as u8,
        (sum.2 / total).round().clamp(0.0, 255.0) as u8,
        (sum.3 / total).round().clamp(0.0, 255.0) as u8,
    )
}

/// Filter ▸ Stylize ▸ Emboss: the picture as though stamped into metal.
///
/// Flat grey everywhere the picture was flat, with a light and a dark edge
/// wherever it changed — which is the *difference* between what is on one
/// side of a pixel and what is on the other, measured along the angle the
/// light comes from. `angle` is where that light is, in degrees; `height` is
/// how far apart the two sides are, in pixels, which is how thick the relief
/// looks; `amount` (CS6's 1–500%) is how hard it is pressed.
///
/// Differenced channel by channel rather than on brightness, which is what
/// leaves the coloured fringes along an edge between two colours of the same
/// tone. CS6 does the same, and an emboss that worked on brightness alone
/// would come back a flat grey relief with the colour thrown away.
pub fn emboss(pixmap: &mut Pixmap, angle: f32, height: f32, amount: f32) {
    if pixmap.is_empty() {
        return;
    }
    let height = height.clamp(1.0, 100.0);
    let strength = amount.clamp(1.0, 500.0) / 100.0;
    let radians = angle.to_radians();
    // The house convention, shared with Motion Blur: y runs down the picture,
    // so the sine is negated and an angle reads the way it does on the dial.
    let (dx, dy) = (radians.cos(), -radians.sin());

    // Height is the thickness of the relief, not just how far apart the two
    // samples are. Reading a sharp edge at two points a long way apart would
    // give two thin lines with nothing between them — a doubled ghost rather
    // than a bevel — so the picture is softened first, by half the distance
    // the samples are about to span.
    let mut source = pixmap.clone();
    let blur = ((height - 1.0) / 2.0).round() as u32;
    if blur > 0 {
        super::convolve::box_blur(&mut source, blur);
    }
    let reach = height / 2.0;

    let width = pixmap.width() as i32;
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as f32 + 0.5;
            for x in 0..width {
                let at = x as f32 + 0.5;
                // Brightness is read as height, so the two samples are the
                // ground on either side of this pixel along the light's line.
                let toward = crate::resample::bilinear(&source, at + dx * reach - 0.5,
                                                       y + dy * reach - 0.5);
                let away = crate::resample::bilinear(&source, at - dx * reach - 0.5,
                                                     y - dy * reach - 0.5);
                let i = x as usize * 4;
                for (channel, (near, far)) in
                    [(toward.r, away.r), (toward.g, away.g), (toward.b, away.b)]
                        .into_iter()
                        .enumerate()
                {
                    // Ground that *falls away* towards the lamp is tilted to
                    // face it, and so is the lit side — which is why the far
                    // sample is the positive one. Get this the other way
                    // about and the whole picture reads as stamped in from
                    // the back, lit from the opposite corner to the one the
                    // angle names.
                    //
                    // Mid grey where nothing changes at all, which is what
                    // makes an embossed picture read as unlit metal rather
                    // than as a darkened photograph.
                    let relief = 128.0 + (far as f32 - near as f32) * strength;
                    out[i + channel] = relief.clamp(0.0, 255.0) as u8;
                }
                // Alpha stands: stamping a layer does not change its shape.
            }
        });
}

/// Filter ▸ Stylize ▸ Find Edges: draw the picture's edges as dark lines on
/// white.
///
/// Every channel is run through a Sobel gradient and the result inverted, so
/// flat ground — where nothing changes and the gradient is zero — comes back
/// white, and a step between two tones comes back as a dark line. The gradient
/// is *not* normalised: it is the raw Sobel, clamped at 255, which is what
/// makes the lines bold — a step of even a quarter of the range goes to black
/// and the picture's own texture comes through as grey. Dividing it down to a
/// full-contrast step instead leaves a photograph almost white.
///
/// Working channel by channel rather than on brightness is what leaves the
/// coloured fringes along an edge between two colours: only the channels that
/// actually change darken, so the line takes the colour of the darker side. On
/// brightness alone the same edge would come back grey.
///
/// There is no GPU path. It is a 3×3 neighbourhood op and so would fit the
/// shader shape, but like the rest of the Blur and Stylize families it uploads
/// its input and reads the result straight back, which the measurements in
/// docs/gpu-migration.md say rarely pays for the trip.
pub fn find_edges(pixmap: &mut Pixmap) {
    if pixmap.is_empty() {
        return;
    }
    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;

    // Premultiplied, as the rest of the convolution family is. On a layer with
    // soft edges, straight-alpha neighbours would otherwise read the hidden
    // colour of transparent pixels as a step and draw a halo round the
    // subject. The result is an intensity rather than an average, so it is
    // written back straight; the alpha it came with stands.
    let mut source = pixmap.clone();
    source.premultiply();
    let source = &source;

    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            for x in 0..width {
                // Clamp-to-edge, so the border does not read as a cliff.
                let sample = |dx: i32, dy: i32| {
                    source.get((x + dx).clamp(0, width - 1), (y + dy).clamp(0, height - 1))
                };
                let (tl, t, tr) = (sample(-1, -1), sample(0, -1), sample(1, -1));
                let (l, r) = (sample(-1, 0), sample(1, 0));
                let (bl, b, br) = (sample(-1, 1), sample(0, 1), sample(1, 1));

                let i = x as usize * 4;
                for c in 0..3 {
                    let pick = |p: Rgba8| match c {
                        0 => p.r as f32,
                        1 => p.g as f32,
                        _ => p.b as f32,
                    };
                    let gx =
                        -pick(tl) - 2.0 * pick(l) - pick(bl) + pick(tr) + 2.0 * pick(r) + pick(br);
                    let gy =
                        -pick(tl) - 2.0 * pick(t) - pick(tr) + pick(bl) + 2.0 * pick(b) + pick(br);
                    // The raw Sobel, clamped, then inverted: full contrast
                    // lands on black, nothing changing on white.
                    let magnitude = (gx * gx + gy * gy).sqrt().min(255.0);
                    out[i + c] = (255.0 - magnitude) as u8;
                }
                // Alpha stands: finding edges does not change the layer's shape.
            }
        });
}

/// CS6's two Extrude shapes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ExtrudeType {
    /// Square towers, each carrying its own piece of the picture on its face.
    #[default]
    Blocks,
    /// Four-sided spikes standing on the same grid, shaded and with no face
    /// to carry anything — which is why CS6 greys Solid Front Faces out for
    /// them.
    Pyramids,
}

impl ExtrudeType {
    pub fn from_i32(value: i32) -> ExtrudeType {
        match value {
            1 => ExtrudeType::Pyramids,
            _ => ExtrudeType::Blocks,
        }
    }
}

/// Everything CS6's Extrude dialog collects.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ExtrudeOptions {
    pub kind: ExtrudeType,
    /// The grid's square, in pixels (CS6's 2–255).
    pub size: u32,
    /// How far the towers stand (1–255), and whether that comes from the
    /// tile's own brightness rather than from a roll of the dice.
    pub depth: f32,
    pub level_based: bool,
    /// Face the blocks with one flat colour instead of their piece of the
    /// picture. Read only for blocks.
    pub solid_front: bool,
    /// Leave out the part-tiles along the right and bottom edges, where the
    /// grid does not divide the picture evenly.
    pub mask_incomplete: bool,
}

impl Default for ExtrudeOptions {
    fn default() -> Self {
        Self {
            kind: ExtrudeType::Blocks,
            size: 30,
            depth: 30.0,
            level_based: false,
            solid_front: false,
            mask_incomplete: false,
        }
    }
}

/// How far the furthest tower is thrown outwards, as a fraction of its
/// distance from the middle of the frame, at full Depth.
const THROW: f32 = 0.85;

/// How each face is lit, in the order [top, right, bottom, left]. A lamp up
/// and to the left, which is where CS6's is.
const FACES: [f32; 4] = [1.30, 0.78, 0.60, 1.12];

/// Filter ▸ Stylize ▸ Extrude: break the picture into towers standing out of
/// the frame.
///
/// The whole thing is one perspective from a viewer over the middle of the
/// picture: a tile pushed towards them moves *away from the centre* and grows,
/// which is what makes the towers lean outwards and why the ones in the middle
/// stand square on. Everything is drawn back to front, so a nearer tower hides
/// what is behind it — the cheapest way to get that right, and the reason this
/// is the one filter here that cannot run a row at a time across threads.
///
/// The towers are drawn *over* the picture rather than onto an empty frame, so
/// what shows between them is the original — as CS6 leaves it.
pub fn extrude(pixmap: &mut Pixmap, opt: ExtrudeOptions) {
    if pixmap.is_empty() {
        return;
    }
    let size = opt.size.clamp(2, 255);
    let (width, height) = (pixmap.width(), pixmap.height());
    let source = pixmap.clone();
    let middle = (width as f32 / 2.0, height as f32 / 2.0);
    let throw = opt.depth.clamp(1.0, 255.0) / 255.0 * THROW;

    // Every tile, with how far it stands. Sorted before anything is drawn:
    // painting them in grid order would let a tile at the back cover one in
    // front of it purely because it came later in the picture.
    let mut towers: Vec<(f32, crate::buffer::Rect, Rgba8)> = Vec::new();
    let mut y = 0u32;
    while y < height {
        let mut x = 0u32;
        while x < width {
            let tile = crate::buffer::Rect::new(
                x as i32,
                y as i32,
                size.min(width - x),
                size.min(height - y),
            );
            let whole = tile.width == size && tile.height == size;
            if whole || !opt.mask_incomplete {
                let average = average_of(&source, tile);
                let stands = if opt.level_based {
                    // Bright tiles stand tallest, which is what makes a
                    // level-based extrude read as a relief of the picture.
                    luma(average) / 255.0
                } else {
                    hash(x, y) as f32 / u32::MAX as f32
                };
                towers.push((stands, tile, average));
            }
            x += size;
        }
        y += size;
    }
    towers.sort_by(|a, b| a.0.total_cmp(&b.0));

    for (stands, tile, average) in towers {
        let grow = 1.0 + throw * stands;
        let out = |px: f32, py: f32| {
            (
                middle.0 + (px - middle.0) * grow,
                middle.1 + (py - middle.1) * grow,
            )
        };
        let (x0, y0) = (tile.x as f32, tile.y as f32);
        let (x1, y1) = (x0 + tile.width as f32, y0 + tile.height as f32);
        let base = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
        let front = base.map(|(px, py)| out(px, py));

        match opt.kind {
            ExtrudeType::Blocks => {
                // The four walls, then the face over them. A wall on the far
                // side of a tower lands inside its own front face and is
                // painted over by it, which is back-face culling for free.
                for edge in 0..4 {
                    let next = (edge + 1) % 4;
                    fill_convex(
                        pixmap,
                        &[base[edge], base[next], front[next], front[edge]],
                        shade(average, FACES[edge]),
                    );
                }
                if opt.solid_front {
                    fill_convex(pixmap, &front, average);
                } else {
                    face_of_picture(pixmap, &source, tile, front[0], front[2]);
                }
            }
            ExtrudeType::Pyramids => {
                // A spike from the tile's own square up to a point over its
                // middle. With no face to carry the picture, all that tells
                // them apart is the shading — which is why a pyramid in the
                // dead centre of the frame, thrown nowhere at all, still
                // reads as a pyramid.
                let apex = out((x0 + x1) / 2.0, (y0 + y1) / 2.0);
                for edge in 0..4 {
                    let next = (edge + 1) % 4;
                    fill_convex(
                        pixmap,
                        &[base[edge], base[next], apex],
                        shade(average, FACES[edge]),
                    );
                }
            }
        }
    }
}

/// The mean colour of a tile, which is what its walls are painted in.
fn average_of(source: &Pixmap, tile: crate::buffer::Rect) -> Rgba8 {
    let mut sum = [0u64; 4];
    let mut count = 0u64;
    for y in tile.y..tile.y + tile.height as i32 {
        for x in tile.x..tile.x + tile.width as i32 {
            let p = source.get(x, y);
            sum[0] += p.r as u64;
            sum[1] += p.g as u64;
            sum[2] += p.b as u64;
            sum[3] += p.a as u64;
            count += 1;
        }
    }
    if count == 0 {
        return Rgba8::TRANSPARENT;
    }
    Rgba8::new(
        (sum[0] / count) as u8,
        (sum[1] / count) as u8,
        (sum[2] / count) as u8,
        (sum[3] / count) as u8,
    )
}

/// Lighten or darken a wall, keeping its alpha.
fn shade(c: Rgba8, by: f32) -> Rgba8 {
    let level = |v: u8| (v as f32 * by).clamp(0.0, 255.0) as u8;
    Rgba8::new(level(c.r), level(c.g), level(c.b), c.a)
}

/// Paint a block's face with its own piece of the picture, stretched to the
/// size the perspective made of it.
fn face_of_picture(
    pixmap: &mut Pixmap,
    source: &Pixmap,
    tile: crate::buffer::Rect,
    top_left: (f32, f32),
    bottom_right: (f32, f32),
) {
    let (fx0, fy0) = top_left;
    let (fx1, fy1) = bottom_right;
    let (span_x, span_y) = (fx1 - fx0, fy1 - fy0);
    if span_x <= 0.0 || span_y <= 0.0 {
        return;
    }
    let from = (fy0.floor().max(0.0) as i32).max(0);
    let to = (fy1.ceil() as i32).min(pixmap.height() as i32);
    let left = (fx0.floor().max(0.0) as i32).max(0);
    let right = (fx1.ceil() as i32).min(pixmap.width() as i32);
    for y in from..to {
        let v = (y as f32 + 0.5 - fy0) / span_y;
        if !(0.0..1.0).contains(&v) {
            continue;
        }
        for x in left..right {
            let u = (x as f32 + 0.5 - fx0) / span_x;
            if !(0.0..1.0).contains(&u) {
                continue;
            }
            let sample = crate::resample::bilinear(
                source,
                tile.x as f32 + u * tile.width as f32 - 0.5,
                tile.y as f32 + v * tile.height as f32 - 0.5,
            );
            pixmap.set(x, y, sample);
        }
    }
}

/// Fill a convex polygon — a wall, or one side of a spike.
///
/// Convex, so a row crosses the outline exactly twice and the span between
/// those two crossings is the inside. A general polygon filler would need to
/// sort the crossings and pair them off; nothing here is ever concave.
fn fill_convex(pixmap: &mut Pixmap, points: &[(f32, f32)], colour: Rgba8) {
    if points.len() < 3 {
        return;
    }
    let top = points.iter().fold(f32::MAX, |a, p| a.min(p.1));
    let bottom = points.iter().fold(f32::MIN, |a, p| a.max(p.1));
    let first = (top.floor() as i32).max(0);
    let last = (bottom.ceil() as i32).min(pixmap.height() as i32);

    for y in first..last {
        // Rows are sampled down the middle, as everything else here is.
        let scan = y as f32 + 0.5;
        let (mut left, mut right) = (f32::MAX, f32::MIN);
        for i in 0..points.len() {
            let (ax, ay) = points[i];
            let (bx, by) = points[(i + 1) % points.len()];
            // A horizontal edge crosses nothing; the two edges either side of
            // it answer for its row.
            if (ay <= scan) == (by <= scan) {
                continue;
            }
            let t = (scan - ay) / (by - ay);
            let x = ax + (bx - ax) * t;
            left = left.min(x);
            right = right.max(x);
        }
        if left > right {
            continue;
        }
        let from = (left.round() as i32).max(0);
        let to = (right.round() as i32).min(pixmap.width() as i32);
        for x in from..to {
            pixmap.set(x, y, colour);
        }
    }
}

/// Filter ▸ Stylize ▸ Solarize: blend the picture with its own negative.
///
/// The darkroom trick it is named after is a print exposed to light part way
/// through developing, which sends the brightest tones back down towards dark.
/// As a curve that is a triangle: black stays black, mid grey goes to white,
/// white comes back to black — `255 - |2v - 255|`, which is exactly Curves
/// with a ∧ through it, and is what CS6 applies. Takes no parameters, as in
/// CS6, which runs it straight off the menu with no dialog.
///
/// Channel by channel on straight alpha, not on brightness: a colour whose
/// channels sit either side of mid grey comes back with them swapped in
/// relative order, which is where the mauves and cyans in a solarized
/// photograph come from. On brightness alone it would only ever be a
/// contrast curve.
///
/// No GPU path. It is per-pixel and so exactly the shader shape, but a table
/// of 256 entries applied to bytes is memory-bound on the CPU too — the trip
/// to the device and straight back would cost more than the arithmetic it
/// saves (docs/gpu-migration.md).
pub fn solarize(pixmap: &mut Pixmap) {
    if pixmap.is_empty() {
        return;
    }
    // The curve is the same for every pixel and there are only 256 inputs, so
    // it is worth a table rather than the arithmetic a few million times.
    let mut curve = [0u8; 256];
    for (v, out) in curve.iter_mut().enumerate() {
        *out = (255 - (2 * v as i32 - 255).abs()) as u8;
    }

    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(4)
        .for_each(|pixel| {
            pixel[0] = curve[pixel[0] as usize];
            pixel[1] = curve[pixel[1] as usize];
            pixel[2] = curve[pixel[2] as usize];
            // Alpha stands: solarizing changes colour, not shape.
        });
}

/// What CS6's Tiles puts in the gaps the shifted tiles leave behind, in the
/// order its "Fill Empty Area With" box lists them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TileFill {
    #[default]
    BackgroundColor,
    ForegroundColor,
    /// The picture itself, inverted — so the gaps read as a negative of what
    /// the tile that moved away was showing.
    InverseImage,
    /// The picture itself, untouched, which leaves only the tiles that moved
    /// far enough to double an edge visible.
    UnalteredImage,
}

impl TileFill {
    pub fn from_i32(value: i32) -> TileFill {
        match value {
            1 => TileFill::ForegroundColor,
            2 => TileFill::InverseImage,
            3 => TileFill::UnalteredImage,
            _ => TileFill::BackgroundColor,
        }
    }
}

/// Everything CS6's Tiles dialog collects, plus the two swatch colours, which
/// it does not ask for because they belong to the document.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TileOptions {
    /// CS6's "Number Of Tiles", 1–99. It counts tiles across the *shorter*
    /// side, so the tiles come out square and 10 on a portrait photograph
    /// means ten columns rather than ten rows.
    pub count: u32,
    /// CS6's "Maximum Offset", 1–99, a percentage of the tile's own size —
    /// so the same figure shifts a big tile further than a small one.
    pub offset: u32,
    pub fill: TileFill,
    /// Filled in by the bridge from the document's swatches.
    pub foreground: Rgba8,
    pub background: Rgba8,
}

impl Default for TileOptions {
    fn default() -> Self {
        Self {
            count: 10,
            offset: 10,
            fill: TileFill::BackgroundColor,
            foreground: Rgba8::BLACK,
            background: Rgba8::WHITE,
        }
    }
}

/// Filter ▸ Stylize ▸ Tiles: cut the picture into squares and nudge each one
/// off where it was.
///
/// The ground is laid first — whichever of the four fills was asked for — and
/// then every tile is dropped onto it at its own small offset, so what shows
/// through is the gap each tile left behind. Tiles never overlap by more than
/// they are offset, and nothing is scaled: a tile is a straight copy of its
/// square, moved.
///
/// The offsets are drawn from the tile's position in the grid rather than from
/// a RNG, so re-running the filter during an undo/redo replay lands every tile
/// exactly where it was. That is also why [`crate::filters::Filter::reach`]
/// says `None` for it: the grid is laid on the layer's own corner, so a crop
/// would start the grid somewhere else and its tiles would not line up with
/// the ones either side.
///
/// No GPU path. Each tile is a block copy — memory movement, not arithmetic —
/// and a shader would still have to upload the picture and read it back
/// (docs/gpu-migration.md).
pub fn tiles(pixmap: &mut Pixmap, opt: TileOptions) {
    if pixmap.is_empty() {
        return;
    }
    let (width, height) = (pixmap.width(), pixmap.height());
    let count = opt.count.clamp(1, 99);
    // Square tiles, counted across the shorter side. At least one pixel, or a
    // tall thin selection with 99 tiles asked of it would divide to nothing.
    let size = (width.min(height) / count).max(1);
    // CS6's percentage is of the tile, and at least a pixel once it is asked
    // for at all — otherwise 1% of a small tile rounds to no movement and the
    // filter appears to do nothing.
    let reach = ((size as f32 * opt.offset.clamp(0, 99) as f32 / 100.0).round() as i32).max(1);

    let source = pixmap.clone();

    // The ground the tiles land on.
    match opt.fill {
        TileFill::BackgroundColor => pixmap.fill(opt.background),
        TileFill::ForegroundColor => pixmap.fill(opt.foreground),
        TileFill::UnalteredImage => {}
        TileFill::InverseImage => {
            for px in pixmap.as_bytes_mut().chunks_exact_mut(4) {
                px[0] = 255 - px[0];
                px[1] = 255 - px[1];
                px[2] = 255 - px[2];
                // Alpha stands: inverting is a colour, not a shape.
            }
        }
    }

    let columns = width.div_ceil(size);
    let rows = height.div_ceil(size);
    for row in 0..rows {
        for column in 0..columns {
            // Two offsets from one hash, taken from opposite ends of it so
            // that a tile's horizontal and vertical shifts are independent.
            let h = hash(column, row);
            let span = (reach * 2 + 1) as u32;
            let dx = (h % span) as i32 - reach;
            let dy = ((h >> 16) % span) as i32 - reach;

            let (left, top) = ((column * size) as i32, (row * size) as i32);
            for y in top..(top + size as i32).min(height as i32) {
                for x in left..(left + size as i32).min(width as i32) {
                    let (tx, ty) = (x + dx, y + dy);
                    if tx >= 0 && ty >= 0 && tx < width as i32 && ty < height as i32 {
                        pixmap.set(tx, ty, source.get(x, y));
                    }
                }
            }
        }
    }
}

/// Which side of the threshold CS6's Trace Contour draws its line on.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ContourEdge {
    /// Outline the pixels *below* the level — the line runs along the dark
    /// side of the boundary.
    Lower,
    /// Outline the pixels at or above it, which is the side CS6 starts on.
    #[default]
    Upper,
}

impl ContourEdge {
    pub fn from_i32(value: i32) -> ContourEdge {
        match value {
            0 => ContourEdge::Lower,
            _ => ContourEdge::Upper,
        }
    }
}

/// Filter ▸ Stylize ▸ Trace Contour: draw the line where each channel crosses
/// a brightness.
///
/// It is a contour map of the picture: pick a level, and wherever a channel
/// steps across it, mark the pixel on the side Edge names. Everything else
/// goes white.
///
/// The three channels are traced *separately* and marked to black on their
/// own, which is where the colours in the result come from: a boundary only
/// the red channel crosses leaves red at 0 and the other two at 255, so the
/// line is cyan. Black lines are where all three cross together. Tracing
/// brightness instead would give a single black line and lose the whole
/// character of the filter.
///
/// The frame's own edge is sampled clamped, so the border does not read as a
/// crossing and get outlined all the way round.
///
/// No GPU path. It is a 3×3 neighbourhood test, so it would fit the shader
/// shape, but like the rest of this family it would upload its input and read
/// the result straight back (docs/gpu-migration.md).
pub fn trace_contour(pixmap: &mut Pixmap, level: u8, edge: ContourEdge) {
    if pixmap.is_empty() {
        return;
    }
    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;
    let source = pixmap.clone();
    let source = &source;
    let level = level as i32;

    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            for x in 0..width {
                let sample = |dx: i32, dy: i32| {
                    source.get((x + dx).clamp(0, width - 1), (y + dy).clamp(0, height - 1))
                };
                let here = sample(0, 0);
                // The four neighbours: a crossing is a step between two
                // pixels that share a side, so the diagonals would only
                // thicken the line without finding anything new.
                let around = [sample(-1, 0), sample(1, 0), sample(0, -1), sample(0, 1)];

                let i = x as usize * 4;
                for c in 0..3 {
                    let pick = |p: Rgba8| match c {
                        0 => p.r as i32,
                        1 => p.g as i32,
                        _ => p.b as i32,
                    };
                    // On the side Edge names, with a neighbour on the other
                    // side of the level — which is exactly "the boundary
                    // passes between us, and I am the one who gets inked".
                    let mine = pick(here);
                    let marked = match edge {
                        ContourEdge::Upper => {
                            mine >= level && around.iter().any(|&n| pick(n) < level)
                        }
                        ContourEdge::Lower => {
                            mine < level && around.iter().any(|&n| pick(n) >= level)
                        }
                    };
                    out[i + c] = if marked { 0 } else { 255 };
                }
                // Alpha stands: the contour is drawn on the layer's own shape.
            }
        });
}

/// CS6's three Wind methods, in the order its dialog lists them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum WindMethod {
    /// Fine streaks off the edges — the gentlest of the three.
    #[default]
    Wind,
    /// The same idea driven much harder: longer streaks, more of them, and
    /// barely fading, so the picture is torn sideways.
    Blast,
    /// Streaks that wander a row up or down as they run, which is what breaks
    /// the edges into the ragged, shuffled look.
    Stagger,
}

impl WindMethod {
    pub fn from_i32(value: i32) -> WindMethod {
        match value {
            1 => WindMethod::Blast,
            2 => WindMethod::Stagger,
            _ => WindMethod::Wind,
        }
    }

    /// How far a streak can run, how many edges get one (out of 100), and how
    /// much of the streak's strength is left at its far end.
    fn shape(self) -> (u32, u32, f32) {
        match self {
            WindMethod::Wind => (12, 55, 0.0),
            WindMethod::Blast => (32, 85, 0.55),
            WindMethod::Stagger => (16, 70, 0.25),
        }
    }
}

/// How different two neighbouring pixels have to be, in levels of brightness,
/// before the wind can catch the edge between them. Low enough that a
/// photograph streaks all over, high enough that film grain does not.
const WIND_EDGE: f32 = 10.0;

/// Filter ▸ Stylize ▸ Wind: blow the picture sideways off its edges.
///
/// Every row is worked on its own — the wind is horizontal, so nothing crosses
/// between rows — and within a row the filter looks for a step *down* in
/// brightness away from the wind. Where it finds one, the bright pixel at the
/// step is dragged downwind over a few pixels, fading as it goes, which is the
/// streak. Flat ground has no step to catch and so comes through untouched,
/// which is why the effect reads as edges torn sideways rather than as a blur.
///
/// Because only that one polarity of step counts, a shape streaks off the side
/// the wind blows it towards and the other side stays clean — which is what
/// CS6 does, and the thing that makes the Direction setting visible at all.
///
/// `from_right` is CS6's Direction: the wind *comes from* that side, so "From
/// the Right" drags the picture towards the left.
///
/// The three methods are the same machine at different settings — see
/// [`WindMethod::shape`] — except that Stagger also lets a streak wander a row
/// up or down as it runs. It reads its neighbouring rows but never writes to
/// them, so the rows still parallelise.
///
/// The streak lengths are drawn from each pixel's coordinates rather than from
/// a RNG, so a preview, the commit behind it and an undo/redo replay all blow
/// the same way. CS6 re-rolls on every apply; that is the one difference, and
/// the same one Diffuse already carries.
///
/// No GPU path. A streak writes over the pixels ahead of it and the next
/// streak writes over that, so the work is sequential along a row rather than
/// per-pixel — the shape a shader is worst at.
pub fn wind(pixmap: &mut Pixmap, method: WindMethod, from_right: bool) {
    if pixmap.is_empty() {
        return;
    }
    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;
    let source = pixmap.clone();
    let source = &source;
    // Which way the picture travels: away from where the wind comes from.
    let step = if from_right { -1 } else { 1 };
    let (longest, density, tail) = method.shape();

    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            for x in 0..width {
                let here = source.get(x, y);
                // The pixel the wind reached first. A step between the two is
                // an edge facing into it, and so something for it to catch:
                // what gets dragged is that upwind pixel, over this one and
                // the ones beyond. Dragging *this* pixel forward instead
                // would smear the far side of the edge over itself and show
                // nothing at all.
                let behind = (x - step).clamp(0, width - 1);
                let upwind = source.get(behind, y);
                // Only where the upwind pixel is the *brighter* of the two.
                // That is what keeps the streaks to one side of a shape: a
                // bright subject on a dark ground is caught where it gives
                // way to the ground and blown out over it, while its other
                // side — where the ground gives way to the subject — is left
                // alone. Catching both would streak every shape from both
                // sides at once, which is not what the wind does.
                if luma(upwind) - luma(here) < WIND_EDGE {
                    continue;
                }

                let h = hash(x as u32, y as u32);
                if h % 100 >= density {
                    continue;
                }
                let length = 1 + (h >> 8) % longest;

                for i in 0..length as i32 {
                    let tx = x + step * i;
                    if tx < 0 || tx >= width {
                        break;
                    }
                    // Stagger's wander: the streak reads a row above or below
                    // as it runs, so its far end no longer lines up with the
                    // edge it came from.
                    let sy = match method {
                        WindMethod::Stagger => {
                            let drift = (hash(x as u32, (y + i) as u32) % 3) as i32 - 1;
                            (y + drift).clamp(0, height - 1)
                        }
                        _ => y,
                    };
                    let colour = source.get(behind, sy);

                    // Full strength at the edge, falling to `tail` at the far
                    // end — a streak that stopped dead would read as a bar.
                    let t = i as f32 / length as f32;
                    let weight = 1.0 - t * (1.0 - tail);
                    let i = tx as usize * 4;
                    for (c, value) in [colour.r, colour.g, colour.b].into_iter().enumerate() {
                        let was = out[i + c] as f32;
                        out[i + c] = (was + (value as f32 - was) * weight).round() as u8;
                    }
                    // Alpha stands: the wind moves colour about, and a streak
                    // that carried alpha with it would tear holes in the layer.
                }
            }
        });
}

/// CS6's ranges for Glowing Edges, which its three sliders run over.
pub const GLOW_WIDTH: std::ops::RangeInclusive<u32> = 1..=14;
pub const GLOW_BRIGHTNESS: std::ops::RangeInclusive<u32> = 0..=20;
pub const GLOW_SMOOTHNESS: std::ops::RangeInclusive<u32> = 1..=15;

/// How much Gaussian each step of Smoothness is worth, in pixels of radius.
const GLOW_SMOOTHING: f32 = 0.3;

/// How far a pixel of Glowing Edges reads: three sigma of the smoothing blur,
/// one more for the Sobel taken on top of it, and then however far the edge is
/// widened. Lives here rather than in [`crate::filters::Filter::reach`] so
/// that it cannot drift from the constants above it.
pub fn glow_reach(width: u32, smoothness: u32) -> u32 {
    let smoothness = smoothness.clamp(*GLOW_SMOOTHNESS.start(), *GLOW_SMOOTHNESS.end());
    let width = width.clamp(*GLOW_WIDTH.start(), *GLOW_WIDTH.end());
    (smoothness as f32 * GLOW_SMOOTHING * 3.0).ceil() as u32 + 1 + width / 2
}

/// Glowing Edges: the picture's edges lit up on a black ground.
///
/// It is Find Edges the other way about — the raw gradient kept rather than
/// inverted, so what changes glows and what is flat goes black — with the
/// three controls CS6 gives it:
///
/// * **Smoothness** blurs the picture before the gradient is taken, so grain
///   and fine texture stop registering as edges and only real boundaries
///   light up.
/// * **Edge Width** thickens the line afterwards, by letting each pixel take
///   the brightest gradient within half that width of it.
/// * **Edge Brightness** is the gain on the result, and the reason the lines
///   blow out to white at the top of its range.
///
/// The gradient is taken channel by channel, which is where the colour comes
/// from: a pink petal against dark ground steps furthest in red, so its outline
/// glows magenta, while a boundary all three channels cross comes back white.
/// On brightness alone every edge would be the same grey.
///
/// Note that CS6 keeps this one in the Filter Gallery rather than in the
/// Filter ▸ Stylize submenu. The Gallery is not built (docs/ROADMAP.md), so the
/// submenu is where it is reachable from.
///
/// No GPU path, for the reason the rest of this family has none: it would
/// upload its input and read the result straight back
/// (docs/gpu-migration.md). The blur inside it does go through the backend.
pub fn glowing_edges(pixmap: &mut Pixmap, width: u32, brightness: u32, smoothness: u32) {
    if pixmap.is_empty() {
        return;
    }
    let smoothness = smoothness.clamp(*GLOW_SMOOTHNESS.start(), *GLOW_SMOOTHNESS.end());
    let width = width.clamp(*GLOW_WIDTH.start(), *GLOW_WIDTH.end());
    let brightness = brightness.clamp(*GLOW_BRIGHTNESS.start(), *GLOW_BRIGHTNESS.end());

    // Smoothness first: the gradient is taken on the blurred copy, so what
    // the blur removed never becomes an edge in the first place.
    let mut smoothed = pixmap.clone();
    crate::filters::convolve::gaussian_blur_accelerated(
        &mut smoothed,
        smoothness as f32 * GLOW_SMOOTHING,
    );
    // Premultiplied, as the rest of the convolution family is: on a layer
    // with soft edges, straight-alpha neighbours would read the hidden colour
    // of transparent pixels as a step and ring the subject with a glow it
    // does not have.
    smoothed.premultiply();

    let mut edges = gradient(&smoothed);
    if width / 2 > 0 {
        edges = crate::filters::convolve::dilate(&edges, (width / 2) as i32);
    }

    // Edge Brightness is a plain gain, and CS6's 6 is about life-size.
    let gain = brightness as f32 / 5.0;
    let stride = pixmap.stride();
    let edges = &edges;
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            for x in 0..edges.width() as i32 {
                let lit = edges.get(x, y);
                let i = x as usize * 4;
                for (c, value) in [lit.r, lit.g, lit.b].into_iter().enumerate() {
                    out[i + c] = (value as f32 * gain).min(255.0) as u8;
                }
                // Alpha stands: lighting the edges does not change the
                // layer's shape.
            }
        });
}

/// The raw Sobel magnitude of each channel, as a picture in its own right.
///
/// Shared ground with [`find_edges`], which inverts this and draws it on
/// white; here it is what glows.
fn gradient(source: &Pixmap) -> Pixmap {
    let width = source.width() as i32;
    let height = source.height() as i32;
    let mut out = Pixmap::new(source.width(), source.height());

    let stride = out.stride();
    out.as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            for x in 0..width {
                // Clamp-to-edge, so the border does not read as a cliff and
                // light up all the way round the frame.
                let sample = |dx: i32, dy: i32| {
                    source.get((x + dx).clamp(0, width - 1), (y + dy).clamp(0, height - 1))
                };
                let (tl, t, tr) = (sample(-1, -1), sample(0, -1), sample(1, -1));
                let (l, r) = (sample(-1, 0), sample(1, 0));
                let (bl, b, br) = (sample(-1, 1), sample(0, 1), sample(1, 1));

                let i = x as usize * 4;
                for c in 0..3 {
                    let pick = |p: Rgba8| match c {
                        0 => p.r as f32,
                        1 => p.g as f32,
                        _ => p.b as f32,
                    };
                    let gx =
                        -pick(tl) - 2.0 * pick(l) - pick(bl) + pick(tr) + 2.0 * pick(r) + pick(br);
                    let gy =
                        -pick(tl) - 2.0 * pick(t) - pick(tr) + pick(bl) + 2.0 * pick(b) + pick(br);
                    out[i + c] = (gx * gx + gy * gy).sqrt().min(255.0) as u8;
                }
                out[i + 3] = 255;
            }
        });
    out
}

/// Perceived brightness, for the two modes that ask which of two colours is
/// the darker.
#[inline]
fn luma(c: Rgba8) -> f32 {
    0.299 * c.r as f32 + 0.587 * c.g as f32 + 0.114 * c.b as f32
}

/// Which neighbour a pixel reaches for. Seeded from where the pixel is, so
/// the answer is the same every time the filter runs over the same picture.
#[inline]
fn hash(x: u32, y: u32) -> u32 {
    let mut h = x.wrapping_mul(0x27d4_eb2d) ^ y.wrapping_mul(0x1656_67b1);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545_f491);
    h ^= h >> 13;
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture with an edge in it: light on the left, dark on the right.
    fn edged() -> Pixmap {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(220, 220, 220, 255));
        for y in 0..64 {
            for x in 32..64 {
                pm.set(x, y, Rgba8::new(40, 40, 40, 255));
            }
        }
        pm
    }

    #[test]
    fn diffusing_shuffles_pixels_without_inventing_colours() {
        // Normal takes a neighbour's colour whole, so on a two-tone picture
        // every pixel has to come back as one of the two tones. A filter that
        // averaged instead would leave a grey rim down the edge.
        let mut pm = edged();
        diffuse(&mut pm, DiffuseMode::Normal);
        for p in pm.as_bytes().chunks_exact(4) {
            assert!(p[0] == 220 || p[0] == 40, "{} is neither tone", p[0]);
        }
    }

    #[test]
    fn the_edge_is_what_moves() {
        let mut pm = edged();
        diffuse(&mut pm, DiffuseMode::Normal);
        // Ragged along the seam...
        let mut ragged = 0;
        for y in 0..64 {
            if pm.get(31, y).r != 220 || pm.get(32, y).r != 40 {
                ragged += 1;
            }
        }
        assert!(ragged > 8, "the seam came back straight: only {ragged} rows moved");
        // ...and untouched well away from it, where every neighbour is the
        // same colour anyway.
        for y in 0..64 {
            assert_eq!(pm.get(4, y).r, 220);
            assert_eq!(pm.get(60, y).r, 40);
        }
    }

    #[test]
    fn darken_only_never_lightens_a_pixel_and_lighten_only_never_darkens_one() {
        let before = edged();
        let mut darkened = before.clone();
        diffuse(&mut darkened, DiffuseMode::DarkenOnly);
        let mut lightened = before.clone();
        diffuse(&mut lightened, DiffuseMode::LightenOnly);

        for y in 0..64 {
            for x in 0..64 {
                assert!(darkened.get(x, y).r <= before.get(x, y).r, "Darken Only lightened one");
                assert!(lightened.get(x, y).r >= before.get(x, y).r, "Lighten Only darkened one");
            }
        }
        // And each one actually did something: the dark half grows one way,
        // the light half the other.
        assert_ne!(darkened.as_bytes(), before.as_bytes());
        assert_ne!(lightened.as_bytes(), before.as_bytes());
        assert_ne!(darkened.as_bytes(), lightened.as_bytes());
    }

    /// Anisotropic smooths *along* an edge, not across it — which is the one
    /// thing that separates it from a blur, and the thing that would go
    /// unnoticed if it were wrong, since both look soft.
    #[test]
    fn anisotropic_smooths_the_grain_but_keeps_the_edge()
    {
        // Grain on both sides of the same edge.
        let mut pm = edged();
        for y in 0..64 {
            for x in 0..64 {
                let base = pm.get(x, y).r as i32;
                let jitter = if (x + y) % 2 == 0 { 12 } else { -12 };
                let level = (base + jitter).clamp(0, 255) as u8;
                pm.set(x, y, Rgba8::new(level, level, level, 255));
            }
        }
        let before = pm.clone();
        diffuse(&mut pm, DiffuseMode::Anisotropic);

        // The grain in the middle of a flat region is gone...
        let roughness = |img: &Pixmap| {
            let mut sum = 0i32;
            for y in 10..54 {
                for x in 6..26 {
                    sum += (img.get(x, y).r as i32 - img.get(x + 1, y).r as i32).abs();
                }
            }
            sum
        };
        assert!(
            roughness(&pm) * 3 < roughness(&before),
            "the grain survived: {} before, {} after",
            roughness(&before),
            roughness(&pm)
        );

        // ...and the edge is still an edge, not a ramp.
        let step = (pm.get(31, 32).r as i32 - pm.get(32, 32).r as i32).abs();
        assert!(step > 120, "the edge was blurred away: a step of only {step}");
    }

    #[test]
    fn diffusing_is_the_same_every_time() {
        let run = || {
            let mut pm = edged();
            diffuse(&mut pm, DiffuseMode::Normal);
            pm
        };
        assert_eq!(run().as_bytes(), run().as_bytes());
    }

    #[test]
    fn diffusing_leaves_alpha_where_the_pixel_came_from() {
        // A pixel is moved whole, so a half-transparent one stays half
        // transparent wherever it lands — and the layer's edge is not eaten
        // by the transparency outside it.
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(100, 100, 100, 128));
        diffuse(&mut pm, DiffuseMode::Normal);
        assert!(pm.as_bytes().chunks_exact(4).all(|p| p[3] == 128));
    }

    // ------------------------------------------------------------- emboss --

    /// A pale disc on a darker ground, for the light to catch one side of.
    fn disc() -> Pixmap {
        let mut pm = Pixmap::filled(120, 120, Rgba8::new(90, 90, 90, 255));
        for y in 0..120 {
            for x in 0..120 {
                let (dx, dy) = ((x - 60) as f32, (y - 60) as f32);
                if dx * dx + dy * dy < 35.0 * 35.0 {
                    pm.set(x, y, Rgba8::new(200, 200, 200, 255));
                }
            }
        }
        pm
    }

    /// Everything that did not change comes back mid grey — inside the disc
    /// as well as outside it. An emboss that left the picture's own tones in
    /// place would not be an emboss; it would be a sharpen.
    #[test]
    fn embossing_leaves_flat_ground_grey() {
        let mut pm = disc();
        emboss(&mut pm, 135.0, 3.0, 100.0);
        for (x, y) in [(5, 5), (60, 60), (114, 114)] {
            let p = pm.get(x, y);
            assert_eq!((p.r, p.g, p.b), (128, 128, 128), "({x}, {y}) is not flat grey");
        }
    }

    /// The side the light names is the lit side. Getting this backwards reads
    /// as a picture stamped in from behind, and it is invisible in a test that
    /// only checks *that* the edges changed.
    #[test]
    fn the_light_falls_on_the_side_the_angle_names() {
        let mut pm = disc();
        emboss(&mut pm, 135.0, 3.0, 100.0);
        // 135° is up and to the left, so that rim catches it and the far one
        // is in shadow.
        assert!(pm.get(36, 36).r > 170, "the lit rim came back dark");
        assert!(pm.get(84, 84).r < 86, "the shadowed rim came back light");

        // Turned right around, the two swap over.
        let mut other = disc();
        emboss(&mut other, -45.0, 3.0, 100.0);
        assert!(other.get(36, 36).r < 86);
        assert!(other.get(84, 84).r > 170);
    }

    /// An edge running along the light's own line has no slope across it, so
    /// there is nothing for the light to catch — which is why an embossed
    /// picture loses whatever runs parallel to the angle.
    #[test]
    fn an_edge_along_the_light_disappears() {
        let mut pm = Pixmap::filled(80, 80, Rgba8::new(90, 90, 90, 255));
        for y in 0..80i32 {
            for x in 0..80i32 {
                if (x - y).abs() < 3 {
                    pm.set(x, y, Rgba8::new(220, 220, 220, 255));
                }
            }
        }
        let across = {
            let mut copy = pm.clone();
            emboss(&mut copy, 135.0, 3.0, 100.0);
            copy
        };
        emboss(&mut pm, 45.0, 3.0, 100.0);

        // Read on a line *across* the stripe rather than down the middle of
        // it: in the middle both samples land inside the stripe whichever way
        // the light comes from, and everything looks flat.
        let boldest = |img: &Pixmap| {
            (-8..=8)
                .map(|k| (img.get(40 + k, 40 - k).r as i32 - 128).abs())
                .max()
                .unwrap_or(0)
        };
        // 135° runs along the stripe, so there is no slope for it to catch.
        assert!(boldest(&across) < 12, "a stripe along the light still showed");
        assert!(boldest(&pm) > 60, "a stripe across the light did not show");
    }

    #[test]
    fn amount_presses_the_relief_harder() {
        let rim = |amount| {
            let mut pm = disc();
            emboss(&mut pm, 135.0, 3.0, amount);
            (pm.get(36, 36).r as i32 - 128).abs()
        };
        assert!(rim(300.0) > rim(100.0));
        assert!(rim(100.0) > rim(20.0));
    }

    /// Height is the thickness of the relief, so a taller one spreads further
    /// from the edge that made it.
    #[test]
    fn height_widens_the_band_the_relief_covers() {
        let width = |height| {
            let mut pm = disc();
            emboss(&mut pm, 135.0, height, 100.0);
            // How many pixels along a line through the rim are not flat grey.
            (0..60)
                .filter(|&i| (pm.get(i, i).r as i32 - 128).abs() > 4)
                .count()
        };
        assert!(width(9.0) > width(3.0), "a taller relief was no wider");
    }

    /// Differenced per channel, which is what puts the coloured fringes along
    /// an edge between two colours of the same brightness. Working on
    /// brightness alone would come back flat grey there — no edge at all.
    #[test]
    fn an_edge_between_two_colours_of_one_tone_still_shows() {
        let mut pm = Pixmap::filled(60, 60, Rgba8::new(180, 100, 100, 255));
        for y in 0..60 {
            for x in 30..60 {
                // Much the same luma, a long way apart in colour.
                pm.set(x, y, Rgba8::new(100, 140, 180, 255));
            }
        }
        emboss(&mut pm, 0.0, 3.0, 100.0);
        let seam = pm.get(30, 30);
        assert!(
            (seam.r as i32 - seam.b as i32).abs() > 40,
            "the colour edge came back grey: {seam:?}"
        );
    }

    #[test]
    fn embossing_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 120, 120, 77));
        emboss(&mut pm, 135.0, 3.0, 100.0);
        assert!(pm.as_bytes().chunks_exact(4).all(|p| p[3] == 77));
    }

    // --------------------------------------------------------- find edges --

    /// Flat ground has no gradient anywhere, so the whole picture comes back
    /// white — the ground Find Edges draws its lines on.
    #[test]
    fn finding_edges_leaves_flat_ground_white() {
        let mut pm = Pixmap::filled(48, 48, Rgba8::new(120, 120, 120, 255));
        find_edges(&mut pm);
        for y in 0..48 {
            for x in 0..48 {
                assert_eq!(pm.get(x, y), Rgba8::WHITE, "({x}, {y}) was not left white");
            }
        }
    }

    /// A step is an edge, and the line lands on it rather than a pixel to one
    /// side — which is what separates a Sobel line from a one-sided
    /// difference. The gradient is raw, so a step this strong saturates.
    #[test]
    fn finding_edges_draws_a_line_on_the_step() {
        let mut pm = edged();
        find_edges(&mut pm);
        // Dark down the seam...
        let seam = (pm.get(31, 20).r as i32).min(pm.get(32, 20).r as i32);
        assert!(seam < 20, "the seam came back light: {seam}");
        // ...and white well away from it, on both sides.
        for x in [4, 60] {
            assert_eq!(pm.get(x, 20), Rgba8::WHITE, "flat ground at x={x} was not white");
        }
    }

    /// The gradient is the raw Sobel rather than one normalised to a
    /// full-contrast step, so the picture's own texture comes through: a
    /// gentle step is a grey line, not white. Normalising it is the difference
    /// between a Find Edges that reads as a pencil sketch and one that leaves
    /// a photograph almost blank.
    #[test]
    fn finding_edges_shows_a_gentle_step() {
        let mut pm = Pixmap::filled(48, 48, Rgba8::new(200, 200, 200, 255));
        for y in 0..48 {
            for x in 24..48 {
                pm.set(x, y, Rgba8::new(192, 192, 192, 255));
            }
        }
        find_edges(&mut pm);
        // An eight-level step is a Sobel of 32: visible grey, not white.
        let seam = pm.get(24, 24).r;
        assert!(seam < 240, "a gentle step vanished: {seam}");
    }

    /// Differenced per channel, so a step in one channel alone comes back as
    /// that channel's colour rather than grey. A detector working on
    /// brightness would still draw a line here — red carries brightness too —
    /// so the line's colour is the whole point.
    #[test]
    fn finding_edges_keeps_the_colour_of_an_edge() {
        let mut pm = Pixmap::filled(60, 60, Rgba8::new(200, 60, 60, 255));
        for y in 0..60 {
            for x in 30..60 {
                // Only red steps; green and blue are flat throughout.
                pm.set(x, y, Rgba8::new(60, 60, 60, 255));
            }
        }
        find_edges(&mut pm);
        let seam = pm.get(30, 30);
        assert!(
            seam.g as i32 - seam.r as i32 > 80,
            "the red edge came back grey: {seam:?}"
        );
        assert_eq!(seam.g, seam.b, "green and blue should be untouched");
    }

    #[test]
    fn finding_edges_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 120, 120, 77));
        find_edges(&mut pm);
        assert!(pm.as_bytes().chunks_exact(4).all(|p| p[3] == 77));
    }

    #[test]
    fn finding_edges_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        find_edges(&mut pm);
    }

    // ------------------------------------------------------------ extrude --

    /// A pale disc on a dark ground, big enough for a grid of towers.
    fn scene() -> Pixmap {
        let mut pm = Pixmap::filled(180, 180, Rgba8::new(30, 90, 35, 255));
        for y in 0..180 {
            for x in 0..180 {
                let (dx, dy) = ((x - 90) as f32, (y - 90) as f32);
                if dx * dx + dy * dy < 60.0 * 60.0 {
                    pm.set(x, y, Rgba8::new(230, 130, 160, 255));
                }
            }
        }
        pm
    }

    /// The middle of the frame is where the viewer is, so a tower there is
    /// thrown nowhere at all and its face lands exactly on its own square.
    /// Every other tower's position is measured from that one, so if this is
    /// wrong the whole grid is sliding.
    #[test]
    fn the_tower_in_the_middle_does_not_move() {
        let mut pm = scene();
        extrude(
            &mut pm,
            ExtrudeOptions {
                size: 30,
                solid_front: true,
                ..ExtrudeOptions::default()
            },
        );
        // 180 across in 30s: the middle falls on the corner of four tiles, so
        // read a little inside one of them.
        let p = pm.get(80, 80);
        assert_eq!((p.r, p.g, p.b), (230, 130, 160), "the middle tower moved off its square");
    }

    /// Solid Front Faces is the difference between a face carrying its piece
    /// of the picture and one flat colour. Both have to be possible, or the
    /// tick box does nothing.
    #[test]
    fn solid_front_faces_flattens_what_a_face_carries() {
        // A tile with a gradient across it, so a carried face is not flat by
        // accident.
        let mut graded = Pixmap::filled(60, 60, Rgba8::BLACK);
        for y in 0..60 {
            for x in 0..60 {
                graded.set(x, y, Rgba8::new((x * 4) as u8, 100, 100, 255));
            }
        }
        let run = |solid| {
            let mut pm = graded.clone();
            extrude(
                &mut pm,
                ExtrudeOptions {
                    size: 30,
                    depth: 1.0,
                    solid_front: solid,
                    ..ExtrudeOptions::default()
                },
            );
            // Across the middle of the top-left tile's face.
            (5..25).map(|x| pm.get(x, 15).r as i32).collect::<Vec<_>>()
        };
        let carried = run(false);
        let flat = run(true);
        assert!(carried.windows(2).any(|w| w[0] != w[1]), "a carried face came back flat");
        assert!(flat.windows(2).all(|w| w[0] == w[1]), "a solid face was not one colour");
    }

    /// Pyramids have no face to carry anything, which is why CS6 greys the
    /// tick box out for them — and why it must make no difference here.
    #[test]
    fn pyramids_ignore_a_setting_that_only_blocks_have() {
        let run = |solid| {
            let mut pm = scene();
            extrude(
                &mut pm,
                ExtrudeOptions {
                    kind: ExtrudeType::Pyramids,
                    solid_front: solid,
                    ..ExtrudeOptions::default()
                },
            );
            pm
        };
        assert_eq!(run(false).as_bytes(), run(true).as_bytes());
    }

    /// Level-based reads the height off the picture, so the same picture
    /// always gives the same towers — and a bright subject stands out of a
    /// dark ground rather than being scattered at random through it.
    #[test]
    fn level_based_stands_the_bright_tiles_tallest() {
        let mut level = scene();
        extrude(
            &mut level,
            ExtrudeOptions {
                size: 30,
                depth: 200.0,
                level_based: true,
                solid_front: true,
                ..ExtrudeOptions::default()
            },
        );
        // The pale disc is thrown a long way out; the dark ground barely
        // moves. So a ring outside the disc's original edge now carries the
        // disc's colour.
        let thrown = (0..180)
            .filter(|&y| {
                let p = level.get(20, y);
                p.r > 150
            })
            .count();
        assert!(thrown > 0, "the bright tiles did not stand out over the dark ones");

        // ...and it is the same picture every time, since nothing here is
        // random.
        let again = {
            let mut pm = scene();
            extrude(
                &mut pm,
                ExtrudeOptions {
                    size: 30,
                    depth: 200.0,
                    level_based: true,
                    solid_front: true,
                    ..ExtrudeOptions::default()
                },
            );
            pm
        };
        assert_eq!(level.as_bytes(), again.as_bytes());
    }

    /// Random heights are drawn from where the tile is, so a preview and the
    /// commit behind it agree — the same rule Diffuse follows.
    #[test]
    fn random_heights_are_the_same_every_time() {
        let run = || {
            let mut pm = scene();
            extrude(&mut pm, ExtrudeOptions::default());
            pm
        };
        assert_eq!(run().as_bytes(), run().as_bytes());
    }

    /// The grid rarely divides the picture evenly, and the tick box says what
    /// to do with the part-tiles left along the edges.
    #[test]
    fn masking_incomplete_blocks_leaves_the_ragged_edge_alone() {
        // 100 across in 30s leaves a strip of 10 down the right-hand side.
        let mut original = Pixmap::filled(100, 100, Rgba8::new(200, 40, 40, 255));
        for y in 0..100 {
            for x in 0..100 {
                original.set(x, y, Rgba8::new((x * 2) as u8, (y * 2) as u8, 90, 255));
            }
        }
        let run = |mask| {
            let mut pm = original.clone();
            extrude(
                &mut pm,
                ExtrudeOptions {
                    size: 30,
                    // Barely thrown, so nothing else reaches the strip.
                    depth: 1.0,
                    solid_front: true,
                    mask_incomplete: mask,
                    ..ExtrudeOptions::default()
                },
            );
            pm
        };
        let masked = run(true);
        let built = run(false);
        for y in 0..90 {
            for x in 90..100 {
                assert_eq!(
                    masked.get(x, y),
                    original.get(x, y),
                    "a part-tile at ({x}, {y}) was built anyway"
                );
            }
        }
        assert_ne!(
            masked.as_bytes(),
            built.as_bytes(),
            "the tick box made no difference at all"
        );
    }

    #[test]
    fn extruding_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(90, 90, Rgba8::new(120, 60, 60, 200));
        extrude(&mut pm, ExtrudeOptions::default());
        assert!(pm.as_bytes().chunks_exact(4).all(|p| p[3] == 200));
    }

    #[test]
    fn extruding_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        extrude(&mut pm, ExtrudeOptions::default());
    }

    #[test]
    fn embossing_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        emboss(&mut pm, 135.0, 3.0, 100.0);
    }

    #[test]
    fn diffusing_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        diffuse(&mut pm, DiffuseMode::Normal);
    }

    // ----------------------------------------------------------- solarize --

    /// The three points of the triangle: the two ends stay dark and the
    /// middle goes to white.
    #[test]
    fn solarize_turns_the_tonal_range_into_a_triangle() {
        let mut pm = Pixmap::new(3, 1);
        pm.set(0, 0, Rgba8::new(0, 0, 0, 255));
        pm.set(1, 0, Rgba8::new(128, 128, 128, 255));
        pm.set(2, 0, Rgba8::new(255, 255, 255, 255));
        solarize(&mut pm);
        assert_eq!(pm.get(0, 0).r, 0);
        assert_eq!(pm.get(1, 0).r, 254);
        assert_eq!(pm.get(2, 0).r, 0);
    }

    /// Two tones either side of mid grey that were a step apart come back
    /// the other way round — that swap is what solarizing looks like.
    #[test]
    fn solarize_swaps_tones_either_side_of_the_middle() {
        let mut pm = Pixmap::new(2, 1);
        pm.set(0, 0, Rgba8::new(64, 64, 64, 255));
        pm.set(1, 0, Rgba8::new(192, 192, 192, 255));
        solarize(&mut pm);
        // Not equal to the level: the triangle peaks between 127 and 128, so
        // a pair mirrored about 128 lands a level apart.
        assert!((pm.get(0, 0).r as i32 - pm.get(1, 0).r as i32).abs() <= 2);

        let mut pm = Pixmap::new(2, 1);
        pm.set(0, 0, Rgba8::new(40, 40, 40, 255));
        pm.set(1, 0, Rgba8::new(230, 230, 230, 255));
        solarize(&mut pm);
        assert!(
            pm.get(0, 0).r > pm.get(1, 0).r,
            "the darker tone should have come back the lighter of the two"
        );
    }

    /// Each channel runs through the curve on its own, which is where the
    /// colour shifts come from.
    #[test]
    fn solarize_works_channel_by_channel() {
        let mut pm = Pixmap::filled(4, 4, Rgba8::new(20, 128, 250, 255));
        solarize(&mut pm);
        assert_eq!(pm.get(0, 0), Rgba8::new(40, 254, 10, 255));
    }

    #[test]
    fn solarize_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(16, 16, Rgba8::new(90, 140, 200, 77));
        solarize(&mut pm);
        assert!(pm.as_bytes().chunks_exact(4).all(|p| p[3] == 77));
    }

    #[test]
    fn solarizing_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        solarize(&mut pm);
    }

    // -------------------------------------------------------------- tiles --

    /// A picture of one colour, tiled onto a background of another: what is
    /// left of the ground is exactly the gaps the tiles moved out of, so both
    /// colours have to be there and no third one may appear.
    fn tiled_flat(fill: TileFill) -> Pixmap {
        let mut pm = Pixmap::filled(200, 200, Rgba8::new(40, 90, 160, 255));
        tiles(
            &mut pm,
            TileOptions {
                count: 10,
                offset: 30,
                fill,
                foreground: Rgba8::new(255, 0, 0, 255),
                background: Rgba8::new(0, 255, 0, 255),
                ..TileOptions::default()
            },
        );
        pm
    }

    #[test]
    fn tiles_leave_gaps_in_the_fill_colour() {
        for (fill, expected) in [
            (TileFill::BackgroundColor, Rgba8::new(0, 255, 0, 255)),
            (TileFill::ForegroundColor, Rgba8::new(255, 0, 0, 255)),
        ] {
            let pm = tiled_flat(fill);
            let picture = Rgba8::new(40, 90, 160, 255);
            let mut gaps = 0;
            for y in 0..200 {
                for x in 0..200 {
                    let px = pm.get(x, y);
                    if px == expected {
                        gaps += 1;
                    } else {
                        assert_eq!(px, picture, "a third colour appeared at {x},{y}");
                    }
                }
            }
            assert!(gaps > 0, "nothing moved: no gap was left to fill");
            // The tiles still cover most of the frame — a 30% offset cannot
            // shift a tile off more than a little over half of itself.
            assert!(gaps < 200 * 200 / 2, "the tiles covered less than half");
        }
    }

    /// Inverse Image puts the negative of the picture in the gaps rather than
    /// a swatch colour.
    #[test]
    fn the_inverse_fill_shows_the_negative() {
        let pm = tiled_flat(TileFill::InverseImage);
        let negative = Rgba8::new(215, 165, 95, 255);
        assert!(
            (0..200).any(|x| (0..200).any(|y| pm.get(x, y) == negative)),
            "no part of the negative showed through"
        );
    }

    /// Unaltered Image leaves the ground as it was, so on a flat picture the
    /// filter has nothing to show at all — which is CS6's behaviour, and why
    /// the option is only useful on a picture with detail in it.
    #[test]
    fn the_unaltered_fill_leaves_a_flat_picture_alone() {
        let pm = tiled_flat(TileFill::UnalteredImage);
        assert!(pm
            .as_bytes()
            .chunks_exact(4)
            .all(|p| p[0] == 40 && p[1] == 90 && p[2] == 160));
    }

    /// More tiles means smaller ones, which means more of them and so more
    /// edges where a gap can open.
    #[test]
    fn more_tiles_cut_the_picture_finer() {
        let gaps = |count| {
            let mut pm = Pixmap::filled(200, 200, Rgba8::new(40, 90, 160, 255));
            tiles(
                &mut pm,
                TileOptions {
                    count,
                    offset: 20,
                    fill: TileFill::BackgroundColor,
                    background: Rgba8::new(0, 255, 0, 255),
                    ..TileOptions::default()
                },
            );
            pm.as_bytes().chunks_exact(4).filter(|p| p[1] == 255).count()
        };
        assert!(
            gaps(20) > gaps(5),
            "twenty tiles left no more gap than five"
        );
    }

    /// The offsets come from where a tile sits in the grid, not from a RNG, so
    /// an undo/redo replay lands every tile where it was the first time.
    #[test]
    fn tiles_fall_the_same_way_every_time() {
        let first = tiled_flat(TileFill::BackgroundColor);
        let second = tiled_flat(TileFill::BackgroundColor);
        assert_eq!(first.as_bytes(), second.as_bytes());
    }

    #[test]
    fn tiles_leave_alpha_with_the_pixels_it_came_with() {
        let mut pm = Pixmap::filled(120, 120, Rgba8::new(120, 60, 60, 200));
        tiles(
            &mut pm,
            TileOptions {
                fill: TileFill::UnalteredImage,
                ..TileOptions::default()
            },
        );
        assert!(pm.as_bytes().chunks_exact(4).all(|p| p[3] == 200));
    }

    /// A tall thin region asked for more tiles than it has pixels across
    /// still divides into something rather than into nothing.
    #[test]
    fn tiles_survive_a_region_narrower_than_the_tile_count() {
        let mut pm = Pixmap::filled(8, 300, Rgba8::new(200, 200, 200, 255));
        tiles(
            &mut pm,
            TileOptions {
                count: 99,
                offset: 50,
                ..TileOptions::default()
            },
        );
        assert_eq!(pm.width(), 8);
    }

    #[test]
    fn tiling_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        tiles(&mut pm, TileOptions::default());
    }

    // ------------------------------------------------------ trace contour --

    /// A grey picture split down the middle: dark on the left, light on the
    /// right, with the level between the two.
    fn step() -> Pixmap {
        let mut pm = Pixmap::new(8, 4);
        for y in 0..4 {
            for x in 0..8 {
                let v = if x < 4 { 60 } else { 200 };
                pm.set(x, y, Rgba8::new(v, v, v, 255));
            }
        }
        pm
    }

    /// Upper inks the light side of the step, Lower the dark side — one
    /// column each, and the same boundary either way.
    #[test]
    fn the_edge_setting_picks_which_side_of_the_step_is_inked() {
        let mut upper = step();
        trace_contour(&mut upper, 128, ContourEdge::Upper);
        assert_eq!(upper.get(4, 2), Rgba8::new(0, 0, 0, 255), "the light side");
        assert_eq!(upper.get(3, 2), Rgba8::new(255, 255, 255, 255));

        let mut lower = step();
        trace_contour(&mut lower, 128, ContourEdge::Lower);
        assert_eq!(lower.get(3, 2), Rgba8::new(0, 0, 0, 255), "the dark side");
        assert_eq!(lower.get(4, 2), Rgba8::new(255, 255, 255, 255));
    }

    /// A level outside the picture's range crosses nothing, so there is no
    /// contour to draw and the whole frame goes white.
    #[test]
    fn a_level_nothing_crosses_leaves_a_blank_page() {
        for level in [0, 255] {
            let mut pm = step();
            trace_contour(&mut pm, level, ContourEdge::Upper);
            assert!(
                pm.as_bytes().chunks_exact(4).all(|p| p[0] == 255),
                "level {level} inked something"
            );
        }
    }

    /// Moving the level moves the line, which is the whole point of the
    /// control: it is a contour map, not an edge detector.
    #[test]
    fn the_level_decides_where_the_line_falls() {
        // A ramp across the frame, so each level lands on a different column.
        let mut pm = Pixmap::new(256, 1);
        for x in 0..256 {
            let v = x as u8;
            pm.set(x, 0, Rgba8::new(v, v, v, 255));
        }
        let inked = |level| {
            let mut copy = pm.clone();
            trace_contour(&mut copy, level, ContourEdge::Upper);
            (0..256).find(|&x| copy.get(x, 0).r == 0)
        };
        assert_eq!(inked(64), Some(64));
        assert_eq!(inked(192), Some(192));
    }

    /// Each channel is traced on its own, so a boundary only one of them
    /// crosses leaves that channel at 0 and the others at 255 — a coloured
    /// line, which is what a solid black one would have lost.
    #[test]
    fn a_boundary_in_one_channel_alone_draws_a_coloured_line() {
        let mut pm = Pixmap::new(8, 1);
        for x in 0..8 {
            // Red steps across the level; green and blue never do.
            let r = if x < 4 { 60 } else { 200 };
            pm.set(x, 0, Rgba8::new(r, 200, 200, 255));
        }
        trace_contour(&mut pm, 128, ContourEdge::Upper);
        // Red inked, the other two left white: cyan.
        assert_eq!(pm.get(4, 0), Rgba8::new(0, 255, 255, 255));
    }

    /// The frame's own border is not a crossing, or every picture would come
    /// back with a box drawn round it.
    #[test]
    fn the_border_is_not_traced_as_an_edge() {
        let mut pm = Pixmap::filled(16, 16, Rgba8::new(200, 200, 200, 255));
        trace_contour(&mut pm, 128, ContourEdge::Upper);
        assert!(pm.as_bytes().chunks_exact(4).all(|p| p[0] == 255));
    }

    #[test]
    fn tracing_a_contour_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(16, 16, Rgba8::new(200, 200, 200, 77));
        trace_contour(&mut pm, 128, ContourEdge::Lower);
        assert!(pm.as_bytes().chunks_exact(4).all(|p| p[3] == 77));
    }

    #[test]
    fn tracing_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        trace_contour(&mut pm, 128, ContourEdge::Upper);
    }

    // --------------------------------------------------------------- wind --

    /// A white block in the middle of a black frame, with clear ground either
    /// side of it — so which side the wind streaks off is visible.
    fn block() -> Pixmap {
        let mut pm = Pixmap::filled(96, 32, Rgba8::new(0, 0, 0, 255));
        for y in 0..32 {
            for x in 32..64 {
                pm.set(x, y, Rgba8::new(255, 255, 255, 255));
            }
        }
        pm
    }

    /// How much white has run out onto the ground to the left of the block,
    /// and how much onto the ground to its right.
    fn spread(pm: &Pixmap) -> (i32, i32) {
        let lit = |range: std::ops::Range<i32>| {
            range
                .map(|x| (0..32).filter(|&y| pm.get(x, y).r > 40).count() as i32)
                .sum()
        };
        (lit(0..32), lit(64..96))
    }

    /// The wind comes from the side CS6 names, so the picture travels the
    /// other way — and it streaks off *that* side of the block only. Getting
    /// this backwards mirrors the whole filter; catching both sides of the
    /// step streaks every shape from both at once.
    #[test]
    fn the_wind_streaks_one_side_of_a_shape_and_not_the_other() {
        let mut blown = block();
        wind(&mut blown, WindMethod::Wind, false);
        let (left, right) = spread(&blown);
        assert!(right > 0, "a wind from the left blew nothing to the right");
        assert_eq!(left, 0, "it streaked the upwind side as well");

        let mut blown = block();
        wind(&mut blown, WindMethod::Wind, true);
        let (left, right) = spread(&blown);
        assert!(left > 0, "a wind from the right blew nothing to the left");
        assert_eq!(right, 0, "it streaked the upwind side as well");
    }

    /// Blast is the same machine driven harder, so its streaks run further
    /// than Wind's off the same edge.
    #[test]
    fn blast_reaches_further_than_wind() {
        let reach = |method| {
            let mut pm = block();
            wind(&mut pm, method, false);
            spread(&pm).1
        };
        assert!(
            reach(WindMethod::Blast) > reach(WindMethod::Wind),
            "blast blew no further than a breeze"
        );
    }

    /// Stagger's streaks wander between rows, so the far edge of what it
    /// leaves is ragged rather than a clean column — which is the whole
    /// difference between it and Wind.
    #[test]
    fn stagger_wanders_between_rows_and_the_others_do_not() {
        // Rows that alternate red and green, so a streak carrying a
        // neighbouring row's colour is visible as such. Wind's streaks stay
        // in their own row; Stagger's are the ones that wander.
        let striped = || {
            let mut pm = Pixmap::filled(64, 32, Rgba8::new(0, 0, 0, 255));
            for y in 0..32 {
                let colour = if y % 2 == 0 {
                    Rgba8::new(255, 0, 0, 255)
                } else {
                    Rgba8::new(0, 255, 0, 255)
                };
                for x in 0..32 {
                    pm.set(x, y, colour);
                }
            }
            pm
        };
        let strays = |method| {
            let mut pm = striped();
            wind(&mut pm, method, false);
            (0..32)
                .map(|y| {
                    let wrong = |p: Rgba8| if y % 2 == 0 { p.g > 40 } else { p.r > 40 };
                    (32..64).filter(|&x| wrong(pm.get(x, y))).count()
                })
                .sum::<usize>()
        };
        assert_eq!(strays(WindMethod::Wind), 0, "a breeze crossed between rows");
        assert!(
            strays(WindMethod::Stagger) > 0,
            "stagger stayed in its own row"
        );
    }

    /// Flat ground has no edge to catch, so the wind leaves it exactly as it
    /// was — if it did not, the filter would read as a smear rather than as
    /// edges torn sideways.
    #[test]
    fn flat_ground_is_left_alone() {
        let mut pm = Pixmap::filled(48, 48, Rgba8::new(120, 120, 120, 255));
        let before = pm.clone();
        wind(&mut pm, WindMethod::Blast, false);
        assert_eq!(pm.as_bytes(), before.as_bytes());
    }

    /// Seeded from the pixel's own coordinates, so an undo/redo replay blows
    /// the same way it did the first time.
    #[test]
    fn the_wind_blows_the_same_way_every_time() {
        let mut first = block();
        wind(&mut first, WindMethod::Stagger, true);
        let mut second = block();
        wind(&mut second, WindMethod::Stagger, true);
        assert_eq!(first.as_bytes(), second.as_bytes());
    }

    #[test]
    fn the_wind_leaves_alpha_alone() {
        let mut pm = block();
        for y in 0..pm.height() as i32 {
            for x in 0..pm.width() as i32 {
                let px = pm.get(x, y);
                pm.set(x, y, Rgba8::new(px.r, px.g, px.b, 77));
            }
        }
        wind(&mut pm, WindMethod::Blast, false);
        assert!(pm.as_bytes().chunks_exact(4).all(|p| p[3] == 77));
    }

    #[test]
    fn a_wind_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        wind(&mut pm, WindMethod::Wind, false);
    }

    // ------------------------------------------------------ glowing edges --

    /// A white disc on black: one closed edge, and plenty of flat ground
    /// either side of it that ought to go dark.
    fn white_disc() -> Pixmap {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(0, 0, 0, 255));
        for y in 0..64 {
            for x in 0..64 {
                let (dx, dy) = ((x - 32) as f32, (y - 32) as f32);
                if (dx * dx + dy * dy).sqrt() < 16.0 {
                    pm.set(x, y, Rgba8::new(255, 255, 255, 255));
                }
            }
        }
        pm
    }

    /// The lit band, measured along a line out from the middle.
    fn band(pm: &Pixmap) -> usize {
        (0..64).filter(|&x| pm.get(x, 32).r > 30).count()
    }

    /// Flat ground goes black and the boundary lights up — the two halves of
    /// what the filter is for.
    #[test]
    fn the_edge_lights_up_and_the_rest_goes_dark() {
        let mut pm = white_disc();
        glowing_edges(&mut pm, 3, 6, 5);
        assert!(pm.get(32, 32).r < 30, "the middle of the disc stayed lit");
        assert!(pm.get(2, 2).r < 30, "the corner of the ground stayed lit");
        assert!(band(&pm) > 0, "the edge did not light up at all");
    }

    /// Edge Width thickens the line, which is the only thing it does.
    #[test]
    fn edge_width_thickens_the_line() {
        let wide = {
            let mut pm = white_disc();
            glowing_edges(&mut pm, 12, 6, 5);
            band(&pm)
        };
        let thin = {
            let mut pm = white_disc();
            glowing_edges(&mut pm, 1, 6, 5);
            band(&pm)
        };
        assert!(wide > thin, "a wide edge was no thicker than a thin one");
    }

    /// Edge Brightness is the gain, so it lifts the line without moving it.
    #[test]
    fn edge_brightness_is_the_gain() {
        let lit = |brightness| {
            let mut pm = white_disc();
            glowing_edges(&mut pm, 3, brightness, 5);
            (0..64).map(|x| pm.get(x, 32).r as u32).sum::<u32>()
        };
        assert!(lit(14) > lit(6), "turning the brightness up did nothing");
        // CS6's slider runs down to zero, and zero means no light at all.
        assert_eq!(lit(0), 0);
    }

    /// Smoothness blurs before the gradient is taken, so fine texture stops
    /// registering as an edge — which is the whole reason it is there.
    #[test]
    fn smoothness_stops_grain_reading_as_an_edge() {
        // Grain: all texture, no real boundary. Not a checkerboard — a
        // Sobel reads two pixels either side of the centre, which on a
        // two-pixel period are the same value, so it would measure nothing
        // at any setting and the test would pass for the wrong reason.
        let grainy = || {
            let mut pm = Pixmap::new(64, 64);
            for y in 0..64 {
                for x in 0..64 {
                    let v = 110 + (hash(x as u32, y as u32) % 40) as u8;
                    pm.set(x, y, Rgba8::new(v, v, v, 255));
                }
            }
            pm
        };
        let lit = |smoothness| {
            let mut pm = grainy();
            glowing_edges(&mut pm, 1, 6, smoothness);
            pm.as_bytes()
                .chunks_exact(4)
                .map(|p| p[0] as u32)
                .sum::<u32>()
        };
        assert!(
            lit(15) < lit(1),
            "smoothing left as much grain lit as no smoothing at all"
        );
    }

    /// The channels are taken separately, so an edge in one alone glows in
    /// that colour rather than in white.
    #[test]
    fn an_edge_in_one_channel_glows_in_its_own_colour() {
        let mut pm = Pixmap::new(64, 16);
        for y in 0..16 {
            for x in 0..64 {
                // Red steps in the middle; green and blue never do.
                let r = if x < 32 { 40 } else { 220 };
                pm.set(x, y, Rgba8::new(r, 120, 120, 255));
            }
        }
        glowing_edges(&mut pm, 1, 10, 1);
        let lit = pm.get(32, 8);
        assert!(lit.r > 60, "the channel that stepped did not light up");
        assert_eq!((lit.g, lit.b), (0, 0), "the channels that did not step lit");
    }

    #[test]
    fn glowing_edges_leaves_alpha_alone() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(120, 120, 120, 77));
        glowing_edges(&mut pm, 3, 6, 5);
        assert!(pm.as_bytes().chunks_exact(4).all(|p| p[3] == 77));
    }

    #[test]
    fn glowing_edges_over_an_empty_pixmap_does_nothing() {
        let mut pm = Pixmap::new(0, 0);
        glowing_edges(&mut pm, 3, 6, 5);
    }
}

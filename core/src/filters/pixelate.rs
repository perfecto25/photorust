//! Filter ▸ Pixelate.
//!
//! These throw away the detail of an image and replace it with a pattern of
//! cells, dots or clumps. What they have in common — and what makes them a
//! separate family from the blurs — is that the pattern is anchored to the
//! *canvas* rather than to the picture: it is a screen laid over the image,
//! not something computed from each pixel's neighbourhood.

use crate::buffer::{Pixmap, Rgba8};
use rayon::prelude::*;

/// The four screen angles Color Halftone takes, in degrees, in CS6's channel
/// order: cyan, magenta, yellow, black.
pub type ScreenAngles = [f32; 4];

/// CS6's defaults, which are the angles a real four-colour press uses. They
/// are not arbitrary: screens laid at these separations interfere least, and
/// putting two of them at the same angle is what produces the coarse moiré
/// plaid that ruins a halftone.
pub const DEFAULT_SCREEN_ANGLES: ScreenAngles = [108.0, 162.0, 90.0, 45.0];

/// How different two colours may be and still count as the same patch, summed
/// across the three channels.
///
/// This is the number that makes Facet a *facet* rather than a noise filter.
/// See [`facet`] for why it is needed at all.
const FACET_TOLERANCE: i32 = 8;

/// Filter ▸ Pixelate ▸ Facet.
///
/// Clumps neighbouring pixels of similar colour into flat patches, which is
/// what gives a photograph the look of having been laid down in brush strokes.
///
/// It is done in two passes, and the second one is not an embellishment — it
/// is the filter.
///
/// **First**, each pixel takes the colour of whichever of its nine neighbours
/// is most *typical* of them: the one whose colour is closest to all the
/// others put together. That is a medoid, chosen over a mean or a median
/// because it always returns a colour that was really there. An average
/// invents a new colour halfway between two, which softens the picture instead
/// of clumping it, and a per-channel median can invent one too by taking its
/// red from one neighbour and its green from another. This pass swallows
/// grain and speckle.
///
/// **Second**, patches are grown across the picture: a pixel within
/// [`FACET_TOLERANCE`] of the patch already running to its left or above joins
/// it and takes its colour, and otherwise begins a patch of its own.
///
/// The second pass exists because the first one cannot work on its own, and
/// the reason is worth stating: on a smoothly graded area the most typical of
/// nine values is the one already in the middle, so a medoid — or any other
/// operation that treats the nine symmetrically — hands a gradient straight
/// back untouched. Applied to a photograph it would tidy the grain and leave
/// every smooth petal and every sky exactly as it found them, which is not
/// what the filter is for. Only something that carries a decision from one
/// pixel to the next can lay down a flat patch where the picture is smooth,
/// and that is what growing regions does.
///
/// The cost of that is a direction: patches grow rightwards and downwards
/// from wherever they start. Looking at both the left and the upper neighbour
/// rather than only one keeps the boundaries angular instead of striping the
/// image horizontally.
///
/// Takes no parameters, as in CS6, and is meant to be applied more than once —
/// each pass grows the patches further, which is what Ctrl+F is for.
pub fn facet(pixmap: &mut Pixmap) {
    if pixmap.is_empty() {
        return;
    }
    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;
    let source = pixmap.clone();
    let src = &source;
    let stride = pixmap.stride();

    // -- the medoid pass, which is per pixel and so runs in parallel --------
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            for x in 0..width {
                // The nine candidates, edge-clamped like the blurs next door.
                let mut window = [Rgba8::TRANSPARENT; 9];
                for (k, slot) in window.iter_mut().enumerate() {
                    let dx = (k % 3) as i32 - 1;
                    let dy = (k / 3) as i32 - 1;
                    *slot = src.get(
                        (x + dx).clamp(0, width - 1),
                        (y + dy).clamp(0, height - 1),
                    );
                }

                let mut best = 0usize;
                let mut best_distance = i32::MAX;
                for (k, candidate) in window.iter().enumerate() {
                    let mut total = 0i32;
                    for other in &window {
                        total += difference(*candidate, *other);
                    }
                    if total < best_distance {
                        best_distance = total;
                        best = k;
                    }
                }

                let chosen = window[best];
                let i = x as usize * 4;
                out[i] = chosen.r;
                out[i + 1] = chosen.g;
                out[i + 2] = chosen.b;
                out[i + 3] = chosen.a;
            }
        });

    // -- growing the patches, which has to be in order ---------------------
    //
    // Each pixel is compared against the patches already settled to its left
    // and above, so the pass runs top-left to bottom-right and cannot be split
    // across threads. It is one comparison per pixel, so that costs little.
    let settled = pixmap.clone();
    let mut grown: Vec<Rgba8> = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let here = settled.get(x, y);
            let index = (y * width + x) as usize;

            let left = if x > 0 { Some(grown[index - 1]) } else { None };
            let above = if y > 0 {
                Some(grown[index - width as usize])
            } else {
                None
            };

            // Whichever neighbouring patch this pixel is nearest to, if it is
            // near enough to belong to either.
            let mut best = here;
            let mut best_distance = FACET_TOLERANCE;
            for candidate in [left, above].into_iter().flatten() {
                let distance = difference(here, candidate);
                if distance <= best_distance {
                    best_distance = distance;
                    best = candidate;
                }
            }
            grown.push(best);
        }
    }

    for (out, colour) in pixmap.as_bytes_mut().chunks_exact_mut(4).zip(grown) {
        out[0] = colour.r;
        out[1] = colour.g;
        out[2] = colour.b;
        out[3] = colour.a;
    }
}

/// How the mezzotint threshold is drawn.
///
/// The threshold is uniform across the *whole* range, which makes this a
/// proportional dither: a channel a fifth of the way up is thrown high one
/// time in five. That sounds like it would leave the picture as even confetti
/// with the subject barely showing, and confining the wobble to a narrow band
/// around mid-grey looks like the obvious fix — everything above the band
/// goes solid, everything below stays dark, and only the midtones break up.
///
/// It is the wrong fix, and two things in CS6's own output say so. Its white
/// petals are *not* perfectly solid: they carry a scattering of coloured
/// specks, which a band cannot produce because anything above it is thrown
/// high every time. And its dark ground is not sparsely flecked but densely
/// carpeted, which a band cannot produce either. Both densities are what
/// falls out of the plain proportional draw.
///
/// What made the first attempt look like static was not this at all — it was
/// looking at the result on a canvas fitted to the window. Any mezzotint
/// shrunk with smoothing turns back into the grey it came from.
///
/// **One threshold serves all three channels.** Drawing a separate one per
/// channel is the obvious reading of "each plate is screened", and it is
/// wrong. With independent draws every pixel picks its three answers out of a
/// hat, so all eight corners of the colour cube turn up everywhere: a dark
/// green ground comes back carrying red and blue specks it has no red or blue
/// to justify, and the result is a chromatic riot rather than a print.
///
/// With one threshold the channels cross it **in order of their own
/// strength**, so a pixel can only land on the colours between black and
/// itself. The same dark green ground — 19% red, 28% green, 4% blue — then
/// gives 72% black, 15% yellow, 9% green and 4% white, and no red or blue at
/// all, which is what CS6 produces. A pale pink petal gives 59% white, 25%
/// red, 10% magenta and 6% black: white with red pepper, again as CS6 has it.
///
/// It is tempting to think one threshold could only ever give black and
/// white. That is true of a *grey* pixel, whose three channels cross together
/// — and it is exactly why the colour appears where the picture has colour
/// and nowhere else.
#[inline]
fn dither_threshold(column: i32, band: i32) -> f32 {
    jitter(column, band, 1)
}

/// Which of CS6's ten Mezzotint patterns.
///
/// Three families — dots, lines and strokes — at increasing coarseness. They
/// differ only in the shape of the cell the random pattern is built on, which
/// is the whole of the filter's variety.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MezzotintType {
    #[default]
    FineDots,
    MediumDots,
    GrainyDots,
    CoarseDots,
    ShortLines,
    MediumLines,
    LongLines,
    ShortStrokes,
    MediumStrokes,
    LongStrokes,
}

impl MezzotintType {
    pub fn from_i32(value: i32) -> MezzotintType {
        match value {
            1 => MezzotintType::MediumDots,
            2 => MezzotintType::GrainyDots,
            3 => MezzotintType::CoarseDots,
            4 => MezzotintType::ShortLines,
            5 => MezzotintType::MediumLines,
            6 => MezzotintType::LongLines,
            7 => MezzotintType::ShortStrokes,
            8 => MezzotintType::MediumStrokes,
            9 => MezzotintType::LongStrokes,
            _ => MezzotintType::FineDots,
        }
    }

    /// The cell the pattern is laid on.
    ///
    /// Dots are square, so their grain has no direction. Lines are wide and
    /// one pixel tall, which is what draws them out into streaks. Strokes are
    /// wide *and* a few pixels tall.
    fn cell(self) -> (i32, i32) {
        match self {
            MezzotintType::FineDots => (1, 1),
            MezzotintType::MediumDots => (2, 2),
            MezzotintType::GrainyDots => (3, 3),
            MezzotintType::CoarseDots => (4, 4),
            MezzotintType::ShortLines => (4, 1),
            MezzotintType::MediumLines => (9, 1),
            MezzotintType::LongLines => (18, 1),
            MezzotintType::ShortStrokes => (6, 2),
            MezzotintType::MediumStrokes => (13, 3),
            MezzotintType::LongStrokes => (26, 4),
        }
    }
}

/// Filter ▸ Pixelate ▸ Mezzotint.
///
/// Every channel is thrown to one end or the other — nothing in between — so
/// the picture comes back in nothing but the eight corners of the colour cube:
/// black, white, and the fully saturated primaries and secondaries. That is
/// why a photograph turns into red, green and yellow confetti rather than a
/// gritty version of itself.
///
/// The decision is a **random dither**: a channel three quarters of the way up
/// has three chances in four of being thrown to the top, so an area keeps its
/// tone on average even though no single pixel does. Compare the halftone next
/// door, which keeps tone by growing a dot; this keeps it by weighting a coin.
/// See [`dither_threshold`] for why it is drawn across the whole range rather
/// than a band in the middle, which is the plausible-looking alternative.
///
/// All three channels are decided by the same draw, which is what keeps the
/// palette to the colours a region actually contains. See
/// [`dither_threshold`].
pub fn mezzotint(pixmap: &mut Pixmap, kind: MezzotintType) {
    if pixmap.is_empty() {
        return;
    }
    let (cell_w, cell_h) = kind.cell();
    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;
    let source = pixmap.clone();
    let src = &source;
    let stride = pixmap.stride();

    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            let band = y.div_euclid(cell_h);
            // Every row of cells starts at a different place, so the grain
            // comes out ragged instead of ruled into a grid. Left aligned, a
            // cell bigger than a pixel reads as a block of pixel art rather
            // than as a speck of ink.
            let offset = (jitter(band, 0, 7) * cell_w as f32) as i32;

            for x in 0..width {
                let column = (x + offset).div_euclid(cell_w);
                // The middle of the cell stands for all of it, so the whole
                // cell is thrown the same way and the grain stays the size it
                // is meant to be.
                let cx = (column * cell_w - offset + cell_w / 2).clamp(0, width - 1);
                let cy = (band * cell_h + cell_h / 2).clamp(0, height - 1);
                let centre = src.get(cx, cy);

                let i = x as usize * 4;
                let threshold = dither_threshold(column, band);
                for (c, level) in [centre.r, centre.g, centre.b].into_iter().enumerate() {
                    out[i + c] = if level as f32 / 255.0 > threshold { 255 } else { 0 };
                }
                // Transparency is left where it was: this is a printing
                // process, not an eraser.
                out[i + 3] = src.get(x, y).a;
            }
        });
}

/// How far apart two colours are, summed across the three channels.
#[inline]
fn difference(a: Rgba8, b: Rgba8) -> i32 {
    (a.r as i32 - b.r as i32).abs()
        + (a.g as i32 - b.g as i32).abs()
        + (a.b as i32 - b.b as i32).abs()
}

/// How far Fragment throws each of its four copies, in pixels. CS6's Fragment
/// takes no settings, so this is the whole of it.
const FRAGMENT_OFFSET: i32 = 4;

/// Filter ▸ Pixelate ▸ Fragment.
///
/// Four copies of the picture, shifted away from one another and averaged —
/// the look of a photograph taken through a shaking lens. Takes no parameters,
/// as in CS6.
///
/// The copies go to the four corners of a square rather than to the four
/// compass points. A diamond of offsets averages out to something very close
/// to a plain blur; a square keeps the doubled edges that make it read as four
/// exposures rather than one soft one.
pub fn fragment(pixmap: &mut Pixmap) {
    if pixmap.is_empty() {
        return;
    }
    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;

    // Premultiplied, so a copy that lands half off a soft edge does not drag
    // the colour of invisible pixels into visible ones.
    pixmap.premultiply();
    let source = pixmap.clone();
    let src = &source;
    let stride = pixmap.stride();

    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            for x in 0..width {
                let mut total = [0u32; 4];
                for dy in [-FRAGMENT_OFFSET, FRAGMENT_OFFSET] {
                    for dx in [-FRAGMENT_OFFSET, FRAGMENT_OFFSET] {
                        let p = src.get(
                            (x + dx).clamp(0, width - 1),
                            (y + dy).clamp(0, height - 1),
                        );
                        total[0] += p.r as u32;
                        total[1] += p.g as u32;
                        total[2] += p.b as u32;
                        total[3] += p.a as u32;
                    }
                }
                let i = x as usize * 4;
                for c in 0..4 {
                    out[i + c] = (total[c] / 4) as u8;
                }
            }
        });

    pixmap.unpremultiply();
}

/// Filter ▸ Pixelate ▸ Pointillize.
///
/// The picture is rebuilt out of scattered round dabs, each carrying the
/// average colour of what it covers, on a ground of the **background colour**
/// — that is what Photoshop's description means by "uses the background color
/// as a canvas area between the dots", and it is the part that makes the
/// result look painted rather than merely speckled. The gaps are not left as
/// they were and they are not black; they are bare canvas.
///
/// The dabs sit on a jittered lattice like Crystallize's seeds, but here they
/// do not tile the plane: each is a disc a little smaller than its cell, and
/// the size varies from one to the next. Equal discs on a regular grid read as
/// a pattern; unequal ones scattered read as brush work, and the gaps between
/// them are what the ground shows through.
pub fn pointillize(pixmap: &mut Pixmap, cell_size: u32, background: Rgba8) {
    let cell = cell_size.max(1) as f32;
    if pixmap.is_empty() || cell_size <= 1 {
        return;
    }
    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;

    // What each dab is loaded with. Only a three-by-three average, not the
    // whole cell: averaging a dab's own area is the obvious thing to do and it
    // flattens the picture, because neighbouring dabs then carry nearly the
    // same colour and the result is a smooth field of dots rather than a
    // painting. Taking the colour from near the middle keeps the variation
    // that makes it look mixed on a palette; the small average is only there
    // so a single stuck pixel cannot decide a whole dab.
    let mut averaged = pixmap.clone();
    crate::filters::convolve::box_blur(&mut averaged, 1);

    let blurred = &averaged;
    let source = pixmap.clone();
    let src = &source;
    let stride = pixmap.stride();

    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as f32 + 0.5;
            for x in 0..width {
                let px = x as f32 + 0.5;
                let base_i = (px / cell).floor() as i32;
                let base_j = (y / cell).floor() as i32;

                // Nearest first, so that where two dabs overlap the one whose
                // middle is closer is the one on top — the later stroke.
                let mut best = f32::MAX;
                let mut colour: Option<Rgba8> = None;
                for dj in -1..=1 {
                    for di in -1..=1 {
                        let (i, j) = (base_i + di, base_j + dj);
                        let cx = (i as f32 + jitter(i, j, 1)) * cell;
                        let cy = (j as f32 + jitter(i, j, 2)) * cell;
                        // Between a little over half a cell and four fifths of
                        // one. Big enough that the dabs crowd together and
                        // touch — about a quarter of the ground still shows
                        // through, which is what CS6 leaves — and varied
                        // enough that they do not read as a pattern.
                        let radius = cell * (0.55 + jitter(i, j, 3) * 0.25);

                        let distance = (cx - px) * (cx - px) + (cy - y) * (cy - y);
                        if distance <= radius * radius && distance < best {
                            best = distance;
                            colour = Some(blurred.get(
                                (cx as i32).clamp(0, width - 1),
                                (cy as i32).clamp(0, height - 1),
                            ));
                        }
                    }
                }

                let i = x as usize * 4;
                let paint = colour.unwrap_or(background);
                out[i] = paint.r;
                out[i + 1] = paint.g;
                out[i + 2] = paint.b;
                // Bare canvas is as opaque as the ground colour says; a dab
                // carries whatever transparency it picked up.
                out[i + 3] = match colour {
                    Some(dab) => dab.a,
                    None => {
                        // ...but nothing is painted outside the layer either.
                        if src.get(x, row as i32).a == 0 {
                            0
                        } else {
                            background.a
                        }
                    }
                };
            }
        });
}

/// Filter ▸ Pixelate ▸ Mosaic.
///
/// The picture is ruled into squares and each is filled with the average of
/// what was under it — the plainest of this family, and the one the others are
/// variations on. Crystallize is this with the lattice points nudged off their
/// grid; this is what is left when they are not.
///
/// `cell_size` is the side of a square in pixels, as CS6's slider is. The grid
/// is anchored to the layer's own origin rather than to the middle, so the
/// tiles do not shift about when the filter is re-applied at a different size.
pub fn mosaic(pixmap: &mut Pixmap, cell_size: u32) {
    let cell = cell_size.max(1) as i32;
    if pixmap.is_empty() || cell <= 1 {
        return;
    }
    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;
    let cols = (width + cell - 1) / cell;
    let rows = (height + cell - 1) / cell;

    // Premultiplied, so that a square straddling a soft edge does not average
    // the colour of invisible pixels into the visible ones.
    pixmap.premultiply();

    // One average per square. Rows of squares read their own band of the
    // picture and nothing else, so they can be worked out side by side.
    let source = pixmap.clone();
    let src = &source;
    let mut tiles = vec![[0u8; 4]; (cols * rows) as usize];
    tiles
        .par_chunks_exact_mut(cols as usize)
        .enumerate()
        .for_each(|(row, band)| {
            let top = row as i32 * cell;
            let bottom = (top + cell).min(height);
            for (col, tile) in band.iter_mut().enumerate() {
                let left = col as i32 * cell;
                let right = (left + cell).min(width);

                let mut total = [0u32; 4];
                let mut count = 0u32;
                for y in top..bottom {
                    let line = src.row(y as u32);
                    for x in left..right {
                        let i = x as usize * 4;
                        for c in 0..4 {
                            total[c] += line[i + c] as u32;
                        }
                        count += 1;
                    }
                }
                // The squares at the right and bottom edges are cut short, so
                // they average what is there rather than what would have been.
                let n = count.max(1);
                for c in 0..4 {
                    tile[c] = (total[c] / n) as u8;
                }
            }
        });

    let tiles = &tiles;
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(y, out)| {
            let band = (y as i32 / cell) * cols;
            for x in 0..width as usize {
                let tile = tiles[(band + x as i32 / cell) as usize];
                let i = x * 4;
                out[i..i + 4].copy_from_slice(&tile);
            }
        });

    pixmap.unpremultiply();
}

/// A deterministic hash in 0..1, for jittering the crystal lattice.
///
/// Seeded from the cell's coordinates rather than from a running random
/// source, for the reason every generated pattern in this engine is: undo and
/// redo replay the filter, and a crystal pattern that came out differently
/// each time could not be undone.
fn jitter(i: i32, j: i32, salt: u32) -> f32 {
    let mut h = (i as u32).wrapping_mul(0x9E3779B1)
        ^ (j as u32).wrapping_mul(0x85EBCA77)
        ^ salt.wrapping_mul(0xC2B2AE35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545F491);
    h ^= h >> 13;
    (h % 10007) as f32 / 10007.0
}

/// Filter ▸ Pixelate ▸ Crystallize.
///
/// The picture is broken into irregular polygonal cells, each filled with the
/// average of what was underneath it. The cells are the regions nearest to a
/// scatter of seed points — a Voronoi diagram — and the seeds are a square
/// lattice at `cell_size` spacing with each one nudged somewhere inside its
/// own square. That nudge is the whole difference between this and Mosaic: an
/// unjittered lattice gives back plain square tiles.
///
/// `cell_size` is the lattice spacing in pixels, as CS6's slider is.
pub fn crystallize(pixmap: &mut Pixmap, cell_size: u32) {
    let cell = cell_size.max(1) as f32;
    if pixmap.is_empty() || cell_size <= 1 {
        return;
    }
    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;

    // Enough seeds to cover the canvas with one to spare on each side, so a
    // pixel at the edge still has neighbours to be nearer to.
    let cols = (width as f32 / cell).ceil() as i32 + 2;
    let rows = (height as f32 / cell).ceil() as i32 + 2;
    let seed_at = |i: i32, j: i32| -> (f32, f32) {
        (
            (i as f32 + jitter(i, j, 1)) * cell,
            (j as f32 + jitter(i, j, 2)) * cell,
        )
    };
    let index_of = |i: i32, j: i32| -> usize {
        ((j + 1).clamp(0, rows - 1) * cols + (i + 1).clamp(0, cols - 1)) as usize
    };

    // Which cell each pixel belongs to. Worked out once and kept, rather than
    // twice: the answer is needed both to add a pixel into its cell's average
    // and again to paint that average back over it.
    let mut owner = vec![0u32; (width * height) as usize];
    owner
        .par_chunks_exact_mut(width as usize)
        .enumerate()
        .for_each(|(row, line)| {
            let y = row as f32 + 0.5;
            for (x, slot) in line.iter_mut().enumerate() {
                let px = x as f32 + 0.5;
                let base_i = (px / cell).floor() as i32;
                let base_j = (y / cell).floor() as i32;

                // A seed never leaves its own square, so the nearest one is
                // always in the ring of squares around this pixel's.
                let mut best = f32::MAX;
                let mut best_index = 0usize;
                for dj in -1..=1 {
                    for di in -1..=1 {
                        let (i, j) = (base_i + di, base_j + dj);
                        let (sx, sy) = seed_at(i, j);
                        let distance = (sx - px) * (sx - px) + (sy - y) * (sy - y);
                        if distance < best {
                            best = distance;
                            best_index = index_of(i, j);
                        }
                    }
                }
                *slot = best_index as u32;
            }
        });

    // What each cell averages out to. Accumulated in one sweep rather than in
    // parallel: a per-thread tally of every cell would cost more memory than
    // the image at the smallest cell size.
    let count = (cols * rows) as usize;
    let mut totals = vec![[0u64; 4]; count];
    let mut counts = vec![0u32; count];
    for (i, chunk) in pixmap.as_bytes().chunks_exact(4).enumerate() {
        let cell = owner[i] as usize;
        for c in 0..4 {
            totals[cell][c] += chunk[c] as u64;
        }
        counts[cell] += 1;
    }

    let totals = &totals;
    let counts = &counts;
    let owner = &owner;
    let stride = pixmap.stride();
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            for x in 0..width as usize {
                let cell = owner[row * width as usize + x] as usize;
                let n = counts[cell].max(1) as u64;
                let i = x * 4;
                for c in 0..4 {
                    out[i + c] = (totals[cell][c] / n) as u8;
                }
            }
        });
}

/// Filter ▸ Pixelate ▸ Color Halftone.
///
/// Reproduces what a printing press does: the picture is separated into cyan,
/// magenta, yellow and black, and each is redrawn as a grid of dots whose size
/// follows how much of that ink the area wants. The four grids are set at
/// different angles so their dots fall between one another rather than on top.
///
/// `max_radius` is the size of a dot at full ink, in pixels, and sets the
/// coarseness of the whole thing — the grid spacing follows from it.
pub fn color_halftone(pixmap: &mut Pixmap, max_radius: f32, angles: ScreenAngles) {
    let max_radius = max_radius.max(1.0);
    if pixmap.is_empty() {
        return;
    }
    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;

    // Spacing between dots. A dot of this radius on this grid covers rather
    // more than its own cell, so the darkest areas close up into solid ink
    // instead of stopping at a lattice of touching circles.
    let spacing = max_radius * 2.0;

    // What each dot is worth is the ink over the whole cell it stands for, not
    // the one pixel at its middle — a single pixel would let a speck of noise
    // decide the size of a dot covering a hundred of them. A box blur the
    // width of a cell is that average, already computed for every position.
    let mut averaged = pixmap.clone();
    crate::filters::convolve::box_blur(&mut averaged, max_radius.round().max(1.0) as u32);

    // Each screen's rotation, worked out once.
    let screens: Vec<(f32, f32)> = angles
        .iter()
        .map(|degrees| {
            let (sin, cos) = degrees.to_radians().sin_cos();
            (sin, cos)
        })
        .collect();

    let source = pixmap.clone();
    let blurred = &averaged;
    let original = &source;
    let stride = pixmap.stride();

    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .for_each(|(row, out)| {
            let y = row as i32;
            for x in 0..width {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let mut covered = [false; 4];

                for (channel, &(sin, cos)) in screens.iter().enumerate() {
                    // Into the screen's own frame, where its dots sit on a
                    // plain square lattice.
                    let u = px * cos + py * sin;
                    let v = -px * sin + py * cos;
                    let cell_u = (u / spacing).round();
                    let cell_v = (v / spacing).round();

                    // A dot can reach past its own cell, so the neighbours
                    // have to be asked too or the overlaps that make dark
                    // areas solid would be clipped away.
                    'dots: for du in -1..=1 {
                        for dv in -1..=1 {
                            let centre_u = (cell_u + du as f32) * spacing;
                            let centre_v = (cell_v + dv as f32) * spacing;
                            // Back out of the screen's frame to find which
                            // part of the picture this dot stands for.
                            let cx = centre_u * cos - centre_v * sin;
                            let cy = centre_u * sin + centre_v * cos;

                            let ink = ink_at(blurred, cx, cy, channel, width, height);
                            if ink <= 0.0 {
                                continue;
                            }
                            // Area proportional to the ink, so that half the
                            // ink covers half the paper. Radius is therefore
                            // the square root of it.
                            let radius = spacing * (ink / std::f32::consts::PI).sqrt();
                            let dx = px - cx;
                            let dy = py - cy;
                            if dx * dx + dy * dy <= radius * radius {
                                covered[channel] = true;
                                break 'dots;
                            }
                        }
                    }
                }

                // Ink on paper: each of the three colours takes its own
                // channel out of the white, and black takes all of it.
                let black = if covered[3] { 0.0 } else { 1.0 };
                let value = |c: bool| if c { 0.0 } else { 255.0 * black };
                let i = x as usize * 4;
                out[i] = value(covered[0]) as u8;
                out[i + 1] = value(covered[1]) as u8;
                out[i + 2] = value(covered[2]) as u8;
                // Whatever was see-through stays see-through: a screen is
                // printed on the picture, not on the space around it.
                out[i + 3] = original.get(x, y).a;
            }
        });
}

/// How much of one ink an area wants, from 0 to 1.
///
/// A plain separation — no press profile, no colour management — since what
/// matters here is the pattern of dots. The one judgement in it is how much
/// black to use: the naive answer, black wherever all three colours are
/// wanted, lays down so much of it that a dark area closes up into a solid
/// sheet and the screen disappears. A press does not do that either. This
/// uses a **skeleton black**, squared so that it stays out of the midtones
/// and only comes up in the deepest shadows, leaving the colour to the other
/// three plates where the picture still has some.
fn ink_at(src: &Pixmap, x: f32, y: f32, channel: usize, width: i32, height: i32) -> f32 {
    let px = src.get(
        (x as i32).clamp(0, width - 1),
        (y as i32).clamp(0, height - 1),
    );
    let cyan = 1.0 - px.r as f32 / 255.0;
    let magenta = 1.0 - px.g as f32 / 255.0;
    let yellow = 1.0 - px.b as f32 / 255.0;

    let common = cyan.min(magenta).min(yellow);
    let black = common * common;
    match channel {
        0 => cyan - black,
        1 => magenta - black,
        2 => yellow - black,
        _ => black,
    }
    .clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(size: u32, colour: Rgba8) -> Pixmap {
        Pixmap::filled(size, size, colour)
    }

    /// How much of the image ended up dark, as a fraction.
    fn inked(px: &Pixmap) -> f32 {
        let total = (px.width() * px.height()) as f32;
        let dark = (0..px.height() as i32)
            .flat_map(|y| (0..px.width() as i32).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let p = px.get(x, y);
                (p.r as u32 + p.g as u32 + p.b as u32) < 600
            })
            .count();
        dark as f32 / total
    }

    #[test]
    fn facet_never_invents_a_colour() {
        // The property that makes it a facet rather than a blur: every colour
        // it lays down was really in the picture somewhere. An average, or a
        // per-channel median taking its red from one neighbour and its green
        // from another, would fail this and would soften the picture instead
        // of clumping it.
        let source = many_colours(64);
        let mut after = source.clone();
        facet(&mut after);

        let mut present = std::collections::HashSet::new();
        for chunk in source.as_bytes().chunks_exact(4) {
            present.insert([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        for chunk in after.as_bytes().chunks_exact(4) {
            assert!(
                present.contains(&[chunk[0], chunk[1], chunk[2], chunk[3]]),
                "Facet invented the colour {:?}",
                chunk
            );
        }
    }

    #[test]
    fn facet_lays_flat_patches_across_a_smooth_gradient() {
        // The failure this pins is the one that shipped: a Facet built only on
        // the medoid handed every smoothly graded area straight back, because
        // the most typical of nine values along a gradient is the one already
        // in the middle. Applied to a photograph it tidied the grain and left
        // every petal and every sky exactly as it found them.
        //
        // A clean ramp, so there is no grain to hide behind, and a gentle one
        // — the shallow gradients of a petal or a sky are the ones the old
        // version left untouched.
        let size = 128i32;
        let mut px = Pixmap::new(size as u32, size as u32);
        for y in 0..size {
            for x in 0..size {
                let v = (100 + x * 60 / size) as u8;
                px.set(x, y, Rgba8::new(v, v, v, 255));
            }
        }
        let before = distinct_colours(&px);
        facet(&mut px);
        let after = distinct_colours(&px);

        assert!(
            after * 2 < before,
            "a smooth ramp came back with {} of its {} tones, so no patches were laid down",
            after,
            before
        );

        // ...and the patches are flat: somewhere along the middle row there
        // has to be a run of pixels all the same.
        let mut longest = 1;
        let mut run = 1;
        for x in 1..size {
            if px.get(x, 64) == px.get(x - 1, 64) {
                run += 1;
                longest = longest.max(run);
            } else {
                run = 1;
            }
        }
        assert!(longest >= 3, "the longest flat run was {} pixels", longest);
    }

    #[test]
    fn facet_swallows_a_lone_speck() {
        // Clumping similar colours together means an odd one out loses.
        let mut px = flat(32, Rgba8::new(120, 120, 120, 255));
        px.set(16, 16, Rgba8::new(255, 0, 0, 255));
        facet(&mut px);
        assert_eq!(px.get(16, 16), Rgba8::new(120, 120, 120, 255), "the speck survived");
    }

    /// A gradient with grain on it — a photograph, in miniature.
    fn speckled(size: u32) -> Pixmap {
        let mut px = Pixmap::new(size, size);
        for y in 0..size as i32 {
            for x in 0..size as i32 {
                let ramp = (x + y) as i32 * 200 / (size as i32 * 2);
                let grain = ((x * 37 + y * 91) % 23) - 11;
                let v = (ramp + grain).clamp(0, 255) as u8;
                px.set(x, y, Rgba8::new(v, v / 2 + 60, 255 - v, 255));
            }
        }
        px
    }

    #[test]
    fn repeating_facet_grows_the_patches() {
        // CS6's Facet takes no settings, so repeating it is the only control
        // there is — which means each pass has to go further than the last.
        let source = speckled(96);
        let after_passes = |n: usize| {
            let mut px = source.clone();
            for _ in 0..n {
                facet(&mut px);
            }
            distinct_colours(&px)
        };
        assert!(
            after_passes(4) < after_passes(1),
            "a fourth pass of Facet left as many colours as the first: {} against {}",
            after_passes(4),
            after_passes(1)
        );
    }

    #[test]
    fn mezzotint_leaves_nothing_between_the_corners_of_the_colour_cube() {
        // The defining property: every channel goes all the way to one end or
        // the other. Anything in between means the dither has become a blend,
        // and the picture would come back grey rather than as confetti.
        let mut px = many_colours(64);
        mezzotint(&mut px, MezzotintType::GrainyDots);
        for chunk in px.as_bytes().chunks_exact(4) {
            for &channel in &chunk[..3] {
                assert!(channel == 0 || channel == 255, "a channel came back as {}", channel);
            }
        }
    }

    #[test]
    fn white_and_black_survive_untouched() {
        // The two ends of the dither. A threshold that let white flicker would
        // leave grubby specks all over a highlight.
        for (level, expected) in [(255u8, 255u8), (0, 0)] {
            let mut px = flat(48, Rgba8::new(level, level, level, 255));
            mezzotint(&mut px, MezzotintType::FineDots);
            for y in 0..48 {
                for x in 0..48 {
                    assert_eq!(px.get(x, y).r, expected, "{} came back wrong at {},{}", level, x, y);
                }
            }
        }
    }

    #[test]
    fn one_draw_decides_all_three_channels() {
        // Drawing a threshold per channel is the obvious reading of "each
        // plate is screened" and it is wrong: independent draws let every
        // pixel pick its three answers out of a hat, so all eight corners of
        // the colour cube turn up everywhere and a dark green ground comes
        // back carrying red and blue specks it has no red or blue to justify.
        //
        // With one draw the channels cross it in order of their own strength,
        // so a region can only land on the colours between black and itself.
        // Measured on the ground of the photograph this was found on.
        let ground = Rgba8::new(48, 71, 10, 255);
        let mut px = flat(200, ground);
        mezzotint(&mut px, MezzotintType::FineDots);

        // Green is the strongest channel here, then red, then blue — so the
        // only colours reachable are black, green, yellow and white.
        let mut seen = std::collections::HashSet::new();
        for chunk in px.as_bytes().chunks_exact(4) {
            seen.insert((chunk[0] > 0, chunk[1] > 0, chunk[2] > 0));
        }
        let allowed = [
            (false, false, false),
            (false, true, false),
            (true, true, false),
            (true, true, true),
        ];
        for colour in &seen {
            assert!(
                allowed.contains(colour),
                "a dark green ground produced {:?}, which it has nothing to make it from",
                colour
            );
        }
        // ...and it is not trivially passing by coming back all black.
        assert!(seen.contains(&(false, true, false)), "no green at all");
        assert!(seen.contains(&(true, true, false)), "no yellow at all");
    }

    #[test]
    fn the_dither_is_drawn_across_the_whole_range() {
        // The tempting alternative is a threshold confined to a band around
        // mid-grey, so that lights go solid, darks stay dark and only the
        // midtones break up. It looks more like a mezzotint in the abstract
        // and it is wrong: CS6 speckles a near-white petal and carpets a dark
        // ground, and a band does neither. Pinned by the numbers, because
        // both models produce something that looks like a mezzotint.
        let lit = |level: u8| {
            let mut px = flat(200, Rgba8::new(level, level, level, 255));
            mezzotint(&mut px, MezzotintType::FineDots);
            let count = (0..200i32)
                .flat_map(|y| (0..200i32).map(move |x| (x, y)))
                .filter(|&(x, y)| px.get(x, y).r > 0)
                .count();
            count as f32 / (200.0 * 200.0)
        };

        for level in [40u8, 90, 128, 190, 225] {
            let wanted = level as f32 / 255.0;
            let got = lit(level);
            assert!(
                (got - wanted).abs() < 0.05,
                "a tone of {} came back {:.0}% lit where the tone itself is {:.0}%",
                level,
                got * 100.0,
                wanted * 100.0
            );
        }
    }

    #[test]
    fn a_lighter_area_keeps_more_of_its_ink() {
        // The same thing over a wider span and without the arithmetic: three
        // greys, and the count of lit pixels has to follow them.
        let lit = |level: u8| {
            let mut px = flat(128, Rgba8::new(level, level, level, 255));
            mezzotint(&mut px, MezzotintType::FineDots);
            (0..128i32)
                .flat_map(|y| (0..128i32).map(move |x| (x, y)))
                .filter(|&(x, y)| px.get(x, y).r > 0)
                .count()
        };
        let dark = lit(48);
        let mid = lit(128);
        let light = lit(208);
        assert!(
            dark < mid && mid < light,
            "the dither did not follow the tone: {}, {}, {}",
            dark,
            mid,
            light
        );
    }

    #[test]
    fn the_line_types_run_sideways() {
        // What separates a line from a dot is that its grain has a direction.
        // Built on a square cell by mistake, Long Lines would still produce a
        // perfectly convincing mezzotint — just the same one as Coarse Dots.
        let mut px = flat(200, Rgba8::new(128, 128, 128, 255));
        mezzotint(&mut px, MezzotintType::LongLines);

        // How often the pattern changes along a row, against down a column.
        let changes = |horizontal: bool| {
            let mut count = 0;
            for a in 20..180 {
                for b in 1..200 {
                    let (x0, y0, x1, y1) = if horizontal {
                        (b - 1, a, b, a)
                    } else {
                        (a, b - 1, a, b)
                    };
                    if px.get(x0, y0).r != px.get(x1, y1).r {
                        count += 1;
                    }
                }
            }
            count
        };
        let across = changes(true);
        let down = changes(false);
        assert!(
            down > across * 3,
            "Long Lines changes {} times across and {} times down, which is not a line",
            across,
            down
        );
    }

    #[test]
    fn a_coarser_grain_means_fewer_specks() {
        let specks = |kind| {
            let mut px = flat(160, Rgba8::new(128, 128, 128, 255));
            mezzotint(&mut px, kind);
            let mut count = 0;
            for y in 0..160 {
                for x in 1..160 {
                    if px.get(x, y).r != px.get(x - 1, y).r {
                        count += 1;
                    }
                }
            }
            count
        };
        assert!(
            specks(MezzotintType::FineDots) > specks(MezzotintType::CoarseDots),
            "Fine Dots is no finer than Coarse Dots"
        );
    }

    #[test]
    fn mezzotint_is_deterministic() {
        let run = || {
            let mut px = many_colours(64);
            mezzotint(&mut px, MezzotintType::ShortStrokes);
            px
        };
        assert_eq!(run().as_bytes(), run().as_bytes());
    }

    #[test]
    fn fragment_makes_four_copies_at_the_corners_of_a_square() {
        // This is the whole filter, and the arrangement is the part worth
        // pinning: four copies at the compass points instead average out to
        // something indistinguishable from a small blur.
        let mut px = Pixmap::filled(41, 41, Rgba8::BLACK);
        px.set(20, 20, Rgba8::WHITE);
        fragment(&mut px);

        let lit = |x: i32, y: i32| px.get(x, y).r > 0;
        for (dx, dy) in [(-4, -4), (4, -4), (-4, 4), (4, 4)] {
            assert!(lit(20 + dx, 20 + dy), "no copy landed at {},{}", dx, dy);
        }
        assert!(!lit(20, 20), "the original was left where it was");
        // The compass points, which a diamond of offsets would have lit.
        for (dx, dy) in [(0, -4), (0, 4), (-4, 0), (4, 0)] {
            assert!(!lit(20 + dx, 20 + dy), "a copy landed at {},{}", dx, dy);
        }
    }

    #[test]
    fn fragment_leaves_a_flat_image_alone() {
        // Four copies of the same thing averaged is that thing. A rounding
        // error in the averaging would show up here as a shift of a level.
        let mut px = flat(48, Rgba8::new(77, 155, 211, 255));
        fragment(&mut px);
        assert_eq!(px.get(24, 24), Rgba8::new(77, 155, 211, 255));
    }

    /// A picture with a different colour nearly everywhere, so that flattening
    /// it into cells is measurable as a loss of variety.
    fn many_colours(size: u32) -> Pixmap {
        let mut px = Pixmap::new(size, size);
        for y in 0..size as i32 {
            for x in 0..size as i32 {
                px.set(
                    x,
                    y,
                    Rgba8::new((x * 3 % 256) as u8, (y * 5 % 256) as u8, ((x + y) % 256) as u8, 255),
                );
            }
        }
        px
    }

    fn distinct_colours(px: &Pixmap) -> usize {
        let mut seen = std::collections::HashSet::new();
        for chunk in px.as_bytes().chunks_exact(4) {
            seen.insert([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        seen.len()
    }

    #[test]
    fn pointillize_paints_the_gaps_in_the_background_colour() {
        // The part of the filter that is not obvious from looking at it: the
        // canvas between the dabs is the *background colour*, not what was
        // there before and not black. On a blue ground over a red picture,
        // every pixel has to be one or the other.
        let ground = Rgba8::new(0, 0, 255, 255);
        let mut px = flat(120, Rgba8::new(255, 0, 0, 255));
        pointillize(&mut px, 9, ground);

        let mut dabs = 0;
        let mut canvas = 0;
        for y in 0..120i32 {
            for x in 0..120i32 {
                match px.get(x, y) {
                    p if p == Rgba8::new(255, 0, 0, 255) => dabs += 1,
                    p if p == ground => canvas += 1,
                    other => panic!("{:?} at {},{} is neither dab nor canvas", other, x, y),
                }
            }
        }
        assert!(dabs > 0, "nothing was painted");
        assert!(canvas > 0, "the dabs covered everything, so no ground shows");
    }

    #[test]
    fn the_dabs_carry_the_colour_they_cover() {
        // Half red and half green, and each half must come back in its own
        // colour rather than in one average of the two.
        let size = 120i32;
        let mut px = Pixmap::new(size as u32, size as u32);
        for y in 0..size {
            for x in 0..size {
                let colour = if x < size / 2 {
                    Rgba8::new(220, 0, 0, 255)
                } else {
                    Rgba8::new(0, 220, 0, 255)
                };
                px.set(x, y, colour);
            }
        }
        pointillize(&mut px, 8, Rgba8::WHITE);

        let reddest = |x0: i32, x1: i32| {
            let mut red = 0;
            let mut green = 0;
            for y in 10..110 {
                for x in x0..x1 {
                    let p = px.get(x, y);
                    if p.r > p.g {
                        red += 1;
                    } else if p.g > p.r {
                        green += 1;
                    }
                }
            }
            (red, green)
        };
        let (left_red, left_green) = reddest(5, 45);
        let (right_red, right_green) = reddest(75, 115);
        assert!(left_red > left_green * 4, "the red half came back green");
        assert!(right_green > right_red * 4, "the green half came back red");
    }

    #[test]
    fn the_dabs_crowd_together_without_closing_up() {
        // Two failures either side of this. Too small and the picture reads as
        // specks on a white sheet rather than as paint; too large and the
        // dabs merge into a blur with no ground between them and none of the
        // texture the filter exists for.
        let mut px = flat(200, Rgba8::new(200, 40, 40, 255));
        pointillize(&mut px, 6, Rgba8::WHITE);
        let ground = (0..200i32)
            .flat_map(|y| (0..200i32).map(move |x| (x, y)))
            .filter(|&(x, y)| px.get(x, y) == Rgba8::WHITE)
            .count() as f32
            / (200.0 * 200.0);
        assert!(
            (0.10..0.40).contains(&ground),
            "{:.0}% of the picture came back as bare ground",
            ground * 100.0
        );
    }

    #[test]
    fn neighbouring_dabs_do_not_all_carry_the_same_colour() {
        // Loading each dab with the average over its own cell is the obvious
        // thing to do, and it flattens the picture: neighbouring dabs come out
        // nearly the same colour and the result is a smooth field of dots
        // rather than something mixed on a palette. The texture in the picture
        // has to survive into the dabs.
        //
        // Measured at a coarse cell, where the two ways of loading a dab are
        // furthest apart: a twenty-pixel average leaves almost none of the
        // grain, and a three-by-three one leaves most of it.
        let size = 300i32;
        let ramp = |x: i32| 90.0 + x as f32 * 60.0 / size as f32;
        let mut src = Pixmap::new(size as u32, size as u32);
        for y in 0..size {
            for x in 0..size {
                let grain = ((x * 37 + y * 91) % 81) - 40;
                let v = (ramp(x) + grain as f32).clamp(0.0, 255.0) as u8;
                src.set(x, y, Rgba8::new(v, 200 - v / 2, 120, 255));
            }
        }

        let mut px = src.clone();
        pointillize(&mut px, 20, Rgba8::WHITE);

        // With the ramp taken out, whatever spread is left is the texture.
        let mut residuals = Vec::new();
        for y in 10..size - 10 {
            for x in 10..size - 10 {
                let p = px.get(x, y);
                if p != Rgba8::WHITE {
                    residuals.push(p.r as f32 - ramp(x));
                }
            }
        }
        let mean = residuals.iter().sum::<f32>() / residuals.len() as f32;
        let spread = (residuals.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>()
            / residuals.len() as f32)
            .sqrt();
        assert!(
            spread > 4.0,
            "the dabs vary by only {:.1} levels around the tone they sit on, so the picture \
             was averaged flat before it was painted",
            spread
        );
    }

    #[test]
    fn a_larger_cell_makes_larger_dabs() {
        // Cell Size is the only control there is. Measured as how often a row
        // crosses between a dab and the ground, which falls as the dabs grow.
        let crossings = |cell| {
            let mut px = flat(200, Rgba8::new(200, 40, 40, 255));
            pointillize(&mut px, cell, Rgba8::WHITE);
            let mut count = 0;
            for y in (10..190).step_by(5) {
                for x in 1..200 {
                    if (px.get(x, y).g > 128) != (px.get(x - 1, y).g > 128) {
                        count += 1;
                    }
                }
            }
            count
        };
        assert!(
            crossings(5) > crossings(25),
            "a fine pointillize has no more dabs across a row than a coarse one"
        );
    }

    #[test]
    fn pointillize_is_deterministic() {
        let run = || {
            let mut px = many_colours(96);
            pointillize(&mut px, 7, Rgba8::WHITE);
            px
        };
        assert_eq!(run().as_bytes(), run().as_bytes());
    }

    #[test]
    fn pointillize_does_not_paint_outside_the_layer() {
        // A layer's empty half is not canvas to be painted on; it is nothing.
        let mut px = Pixmap::new(80, 80);
        for y in 0..80 {
            for x in 0..40 {
                px.set(x, y, Rgba8::new(200, 60, 60, 255));
            }
        }
        pointillize(&mut px, 6, Rgba8::WHITE);
        for y in 0..80 {
            assert_eq!(px.get(70, y).a, 0, "the empty half of the layer was painted on");
        }
    }

    #[test]
    fn mosaic_fills_each_square_with_one_colour() {
        // The defining property, and the one that separates it from a blur:
        // a tile is flat, and its edges land on the grid.
        let source = many_colours(100);
        let mut after = source.clone();
        let cell = 10i32;
        mosaic(&mut after, cell as u32);

        for tile_y in 0..10 {
            for tile_x in 0..10 {
                let first = after.get(tile_x * cell, tile_y * cell);
                for y in 0..cell {
                    for x in 0..cell {
                        assert_eq!(
                            after.get(tile_x * cell + x, tile_y * cell + y),
                            first,
                            "the tile at {},{} is not one colour",
                            tile_x,
                            tile_y
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_mosaic_tile_is_the_average_of_what_was_under_it() {
        // Sampling the middle of the square instead would look identical on a
        // photograph and would throw away the tone of everything else in it.
        let source = many_colours(64);
        let mut after = source.clone();
        mosaic(&mut after, 8);

        let mut total = 0u32;
        for y in 0..8i32 {
            for x in 0..8i32 {
                total += source.get(x, y).r as u32;
            }
        }
        let mean = (total / 64) as i32;
        assert!(
            (after.get(3, 3).r as i32 - mean).abs() <= 1,
            "the first tile came back {} where its square averages {}",
            after.get(3, 3).r,
            mean
        );
    }

    #[test]
    fn a_clipped_edge_tile_averages_only_what_is_there() {
        // The squares at the right and bottom edges are cut short when the
        // picture is not a whole number of cells across. Counting the pixels
        // that are not there would darken those two strips — a shadow down
        // the edge of every mosaic, on some images and not others.
        let mut px = flat(101, Rgba8::WHITE);
        mosaic(&mut px, 10);
        for y in 0..101 {
            for x in 0..101 {
                assert_eq!(px.get(x, y), Rgba8::WHITE, "at {},{}", x, y);
            }
        }
    }

    #[test]
    fn a_larger_mosaic_cell_means_fewer_of_them() {
        let source = many_colours(160);
        let colours = |cell| {
            let mut px = source.clone();
            mosaic(&mut px, cell);
            distinct_colours(&px)
        };
        assert!(colours(8) > colours(40), "a fine mosaic has no more tiles than a coarse one");
    }

    #[test]
    fn crystallize_flattens_the_picture_into_cells() {
        let source = many_colours(120);
        let mut after = source.clone();
        crystallize(&mut after, 12);

        let before = distinct_colours(&source);
        let now = distinct_colours(&after);
        assert!(
            now * 10 < before,
            "the picture kept {} of its {} colours, so it was not flattened into cells",
            now,
            before
        );
    }

    #[test]
    fn a_larger_cell_size_means_fewer_cells() {
        // Cell Size is the whole interface. Wired to nothing, or to the wrong
        // end of the scale, the filter still produces a convincing crystal.
        let source = many_colours(160);
        let colours = |cell| {
            let mut px = source.clone();
            crystallize(&mut px, cell);
            distinct_colours(&px)
        };
        assert!(
            colours(8) > colours(40),
            "a fine crystal has no more cells than a coarse one: {} against {}",
            colours(8),
            colours(40)
        );
    }

    #[test]
    fn the_cells_are_not_square_tiles() {
        // What separates Crystallize from Mosaic is that the seeds are nudged
        // off their lattice. Left on it, the cells come out as plain squares
        // and every boundary along a row falls on a multiple of the cell
        // size — which is a different filter with the same slider.
        let source = many_colours(200);
        let mut after = source.clone();
        let cell = 20u32;
        crystallize(&mut after, cell);

        let mut off_lattice = 0;
        for y in (10..190).step_by(7) {
            for x in 1..200 {
                if after.get(x, y) != after.get(x - 1, y) && x as u32 % cell != 0 {
                    off_lattice += 1;
                }
            }
        }
        assert!(
            off_lattice > 20,
            "every cell boundary landed on the lattice, so these are square tiles"
        );
    }

    #[test]
    fn crystallize_is_deterministic() {
        // The seeds are jittered from a hash of their own coordinates rather
        // than from a running random source, because undo and redo replay the
        // filter and a crystal that came out differently each time could not
        // be undone.
        let source = many_colours(96);
        let run = || {
            let mut px = source.clone();
            crystallize(&mut px, 9);
            px
        };
        assert_eq!(run().as_bytes(), run().as_bytes());
    }

    #[test]
    fn a_flat_image_crystallizes_to_itself() {
        // Every cell averages the same colour, so nothing should move — and a
        // cell that reached outside the picture and averaged in nothing would
        // show up here as a darker patch.
        let mut px = flat(80, Rgba8::new(90, 140, 200, 255));
        crystallize(&mut px, 11);
        for y in 0..80 {
            for x in 0..80 {
                assert_eq!(px.get(x, y), Rgba8::new(90, 140, 200, 255), "at {},{}", x, y);
            }
        }
    }

    #[test]
    fn a_halftone_asked_for_without_angles_still_gets_four_screens() {
        // Zero is a legitimate screen angle, so a missing parameter cannot be
        // told from a deliberate one by its value. Naming the fallback is
        // what stops four screens ending up stacked on top of one another,
        // which is a plaid rather than a halftone.
        let filter = crate::filters::Filter::from_menu_name("Color Halftone", &[6.0])
            .expect("Color Halftone is not in the menu-name table");
        match filter {
            crate::filters::Filter::ColorHalftone { angles, .. } => {
                assert_eq!(angles, DEFAULT_SCREEN_ANGLES);
            }
            other => panic!("got {:?}", other),
        }
    }

    #[test]
    fn white_paper_takes_no_ink() {
        // Nothing to print. A screen that laid down dots here would fog the
        // highlights of every image it touched.
        let mut px = flat(64, Rgba8::WHITE);
        color_halftone(&mut px, 6.0, DEFAULT_SCREEN_ANGLES);
        assert_eq!(inked(&px), 0.0, "a white image came back with dots on it");
    }

    #[test]
    fn a_darker_image_takes_more_ink_than_a_lighter_one() {
        // The whole point of a halftone: the dots grow with the tone. Three
        // greys, and the coverage has to follow them in order.
        let coverage = |level: u8| {
            let mut px = flat(96, Rgba8::new(level, level, level, 255));
            color_halftone(&mut px, 6.0, DEFAULT_SCREEN_ANGLES);
            inked(&px)
        };
        let light = coverage(200);
        let mid = coverage(128);
        let dark = coverage(48);
        assert!(
            light < mid && mid < dark,
            "coverage did not follow the tone: {:.2}, {:.2}, {:.2}",
            light,
            mid,
            dark
        );
    }

    #[test]
    fn the_screen_angles_change_where_the_dots_fall() {
        // Four numbers that all look alike in a dialog. Wired to nothing, the
        // filter would still produce a perfectly convincing halftone — just
        // one with every plate on top of its neighbour.
        let render = |angles: ScreenAngles| {
            let mut px = flat(96, Rgba8::new(150, 90, 60, 255));
            color_halftone(&mut px, 6.0, angles);
            px
        };
        let standard = render(DEFAULT_SCREEN_ANGLES);
        let turned = render([0.0, 30.0, 60.0, 15.0]);
        assert_ne!(
            standard.as_bytes(),
            turned.as_bytes(),
            "the screen angles made no difference"
        );
    }

    #[test]
    fn a_bigger_radius_makes_a_coarser_screen() {
        // Max Radius sets the size of a dot at full ink, and the grid follows
        // from it, so a larger one means fewer and bigger dots — not the same
        // screen printed darker.
        let dots = |radius: f32| {
            let mut px = flat(120, Rgba8::new(128, 128, 128, 255));
            color_halftone(&mut px, radius, DEFAULT_SCREEN_ANGLES);
            // Count how often a row crosses from paper to ink, which is twice
            // the number of dots it passes through.
            let mut crossings = 0;
            let mut last = px.get(0, 60).r < 128;
            for x in 1..120 {
                let now = px.get(x, 60).r < 128;
                if now != last {
                    crossings += 1;
                    last = now;
                }
            }
            crossings
        };
        assert!(
            dots(4.0) > dots(12.0),
            "a fine screen has no more dots across a row than a coarse one"
        );
    }

    #[test]
    fn transparent_pixels_stay_transparent() {
        // A screen is printed on the picture, not on the space around it.
        let mut px = Pixmap::new(64, 64);
        for y in 0..64 {
            for x in 0..32 {
                px.set(x, y, Rgba8::new(40, 40, 40, 255));
            }
        }
        color_halftone(&mut px, 5.0, DEFAULT_SCREEN_ANGLES);
        for y in 0..64 {
            assert_eq!(px.get(50, y).a, 0, "the empty half of the layer was printed on");
        }
    }
}

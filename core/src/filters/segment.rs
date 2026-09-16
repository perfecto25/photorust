//! Cutting a picture into regions of one colour.
//!
//! Not a filter of its own — nothing in the Filter menu is called this. It is
//! the pass underneath the ones that describe themselves as working on
//! *regions* rather than on windows: Palette Knife today, and Cutout, Poster
//! Edges and Underpainting are the same shape of problem.
//!
//! The difference from everything in [`convolve`](super::convolve) matters.
//! Those look at a fixed window around a pixel, so a petal and the grass behind
//! it are the same size of thing to them. This asks what a pixel *belongs to*,
//! and the answer is decided by two things at once: how alike two pixels are,
//! and how near. Alikeness alone is not enough and the first attempt here
//! proved it — a graph merge on colour alone (Felzenszwalb and Huttenlocher's,
//! which is the obvious thing to reach for) let the whole out-of-focus
//! background of a photograph merge into one flat mass through a chain of steps
//! no one of which was worth cutting at. Nearness is what bounds a region:
//! nothing comes back much bigger than the spacing asked for, so a busy area
//! comes back as many regions and a plain one as several that happen to match.
//!
//! The method is Achanta's SLIC (2010), which is k-means over a five-number
//! space — three of colour and two of position — with each seed only allowed to
//! compete for pixels near it. `spacing` is how far apart the seeds start and
//! so roughly how big a region comes back; `compactness` is how much the two
//! position numbers count against the three colour ones, which is the whole
//! difference between regions that hug what is in the picture and regions that
//! come back as tidy blocks.

use crate::buffer::Pixmap;
use rayon::prelude::*;

/// How many times the seeds are moved and the picture reassigned.
///
/// SLIC converges fast and the last rounds move very few pixels; ten is the
/// number the paper settles on and there is nothing visible past it.
const ROUNDS: usize = 10;

/// One seed: a colour and a place, both of which move.
#[derive(Clone, Copy)]
struct Seed {
    r: f32,
    g: f32,
    b: f32,
    x: f32,
    y: f32,
}

/// Which way the picture runs at each seed, as a unit vector.
///
/// From the structure tensor: the gradients over a window around the seed are
/// summed as `gx²`, `gx·gy` and `gy²`, and the direction the *gradient* mostly
/// points in falls out of them. What is wanted is the direction across that —
/// along the edge, not up it — because a stroke laid along a petal's outline
/// describes the petal and one laid across it rubs the outline out.
///
/// Where there is no edge to speak of there is no direction either, and the
/// tensor's answer in a flat area is not weakly-preferred but *arbitrary* — all
/// three sums go to zero together and what comes back is whichever way the
/// rounding happened to fall. Taking it at face value tiles a whole flat
/// background with strokes at exactly the same angle, which reads as corduroy.
/// So below a threshold of coherence the seed is given a direction of its own
/// instead, and the background comes back stroked every which way, as a painter
/// working an unimportant area would leave it.
fn lie_of_the_land(pixmap: &Pixmap, seeds: &[Seed], spacing: f32) -> Vec<(f32, f32)> {
    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;
    let px = pixmap.as_bytes();
    let grey = |x: i32, y: i32| -> f32 {
        let i = (y.clamp(0, height - 1) * width + x.clamp(0, width - 1)) as usize * 4;
        px[i] as f32 * 0.299 + px[i + 1] as f32 * 0.587 + px[i + 2] as f32 * 0.114
    };
    let look = (spacing * 0.75).round() as i32;

    seeds
        .par_iter()
        .enumerate()
        .map(|(k, seed)| {
            let (cx, cy) = (seed.x as i32, seed.y as i32);
            let (mut xx, mut xy, mut yy) = (0.0f32, 0.0f32, 0.0f32);
            // Every other pixel in each direction: a direction is a coarse
            // thing and this is a quarter of the work.
            let mut y = cy - look;
            while y <= cy + look {
                let mut x = cx - look;
                while x <= cx + look {
                    let gx = grey(x + 1, y) - grey(x - 1, y);
                    let gy = grey(x, y + 1) - grey(x, y - 1);
                    xx += gx * gx;
                    xy += gx * gy;
                    yy += gy * gy;
                    x += 2;
                }
                y += 2;
            }
            let spread = ((xx - yy) * (xx - yy) + 4.0 * xy * xy).sqrt();
            let coherence = spread / (xx + yy + 1.0);
            let angle = if coherence > COHERENT {
                // Turned a quarter turn off the gradient, onto the edge.
                0.5 * (2.0 * xy).atan2(xx - yy) + std::f32::consts::FRAC_PI_2
            } else {
                wobble(seed.x as i32, seed.y as i32, (spacing as i32 * 3).max(2), 7)
                    * std::f32::consts::PI
            };
            (angle.cos(), angle.sin())
        })
        .collect()
}

/// How much of one direction there has to be in a patch of picture before it
/// counts as running that way at all.
///
/// Low: a petal's outline against the grass is emphatic and anything faint is
/// better off stroked at a direction of its own. Raising it turns more of the
/// picture over to the wandering direction and the strokes stop describing
/// anything.
const COHERENT: f32 = 0.35;

/// Cut `pixmap` into regions and return which region each pixel fell in,
/// numbered from zero, along with how many there were.
///
/// * `spacing` is how far apart the regions start, in pixels, and so roughly
///   how big they come back. It is a bound as much as a size: a pixel can only
///   ever join one of the nine seeds around it, so nothing comes back as one
///   region spanning the picture however evenly the picture is coloured.
/// * `compactness` is how much being *near* counts against being *alike*, in
///   levels of colour per unit of spacing. Low, and a region will reach a long
///   way to take in something the same colour, leaving shapes that follow what
///   is in the picture; high, and the regions come back as tidy blocks with the
///   picture's own boundaries cut across.
/// * `elongation` is how much longer than wide a region is allowed to come
///   back, and which way it runs is not a free choice — each region takes the
///   lie of the picture where it sits, measured in [`lie_of_the_land`], so the
///   regions run along an edge rather than across it. At 1 they are round and
///   none of that is worked out.
pub(crate) fn regions(
    pixmap: &Pixmap,
    spacing: f32,
    compactness: f32,
    elongation: f32,
) -> (Vec<u32>, usize) {
    let width = pixmap.width() as usize;
    let height = pixmap.height() as usize;
    if width == 0 || height == 0 {
        return (Vec::new(), 0);
    }
    let step = spacing.max(1.0);
    let cols = ((width as f32 / step).ceil() as usize).max(1);
    let rows = ((height as f32 / step).ceil() as usize).max(1);
    let count = cols * rows;

    let px = pixmap.as_bytes();
    let colour_at = |x: usize, y: usize| -> (f32, f32, f32) {
        let i = (y * width + x) * 4;
        (px[i] as f32, px[i + 1] as f32, px[i + 2] as f32)
    };

    let mut seeds: Vec<Seed> = (0..count)
        .map(|k| {
            let x = (((k % cols) as f32 + 0.5) * step).min(width as f32 - 1.0);
            let y = (((k / cols) as f32 + 0.5) * step).min(height as f32 - 1.0);
            let (r, g, b) = colour_at(x as usize, y as usize);
            Seed { r, g, b, x, y }
        })
        .collect();

    // Position is measured in units of `spacing` before `compactness` weighs
    // it, so that the two sliders stay independent of one another: changing how
    // big the regions are does not also change how tightly they are held.
    let pull = (compactness / step) * (compactness / step);
    let mut labels = vec![0u32; width * height];

    // Which way each region runs, and how far it may reach to do it. A region
    // stretched along its own axis can be claimed by a seed further away than
    // its own cell, so the search has to widen with it or the far end of the
    // stroke goes to a neighbour and the stretch never happens.
    let stretch = elongation.max(1.0);
    let lie = lie_of_the_land(pixmap, &seeds, step);
    let reach = stretch.ceil() as isize;

    for round in 0..ROUNDS {
        // Every pixel picks its own seed, which is why this is the parallel
        // half: the nine candidates are read-only for the length of the round.
        labels
            .par_chunks_exact_mut(width)
            .enumerate()
            .for_each(|(y, line)| {
                let cell_y = ((y as f32 / step) as isize).clamp(0, rows as isize - 1);
                for (x, slot) in line.iter_mut().enumerate() {
                    let cell_x = ((x as f32 / step) as isize).clamp(0, cols as isize - 1);
                    let i = (y * width + x) * 4;
                    let (r, g, b) = (px[i] as f32, px[i + 1] as f32, px[i + 2] as f32);
                    let mut best = f32::MAX;
                    let mut best_seed = 0u32;
                    for dy in -reach..=reach {
                        for dx in -reach..=reach {
                            let (cx, cy) = (cell_x + dx, cell_y + dy);
                            if cx < 0 || cy < 0 || cx >= cols as isize || cy >= rows as isize {
                                continue;
                            }
                            let k = cy as usize * cols + cx as usize;
                            let seed = seeds[k];
                            let (dr, dg, db) = (r - seed.r, g - seed.g, b - seed.b);
                            let (sx, sy) = (x as f32 - seed.x, y as f32 - seed.y);
                            // In the region's own frame: cheap along its
                            // length, dear across it. The two scalings are
                            // reciprocal so that stretching a region does not
                            // also enlarge it.
                            let (cos, sin) = lie[k];
                            let along = (sx * cos + sy * sin) / stretch;
                            let across = (sy * cos - sx * sin) * stretch;
                            let apart = dr * dr + dg * dg + db * db
                                + (along * along + across * across) * pull;
                            if apart < best {
                                best = apart;
                                best_seed = k as u32;
                            }
                        }
                    }
                    *slot = best_seed;
                }
            });
        if round + 1 == ROUNDS {
            break;
        }

        // Then each seed moves to the middle of what it took. A seed that took
        // nothing stays where it was rather than being dropped: the nine-cell
        // search means its neighbours are relying on it to be there.
        let mut totals = vec![[0f64; 5]; count];
        let mut tally = vec![0u32; count];
        for (p, &label) in labels.iter().enumerate() {
            let k = label as usize;
            let i = p * 4;
            totals[k][0] += px[i] as f64;
            totals[k][1] += px[i + 1] as f64;
            totals[k][2] += px[i + 2] as f64;
            totals[k][3] += (p % width) as f64;
            totals[k][4] += (p / width) as f64;
            tally[k] += 1;
        }
        for (k, seed) in seeds.iter_mut().enumerate() {
            if tally[k] == 0 {
                continue;
            }
            let n = tally[k] as f64;
            seed.r = (totals[k][0] / n) as f32;
            seed.g = (totals[k][1] / n) as f32;
            seed.b = (totals[k][2] / n) as f32;
            seed.x = (totals[k][3] / n) as f32;
            seed.y = (totals[k][4] / n) as f32;
        }
    }

    // Number the regions that actually took a pixel, from zero, so the caller
    // can index by them.
    let mut renumber = vec![u32::MAX; count];
    let mut found = 0u32;
    for label in labels.iter_mut() {
        let k = *label as usize;
        if renumber[k] == u32::MAX {
            renumber[k] = found;
            found += 1;
        }
        *label = renumber[k];
    }
    (labels, found as usize)
}

/// Value noise in canvas coordinates: one number per lattice point, smoothly
/// interpolated between them.
///
/// Seeded from the coordinates rather than from a generator, so the same pixel
/// gets the same number every time it is asked for — a preview, the commit
/// behind it and a replay of the history have to agree.
fn wobble(x: i32, y: i32, cell: i32, salt: u32) -> f32 {
    let point = |a: i32, b: i32| -> f32 {
        let mut h = (a as u32)
            .wrapping_mul(0x27d4_eb2d)
            ^ (b as u32).wrapping_mul(0x1656_67b1)
            ^ salt.wrapping_mul(0x9e37_79b9);
        h ^= h >> 15;
        h = h.wrapping_mul(0x2545_f491);
        h ^= h >> 13;
        (h % 1024) as f32 / 1023.0
    };
    let (cx, cy) = (x.div_euclid(cell), y.div_euclid(cell));
    let ease = |d: i32| {
        let t = d as f32 / cell as f32;
        t * t * (3.0 - 2.0 * t)
    };
    let (sx, sy) = (ease(x.rem_euclid(cell)), ease(y.rem_euclid(cell)));
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let top = lerp(point(cx, cy), point(cx + 1, cy), sx);
    let bottom = lerp(point(cx, cy + 1), point(cx + 1, cy + 1), sx);
    lerp(top, bottom, sy) * 2.0 - 1.0
}

/// Paint every region its own average colour, rounded onto a palette of
/// `rungs` even steps per channel between black and white, with every boundary
/// allowed to wander `wander` pixels off where it really is.
///
/// The wander is what makes the edges *ragged*, and a flattened picture
/// without it does not look scraped on — it looks like a median filter, which
/// is the note this pass was added on. Each pixel is painted with the colour of
/// a region a short way off rather than its own, the distance coming from a
/// noise field, so a boundary breaks into steps and kinks instead of curving.
/// Inside a mass it costs nothing: taking the colour of somewhere else in the
/// same flat area gives back the colour that was already there. Only the joins
/// move, which is the whole trick.
///
/// The rounding is not a detail. It is what makes neighbouring regions come
/// back as *one* mass: two greens a few levels apart round to the same green
/// and the join between them disappears, so what is left is a few large areas
/// whose boundaries wander along whatever the regions underneath happened to
/// do. That is the ragged edge a knife leaves, and it cannot be had from the
/// regions alone — a region boundary on its own is as smooth as the region is
/// compact. `rungs` of 255 leaves every region its own exact average.
///
/// Alpha is left as it was: which region a pixel belongs to says nothing about
/// whether the layer shows it.
pub(crate) fn flatten(
    pixmap: &mut Pixmap,
    labels: &[u32],
    count: usize,
    rungs: u32,
    wander: f32,
) {
    if count == 0 {
        return;
    }
    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;
    let mut totals = vec![[0u64; 3]; count];
    let mut tally = vec![0u32; count];
    for (p, chunk) in pixmap.as_bytes().chunks_exact(4).enumerate() {
        let region = labels[p] as usize;
        totals[region][0] += chunk[0] as u64;
        totals[region][1] += chunk[1] as u64;
        totals[region][2] += chunk[2] as u64;
        tally[region] += 1;
    }
    // Rounded to the nearest rung of the palette. The rungs are spaced so that
    // the top one is white: a palette that ran 0, 24, 48 … would stop at 240
    // and the picture would come back short of its own highlights.
    let rungs = rungs.max(1) as u64;
    let round = |sum: u64, n: u64| -> u8 {
        let mean = (sum + n / 2) / n;
        let rung = (mean * rungs + 127) / 255;
        ((rung * 255 + rungs / 2) / rungs) as u8
    };
    let mean: Vec<[u8; 3]> = totals
        .iter()
        .zip(&tally)
        .map(|(sum, &n)| {
            let n = n.max(1) as u64;
            [
                round(sum[0], n),
                round(sum[1], n),
                round(sum[2], n),
            ]
        })
        .collect();
    // The lattice the wander is drawn on. Wider than the wander itself, so a
    // boundary leaves in one direction and comes back in another over a run of
    // several pixels: a stretch of edge with a kink at each end, rather than a
    // fringe of single pixels, which is what an unsmoothed noise would give and
    // is not what a knife leaves.
    let cell = ((wander * 3.0).round() as i32).max(2);
    pixmap
        .as_bytes_mut()
        .par_chunks_exact_mut(width as usize * 4)
        .enumerate()
        .for_each(|(row, line)| {
            let y = row as i32;
            for (x, chunk) in line.chunks_exact_mut(4).enumerate() {
                let x = x as i32;
                let (sx, sy) = if wander <= 0.0 {
                    (x, y)
                } else {
                    (
                        (x + (wobble(x, y, cell, 1) * wander).round() as i32).clamp(0, width - 1),
                        (y + (wobble(x, y, cell, 2) * wander).round() as i32).clamp(0, height - 1),
                    )
                };
                let colour = mean[labels[(sy * width + sx) as usize] as usize];
                chunk[..3].copy_from_slice(&colour);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Rgba8;

    fn two_fields(width: i32) -> Pixmap {
        let mut pm = Pixmap::new(width as u32, 64);
        for y in 0..64 {
            for x in 0..width {
                let v = if x < width / 2 { 40 } else { 200 };
                pm.set(x, y, Rgba8::new(v, v, v, 255));
            }
        }
        pm
    }

    /// A region stops at a boundary rather than straddling it: that is the
    /// whole of what the colour half of the distance is for.
    #[test]
    fn a_region_does_not_straddle_a_hard_edge() {
        let pm = two_fields(64);
        let (labels, _) = regions(&pm, 16.0, 10.0, 1.0);
        let at = |x: usize, y: usize| labels[y * 64 + x];
        assert_eq!(at(20, 32), at(28, 32), "one flat field came back in two");
        assert_ne!(at(31, 32), at(32, 32), "a region straddled the edge");
    }

    /// Nearness bounds a region even when the picture gives it no reason to
    /// stop. This is the failure that sent the first version of this module to
    /// the bin: on colour alone, a whole flat background is one region.
    #[test]
    fn a_flat_picture_still_comes_back_in_many_regions() {
        let pm = Pixmap::filled(128, 128, Rgba8::new(90, 120, 60, 255));
        let (_, count) = regions(&pm, 16.0, 10.0, 1.0);
        assert!(
            count >= 48,
            "a flat picture came back in {count} regions at a spacing of 16"
        );
    }

    /// Spacing is how big the regions come out.
    #[test]
    fn a_wider_spacing_leaves_fewer_regions() {
        let pm = two_fields(128);
        let coarse = regions(&pm, 32.0, 10.0, 1.0).1;
        let fine = regions(&pm, 8.0, 10.0, 1.0).1;
        assert!(
            coarse * 4 < fine,
            "a coarse cut left about as many regions as a fine one: {coarse} against {fine}"
        );
    }

    /// Compactness is how much nearness counts against likeness. Held tightly,
    /// the regions come back as blocks and cut across what is in the picture;
    /// held loosely, they follow it.
    #[test]
    fn compactness_decides_whether_a_region_follows_the_picture() {
        // A diagonal edge, which no grid of blocks can follow.
        let mut pm = Pixmap::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                let v = if x < y { 40 } else { 200 };
                pm.set(x, y, Rgba8::new(v, v, v, 255));
            }
        }
        // How far the picture has to move to be flattened. A region that
        // follows the edge holds one colour and nothing moves; a region that
        // cuts across it has to average the two sides and everything in it
        // moves. Counting the regions that *straddle* the edge does not
        // measure this, which is what the first version of the test tried:
        // k-means walks a seed onto one side or the other whatever it is
        // holding, so the straddlers come back as none either way and the
        // difference is in what shape they leave behind.
        let strain = |compactness| {
            let (labels, count) = regions(&pm, 16.0, compactness, 1.0);
            let mut flat = pm.clone();
            flatten(&mut flat, &labels, count, 255, 0.0);
            (0..64)
                .map(|y| {
                    (0..64)
                        .map(|x| (flat.get(x, y).r as i32 - pm.get(x, y).r as i32).abs() as u32)
                        .sum::<u32>()
                })
                .sum::<u32>()
        };
        assert!(
            strain(5.0) * 2 < strain(1000.0),
            "holding the regions tightly followed the edge as well as holding them loosely: {} against {}",
            strain(5.0),
            strain(1000.0)
        );
    }

    /// Flattening puts one colour through a region and leaves alpha alone.
    #[test]
    fn flattening_lays_one_colour_through_a_region() {
        let mut pm = Pixmap::new(16, 16);
        for y in 0..16 {
            for x in 0..16 {
                let v = (100 + x) as u8;
                pm.set(x, y, Rgba8::new(v, v, v, 77));
            }
        }
        let (labels, count) = regions(&pm, 64.0, 10.0, 1.0);
        assert_eq!(count, 1);
        flatten(&mut pm, &labels, count, 255, 0.0);
        let first = pm.get(0, 0);
        assert!((0..16).all(|x| pm.get(x, 8).r == first.r));
        // 100 through 115 averages 107.5, and a half rounds up.
        assert_eq!(first.r, 108, "the region did not come back as its average");
        assert!(pm.as_bytes().chunks_exact(4).all(|p| p[3] == 77));
    }

    /// The palette is what fuses neighbouring regions into one mass: two
    /// greens a few levels apart round the same way and the join between them
    /// stops existing.
    #[test]
    fn a_coarse_palette_leaves_fewer_colours_than_there_were_regions() {
        // Twelve bands a few levels apart — near enough that a coarse palette
        // has to fuse them, far enough that a fine one keeps them.
        let mut pm = Pixmap::new(96, 96);
        for y in 0..96 {
            for x in 0..96 {
                let v = (100 + (x / 8) * 4) as u8;
                pm.set(x, y, Rgba8::new(v, v, v, 255));
            }
        }
        let shades = |rungs| {
            let mut flat = pm.clone();
            let (labels, count) = regions(&flat, 8.0, 10.0, 1.0);
            flatten(&mut flat, &labels, count, rungs, 0.0);
            let mut seen: Vec<u8> = flat.as_bytes().chunks_exact(4).map(|p| p[0]).collect();
            seen.sort_unstable();
            seen.dedup();
            seen.len()
        };
        assert!(
            shades(8) < shades(255),
            "a coarse palette carried as many colours as a fine one: {} against {}",
            shades(8),
            shades(255)
        );
    }

    /// A palette reaches white at the top. A run of even steps from zero would
    /// stop short of it and the picture would come back without its
    /// highlights.
    #[test]
    fn the_top_of_the_palette_is_white() {
        let mut pm = Pixmap::filled(16, 16, Rgba8::new(255, 250, 245, 255));
        let (labels, count) = regions(&pm, 64.0, 10.0, 1.0);
        flatten(&mut pm, &labels, count, 6, 0.0);
        assert_eq!(pm.get(8, 8).r, 255);
    }

    /// The wander tears the boundary and leaves everything either side of it
    /// exactly as it was. Both halves matter: a ragged edge that also stirred
    /// up the flat masses would be a noise filter, not a knife.
    #[test]
    fn a_wandering_boundary_is_ragged_and_costs_the_masses_nothing() {
        let pm = two_fields(64);
        let (labels, count) = regions(&pm, 16.0, 10.0, 1.0);
        let torn = |wander| {
            let mut flat = pm.clone();
            flatten(&mut flat, &labels, count, 255, wander);
            flat
        };
        let edge_of = |pm: &Pixmap, y: i32| (0..64).find(|&x| pm.get(x, y).r > 120).unwrap_or(0);

        let straight = torn(0.0);
        assert!(
            (0..64).all(|y| edge_of(&straight, y) == 32),
            "the boundary was not straight to begin with"
        );

        let ragged = torn(4.0);
        let wobbles = (0..64).filter(|&y| edge_of(&ragged, y) != 32).count();
        assert!(
            wobbles > 16,
            "only {wobbles} of 64 rows had the boundary torn"
        );
        // Ten pixels in from either side, nothing has moved.
        assert!(
            (0..64).all(|y| ragged.get(10, y).r == straight.get(10, y).r
                && ragged.get(54, y).r == straight.get(54, y).r),
            "the wander stirred up the middle of a flat mass"
        );
    }

    #[test]
    fn an_empty_pixmap_has_no_regions() {
        let pm = Pixmap::new(0, 0);
        assert_eq!(regions(&pm, 16.0, 10.0, 1.0).1, 0);
    }
}

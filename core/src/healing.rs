//! Healing — reconstructing a region from the pixels around it.
//!
//! The Spot Healing Brush does not paint a colour. It removes what the brush
//! covered and rebuilds it from the surroundings, which is a very different
//! operation from a stroke: it needs the *original* pixels around the hole, so
//! it runs once when the stroke ends rather than dab by dab.
//!
//! CS6's options bar offers three types, all implemented here:
//!
//! * **Proximity Match** — smooth interpolation inward from the boundary.
//!   Solves Laplace's equation over the hole with the surrounding ring as a
//!   fixed boundary, which is the right answer for a blemish on skin, sky or
//!   any other gradient: the fill continues the shading with no visible seam.
//! * **Create Texture** — the same smooth base, plus noise matched to the
//!   roughness of the ring, so the patch does not read as suspiciously clean
//!   against grainy surroundings.
//! * **Content-Aware** — patch synthesis. Fills from the boundary inward,
//!   copying from whichever nearby patch best matches what is already known
//!   around each pixel, so structure and texture carry across the hole
//!   instead of being smoothed away.

use crate::buffer::{Pixmap, Rect, Rgba8};

/// Which reconstruction to use. Mirrors CS6's Type buttons.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(i32)]
pub enum HealMode {
    ProximityMatch = 0,
    CreateTexture = 1,
    /// CS6's default.
    #[default]
    ContentAware = 2,
}

impl HealMode {
    pub fn from_i32(v: i32) -> HealMode {
        match v {
            0 => HealMode::ProximityMatch,
            1 => HealMode::CreateTexture,
            _ => HealMode::ContentAware,
        }
    }
}

/// Coverage at or above which a pixel counts as inside the hole.
///
/// A soft brush tapers, and everything under the taper still gets *blended*
/// toward the fill — but only the solid core is treated as unknown and rebuilt.
/// Rebuilding the taper too would make the healed area noticeably larger than
/// the brush.
const HOLE_THRESHOLD: f32 = 0.5;

/// How far outside the hole the boundary data is read from.
const BORDER: u32 = 4;

/// Relaxation sweeps for the Laplace solve.
///
/// Convergence is quick for the small regions a spot brush covers, and the eye
/// cannot see the last fraction of a percent of residual — this is a cosmetic
/// fill, not a simulation.
const RELAX_SWEEPS: usize = 96;

/// Patch size for content-aware synthesis, as a radius. 2 gives a 5×5 patch.
const PATCH_RADIUS: i32 = 2;

/// How far from a hole pixel content-aware search looks for a source patch.
const SEARCH_RADIUS: i32 = 24;

/// Heal the region of `pixels` marked by `coverage`.
///
/// `region` is in pixmap coordinates and `coverage` holds
/// `region.width * region.height` values in `0.0..=1.0`, row-major — normally a
/// brush-stroke mask. Returns the rectangle actually modified, empty when there
/// was nothing to do.
///
/// The pixmap is edited in place, and the pixels outside the hole are read but
/// never written, so a caller can hand this the layer's own buffer.
pub fn heal_region(pixels: &mut Pixmap, region: Rect, coverage: &[f32], mode: HealMode) -> Rect {
    if coverage.len() != (region.width as usize) * (region.height as usize) {
        debug_assert!(false, "coverage size does not match the region");
        return Rect::default();
    }

    // Work over the hole plus a ring of surrounding pixels to read from.
    let canvas = pixels.rect();
    let work = region.inflate(BORDER).intersect(&canvas);
    if work.is_empty() {
        return Rect::default();
    }

    let w = work.width as usize;
    let h = work.height as usize;
    let at = |x: i32, y: i32| -> usize {
        ((y - work.y) as usize) * w + (x - work.x) as usize
    };

    // Lift the working area into floats, and record which pixels are unknown.
    let mut rgba = vec![[0.0f32; 4]; w * h];
    let mut alpha_cov = vec![0.0f32; w * h];
    let mut hole = vec![false; w * h];
    let mut hole_count = 0usize;

    for y in work.y..work.bottom() {
        for x in work.x..work.right() {
            let index = at(x, y);
            let px = pixels.get(x, y);
            rgba[index] = [px.r as f32, px.g as f32, px.b as f32, px.a as f32];

            // Coverage only exists inside `region`; the surrounding ring is
            // known by definition.
            if region.contains(x, y) {
                let c = coverage[((y - region.y) as usize) * (region.width as usize)
                    + (x - region.x) as usize]
                    .clamp(0.0, 1.0);
                alpha_cov[index] = c;
                if c >= HOLE_THRESHOLD {
                    hole[index] = true;
                    hole_count += 1;
                }
            }
        }
    }

    if hole_count == 0 {
        return Rect::default();
    }

    // A hole that reaches every edge of the working area has no boundary to
    // reconstruct from, so there is nothing meaningful to do.
    if hole.iter().all(|&h| h) {
        return Rect::default();
    }

    let filled = match mode {
        HealMode::ProximityMatch => laplace_fill(&rgba, &hole, w, h),
        HealMode::CreateTexture => {
            let mut smooth = laplace_fill(&rgba, &hole, w, h);
            add_matched_noise(&mut smooth, &rgba, &hole, w, h);
            smooth
        }
        HealMode::ContentAware => content_aware_fill(&rgba, &hole, w, h),
    };

    // Blend by coverage so a soft brush edge fades into the original.
    for y in work.y..work.bottom() {
        for x in work.x..work.right() {
            let index = at(x, y);
            let t = alpha_cov[index];
            if t <= 0.0 {
                continue;
            }
            let src = rgba[index];
            let dst = filled[index];
            let mix = |a: f32, b: f32| (a + (b - a) * t).round().clamp(0.0, 255.0) as u8;
            pixels.set(
                x,
                y,
                Rgba8::new(
                    mix(src[0], dst[0]),
                    mix(src[1], dst[1]),
                    mix(src[2], dst[2]),
                    mix(src[3], dst[3]),
                ),
            );
        }
    }

    region.intersect(&canvas)
}

/// Solve Laplace's equation over the hole, with the known pixels as boundary.
///
/// Gauss-Seidel relaxation: each unknown becomes the average of its four
/// neighbours, swept repeatedly. Seeded with the mean of the boundary so it
/// starts somewhere sensible and converges in far fewer sweeps than starting
/// from black would.
fn laplace_fill(rgba: &[[f32; 4]], hole: &[bool], w: usize, h: usize) -> Vec<[f32; 4]> {
    let mut out = rgba.to_vec();

    // Mean of the known pixels immediately around the hole.
    let mut sum = [0.0f32; 4];
    let mut ring = 0usize;
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if hole[i] || !touches_hole(hole, w, h, x, y) {
                continue;
            }
            for c in 0..4 {
                sum[c] += rgba[i][c];
            }
            ring += 1;
        }
    }
    if ring == 0 {
        return out;
    }
    let mean = [
        sum[0] / ring as f32,
        sum[1] / ring as f32,
        sum[2] / ring as f32,
        sum[3] / ring as f32,
    ];
    for (i, px) in out.iter_mut().enumerate() {
        if hole[i] {
            *px = mean;
        }
    }

    for _ in 0..RELAX_SWEEPS {
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if !hole[i] {
                    continue;
                }
                let mut acc = [0.0f32; 4];
                let mut n = 0.0f32;
                // Neighbours outside the working area are simply skipped, which
                // is a zero-flux edge — the fill flattens toward the border
                // rather than being pulled to some arbitrary value.
                if x > 0 {
                    add(&mut acc, &out[i - 1]);
                    n += 1.0;
                }
                if x + 1 < w {
                    add(&mut acc, &out[i + 1]);
                    n += 1.0;
                }
                if y > 0 {
                    add(&mut acc, &out[i - w]);
                    n += 1.0;
                }
                if y + 1 < h {
                    add(&mut acc, &out[i + w]);
                    n += 1.0;
                }
                if n > 0.0 {
                    for c in 0..4 {
                        out[i][c] = acc[c] / n;
                    }
                }
            }
        }
    }
    out
}

fn add(acc: &mut [f32; 4], px: &[f32; 4]) {
    for c in 0..4 {
        acc[c] += px[c];
    }
}

/// True when a known pixel is 4-adjacent to the hole.
fn touches_hole(hole: &[bool], w: usize, h: usize, x: usize, y: usize) -> bool {
    (x > 0 && hole[y * w + x - 1])
        || (x + 1 < w && hole[y * w + x + 1])
        || (y > 0 && hole[(y - 1) * w + x])
        || (y + 1 < h && hole[(y + 1) * w + x])
}

/// Add noise to a smooth fill, matched to the roughness of the surroundings.
///
/// The standard deviation comes from the known pixels' deviation from their own
/// local mean, so a smooth area gets almost nothing and a grainy one gets grain
/// of the right strength.
fn add_matched_noise(
    fill: &mut [[f32; 4]],
    rgba: &[[f32; 4]],
    hole: &[bool],
    w: usize,
    h: usize,
) {
    let mut sum_sq = [0.0f32; 3];
    let mut n = 0.0f32;
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let i = y * w + x;
            if hole[i] {
                continue;
            }
            // Local mean over the 4-neighbourhood; skip anywhere the hole
            // intrudes, so the hole's own flat fill is not measured.
            if hole[i - 1] || hole[i + 1] || hole[i - w] || hole[i + w] {
                continue;
            }
            for c in 0..3 {
                let local = (rgba[i - 1][c] + rgba[i + 1][c] + rgba[i - w][c] + rgba[i + w][c])
                    / 4.0;
                let d = rgba[i][c] - local;
                sum_sq[c] += d * d;
            }
            n += 1.0;
        }
    }
    if n < 1.0 {
        return;
    }

    let sigma = [
        (sum_sq[0] / n).sqrt(),
        (sum_sq[1] / n).sqrt(),
        (sum_sq[2] / n).sqrt(),
    ];

    // A fixed-seed LCG: reproducible, so the same stroke heals the same way
    // twice and the tests are not flaky.
    let mut state: u32 = 0x9E3779B9;
    let mut next = || -> f32 {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        // Two draws averaged, which is closer to a bell than a flat spread.
        let a = ((state >> 8) & 0xFFFF) as f32 / 65535.0;
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let b = ((state >> 8) & 0xFFFF) as f32 / 65535.0;
        (a + b) - 1.0
    };

    for i in 0..fill.len() {
        if !hole[i] {
            continue;
        }
        for c in 0..3 {
            fill[i][c] = (fill[i][c] + next() * sigma[c]).clamp(0.0, 255.0);
        }
    }
}

/// Patch-based fill, working inward from the boundary.
///
/// Repeatedly takes the hole pixels that already have known neighbours, and for
/// each searches nearby known pixels for the patch whose surroundings best
/// match. Filling from the outside in ("onion peeling") means every pixel is
/// decided with as much context as possible, which is what lets an edge running
/// into the hole continue across it.
fn content_aware_fill(rgba: &[[f32; 4]], hole: &[bool], w: usize, h: usize) -> Vec<[f32; 4]> {
    let mut out = rgba.to_vec();
    let mut unknown = hole.to_vec();
    let mut remaining = unknown.iter().filter(|&&u| u).count();

    // Candidate sources: known pixels far enough from any edge to have a full
    // patch around them.
    let mut sources: Vec<(i32, i32)> = Vec::new();
    for y in PATCH_RADIUS..(h as i32 - PATCH_RADIUS) {
        for x in PATCH_RADIUS..(w as i32 - PATCH_RADIUS) {
            if !hole[(y as usize) * w + x as usize] {
                sources.push((x, y));
            }
        }
    }
    if sources.is_empty() {
        return laplace_fill(rgba, hole, w, h);
    }

    // Bounded so a large brush cannot turn one stroke into a long stall. Each
    // pass fills at least the current boundary layer, so this cannot spin.
    let max_passes = w.max(h) + 2;
    let mut passes = 0;

    while remaining > 0 && passes < max_passes {
        passes += 1;

        // This pass's boundary layer, resolved against a snapshot so pixels
        // filled during the pass do not become sources within it.
        let layer: Vec<(i32, i32)> = (0..h as i32)
            .flat_map(|y| (0..w as i32).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                unknown[(y as usize) * w + x as usize]
                    && touches_known(&unknown, w, h, x as usize, y as usize)
            })
            .collect();

        if layer.is_empty() {
            break;
        }

        let mut resolved: Vec<((i32, i32), [f32; 4])> = Vec::with_capacity(layer.len());
        for &(hx, hy) in &layer {
            let mut best = f32::MAX;
            let mut best_value = out[(hy as usize) * w + hx as usize];

            for &(sx, sy) in &sources {
                if (sx - hx).abs() > SEARCH_RADIUS || (sy - hy).abs() > SEARCH_RADIUS {
                    continue;
                }
                // Sum of squared differences over the part of the patch that is
                // already known, so the match is judged only on real data.
                let mut cost = 0.0f32;
                let mut counted = 0;
                for dy in -PATCH_RADIUS..=PATCH_RADIUS {
                    for dx in -PATCH_RADIUS..=PATCH_RADIUS {
                        let (px, py) = (hx + dx, hy + dy);
                        if px < 0 || py < 0 || px >= w as i32 || py >= h as i32 {
                            continue;
                        }
                        let pi = (py as usize) * w + px as usize;
                        if unknown[pi] {
                            continue;
                        }
                        let qi = ((sy + dy) as usize) * w + (sx + dx) as usize;
                        for c in 0..3 {
                            let d = out[pi][c] - out[qi][c];
                            cost += d * d;
                        }
                        counted += 1;
                    }
                }
                if counted == 0 {
                    continue;
                }
                let cost = cost / counted as f32;
                if cost < best {
                    best = cost;
                    best_value = out[(sy as usize) * w + sx as usize];
                }
            }
            resolved.push(((hx, hy), best_value));
        }

        for ((x, y), value) in resolved {
            let i = (y as usize) * w + x as usize;
            out[i] = value;
            unknown[i] = false;
            remaining -= 1;
        }
    }

    // Anything the passes could not reach falls back to the smooth solve rather
    // than being left as it was.
    if remaining > 0 {
        let leftover = laplace_fill(&out, &unknown, w, h);
        for i in 0..out.len() {
            if unknown[i] {
                out[i] = leftover[i];
            }
        }
    }
    out
}

/// True when an unknown pixel is 4-adjacent to something already known.
fn touches_known(unknown: &[bool], w: usize, h: usize, x: usize, y: usize) -> bool {
    (x > 0 && !unknown[y * w + x - 1])
        || (x + 1 < w && !unknown[y * w + x + 1])
        || (y > 0 && !unknown[(y - 1) * w + x])
        || (y + 1 < h && !unknown[(y + 1) * w + x])
}

/// How much of the source to transfer.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Transfer {
    /// Texture and colour, with the destination's lighting. The default.
    #[default]
    Full,
    /// Texture only: the destination keeps its own colour, and just takes the
    /// source's detail. This is what the Patch tool's **Transparent** option
    /// does — useful for patching over something that has to stay the colour it
    /// already is, like a stain on a coloured wall.
    TextureOnly,
}

/// Paste the pixels at `source` into the covered region, keeping the
/// destination's own lighting — the Healing Brush and the Patch tool.
///
/// This is a **Poisson solve**, not a copy. What gets transplanted is the
/// source's *gradient* — its texture and detail — while the destination's
/// existing pixels around the region are held fixed as the boundary. The result
/// carries the source's grain and structure but takes its overall brightness and
/// colour from where it lands, which is why a healed patch has no visible seam
/// even when source and destination differ in tone.
///
/// A plain copy is what the Clone Stamp does, and it is exactly the visible-edge
/// problem the healing family exists to avoid.
///
/// `source` is the offset added to a destination pixel to find its source, so
/// `(-40, 0)` samples forty pixels to the left. Returns the rectangle modified.
pub fn clone_region(
    pixels: &mut Pixmap,
    region: Rect,
    coverage: &[f32],
    source: (i32, i32),
    transfer: Transfer,
) -> Rect {
    if coverage.len() != (region.width as usize) * (region.height as usize) {
        debug_assert!(false, "coverage size does not match the region");
        return Rect::default();
    }
    if source == (0, 0) {
        // Sampling from itself would be a no-op solve; treat it as unset rather
        // than grinding through the relaxation to change nothing.
        return Rect::default();
    }

    let canvas = pixels.rect();
    let work = region.inflate(BORDER).intersect(&canvas);
    if work.is_empty() {
        return Rect::default();
    }

    let w = work.width as usize;
    let h = work.height as usize;
    let at = |x: i32, y: i32| ((y - work.y) as usize) * w + (x - work.x) as usize;

    let mut dest = vec![[0.0f32; 4]; w * h];
    let mut src = vec![[0.0f32; 4]; w * h];
    let mut cov = vec![0.0f32; w * h];
    let mut hole = vec![false; w * h];
    let mut hole_count = 0usize;

    for y in work.y..work.bottom() {
        for x in work.x..work.right() {
            let index = at(x, y);
            let d = pixels.get(x, y);
            dest[index] = [d.r as f32, d.g as f32, d.b as f32, d.a as f32];

            // Source pixels outside the canvas read as transparent, which would
            // drag the gradient toward nothing; clamp to the edge instead.
            let sx = (x + source.0).clamp(0, canvas.right() - 1);
            let sy = (y + source.1).clamp(0, canvas.bottom() - 1);
            let sp = pixels.get(sx, sy);
            src[index] = [sp.r as f32, sp.g as f32, sp.b as f32, sp.a as f32];

            if region.contains(x, y) {
                let c = coverage[((y - region.y) as usize) * (region.width as usize)
                    + (x - region.x) as usize]
                    .clamp(0.0, 1.0);
                cov[index] = c;
                if c >= HOLE_THRESHOLD {
                    hole[index] = true;
                    hole_count += 1;
                }
            }
        }
    }

    if hole_count == 0 || hole.iter().all(|&h| h) {
        return Rect::default();
    }

    // Gauss-Seidel on the Poisson equation. Each unknown becomes the average of
    // its neighbours *plus* the average difference to its neighbours in the
    // source — that second term is what carries the source's detail over.
    let mut out = dest.clone();
    for i in 0..out.len() {
        if hole[i] {
            out[i] = src[i];
        }
    }

    for _ in 0..RELAX_SWEEPS {
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if !hole[i] {
                    continue;
                }
                let mut acc = [0.0f32; 4];
                let mut n = 0.0f32;
                let neighbour = |j: usize, acc: &mut [f32; 4]| {
                    for c in 0..4 {
                        acc[c] += out[j][c] + (src[i][c] - src[j][c]);
                    }
                };
                if x > 0 {
                    neighbour(i - 1, &mut acc);
                    n += 1.0;
                }
                if x + 1 < w {
                    neighbour(i + 1, &mut acc);
                    n += 1.0;
                }
                if y > 0 {
                    neighbour(i - w, &mut acc);
                    n += 1.0;
                }
                if y + 1 < h {
                    neighbour(i + w, &mut acc);
                    n += 1.0;
                }
                if n > 0.0 {
                    for c in 0..4 {
                        out[i][c] = (acc[c] / n).clamp(0.0, 255.0);
                    }
                }
            }
        }
    }

    if transfer == Transfer::TextureOnly {
        keep_destination_colour(&mut out, &dest, &hole);
    }

    write_back(pixels, work, &dest, &out, &cov);
    region.intersect(&canvas)
}

/// Rescale a solved result to the destination's own colour, keeping only its
/// luminance.
///
/// The solved pixel supplies the brightness — which is where texture and detail
/// live — while the destination's channel ratios are preserved, so hue and
/// saturation come from where the patch lands rather than from the source.
fn keep_destination_colour(solved: &mut [[f32; 4]], dest: &[[f32; 4]], hole: &[bool]) {
    const LUMA: [f32; 3] = [0.299, 0.587, 0.114];
    for i in 0..solved.len() {
        if !hole[i] {
            continue;
        }
        let luma = |px: &[f32; 4]| LUMA[0] * px[0] + LUMA[1] * px[1] + LUMA[2] * px[2];
        let want = luma(&solved[i]);
        let have = luma(&dest[i]);
        if have <= 1.0 {
            // Near-black has no colour to preserve; leave the solve alone rather
            // than scaling by a huge factor.
            continue;
        }
        let scale = want / have;
        for c in 0..3 {
            solved[i][c] = (dest[i][c] * scale).clamp(0.0, 255.0);
        }
        solved[i][3] = dest[i][3];
    }
}

/// Move the covered region by `(dx, dy)`, healing the hole it leaves behind —
/// the Content-Aware Move tool.
///
/// With `extend` set the original is left in place, so the region is duplicated
/// rather than moved. That is CS6's Extend mode, used for lengthening things
/// like a wall or a branch.
///
/// The moved copy is blended with a Poisson solve so it settles into its new
/// surroundings, and the vacated area is rebuilt content-aware.
pub fn move_region(
    pixels: &mut Pixmap,
    region: Rect,
    coverage: &[f32],
    dx: i32,
    dy: i32,
    extend: bool,
) -> Rect {
    if coverage.len() != (region.width as usize) * (region.height as usize) {
        debug_assert!(false, "coverage size does not match the region");
        return Rect::default();
    }
    if (dx, dy) == (0, 0) {
        return Rect::default();
    }

    let canvas = pixels.rect();
    let destination = Rect::new(region.x + dx, region.y + dy, region.width, region.height);
    if destination.intersect(&canvas).is_empty() {
        return Rect::default();
    }

    // Drop the moved copy in first, sampling backwards from where it came.
    let placed = clone_region(pixels, destination, coverage, (-dx, -dy), Transfer::Full);
    if placed.is_empty() {
        return Rect::default();
    }

    if extend {
        return destination.intersect(&canvas);
    }

    // Then close the hole. Content-aware, because what belongs there is
    // whatever the surroundings continue into — the same question the Spot
    // Healing Brush asks.
    let filled = heal_region(pixels, region, coverage, HealMode::ContentAware);
    destination.intersect(&canvas).union(&filled)
}

/// Take the red out of a red-eye flash reflection.
///
/// `pupil` is CS6's Pupil Size (0–100): how aggressively a pixel counts as red.
/// `darken` is its Darken Amount (0–100), applied to whatever is neutralised.
///
/// Only pixels where red genuinely dominates are touched, so this can be
/// dragged loosely over an eye without draining the skin around it.
pub fn red_eye_region(
    pixels: &mut Pixmap,
    region: Rect,
    coverage: &[f32],
    pupil: u32,
    darken: u32,
) -> Rect {
    if coverage.len() != (region.width as usize) * (region.height as usize) {
        debug_assert!(false, "coverage size does not match the region");
        return Rect::default();
    }
    let work = region.intersect(&pixels.rect());
    if work.is_empty() {
        return Rect::default();
    }

    // Pupil Size widens the net: at 0 only strongly red pixels qualify, at 100
    // anything where red merely leads.
    let ratio = 1.8 - (pupil.min(100) as f32 / 100.0) * 0.75;
    let darken = (darken.min(100) as f32) / 100.0;

    let mut touched = false;
    for y in work.y..work.bottom() {
        for x in work.x..work.right() {
            let c = coverage[((y - region.y) as usize) * (region.width as usize)
                + (x - region.x) as usize]
                .clamp(0.0, 1.0);
            if c <= 0.0 {
                continue;
            }

            let px = pixels.get(x, y);
            let (r, g, b) = (px.r as f32, px.g as f32, px.b as f32);
            let other = g.max(b).max(1.0);
            if r < other * ratio {
                continue;
            }

            // Replace red with the green/blue level, which is what the pupil
            // would have been without the flash, then darken.
            let neutral = (g + b) / 2.0;
            let target = neutral * (1.0 - darken * 0.6);
            let mix = |from: f32, to: f32| {
                (from + (to - from) * c).round().clamp(0.0, 255.0) as u8
            };
            pixels.set(
                x,
                y,
                Rgba8::new(
                    mix(r, target),
                    mix(g, g * (1.0 - darken * 0.6)),
                    mix(b, b * (1.0 - darken * 0.6)),
                    px.a,
                ),
            );
            touched = true;
        }
    }

    if touched {
        work
    } else {
        Rect::default()
    }
}

/// Blend a solved result back over the original, weighted by coverage.
fn write_back(
    pixels: &mut Pixmap,
    work: Rect,
    original: &[[f32; 4]],
    solved: &[[f32; 4]],
    coverage: &[f32],
) {
    let w = work.width as usize;
    for y in work.y..work.bottom() {
        for x in work.x..work.right() {
            let index = ((y - work.y) as usize) * w + (x - work.x) as usize;
            let t = coverage[index];
            if t <= 0.0 {
                continue;
            }
            let (a, b) = (original[index], solved[index]);
            let mix = |p: f32, q: f32| (p + (q - p) * t).round().clamp(0.0, 255.0) as u8;
            pixels.set(
                x,
                y,
                Rgba8::new(
                    mix(a[0], b[0]),
                    mix(a[1], b[1]),
                    mix(a[2], b[2]),
                    mix(a[3], b[3]),
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A disc of coverage centred in `region`, as a soft brush would leave.
    fn disc(region: Rect, radius: f32) -> Vec<f32> {
        let cx = region.x as f32 + region.width as f32 / 2.0;
        let cy = region.y as f32 + region.height as f32 / 2.0;
        let mut out = vec![0.0f32; (region.width as usize) * (region.height as usize)];
        for y in 0..region.height as i32 {
            for x in 0..region.width as i32 {
                let px = region.x as f32 + x as f32 + 0.5;
                let py = region.y as f32 + y as f32 + 0.5;
                let d = (px - cx).hypot(py - cy);
                if d <= radius {
                    out[(y as usize) * (region.width as usize) + x as usize] = 1.0;
                }
            }
        }
        out
    }

    fn all_ones(region: Rect) -> Vec<f32> {
        vec![1.0f32; (region.width as usize) * (region.height as usize)]
    }

    #[test]
    fn a_blemish_on_a_flat_field_disappears() {
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(180, 150, 130, 255));
        // The spot to remove.
        for y in 28..36 {
            for x in 28..36 {
                pm.set(x, y, Rgba8::new(90, 40, 40, 255));
            }
        }

        let region = Rect::new(26, 26, 12, 12);
        let dirty = heal_region(&mut pm, region, &disc(region, 6.0), HealMode::ProximityMatch);
        assert!(!dirty.is_empty());

        // The centre should now match its surroundings closely.
        let px = pm.get(32, 32);
        assert!(
            (px.r as i32 - 180).abs() <= 3
                && (px.g as i32 - 150).abs() <= 3
                && (px.b as i32 - 130).abs() <= 3,
            "healed centre is {:?}, wanted about (180, 150, 130)",
            px
        );
    }

    #[test]
    fn proximity_match_continues_a_gradient() {
        // A horizontal ramp: the fill should carry the ramp across, not flatten
        // it to the average.
        let mut pm = Pixmap::new(64, 32);
        for y in 0..32 {
            for x in 0..64 {
                let v = (x * 4).min(255) as u8;
                pm.set(x, y, Rgba8::new(v, v, v, 255));
            }
        }
        // Blot out a square.
        for y in 12..20 {
            for x in 20..28 {
                pm.set(x, y, Rgba8::new(0, 255, 0, 255));
            }
        }

        let region = Rect::new(19, 11, 10, 10);
        heal_region(&mut pm, region, &all_ones(region), HealMode::ProximityMatch);

        // At x = 24 the ramp is 96; a flat fill would land near the ring mean
        // and the green would be gone either way, so check the value tracks x.
        let left = pm.get(21, 16).r as i32;
        let right = pm.get(27, 16).r as i32;
        assert!(right > left + 10, "gradient was flattened: {} then {}", left, right);
        assert_eq!(pm.get(24, 16).g, pm.get(24, 16).r, "green cast survived");
    }

    #[test]
    fn content_aware_carries_an_edge_across_the_hole() {
        // Two-tone image with a hard vertical edge at x = 32, and a hole
        // straddling it. Proximity match would blur the edge; content-aware
        // should keep both sides distinct.
        let mut pm = Pixmap::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                let c = if x < 32 { 30u8 } else { 220u8 };
                pm.set(x, y, Rgba8::new(c, c, c, 255));
            }
        }
        let region = Rect::new(26, 26, 12, 12);
        let coverage = disc(region, 5.0);
        heal_region(&mut pm, region, &coverage, HealMode::ContentAware);

        let dark = pm.get(29, 32).r as i32;
        let light = pm.get(35, 32).r as i32;
        assert!(dark < 100, "left of the edge came out at {}", dark);
        assert!(light > 150, "right of the edge came out at {}", light);
    }

    #[test]
    fn create_texture_adds_grain_where_the_surroundings_have_grain() {
        // A noisy field. The healed area should not be perfectly flat.
        let mut pm = Pixmap::new(64, 64);
        let mut state: u32 = 12345;
        for y in 0..64 {
            for x in 0..64 {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let v = 120 + ((state >> 16) % 40) as u8;
                pm.set(x, y, Rgba8::new(v, v, v, 255));
            }
        }

        let region = Rect::new(26, 26, 12, 12);
        let coverage = all_ones(region);

        let mut smooth = pm.clone();
        heal_region(&mut smooth, region, &coverage, HealMode::ProximityMatch);
        let mut textured = pm.clone();
        heal_region(&mut textured, region, &coverage, HealMode::CreateTexture);

        // Spread of the healed centre, both ways.
        let spread = |img: &Pixmap| -> f32 {
            let mut values = Vec::new();
            for y in 29..35 {
                for x in 29..35 {
                    values.push(img.get(x, y).r as f32);
                }
            }
            let mean = values.iter().sum::<f32>() / values.len() as f32;
            (values.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / values.len() as f32)
                .sqrt()
        };
        assert!(
            spread(&textured) > spread(&smooth) + 1.0,
            "no grain added: {} vs {}",
            spread(&textured),
            spread(&smooth)
        );
    }

    #[test]
    fn healing_leaves_pixels_outside_the_brush_alone() {
        let mut pm = Pixmap::filled(48, 48, Rgba8::new(200, 200, 200, 255));
        pm.set(2, 2, Rgba8::new(10, 20, 30, 255));
        let before = pm.get(2, 2);

        let region = Rect::new(20, 20, 10, 10);
        heal_region(&mut pm, region, &disc(region, 4.0), HealMode::ProximityMatch);
        assert_eq!(pm.get(2, 2), before, "a far-away pixel was modified");
    }

    #[test]
    fn partial_coverage_blends_rather_than_replacing() {
        // Uniform 60% coverage over a blot that fills the region. The fill
        // comes from the surrounding ring, so the result must land between the
        // blot and the surroundings — not at either end.
        let mut pm = Pixmap::filled(48, 48, Rgba8::new(200, 100, 50, 255));
        let region = Rect::new(20, 20, 8, 8);
        for y in region.y..region.bottom() {
            for x in region.x..region.right() {
                pm.set(x, y, Rgba8::new(0, 0, 0, 255));
            }
        }

        let coverage = vec![0.6f32; 64];
        heal_region(&mut pm, region, &coverage, HealMode::ProximityMatch);

        // 0.6 of the way from black to (200, 100, 50) is about (120, 60, 30).
        let px = pm.get(23, 23);
        assert!(
            (px.r as i32 - 120).abs() <= 12,
            "partial coverage gave {:?}, wanted about (120, 60, 30)",
            px
        );
        assert!(px.r > 0 && px.r < 200, "coverage was treated as all-or-nothing");
    }

    #[test]
    fn an_empty_coverage_mask_does_nothing() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::WHITE);
        let before = pm.as_bytes().to_vec();
        let region = Rect::new(10, 10, 8, 8);
        let dirty = heal_region(&mut pm, region, &vec![0.0f32; 64], HealMode::ContentAware);
        assert!(dirty.is_empty());
        assert_eq!(pm.as_bytes(), &before[..]);
    }

    #[test]
    fn a_hole_with_no_surroundings_is_refused() {
        // The brush covers the entire image, so there is nothing to heal from.
        let mut pm = Pixmap::filled(16, 16, Rgba8::new(50, 60, 70, 255));
        let before = pm.as_bytes().to_vec();
        let region = Rect::new(0, 0, 16, 16);
        let dirty = heal_region(&mut pm, region, &all_ones(region), HealMode::ProximityMatch);
        assert!(dirty.is_empty(), "healed a hole with no boundary");
        assert_eq!(pm.as_bytes(), &before[..]);
    }

    #[test]
    fn healing_is_deterministic() {
        // Two identical runs must agree, including Create Texture's noise —
        // otherwise undo/redo would produce a different image.
        let source = {
            let mut pm = Pixmap::filled(48, 48, Rgba8::new(140, 130, 120, 255));
            for y in 20..26 {
                for x in 20..26 {
                    pm.set(x, y, Rgba8::new(20, 20, 20, 255));
                }
            }
            pm
        };
        let region = Rect::new(18, 18, 10, 10);

        for mode in [HealMode::ProximityMatch, HealMode::CreateTexture, HealMode::ContentAware] {
            let mut a = source.clone();
            let mut b = source.clone();
            heal_region(&mut a, region, &disc(region, 4.0), mode);
            heal_region(&mut b, region, &disc(region, 4.0), mode);
            assert_eq!(a.as_bytes(), b.as_bytes(), "{:?} is not deterministic", mode);
        }
    }

    #[test]
    fn cloning_carries_the_source_texture_over() {
        // Left half smooth, right half striped. Clone the stripes leftward and
        // the destination should end up striped.
        let mut pm = Pixmap::filled(80, 40, Rgba8::new(150, 150, 150, 255));
        for y in 0..40 {
            for x in 40..80 {
                let v = if (x / 2) % 2 == 0 { 110u8 } else { 190u8 };
                pm.set(x, y, Rgba8::new(v, v, v, 255));
            }
        }

        let region = Rect::new(12, 12, 16, 16);
        let dirty = clone_region(&mut pm, region, &all_ones(region), (40, 0), Transfer::Full);
        assert!(!dirty.is_empty());

        // Variation across the healed area means the stripes arrived.
        let mut values = Vec::new();
        for x in 16..25 {
            values.push(pm.get(x, 20).r as f32);
        }
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        let spread = (values.iter().map(|v| (v - mean).powi(2)).sum::<f32>()
            / values.len() as f32)
            .sqrt();
        assert!(spread > 8.0, "source texture did not transfer (spread {})", spread);
    }

    #[test]
    fn cloning_takes_its_brightness_from_the_destination() {
        // This is the whole point of the healing brush over the clone stamp: a
        // dark source pasted into a light area must come out light.
        let mut pm = Pixmap::filled(80, 40, Rgba8::new(200, 200, 200, 255));
        for y in 0..40 {
            for x in 40..80 {
                pm.set(x, y, Rgba8::new(40, 40, 40, 255));
            }
        }

        let region = Rect::new(12, 12, 12, 12);
        clone_region(&mut pm, region, &all_ones(region), (40, 0), Transfer::Full);

        let px = pm.get(18, 18);
        assert!(
            px.r > 150,
            "a dark source darkened the destination ({}); Poisson blending should \
             have matched the surroundings",
            px.r
        );
    }

    #[test]
    fn cloning_with_no_offset_does_nothing() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(70, 80, 90, 255));
        let before = pm.as_bytes().to_vec();
        let region = Rect::new(8, 8, 8, 8);
        assert!(clone_region(&mut pm, region, &all_ones(region), (0, 0), Transfer::Full).is_empty());
        assert_eq!(pm.as_bytes(), &before[..]);
    }

    #[test]
    fn moving_a_region_fills_the_hole_it_leaves() {
        // A dark blob on a light field; move it right and the old spot should
        // come back light.
        let mut pm = Pixmap::filled(96, 48, Rgba8::new(210, 200, 190, 255));
        for y in 18..30 {
            for x in 18..30 {
                pm.set(x, y, Rgba8::new(50, 40, 40, 255));
            }
        }

        let region = Rect::new(16, 16, 16, 16);
        let dirty = move_region(&mut pm, region, &all_ones(region), 40, 0, false);
        assert!(!dirty.is_empty());

        let vacated = pm.get(24, 24);
        assert!(
            (vacated.r as i32 - 210).abs() <= 20,
            "the hole was not filled: {:?}",
            vacated
        );
    }

    #[test]
    fn extend_mode_leaves_the_original_in_place() {
        let mut pm = Pixmap::filled(96, 48, Rgba8::new(210, 200, 190, 255));
        for y in 18..30 {
            for x in 18..30 {
                pm.set(x, y, Rgba8::new(50, 40, 40, 255));
            }
        }
        let before = pm.get(24, 24);

        let region = Rect::new(16, 16, 16, 16);
        move_region(&mut pm, region, &all_ones(region), 40, 0, true);

        assert_eq!(pm.get(24, 24), before, "extend mode erased the original");
    }

    #[test]
    fn a_move_of_zero_does_nothing() {
        let mut pm = Pixmap::filled(48, 48, Rgba8::new(100, 110, 120, 255));
        let before = pm.as_bytes().to_vec();
        let region = Rect::new(10, 10, 10, 10);
        assert!(move_region(&mut pm, region, &all_ones(region), 0, 0, false).is_empty());
        assert_eq!(pm.as_bytes(), &before[..]);
    }

    #[test]
    fn red_eye_neutralises_red_and_leaves_skin_alone() {
        // A red pupil surrounded by skin tone. Skin is reddish but not
        // red-dominant enough to qualify.
        let mut pm = Pixmap::filled(40, 40, Rgba8::new(215, 175, 150, 255));
        for y in 18..23 {
            for x in 18..23 {
                pm.set(x, y, Rgba8::new(220, 30, 30, 255));
            }
        }

        let region = Rect::new(14, 14, 12, 12);
        let dirty = red_eye_region(&mut pm, region, &all_ones(region), 50, 50);
        assert!(!dirty.is_empty());

        let pupil = pm.get(20, 20);
        assert!(pupil.r < 80, "the pupil is still red: {:?}", pupil);
        // Skin inside the dragged area must survive.
        let skin = pm.get(15, 15);
        assert!(
            (skin.r as i32 - 215).abs() <= 4 && (skin.g as i32 - 175).abs() <= 4,
            "skin was drained: {:?}",
            skin
        );
    }

    #[test]
    fn darken_amount_controls_how_dark_the_pupil_ends_up() {
        let build = || {
            let mut pm = Pixmap::filled(40, 40, Rgba8::new(215, 175, 150, 255));
            for y in 18..23 {
                for x in 18..23 {
                    pm.set(x, y, Rgba8::new(220, 40, 40, 255));
                }
            }
            pm
        };
        let region = Rect::new(14, 14, 12, 12);

        let mut light = build();
        red_eye_region(&mut light, region, &all_ones(region), 50, 0);
        let mut dark = build();
        red_eye_region(&mut dark, region, &all_ones(region), 50, 100);

        assert!(
            dark.get(20, 20).r < light.get(20, 20).r,
            "darken had no effect: {} vs {}",
            dark.get(20, 20).r,
            light.get(20, 20).r
        );
    }

    #[test]
    fn red_eye_ignores_an_area_with_no_red() {
        let mut pm = Pixmap::filled(32, 32, Rgba8::new(80, 140, 200, 255));
        let before = pm.as_bytes().to_vec();
        let region = Rect::new(8, 8, 10, 10);
        assert!(red_eye_region(&mut pm, region, &all_ones(region), 50, 50).is_empty());
        assert_eq!(pm.as_bytes(), &before[..]);
    }

    #[test]
    fn mode_round_trips_through_its_integer() {
        assert_eq!(HealMode::from_i32(0), HealMode::ProximityMatch);
        assert_eq!(HealMode::from_i32(1), HealMode::CreateTexture);
        assert_eq!(HealMode::from_i32(2), HealMode::ContentAware);
        // Anything unexpected falls back to CS6's default.
        assert_eq!(HealMode::from_i32(99), HealMode::ContentAware);
        assert_eq!(HealMode::default(), HealMode::ContentAware);
    }

    #[test]
    fn transparency_is_healed_too() {
        // A hole punched through an opaque layer should come back opaque.
        let mut pm = Pixmap::filled(48, 48, Rgba8::new(120, 120, 120, 255));
        for y in 20..26 {
            for x in 20..26 {
                pm.set(x, y, Rgba8::TRANSPARENT);
            }
        }
        let region = Rect::new(18, 18, 10, 10);
        heal_region(&mut pm, region, &disc(region, 4.0), HealMode::ProximityMatch);
        assert!(pm.get(23, 23).a > 240, "alpha was not reconstructed");
    }
}

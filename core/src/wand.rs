//! Colour-driven selection: the Magic Wand and the Quick Selection tool.
//!
//! Both answer the same question — "which pixels belong with this one?" — and
//! differ in how they are asked. The wand is told once, by a click, and floods
//! outward on colour similarity alone. The quick selector is told repeatedly,
//! by a brush dragged across the image, and each dab grows a region that stops
//! at edges rather than at a fixed colour threshold.
//!
//! Neither writes to the document. They produce a coverage mask, which the
//! caller combines into the selection under the usual op and feather
//! (see [`crate::selection::Selection::apply_mask_feathered`]).

use crate::buffer::{Pixmap, Rgba8};
use std::collections::VecDeque;

/// Per-channel distance between two pixels, in 0–255.
///
/// The max rather than the sum, which is how Photoshop's Tolerance reads: a
/// tolerance of 32 admits anything within 32 levels **on every channel**, so
/// one badly-off channel is enough to reject a pixel.
fn channel_distance(a: Rgba8, b: Rgba8) -> u32 {
    let d = |p: u8, q: u8| (p as i32 - q as i32).unsigned_abs();
    d(a.r, b.r)
        .max(d(a.g, b.g))
        .max(d(a.b, b.b))
        .max(d(a.a, b.a))
}

/// Select pixels matching the one at `seed`, Photoshop's Magic Wand.
///
/// * `tolerance` — 0–255, how far a pixel may differ per channel.
/// * `contiguous` — when true only the region connected to the seed is taken,
///   which is the checkbox's default state. When false every matching pixel in
///   the image is, however far away.
/// * `antialias` — soften the boundary by about half a pixel, as the Anti-alias
///   checkbox does.
///
/// Returns a whole-canvas coverage mask. An out-of-bounds seed selects nothing.
pub fn magic_wand(
    pixels: &Pixmap,
    seed: (i32, i32),
    tolerance: u32,
    contiguous: bool,
    antialias: bool,
) -> Vec<u8> {
    let width = pixels.width() as usize;
    let height = pixels.height() as usize;
    let mut mask = vec![0u8; width * height];

    if !pixels.rect().contains(seed.0, seed.1) {
        return mask;
    }
    let target = pixels.get(seed.0, seed.1);

    if !contiguous {
        // Global: every matching pixel, connected or not.
        for y in 0..height {
            for x in 0..width {
                if channel_distance(pixels.get(x as i32, y as i32), target) <= tolerance {
                    mask[y * width + x] = 255;
                }
            }
        }
    } else {
        // Four-connected flood from the seed, which is the connectivity
        // Photoshop uses — a diagonal touch does not bridge two regions.
        let mut queue = VecDeque::new();
        mask[seed.1 as usize * width + seed.0 as usize] = 255;
        queue.push_back((seed.0, seed.1));

        while let Some((x, y)) = queue.pop_front() {
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let (nx, ny) = (x + dx, y + dy);
                if !pixels.rect().contains(nx, ny) {
                    continue;
                }
                let index = ny as usize * width + nx as usize;
                if mask[index] != 0 {
                    continue;
                }
                if channel_distance(pixels.get(nx, ny), target) <= tolerance {
                    mask[index] = 255;
                    queue.push_back((nx, ny));
                }
            }
        }
    }

    if antialias {
        soften_edges(&mut mask, width, height);
    }
    mask
}

/// Half-pixel smoothing of a binary mask's boundary.
///
/// A 3×3 mean, which leaves solid interiors at 255 and empty areas at 0 and so
/// only does anything within one pixel of an edge — exactly the reach the
/// Anti-alias checkbox has.
fn soften_edges(mask: &mut [u8], width: usize, height: usize) {
    if width < 3 || height < 3 {
        return;
    }
    let source = mask.to_vec();
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let index = y * width + x;
            // Skip the interior and the far outside: nothing to soften, and
            // this is the overwhelming majority of the image.
            let here = source[index];
            let mut sum = 0u32;
            let mut uniform = true;
            for dy in 0..3 {
                for dx in 0..3 {
                    let s = source[(y + dy - 1) * width + (x + dx - 1)];
                    sum += s as u32;
                    uniform &= s == here;
                }
            }
            if !uniform {
                mask[index] = (sum / 9) as u8;
            }
        }
    }
}

/// The Quick Selection tool's accumulating region.
///
/// Built once per drag, because the edge field costs a pass over the image and
/// the brush dabs arrive many times a second. Each dab grows the region from
/// the pixels under the brush and unions the result in, so dragging across an
/// object gradually fills it.
pub struct QuickSelector {
    width: u32,
    height: u32,
    pixels: Pixmap,
    /// Gradient magnitude per pixel, normalised to `0.0..=1.0`. Growth will
    /// not cross a pixel above [`EDGE_LIMIT`].
    edges: Vec<f32>,
    /// Everything selected by the dabs so far.
    mask: Vec<u8>,
}

/// Normalised gradient above which growth stops.
///
/// This is what makes the tool "quick": the region runs to the object's
/// boundary and halts there, instead of needing a tolerance tuned per image.
const EDGE_LIMIT: f32 = 0.18;

/// Ceiling on how many pixels one dab may add.
///
/// Without it a dab on a flat background floods the whole image and the drag
/// stops feeling interactive. Photoshop has the same practical behaviour: one
/// dab claims a region, not the document.
const MAX_DAB_PIXELS: usize = 1 << 22;

impl QuickSelector {
    pub fn new(pixels: &Pixmap) -> Self {
        let width = pixels.width();
        let height = pixels.height();
        let edges = gradient_field(pixels);
        Self {
            width,
            height,
            pixels: pixels.clone(),
            edges,
            mask: vec![0u8; (width as usize) * (height as usize)],
        }
    }

    /// The region selected so far.
    pub fn mask(&self) -> &[u8] {
        &self.mask
    }

    /// Grow from the pixels under a brush dab and union the result in.
    ///
    /// There is deliberately no tolerance parameter: CS6's Quick Selection has
    /// no such control. The threshold comes from the dab's own colour spread,
    /// so a brush set down on flat sky wanders further than one set down on
    /// texture, without the user tuning anything.
    pub fn add_dab(&mut self, cx: f32, cy: f32, radius: f32) {
        if let Some(region) = self.grow(cx, cy, radius) {
            for (dst, src) in self.mask.iter_mut().zip(region.iter()) {
                *dst = (*dst).max(*src);
            }
        }
    }

    /// As `add_dab`, taking the grown region back out again — what holding the
    /// subtract modifier during a drag does.
    pub fn subtract_dab(&mut self, cx: f32, cy: f32, radius: f32) {
        if let Some(region) = self.grow(cx, cy, radius) {
            for (dst, src) in self.mask.iter_mut().zip(region.iter()) {
                *dst = dst.saturating_sub(*src);
            }
        }
    }

    /// Region-grow from one dab. `None` when the dab lands off-canvas.
    fn grow(&self, cx: f32, cy: f32, radius: f32) -> Option<Vec<u8>> {
        let w = self.width as usize;
        let h = self.height as usize;
        let radius = radius.max(1.0);

        // Seed pixels: everything under the brush. They are taken regardless
        // of colour or edge — the user pointed at them.
        let mut region = vec![0u8; w * h];
        let mut queue = VecDeque::new();
        let mut sum = (0u64, 0u64, 0u64);
        let mut sum_sq = (0u64, 0u64, 0u64);
        let mut seeds = 0u64;

        let x0 = ((cx - radius).floor() as i32).max(0);
        let x1 = ((cx + radius).ceil() as i32).min(self.width as i32 - 1);
        let y0 = ((cy - radius).floor() as i32).max(0);
        let y1 = ((cy + radius).ceil() as i32).min(self.height as i32 - 1);

        for y in y0..=y1 {
            for x in x0..=x1 {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let px = self.pixels.get(x, y);
                sum.0 += px.r as u64;
                sum.1 += px.g as u64;
                sum.2 += px.b as u64;
                sum_sq.0 += (px.r as u64) * (px.r as u64);
                sum_sq.1 += (px.g as u64) * (px.g as u64);
                sum_sq.2 += (px.b as u64) * (px.b as u64);
                seeds += 1;
                region[y as usize * w + x as usize] = 255;
                queue.push_back((x, y));
            }
        }

        if seeds == 0 {
            return None;
        }

        let mean = Rgba8::new(
            (sum.0 / seeds) as u8,
            (sum.1 / seeds) as u8,
            (sum.2 / seeds) as u8,
            255,
        );
        // How far the region may stray from the dab's mean, derived from how
        // varied the dab itself was. Three standard deviations covers the
        // material the brush actually sat on; the floor keeps a dab on a
        // perfectly flat area from being unable to grow at all, and the
        // ceiling stops a dab straddling two materials from taking both.
        let variance = |total: u64, squares: u64| -> f32 {
            let mean = total as f32 / seeds as f32;
            (squares as f32 / seeds as f32 - mean * mean).max(0.0)
        };
        let spread = variance(sum.0, sum_sq.0)
            .max(variance(sum.1, sum_sq.1))
            .max(variance(sum.2, sum_sq.2))
            .sqrt();
        let limit = ((3.0 * spread) as u32).clamp(12, 64);

        let mut claimed = seeds as usize;
        while let Some((x, y)) = queue.pop_front() {
            if claimed >= MAX_DAB_PIXELS {
                break;
            }
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= self.width as i32 || ny >= self.height as i32 {
                    continue;
                }
                let index = ny as usize * w + nx as usize;
                if region[index] != 0 {
                    continue;
                }
                if self.edges[index] > EDGE_LIMIT {
                    continue;
                }
                let mut candidate = Rgba8 { a: 255, ..self.pixels.get(nx, ny) };
                candidate.a = 255;
                if channel_distance(candidate, mean) > limit {
                    continue;
                }
                region[index] = 255;
                claimed += 1;
                queue.push_back((nx, ny));
            }
        }

        Some(region)
    }
}

/// Sobel gradient magnitude over luminance, normalised against the image's own
/// peak so the edge limit means the same thing on a flat photo and a graphic.
///
/// The kernel samples with clamped coordinates rather than skipping the
/// one-pixel border. Leaving the border at zero would give region growth a
/// gradient-free lane all the way around the image, and it would use it —
/// slipping past the very edge it is supposed to stop at.
fn gradient_field(pixels: &Pixmap) -> Vec<f32> {
    let w = pixels.width() as usize;
    let h = pixels.height() as usize;
    let mut field = vec![0.0f32; w * h];
    if w == 0 || h == 0 {
        return field;
    }

    let mut luma = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let px = pixels.get(x as i32, y as i32);
            luma[y * w + x] = 0.299 * px.r as f32 + 0.587 * px.g as f32 + 0.114 * px.b as f32;
        }
    }

    let sample = |x: i32, y: i32| -> f32 {
        let cx = x.clamp(0, w as i32 - 1) as usize;
        let cy = y.clamp(0, h as i32 - 1) as usize;
        luma[cy * w + cx]
    };

    let mut peak = 0.0f32;
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let gx = -sample(x - 1, y - 1) - 2.0 * sample(x - 1, y) - sample(x - 1, y + 1)
                + sample(x + 1, y - 1) + 2.0 * sample(x + 1, y) + sample(x + 1, y + 1);
            let gy = -sample(x - 1, y - 1) - 2.0 * sample(x, y - 1) - sample(x + 1, y - 1)
                + sample(x - 1, y + 1) + 2.0 * sample(x, y + 1) + sample(x + 1, y + 1);
            let g = (gx * gx + gy * gy).sqrt();
            field[y as usize * w + x as usize] = g;
            peak = peak.max(g);
        }
    }

    if peak > 0.0 {
        for g in field.iter_mut() {
            *g /= peak;
        }
    }
    field
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two solid blocks side by side with a hard boundary at `split`.
    fn split_image(width: u32, height: u32, split: i32, left: Rgba8, right: Rgba8) -> Pixmap {
        let mut pm = Pixmap::filled(width, height, left);
        for y in 0..height as i32 {
            for x in split..width as i32 {
                pm.set(x, y, right);
            }
        }
        pm
    }

    fn count_selected(mask: &[u8]) -> usize {
        mask.iter().filter(|&&c| c > 128).count()
    }

    #[test]
    fn the_wand_takes_the_block_it_was_clicked_on() {
        let pm = split_image(32, 32, 16, Rgba8::BLACK, Rgba8::WHITE);
        let mask = magic_wand(&pm, (4, 4), 32, true, false);

        assert_eq!(mask[4 * 32 + 4], 255, "the seed itself was not selected");
        assert_eq!(mask[4 * 32 + 20], 0, "the wand crossed the boundary");
        assert_eq!(count_selected(&mask), 16 * 32, "expected exactly the left block");
    }

    #[test]
    fn a_wide_tolerance_takes_both_blocks() {
        let pm = split_image(32, 32, 16, Rgba8::new(100, 100, 100, 255), Rgba8::new(120, 120, 120, 255));
        let narrow = magic_wand(&pm, (4, 4), 5, true, false);
        assert_eq!(count_selected(&narrow), 16 * 32);

        let wide = magic_wand(&pm, (4, 4), 40, true, false);
        assert_eq!(count_selected(&wide), 32 * 32, "tolerance 40 should span 20 levels");
    }

    #[test]
    fn contiguous_off_reaches_a_disconnected_match() {
        // Two black squares with white between them.
        let mut pm = Pixmap::filled(32, 32, Rgba8::WHITE);
        for y in 2..8 {
            for x in 2..8 {
                pm.set(x, y, Rgba8::BLACK);
            }
        }
        for y in 20..26 {
            for x in 20..26 {
                pm.set(x, y, Rgba8::BLACK);
            }
        }

        let joined = magic_wand(&pm, (4, 4), 10, true, false);
        assert_eq!(joined[22 * 32 + 22], 0, "contiguous mode jumped the gap");
        assert_eq!(count_selected(&joined), 36);

        let global = magic_wand(&pm, (4, 4), 10, false, false);
        assert_eq!(global[22 * 32 + 22], 255, "non-contiguous missed the far square");
        assert_eq!(count_selected(&global), 72);
    }

    #[test]
    fn an_out_of_bounds_click_selects_nothing() {
        let pm = split_image(16, 16, 8, Rgba8::BLACK, Rgba8::WHITE);
        let mask = magic_wand(&pm, (99, 99), 32, true, false);
        assert_eq!(count_selected(&mask), 0);
    }

    #[test]
    fn antialias_softens_the_boundary_but_not_the_interior() {
        let pm = split_image(32, 32, 16, Rgba8::BLACK, Rgba8::WHITE);
        let hard = magic_wand(&pm, (4, 4), 32, true, false);
        let soft = magic_wand(&pm, (4, 4), 32, true, true);

        assert_eq!(soft[16 * 32 + 4], 255, "the interior was softened");
        // The column either side of the boundary picks up partial coverage.
        let edge = soft[16 * 32 + 15];
        assert!(edge > 0 && edge < 255, "expected a soft edge, got {}", edge);
        assert_ne!(hard[16 * 32 + 15], edge);
    }

    #[test]
    fn quick_select_fills_the_region_under_the_brush() {
        let pm = split_image(64, 64, 32, Rgba8::BLACK, Rgba8::WHITE);
        let mut q = QuickSelector::new(&pm);
        q.add_dab(10.0, 32.0, 5.0);

        let mask = q.mask();
        assert_eq!(mask[32 * 64 + 10], 255, "the dab itself was not selected");
        assert!(count_selected(mask) > 1000, "the region barely grew");
    }

    #[test]
    fn quick_select_stops_at_an_edge() {
        let pm = split_image(64, 64, 32, Rgba8::BLACK, Rgba8::WHITE);
        let mut q = QuickSelector::new(&pm);
        q.add_dab(10.0, 32.0, 5.0);

        // Even at full tolerance the gradient at the boundary halts growth.
        let mask = q.mask();
        assert_eq!(mask[32 * 64 + 45], 0, "growth crossed the object boundary");
    }

    #[test]
    fn quick_select_cannot_slip_around_an_edge_along_the_border() {
        // The gradient kernel used to skip the one-pixel border, leaving a
        // gradient-free lane around the image that growth would run along to
        // get past the boundary. The dab sits on the top row, right against
        // it.
        let pm = split_image(64, 64, 32, Rgba8::BLACK, Rgba8::WHITE);
        let mut q = QuickSelector::new(&pm);
        q.add_dab(10.0, 0.0, 3.0);

        let mask = q.mask();
        assert_eq!(mask[45], 0, "growth ran along the top row past the edge");
        assert_eq!(mask[63 * 64 + 45], 0, "growth reached the far block");
    }

    #[test]
    fn quick_select_accumulates_across_dabs() {
        // Three separated grey squares on white; one dab each.
        let mut pm = Pixmap::filled(64, 64, Rgba8::WHITE);
        for (ox, oy) in [(4, 4), (30, 4), (4, 30)] {
            for y in oy..oy + 10 {
                for x in ox..ox + 10 {
                    pm.set(x, y, Rgba8::new(40, 40, 40, 255));
                }
            }
        }

        let mut q = QuickSelector::new(&pm);
        q.add_dab(8.0, 8.0, 2.0);
        let after_one = count_selected(q.mask());
        q.add_dab(34.0, 8.0, 2.0);
        let after_two = count_selected(q.mask());

        assert!(after_two > after_one, "the second dab added nothing");
        assert_eq!(q.mask()[8 * 64 + 8], 255, "the first dab's region was lost");
        assert_eq!(q.mask()[8 * 64 + 34], 255, "the second dab's region is missing");
    }

    #[test]
    fn quick_select_subtract_removes_what_it_covers() {
        let pm = split_image(64, 64, 32, Rgba8::BLACK, Rgba8::WHITE);
        let mut q = QuickSelector::new(&pm);
        q.add_dab(10.0, 32.0, 5.0);
        assert!(count_selected(q.mask()) > 0);

        q.subtract_dab(10.0, 32.0, 5.0);
        assert_eq!(count_selected(q.mask()), 0, "subtracting the same dab left pixels behind");
    }

    #[test]
    fn a_dab_off_canvas_changes_nothing() {
        let pm = split_image(32, 32, 16, Rgba8::BLACK, Rgba8::WHITE);
        let mut q = QuickSelector::new(&pm);
        q.add_dab(-50.0, -50.0, 4.0);
        assert_eq!(count_selected(q.mask()), 0);
    }
}

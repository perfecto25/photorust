//! Edge snapping for the Magnetic Lasso.
//!
//! Photoshop's magnetic lasso is a *live wire*: as the cursor moves, the
//! segment from the last anchor to the cursor is not a straight line but the
//! cheapest path through an edge-cost field, so it clings to the boundary the
//! user is tracing. This module builds that field once per gesture and answers
//! path queries against it.
//!
//! The two options that matter map onto CS6's own:
//!
//! * **Contrast** — how strong a gradient has to be before it counts as an
//!   edge at all. Raising it makes the wire ignore texture and hold out for
//!   real boundaries.
//! * **Width** — how far either side of the straight line the search is
//!   allowed to wander looking for one.

use crate::buffer::{Pixmap, Rect};
use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Largest search corridor, in pixels, that a single trace will explore.
///
/// A live wire is re-traced on every mouse move, so the query has to stay
/// interactive. Past this the corridor is not searched at all and the caller
/// gets a straight line — which is also the right answer, because a jump that
/// long means the user has moved far from the last anchor and is not tracing
/// an edge any more.
const MAX_TRACE_PIXELS: usize = 1 << 20;

/// Cost charged for every step regardless of edge strength.
///
/// Without it the wire would happily take a long detour along a slightly
/// stronger edge rather than the short obvious route. This is the "keep it
/// taut" term.
const STEP_COST: f32 = 0.05;

/// A per-pixel traversal cost field: low on edges, high on flat areas.
#[derive(Clone, Debug)]
pub struct EdgeMap {
    width: u32,
    height: u32,
    /// Cost in `0.0..=1.0`; 0 is a strong edge, 1 is flat.
    cost: Vec<f32>,
}

impl EdgeMap {
    /// Build the cost field from a composited image.
    ///
    /// `contrast` is CS6's 1–100 percentage. Gradients below it are flattened
    /// to "no edge", and what survives is rescaled across the remaining range
    /// so the wire still discriminates between what is left.
    pub fn from_pixmap(pixels: &Pixmap, contrast: u32) -> Self {
        let width = pixels.width();
        let height = pixels.height();
        let mut cost = vec![1.0f32; (width as usize) * (height as usize)];

        if width < 3 || height < 3 {
            return Self { width, height, cost };
        }

        // Luminance is enough to find boundaries and is a third of the work of
        // running the operator on three channels.
        let mut luma = vec![0.0f32; (width as usize) * (height as usize)];
        for y in 0..height {
            for x in 0..width {
                let px = pixels.get(x as i32, y as i32);
                luma[(y as usize) * (width as usize) + x as usize] =
                    0.299 * px.r as f32 + 0.587 * px.g as f32 + 0.114 * px.b as f32;
            }
        }

        // Sobel, skipping the one-pixel border where the kernel would hang off
        // the edge. Those pixels keep the maximum cost, which also stops the
        // wire from cheating along the canvas border.
        let mut magnitude = vec![0.0f32; luma.len()];
        let mut peak = 0.0f32;
        let w = width as usize;
        for y in 1..(height as usize - 1) {
            for x in 1..(w - 1) {
                let at = |dx: usize, dy: usize| luma[(y + dy - 1) * w + (x + dx - 1)];
                let gx = -at(0, 0) - 2.0 * at(0, 1) - at(0, 2)
                    + at(2, 0) + 2.0 * at(2, 1) + at(2, 2);
                let gy = -at(0, 0) - 2.0 * at(1, 0) - at(2, 0)
                    + at(0, 2) + 2.0 * at(1, 2) + at(2, 2);
                let g = (gx * gx + gy * gy).sqrt();
                magnitude[y * w + x] = g;
                peak = peak.max(g);
            }
        }

        if peak <= 0.0 {
            // A perfectly flat image has no edges to snap to; every path costs
            // the same and the wire comes out straight.
            return Self { width, height, cost };
        }

        let threshold = (contrast.clamp(1, 100) as f32) / 100.0;
        let span = (1.0 - threshold).max(1e-6);
        for (c, &g) in cost.iter_mut().zip(magnitude.iter()) {
            let normalised = g / peak;
            let strength = ((normalised - threshold) / span).clamp(0.0, 1.0);
            *c = 1.0 - strength;
        }

        Self { width, height, cost }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Cost of entering the pixel at `(x, y)`. Out of bounds is impassable.
    fn cost_at(&self, x: i32, y: i32) -> Option<f32> {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return None;
        }
        Some(self.cost[(y as usize) * (self.width as usize) + x as usize])
    }

    /// The cheapest path from `from` to `to`, inclusive of both.
    ///
    /// `width` is CS6's detection width: the corridor around the straight line
    /// that the search may use. Returns a straight line when no better path
    /// exists, when the corridor would be too large to search interactively,
    /// or when either endpoint is off-canvas — a straight segment is always a
    /// usable answer, so this never fails.
    pub fn trace(&self, from: (i32, i32), to: (i32, i32), width: u32) -> Vec<(i32, i32)> {
        let canvas = Rect::from_size(self.width, self.height);
        if !canvas.contains(from.0, from.1) || !canvas.contains(to.0, to.1) {
            return straight_line(from, to);
        }
        if from == to {
            return vec![from];
        }

        // Corridor: the segment's bounding box, opened up by the detection
        // width so the wire can bow out to reach an edge beside the line.
        let region = Rect::new(
            from.0.min(to.0),
            from.1.min(to.1),
            (from.0 - to.0).unsigned_abs() + 1,
            (from.1 - to.1).unsigned_abs() + 1,
        )
        .inflate(width.max(1))
        .intersect(&canvas);

        let rw = region.width as usize;
        let rh = region.height as usize;
        if rw * rh > MAX_TRACE_PIXELS {
            return straight_line(from, to);
        }

        let index = |x: i32, y: i32| -> usize {
            ((y - region.y) as usize) * rw + (x - region.x) as usize
        };

        let mut best = vec![f32::INFINITY; rw * rh];
        // Predecessor per visited pixel, as an index into the same grid.
        let mut came_from = vec![usize::MAX; rw * rh];
        let mut settled = vec![false; rw * rh];

        let start = index(from.0, from.1);
        let goal = index(to.0, to.1);
        best[start] = 0.0;

        let mut queue: BinaryHeap<Step> = BinaryHeap::new();
        queue.push(Step { cost: 0.0, at: start });

        // Eight-way movement; diagonals pay their real length so the wire does
        // not prefer staircases to straight diagonals.
        const NEIGHBOURS: [(i32, i32, f32); 8] = [
            (-1, 0, 1.0), (1, 0, 1.0), (0, -1, 1.0), (0, 1, 1.0),
            (-1, -1, std::f32::consts::SQRT_2), (1, -1, std::f32::consts::SQRT_2),
            (-1, 1, std::f32::consts::SQRT_2), (1, 1, std::f32::consts::SQRT_2),
        ];

        while let Some(Step { at, .. }) = queue.pop() {
            if settled[at] {
                continue;
            }
            settled[at] = true;
            if at == goal {
                break;
            }

            let x = region.x + (at % rw) as i32;
            let y = region.y + (at / rw) as i32;

            for &(dx, dy, length) in &NEIGHBOURS {
                let (nx, ny) = (x + dx, y + dy);
                if !region.contains(nx, ny) {
                    continue;
                }
                let Some(edge_cost) = self.cost_at(nx, ny) else {
                    continue;
                };
                let next = index(nx, ny);
                if settled[next] {
                    continue;
                }
                let candidate = best[at] + (edge_cost + STEP_COST) * length;
                if candidate < best[next] {
                    best[next] = candidate;
                    came_from[next] = at;
                    queue.push(Step { cost: candidate, at: next });
                }
            }
        }

        if !settled[goal] {
            return straight_line(from, to);
        }

        // Walk the predecessors back and flip, so the caller gets the path in
        // travel order.
        let mut path = Vec::new();
        let mut cursor = goal;
        loop {
            path.push((
                region.x + (cursor % rw) as i32,
                region.y + (cursor / rw) as i32,
            ));
            if cursor == start {
                break;
            }
            cursor = came_from[cursor];
            if cursor == usize::MAX {
                return straight_line(from, to);
            }
        }
        path.reverse();
        path
    }
}

/// One entry in Dijkstra's frontier.
///
/// `Ord` is deliberately reversed so `BinaryHeap` — a max-heap — pops the
/// cheapest step first.
struct Step {
    cost: f32,
    at: usize,
}

impl PartialEq for Step {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost
    }
}

impl Eq for Step {}

impl PartialOrd for Step {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Step {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
            .then_with(|| self.at.cmp(&other.at))
    }
}

/// Bresenham between two points, used whenever the wire has nothing to snap to.
fn straight_line(from: (i32, i32), to: (i32, i32)) -> Vec<(i32, i32)> {
    let (mut x, mut y) = from;
    let dx = (to.0 - x).abs();
    let dy = -(to.1 - y).abs();
    let sx = if x < to.0 { 1 } else { -1 };
    let sy = if y < to.1 { 1 } else { -1 };
    let mut err = dx + dy;

    let mut points = Vec::new();
    loop {
        points.push((x, y));
        if x == to.0 && y == to.1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Rgba8;

    /// A canvas split down the middle: black on the left, white on the right,
    /// so there is exactly one strong vertical edge at `x = split`.
    fn vertical_edge(width: u32, height: u32, split: i32) -> Pixmap {
        let mut pm = Pixmap::filled(width, height, Rgba8::BLACK);
        for y in 0..height as i32 {
            for x in split..width as i32 {
                pm.set(x, y, Rgba8::WHITE);
            }
        }
        pm
    }

    #[test]
    fn a_flat_image_traces_a_straight_line() {
        let pm = Pixmap::filled(32, 32, Rgba8::WHITE);
        let map = EdgeMap::from_pixmap(&pm, 10);
        let path = map.trace((4, 4), (4, 20), 10);

        assert_eq!(path.first(), Some(&(4, 4)));
        assert_eq!(path.last(), Some(&(4, 20)));
        for &(x, _) in &path {
            assert_eq!(x, 4, "wire wandered off a featureless image");
        }
    }

    #[test]
    fn the_wire_snaps_onto_a_nearby_edge() {
        let pm = vertical_edge(64, 64, 32);
        let map = EdgeMap::from_pixmap(&pm, 10);

        // Both endpoints sit on the edge, but a few pixels off it in the
        // middle would be cheaper only if the edge did not attract.
        let path = map.trace((32, 8), (32, 56), 12);
        assert!(path.len() >= 40, "path was suspiciously short: {}", path.len());
        for &(x, _) in &path {
            assert!((x - 32).abs() <= 1, "wire left the edge at x = {}", x);
        }
    }

    #[test]
    fn the_wire_bows_toward_an_edge_beside_the_line() {
        let pm = vertical_edge(64, 64, 32);
        let map = EdgeMap::from_pixmap(&pm, 10);

        // Start and end off the edge; the cheap route detours onto it.
        let path = map.trace((26, 8), (26, 56), 16);
        let reached = path.iter().any(|&(x, _)| (x - 32).abs() <= 1);
        assert!(reached, "wire ignored the edge next to it");
        assert_eq!(path.first(), Some(&(26, 8)));
        assert_eq!(path.last(), Some(&(26, 56)));
    }

    #[test]
    fn a_narrow_corridor_keeps_the_wire_near_the_line() {
        let pm = vertical_edge(64, 64, 32);
        let map = EdgeMap::from_pixmap(&pm, 10);

        // Width 2 cannot reach the edge six pixels away, so the wire stays put
        // rather than snapping across the image.
        let path = map.trace((26, 8), (26, 56), 2);
        for &(x, _) in &path {
            assert!((x - 26).abs() <= 2, "wire escaped its corridor at x = {}", x);
        }
    }

    #[test]
    fn a_high_contrast_setting_ignores_a_weak_edge() {
        // A barely-there boundary: two greys a few levels apart.
        let mut pm = Pixmap::filled(64, 64, Rgba8::new(120, 120, 120, 255));
        for y in 0..64 {
            for x in 32..64 {
                pm.set(x, y, Rgba8::new(128, 128, 128, 255));
            }
        }
        // Add a genuinely strong edge elsewhere so `peak` is set by that, and
        // the weak one normalises to well under the threshold.
        for y in 0..64 {
            pm.set(8, y, Rgba8::BLACK);
        }

        let map = EdgeMap::from_pixmap(&pm, 60);
        let path = map.trace((26, 8), (26, 56), 16);
        for &(x, _) in &path {
            assert!((x - 26).abs() <= 1, "wire chased a sub-threshold edge to x = {}", x);
        }
    }

    #[test]
    fn endpoints_are_always_honoured() {
        let pm = vertical_edge(64, 64, 32);
        let map = EdgeMap::from_pixmap(&pm, 10);
        let path = map.trace((3, 3), (60, 60), 20);
        assert_eq!(path.first(), Some(&(3, 3)));
        assert_eq!(path.last(), Some(&(60, 60)));
    }

    #[test]
    fn an_off_canvas_endpoint_falls_back_to_a_straight_line() {
        let pm = vertical_edge(64, 64, 32);
        let map = EdgeMap::from_pixmap(&pm, 10);
        let path = map.trace((10, 10), (200, 10), 10);
        assert_eq!(path.first(), Some(&(10, 10)));
        assert_eq!(path.last(), Some(&(200, 10)));
    }

    #[test]
    fn the_path_is_connected_step_by_step() {
        let pm = vertical_edge(64, 64, 32);
        let map = EdgeMap::from_pixmap(&pm, 10);
        let path = map.trace((10, 10), (50, 40), 12);
        for pair in path.windows(2) {
            let (dx, dy) = (pair[1].0 - pair[0].0, pair[1].1 - pair[0].1);
            assert!(dx.abs() <= 1 && dy.abs() <= 1, "path jumped by ({}, {})", dx, dy);
        }
    }
}

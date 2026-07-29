//! Selections.
//!
//! A selection is an 8-bit coverage mask over the whole canvas, so feathered
//! and antialiased edges are represented exactly the same way as hard ones —
//! matching Photoshop, where "selected" is a continuous quantity.

use crate::buffer::{Pixmap, Rect};
use std::cell::Cell;
use std::collections::BTreeMap;

/// Coverage at which a pixel counts as inside the marching-ants boundary.
///
/// Photoshop draws the ants at the 50% contour, which is why a heavily
/// feathered selection's outline sits well inside the visible falloff — the
/// outline is not the edge of the affected area, it is where the selection is
/// half-strength.
const OUTLINE_THRESHOLD: u8 = 128;

/// How a new selection combines with the existing one. Mirrors the four
/// buttons at the left of the marquee tool's options bar.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(i32)]
pub enum SelectionOp {
    #[default]
    Replace = 0,
    Add = 1,
    Subtract = 2,
    Intersect = 3,
}

impl SelectionOp {
    pub fn from_i32(v: i32) -> SelectionOp {
        match v {
            1 => SelectionOp::Add,
            2 => SelectionOp::Subtract,
            3 => SelectionOp::Intersect,
            _ => SelectionOp::Replace,
        }
    }
}

/// A per-pixel coverage mask, `0` = unselected, `255` = fully selected.
#[derive(Clone, Debug)]
pub struct Selection {
    width: u32,
    height: u32,
    coverage: Vec<u8>,
    /// Cached bounding box of all non-zero coverage. `None` means "not yet
    /// computed"; recalculated lazily by [`Selection::bounds`].
    cached_bounds: Option<Rect>,
    /// Memoised answer for [`Selection::is_empty`].
    ///
    /// A `Cell` because `is_empty` takes `&self` and is called from hot paths
    /// that only have a shared borrow — the compositor asks it once per stroke
    /// and the shell once per repaint. Without the memo each call walks the
    /// whole mask.
    cached_empty: Cell<Option<bool>>,
}

impl Selection {
    /// An empty (nothing selected) selection.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            coverage: vec![0u8; (width as usize) * (height as usize)],
            cached_bounds: Some(Rect::default()),
            cached_empty: Cell::new(Some(true)),
        }
    }

    /// Everything selected — the state after Select ▸ All.
    pub fn all(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            coverage: vec![255u8; (width as usize) * (height as usize)],
            cached_bounds: Some(Rect::from_size(width, height)),
            cached_empty: Cell::new(Some(false)),
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// True when nothing is selected. Callers treat this as "operate on the
    /// whole layer", which is how Photoshop behaves with no active marquee.
    pub fn is_empty(&self) -> bool {
        if let Some(known) = self.cached_empty.get() {
            return known;
        }
        let empty = self.coverage.iter().all(|&c| c == 0);
        self.cached_empty.set(Some(empty));
        empty
    }

    /// Coverage at a point, `0.0..=1.0`. Outside the canvas reads as 0.
    #[inline]
    pub fn coverage_at(&self, x: i32, y: i32) -> f32 {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return 0.0;
        }
        self.coverage[y as usize * self.width as usize + x as usize] as f32 / 255.0
    }

    #[inline]
    fn set_raw(&mut self, x: i32, y: i32, v: u8) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        self.coverage[y as usize * self.width as usize + x as usize] = v;
        // A single store, but skipping it would leave the caches describing the
        // mask as it was. Callers build whole regions a pixel at a time, so
        // correctness here is worth more than the branch.
        self.cached_bounds = None;
        self.cached_empty.set(None);
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.coverage
    }

    /// Combine a rectangular region into the selection.
    pub fn apply_rect(&mut self, rect: Rect, op: SelectionOp) {
        let mut incoming = Selection::new(self.width, self.height);
        let r = rect.intersect(&Rect::from_size(self.width, self.height));
        for y in r.y..r.bottom() {
            for x in r.x..r.right() {
                incoming.set_raw(x, y, 255);
            }
        }
        self.combine(&incoming, op);
    }

    /// Combine an elliptical region inscribed in `rect`, with antialiased edges.
    pub fn apply_ellipse(&mut self, rect: Rect, op: SelectionOp) {
        let mut incoming = Selection::new(self.width, self.height);
        if !rect.is_empty() {
            let cx = rect.x as f32 + rect.width as f32 / 2.0;
            let cy = rect.y as f32 + rect.height as f32 / 2.0;
            let rx = rect.width as f32 / 2.0;
            let ry = rect.height as f32 / 2.0;

            let scan = rect
                .inflate(1)
                .intersect(&Rect::from_size(self.width, self.height));
            for y in scan.y..scan.bottom() {
                for x in scan.x..scan.right() {
                    // Distance in normalised ellipse space; 1.0 is the edge.
                    let dx = (x as f32 + 0.5 - cx) / rx.max(1e-6);
                    let dy = (y as f32 + 0.5 - cy) / ry.max(1e-6);
                    let d = (dx * dx + dy * dy).sqrt();
                    // Feather across roughly one pixel for antialiasing.
                    let px_step = 1.0 / rx.max(ry).max(1.0);
                    let cov = ((1.0 - d) / px_step + 0.5).clamp(0.0, 1.0);
                    incoming.set_raw(x, y, (cov * 255.0 + 0.5) as u8);
                }
            }
        }
        self.combine(&incoming, op);
    }

    /// Merge `incoming` into `self` under `op`.
    pub fn combine(&mut self, incoming: &Selection, op: SelectionOp) {
        debug_assert_eq!(self.coverage.len(), incoming.coverage.len());
        for (dst, &src) in self.coverage.iter_mut().zip(incoming.coverage.iter()) {
            *dst = match op {
                SelectionOp::Replace => src,
                SelectionOp::Add => (*dst).max(src),
                // Subtract removes `src` worth of coverage.
                SelectionOp::Subtract => dst.saturating_sub(src),
                SelectionOp::Intersect => (*dst).min(src),
            };
        }
        self.cached_bounds = None;
        self.cached_empty.set(None);
    }

    /// Select everything.
    pub fn select_all(&mut self) {
        self.coverage.fill(255);
        self.cached_bounds = Some(Rect::from_size(self.width, self.height));
        self.cached_empty.set(Some(false));
    }

    /// Deselect everything.
    pub fn clear(&mut self) {
        self.coverage.fill(0);
        self.cached_bounds = Some(Rect::default());
        self.cached_empty.set(Some(true));
    }

    /// Select ▸ Inverse.
    pub fn invert(&mut self) {
        for c in self.coverage.iter_mut() {
            *c = 255 - *c;
        }
        self.cached_bounds = None;
        self.cached_empty.set(None);
    }

    /// Feather the edges with a box blur of the given radius.
    ///
    /// Photoshop uses a Gaussian here; a box blur is a close enough
    /// approximation at the radii users actually pick and is much cheaper.
    pub fn feather(&mut self, radius: u32) {
        if radius == 0 || self.width == 0 || self.height == 0 {
            return;
        }
        let r = radius as i32;
        let w = self.width as i32;
        let h = self.height as i32;

        // Horizontal pass.
        let mut tmp = vec![0u8; self.coverage.len()];
        for y in 0..h {
            for x in 0..w {
                let mut sum = 0u32;
                let mut n = 0u32;
                for d in -r..=r {
                    let sx = x + d;
                    if sx < 0 || sx >= w {
                        continue;
                    }
                    sum += self.coverage[y as usize * w as usize + sx as usize] as u32;
                    n += 1;
                }
                tmp[y as usize * w as usize + x as usize] = (sum / n.max(1)) as u8;
            }
        }
        // Vertical pass.
        for y in 0..h {
            for x in 0..w {
                let mut sum = 0u32;
                let mut n = 0u32;
                for d in -r..=r {
                    let sy = y + d;
                    if sy < 0 || sy >= h {
                        continue;
                    }
                    sum += tmp[sy as usize * w as usize + x as usize] as u32;
                    n += 1;
                }
                self.coverage[y as usize * w as usize + x as usize] = (sum / n.max(1)) as u8;
            }
        }
        self.cached_bounds = None;
        self.cached_empty.set(None);
    }

    /// Bounding box of all selected pixels. Empty when nothing is selected.
    pub fn bounds(&mut self) -> Rect {
        if let Some(b) = self.cached_bounds {
            return b;
        }
        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;

        for y in 0..self.height as i32 {
            for x in 0..self.width as i32 {
                if self.coverage[y as usize * self.width as usize + x as usize] > 0 {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }

        let b = if min_x > max_x {
            Rect::default()
        } else {
            Rect::new(
                min_x,
                min_y,
                (max_x - min_x + 1) as u32,
                (max_y - min_y + 1) as u32,
            )
        };
        self.cached_bounds = Some(b);
        // The scan just answered both questions.
        self.cached_empty.set(Some(b.is_empty()));
        b
    }

    /// Trace the selection boundary as closed loops of pixel-corner points, in
    /// document coordinates.
    ///
    /// This is what the marching ants follow. The result is the real contour of
    /// the mask, not its bounding box: an elliptical selection comes back as an
    /// ellipse, a subtracted region leaves a hole as its own loop, and a
    /// multi-part selection returns one loop per part.
    ///
    /// Points sit on pixel *corners*, so a single selected pixel at (3, 4)
    /// yields the loop (3,4) (4,4) (4,5) (3,5) — the ants trace the outside of
    /// the pixel, as Photoshop draws them. Runs along a straight edge are
    /// collapsed to their endpoints, which keeps a full-canvas selection at
    /// four points instead of one per pixel.
    pub fn outline(&self) -> Vec<Vec<(i32, i32)>> {
        let w = self.width as i32;
        let h = self.height as i32;
        if w == 0 || h == 0 {
            return Vec::new();
        }

        let inside = |x: i32, y: i32| -> bool {
            x >= 0
                && y >= 0
                && x < w
                && y < h
                && self.coverage[y as usize * self.width as usize + x as usize] >= OUTLINE_THRESHOLD
        };

        // Every boundary between an inside pixel and an outside one becomes a
        // directed unit edge, wound so the selected side is always on the
        // right. That consistent winding is what makes the walk below trivial:
        // in-degree equals out-degree at every corner, so following edges from
        // any starting corner is guaranteed to come back to it.
        //
        // BTreeMap rather than HashMap so loop order and starting corners are
        // deterministic — an outline that reshuffles between identical
        // selections would make the ants crawl for no reason.
        let mut edges: BTreeMap<(i32, i32), Vec<(i32, i32)>> = BTreeMap::new();
        for y in 0..h {
            for x in 0..w {
                if !inside(x, y) {
                    continue;
                }
                if !inside(x, y - 1) {
                    edges.entry((x, y)).or_default().push((x + 1, y));
                }
                if !inside(x + 1, y) {
                    edges.entry((x + 1, y)).or_default().push((x + 1, y + 1));
                }
                if !inside(x, y + 1) {
                    edges.entry((x + 1, y + 1)).or_default().push((x, y + 1));
                }
                if !inside(x - 1, y) {
                    edges.entry((x, y + 1)).or_default().push((x, y));
                }
            }
        }

        let mut loops: Vec<Vec<(i32, i32)>> = Vec::new();
        while let Some((&start, _)) = edges.iter().next() {
            let mut path: Vec<(i32, i32)> = Vec::new();
            let mut at = start;
            loop {
                let next = match edges.get_mut(&at) {
                    Some(outgoing) => {
                        let next = outgoing.pop();
                        if outgoing.is_empty() {
                            edges.remove(&at);
                        }
                        next
                    }
                    None => None,
                };
                let Some(next) = next else {
                    // Only reachable at a corner where diagonally touching
                    // regions meet and the walk has already used both of its
                    // outgoing edges; the leftovers become their own loop.
                    break;
                };
                path.push(at);
                at = next;
                if at == start {
                    break;
                }
            }
            if path.len() >= 4 {
                loops.push(collapse_runs(&path));
            }
        }
        loops
    }

    /// Render the mask as a greyscale [`Pixmap`], for Quick Mask display and
    /// for turning a selection into a layer mask.
    pub fn to_pixmap(&self) -> Pixmap {
        let mut pm = Pixmap::new(self.width, self.height);
        for (i, &c) in self.coverage.iter().enumerate() {
            let o = i * 4;
            let bytes = pm.as_bytes_mut();
            bytes[o] = c;
            bytes[o + 1] = c;
            bytes[o + 2] = c;
            bytes[o + 3] = c;
        }
        pm
    }

    /// Resize the selection canvas, preserving overlapping coverage.
    pub fn resize(&mut self, width: u32, height: u32) {
        let mut next = vec![0u8; (width as usize) * (height as usize)];
        let copy_w = width.min(self.width) as usize;
        for y in 0..height.min(self.height) as usize {
            let src = y * self.width as usize;
            let dst = y * width as usize;
            next[dst..dst + copy_w].copy_from_slice(&self.coverage[src..src + copy_w]);
        }
        self.coverage = next;
        self.width = width;
        self.height = height;
        self.cached_bounds = None;
        self.cached_empty.set(None);
    }
}

/// Drop the interior points of straight runs in a closed polygon.
///
/// Every step in a traced outline is one pixel along an axis, so a horizontal
/// edge a thousand pixels long arrives as a thousand points. Keeping only the
/// corners costs one pass and saves the painter the rest.
fn collapse_runs(path: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let n = path.len();
    if n < 3 {
        return path.to_vec();
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let prev = path[(i + n - 1) % n];
        let cur = path[i];
        let next = path[(i + 1) % n];
        let incoming = (cur.0 - prev.0, cur.1 - prev.1);
        let outgoing = (next.0 - cur.0, next.1 - cur.1);
        if incoming != outgoing {
            out.push(cur);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_selection_is_empty() {
        let s = Selection::new(8, 8);
        assert!(s.is_empty());
        assert_eq!(s.coverage_at(4, 4), 0.0);
    }

    #[test]
    fn select_all_covers_everything() {
        let s = Selection::all(4, 4);
        assert!(!s.is_empty());
        assert_eq!(s.coverage_at(0, 0), 1.0);
        assert_eq!(s.coverage_at(3, 3), 1.0);
    }

    #[test]
    fn coverage_outside_canvas_is_zero() {
        let s = Selection::all(4, 4);
        assert_eq!(s.coverage_at(-1, 0), 0.0);
        assert_eq!(s.coverage_at(0, 99), 0.0);
    }

    #[test]
    fn rect_replace_selects_only_that_rect() {
        let mut s = Selection::new(8, 8);
        s.apply_rect(Rect::new(2, 2, 3, 3), SelectionOp::Replace);
        assert_eq!(s.coverage_at(2, 2), 1.0);
        assert_eq!(s.coverage_at(4, 4), 1.0);
        assert_eq!(s.coverage_at(5, 5), 0.0);
        assert_eq!(s.coverage_at(1, 1), 0.0);
    }

    #[test]
    fn add_unions_two_rects() {
        let mut s = Selection::new(8, 8);
        s.apply_rect(Rect::new(0, 0, 2, 2), SelectionOp::Replace);
        s.apply_rect(Rect::new(4, 4, 2, 2), SelectionOp::Add);
        assert_eq!(s.coverage_at(0, 0), 1.0);
        assert_eq!(s.coverage_at(5, 5), 1.0);
        assert_eq!(s.coverage_at(3, 3), 0.0);
    }

    #[test]
    fn subtract_removes_overlap() {
        let mut s = Selection::new(8, 8);
        s.apply_rect(Rect::new(0, 0, 4, 4), SelectionOp::Replace);
        s.apply_rect(Rect::new(2, 2, 4, 4), SelectionOp::Subtract);
        assert_eq!(s.coverage_at(0, 0), 1.0);
        assert_eq!(s.coverage_at(3, 3), 0.0);
    }

    #[test]
    fn intersect_keeps_only_the_overlap() {
        let mut s = Selection::new(8, 8);
        s.apply_rect(Rect::new(0, 0, 4, 4), SelectionOp::Replace);
        s.apply_rect(Rect::new(2, 2, 4, 4), SelectionOp::Intersect);
        assert_eq!(s.coverage_at(3, 3), 1.0);
        assert_eq!(s.coverage_at(0, 0), 0.0);
        assert_eq!(s.coverage_at(5, 5), 0.0);
    }

    #[test]
    fn replace_discards_the_previous_selection() {
        let mut s = Selection::new(8, 8);
        s.apply_rect(Rect::new(0, 0, 4, 4), SelectionOp::Replace);
        s.apply_rect(Rect::new(6, 6, 2, 2), SelectionOp::Replace);
        assert_eq!(s.coverage_at(1, 1), 0.0);
        assert_eq!(s.coverage_at(6, 6), 1.0);
    }

    #[test]
    fn invert_flips_coverage() {
        let mut s = Selection::new(4, 4);
        s.apply_rect(Rect::new(0, 0, 2, 2), SelectionOp::Replace);
        s.invert();
        assert_eq!(s.coverage_at(0, 0), 0.0);
        assert_eq!(s.coverage_at(3, 3), 1.0);
    }

    #[test]
    fn bounds_tracks_the_selected_region() {
        let mut s = Selection::new(16, 16);
        s.apply_rect(Rect::new(3, 4, 5, 6), SelectionOp::Replace);
        assert_eq!(s.bounds(), Rect::new(3, 4, 5, 6));
    }

    #[test]
    fn bounds_of_empty_selection_is_empty() {
        let mut s = Selection::new(8, 8);
        assert!(s.bounds().is_empty());
    }

    #[test]
    fn bounds_cache_invalidated_by_edits() {
        let mut s = Selection::new(16, 16);
        s.apply_rect(Rect::new(0, 0, 2, 2), SelectionOp::Replace);
        assert_eq!(s.bounds(), Rect::new(0, 0, 2, 2));
        // A second edit must not return the stale cached box.
        s.apply_rect(Rect::new(10, 10, 2, 2), SelectionOp::Add);
        assert_eq!(s.bounds(), Rect::new(0, 0, 12, 12));
    }

    #[test]
    fn ellipse_selects_the_centre_and_not_the_corners() {
        let mut s = Selection::new(32, 32);
        s.apply_ellipse(Rect::new(0, 0, 32, 32), SelectionOp::Replace);
        assert_eq!(s.coverage_at(16, 16), 1.0);
        assert_eq!(s.coverage_at(0, 0), 0.0, "corner should be outside");
        assert_eq!(s.coverage_at(31, 31), 0.0, "corner should be outside");
    }

    #[test]
    fn ellipse_edges_are_antialiased() {
        let mut s = Selection::new(64, 64);
        s.apply_ellipse(Rect::new(0, 0, 64, 64), SelectionOp::Replace);
        // Somewhere along the boundary there must be partial coverage.
        let partial = (0..64)
            .flat_map(|y| (0..64).map(move |x| (x, y)))
            .any(|(x, y)| {
                let c = s.coverage_at(x, y);
                c > 0.05 && c < 0.95
            });
        assert!(partial, "ellipse edge was not antialiased");
    }

    #[test]
    fn degenerate_ellipse_does_not_panic() {
        let mut s = Selection::new(8, 8);
        s.apply_ellipse(Rect::new(0, 0, 0, 0), SelectionOp::Replace);
        s.apply_ellipse(Rect::new(2, 2, 1, 0), SelectionOp::Replace);
        assert!(s.is_empty());
    }

    #[test]
    fn feather_softens_the_edge() {
        let mut s = Selection::new(32, 32);
        s.apply_rect(Rect::new(8, 8, 16, 16), SelectionOp::Replace);
        s.feather(3);
        // Just outside the original rect now has partial coverage.
        let c = s.coverage_at(7, 16);
        assert!(c > 0.0 && c < 1.0, "expected soft edge, got {}", c);
        // Well inside is still fully selected.
        assert!(s.coverage_at(16, 16) > 0.95);
    }

    #[test]
    fn feather_zero_is_a_no_op() {
        let mut s = Selection::new(8, 8);
        s.apply_rect(Rect::new(2, 2, 4, 4), SelectionOp::Replace);
        let before = s.as_bytes().to_vec();
        s.feather(0);
        assert_eq!(s.as_bytes(), &before[..]);
    }

    #[test]
    fn to_pixmap_mirrors_coverage_into_all_channels() {
        let mut s = Selection::new(4, 4);
        s.apply_rect(Rect::new(0, 0, 2, 2), SelectionOp::Replace);
        let pm = s.to_pixmap();
        assert_eq!(pm.get(0, 0).a, 255);
        assert_eq!(pm.get(0, 0).r, 255);
        assert_eq!(pm.get(3, 3).a, 0);
    }

    #[test]
    fn resize_preserves_overlapping_coverage() {
        let mut s = Selection::new(8, 8);
        s.apply_rect(Rect::new(0, 0, 4, 4), SelectionOp::Replace);
        s.resize(16, 16);
        assert_eq!(s.width(), 16);
        assert_eq!(s.coverage_at(3, 3), 1.0);
        assert_eq!(s.coverage_at(10, 10), 0.0);
    }

    #[test]
    fn resize_smaller_crops() {
        let mut s = Selection::all(8, 8);
        s.resize(4, 4);
        assert_eq!(s.coverage_at(3, 3), 1.0);
        assert_eq!(s.as_bytes().len(), 16);
    }

    /// Loops are closed and rotation-independent, so compare them as sets of
    /// points rather than as sequences.
    fn point_set(loops: &[Vec<(i32, i32)>], index: usize) -> Vec<(i32, i32)> {
        let mut pts = loops[index].clone();
        pts.sort_unstable();
        pts
    }

    #[test]
    fn is_empty_tracks_edits_through_the_cache() {
        let mut s = Selection::new(16, 16);
        assert!(s.is_empty());
        s.apply_rect(Rect::new(2, 2, 4, 4), SelectionOp::Replace);
        assert!(!s.is_empty(), "cache went stale after a selection was made");
        s.apply_rect(Rect::new(2, 2, 4, 4), SelectionOp::Subtract);
        assert!(s.is_empty(), "cache went stale after the selection was removed");
        s.select_all();
        assert!(!s.is_empty());
        s.clear();
        assert!(s.is_empty());
        s.apply_rect(Rect::new(0, 0, 16, 16), SelectionOp::Replace);
        s.invert();
        assert!(s.is_empty(), "inverting a full selection empties it");
    }

    #[test]
    fn bounds_answers_emptiness_too() {
        // bounds() walks the mask, so it should leave is_empty() free.
        let mut s = Selection::new(8, 8);
        s.apply_rect(Rect::new(1, 1, 2, 2), SelectionOp::Replace);
        assert_eq!(s.bounds(), Rect::new(1, 1, 2, 2));
        assert!(!s.is_empty());
    }

    #[test]
    fn outline_of_a_rect_is_its_four_corners() {
        let mut s = Selection::new(16, 16);
        s.apply_rect(Rect::new(2, 3, 4, 5), SelectionOp::Replace);
        let loops = s.outline();
        assert_eq!(loops.len(), 1);
        // Corner coordinates, not pixel centres: the ants run outside the
        // selected pixels.
        assert_eq!(point_set(&loops, 0), vec![(2, 3), (2, 8), (6, 3), (6, 8)]);
    }

    #[test]
    fn outline_of_nothing_is_empty() {
        let s = Selection::new(8, 8);
        assert!(s.outline().is_empty());
    }

    #[test]
    fn outline_of_one_pixel_wraps_it() {
        let mut s = Selection::new(8, 8);
        s.apply_rect(Rect::new(3, 4, 1, 1), SelectionOp::Replace);
        let loops = s.outline();
        assert_eq!(loops.len(), 1);
        assert_eq!(point_set(&loops, 0), vec![(3, 4), (3, 5), (4, 4), (4, 5)]);
    }

    #[test]
    fn outline_of_an_ellipse_is_not_its_bounding_box() {
        // The bug this exists to catch: an elliptical selection whose ants are
        // drawn as a rectangle.
        let mut s = Selection::new(64, 64);
        s.apply_ellipse(Rect::new(0, 0, 64, 64), SelectionOp::Replace);
        let loops = s.outline();
        assert_eq!(loops.len(), 1);
        assert!(
            loops[0].len() > 8,
            "an ellipse should trace many corners, got {}",
            loops[0].len()
        );
        // No corner of the bounding box is on the contour.
        for corner in [(0, 0), (64, 0), (0, 64), (64, 64)] {
            assert!(!loops[0].contains(&corner), "{:?} is on the outline", corner);
        }
    }

    #[test]
    fn outline_returns_one_loop_per_disjoint_region() {
        let mut s = Selection::new(32, 32);
        s.apply_rect(Rect::new(0, 0, 4, 4), SelectionOp::Replace);
        s.apply_rect(Rect::new(20, 20, 4, 4), SelectionOp::Add);
        assert_eq!(s.outline().len(), 2);
    }

    #[test]
    fn outline_traces_a_hole_as_its_own_loop() {
        let mut s = Selection::new(32, 32);
        s.apply_rect(Rect::new(4, 4, 20, 20), SelectionOp::Replace);
        s.apply_rect(Rect::new(10, 10, 6, 6), SelectionOp::Subtract);
        let loops = s.outline();
        assert_eq!(loops.len(), 2, "expected an outer loop and the hole");
        let sizes: Vec<usize> = loops.iter().map(|l| l.len()).collect();
        assert_eq!(sizes, vec![4, 4], "both loops are rectangles");
    }

    #[test]
    fn outline_follows_the_fifty_percent_contour() {
        // Coverage below half is outside the ants, even though those pixels are
        // still partly selected.
        let mut s = Selection::new(8, 8);
        s.apply_rect(Rect::new(2, 2, 4, 4), SelectionOp::Replace);
        s.set_raw(2, 2, 100);
        let loops = s.outline();
        assert_eq!(loops.len(), 1);
        assert!(
            !loops[0].contains(&(2, 2)),
            "a 39% pixel should sit outside the contour"
        );
    }

    #[test]
    fn outline_handles_a_selection_touching_the_canvas_edge() {
        let mut s = Selection::all(8, 8);
        let loops = s.outline();
        assert_eq!(loops.len(), 1);
        assert_eq!(point_set(&loops, 0), vec![(0, 0), (0, 8), (8, 0), (8, 8)]);
        assert!(s.bounds() == Rect::new(0, 0, 8, 8));
    }

    #[test]
    fn outline_handles_diagonally_touching_regions() {
        // Two pixels meeting at a corner give that corner two outgoing edges;
        // the walk must not lose either region.
        let mut s = Selection::new(8, 8);
        s.apply_rect(Rect::new(2, 2, 1, 1), SelectionOp::Replace);
        s.apply_rect(Rect::new(3, 3, 1, 1), SelectionOp::Add);
        let loops = s.outline();
        let total: usize = loops.iter().map(|l| l.len()).sum();
        assert_eq!(total, 8, "both pixels must be traced");
    }

    #[test]
    fn selection_op_from_i32_falls_back_to_replace() {
        assert_eq!(SelectionOp::from_i32(0), SelectionOp::Replace);
        assert_eq!(SelectionOp::from_i32(3), SelectionOp::Intersect);
        assert_eq!(SelectionOp::from_i32(99), SelectionOp::Replace);
    }
}

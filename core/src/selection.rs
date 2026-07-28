//! Selections.
//!
//! A selection is an 8-bit coverage mask over the whole canvas, so feathered
//! and antialiased edges are represented exactly the same way as hard ones —
//! matching Photoshop, where "selected" is a continuous quantity.

use crate::buffer::{Pixmap, Rect};

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
}

impl Selection {
    /// An empty (nothing selected) selection.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            coverage: vec![0u8; (width as usize) * (height as usize)],
            cached_bounds: Some(Rect::default()),
        }
    }

    /// Everything selected — the state after Select ▸ All.
    pub fn all(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            coverage: vec![255u8; (width as usize) * (height as usize)],
            cached_bounds: Some(Rect::from_size(width, height)),
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
        self.coverage.iter().all(|&c| c == 0)
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
    }

    /// Select everything.
    pub fn select_all(&mut self) {
        self.coverage.fill(255);
        self.cached_bounds = Some(Rect::from_size(self.width, self.height));
    }

    /// Deselect everything.
    pub fn clear(&mut self) {
        self.coverage.fill(0);
        self.cached_bounds = Some(Rect::default());
    }

    /// Select ▸ Inverse.
    pub fn invert(&mut self) {
        for c in self.coverage.iter_mut() {
            *c = 255 - *c;
        }
        self.cached_bounds = None;
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
        b
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
    }
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

    #[test]
    fn selection_op_from_i32_falls_back_to_replace() {
        assert_eq!(SelectionOp::from_i32(0), SelectionOp::Replace);
        assert_eq!(SelectionOp::from_i32(3), SelectionOp::Intersect);
        assert_eq!(SelectionOp::from_i32(99), SelectionOp::Replace);
    }
}

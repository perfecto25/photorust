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

/// Sub-scanlines per pixel row when scan-converting a polygon.
///
/// Four is what most rasterisers settle on: the vertical banding on a shallow
/// diagonal is already invisible at 100% zoom, and the cost is linear in this
/// number.
const POLY_SUBSAMPLES: u32 = 4;

/// Accumulate `weight` worth of coverage for the span `x0..x1` into `row`,
/// which covers pixels `origin_x .. origin_x + len`.
///
/// The end pixels take a fraction proportional to how much of them the span
/// covers, which is where the horizontal antialiasing comes from.
fn add_span(row: &mut [f32], origin_x: i32, len: u32, x0: f32, x1: f32, weight: f32) {
    let left = x0.max(origin_x as f32);
    let right = x1.min((origin_x + len as i32) as f32);
    if right <= left {
        return;
    }

    let first = left.floor() as i32;
    let last = (right.ceil() as i32 - 1).max(first);
    for px in first..=last {
        // Overlap between the span and this pixel's 1-wide footprint.
        let lo = left.max(px as f32);
        let hi = right.min(px as f32 + 1.0);
        if hi <= lo {
            continue;
        }
        let index = (px - origin_x) as usize;
        if index < row.len() {
            row[index] += (hi - lo) * weight;
        }
    }
}

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

    /// Wrap a coverage mask that was built elsewhere — by the magic wand or
    /// the quick selector. `None` if it is not exactly `width * height` bytes.
    pub fn from_coverage(width: u32, height: u32, coverage: Vec<u8>) -> Option<Self> {
        if coverage.len() != (width as usize) * (height as usize) {
            return None;
        }
        Some(Self {
            width,
            height,
            coverage,
            cached_bounds: None,
            cached_empty: Cell::new(None),
        })
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
        self.apply_rect_feathered(rect, op, 0);
    }

    /// As `apply_rect`, softening the incoming region first.
    ///
    /// The feather lands on the *new* region, before it is combined — which is
    /// what the options bar's Feather field means. Feathering the whole mask
    /// afterwards (what Select ▸ Feather does) would also re-soften whatever
    /// the selection already held.
    pub fn apply_rect_feathered(&mut self, rect: Rect, op: SelectionOp, feather: u32) {
        let mut incoming = Selection::new(self.width, self.height);
        let r = rect.intersect(&Rect::from_size(self.width, self.height));
        for y in r.y..r.bottom() {
            for x in r.x..r.right() {
                incoming.set_raw(x, y, 255);
            }
        }
        // The mask is empty outside the rect, so the blur can only reach
        // `feather` pixels beyond it.
        incoming.feather_region(feather, rect.inflate(feather));
        self.combine(&incoming, op);
    }

    /// Combine an elliptical region inscribed in `rect`, with antialiased edges.
    pub fn apply_ellipse(&mut self, rect: Rect, op: SelectionOp) {
        self.apply_ellipse_feathered(rect, op, 0);
    }

    /// As `apply_ellipse`, softening the incoming region first.
    pub fn apply_ellipse_feathered(&mut self, rect: Rect, op: SelectionOp, feather: u32) {
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
        // The antialiased edge already spills one pixel past the rect.
        incoming.feather_region(feather, rect.inflate(feather + 1));
        self.combine(&incoming, op);
    }

    /// Combine a closed polygon into the selection, with antialiased edges.
    ///
    /// `points` are document-space vertices; the polygon is implicitly closed
    /// from the last back to the first. This is what the lasso family produces
    /// — a freehand drag is just a polygon with a great many short edges.
    pub fn apply_polygon(&mut self, points: &[(f32, f32)], op: SelectionOp) {
        self.apply_polygon_feathered(points, op, 0);
    }

    /// As `apply_polygon`, softening the incoming region first.
    pub fn apply_polygon_feathered(
        &mut self,
        points: &[(f32, f32)],
        op: SelectionOp,
        feather: u32,
    ) {
        let mut incoming = Selection::new(self.width, self.height);
        // Fewer than three vertices encloses no area at all. Combining the
        // empty mask still does the right thing for every op, so there is no
        // early return: an intersect against nothing must clear the selection.
        let bounds = if points.len() >= 3 {
            incoming.rasterize_polygon(points)
        } else {
            Rect::default()
        };
        // The antialiased edge can spill one pixel past the exact bounds.
        incoming.feather_region(feather, bounds.inflate(feather + 1));
        self.combine(&incoming, op);
    }

    /// Scan-convert `points` into this (assumed empty) mask, returning the
    /// pixel bounds actually touched.
    ///
    /// Coverage comes from `POLY_SUBSAMPLES` sub-scanlines per pixel row, each
    /// filled with fractional coverage at the span ends. That gives vertical
    /// and horizontal antialiasing respectively, for the cost of one pass over
    /// the edge list per sub-scanline — far cheaper than point-sampling every
    /// pixel against every edge.
    fn rasterize_polygon(&mut self, points: &[(f32, f32)]) -> Rect {
        let canvas = Rect::from_size(self.width, self.height);

        let (mut min_x, mut min_y) = (f32::MAX, f32::MAX);
        let (mut max_x, mut max_y) = (f32::MIN, f32::MIN);
        for &(x, y) in points {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }

        let region = Rect::new(
            min_x.floor() as i32,
            min_y.floor() as i32,
            (max_x.ceil() - min_x.floor()).max(0.0) as u32 + 1,
            (max_y.ceil() - min_y.floor()).max(0.0) as u32 + 1,
        )
        .intersect(&canvas);
        if region.is_empty() {
            return Rect::default();
        }

        // One row of accumulated coverage, reused down the region.
        let mut row = vec![0.0f32; region.width as usize];
        // x position of each edge crossing, paired with its winding direction.
        let mut crossings: Vec<(f32, i32)> = Vec::with_capacity(points.len());
        let weight = 1.0 / POLY_SUBSAMPLES as f32;

        for y in region.y..region.bottom() {
            row.fill(0.0);

            for s in 0..POLY_SUBSAMPLES {
                let sy = y as f32 + (s as f32 + 0.5) / POLY_SUBSAMPLES as f32;

                crossings.clear();
                for i in 0..points.len() {
                    let (x0, y0) = points[i];
                    let (x1, y1) = points[(i + 1) % points.len()];
                    // Half-open in y so a vertex exactly on the sub-scanline
                    // is counted once, not twice or zero times.
                    if (y0 <= sy) == (y1 <= sy) {
                        continue;
                    }
                    let t = (sy - y0) / (y1 - y0);
                    let direction = if y0 <= sy { 1 } else { -1 };
                    crossings.push((x0 + t * (x1 - x0), direction));
                }
                if crossings.len() < 2 {
                    continue;
                }
                crossings.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

                // Nonzero winding, which is what a freehand loop wants: a
                // stroke that crosses back over itself stays solid instead of
                // punching an even-odd hole where the user's hand wobbled.
                let mut winding = 0;
                for pair in crossings.windows(2) {
                    winding += pair[0].1;
                    if winding != 0 {
                        add_span(&mut row, region.x, region.width, pair[0].0, pair[1].0, weight);
                    }
                }
            }

            for (i, &cov) in row.iter().enumerate() {
                if cov > 0.0 {
                    let value = (cov.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    self.set_raw(region.x + i as i32, y, value);
                }
            }
        }

        region
    }

    /// Combine a coverage mask built elsewhere, softening it first.
    ///
    /// The route the magic wand and quick selector take into the selection:
    /// they produce a whole-canvas mask rather than a shape, but the feather
    /// and combine semantics are identical to the other `apply_*` calls.
    /// A mask of the wrong size is ignored rather than panicking.
    pub fn apply_mask_feathered(&mut self, coverage: &[u8], op: SelectionOp, feather: u32) {
        if coverage.len() != self.coverage.len() {
            debug_assert!(false, "mask size does not match the canvas");
            return;
        }
        let Some(mut incoming) = Selection::from_coverage(self.width, self.height, coverage.to_vec())
        else {
            return;
        };
        incoming.feather(feather);
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
        let canvas = Rect::from_size(self.width, self.height);
        self.feather_region(radius, canvas);
    }

    /// Feather, writing only inside `region`.
    ///
    /// A caller that knows the mask is empty outside a box — which is the case
    /// for the shape a marquee just drew — passes it here, so the work tracks
    /// the size of the shape instead of the size of the canvas. Samples
    /// outside `region` read as 0, so the result matches a whole-canvas
    /// feather exactly whenever that assumption holds.
    ///
    /// Both passes carry a running sum across the row or column rather than
    /// re-adding the kernel at every pixel, so the cost no longer scales with
    /// the radius: feathering by 50 px costs what feathering by 1 px costs.
    /// This is a hot path — it runs on every mouse-up with a selection tool.
    pub fn feather_region(&mut self, radius: u32, region: Rect) {
        if radius == 0 || self.width == 0 || self.height == 0 {
            return;
        }
        let region = region.intersect(&Rect::from_size(self.width, self.height));
        if region.is_empty() {
            return;
        }

        let r = radius as i32;
        let w = self.width as i32;
        let h = self.height as i32;
        let (rx, ry) = (region.x, region.y);
        let (rw, rh) = (region.width as i32, region.height as i32);

        // Horizontal pass, into a scratch buffer the size of the region.
        //
        // Each output is the mean of the kernel samples that land on the
        // canvas, so the divisor shrinks towards the edges — the same
        // normalisation the per-pixel version used.
        let mut tmp = vec![0u8; (rw * rh) as usize];
        for y in ry..ry + rh {
            let row = y as usize * w as usize;
            let mut lo = (rx - r).max(0);
            let mut hi = (rx + r).min(w - 1);
            let mut sum: u32 = self.coverage[row + lo as usize..=row + hi as usize]
                .iter()
                .map(|&c| c as u32)
                .sum();

            for x in rx..rx + rw {
                // Both bounds only ever move right, so each sample enters and
                // leaves the window at most once per row.
                let want_lo = (x - r).max(0);
                let want_hi = (x + r).min(w - 1);
                while lo < want_lo {
                    sum -= self.coverage[row + lo as usize] as u32;
                    lo += 1;
                }
                while hi < want_hi {
                    hi += 1;
                    sum += self.coverage[row + hi as usize] as u32;
                }
                let n = (want_hi - want_lo + 1) as u32;
                tmp[((y - ry) * rw + (x - rx)) as usize] = (sum / n) as u8;
            }
        }

        // Vertical pass, reading the scratch buffer a column at a time. Rows
        // outside the region contribute 0 but still count towards the divisor,
        // which is what keeps this identical to the whole-canvas result.
        for x in rx..rx + rw {
            let col = (x - rx) as usize;
            let sample = |buf: &[u8], y: i32| -> u32 {
                if y < ry || y >= ry + rh {
                    0
                } else {
                    buf[((y - ry) * rw) as usize + col] as u32
                }
            };

            let mut lo = (ry - r).max(0);
            let mut hi = (ry + r).min(h - 1);
            let mut sum: u32 = (lo..=hi).map(|y| sample(&tmp, y)).sum();

            for y in ry..ry + rh {
                let want_lo = (y - r).max(0);
                let want_hi = (y + r).min(h - 1);
                while lo < want_lo {
                    sum -= sample(&tmp, lo);
                    lo += 1;
                }
                while hi < want_hi {
                    hi += 1;
                    sum += sample(&tmp, hi);
                }
                let n = (want_hi - want_lo + 1) as u32;
                self.coverage[y as usize * w as usize + x as usize] = (sum / n) as u8;
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
    /// Crop to `rect`, keeping the coverage that falls inside it.
    ///
    /// Unlike `resize`, which keeps the top-left corner, this takes the
    /// coverage from an arbitrary offset — what the Crop tool needs, since the
    /// kept region rarely starts at the origin.
    pub fn crop(&mut self, rect: Rect) {
        let mut next = vec![0u8; (rect.width as usize) * (rect.height as usize)];
        for y in 0..rect.height as i32 {
            for x in 0..rect.width as i32 {
                let (sx, sy) = (rect.x + x, rect.y + y);
                if sx < 0 || sy < 0 || sx >= self.width as i32 || sy >= self.height as i32 {
                    continue;
                }
                next[(y as usize) * (rect.width as usize) + x as usize] =
                    self.coverage[(sy as usize) * (self.width as usize) + sx as usize];
            }
        }
        self.coverage = next;
        self.width = rect.width;
        self.height = rect.height;
        self.cached_bounds = None;
        self.cached_empty.set(None);
    }

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

    /// The straightforward per-pixel box blur the running-sum version replaced.
    /// Kept here as the oracle for it.
    fn naive_feather(s: &Selection, radius: i32) -> Vec<u8> {
        let (w, h) = (s.width as i32, s.height as i32);
        let mut tmp = vec![0u8; s.coverage.len()];
        for y in 0..h {
            for x in 0..w {
                let (mut sum, mut n) = (0u32, 0u32);
                for d in -radius..=radius {
                    let sx = x + d;
                    if sx >= 0 && sx < w {
                        sum += s.coverage[(y * w + sx) as usize] as u32;
                        n += 1;
                    }
                }
                tmp[(y * w + x) as usize] = (sum / n.max(1)) as u8;
            }
        }
        let mut out = vec![0u8; s.coverage.len()];
        for y in 0..h {
            for x in 0..w {
                let (mut sum, mut n) = (0u32, 0u32);
                for d in -radius..=radius {
                    let sy = y + d;
                    if sy >= 0 && sy < h {
                        sum += tmp[(sy * w + x) as usize] as u32;
                        n += 1;
                    }
                }
                out[(y * w + x) as usize] = (sum / n.max(1)) as u8;
            }
        }
        out
    }

    #[test]
    fn running_sum_feather_matches_the_per_pixel_blur() {
        for &radius in &[1, 3, 7, 20] {
            let mut s = Selection::new(37, 29);
            // An off-centre rect plus a blob touching the canvas edge, so the
            // clamped-divisor behaviour at the borders is exercised too.
            s.apply_rect(Rect::new(5, 4, 11, 9), SelectionOp::Replace);
            s.apply_rect(Rect::new(30, 0, 7, 6), SelectionOp::Add);

            let expected = naive_feather(&s, radius);
            s.feather(radius as u32);
            assert_eq!(s.as_bytes(), &expected[..], "radius {}", radius);
        }
    }

    #[test]
    fn region_limited_feather_matches_a_whole_canvas_one() {
        // The marquee path feathers only the box the shape can reach. That has
        // to give the same mask as feathering everything.
        let rect = Rect::new(8, 6, 20, 14);
        for &radius in &[1, 5, 12] {
            let mut whole = Selection::new(48, 40);
            whole.apply_rect(rect, SelectionOp::Replace);
            whole.feather(radius);

            let mut region = Selection::new(48, 40);
            region.apply_rect(rect, SelectionOp::Replace);
            region.feather_region(radius, rect.inflate(radius));

            assert_eq!(region.as_bytes(), whole.as_bytes(), "radius {}", radius);
        }
    }

    #[test]
    fn feathered_apply_softens_only_the_new_region() {
        let mut s = Selection::new(64, 64);
        // A hard-edged first selection, then a feathered one added beside it.
        s.apply_rect(Rect::new(4, 4, 16, 56), SelectionOp::Replace);
        s.apply_rect_feathered(Rect::new(32, 4, 16, 56), SelectionOp::Add, 3);

        // The original region keeps its hard edge…
        assert_eq!(s.coverage_at(3, 32), 0.0, "old region bled outward");
        assert_eq!(s.coverage_at(4, 32), 1.0, "old region was softened");
        // …while the region just added has a falloff.
        let soft = s.coverage_at(31, 32);
        assert!(soft > 0.0 && soft < 1.0, "expected soft edge, got {}", soft);
    }

    #[test]
    fn feathered_apply_with_zero_matches_the_hard_edged_call() {
        let mut soft = Selection::new(16, 16);
        soft.apply_rect_feathered(Rect::new(2, 2, 8, 8), SelectionOp::Replace, 0);
        let mut hard = Selection::new(16, 16);
        hard.apply_rect(Rect::new(2, 2, 8, 8), SelectionOp::Replace);
        assert_eq!(soft.as_bytes(), hard.as_bytes());
    }

    #[test]
    fn polygon_rectangle_matches_the_rect_call() {
        // A polygon tracing a rectangle on pixel boundaries must scan-convert
        // to exactly what apply_rect produces — no half-covered edge pixels.
        let mut poly = Selection::new(32, 32);
        poly.apply_polygon(
            &[(8.0, 4.0), (24.0, 4.0), (24.0, 20.0), (8.0, 20.0)],
            SelectionOp::Replace,
        );
        let mut rect = Selection::new(32, 32);
        rect.apply_rect(Rect::new(8, 4, 16, 16), SelectionOp::Replace);
        assert_eq!(poly.as_bytes(), rect.as_bytes());
    }

    #[test]
    fn polygon_fills_its_interior_and_nothing_outside() {
        let mut s = Selection::new(64, 64);
        // A triangle with its right angle at the top left.
        s.apply_polygon(&[(8.0, 8.0), (56.0, 8.0), (8.0, 56.0)], SelectionOp::Replace);

        assert_eq!(s.coverage_at(12, 12), 1.0, "interior not filled");
        assert_eq!(s.coverage_at(50, 50), 0.0, "outside the hypotenuse was filled");
        assert_eq!(s.coverage_at(2, 2), 0.0, "outside the bounding box was filled");

        // The diagonal edge is antialiased rather than hard. The hypotenuse is
        // x + y = 64, so this pixel's footprint straddles it.
        let edge = s.coverage_at(31, 32);
        assert!(edge > 0.0 && edge < 1.0, "expected a soft diagonal, got {}", edge);
    }

    #[test]
    fn polygon_respects_the_combine_ops() {
        let square = [(0.0, 0.0), (32.0, 0.0), (32.0, 32.0), (0.0, 32.0)];
        let right = [(16.0, 0.0), (48.0, 0.0), (48.0, 32.0), (16.0, 32.0)];

        let mut s = Selection::new(48, 32);
        s.apply_polygon(&square, SelectionOp::Replace);
        s.apply_polygon(&right, SelectionOp::Intersect);
        assert_eq!(s.coverage_at(8, 16), 0.0, "intersect kept the left half");
        assert_eq!(s.coverage_at(24, 16), 1.0, "intersect dropped the overlap");

        let mut s = Selection::new(48, 32);
        s.apply_polygon(&square, SelectionOp::Replace);
        s.apply_polygon(&right, SelectionOp::Subtract);
        assert_eq!(s.coverage_at(8, 16), 1.0, "subtract removed the wrong half");
        assert_eq!(s.coverage_at(24, 16), 0.0, "subtract left the overlap behind");
    }

    #[test]
    fn polygon_clips_to_the_canvas() {
        // Vertices well outside the canvas must not panic or wrap around.
        let mut s = Selection::new(16, 16);
        s.apply_polygon(
            &[(-40.0, -40.0), (60.0, -40.0), (60.0, 60.0), (-40.0, 60.0)],
            SelectionOp::Replace,
        );
        assert_eq!(s.coverage_at(0, 0), 1.0);
        assert_eq!(s.coverage_at(15, 15), 1.0);
    }

    #[test]
    fn polygon_with_too_few_points_selects_nothing() {
        let mut s = Selection::new(16, 16);
        s.apply_rect(Rect::new(2, 2, 8, 8), SelectionOp::Replace);
        // A click without a drag: replace with a degenerate shape clears.
        s.apply_polygon(&[(4.0, 4.0), (5.0, 5.0)], SelectionOp::Replace);
        assert!(s.is_empty());
    }

    #[test]
    fn polygon_uses_nonzero_winding_not_even_odd() {
        // A path that doubles back over itself, as a shaky freehand drag does.
        // Under even-odd the overlapped middle would come out unselected.
        let mut s = Selection::new(64, 32);
        s.apply_polygon(
            &[
                (4.0, 4.0),
                (60.0, 4.0),
                (60.0, 28.0),
                (4.0, 28.0),
                // Back around the middle in the same direction, overlapping.
                (4.0, 4.0),
                (40.0, 4.0),
                (40.0, 28.0),
                (20.0, 28.0),
                (20.0, 4.0),
            ],
            SelectionOp::Replace,
        );
        assert_eq!(s.coverage_at(30, 16), 1.0, "the overlapped region got a hole");
        assert_eq!(s.coverage_at(50, 16), 1.0, "the outer region was dropped");
    }

    #[test]
    fn feathered_polygon_softens_its_edge() {
        let mut hard = Selection::new(64, 64);
        let square = [(16.0, 16.0), (48.0, 16.0), (48.0, 48.0), (16.0, 48.0)];
        hard.apply_polygon(&square, SelectionOp::Replace);
        assert_eq!(hard.coverage_at(15, 32), 0.0);

        let mut soft = Selection::new(64, 64);
        soft.apply_polygon_feathered(&square, SelectionOp::Replace, 4);
        let bleed = soft.coverage_at(15, 32);
        assert!(bleed > 0.0 && bleed < 1.0, "expected falloff, got {}", bleed);
        assert_eq!(soft.coverage_at(32, 32), 1.0, "the middle should stay solid");
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

//! Vector paths — the data behind the Pen tool and the Paths panel.
//!
//! A path is built from **anchor points** joined by straight or curved
//! segments. Each anchor optionally carries two direction handles, stored as
//! *absolute* positions rather than offsets, which is what lets every operation
//! here — move a point, split a segment, drag a handle — read as "set this
//! position" instead of "add this delta to that other delta".
//!
//! A **corner** point has no handles (or two independent ones) and the
//! segments either side of it meet at an angle. A **smooth** point's handles
//! stay collinear through the anchor, so the curve flows through it without a
//! kink — that collinearity is the one invariant [`VectorPath::move_handle`]
//! has to maintain and everything else leaves alone.
//!
//! A segment with no handle on either end is a straight line. This needs no
//! special case anywhere: a cubic Bezier whose control points sit *on* its
//! endpoints (`P1 = P0`, `P2 = P3`) already draws a straight line, so
//! [`Subpath::segment`] simply defaults a missing handle to its own anchor and
//! every curve routine downstream — flattening, hit-testing, splitting — stays
//! ignorant of the distinction.
//!
//! Path edits are deliberately **not** part of the undo history, the same
//! choice already made for slices and annotations (see `slice.rs`): they are
//! vector overlay data, not pixels, and folding them into the layer-stack
//! snapshots history takes would mean carrying them through every undo step no
//! tool actually touches. What a finished path *does* to the image — Fill
//! Path, Stroke Path, Make Selection — commits normally, like any other paint.

use crate::buffer::Rect;

/// Which of a point's two handles.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HandleSide {
    In,
    Out,
}

/// One anchor point on a subpath.
#[derive(Clone, Copy, Debug)]
pub struct PathPoint {
    pub anchor: (f32, f32),
    /// The handle curving the segment arriving at this point.
    pub in_handle: Option<(f32, f32)>,
    /// The handle curving the segment leaving this point.
    pub out_handle: Option<(f32, f32)>,
    /// Whether the two handles are kept collinear through the anchor when
    /// either is dragged. A point can hold handles and still not be smooth —
    /// that is what an Alt-dragged handle leaves behind.
    pub smooth: bool,
}

impl PathPoint {
    fn corner(anchor: (f32, f32)) -> Self {
        Self { anchor, in_handle: None, out_handle: None, smooth: false }
    }
}

/// One contiguous run of anchors — what Photoshop draws as a single outline
/// before you lift the pen and start elsewhere, or close back to the start.
#[derive(Clone, Debug, Default)]
pub struct Subpath {
    pub points: Vec<PathPoint>,
    /// Whether the last point connects back to the first.
    pub closed: bool,
}

impl Subpath {
    /// Number of drawable segments: one fewer than the points if open, or one
    /// per point (the last wrapping to the first) if closed.
    pub fn segment_count(&self) -> usize {
        if self.points.len() < 2 {
            0
        } else if self.closed {
            self.points.len()
        } else {
            self.points.len() - 1
        }
    }

    /// The four Bezier control points of segment `i`: the two anchors and the
    /// handles between them, defaulting a missing handle to its own anchor so
    /// the result is a straight line exactly where the path has no curve.
    pub fn segment(&self, i: usize) -> Option<[(f32, f32); 4]> {
        if i >= self.segment_count() {
            return None;
        }
        let a = &self.points[i];
        let b = &self.points[(i + 1) % self.points.len()];
        Some([
            a.anchor,
            a.out_handle.unwrap_or(a.anchor),
            b.in_handle.unwrap_or(b.anchor),
            b.anchor,
        ])
    }
}

/// One path: possibly several disconnected outlines, edited together.
///
/// `editing` tracks which subpath the Pen tool is currently extending — the
/// one a fresh anchor gets appended to. It is `None` between drawing sessions
/// (after closing, finishing, or when nothing has been drawn yet), which is
/// what tells [`VectorPath::append_corner`] to start a new one rather than
/// resuming an old one.
#[derive(Clone, Debug, Default)]
pub struct VectorPath {
    pub subpaths: Vec<Subpath>,
    editing: Option<usize>,
}

/// Maximum recursion when flattening a curve — plenty for any Bezier that
/// isn't pathological, and cheap insurance against one that is.
const FLATTEN_MAX_DEPTH: u32 = 10;

impl VectorPath {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.subpaths.iter().all(|s| s.points.is_empty())
    }

    /// The bounding box of every anchor and handle — generous but cheap, and
    /// exact enough for scrolling a new path into view or sizing an overlay
    /// redraw.
    pub fn bounds(&self) -> Rect {
        let (mut min_x, mut min_y) = (f32::MAX, f32::MAX);
        let (mut max_x, mut max_y) = (f32::MIN, f32::MIN);
        let mut touch = |p: (f32, f32)| {
            min_x = min_x.min(p.0);
            min_y = min_y.min(p.1);
            max_x = max_x.max(p.0);
            max_y = max_y.max(p.1);
        };
        for sp in &self.subpaths {
            for pt in &sp.points {
                touch(pt.anchor);
                if let Some(h) = pt.in_handle {
                    touch(h);
                }
                if let Some(h) = pt.out_handle {
                    touch(h);
                }
            }
        }
        if min_x > max_x {
            return Rect::default();
        }
        Rect::new(
            min_x.floor() as i32,
            min_y.floor() as i32,
            (max_x - min_x).max(0.0) as u32 + 1,
            (max_y - min_y).max(0.0) as u32 + 1,
        )
    }

    // -- drawing with the Pen tool -------------------------------------------

    /// Append a corner anchor, starting a new subpath if none is being edited.
    /// Returns the new point's `(subpath, point)` index.
    pub fn append_corner(&mut self, x: f32, y: f32) -> (usize, usize) {
        let sp = match self.editing {
            Some(i) => i,
            None => {
                self.subpaths.push(Subpath::default());
                let i = self.subpaths.len() - 1;
                self.editing = Some(i);
                i
            }
        };
        self.subpaths[sp].points.push(PathPoint::corner((x, y)));
        (sp, self.subpaths[sp].points.len() - 1)
    }

    /// Live-update the handle of the point last appended, as the Pen tool
    /// drags away from where it clicked. Below about a pixel of drag the point
    /// stays a plain corner, so a stray twitch on a click does not leave it
    /// carrying an invisible curve.
    ///
    /// `independent`, CS6's Alt-drag-while-placing, sets only the outgoing
    /// handle: the segment about to be drawn curves away from this point while
    /// the one already drawn into it is unaffected — a straight-in, curved-out
    /// corner, which is how a rounded shape with one sharp corner gets drawn by
    /// hand.
    pub fn update_last_handle(&mut self, x: f32, y: f32, independent: bool) -> bool {
        let Some(sp) = self.editing else { return false };
        let Some(pt) = self.subpaths[sp].points.last_mut() else {
            return false;
        };
        let (ax, ay) = pt.anchor;
        let (dx, dy) = (x - ax, y - ay);
        if dx.hypot(dy) < 1.0 {
            pt.smooth = false;
            pt.in_handle = None;
            pt.out_handle = None;
            return true;
        }
        pt.out_handle = Some((x, y));
        if independent {
            pt.smooth = false;
            pt.in_handle = None;
        } else {
            pt.smooth = true;
            pt.in_handle = Some((2.0 * ax - x, 2.0 * ay - y));
        }
        true
    }

    /// Close the subpath being edited back to its first anchor. Refused with
    /// fewer than two points — closing a single point encloses nothing.
    pub fn close_active_subpath(&mut self) -> bool {
        let Some(sp) = self.editing else { return false };
        if self.subpaths[sp].points.len() < 2 {
            return false;
        }
        self.subpaths[sp].closed = true;
        self.editing = None;
        true
    }

    /// Stop extending the current subpath without closing it — Enter,
    /// double-click, Escape, or switching tools.
    pub fn finish_editing(&mut self) {
        self.editing = None;
    }

    pub fn is_editing(&self) -> bool {
        self.editing.is_some()
    }

    /// The subpath index the Pen tool would extend next, if any.
    pub fn editing_subpath(&self) -> Option<usize> {
        self.editing
    }

    // -- Direct Selection -----------------------------------------------------

    /// Move an anchor to an absolute position, carrying its handles with it by
    /// the same delta so the curve either side keeps its shape.
    pub fn move_anchor(&mut self, sp: usize, pt: usize, x: f32, y: f32) -> bool {
        let Some(point) = self.point_mut(sp, pt) else { return false };
        let (dx, dy) = (x - point.anchor.0, y - point.anchor.1);
        point.anchor = (x, y);
        if let Some((hx, hy)) = point.in_handle {
            point.in_handle = Some((hx + dx, hy + dy));
        }
        if let Some((hx, hy)) = point.out_handle {
            point.out_handle = Some((hx + dx, hy + dy));
        }
        true
    }

    /// Drag one handle to an absolute position.
    ///
    /// On a smooth point this keeps the opposite handle collinear through the
    /// anchor — its own length is left alone, only its angle follows — unless
    /// `independent` (Alt) is held, which moves just the one handle and
    /// permanently breaks the point's smoothness, exactly as Direct Selection
    /// does in Photoshop.
    pub fn move_handle(
        &mut self,
        sp: usize,
        pt: usize,
        side: HandleSide,
        x: f32,
        y: f32,
        independent: bool,
    ) -> bool {
        let Some(point) = self.point_mut(sp, pt) else { return false };
        let anchor = point.anchor;
        match side {
            HandleSide::In => point.in_handle = Some((x, y)),
            HandleSide::Out => point.out_handle = Some((x, y)),
        }
        if independent {
            point.smooth = false;
            return true;
        }
        if !point.smooth {
            return true;
        }
        let (dx, dy) = (x - anchor.0, y - anchor.1);
        let dist = dx.hypot(dy).max(1e-6);
        let (dir_x, dir_y) = (dx / dist, dy / dist);
        let other = match side {
            HandleSide::In => &mut point.out_handle,
            HandleSide::Out => &mut point.in_handle,
        };
        let length = other.map_or(dist, |(ox, oy)| (ox - anchor.0).hypot(oy - anchor.1));
        *other = Some((anchor.0 - dir_x * length, anchor.1 - dir_y * length));
        true
    }

    /// Convert Point's click: strip both handles, leaving a plain corner.
    pub fn set_corner(&mut self, sp: usize, pt: usize) -> bool {
        let Some(point) = self.point_mut(sp, pt) else { return false };
        point.in_handle = None;
        point.out_handle = None;
        point.smooth = false;
        true
    }

    /// Convert Point's drag from a corner: pull out a fresh symmetric pair of
    /// handles, turning the point smooth.
    pub fn drag_new_handles(&mut self, sp: usize, pt: usize, x: f32, y: f32) -> bool {
        let Some(point) = self.point_mut(sp, pt) else { return false };
        let (ax, ay) = point.anchor;
        point.out_handle = Some((x, y));
        point.in_handle = Some((2.0 * ax - x, 2.0 * ay - y));
        point.smooth = true;
        true
    }

    /// Move a whole subpath by a delta — the Path Selection tool.
    pub fn move_subpath(&mut self, sp: usize, dx: f32, dy: f32) -> bool {
        let Some(subpath) = self.subpaths.get_mut(sp) else { return false };
        for point in &mut subpath.points {
            point.anchor.0 += dx;
            point.anchor.1 += dy;
            if let Some(h) = point.in_handle.as_mut() {
                h.0 += dx;
                h.1 += dy;
            }
            if let Some(h) = point.out_handle.as_mut() {
                h.0 += dx;
                h.1 += dy;
            }
        }
        true
    }

    // -- Add / Delete Anchor Point --------------------------------------------

    /// Split segment `seg` of subpath `sp` at parameter `t`, inserting a new
    /// anchor exactly on the curve — De Casteljau's algorithm, which is what
    /// keeps the visible shape identical before and after the split.
    ///
    /// A straight segment (no handle on either end) splits into two straight
    /// segments with no handles, rather than manufacturing a curve that was
    /// never there.
    pub fn insert_anchor(&mut self, sp: usize, seg: usize, t: f32) -> bool {
        let Some(subpath) = self.subpaths.get_mut(sp) else { return false };
        let Some(quad) = subpath.segment(seg) else { return false };
        let t = t.clamp(0.0, 1.0);
        let [p0, p1, p2, p3] = quad;
        let had_curve = subpath.points[seg].out_handle.is_some()
            || subpath.points[(seg + 1) % subpath.points.len()].in_handle.is_some();

        let p01 = lerp(p0, p1, t);
        let p12 = lerp(p1, p2, t);
        let p23 = lerp(p2, p3, t);
        let p012 = lerp(p01, p12, t);
        let p123 = lerp(p12, p23, t);
        let mid = lerp(p012, p123, t);

        let new_point = PathPoint {
            anchor: mid,
            in_handle: had_curve.then_some(p012),
            out_handle: had_curve.then_some(p123),
            smooth: had_curve,
        };

        let next = (seg + 1) % subpath.points.len();
        subpath.points[seg].out_handle = had_curve.then_some(p01);
        // Insert before `next`, except when the segment wraps a closed
        // subpath's last point back to its first — there `next` is index 0
        // and the new point belongs at the end instead.
        if next == 0 && subpath.closed {
            subpath.points[0].in_handle = had_curve.then_some(p23);
            subpath.points.push(new_point);
        } else {
            subpath.points[next].in_handle = had_curve.then_some(p23);
            subpath.points.insert(next, new_point);
        }
        true
    }

    /// Remove an anchor. Emptying a subpath removes it entirely, and shifts
    /// `editing` to track the same subpath if a later one was renumbered out
    /// from under it.
    pub fn delete_anchor(&mut self, sp: usize, pt: usize) -> bool {
        let Some(subpath) = self.subpaths.get_mut(sp) else { return false };
        if pt >= subpath.points.len() {
            return false;
        }
        subpath.points.remove(pt);
        if subpath.points.is_empty() {
            self.subpaths.remove(sp);
            self.editing = match self.editing {
                Some(e) if e == sp => None,
                Some(e) if e > sp => Some(e - 1),
                other => other,
            };
        }
        true
    }

    fn point_mut(&mut self, sp: usize, pt: usize) -> Option<&mut PathPoint> {
        self.subpaths.get_mut(sp)?.points.get_mut(pt)
    }

    // -- hit testing ------------------------------------------------------

    /// The nearest anchor within `radius` document units, if any.
    pub fn hit_anchor(&self, x: f32, y: f32, radius: f32) -> Option<(usize, usize)> {
        let mut best: Option<(usize, usize, f32)> = None;
        for (si, sp) in self.subpaths.iter().enumerate() {
            for (pi, pt) in sp.points.iter().enumerate() {
                let d = (pt.anchor.0 - x).hypot(pt.anchor.1 - y);
                if d <= radius && best.is_none_or(|(_, _, bd)| d < bd) {
                    best = Some((si, pi, d));
                }
            }
        }
        best.map(|(s, p, _)| (s, p))
    }

    /// The nearest handle within `radius`, if any. Only handles that exist are
    /// considered — there is nothing to grab on a straight corner.
    pub fn hit_handle(&self, x: f32, y: f32, radius: f32) -> Option<(usize, usize, HandleSide)> {
        let mut best: Option<(usize, usize, HandleSide, f32)> = None;
        for (si, sp) in self.subpaths.iter().enumerate() {
            for (pi, pt) in sp.points.iter().enumerate() {
                for (side, h) in [(HandleSide::In, pt.in_handle), (HandleSide::Out, pt.out_handle)]
                {
                    let Some(h) = h else { continue };
                    let d = (h.0 - x).hypot(h.1 - y);
                    if d <= radius && best.is_none_or(|(_, _, _, bd)| d < bd) {
                        best = Some((si, pi, side, d));
                    }
                }
            }
        }
        best.map(|(s, p, side, _)| (s, p, side))
    }

    /// The nearest point on any segment within `radius`, as `(subpath, segment,
    /// t)` — what Auto Add and the Add Anchor Point tool hit-test against.
    pub fn hit_segment(&self, x: f32, y: f32, radius: f32) -> Option<(usize, usize, f32)> {
        let mut best: Option<(usize, usize, f32, f32)> = None;
        for (si, sp) in self.subpaths.iter().enumerate() {
            for seg in 0..sp.segment_count() {
                let Some(quad) = sp.segment(seg) else { continue };
                let (t, d) = nearest_on_cubic(quad, (x, y));
                if d <= radius && best.is_none_or(|(_, _, _, bd)| d < bd) {
                    best = Some((si, seg, t, d));
                }
            }
        }
        best.map(|(s, seg, t, _)| (s, seg, t))
    }

    /// The subpath nearest `(x, y)` — near any of its segments, or anywhere
    /// inside it if closed — for the Path Selection tool.
    pub fn hit_subpath(&self, x: f32, y: f32, radius: f32) -> Option<usize> {
        for (si, sp) in self.subpaths.iter().enumerate() {
            if sp.closed && point_in_polygon(&self.flatten_subpath(si, 0.5), (x, y)) {
                return Some(si);
            }
        }
        self.hit_segment(x, y, radius).map(|(s, _, _)| s)
    }

    // -- flattening ---------------------------------------------------------

    /// Every subpath as a polyline, within `tolerance` document units of the
    /// true curve — what rendering and rasterising both work from.
    pub fn flatten(&self, tolerance: f32) -> Vec<(Vec<(f32, f32)>, bool)> {
        (0..self.subpaths.len())
            .map(|i| (self.flatten_subpath(i, tolerance), self.subpaths[i].closed))
            .collect()
    }

    fn flatten_subpath(&self, index: usize, tolerance: f32) -> Vec<(f32, f32)> {
        let sp = &self.subpaths[index];
        if sp.points.is_empty() {
            return Vec::new();
        }
        let mut out = vec![sp.points[0].anchor];
        for seg in 0..sp.segment_count() {
            let Some([p0, p1, p2, p3]) = sp.segment(seg) else { continue };
            flatten_cubic(p0, p1, p2, p3, tolerance, FLATTEN_MAX_DEPTH, &mut out);
        }
        out
    }
}

fn lerp(a: (f32, f32), b: (f32, f32), t: f32) -> (f32, f32) {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

/// Recursively split a cubic Bezier until each piece is flat within
/// `tolerance`, appending its far endpoint to `out` (the near endpoint is
/// already there, from the previous segment or the subpath's first anchor).
fn flatten_cubic(
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
    tolerance: f32,
    depth: u32,
    out: &mut Vec<(f32, f32)>,
) {
    if depth == 0 || is_flat(p0, p1, p2, p3, tolerance) {
        out.push(p3);
        return;
    }
    let p01 = lerp(p0, p1, 0.5);
    let p12 = lerp(p1, p2, 0.5);
    let p23 = lerp(p2, p3, 0.5);
    let p012 = lerp(p01, p12, 0.5);
    let p123 = lerp(p12, p23, 0.5);
    let mid = lerp(p012, p123, 0.5);
    flatten_cubic(p0, p01, p012, mid, tolerance, depth - 1, out);
    flatten_cubic(mid, p123, p23, p3, tolerance, depth - 1, out);
}

/// Whether both control points sit within `tolerance` of the chord — the
/// standard flatness test for adaptive Bezier subdivision.
fn is_flat(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32), tolerance: f32) -> bool {
    distance_to_segment(p1, p0, p3) <= tolerance && distance_to_segment(p2, p0, p3) <= tolerance
}

fn distance_to_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (abx, aby) = (b.0 - a.0, b.1 - a.1);
    let len2 = abx * abx + aby * aby;
    if len2 <= 1e-9 {
        return (p.0 - a.0).hypot(p.1 - a.1);
    }
    let t = (((p.0 - a.0) * abx + (p.1 - a.1) * aby) / len2).clamp(0.0, 1.0);
    let proj = (a.0 + abx * t, a.1 + aby * t);
    (p.0 - proj.0).hypot(p.1 - proj.1)
}

/// Closest point on a cubic Bezier to `p`, found by sampling — exact enough
/// for hit-testing at interactive tolerances, and far simpler than solving the
/// quintic that exact minimisation needs.
fn nearest_on_cubic(quad: [(f32, f32); 4], p: (f32, f32)) -> (f32, f32) {
    const SAMPLES: usize = 24;
    let [p0, p1, p2, p3] = quad;
    let point_at = |t: f32| {
        let p01 = lerp(p0, p1, t);
        let p12 = lerp(p1, p2, t);
        let p23 = lerp(p2, p3, t);
        let p012 = lerp(p01, p12, t);
        let p123 = lerp(p12, p23, t);
        lerp(p012, p123, t)
    };
    let mut best = (0.0, f32::MAX);
    for i in 0..=SAMPLES {
        let t = i as f32 / SAMPLES as f32;
        let q = point_at(t);
        let d = (q.0 - p.0).hypot(q.1 - p.1);
        if d < best.1 {
            best = (t, d);
        }
    }
    best
}

/// Even-odd point-in-polygon test over an already-flattened contour.
fn point_in_polygon(points: &[(f32, f32)], p: (f32, f32)) -> bool {
    if points.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = points.len() - 1;
    for i in 0..points.len() {
        let (xi, yi) = points[i];
        let (xj, yj) = points[j];
        if (yi > p.1) != (yj > p.1) {
            let x_cross = xi + (p.1 - yi) / (yj - yi) * (xj - xi);
            if p.0 < x_cross {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Reduce a freehand drag to a handful of corner anchors — the Freeform Pen
/// tool, which lets you draw as if with a pencil and has Photoshop fit a path
/// to the stroke afterward.
///
/// This fits *no curves*: every point Douglas-Peucker keeps becomes a plain
/// corner anchor, so a freehand circle comes out as a many-sided polygon
/// rather than a smooth Bezier loop. That is a real simplification against
/// Photoshop, which fits actual curves to the stroke — call out anywhere this
/// is surfaced. Straight-line corners are still fully editable afterward with
/// Direct Selection and Convert Point, which is how you would turn one smooth
/// in Photoshop too.
pub fn simplify_freehand(points: &[(f32, f32)], tolerance: f32) -> Vec<(f32, f32)> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    douglas_peucker(points, 0, points.len() - 1, tolerance, &mut keep);
    points
        .iter()
        .zip(keep.iter())
        .filter(|(_, &k)| k)
        .map(|(&p, _)| p)
        .collect()
}

fn douglas_peucker(points: &[(f32, f32)], start: usize, end: usize, tolerance: f32, keep: &mut [bool]) {
    if end <= start + 1 {
        return;
    }
    let (mut farthest, mut farthest_dist) = (start, 0.0f32);
    for i in (start + 1)..end {
        let d = distance_to_segment(points[i], points[start], points[end]);
        if d > farthest_dist {
            farthest = i;
            farthest_dist = d;
        }
    }
    if farthest_dist <= tolerance {
        return;
    }
    keep[farthest] = true;
    douglas_peucker(points, start, farthest, tolerance, keep);
    douglas_peucker(points, farthest, end, tolerance, keep);
}

// ---------------------------------------------------------------------------
// The document's collection of paths

/// One named path in the Paths panel.
#[derive(Clone, Debug)]
pub struct PathEntry {
    pub name: String,
    pub path: VectorPath,
}

/// The document's paths, and which one the Pen and selection tools act on.
///
/// Plain indices rather than the `LayerId` indirection layers use: paths are
/// never reordered by dragging the way layers are, so there is no stack whose
/// positions shift under a stored id.
#[derive(Clone, Debug, Default)]
pub struct PathSet {
    entries: Vec<PathEntry>,
    active: Option<usize>,
    /// Next number handed to a path created via "New Path", so they read
    /// "Path 1", "Path 2", ... the way Photoshop's do.
    next_untitled: u32,
}

impl PathSet {
    pub fn new() -> Self {
        Self { next_untitled: 1, ..Self::default() }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[PathEntry] {
        &self.entries
    }

    pub fn active_index(&self) -> Option<usize> {
        self.active
    }

    pub fn set_active(&mut self, index: usize) -> bool {
        if index >= self.entries.len() {
            return false;
        }
        self.active = Some(index);
        true
    }

    pub fn active(&self) -> Option<&VectorPath> {
        self.active.and_then(|i| self.entries.get(i)).map(|e| &e.path)
    }

    pub fn active_mut(&mut self) -> Option<&mut VectorPath> {
        let i = self.active?;
        self.entries.get_mut(i).map(|e| &mut e.path)
    }

    /// A fresh path named "Path N", made active — the panel's "New Path".
    pub fn add_named(&mut self) -> usize {
        let name = format!("Path {}", self.next_untitled);
        self.next_untitled += 1;
        self.entries.push(PathEntry { name, path: VectorPath::new() });
        self.active = Some(self.entries.len() - 1);
        self.active.unwrap()
    }

    /// The active path, creating a "Work Path" if none exists yet — what
    /// drawing with the Pen tool does when the panel is empty, mirroring how
    /// Photoshop starts a Work Path the first time you click with no path
    /// selected.
    pub fn ensure_active(&mut self) -> &mut VectorPath {
        if self.active.is_none() {
            self.entries.push(PathEntry {
                name: "Work Path".to_string(),
                path: VectorPath::new(),
            });
            self.active = Some(self.entries.len() - 1);
        }
        self.active_mut().unwrap()
    }

    pub fn duplicate(&mut self, index: usize) -> Option<usize> {
        let entry = self.entries.get(index)?.clone();
        let name = format!("{} copy", entry.name);
        self.entries.push(PathEntry { name, path: entry.path });
        self.active = Some(self.entries.len() - 1);
        self.active
    }

    pub fn remove(&mut self, index: usize) -> bool {
        if index >= self.entries.len() {
            return false;
        }
        self.entries.remove(index);
        self.active = match self.active {
            Some(a) if a == index => None,
            Some(a) if a > index => Some(a - 1),
            other => other,
        };
        true
    }

    pub fn rename(&mut self, index: usize, name: impl Into<String>) -> bool {
        let Some(entry) = self.entries.get_mut(index) else { return false };
        entry.name = name.into();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appending_corners_builds_a_polyline() {
        let mut p = VectorPath::new();
        p.append_corner(0.0, 0.0);
        p.append_corner(10.0, 0.0);
        p.append_corner(10.0, 10.0);
        assert_eq!(p.subpaths.len(), 1);
        assert_eq!(p.subpaths[0].points.len(), 3);
        assert!(!p.subpaths[0].closed);

        let flat = p.flatten(0.1);
        assert_eq!(flat.len(), 1);
        assert_eq!(flat[0].0, vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]);
        assert!(!flat[0].1);
    }

    #[test]
    fn a_short_drag_stays_a_corner() {
        let mut p = VectorPath::new();
        p.append_corner(0.0, 0.0);
        p.update_last_handle(0.3, 0.2, false);
        assert!(!p.subpaths[0].points[0].smooth);
        assert!(p.subpaths[0].points[0].out_handle.is_none());
    }

    #[test]
    fn dragging_places_symmetric_handles() {
        let mut p = VectorPath::new();
        p.append_corner(0.0, 0.0);
        p.update_last_handle(10.0, 0.0, false);
        let pt = p.subpaths[0].points[0];
        assert!(pt.smooth);
        assert_eq!(pt.out_handle, Some((10.0, 0.0)));
        assert_eq!(pt.in_handle, Some((-10.0, 0.0)), "the in-handle did not mirror");
    }

    #[test]
    fn alt_drag_while_placing_leaves_the_in_handle_alone() {
        let mut p = VectorPath::new();
        p.append_corner(0.0, 0.0);
        p.update_last_handle(10.0, 0.0, true);
        let pt = p.subpaths[0].points[0];
        assert!(!pt.smooth);
        assert_eq!(pt.out_handle, Some((10.0, 0.0)));
        assert_eq!(pt.in_handle, None, "an independent drag invented an in-handle");
    }

    #[test]
    fn closing_needs_at_least_two_points() {
        let mut p = VectorPath::new();
        p.append_corner(0.0, 0.0);
        assert!(!p.close_active_subpath(), "a single point should not close");
        p.append_corner(10.0, 0.0);
        assert!(p.close_active_subpath());
        assert!(p.subpaths[0].closed);
        assert!(!p.is_editing(), "closing should end the editing session");
    }

    #[test]
    fn finishing_leaves_the_subpath_open_and_appending_again_starts_a_new_one() {
        let mut p = VectorPath::new();
        p.append_corner(0.0, 0.0);
        p.append_corner(10.0, 0.0);
        p.finish_editing();
        assert!(!p.subpaths[0].closed);

        p.append_corner(50.0, 50.0);
        assert_eq!(p.subpaths.len(), 2, "a new subpath should have started");
    }

    #[test]
    fn moving_an_anchor_carries_its_handles() {
        let mut p = VectorPath::new();
        p.append_corner(0.0, 0.0);
        p.update_last_handle(10.0, 0.0, false);
        p.move_anchor(0, 0, 5.0, 5.0);
        let pt = p.subpaths[0].points[0];
        assert_eq!(pt.anchor, (5.0, 5.0));
        assert_eq!(pt.out_handle, Some((15.0, 5.0)));
        assert_eq!(pt.in_handle, Some((-5.0, 5.0)));
    }

    #[test]
    fn dragging_one_handle_of_a_smooth_point_mirrors_the_other_angle_not_length() {
        let mut p = VectorPath::new();
        p.append_corner(0.0, 0.0);
        p.update_last_handle(10.0, 0.0, false); // out=(10,0) in=(-10,0), both length 10
        // Shrink the in-handle to length 4 first.
        p.move_handle(0, 0, HandleSide::In, -4.0, 0.0, false);
        // Now swing the out-handle upward; the in-handle should follow the
        // angle (opposite direction) but keep its own length of 4.
        p.move_handle(0, 0, HandleSide::Out, 0.0, 10.0, false);
        let pt = p.subpaths[0].points[0];
        assert_eq!(pt.out_handle, Some((0.0, 10.0)));
        let in_h = pt.in_handle.unwrap();
        assert!((in_h.0).abs() < 1e-3, "in-handle x should be ~0: {in_h:?}");
        assert!(in_h.1 < 0.0, "in-handle should point the opposite way: {in_h:?}");
        let len = (in_h.0 * in_h.0 + in_h.1 * in_h.1).sqrt();
        assert!((len - 4.0).abs() < 1e-3, "in-handle length changed: {len}");
    }

    #[test]
    fn alt_dragging_a_handle_breaks_symmetry_permanently() {
        let mut p = VectorPath::new();
        p.append_corner(0.0, 0.0);
        p.update_last_handle(10.0, 0.0, false);
        p.move_handle(0, 0, HandleSide::Out, 0.0, 10.0, true);
        let pt = p.subpaths[0].points[0];
        assert!(!pt.smooth);
        assert_eq!(pt.in_handle, Some((-10.0, 0.0)), "the in-handle moved when it should not have");

        // A later non-Alt drag must not suddenly start mirroring again.
        p.move_handle(0, 0, HandleSide::Out, 0.0, 20.0, false);
        let pt = p.subpaths[0].points[0];
        assert_eq!(pt.in_handle, Some((-10.0, 0.0)), "smoothness came back after being broken");
    }

    #[test]
    fn convert_point_click_strips_handles() {
        let mut p = VectorPath::new();
        p.append_corner(0.0, 0.0);
        p.update_last_handle(10.0, 0.0, false);
        assert!(p.set_corner(0, 0));
        let pt = p.subpaths[0].points[0];
        assert!(!pt.smooth);
        assert!(pt.in_handle.is_none());
        assert!(pt.out_handle.is_none());
    }

    #[test]
    fn convert_point_drag_pulls_symmetric_handles_from_a_corner() {
        let mut p = VectorPath::new();
        p.append_corner(0.0, 0.0);
        p.finish_editing();
        assert!(p.drag_new_handles(0, 0, 8.0, 0.0));
        let pt = p.subpaths[0].points[0];
        assert!(pt.smooth);
        assert_eq!(pt.out_handle, Some((8.0, 0.0)));
        assert_eq!(pt.in_handle, Some((-8.0, 0.0)));
    }

    #[test]
    fn inserting_an_anchor_on_a_straight_segment_stays_straight() {
        let mut p = VectorPath::new();
        p.append_corner(0.0, 0.0);
        p.append_corner(10.0, 0.0);
        p.finish_editing();
        assert!(p.insert_anchor(0, 0, 0.5));
        assert_eq!(p.subpaths[0].points.len(), 3);
        let mid = p.subpaths[0].points[1];
        assert_eq!(mid.anchor, (5.0, 0.0));
        assert!(mid.in_handle.is_none());
        assert!(mid.out_handle.is_none());
        // The shape must not have moved: still a straight line 0..10.
        let flat = p.flatten(0.01);
        assert_eq!(flat[0].0, vec![(0.0, 0.0), (5.0, 0.0), (10.0, 0.0)]);
    }

    #[test]
    fn inserting_an_anchor_on_a_curve_preserves_its_shape() {
        let mut p = VectorPath::new();
        p.append_corner(0.0, 0.0);
        p.update_last_handle(0.0, 10.0, false);
        p.append_corner(20.0, 0.0);
        p.finish_editing();

        let before = p.flatten(0.05);
        assert!(p.insert_anchor(0, 0, 0.5));
        assert_eq!(p.subpaths[0].points.len(), 3);
        assert!(p.subpaths[0].points[1].smooth, "a split curve segment should stay a curve");
        let after = p.flatten(0.05);

        // Sample a handful of points along both and confirm the curve barely
        // moved — splitting must reproduce the original shape almost exactly.
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let i = ((before[0].0.len() - 1) as f32 * t).round() as usize;
            let j = ((after[0].0.len() - 1) as f32 * t).round() as usize;
            let (bx, by) = before[0].0[i];
            let (ax, ay) = after[0].0[j];
            assert!((bx - ax).hypot(by - ay) < 0.5, "the curve moved at t={t}");
        }
    }

    #[test]
    fn deleting_the_last_anchor_removes_the_subpath() {
        let mut p = VectorPath::new();
        p.append_corner(0.0, 0.0);
        assert!(p.delete_anchor(0, 0));
        assert!(p.subpaths.is_empty());
        assert!(p.is_empty());
    }

    #[test]
    fn deleting_reindexes_editing_correctly() {
        let mut p = VectorPath::new();
        p.append_corner(0.0, 0.0);
        p.append_corner(1.0, 1.0);
        p.finish_editing();
        p.append_corner(9.0, 9.0); // subpath 1, still being edited
        p.append_corner(9.0, 9.0);
        // Delete every point of subpath 0, which should shift subpath 1 to 0
        // and keep `editing` pointing at it.
        p.delete_anchor(0, 1);
        p.delete_anchor(0, 0);
        assert_eq!(p.subpaths.len(), 1);
        assert_eq!(p.editing_subpath(), Some(0));
    }

    #[test]
    fn moving_a_subpath_shifts_every_anchor_and_handle() {
        let mut p = VectorPath::new();
        p.append_corner(0.0, 0.0);
        p.update_last_handle(4.0, 0.0, false);
        p.append_corner(10.0, 0.0);
        p.move_subpath(0, 100.0, 0.0);
        assert_eq!(p.subpaths[0].points[0].anchor, (100.0, 0.0));
        assert_eq!(p.subpaths[0].points[0].out_handle, Some((104.0, 0.0)));
        assert_eq!(p.subpaths[0].points[1].anchor, (110.0, 0.0));
    }

    #[test]
    fn hit_anchor_finds_the_nearest_within_radius() {
        let mut p = VectorPath::new();
        p.append_corner(0.0, 0.0);
        p.append_corner(100.0, 0.0);
        assert_eq!(p.hit_anchor(2.0, 2.0, 5.0), Some((0, 0)));
        assert_eq!(p.hit_anchor(2.0, 2.0, 1.0), None);
        assert_eq!(p.hit_anchor(98.0, 1.0, 5.0), Some((0, 1)));
    }

    #[test]
    fn hit_segment_finds_the_point_and_a_matching_split_lands_near_the_click() {
        // A straight segment is a cubic with both control points doubled onto
        // its endpoints, and that parameterisation eases in and out rather than
        // running linearly with distance — `t` for a click a quarter of the way
        // along is *not* 0.25. What actually has to hold is that the reported
        // point is on the line, and that splitting the segment at the returned
        // `t` inserts an anchor near where the click landed.
        let mut p = VectorPath::new();
        p.append_corner(0.0, 0.0);
        p.append_corner(100.0, 0.0);
        p.finish_editing();
        let (sp, seg, t) = p.hit_segment(25.0, 0.0, 2.0).expect("no hit");
        assert_eq!((sp, seg), (0, 0));

        assert!(p.insert_anchor(sp, seg, t));
        let inserted = p.subpaths[0].points[1].anchor;
        assert!((inserted.0 - 25.0).abs() < 1.0, "split landed at {inserted:?}, not near x=25");
        assert!(inserted.1.abs() < 0.01, "split left the line: {inserted:?}");
    }

    #[test]
    fn a_closed_subpath_is_hit_from_inside() {
        let mut p = VectorPath::new();
        p.append_corner(0.0, 0.0);
        p.append_corner(20.0, 0.0);
        p.append_corner(20.0, 20.0);
        p.append_corner(0.0, 20.0);
        p.close_active_subpath();
        assert_eq!(p.hit_subpath(10.0, 10.0, 2.0), Some(0), "the interior should hit");
        assert_eq!(p.hit_subpath(-50.0, -50.0, 2.0), None);
    }

    #[test]
    fn closed_subpath_flattening_includes_the_closing_segment() {
        let mut p = VectorPath::new();
        p.append_corner(0.0, 0.0);
        p.append_corner(10.0, 0.0);
        p.append_corner(10.0, 10.0);
        p.close_active_subpath();
        let flat = p.flatten(0.1);
        assert!(flat[0].1, "should report closed");
        // Closing wraps the last point back to the first; segment_count for a
        // 3-point closed subpath is 3, so flattening adds the return to (0,0).
        assert_eq!(flat[0].0.last(), Some(&(0.0, 0.0)));
    }

    #[test]
    fn flattening_a_curve_stays_within_tolerance() {
        // What "within tolerance" actually promises is not that consecutive
        // output points are close together — a long straight run of the curve
        // legitimately flattens to one long segment — but that the polyline
        // never strays far from the true curve. So sample the real curve
        // independently and check every sample lands near *some* segment of
        // the flattened result.
        let p0 = (0.0f32, 0.0f32);
        let p1 = (0.0f32, 50.0f32);
        let p2 = (100.0f32, 0.0f32);
        let p3 = (100.0f32, 0.0f32);

        let mut p = VectorPath::new();
        p.append_corner(p0.0, p0.1);
        p.update_last_handle(p1.0, p1.1, false);
        p.append_corner(p3.0, p3.1);
        p.finish_editing();

        let tolerance = 0.25;
        let flat = &p.flatten(tolerance)[0].0;
        assert!(flat.len() > 4, "a curved segment flattened to almost nothing: {flat:?}");

        let eval = |t: f32| {
            let p01 = lerp(p0, p1, t);
            let p12 = lerp(p1, p2, t);
            let p23 = lerp(p2, p3, t);
            let p012 = lerp(p01, p12, t);
            let p123 = lerp(p12, p23, t);
            lerp(p012, p123, t)
        };

        const SAMPLES: usize = 400;
        let allowed = tolerance * 4.0; // generous slack over the raw flatness bound
        for i in 0..=SAMPLES {
            let sample = eval(i as f32 / SAMPLES as f32);
            let nearest = flat
                .windows(2)
                .map(|w| distance_to_segment(sample, w[0], w[1]))
                .fold(f32::MAX, f32::min);
            assert!(nearest <= allowed, "sample {sample:?} sat {nearest} from the polyline");
        }
    }

    #[test]
    fn simplify_freehand_keeps_the_ends_and_drops_nearly_straight_points() {
        // A wobbly-but-basically-straight line: only the ends should survive a
        // generous tolerance.
        let points: Vec<(f32, f32)> = (0..=20)
            .map(|i| {
                let x = i as f32 * 5.0;
                let wobble = if i % 2 == 0 { 0.3 } else { -0.3 };
                (x, wobble)
            })
            .collect();
        let simplified = simplify_freehand(&points, 1.0);
        assert_eq!(simplified.first(), points.first());
        assert_eq!(simplified.last(), points.last());
        assert!(simplified.len() < points.len(), "nothing was simplified away");
        assert!(simplified.len() >= 2);
    }

    #[test]
    fn simplify_freehand_keeps_a_real_corner() {
        // An L-shape: the corner carries real information a straight-line fit
        // would lose, so it has to survive simplification.
        let mut points: Vec<(f32, f32)> = (0..=10).map(|i| (i as f32 * 10.0, 0.0)).collect();
        points.extend((1..=10).map(|i| (100.0, i as f32 * 10.0)));
        let simplified = simplify_freehand(&points, 2.0);
        assert!(
            simplified.iter().any(|&(x, y)| (x - 100.0).abs() < 1.0 && y.abs() < 1.0),
            "the corner at (100, 0) was simplified away: {simplified:?}"
        );
    }

    #[test]
    fn path_set_ensure_active_creates_a_work_path_once() {
        let mut set = PathSet::new();
        assert!(set.is_empty());
        {
            let path = set.ensure_active();
            path.append_corner(0.0, 0.0);
        }
        assert_eq!(set.len(), 1);
        assert_eq!(set.entries()[0].name, "Work Path");
        // A second call must not create a second Work Path.
        set.ensure_active().append_corner(1.0, 1.0);
        assert_eq!(set.len(), 1);
        assert_eq!(set.active().unwrap().subpaths[0].points.len(), 2);
    }

    #[test]
    fn path_set_add_named_numbers_paths_in_order() {
        let mut set = PathSet::new();
        let a = set.add_named();
        let b = set.add_named();
        assert_eq!(set.entries()[a].name, "Path 1");
        assert_eq!(set.entries()[b].name, "Path 2");
        assert_eq!(set.active_index(), Some(b));
    }

    #[test]
    fn removing_a_path_updates_the_active_index() {
        let mut set = PathSet::new();
        set.add_named();
        let b = set.add_named();
        set.set_active(0);
        assert!(set.remove(0));
        // The active index pointed at the removed entry's slot; `b` shifted
        // down to fill it, but nothing should now claim to be active.
        assert_eq!(set.active_index(), None);
        assert_eq!(set.len(), 1);
        let _ = b;
    }
}

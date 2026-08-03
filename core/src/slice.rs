//! Slices — the document's web-export cut lines.
//!
//! A slice is a rectangle of the canvas that exports as its own image file.
//! Photoshop keeps two kinds:
//!
//! * **User slices**, drawn deliberately with the Slice tool. Solid lines and
//!   a blue badge in the interface.
//! * **Auto slices**, which the application generates to cover everything the
//!   user slices do not. Dotted lines and a grey badge. They are not stored —
//!   they are recomputed from the user slices every time, because moving one
//!   user slice changes all of them.
//!
//! Slices carry no pixels of their own; they are rectangles over the composite,
//! and the export step crops it.

use crate::buffer::Rect;

/// Where a slice came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SliceKind {
    /// Drawn by the user with the Slice tool.
    User,
    /// Generated to fill the gaps between user slices.
    Auto,
}

/// One resolved slice, ready to draw or export.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Slice {
    pub rect: Rect,
    pub kind: SliceKind,
    /// Photoshop's badge number, starting at 1 and running in reading order.
    pub number: u32,
    /// Index into the user slice list, or `None` for an auto slice. This is
    /// what the shell needs to move or delete the slice the user grabbed.
    pub user_index: Option<usize>,
}

/// The document's user slices.
///
/// Auto slices are derived, so only the user's own rectangles are stored.
#[derive(Clone, Debug, Default)]
pub struct SliceSet {
    user: Vec<Rect>,
}

impl SliceSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.user.is_empty()
    }

    pub fn len(&self) -> usize {
        self.user.len()
    }

    pub fn user_slices(&self) -> &[Rect] {
        &self.user
    }

    /// Add a user slice. Empty rectangles are ignored — a click that was not
    /// a drag should not leave an invisible slice behind.
    pub fn add(&mut self, rect: Rect) -> bool {
        if rect.is_empty() {
            return false;
        }
        self.user.push(rect);
        true
    }

    /// Move or resize an existing user slice.
    pub fn set(&mut self, index: usize, rect: Rect) -> bool {
        if rect.is_empty() || index >= self.user.len() {
            return false;
        }
        self.user[index] = rect;
        true
    }

    pub fn remove(&mut self, index: usize) -> bool {
        if index >= self.user.len() {
            return false;
        }
        self.user.remove(index);
        true
    }

    pub fn clear(&mut self) {
        self.user.clear();
    }

    /// The full slice list for a canvas: the user slices, plus auto slices
    /// covering everything else, all numbered in reading order.
    ///
    /// With no user slices the whole canvas is one auto slice, which is what
    /// Photoshop shows for an unsliced document.
    pub fn resolve(&self, canvas: Rect) -> Vec<Slice> {
        if canvas.is_empty() {
            return Vec::new();
        }

        // Clipped user slices, keeping their original index so the shell can
        // still address them after one has been clipped away entirely.
        let mut slices: Vec<Slice> = Vec::new();
        let mut kept: Vec<Rect> = Vec::new();
        for (index, rect) in self.user.iter().enumerate() {
            let clipped = rect.intersect(&canvas);
            if clipped.is_empty() {
                continue;
            }
            kept.push(clipped);
            slices.push(Slice {
                rect: clipped,
                kind: SliceKind::User,
                number: 0,
                user_index: Some(index),
            });
        }

        for rect in auto_slices(&kept, canvas) {
            slices.push(Slice {
                rect,
                kind: SliceKind::Auto,
                number: 0,
                user_index: None,
            });
        }

        // Photoshop numbers every slice, user and auto alike, left to right
        // then top to bottom.
        slices.sort_by_key(|s| (s.rect.y, s.rect.x));
        for (i, slice) in slices.iter_mut().enumerate() {
            slice.number = i as u32 + 1;
        }
        slices
    }
}

/// The rectangles covering everything in `canvas` that `user` does not.
///
/// Every user slice edge is extended across the whole canvas, which cuts it
/// into a grid; the cells no user slice covers are the auto slices. Runs of
/// them along a row are then merged, so a wide empty band is one slice rather
/// than a row of thin ones — the same shape Photoshop produces.
fn auto_slices(user: &[Rect], canvas: Rect) -> Vec<Rect> {
    let mut xs = vec![canvas.x, canvas.right()];
    let mut ys = vec![canvas.y, canvas.bottom()];
    for rect in user {
        xs.push(rect.x);
        xs.push(rect.right());
        ys.push(rect.y);
        ys.push(rect.bottom());
    }
    xs.sort_unstable();
    xs.dedup();
    ys.sort_unstable();
    ys.dedup();
    xs.retain(|&x| x >= canvas.x && x <= canvas.right());
    ys.retain(|&y| y >= canvas.y && y <= canvas.bottom());

    let covered = |x: i32, y: i32| user.iter().any(|r| r.contains(x, y));

    let mut out = Vec::new();
    for row in ys.windows(2) {
        let (top, bottom) = (row[0], row[1]);
        if bottom <= top {
            continue;
        }

        // Walk the row, accumulating a run of uncovered cells and flushing it
        // whenever a covered cell interrupts.
        let mut run_start: Option<i32> = None;
        for col in xs.windows(2) {
            let (left, right) = (col[0], col[1]);
            if right <= left {
                continue;
            }

            if covered(left, top) {
                if let Some(start) = run_start.take() {
                    out.push(Rect::new(start, top, (left - start) as u32, (bottom - top) as u32));
                }
            } else if run_start.is_none() {
                run_start = Some(left);
            }
        }
        if let Some(start) = run_start {
            let end = *xs.last().unwrap_or(&start);
            if end > start {
                out.push(Rect::new(start, top, (end - start) as u32, (bottom - top) as u32));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas() -> Rect {
        Rect::new(0, 0, 100, 100)
    }

    fn area(slices: &[Slice]) -> u32 {
        slices.iter().map(|s| s.rect.width * s.rect.height).sum()
    }

    #[test]
    fn an_unsliced_document_is_one_auto_slice() {
        let set = SliceSet::new();
        let slices = set.resolve(canvas());

        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].kind, SliceKind::Auto);
        assert_eq!(slices[0].rect, canvas());
        assert_eq!(slices[0].number, 1);
    }

    #[test]
    fn slices_tile_the_canvas_exactly() {
        // Whatever the arrangement, the slices must cover the canvas with no
        // gap and no overlap — that is the whole point of auto slices.
        let mut set = SliceSet::new();
        set.add(Rect::new(20, 20, 30, 30));
        set.add(Rect::new(60, 10, 30, 20));

        let slices = set.resolve(canvas());
        assert_eq!(area(&slices), 100 * 100, "slices do not tile the canvas");

        // No two slices overlap: check every pair.
        for (i, a) in slices.iter().enumerate() {
            for b in slices.iter().skip(i + 1) {
                assert!(
                    a.rect.intersect(&b.rect).is_empty(),
                    "{:?} overlaps {:?}",
                    a.rect,
                    b.rect
                );
            }
        }
    }

    #[test]
    fn a_user_slice_survives_as_itself() {
        let mut set = SliceSet::new();
        set.add(Rect::new(20, 20, 30, 30));

        let slices = set.resolve(canvas());
        let user: Vec<_> = slices.iter().filter(|s| s.kind == SliceKind::User).collect();
        assert_eq!(user.len(), 1);
        assert_eq!(user[0].rect, Rect::new(20, 20, 30, 30));
        assert_eq!(user[0].user_index, Some(0));
    }

    #[test]
    fn numbering_runs_in_reading_order() {
        let mut set = SliceSet::new();
        set.add(Rect::new(0, 50, 50, 50));

        let slices = set.resolve(canvas());
        // Numbers are 1..n with no gaps, and increase down then across.
        let mut numbers: Vec<u32> = slices.iter().map(|s| s.number).collect();
        numbers.sort_unstable();
        assert_eq!(numbers, (1..=slices.len() as u32).collect::<Vec<_>>());

        for pair in slices.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if a.number < b.number {
                assert!(
                    (a.rect.y, a.rect.x) <= (b.rect.y, b.rect.x),
                    "{:?} numbered before {:?}",
                    a.rect,
                    b.rect
                );
            }
        }
    }

    #[test]
    fn an_empty_run_is_merged_into_one_wide_slice() {
        // One user slice in the middle of the top row leaves a full-width band
        // beneath it, which should be a single slice rather than three.
        let mut set = SliceSet::new();
        set.add(Rect::new(40, 0, 20, 20));

        let slices = set.resolve(canvas());
        let band = slices
            .iter()
            .find(|s| s.rect.y == 20 && s.rect.height == 80)
            .expect("no band beneath the slice");
        assert_eq!(band.rect.width, 100, "the band was left split into columns");
    }

    #[test]
    fn a_full_canvas_user_slice_leaves_no_auto_slices() {
        let mut set = SliceSet::new();
        set.add(canvas());

        let slices = set.resolve(canvas());
        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].kind, SliceKind::User);
    }

    #[test]
    fn slices_are_clipped_to_the_canvas() {
        let mut set = SliceSet::new();
        set.add(Rect::new(80, 80, 60, 60));

        let slices = set.resolve(canvas());
        assert_eq!(area(&slices), 100 * 100, "the overhang escaped the canvas");
        let user = slices.iter().find(|s| s.kind == SliceKind::User).unwrap();
        assert_eq!(user.rect, Rect::new(80, 80, 20, 20));
    }

    #[test]
    fn a_slice_entirely_off_canvas_disappears() {
        let mut set = SliceSet::new();
        set.add(Rect::new(200, 200, 20, 20));

        let slices = set.resolve(canvas());
        assert!(slices.iter().all(|s| s.kind == SliceKind::Auto));
        assert_eq!(area(&slices), 100 * 100);
        // The set still holds it, so the shell's indices stay valid.
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn empty_slices_are_refused() {
        let mut set = SliceSet::new();
        assert!(!set.add(Rect::new(10, 10, 0, 0)));
        assert!(!set.add(Rect::new(10, 10, 5, 0)));
        assert!(set.is_empty());
    }

    #[test]
    fn set_and_remove_address_the_right_slice() {
        let mut set = SliceSet::new();
        set.add(Rect::new(0, 0, 10, 10));
        set.add(Rect::new(20, 20, 10, 10));

        assert!(set.set(1, Rect::new(50, 50, 10, 10)));
        assert_eq!(set.user_slices()[1], Rect::new(50, 50, 10, 10));
        assert!(!set.set(9, Rect::new(0, 0, 5, 5)), "out-of-range index accepted");

        assert!(set.remove(0));
        assert_eq!(set.len(), 1);
        assert_eq!(set.user_slices()[0], Rect::new(50, 50, 10, 10));
        assert!(!set.remove(9));
    }

    #[test]
    fn overlapping_user_slices_still_tile() {
        // Photoshop lets user slices overlap; the auto slices must fill what
        // is left without double-counting the overlap.
        let mut set = SliceSet::new();
        set.add(Rect::new(10, 10, 40, 40));
        set.add(Rect::new(30, 30, 40, 40));

        let slices = set.resolve(canvas());
        let autos: Vec<_> = slices
            .iter()
            .filter(|s| s.kind == SliceKind::Auto)
            .collect();
        for auto in &autos {
            for user in set.user_slices() {
                assert!(
                    auto.rect.intersect(user).is_empty(),
                    "auto slice {:?} runs under user slice {:?}",
                    auto.rect,
                    user
                );
            }
        }
    }
}

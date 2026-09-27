//! What part of the canvas has changed since the shell last redrew it.
//!
//! The canvas keeps the composite it last drew and, when told the document
//! changed, asks for just the rectangle that did rather than recompositing the
//! whole picture. On a 16000-pixel-square map that is the difference between a
//! brush stroke costing the pixels it touched and costing 1.4 seconds and a
//! gigabyte.
//!
//! # The rule that keeps this correct
//!
//! **Any change nobody described is a change to the whole canvas.** Every
//! mutable use of the document goes through the bridge's `doc_mut()`, which
//! calls [`CanvasDamage::edited`]; the canvas then recomposites everything, as
//! it always used to. Only code that has gone out of its way to say what it
//! touched — through `doc_mut_within()` and [`CanvasDamage::mark`] — gets a
//! partial redraw. So forgetting to describe a change costs speed, never a
//! stale picture.
//!
//! A mark only ever adds to the damage. It cannot clear an undescribed edit
//! made earlier by some other operation: only [`CanvasDamage::take`], the
//! canvas catching up, does that. Otherwise a small, well-described edit
//! following an undescribed one would hide it.

use crate::buffer::Rect;

/// The canvas's outstanding damage. See the module notes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CanvasDamage {
    /// The union of every described change since the last `take`.
    region: Rect,
    /// Whether anything changed without being described.
    everywhere: bool,
}

impl CanvasDamage {
    /// The document was changed in a way nobody described.
    pub fn edited(&mut self) {
        self.everywhere = true;
    }

    /// The document was changed within `rect`, in document pixels, and
    /// nowhere else. An empty rectangle is a change that shows nowhere — a
    /// stroke beginning, which lays nothing on the layer until it ends.
    pub fn mark(&mut self, rect: Rect) {
        self.region = self.region.union(&rect);
    }

    /// What the canvas must redraw to catch up, clipped to `canvas`, and
    /// start afresh. The whole canvas if anything went undescribed; an empty
    /// rectangle if nothing changed at all.
    pub fn take(&mut self, canvas: Rect) -> Rect {
        let damage = if self.everywhere { canvas } else { self.region.intersect(&canvas) };
        *self = CanvasDamage::default();
        damage
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CANVAS: Rect = Rect { x: 0, y: 0, width: 100, height: 80 };

    #[test]
    fn nothing_changed_means_nothing_to_redraw() {
        assert!(CanvasDamage::default().take(CANVAS).is_empty());
    }

    #[test]
    fn described_changes_add_up() {
        let mut damage = CanvasDamage::default();
        damage.mark(Rect::new(10, 10, 5, 5));
        damage.mark(Rect::new(30, 20, 5, 5));
        assert_eq!(damage.take(CANVAS), Rect::new(10, 10, 25, 15));
    }

    #[test]
    fn an_undescribed_change_is_the_whole_canvas() {
        let mut damage = CanvasDamage::default();
        damage.mark(Rect::new(10, 10, 5, 5));
        damage.edited();
        assert_eq!(damage.take(CANVAS), CANVAS);
    }

    #[test]
    fn a_mark_cannot_hide_an_earlier_undescribed_change() {
        // The trap this module exists to avoid: an undescribed edit, then a
        // small described one before the canvas catches up. Redrawing only
        // the small one would leave the first on screen stale.
        let mut damage = CanvasDamage::default();
        damage.edited();
        damage.mark(Rect::new(10, 10, 5, 5));
        assert_eq!(damage.take(CANVAS), CANVAS);
    }

    #[test]
    fn taking_starts_afresh() {
        let mut damage = CanvasDamage::default();
        damage.edited();
        damage.take(CANVAS);
        assert!(damage.take(CANVAS).is_empty());
    }

    #[test]
    fn damage_is_clipped_to_the_canvas() {
        // A layer effect or a layer hanging off the edge can describe pixels
        // that are not on the canvas at all.
        let mut damage = CanvasDamage::default();
        damage.mark(Rect::new(90, 70, 40, 40));
        assert_eq!(damage.take(CANVAS), Rect::new(90, 70, 10, 10));
    }
}

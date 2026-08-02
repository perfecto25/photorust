//! The document — one open image.
//!
//! Owns the layer stack, the selection, the undo history and the in-progress
//! brush stroke, and is the single object the bridge drives. Every mutating
//! method that a user would recognise as "an action" records a history state.

use crate::blend::BlendMode;
use crate::brush::{Brush, StrokeMask};
use crate::buffer::{Pixmap, Rect, Rgba8};
use crate::compositor;
use crate::filters::{Adjustment, Filter};
use crate::history::History;
use crate::layer::{Layer, LayerId, LayerKind, LayerStack};
use crate::selection::{Selection, SelectionOp};

/// One open image.
pub struct Document {
    width: u32,
    height: u32,
    stack: LayerStack,
    selection: Selection,
    history: History,

    /// Layer the tools act on.
    active_layer: LayerId,

    /// Scratch buffer for the stroke currently being drawn, if any.
    stroke: Option<StrokeMask>,
    /// Snapshot taken when the stroke began, so the whole stroke is one undo
    /// step rather than one per mouse-move.
    stroke_undo_base: Option<LayerStack>,

    /// File path, once saved.
    pub path: Option<String>,
    /// Set on every mutation, cleared on save.
    dirty: bool,
}

impl Document {
    /// A new document with a single Background layer filled with `background`.
    pub fn new(width: u32, height: u32, background: Rgba8) -> Self {
        let mut stack = LayerStack::new();
        let id = stack.allocate_id();
        stack.push(Layer::new_filled(id, "Background", width, height, background));

        let history = History::new(stack.clone());
        Self {
            width,
            height,
            stack,
            selection: Selection::new(width, height),
            history,
            active_layer: id,
            stroke: None,
            stroke_undo_base: None,
            path: None,
            dirty: false,
        }
    }

    /// A document with a single transparent layer.
    pub fn new_transparent(width: u32, height: u32) -> Self {
        let mut doc = Self::new(width, height, Rgba8::TRANSPARENT);
        if let Some(l) = doc.stack.get_mut(0) {
            l.name = "Layer 1".to_string();
        }
        doc.history = History::new(doc.stack.clone());
        doc
    }

    /// Wrap an existing image as the Background of a new document.
    pub fn from_pixmap(pixels: Pixmap) -> Self {
        let (width, height) = (pixels.width(), pixels.height());
        let mut stack = LayerStack::new();
        let id = stack.allocate_id();
        let mut layer = Layer::new_raster(id, "Background", width, height);
        layer.pixels = pixels;
        stack.push(layer);

        let history = History::new(stack.clone());
        Self {
            width,
            height,
            stack,
            selection: Selection::new(width, height),
            history,
            active_layer: id,
            stroke: None,
            stroke_undo_base: None,
            path: None,
            dirty: false,
        }
    }

    // -- basic properties ---------------------------------------------------

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    /// Title-bar text: file name (or "Untitled") plus a modified marker.
    pub fn display_name(&self) -> String {
        let base = self
            .path
            .as_deref()
            .and_then(|p| p.rsplit('/').next())
            .unwrap_or("Untitled-1");
        if self.dirty {
            format!("{}*", base)
        } else {
            base.to_string()
        }
    }

    // -- layers -------------------------------------------------------------

    pub fn layers(&self) -> &LayerStack {
        &self.stack
    }

    pub fn layer_count(&self) -> usize {
        self.stack.len()
    }

    pub fn active_layer_id(&self) -> LayerId {
        self.active_layer
    }

    pub fn active_layer(&self) -> Option<&Layer> {
        self.stack.by_id(self.active_layer)
    }

    pub fn active_layer_mut(&mut self) -> Option<&mut Layer> {
        let id = self.active_layer;
        self.stack.by_id_mut(id)
    }

    /// Index of the active layer in the stack (0 = bottom).
    pub fn active_index(&self) -> Option<usize> {
        self.stack.index_of(self.active_layer)
    }

    /// Select a layer. Ignored if `id` is not in this document.
    pub fn set_active_layer(&mut self, id: LayerId) -> bool {
        if self.stack.by_id(id).is_some() {
            self.active_layer = id;
            true
        } else {
            false
        }
    }

    /// Add a transparent layer above the active one and select it.
    pub fn add_layer(&mut self, name: Option<String>) -> LayerId {
        let id = self.stack.allocate_id();
        let name = name.unwrap_or_else(|| self.stack.suggest_name());
        let layer = Layer::new_raster(id, name, self.width, self.height);

        let at = self.active_index().map_or(self.stack.len(), |i| i + 1);
        self.stack.insert(at, layer);
        self.active_layer = id;
        self.commit("New Layer");
        id
    }

    /// Add an adjustment layer above the active one.
    pub fn add_adjustment_layer(&mut self, adjustment: Adjustment) -> LayerId {
        let id = self.stack.allocate_id();
        let layer = Layer::new_adjustment(id, adjustment.name(), adjustment);

        let at = self.active_index().map_or(self.stack.len(), |i| i + 1);
        self.stack.insert(at, layer);
        self.active_layer = id;
        self.commit(adjustment.name());
        id
    }

    /// Duplicate a layer, inserting the copy directly above the original.
    pub fn duplicate_layer(&mut self, id: LayerId) -> Option<LayerId> {
        let index = self.stack.index_of(id)?;
        let mut copy = self.stack.get(index)?.clone();
        copy.id = self.stack.allocate_id();
        copy.name = format!("{} copy", copy.name);
        let new_id = copy.id;

        self.stack.insert(index + 1, copy);
        self.active_layer = new_id;
        self.commit("Duplicate Layer");
        Some(new_id)
    }

    /// Delete a layer. Refuses to remove the last remaining one.
    pub fn delete_layer(&mut self, id: LayerId) -> bool {
        if self.stack.len() <= 1 {
            return false;
        }
        let Some(index) = self.stack.index_of(id) else {
            return false;
        };
        self.stack.remove(index);

        if self.active_layer == id {
            // Select the layer that took its place, or the new top.
            let next = index.min(self.stack.len().saturating_sub(1));
            self.active_layer = self.stack.get(next).map_or(LayerId::NONE, |l| l.id);
        }
        self.commit("Delete Layer");
        true
    }

    /// Move a layer to a new stack position.
    pub fn reorder_layer(&mut self, id: LayerId, to: usize) -> bool {
        let Some(from) = self.stack.index_of(id) else {
            return false;
        };
        if to >= self.stack.len() || from == to {
            return false;
        }
        self.stack.reorder(from, to);
        self.commit("Reorder Layer");
        true
    }

    /// Merge a layer down into the one below it.
    pub fn merge_down(&mut self, id: LayerId) -> bool {
        let Some(index) = self.stack.index_of(id) else {
            return false;
        };
        if index == 0 {
            return false;
        }

        // Composite just these two layers, bottom-up, into the lower one.
        let mut pair = LayerStack::new();
        if let Some(lower) = self.stack.get(index - 1) {
            pair.push(lower.clone());
        }
        if let Some(upper) = self.stack.get(index) {
            pair.push(upper.clone());
        }
        let merged = compositor::composite(&pair, self.width, self.height);

        self.stack.remove(index);
        if let Some(lower) = self.stack.get_mut(index - 1) {
            lower.pixels = merged;
            lower.offset = (0, 0);
            lower.blend_mode = BlendMode::Normal;
            lower.opacity = 1.0;
            lower.fill_opacity = 1.0;
            lower.mask = None;
            self.active_layer = lower.id;
        }
        self.commit("Merge Layers");
        true
    }

    /// Flatten every layer into a single opaque Background.
    pub fn flatten(&mut self, background: Rgba8) {
        let flat = compositor::flatten(&self.stack, self.width, self.height, background);
        let mut stack = LayerStack::new();
        let id = stack.allocate_id();
        let mut layer = Layer::new_raster(id, "Background", self.width, self.height);
        layer.pixels = flat;
        stack.push(layer);

        self.stack = stack;
        self.active_layer = id;
        self.commit("Flatten Image");
    }

    // -- layer properties ---------------------------------------------------

    pub fn set_layer_visible(&mut self, id: LayerId, visible: bool) {
        if let Some(l) = self.stack.by_id_mut(id) {
            if l.visible != visible {
                l.visible = visible;
                self.commit("Layer Visibility");
            }
        }
    }

    pub fn set_layer_opacity(&mut self, id: LayerId, opacity: f32) {
        if let Some(l) = self.stack.by_id_mut(id) {
            l.opacity = opacity.clamp(0.0, 1.0);
            self.commit("Layer Opacity");
        }
    }

    pub fn set_layer_fill_opacity(&mut self, id: LayerId, opacity: f32) {
        if let Some(l) = self.stack.by_id_mut(id) {
            l.fill_opacity = opacity.clamp(0.0, 1.0);
            self.commit("Fill Opacity");
        }
    }

    pub fn set_layer_blend_mode(&mut self, id: LayerId, mode: BlendMode) {
        if let Some(l) = self.stack.by_id_mut(id) {
            l.blend_mode = mode;
            self.commit("Blending Mode");
        }
    }

    pub fn set_layer_name(&mut self, id: LayerId, name: impl Into<String>) {
        if let Some(l) = self.stack.by_id_mut(id) {
            l.name = name.into();
            self.commit("Rename Layer");
        }
    }

    pub fn set_layer_clipping(&mut self, id: LayerId, clipping: bool) {
        if let Some(l) = self.stack.by_id_mut(id) {
            l.clipping = clipping;
            self.commit("Create Clipping Mask");
        }
    }

    /// Move a layer's pixels by a delta, as the Move tool does.
    pub fn offset_layer(&mut self, id: LayerId, dx: i32, dy: i32) {
        if let Some(l) = self.stack.by_id_mut(id) {
            if l.lock_position {
                return;
            }
            l.offset.0 += dx;
            l.offset.1 += dy;
            self.commit("Move Layer");
        }
    }

    /// Add a mask to a layer, either revealing or hiding everything.
    pub fn add_layer_mask(&mut self, id: LayerId, reveal_all: bool) {
        if let Some(l) = self.stack.by_id_mut(id) {
            if reveal_all {
                l.add_reveal_all_mask();
            } else {
                l.add_hide_all_mask();
            }
            self.commit("Add Layer Mask");
        }
    }

    // -- selection ----------------------------------------------------------

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    pub fn selection_mut(&mut self) -> &mut Selection {
        &mut self.selection
    }

    /// `feather` softens the incoming region before it combines, which is the
    /// options bar's Feather field. Pass 0 for a hard edge.
    pub fn select_rect(&mut self, rect: Rect, op: SelectionOp, feather: u32) {
        self.selection.apply_rect_feathered(rect, op, feather);
    }

    pub fn select_ellipse(&mut self, rect: Rect, op: SelectionOp, feather: u32) {
        self.selection.apply_ellipse_feathered(rect, op, feather);
    }

    pub fn select_all(&mut self) {
        self.selection.select_all();
    }

    pub fn deselect(&mut self) {
        self.selection.clear();
    }

    pub fn invert_selection(&mut self) {
        self.selection.invert();
    }

    /// Whether a marquee is currently active.
    pub fn has_selection(&self) -> bool {
        !self.selection.is_empty()
    }

    // -- painting -----------------------------------------------------------

    /// Begin a brush stroke at a document-space point.
    ///
    /// Returns false when the active layer cannot be painted on.
    pub fn begin_stroke(&mut self, brush: &Brush, x: f32, y: f32, pressure: f32) -> bool {
        let Some(layer) = self.active_layer() else {
            return false;
        };
        if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
            return false;
        }

        // Snapshot before the first dab so undo restores the pre-stroke state.
        self.stroke_undo_base = Some(self.stack.clone());

        let mut mask = StrokeMask::new(self.width, self.height);
        mask.begin(brush, x, y, pressure);
        self.stroke = Some(mask);
        true
    }

    /// Extend the active stroke. No-op if no stroke is in progress.
    pub fn extend_stroke(&mut self, brush: &Brush, x: f32, y: f32, pressure: f32) {
        if let Some(mask) = self.stroke.as_mut() {
            mask.extend(brush, x, y, pressure);
        }
    }

    /// The region the in-progress stroke has touched, for incremental repaint.
    pub fn stroke_dirty(&self) -> Rect {
        self.stroke.as_ref().map_or(Rect::default(), |m| m.dirty())
    }

    /// Composite the in-progress stroke onto the active layer *without*
    /// finishing it. Used to show the stroke live as the user drags.
    ///
    /// Returns a preview of the full document with the stroke applied.
    pub fn preview_stroke(&self, color: Rgba8, opacity: f32) -> Option<Pixmap> {
        let mask = self.stroke.as_ref()?;
        let layer = self.active_layer()?;

        let mut preview_stack = self.stack.clone();
        let target = preview_stack.by_id_mut(layer.id)?;
        let selection = if self.selection.is_empty() {
            None
        } else {
            Some(&self.selection)
        };
        mask.composite_onto(
            &mut target.pixels,
            color,
            opacity,
            target.offset,
            selection,
            target.lock_transparency,
        );

        Some(compositor::composite(&preview_stack, self.width, self.height))
    }

    /// Finish the stroke, baking it into the active layer and recording one
    /// history state for the whole thing.
    pub fn end_stroke(&mut self, color: Rgba8, opacity: f32) -> Rect {
        let Some(mask) = self.stroke.take() else {
            return Rect::default();
        };
        self.stroke_undo_base = None;

        if mask.is_empty() {
            return Rect::default();
        }

        let selection_empty = self.selection.is_empty();
        let id = self.active_layer;
        // Cloned so the immutable selection borrow does not overlap the
        // mutable layer borrow.
        let selection = if selection_empty {
            None
        } else {
            Some(self.selection.clone())
        };

        let dirty = if let Some(layer) = self.stack.by_id_mut(id) {
            let offset = layer.offset;
            let lock = layer.lock_transparency;
            mask.composite_onto(
                &mut layer.pixels,
                color,
                opacity,
                offset,
                selection.as_ref(),
                lock,
            )
        } else {
            Rect::default()
        };

        self.commit("Brush Tool");
        dirty
    }

    /// Abandon the in-progress stroke without applying it.
    pub fn cancel_stroke(&mut self) {
        self.stroke = None;
        if let Some(base) = self.stroke_undo_base.take() {
            self.stack = base;
        }
    }

    /// Flood the selection (or the whole layer) with a colour.
    pub fn fill(&mut self, color: Rgba8) {
        let selection_empty = self.selection.is_empty();
        let selection = if selection_empty {
            None
        } else {
            Some(self.selection.clone())
        };
        let id = self.active_layer;

        if let Some(layer) = self.stack.by_id_mut(id) {
            if layer.lock_pixels {
                return;
            }
            let offset = layer.offset;
            let lock_alpha = layer.lock_transparency;
            let (w, h) = (layer.pixels.width(), layer.pixels.height());

            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let mut alpha = 1.0f32;
                    if let Some(sel) = &selection {
                        alpha = sel.coverage_at(x + offset.0, y + offset.1);
                        if alpha <= 0.0 {
                            continue;
                        }
                    }
                    let dst = layer.pixels.get(x, y);
                    if lock_alpha {
                        if dst.a == 0 {
                            continue;
                        }
                        alpha *= dst.a as f32 / 255.0;
                    }
                    layer
                        .pixels
                        .set(x, y, crate::brush::source_over(dst, color, alpha));
                }
            }
        }
        self.commit("Fill");
    }

    /// Erase within the selection (or the whole layer).
    pub fn clear_selection_pixels(&mut self) {
        let selection_empty = self.selection.is_empty();
        let selection = if selection_empty {
            None
        } else {
            Some(self.selection.clone())
        };
        let id = self.active_layer;

        if let Some(layer) = self.stack.by_id_mut(id) {
            if layer.lock_pixels {
                return;
            }
            let offset = layer.offset;
            let (w, h) = (layer.pixels.width(), layer.pixels.height());
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let cov = match &selection {
                        Some(sel) => sel.coverage_at(x + offset.0, y + offset.1),
                        None => 1.0,
                    };
                    if cov <= 0.0 {
                        continue;
                    }
                    let mut px = layer.pixels.get(x, y);
                    px.a = ((px.a as f32) * (1.0 - cov)).round().clamp(0.0, 255.0) as u8;
                    layer.pixels.set(x, y, px);
                }
            }
        }
        self.commit("Clear");
    }

    // -- filters ------------------------------------------------------------

    /// Apply a destructive filter to the active layer.
    pub fn apply_filter(&mut self, filter: Filter) {
        let id = self.active_layer;
        if let Some(layer) = self.stack.by_id_mut(id) {
            if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
                return;
            }
            filter.apply(&mut layer.pixels);
        }
        self.commit(filter.name());
    }

    /// Apply an adjustment destructively to the active layer.
    pub fn apply_adjustment(&mut self, adjustment: Adjustment) {
        let id = self.active_layer;
        if let Some(layer) = self.stack.by_id_mut(id) {
            if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
                return;
            }
            adjustment.apply_to(&mut layer.pixels);
        }
        self.commit(adjustment.name());
    }

    // -- canvas -------------------------------------------------------------

    /// Resize the canvas without scaling layer content.
    pub fn resize_canvas(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.selection.resize(width, height);
        if let Some(s) = self.stroke.as_mut() {
            s.resize(width, height);
        }
        self.commit("Canvas Size");
    }

    // -- compositing --------------------------------------------------------

    /// Composite the whole document.
    pub fn composite(&self) -> Pixmap {
        compositor::composite(&self.stack, self.width, self.height)
    }

    /// Composite only `region`.
    pub fn composite_region(&self, region: Rect) -> Pixmap {
        compositor::composite_region(&self.stack, self.width, self.height, region).pixels
    }

    /// Flatten to an opaque image over `background`.
    pub fn flattened(&self, background: Rgba8) -> Pixmap {
        compositor::flatten(&self.stack, self.width, self.height, background)
    }

    // -- history ------------------------------------------------------------

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn undo(&mut self) -> bool {
        // An in-progress stroke is discarded rather than half-applied.
        self.stroke = None;
        self.stroke_undo_base = None;

        if let Some(stack) = self.history.undo() {
            self.stack = stack.clone();
            self.reconcile_active_layer();
            self.dirty = true;
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        self.stroke = None;
        self.stroke_undo_base = None;

        if let Some(stack) = self.history.redo() {
            self.stack = stack.clone();
            self.reconcile_active_layer();
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Jump to a history state by index, as the History panel does.
    pub fn jump_to_history(&mut self, index: usize) -> bool {
        self.stroke = None;
        self.stroke_undo_base = None;

        if let Some(stack) = self.history.jump_to(index) {
            self.stack = stack.clone();
            self.reconcile_active_layer();
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Record the current stack as a new history state.
    pub fn commit(&mut self, name: impl Into<String>) {
        self.history.push(name, self.stack.clone());
        self.dirty = true;
    }

    /// After restoring a snapshot the active layer may no longer exist.
    fn reconcile_active_layer(&mut self) {
        if self.stack.by_id(self.active_layer).is_none() {
            self.active_layer = self
                .stack
                .as_slice()
                .last()
                .map_or(LayerId::NONE, |l| l.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Document {
        Document::new(16, 16, Rgba8::WHITE)
    }

    #[test]
    fn new_document_has_one_background_layer() {
        let d = doc();
        assert_eq!(d.layer_count(), 1);
        assert_eq!(d.layers().get(0).unwrap().name, "Background");
        assert_eq!(d.size(), (16, 16));
        assert!(!d.is_dirty());
    }

    #[test]
    fn new_document_composites_to_the_background_color() {
        let d = doc();
        assert_eq!(d.composite().get(8, 8), Rgba8::WHITE);
    }

    #[test]
    fn transparent_document_starts_empty() {
        let d = Document::new_transparent(8, 8);
        assert_eq!(d.composite().get(4, 4).a, 0);
        assert_eq!(d.layers().get(0).unwrap().name, "Layer 1");
        assert!(!d.can_undo(), "constructing a document is not an undo step");
    }

    #[test]
    fn add_layer_inserts_above_active_and_selects_it() {
        let mut d = doc();
        let bg = d.active_layer_id();
        let new_id = d.add_layer(None);

        assert_eq!(d.layer_count(), 2);
        assert_eq!(d.active_layer_id(), new_id);
        assert_eq!(d.layers().index_of(bg), Some(0));
        assert_eq!(d.layers().index_of(new_id), Some(1));
    }

    #[test]
    fn delete_layer_refuses_to_empty_the_document() {
        let mut d = doc();
        assert!(!d.delete_layer(d.active_layer_id()));
        assert_eq!(d.layer_count(), 1);
    }

    #[test]
    fn delete_layer_reselects_something_valid() {
        let mut d = doc();
        let second = d.add_layer(None);
        assert!(d.delete_layer(second));
        assert_eq!(d.layer_count(), 1);
        assert!(
            d.active_layer().is_some(),
            "active layer dangled after delete"
        );
    }

    #[test]
    fn duplicate_layer_copies_pixels_and_names_it() {
        let mut d = doc();
        let original = d.active_layer_id();
        let copy = d.duplicate_layer(original).unwrap();

        assert_eq!(d.layer_count(), 2);
        assert_eq!(d.layers().by_id(copy).unwrap().name, "Background copy");
        assert_eq!(d.layers().index_of(copy), Some(1));
        assert_eq!(d.layers().by_id(copy).unwrap().pixels.get(4, 4), Rgba8::WHITE);
    }

    #[test]
    fn set_active_layer_rejects_unknown_ids() {
        let mut d = doc();
        assert!(!d.set_active_layer(LayerId(9999)));
    }

    #[test]
    fn merge_down_combines_two_layers() {
        let mut d = Document::new_transparent(8, 8);
        // Bottom: red. Top: blue at half opacity.
        d.active_layer_mut().unwrap().pixels.fill(Rgba8::new(255, 0, 0, 255));
        let top = d.add_layer(None);
        d.active_layer_mut().unwrap().pixels.fill(Rgba8::new(0, 0, 255, 255));
        d.set_layer_opacity(top, 0.5);

        assert!(d.merge_down(top));
        assert_eq!(d.layer_count(), 1);

        let p = d.layers().get(0).unwrap().pixels.get(4, 4);
        assert!((p.r as i32 - 128).abs() <= 3, "merge lost blending: {:?}", p);
        assert!((p.b as i32 - 128).abs() <= 3, "merge lost blending: {:?}", p);
    }

    #[test]
    fn merge_down_on_the_bottom_layer_fails() {
        let mut d = doc();
        assert!(!d.merge_down(d.active_layer_id()));
    }

    #[test]
    fn flatten_reduces_to_one_opaque_layer() {
        let mut d = Document::new_transparent(8, 8);
        d.add_layer(None);
        d.flatten(Rgba8::WHITE);

        assert_eq!(d.layer_count(), 1);
        assert_eq!(d.composite().get(4, 4), Rgba8::WHITE);
    }

    #[test]
    fn undo_restores_the_previous_state() {
        let mut d = doc();
        assert!(!d.can_undo());

        d.add_layer(None);
        assert_eq!(d.layer_count(), 2);
        assert!(d.can_undo());

        assert!(d.undo());
        assert_eq!(d.layer_count(), 1);
        assert!(d.can_redo());

        assert!(d.redo());
        assert_eq!(d.layer_count(), 2);
    }

    #[test]
    fn undo_reconciles_a_dangling_active_layer() {
        let mut d = doc();
        let added = d.add_layer(None);
        assert_eq!(d.active_layer_id(), added);

        d.undo();
        // The active layer no longer exists in the restored stack.
        assert!(
            d.active_layer().is_some(),
            "active layer points at a deleted layer"
        );
    }

    #[test]
    fn a_whole_stroke_is_a_single_undo_step() {
        let mut d = Document::new_transparent(32, 32);
        let brush = Brush {
            size: 8.0,
            ..Default::default()
        };

        let before = d.history().len();
        assert!(d.begin_stroke(&brush, 5.0, 16.0, 1.0));
        for x in 6..28 {
            d.extend_stroke(&brush, x as f32, 16.0, 1.0);
        }
        d.end_stroke(Rgba8::BLACK, 1.0);

        assert_eq!(
            d.history().len(),
            before + 1,
            "stroke recorded more than one history state"
        );
        assert_eq!(d.history().undo_name(), Some("Brush Tool"));
    }

    #[test]
    fn stroke_paints_onto_the_active_layer() {
        let mut d = Document::new_transparent(32, 32);
        let brush = Brush {
            size: 10.0,
            ..Default::default()
        };
        d.begin_stroke(&brush, 16.0, 16.0, 1.0);
        d.end_stroke(Rgba8::new(255, 0, 0, 255), 1.0);

        let p = d.composite().get(16, 16);
        assert!(p.a > 200 && p.r > 200, "stroke did not paint: {:?}", p);
    }

    #[test]
    fn undo_after_a_stroke_restores_the_blank_layer() {
        let mut d = Document::new_transparent(32, 32);
        let brush = Brush {
            size: 10.0,
            ..Default::default()
        };
        d.begin_stroke(&brush, 16.0, 16.0, 1.0);
        d.end_stroke(Rgba8::BLACK, 1.0);
        assert!(d.composite().get(16, 16).a > 0);

        assert!(d.undo());
        assert_eq!(d.composite().get(16, 16).a, 0, "undo left paint behind");
    }

    #[test]
    fn cancel_stroke_discards_it() {
        let mut d = Document::new_transparent(32, 32);
        let brush = Brush {
            size: 10.0,
            ..Default::default()
        };
        let before = d.history().len();
        d.begin_stroke(&brush, 16.0, 16.0, 1.0);
        d.cancel_stroke();

        assert_eq!(d.composite().get(16, 16).a, 0);
        assert_eq!(d.history().len(), before, "cancelled stroke was recorded");
    }

    #[test]
    fn stroke_on_an_adjustment_layer_is_refused() {
        let mut d = doc();
        d.add_adjustment_layer(Adjustment::Invert);
        assert!(
            !d.begin_stroke(&Brush::default(), 8.0, 8.0, 1.0),
            "painting on an adjustment layer should be refused"
        );
    }

    #[test]
    fn stroke_on_a_pixel_locked_layer_is_refused() {
        let mut d = doc();
        d.active_layer_mut().unwrap().lock_pixels = true;
        assert!(!d.begin_stroke(&Brush::default(), 8.0, 8.0, 1.0));
    }

    #[test]
    fn preview_shows_the_stroke_without_committing_it() {
        let mut d = Document::new_transparent(32, 32);
        let brush = Brush {
            size: 10.0,
            ..Default::default()
        };
        d.begin_stroke(&brush, 16.0, 16.0, 1.0);

        let preview = d.preview_stroke(Rgba8::BLACK, 1.0).unwrap();
        assert!(preview.get(16, 16).a > 0, "preview missing the stroke");
        // The document itself is untouched until end_stroke.
        assert_eq!(d.composite().get(16, 16).a, 0, "preview mutated the document");
    }

    #[test]
    fn fill_respects_the_selection() {
        let mut d = Document::new_transparent(16, 16);
        d.select_rect(Rect::new(0, 0, 8, 16), SelectionOp::Replace, 0);
        d.fill(Rgba8::new(255, 0, 0, 255));

        let out = d.composite();
        assert_eq!(out.get(4, 8).r, 255);
        assert_eq!(out.get(12, 8).a, 0, "fill leaked outside the selection");
    }

    #[test]
    fn fill_without_a_selection_covers_the_layer() {
        let mut d = Document::new_transparent(8, 8);
        d.fill(Rgba8::new(0, 255, 0, 255));
        assert_eq!(d.composite().get(7, 7).g, 255);
    }

    #[test]
    fn clear_erases_within_the_selection() {
        let mut d = doc();
        d.select_rect(Rect::new(0, 0, 8, 16), SelectionOp::Replace, 0);
        d.clear_selection_pixels();

        let out = d.composite();
        assert_eq!(out.get(4, 8).a, 0, "selection was not cleared");
        assert_eq!(out.get(12, 8), Rgba8::WHITE, "cleared outside the selection");
    }

    #[test]
    fn adjustment_layer_affects_the_composite_non_destructively() {
        let mut d = doc();
        d.add_adjustment_layer(Adjustment::Invert);

        let p = d.composite().get(8, 8);
        assert!(p.r < 5, "adjustment layer had no effect: {:?}", p);
        // The Background layer's own pixels are untouched.
        assert_eq!(d.layers().get(0).unwrap().pixels.get(8, 8), Rgba8::WHITE);
    }

    #[test]
    fn destructive_adjustment_modifies_layer_pixels() {
        let mut d = doc();
        d.apply_adjustment(Adjustment::Invert);
        assert!(d.layers().get(0).unwrap().pixels.get(8, 8).r < 5);
    }

    #[test]
    fn adjustments_skip_non_raster_layers() {
        let mut d = doc();
        let adj = d.add_adjustment_layer(Adjustment::Invert);
        d.set_active_layer(adj);
        // Should be a no-op rather than a panic.
        d.apply_adjustment(Adjustment::Invert);
        d.apply_filter(Filter::Sharpen);
    }

    #[test]
    fn offset_layer_moves_content() {
        let mut d = doc();
        let id = d.active_layer_id();
        d.offset_layer(id, 4, 4);
        assert_eq!(d.layers().by_id(id).unwrap().offset, (4, 4));
        assert_eq!(d.composite().get(0, 0).a, 0, "content did not move");
    }

    #[test]
    fn position_lock_blocks_moving() {
        let mut d = doc();
        let id = d.active_layer_id();
        d.active_layer_mut().unwrap().lock_position = true;
        d.offset_layer(id, 4, 4);
        assert_eq!(d.layers().by_id(id).unwrap().offset, (0, 0));
    }

    #[test]
    fn resize_canvas_updates_size_and_selection() {
        let mut d = doc();
        d.select_all();
        d.resize_canvas(32, 32);
        assert_eq!(d.size(), (32, 32));
        assert_eq!(d.selection().width(), 32);
        assert_eq!(d.composite().width(), 32);
    }

    #[test]
    fn display_name_marks_unsaved_changes() {
        let mut d = doc();
        assert_eq!(d.display_name(), "Untitled-1");
        d.add_layer(None);
        assert_eq!(d.display_name(), "Untitled-1*");
        d.mark_saved();
        assert_eq!(d.display_name(), "Untitled-1");
    }

    #[test]
    fn display_name_uses_the_file_name() {
        let mut d = doc();
        d.path = Some("/home/user/pictures/sunset.psd".to_string());
        assert_eq!(d.display_name(), "sunset.psd");
    }

    #[test]
    fn jump_to_history_moves_the_document() {
        let mut d = doc();
        d.add_layer(None);
        d.add_layer(None);
        assert_eq!(d.layer_count(), 3);

        assert!(d.jump_to_history(0));
        assert_eq!(d.layer_count(), 1);
        assert!(!d.jump_to_history(99));
    }

    #[test]
    fn selection_helpers_round_trip() {
        let mut d = doc();
        assert!(!d.has_selection());
        d.select_all();
        assert!(d.has_selection());
        d.invert_selection();
        assert!(!d.has_selection(), "inverting a full selection empties it");
        d.deselect();
        assert!(!d.has_selection());
    }
}

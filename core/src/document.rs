//! The document — one open image.
//!
//! Owns the layer stack, the selection, the undo history and the in-progress
//! brush stroke, and is the single object the bridge drives. Every mutating
//! method that a user would recognise as "an action" records a history state.

use crate::annotation::Annotations;
use crate::blend::BlendMode;
use crate::brush::{Brush, StrokeMask};
use crate::buffer::{Pixmap, Rect, Rgba8};
use crate::compositor;
use crate::filters::{Adjustment, Filter};
use crate::healing::{self, HealMode, Transfer};
use crate::history::History;
use crate::layer::{Layer, LayerId, LayerKind, LayerStack};
use crate::perspective;
use crate::selection::{Selection, SelectionOp};
use crate::slice::{Slice, SliceSet};

/// What the Patch tool was asked to do — CS6's options bar, as one value.
#[derive(Clone, Copy, Debug, Default)]
pub struct PatchOptions {
    /// The drag, in document pixels.
    pub dx: i32,
    pub dy: i32,
    /// Rebuild the selection from its surroundings and ignore the drag.
    pub content_aware: bool,
    /// Treat the selection as the source and the dragged-to area as the target,
    /// rather than the other way round.
    pub destination: bool,
    /// Transfer texture only, keeping the patched area's own colour.
    pub transparent: bool,
}

/// One open image.
pub struct Document {
    width: u32,
    height: u32,
    stack: LayerStack,
    selection: Selection,
    history: History,
    /// Web-export slices. Not part of a history state: Photoshop does not put
    /// slice edits on the History panel either.
    slices: SliceSet,
    /// Colour samplers, notes, count markers and the ruler. Like slices, these
    /// annotate the document without editing it, and stay off the History
    /// panel for the same reason.
    annotations: Annotations,

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

        let history = History::new(stack.clone(), (width, height));
        Self {
            width,
            height,
            stack,
            selection: Selection::new(width, height),
            history,
            slices: SliceSet::new(),
            annotations: Annotations::new(),
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
        doc.history = History::new(doc.stack.clone(), (doc.width, doc.height));
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

        let history = History::new(stack.clone(), (width, height));
        Self {
            width,
            height,
            stack,
            selection: Selection::new(width, height),
            history,
            slices: SliceSet::new(),
            annotations: Annotations::new(),
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

    /// Combine a freehand/polygonal region — the lasso family. `points` are
    /// document-space vertices; the shape closes back to the first.
    pub fn select_polygon(&mut self, points: &[(f32, f32)], op: SelectionOp, feather: u32) {
        self.selection.apply_polygon_feathered(points, op, feather);
    }

    /// Combine a coverage mask produced by the magic wand or quick selector.
    pub fn select_mask(&mut self, coverage: &[u8], op: SelectionOp, feather: u32) {
        self.selection.apply_mask_feathered(coverage, op, feather);
    }

    /// Replace the selection outright, for the live preview a Quick Selection
    /// drag paints as it goes.
    pub fn set_selection(&mut self, selection: Selection) {
        self.selection = selection;
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

    /// Finish the stroke by *healing* what it covered rather than painting it.
    ///
    /// The Spot Healing Brush works this way round: the brush marks a region,
    /// and the region is then rebuilt from the pixels around it. That is why it
    /// happens here at the end of the stroke and not dab by dab — every dab
    /// would otherwise heal from the previous dab's output and the stroke would
    /// smear along itself.
    pub fn end_heal_stroke(&mut self, mode: HealMode) -> Rect {
        self.finish_stroke_with("Spot Healing Brush", |pixels, region, coverage| {
            healing::heal_region(pixels, region, coverage, mode)
        })
    }

    /// Finish the stroke by cloning from an offset source — the Healing Brush.
    ///
    /// Unlike the Spot Healing Brush this takes an explicit source (Alt-clicked
    /// by the user), and transplants its texture with the destination's own
    /// lighting.
    pub fn end_heal_clone_stroke(&mut self, dx: i32, dy: i32) -> Rect {
        self.finish_stroke_with("Healing Brush", |pixels, region, coverage| {
            healing::clone_region(pixels, region, coverage, (dx, dy), Transfer::Full)
        })
    }

    /// Shared tail of the healing strokes: take the stroke mask, turn it into
    /// coverage in the layer's own coordinates, run `op`, and commit.
    fn finish_stroke_with<F>(&mut self, name: &str, op: F) -> Rect
    where
        F: FnOnce(&mut Pixmap, Rect, &[f32]) -> Rect,
    {
        let Some(mask) = self.stroke.take() else {
            return Rect::default();
        };
        self.stroke_undo_base = None;

        if mask.is_empty() {
            return Rect::default();
        }
        let region = mask.dirty();
        if region.is_empty() {
            return Rect::default();
        }

        let selection = if self.selection.is_empty() {
            None
        } else {
            Some(self.selection.clone())
        };

        let id = self.active_layer;
        let Some(layer) = self.stack.by_id_mut(id) else {
            return Rect::default();
        };
        if layer.lock_pixels {
            return Rect::default();
        }
        let offset = layer.offset;

        let mut coverage = vec![0.0f32; (region.width as usize) * (region.height as usize)];
        for y in 0..region.height as i32 {
            for x in 0..region.width as i32 {
                let (doc_x, doc_y) = (region.x + x, region.y + y);
                let mut c = mask.coverage_at(doc_x, doc_y);
                if let Some(sel) = selection.as_ref() {
                    c *= sel.coverage_at(doc_x, doc_y);
                }
                coverage[(y as usize) * (region.width as usize) + x as usize] = c;
            }
        }

        let local = Rect::new(
            region.x - offset.0,
            region.y - offset.1,
            region.width,
            region.height,
        );
        let dirty = op(&mut layer.pixels, local, &coverage);
        if dirty.is_empty() {
            return Rect::default();
        }

        self.commit(name);
        Rect::new(dirty.x + offset.0, dirty.y + offset.1, dirty.width, dirty.height)
    }

    /// Apply the Patch tool.
    ///
    /// The options mirror CS6's bar:
    ///
    /// * **Source** (`destination = false`) — the selection is the flaw, and the
    ///   drag says where to sample the repair from.
    /// * **Destination** — the roles reverse: the selection is good material,
    ///   and the drag says where to apply it.
    /// * **Transparent** — transfer only the source's texture, leaving the
    ///   patched area its own colour.
    /// * **Content-Aware** — ignore the drag entirely and rebuild the selection
    ///   from its surroundings, as the Spot Healing Brush does.
    pub fn patch_selection(&mut self, options: PatchOptions) -> Rect {
        if options.content_aware {
            // Nothing is sampled from a drag in this mode; the selection is
            // simply reconstructed in place.
            return self.apply_to_selection_at("Patch Tool", (0, 0), |pixels, region, cov| {
                healing::heal_region(pixels, region, cov, HealMode::ContentAware)
            });
        }

        let transfer = if options.transparent {
            Transfer::TextureOnly
        } else {
            Transfer::Full
        };
        let (dx, dy) = (options.dx, options.dy);

        if options.destination {
            // Patch the area the selection was dragged *to*, taking its content
            // from where the selection sits.
            self.apply_to_selection_at("Patch Tool", (dx, dy), move |pixels, region, cov| {
                healing::clone_region(pixels, region, cov, (-dx, -dy), transfer)
            })
        } else {
            self.apply_to_selection_at("Patch Tool", (0, 0), move |pixels, region, cov| {
                healing::clone_region(pixels, region, cov, (dx, dy), transfer)
            })
        }
    }

    /// Move the selection's contents by `(dx, dy)` and heal what it leaves —
    /// the Content-Aware Move tool. `extend` duplicates instead of moving.
    pub fn content_aware_move(&mut self, dx: i32, dy: i32, extend: bool) -> Rect {
        self.apply_to_selection_at("Content-Aware Move", (0, 0), |pixels, region, coverage| {
            healing::move_region(pixels, region, coverage, dx, dy, extend)
        })
    }

    /// Neutralise red-eye inside `rect` — the Red Eye tool.
    ///
    /// This one takes a rectangle rather than the selection: CS6's Red Eye tool
    /// is dragged over an eye directly.
    pub fn remove_red_eye(&mut self, rect: Rect, pupil: u32, darken: u32) -> Rect {
        let rect = rect.intersect(&Rect::from_size(self.width, self.height));
        if rect.is_empty() {
            return Rect::default();
        }
        let coverage = vec![1.0f32; (rect.width as usize) * (rect.height as usize)];

        let id = self.active_layer;
        let Some(layer) = self.stack.by_id_mut(id) else {
            return Rect::default();
        };
        if layer.lock_pixels {
            return Rect::default();
        }
        let offset = layer.offset;
        let local = Rect::new(rect.x - offset.0, rect.y - offset.1, rect.width, rect.height);

        let dirty = healing::red_eye_region(&mut layer.pixels, local, &coverage, pupil, darken);
        if dirty.is_empty() {
            return Rect::default();
        }
        self.commit("Red Eye Tool");
        Rect::new(dirty.x + offset.0, dirty.y + offset.1, dirty.width, dirty.height)
    }

    /// Run a healing operation over the active selection, optionally displaced.
    ///
    /// The Patch and Content-Aware Move tools both work on a selection rather
    /// than a brush stroke, and both need it as coverage over its bounding box.
    /// `offset` moves where that coverage is *applied* while keeping its shape —
    /// which is what the Patch tool's Destination mode needs.
    fn apply_to_selection_at<F>(&mut self, name: &str, offset: (i32, i32), op: F) -> Rect
    where
        F: FnOnce(&mut Pixmap, Rect, &[f32]) -> Rect,
    {
        if self.selection.is_empty() {
            return Rect::default();
        }
        let bounds = self.selection.bounds();
        if bounds.is_empty() {
            return Rect::default();
        }

        let mut coverage = vec![0.0f32; (bounds.width as usize) * (bounds.height as usize)];
        for y in 0..bounds.height as i32 {
            for x in 0..bounds.width as i32 {
                coverage[(y as usize) * (bounds.width as usize) + x as usize] =
                    self.selection.coverage_at(bounds.x + x, bounds.y + y);
            }
        }

        let region = Rect::new(
            bounds.x + offset.0,
            bounds.y + offset.1,
            bounds.width,
            bounds.height,
        );

        let id = self.active_layer;
        let Some(layer) = self.stack.by_id_mut(id) else {
            return Rect::default();
        };
        if layer.lock_pixels {
            return Rect::default();
        }
        let offset = layer.offset;
        let local = Rect::new(
            region.x - offset.0,
            region.y - offset.1,
            region.width,
            region.height,
        );

        let dirty = op(&mut layer.pixels, local, &coverage);
        if dirty.is_empty() {
            return Rect::default();
        }
        self.commit(name);
        Rect::new(dirty.x + offset.0, dirty.y + offset.1, dirty.width, dirty.height)
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

    /// Crop the document to `rect`, in document coordinates.
    ///
    /// Every layer moves with the canvas rather than being resampled: only the
    /// origin changes, so nothing is resized or blurred. `delete_cropped`
    /// mirrors CS6's checkbox — when set, pixels now outside the canvas are
    /// discarded; when clear they are kept, hanging off the edge, and come back
    /// if the canvas is enlarged again.
    ///
    /// A rect that misses the canvas entirely, or has no area, is ignored.
    pub fn crop(&mut self, rect: Rect, delete_cropped: bool) {
        let rect = rect.intersect(&Rect::from_size(self.width, self.height));
        if rect.is_empty() {
            return;
        }

        for layer in self.stack.iter_mut() {
            layer.offset = (layer.offset.0 - rect.x, layer.offset.1 - rect.y);

            if !delete_cropped || layer.pixels.is_empty() {
                continue;
            }

            // The part of this layer still on the canvas, in the layer's own
            // coordinates.
            let canvas = Rect::from_size(rect.width, rect.height);
            let bounds = Rect::new(
                layer.offset.0,
                layer.offset.1,
                layer.pixels.width(),
                layer.pixels.height(),
            );
            let keep = bounds.intersect(&canvas);
            if keep.is_empty() {
                // Nothing of this layer survives. Keep the layer — deleting it
                // would be a structural change the user did not ask for — but
                // drop its pixels.
                layer.pixels = Pixmap::new(0, 0);
                layer.mask = None;
                layer.offset = (0, 0);
                continue;
            }

            let local = Rect::new(
                keep.x - layer.offset.0,
                keep.y - layer.offset.1,
                keep.width,
                keep.height,
            );
            layer.pixels = layer.pixels.crop(local);
            // The mask is stored at the same size and origin as the pixels, so
            // it has to be cropped identically or the two fall out of step.
            if let Some(mask) = layer.mask.as_ref() {
                layer.mask = Some(mask.crop(local));
            }
            layer.offset = (keep.x, keep.y);
        }

        self.width = rect.width;
        self.height = rect.height;
        self.selection.crop(rect);
        if let Some(s) = self.stroke.as_mut() {
            s.resize(rect.width, rect.height);
        }
        self.commit("Crop");
    }

    /// Straighten a quadrilateral into a rectangle and crop to it — the
    /// Perspective Crop tool.
    ///
    /// `quad` is the four corners in document coordinates, ordered top-left,
    /// top-right, bottom-right, bottom-left. Unlike an ordinary crop this
    /// *resamples*: every layer is warped through the same homography, so the
    /// stack stays in register.
    ///
    /// Returns false, changing nothing, for a degenerate quad.
    pub fn perspective_crop(&mut self, quad: &[(f32, f32); 4]) -> bool {
        let (width, height) = perspective::suggested_size(quad);
        let Some(map) = perspective::inverse_map(quad, width, height) else {
            return false;
        };

        for layer in self.stack.iter_mut() {
            // Adjustment and fill layers have no pixels to warp; they are
            // evaluated over whatever canvas they end up on.
            if layer.pixels.is_empty() {
                continue;
            }
            layer.pixels = perspective::warp(&layer.pixels, layer.offset, &map, width, height);
            if let Some(mask) = layer.mask.as_ref() {
                layer.mask = Some(perspective::warp(mask, layer.offset, &map, width, height));
            }
            // The warp resolves everything into canvas coordinates, so no
            // layer hangs off the edge any more.
            layer.offset = (0, 0);
        }

        let warped = perspective::warp_mask(
            self.selection.as_bytes(),
            self.width,
            self.height,
            &map,
            width,
            height,
        );
        if let Some(selection) = Selection::from_coverage(width, height, warped) {
            self.selection = selection;
        }

        self.width = width;
        self.height = height;
        self.stroke = None;
        self.commit("Perspective Crop");
        true
    }

    // -- annotations ----------------------------------------------------------

    pub fn annotations(&self) -> &Annotations {
        &self.annotations
    }

    pub fn annotations_mut(&mut self) -> &mut Annotations {
        &mut self.annotations
    }

    // -- slices ---------------------------------------------------------------

    pub fn slices(&self) -> &SliceSet {
        &self.slices
    }

    pub fn slices_mut(&mut self) -> &mut SliceSet {
        &mut self.slices
    }

    /// The full slice list — user slices plus the auto slices filling the rest
    /// of the canvas — numbered in reading order.
    pub fn resolved_slices(&self) -> Vec<Slice> {
        self.slices.resolve(Rect::from_size(self.width, self.height))
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

        if let Some(state) = self.history.undo() {
            let (stack, size) = (state.stack.clone(), state.size);
            self.stack = stack;
            self.restore_size(size);
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

        if let Some(state) = self.history.redo() {
            let (stack, size) = (state.stack.clone(), state.size);
            self.stack = stack;
            self.restore_size(size);
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

        if let Some(state) = self.history.jump_to(index) {
            let (stack, size) = (state.stack.clone(), state.size);
            self.stack = stack;
            self.restore_size(size);
            self.reconcile_active_layer();
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Record the current stack as a new history state.
    pub fn commit(&mut self, name: impl Into<String>) {
        self.history.push(name, self.stack.clone(), (self.width, self.height));
        self.dirty = true;
    }

    /// Adopt a canvas size restored from history.
    ///
    /// Crop and Canvas Size change the dimensions, so stepping across one of
    /// those states has to bring the selection and any live stroke buffer with
    /// it or they are left sized for a document that no longer exists.
    fn restore_size(&mut self, size: (u32, u32)) {
        if (self.width, self.height) == size {
            return;
        }
        self.width = size.0;
        self.height = size.1;
        self.selection.resize(size.0, size.1);
        self.stroke = None;
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
    fn crop_resizes_the_canvas_and_keeps_the_right_pixels() {
        let mut d = Document::new(32, 32, Rgba8::WHITE);
        // Mark one pixel so we can tell whether the right region survived.
        d.active_layer_mut().unwrap().pixels.set(20, 20, Rgba8::BLACK);

        d.crop(Rect::new(16, 16, 8, 8), true);

        assert_eq!(d.size(), (8, 8));
        assert_eq!(d.composite().width(), 8);
        // (20, 20) in the old document is (4, 4) in the new one.
        assert_eq!(d.composite().get(4, 4), Rgba8::BLACK);
        assert_eq!(d.composite().get(0, 0), Rgba8::WHITE);
    }

    #[test]
    fn crop_moves_the_selection_with_the_canvas() {
        let mut d = Document::new(32, 32, Rgba8::WHITE);
        d.select_rect(Rect::new(16, 16, 8, 8), SelectionOp::Replace, 0);
        d.crop(Rect::new(16, 16, 8, 8), true);

        assert_eq!(d.selection().width(), 8);
        assert_eq!(d.selection().coverage_at(0, 0), 1.0, "selection did not move with the crop");
        assert_eq!(d.selection().coverage_at(7, 7), 1.0);
    }

    #[test]
    fn crop_without_deleting_keeps_pixels_off_canvas() {
        let mut d = Document::new(32, 32, Rgba8::WHITE);
        d.active_layer_mut().unwrap().pixels.set(2, 2, Rgba8::BLACK);
        d.crop(Rect::new(16, 16, 8, 8), false);

        // The layer still holds its full 32×32 buffer, now hanging off the
        // top-left of the smaller canvas.
        let layer = d.active_layer().unwrap();
        assert_eq!(layer.pixels.width(), 32);
        assert_eq!(layer.offset, (-16, -16));

        // Deleting instead trims the buffer to the canvas.
        let mut d = Document::new(32, 32, Rgba8::WHITE);
        d.crop(Rect::new(16, 16, 8, 8), true);
        assert_eq!(d.active_layer().unwrap().pixels.width(), 8);
        assert_eq!(d.active_layer().unwrap().offset, (0, 0));
    }

    #[test]
    fn crop_clamps_to_the_canvas() {
        let mut d = Document::new(32, 32, Rgba8::WHITE);
        // A rect running off the bottom-right takes only what exists.
        d.crop(Rect::new(24, 24, 100, 100), true);
        assert_eq!(d.size(), (8, 8));
    }

    #[test]
    fn a_degenerate_crop_is_ignored() {
        let mut d = Document::new(32, 32, Rgba8::WHITE);
        d.crop(Rect::new(4, 4, 0, 0), true);
        assert_eq!(d.size(), (32, 32), "an empty rect cropped the document");
        d.crop(Rect::new(100, 100, 8, 8), true);
        assert_eq!(d.size(), (32, 32), "an off-canvas rect cropped the document");
    }

    #[test]
    fn crop_is_undoable() {
        let mut d = Document::new(32, 32, Rgba8::WHITE);
        d.crop(Rect::new(8, 8, 16, 16), true);
        assert_eq!(d.size(), (16, 16));
        d.undo();
        assert_eq!(d.size(), (32, 32), "undo did not restore the canvas");
    }

    #[test]
    fn crop_keeps_a_layer_mask_aligned_with_its_pixels() {
        let mut d = Document::new(32, 32, Rgba8::WHITE);
        let id = d.active_layer_id();
        d.add_layer_mask(id, true);
        d.crop(Rect::new(8, 8, 16, 16), true);

        let layer = d.active_layer().unwrap();
        let mask = layer.mask.as_ref().expect("the mask was dropped");
        assert_eq!(mask.width(), layer.pixels.width());
        assert_eq!(mask.height(), layer.pixels.height());
    }

    #[test]
    fn healing_a_stroke_rebuilds_it_from_the_surroundings() {
        let mut d = Document::new(64, 64, Rgba8::new(190, 160, 140, 255));
        // A dark blemish for the brush to remove.
        if let Some(layer) = d.active_layer_mut() {
            for y in 28..36 {
                for x in 28..36 {
                    layer.pixels.set(x, y, Rgba8::new(60, 30, 30, 255));
                }
            }
        }

        let mut brush = Brush::default();
        brush.size = 22.0;
        brush.hardness = 100.0;
        assert!(d.begin_stroke(&brush, 32.0, 32.0, 1.0));
        let dirty = d.end_heal_stroke(HealMode::ProximityMatch);
        assert!(!dirty.is_empty(), "healing reported nothing changed");

        let px = d.composite().get(32, 32);
        assert!(
            (px.r as i32 - 190).abs() <= 6,
            "the blemish survived healing: {:?}",
            px
        );
    }

    #[test]
    fn healing_is_one_undo_step() {
        let mut d = Document::new(64, 64, Rgba8::new(190, 160, 140, 255));
        if let Some(layer) = d.active_layer_mut() {
            layer.pixels.set(32, 32, Rgba8::BLACK);
        }
        // Record the setup, so undo has the blemish to come back to rather than
        // the blank document underneath it.
        d.commit("Setup");
        let before = d.composite().get(32, 32);

        let mut brush = Brush::default();
        brush.size = 18.0;
        d.begin_stroke(&brush, 32.0, 32.0, 1.0);
        // Several dabs, as a real drag would produce.
        d.extend_stroke(&brush, 34.0, 32.0, 1.0);
        d.extend_stroke(&brush, 36.0, 32.0, 1.0);
        d.end_heal_stroke(HealMode::ContentAware);
        assert_ne!(d.composite().get(32, 32), before);

        assert!(d.undo(), "nothing to undo after healing");
        assert_eq!(d.composite().get(32, 32), before, "one undo did not restore the stroke");
    }

    #[test]
    fn healing_respects_a_locked_layer() {
        let mut d = Document::new(32, 32, Rgba8::WHITE);
        let mut brush = Brush::default();
        brush.size = 10.0;
        d.begin_stroke(&brush, 16.0, 16.0, 1.0);
        if let Some(layer) = d.active_layer_mut() {
            layer.lock_pixels = true;
        }
        assert!(d.end_heal_stroke(HealMode::ProximityMatch).is_empty());
    }

    #[test]
    fn healing_outside_the_selection_is_confined() {
        let mut d = Document::new(64, 64, Rgba8::new(200, 200, 200, 255));
        if let Some(layer) = d.active_layer_mut() {
            for y in 20..44 {
                for x in 20..44 {
                    layer.pixels.set(x, y, Rgba8::BLACK);
                }
            }
        }
        // Only the left half is selected.
        d.select_rect(Rect::new(0, 0, 32, 64), SelectionOp::Replace, 0);

        let mut brush = Brush::default();
        brush.size = 40.0;
        brush.hardness = 100.0;
        d.begin_stroke(&brush, 32.0, 32.0, 1.0);
        d.end_heal_stroke(HealMode::ProximityMatch);

        // Right of the selection edge the black blot must be untouched.
        assert_eq!(d.composite().get(40, 32), Rgba8::BLACK, "healing escaped the selection");
    }

    /// A light field with a dark blot on the left, for the Patch tests.
    fn patch_doc() -> Document {
        let mut d = Document::new(140, 60, Rgba8::new(210, 200, 190, 255));
        if let Some(layer) = d.active_layer_mut() {
            for y in 20..40 {
                for x in 20..40 {
                    layer.pixels.set(x, y, Rgba8::new(40, 30, 30, 255));
                }
            }
        }
        d.commit("Setup");
        d
    }

    #[test]
    fn patch_in_source_mode_repairs_the_selection() {
        let mut d = patch_doc();
        d.select_rect(Rect::new(20, 20, 20, 20), SelectionOp::Replace, 0);

        let options = PatchOptions { dx: 60, dy: 0, ..PatchOptions::default() };
        assert!(!d.patch_selection(options).is_empty());

        // The selected blot is gone, and the sampled area is untouched.
        assert!(d.composite().get(30, 30).r > 140, "the blot survived the patch");
        assert!(d.composite().get(90, 30).r > 140, "the source area was modified");
    }

    #[test]
    fn patch_in_destination_mode_patches_where_it_was_dragged() {
        // Select clean pixels and drag them onto the blot: the blot end changes
        // and the selection itself does not.
        let mut d = patch_doc();
        d.select_rect(Rect::new(80, 20, 20, 20), SelectionOp::Replace, 0);

        let options = PatchOptions {
            dx: -60,
            dy: 0,
            destination: true,
            ..PatchOptions::default()
        };
        assert!(!d.patch_selection(options).is_empty());

        assert!(d.composite().get(30, 30).r > 140, "the destination was not patched");
    }

    #[test]
    fn source_and_destination_modes_change_opposite_ends() {
        // The same drag in the two modes must edit different places.
        let mut source = patch_doc();
        source.select_rect(Rect::new(20, 20, 20, 20), SelectionOp::Replace, 0);
        source.patch_selection(PatchOptions { dx: 60, dy: 0, ..PatchOptions::default() });

        let mut destination = patch_doc();
        destination.select_rect(Rect::new(20, 20, 20, 20), SelectionOp::Replace, 0);
        destination.patch_selection(PatchOptions {
            dx: 60,
            dy: 0,
            destination: true,
            ..PatchOptions::default()
        });

        // Source mode fixed the blot; destination mode copied the blot rightward
        // and left it where it was.
        assert!(source.composite().get(30, 30).r > 140);
        assert!(destination.composite().get(30, 30).r < 120,
                "destination mode should have left the selection alone");
        assert!(destination.composite().get(90, 30).r < 160,
                "destination mode did not apply the patch at the drag target");
    }

    #[test]
    fn transparent_patch_keeps_the_destination_colour() {
        // A blue field with a blot, patched from a red area. Without
        // Transparent the patch is neutral; with it the blue survives.
        let build = || {
            let mut d = Document::new(140, 60, Rgba8::new(60, 90, 200, 255));
            if let Some(layer) = d.active_layer_mut() {
                for y in 20..40 {
                    for x in 80..120 {
                        layer.pixels.set(x, y, Rgba8::new(200, 80, 40, 255));
                    }
                }
            }
            d.commit("Setup");
            d.select_rect(Rect::new(20, 20, 20, 20), SelectionOp::Replace, 0);
            d
        };

        let mut transparent = build();
        transparent.patch_selection(PatchOptions {
            dx: 70,
            dy: 0,
            transparent: true,
            ..PatchOptions::default()
        });

        // Blue must still dominate red in the patched area.
        let px = transparent.composite().get(30, 30);
        assert!(px.b > px.r, "the destination colour was lost: {:?}", px);
    }

    #[test]
    fn content_aware_patch_ignores_the_drag() {
        // Two very different drags must give the same result, because this mode
        // rebuilds from the surroundings rather than sampling.
        let mut a = patch_doc();
        a.select_rect(Rect::new(20, 20, 20, 20), SelectionOp::Replace, 0);
        a.patch_selection(PatchOptions {
            dx: 60,
            dy: 0,
            content_aware: true,
            ..PatchOptions::default()
        });

        let mut b = patch_doc();
        b.select_rect(Rect::new(20, 20, 20, 20), SelectionOp::Replace, 0);
        b.patch_selection(PatchOptions {
            dx: -10,
            dy: 25,
            content_aware: true,
            ..PatchOptions::default()
        });

        assert_eq!(
            a.composite().as_bytes(),
            b.composite().as_bytes(),
            "content-aware patch depended on the drag"
        );
        assert!(a.composite().get(30, 30).r > 140, "content-aware patch left the blot");
    }

    #[test]
    fn patch_without_a_selection_does_nothing() {
        let mut d = patch_doc();
        let before = d.composite().as_bytes().to_vec();
        assert!(d
            .patch_selection(PatchOptions { dx: 40, dy: 0, ..PatchOptions::default() })
            .is_empty());
        assert_eq!(d.composite().as_bytes(), &before[..]);
    }

    #[test]
    fn perspective_crop_straightens_and_resizes() {
        let mut d = Document::new(64, 64, Rgba8::WHITE);
        // A keystoned quad: the top edge narrower than the bottom.
        let quad = [(20.0, 10.0), (44.0, 10.0), (56.0, 50.0), (8.0, 50.0)];
        assert!(d.perspective_crop(&quad));

        // 48 wide (the longer, bottom edge) and hypot(12, 40) ≈ 41.8 tall.
        assert_eq!(d.size(), (48, 42));
        assert_eq!(d.composite().width(), 48);
        assert_eq!(d.selection().width(), 48);
    }

    #[test]
    fn perspective_crop_pulls_the_marked_region_to_the_corners() {
        let mut d = Document::new(64, 64, Rgba8::WHITE);
        // Mark the four corners of the quad we are about to straighten; each
        // should end up at the matching corner of the result.
        d.active_layer_mut().unwrap().pixels.set(20, 10, Rgba8::BLACK);
        d.active_layer_mut().unwrap().pixels.set(43, 10, Rgba8::BLACK);

        let quad = [(20.0, 10.0), (44.0, 10.0), (44.0, 50.0), (20.0, 50.0)];
        assert!(d.perspective_crop(&quad));

        // An axis-aligned quad is just a crop, so this is exact.
        assert_eq!(d.size(), (24, 40));
        assert_eq!(d.composite().get(0, 0), Rgba8::BLACK);
        assert_eq!(d.composite().get(23, 0), Rgba8::BLACK);
        assert_eq!(d.composite().get(12, 20), Rgba8::WHITE);
    }

    #[test]
    fn a_degenerate_perspective_crop_is_refused() {
        let mut d = Document::new(64, 64, Rgba8::WHITE);
        // All four corners on one line: no homography exists.
        let line = [(0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (30.0, 0.0)];
        assert!(!d.perspective_crop(&line));
        assert_eq!(d.size(), (64, 64), "the document changed anyway");
    }

    #[test]
    fn perspective_crop_is_undoable() {
        let mut d = Document::new(64, 64, Rgba8::WHITE);
        let quad = [(20.0, 10.0), (44.0, 10.0), (56.0, 50.0), (8.0, 50.0)];
        d.perspective_crop(&quad);
        assert_ne!(d.size(), (64, 64));

        d.undo();
        assert_eq!(d.size(), (64, 64), "undo did not restore the canvas");
        assert_eq!(d.selection().width(), 64);
    }

    #[test]
    fn perspective_crop_keeps_every_layer_in_register() {
        let mut d = Document::new(64, 64, Rgba8::WHITE);
        d.add_layer(None);
        let id = d.active_layer_id();
        d.add_layer_mask(id, true);

        let quad = [(20.0, 10.0), (44.0, 10.0), (56.0, 50.0), (8.0, 50.0)];
        assert!(d.perspective_crop(&quad));

        let (w, h) = d.size();
        for layer in d.layers().iter() {
            assert_eq!(layer.offset, (0, 0), "layer {} kept an offset", layer.name);
            assert_eq!(layer.pixels.width(), w);
            assert_eq!(layer.pixels.height(), h);
            if let Some(mask) = layer.mask.as_ref() {
                assert_eq!(mask.width(), w, "mask fell out of step with its pixels");
                assert_eq!(mask.height(), h);
            }
        }
    }

    #[test]
    fn stepping_across_a_crop_resizes_the_selection_too() {
        // The selection is sized to the canvas, so a history step that changes
        // the canvas has to bring it along or the two disagree.
        let mut d = Document::new(32, 32, Rgba8::WHITE);
        d.crop(Rect::new(8, 8, 16, 16), true);
        assert_eq!(d.selection().width(), 16);

        d.undo();
        assert_eq!(d.selection().width(), 32, "selection kept the cropped size");
        d.redo();
        assert_eq!(d.size(), (16, 16), "redo did not re-apply the crop");
        assert_eq!(d.selection().width(), 16);
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

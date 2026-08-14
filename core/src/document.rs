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
use crate::healing::{self, HealMode, MoveOptions, Transfer};
use crate::history::History;
use crate::layer::{Layer, LayerId, LayerKind, LayerStack, TextContent};
use crate::perspective;
use crate::mixer::{MixerBrush, MixerOptions, Sampled};
use crate::replace::{ColorReplacer, ReplaceOptions, ReplaceSampling};
use crate::focus::{self, FocusOptions};
use crate::smudge::{Smudge, SmudgeOptions};
use crate::tone::{ToneOptions, ToneStroke};
use crate::bucket::{self, BucketOptions, FloodMask};
use crate::wand;
use crate::gradient::{self, Gradient, GradientOptions};
use crate::stamp::{self, CloneSampling, CloneStroke};
use crate::selection::{Selection, SelectionOp};
use crate::path::PathSet;
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
    /// Vector paths from the Pen tool and the Paths panel. Also not part of a
    /// history state, and for the same reason: they are overlay geometry, not
    /// pixels, and what a finished path *does* to the image — Fill Path,
    /// Stroke Path, Make Selection — is what commits, exactly as if the user
    /// had used the Brush or the Lasso directly.
    paths: PathSet,

    /// Layer the tools act on.
    active_layer: LayerId,

    /// Scratch buffer for the stroke currently being drawn, if any.
    stroke: Option<StrokeMask>,
    /// Snapshot taken when the stroke began, so the whole stroke is one undo
    /// step rather than one per mouse-move.
    stroke_undo_base: Option<LayerStack>,

    /// State for a Color Replacement stroke. That tool edits the layer directly
    /// as it goes rather than accumulating into a mask, because what it replaces
    /// depends on what is already there.
    replacer: Option<ColorReplacer>,
    /// Where the replacement stroke last reached, for even dab spacing.
    replace_last: Option<(f32, f32)>,

    /// State for a Mixer Brush stroke. Direct-to-layer for the same reason the
    /// replacer is: each dab mixes with what the last one left.
    mixer: Option<MixerBrush>,
    /// Where the mixer stroke last reached, for even dab spacing.
    mixer_last: Option<(f32, f32)>,

    /// The Blur or Sharpen stroke in progress. Like the mixer's, it edits the
    /// layer dab by dab — each dab has to work on what the last one left, which
    /// is what makes dwelling deepen the effect.
    focus: Option<FocusOptions>,
    /// The Smudge stroke in progress, which additionally carries the patch of
    /// pixels the finger is dragging.
    smudge: Option<Smudge>,
    /// The Dodge, Burn or Sponge stroke in progress. It carries the coverage it
    /// has already applied, so a pass tones once rather than once per dab.
    tone: Option<ToneStroke>,
    /// Where the retouch stroke last reached, for even dab spacing. Only one of
    /// the six can be running at a time, so one field serves all.
    retouch_last: Option<(f32, f32)>,

    /// The source of the clone stroke in progress. Set only between
    /// `begin_clone_stroke` and the end of that stroke: what the Clone Stamp
    /// copies is the image as it was when the stroke started, so the snapshot
    /// belongs to the stroke rather than to the document.
    clone: Option<CloneStroke>,

    /// File path, once saved.
    pub path: Option<String>,
    /// Which "Untitled-N" this is, for a document that has never been saved.
    /// Photoshop numbers them from 1 upward across the session.
    pub untitled_number: u32,
    /// Set on every mutation, cleared on save.
    dirty: bool,

    /// The type layer the Type tool currently has open, and the visibility it
    /// had before the edit hid it. See [`Document::begin_text_edit`].
    text_edit: Option<(LayerId, bool)>,
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
            paths: PathSet::new(),
            active_layer: id,
            stroke: None,
            stroke_undo_base: None,
            replacer: None,
            replace_last: None,
            mixer: None,
            mixer_last: None,
            focus: None,
            smudge: None,
            tone: None,
            retouch_last: None,
            clone: None,
            path: None,
            untitled_number: 1,
            dirty: false,
            text_edit: None,
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
            paths: PathSet::new(),
            active_layer: id,
            stroke: None,
            stroke_undo_base: None,
            replacer: None,
            replace_last: None,
            mixer: None,
            mixer_last: None,
            focus: None,
            smudge: None,
            tone: None,
            retouch_last: None,
            clone: None,
            path: None,
            untitled_number: 1,
            dirty: false,
            text_edit: None,
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
        let untitled = format!("Untitled-{}", self.untitled_number);
        let base = self
            .path
            .as_deref()
            .and_then(|p| p.rsplit('/').next())
            .unwrap_or(&untitled);
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

    /// Add a layer of already-rasterized pixels above the active one and
    /// select it — what the Type tool commits.
    ///
    /// Text shaping and rendering happen in the C++ shell (CLAUDE.md §2: Qt's
    /// font engine is the natural tool for that, and re-implementing it here
    /// would mean shipping a second one); this stores the result like any
    /// other layer's pixels, plus the [`TextContent`] they came from so the
    /// layer can be reopened and retyped.
    pub fn add_text_layer(
        &mut self,
        pixels: Pixmap,
        offset: (i32, i32),
        name: String,
        text: TextContent,
    ) -> LayerId {
        let id = self.stack.allocate_id();
        let mut layer = Layer::new_raster(id, name, 0, 0);
        layer.pixels = pixels;
        layer.offset = offset;
        layer.text = Some(text);

        let at = self.active_index().map_or(self.stack.len(), |i| i + 1);
        self.stack.insert(at, layer);
        self.active_layer = id;
        self.commit("Type Tool");
        id
    }

    /// Re-render an existing type layer in place — the second and later commits
    /// of the same piece of text.
    ///
    /// The layer keeps its identity, and so its place in the stack, its blend
    /// mode, opacity, mask and everything else the user set on it: only the
    /// pixels, their offset, the name and the type record change. Returns false
    /// if the layer has gone (undone away mid-edit, say), which the caller
    /// treats as reason to add a fresh one instead.
    pub fn update_text_layer(
        &mut self,
        id: LayerId,
        pixels: Pixmap,
        offset: (i32, i32),
        name: String,
        text: TextContent,
    ) -> bool {
        let Some(layer) = self.stack.by_id_mut(id) else {
            return false;
        };
        layer.pixels = pixels;
        layer.offset = offset;
        layer.name = name;
        layer.text = Some(text);
        self.active_layer = id;
        self.commit("Edit Type Layer");
        true
    }

    /// The topmost type layer whose bounds contain a document-space point.
    ///
    /// Photoshop reopens text when you click anywhere in its bounding box, not
    /// only on an inked pixel, so this tests bounds. Hidden layers are skipped:
    /// clicking where invisible text happens to sit should start new text, not
    /// silently reopen something that is not on screen.
    pub fn text_layer_at(&self, x: i32, y: i32) -> Option<LayerId> {
        self.stack
            .iter()
            .rev()
            .find(|l| {
                l.text.is_some() && !l.is_invisible() && l.bounds().contains(x, y)
            })
            .map(|l| l.id)
    }

    /// Suppress a type layer's pixels while the Type tool has it open, so the
    /// live overlay is what the user sees rather than the overlay drawn on top
    /// of the previous rendering.
    ///
    /// Deliberately *not* a history step and not the Layers panel's eye: it is
    /// a view state belonging to an edit in progress, and it ends when the edit
    /// does. [`Document::text_edit_layer`] lets callers keep reporting the
    /// layer's real visibility while it is held down.
    pub fn begin_text_edit(&mut self, id: LayerId) -> bool {
        self.end_text_edit();
        let Some(layer) = self.stack.by_id_mut(id) else {
            return false;
        };
        let was_visible = layer.visible;
        layer.visible = false;
        self.text_edit = Some((id, was_visible));
        self.dirty = true;
        true
    }

    /// Restore the visibility [`Document::begin_text_edit`] took away.
    pub fn end_text_edit(&mut self) {
        if let Some((id, was_visible)) = self.text_edit.take() {
            if let Some(layer) = self.stack.by_id_mut(id) {
                layer.visible = was_visible;
            }
            self.dirty = true;
        }
    }

    /// The type layer currently open in the Type tool, and the visibility it
    /// will get back when the edit finishes.
    pub fn text_edit_layer(&self) -> Option<(LayerId, bool)> {
        self.text_edit
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

    /// Delete a layer. Refuses to remove the last remaining one, or a fully
    /// locked one — Photoshop will not throw away a layer you have locked
    /// against being touched.
    pub fn delete_layer(&mut self, id: LayerId) -> bool {
        if self.stack.len() <= 1 {
            return false;
        }
        let Some(index) = self.stack.index_of(id) else {
            return false;
        };
        if self.stack.get(index).is_some_and(Layer::is_fully_locked) {
            return false;
        }
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
    ///
    /// Refused when either layer is fully locked: the upper one would be
    /// destroyed and the lower one rewritten.
    pub fn merge_down(&mut self, id: LayerId) -> bool {
        let Some(index) = self.stack.index_of(id) else {
            return false;
        };
        if index == 0 {
            return false;
        }
        let locked = |i: usize| self.stack.get(i).is_some_and(Layer::is_fully_locked);
        if locked(index) || locked(index - 1) {
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

    /// Set the three locks on a layer in one step — the panel's Lock row.
    pub fn set_layer_locks(
        &mut self,
        id: LayerId,
        transparency: bool,
        pixels: bool,
        position: bool,
    ) {
        if let Some(l) = self.stack.by_id_mut(id) {
            if l.lock_transparency == transparency
                && l.lock_pixels == pixels
                && l.lock_position == position
            {
                return;
            }
            l.lock_transparency = transparency;
            l.lock_pixels = pixels;
            l.lock_position = position;
            self.commit("Lock Layer");
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
            // A type layer's anchor travels with its pixels, so reopening it
            // after a move resumes where the text now is rather than snapping
            // back to where it was first clicked.
            if let Some(text) = l.text.as_mut() {
                text.origin.0 += dx as f32;
                text.origin.1 += dy as f32;
            }
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

    /// Begin a Clone Stamp stroke.
    ///
    /// `offset` is added to a destination pixel to find its source, in document
    /// units — the delta between the Alt-clicked source point and where the
    /// stroke starts. Everything else about the stroke is an ordinary brush
    /// stroke, so this only adds the snapshot the dabs will copy from.
    ///
    /// Returns false when the active layer cannot be painted on.
    pub fn begin_clone_stroke(
        &mut self,
        brush: &Brush,
        x: f32,
        y: f32,
        pressure: f32,
        offset: (i32, i32),
        sampling: CloneSampling,
    ) -> bool {
        if offset == (0, 0) {
            // Sampling where it is painting would copy each pixel onto itself.
            return false;
        }
        let Some(layer) = self.active_layer() else {
            return false;
        };
        if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
            return false;
        }

        let source = stamp::snapshot(&self.stack, layer, self.width, self.height, sampling);
        if !self.begin_stroke(brush, x, y, pressure) {
            return false;
        }
        self.clone = Some(CloneStroke { source, offset });
        true
    }

    /// Whether a Clone Stamp stroke is in progress, and so whether ending the
    /// stroke should copy pixels rather than paint a colour.
    pub fn is_cloning(&self) -> bool {
        self.clone.is_some()
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
        // A clone stroke previews the pixels it is copying, not the foreground
        // colour. The shell asks for one preview whatever the tool, so the
        // decision belongs here.
        match self.clone.as_ref() {
            Some(clone) => mask.composite_source_onto(
                &mut target.pixels,
                &clone.source,
                clone.offset,
                opacity,
                target.offset,
                selection,
                target.lock_transparency,
            ),
            None => mask.composite_onto(
                &mut target.pixels,
                color,
                opacity,
                target.offset,
                selection,
                target.lock_transparency,
            ),
        };

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

    /// Finish a Clone Stamp stroke, copying the snapshot through the stroke's
    /// coverage and recording one history state for the whole thing.
    pub fn end_clone_stroke(&mut self, opacity: f32) -> Rect {
        let (Some(mask), Some(clone)) = (self.stroke.take(), self.clone.take()) else {
            self.stroke = None;
            self.clone = None;
            return Rect::default();
        };
        self.stroke_undo_base = None;
        if mask.is_empty() {
            return Rect::default();
        }

        let selection = if self.selection.is_empty() {
            None
        } else {
            Some(self.selection.clone())
        };

        let id = self.active_layer;
        let dirty = if let Some(layer) = self.stack.by_id_mut(id) {
            let offset = layer.offset;
            let lock = layer.lock_transparency;
            mask.composite_source_onto(
                &mut layer.pixels,
                &clone.source,
                clone.offset,
                opacity,
                offset,
                selection.as_ref(),
                lock,
            )
        } else {
            Rect::default()
        };

        self.commit("Clone Stamp");
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

    /// Move the selection's contents and heal what it leaves — the
    /// Content-Aware Move tool.
    ///
    /// With `sample_all_layers` the pixels read come from the composite rather
    /// than the active layer, so a subject spread across layers moves as it
    /// looks. The result is still written to the active layer alone.
    pub fn content_aware_move(
        &mut self,
        options: &MoveOptions,
        sample_all_layers: bool,
    ) -> Rect {
        let sampled = if sample_all_layers {
            Some(self.composite())
        } else {
            None
        };
        let options = *options;

        self.apply_to_selection_at("Content-Aware Move", (0, 0), move |pixels, region, cov| {
            // Without Sample All Layers the layer is both source and target;
            // reading its own pixels needs a snapshot, since the move writes
            // into it as it goes.
            match sampled {
                Some(source) => healing::move_region(pixels, &source, region, cov, &options),
                None => {
                    let snapshot = pixels.clone();
                    healing::move_region(pixels, &snapshot, region, cov, &options)
                }
            }
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

    /// Begin a Color Replacement stroke.
    ///
    /// `reference` is the colour to match for the sampling modes that fix it up
    /// front; Continuous sampling reads the layer as the brush moves and ignores
    /// it. `replacement` is the colour being painted. Returns false if the layer
    /// cannot be painted.
    #[allow(clippy::too_many_arguments)]
    pub fn begin_replace(
        &mut self,
        brush: &Brush,
        options: ReplaceOptions,
        reference: Option<Rgba8>,
        replacement: Rgba8,
        x: f32,
        y: f32,
        pressure: f32,
    ) -> bool {
        let Some(layer) = self.active_layer() else {
            return false;
        };
        if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
            return false;
        }

        self.stroke_undo_base = Some(self.stack.clone());
        let (w, h) = {
            let pixels = &self.active_layer().unwrap().pixels;
            (pixels.width(), pixels.height())
        };
        // Background Swatch sampling matches a colour that may appear nowhere
        // under the brush, so it still needs a reference even though nothing is
        // sampled from the image.
        let reference = match options.sampling {
            ReplaceSampling::Continuous => None,
            _ => reference,
        };
        self.replacer = Some(ColorReplacer::new(w, h, options, reference));
        self.replace_last = None;
        // The first dab must paint the replacement colour like every other one.
        // Passing anything else here marks its pixels as done, and the colour the
        // user actually chose never reaches them.
        self.extend_replace(brush, x, y, pressure, replacement);
        true
    }

    /// Continue a Color Replacement stroke, laying dabs to `(x, y)`.
    ///
    /// `replacement` is the colour being painted — the foreground.
    pub fn extend_replace(
        &mut self,
        brush: &Brush,
        x: f32,
        y: f32,
        pressure: f32,
        replacement: Rgba8,
    ) -> Rect {
        if self.replacer.is_none() {
            return Rect::default();
        }
        let id = self.active_layer;
        let offset = match self.stack.by_id(id) {
            Some(layer) => layer.offset,
            None => return Rect::default(),
        };

        // Dab positions along the segment, spaced as the brush asks.
        let step = (brush.size * brush.spacing.max(0.01)).max(0.5);
        let mut points = Vec::new();
        match self.replace_last {
            None => points.push((x, y)),
            Some((lx, ly)) => {
                let (dx, dy) = (x - lx, y - ly);
                let distance = (dx * dx + dy * dy).sqrt();
                if distance < 1e-6 {
                    return Rect::default();
                }
                let mut travelled = step;
                while travelled <= distance {
                    let t = travelled / distance;
                    points.push((lx + dx * t, ly + dy * t));
                    travelled += step;
                }
                if points.is_empty() {
                    // Too short a move to warrant a dab; wait for the next one
                    // rather than bunching dabs up at the start.
                    return Rect::default();
                }
            }
        }

        let selection = if self.selection.is_empty() {
            None
        } else {
            Some(self.selection.clone())
        };

        let mut dirty = Rect::default();
        let (Some(replacer), Some(layer)) = (self.replacer.as_mut(), self.stack.by_id_mut(id))
        else {
            return Rect::default();
        };

        for (px, py) in points {
            // The replacer works in the layer's own coordinates.
            let touched = replacer.apply_dab(
                &mut layer.pixels,
                brush,
                px - offset.0 as f32,
                py - offset.1 as f32,
                pressure,
                replacement,
            );
            if !touched.is_empty() {
                dirty = dirty.union(&Rect::new(
                    touched.x + offset.0,
                    touched.y + offset.1,
                    touched.width,
                    touched.height,
                ));
            }
        }

        // A marquee confines this exactly as it confines painting. Applied after
        // the fact by restoring what fell outside, which keeps the replacer's own
        // logic free of selection handling.
        if let Some(sel) = selection.as_ref() {
            if let (Some(base), Some(layer)) =
                (self.stroke_undo_base.as_ref(), self.stack.by_id_mut(id))
            {
                if let Some(original) = base.by_id(id) {
                    for y in dirty.y..dirty.bottom() {
                        for x in dirty.x..dirty.right() {
                            if sel.coverage_at(x, y) <= 0.0 {
                                let (lx, ly) = (x - offset.0, y - offset.1);
                                layer.pixels.set(lx, ly, original.pixels.get(lx, ly));
                            }
                        }
                    }
                }
            }
        }

        self.replace_last = Some((x, y));
        dirty
    }

    /// Finish a Color Replacement stroke, recording it as one undo step.
    pub fn end_replace(&mut self) -> bool {
        if self.replacer.take().is_none() {
            return false;
        }
        self.replace_last = None;
        self.stroke_undo_base = None;
        self.commit("Color Replacement Tool");
        true
    }

    /// Abandon a Color Replacement stroke, restoring what it changed.
    pub fn cancel_replace(&mut self) {
        self.replacer = None;
        self.replace_last = None;
        if let Some(base) = self.stroke_undo_base.take() {
            self.stack = base;
        }
    }

    /// Begin a Mixer Brush stroke.
    ///
    /// `reservoir` is the paint on the brush. Returns false if the layer cannot
    /// be painted.
    pub fn begin_mixer(
        &mut self,
        brush: &Brush,
        options: MixerOptions,
        reservoir: Rgba8,
        x: f32,
        y: f32,
        pressure: f32,
    ) -> bool {
        let Some(layer) = self.active_layer() else {
            return false;
        };
        if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
            return false;
        }

        // The transparency lock is the layer's, not the tool's, so it is folded
        // in here rather than being another thing the shell has to remember to
        // send.
        let options = MixerOptions { preserve_alpha: layer.lock_transparency, ..options };
        self.stroke_undo_base = Some(self.stack.clone());
        self.mixer = Some(MixerBrush::new(options, reservoir));
        self.mixer_last = None;
        self.extend_mixer(brush, x, y, pressure);
        true
    }

    /// Continue a Mixer Brush stroke, laying dabs to `(x, y)`.
    pub fn extend_mixer(&mut self, brush: &Brush, x: f32, y: f32, pressure: f32) -> Rect {
        if self.mixer.is_none() {
            return Rect::default();
        }
        let id = self.active_layer;
        let offset = match self.stack.by_id(id) {
            Some(layer) => layer.offset,
            None => return Rect::default(),
        };

        // Dab positions along the segment, spaced as the brush asks.
        let step = (brush.size * brush.spacing.max(0.01)).max(0.5);
        let mut points = Vec::new();
        match self.mixer_last {
            None => points.push((x, y)),
            Some((lx, ly)) => {
                let (dx, dy) = (x - lx, y - ly);
                let distance = (dx * dx + dy * dy).sqrt();
                if distance < 1e-6 {
                    return Rect::default();
                }
                let mut travelled = step;
                while travelled <= distance {
                    let t = travelled / distance;
                    points.push((lx + dx * t, ly + dy * t));
                    travelled += step;
                }
                if points.is_empty() {
                    // Too short a move to warrant a dab; wait for the next one
                    // rather than bunching dabs up at the start.
                    return Rect::default();
                }
            }
        }

        let selection = if self.selection.is_empty() {
            None
        } else {
            Some(self.selection.clone())
        };

        let sample_all = self
            .mixer
            .as_ref()
            .is_some_and(|mixer| mixer.options().sample_all_layers);
        let radius = brush.radius() * pressure.clamp(0.05, 1.0);
        let mut dirty = Rect::default();

        for (px, py) in points {
            // Sample All Layers picks the colour up from the composite. Only the
            // dab's own neighbourhood is composited, and it is recomposited per
            // dab, so a wet brush picks up its own deposits as it travels — the
            // same as when it reads the layer directly.
            let sampled = if sample_all {
                let area = Rect::new(
                    (px - radius - 1.0).floor() as i32,
                    (py - radius - 1.0).floor() as i32,
                    (radius * 2.0 + 3.0) as u32,
                    (radius * 2.0 + 3.0) as u32,
                )
                .intersect(&Rect::from_size(self.width, self.height));
                if area.is_empty() {
                    None
                } else {
                    Some((self.composite_region(area), area))
                }
            } else {
                None
            };

            let (Some(mixer), Some(layer)) = (self.mixer.as_mut(), self.stack.by_id_mut(id)) else {
                return dirty;
            };
            // The mixer works in the layer's own coordinates.
            let touched = mixer.apply_dab(
                &mut layer.pixels,
                sampled.as_ref().map(|(pixels, area)| Sampled {
                    pixels,
                    origin: (area.x - offset.0, area.y - offset.1),
                }),
                brush,
                px - offset.0 as f32,
                py - offset.1 as f32,
                pressure,
            );
            if !touched.is_empty() {
                dirty = dirty.union(&Rect::new(
                    touched.x + offset.0,
                    touched.y + offset.1,
                    touched.width,
                    touched.height,
                ));
            }
        }

        // A marquee confines this exactly as it confines painting, and by the
        // same after-the-fact restore the replacer uses.
        if let Some(sel) = selection.as_ref() {
            if let (Some(base), Some(layer)) =
                (self.stroke_undo_base.as_ref(), self.stack.by_id_mut(id))
            {
                if let Some(original) = base.by_id(id) {
                    for y in dirty.y..dirty.bottom() {
                        for x in dirty.x..dirty.right() {
                            if sel.coverage_at(x, y) <= 0.0 {
                                let (lx, ly) = (x - offset.0, y - offset.1);
                                layer.pixels.set(lx, ly, original.pixels.get(lx, ly));
                            }
                        }
                    }
                }
            }
        }

        self.mixer_last = Some((x, y));
        dirty
    }

    /// Begin a Blur or Sharpen stroke.
    ///
    /// Returns false if the active layer cannot be painted on.
    pub fn begin_focus(
        &mut self,
        brush: &Brush,
        options: FocusOptions,
        x: f32,
        y: f32,
        pressure: f32,
    ) -> bool {
        let Some(layer) = self.active_layer() else {
            return false;
        };
        if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
            return false;
        }

        // The transparency lock belongs to the layer, not the options bar.
        let options = FocusOptions { preserve_alpha: layer.lock_transparency, ..options };
        self.stroke_undo_base = Some(self.stack.clone());
        self.focus = Some(options);
        self.retouch_last = None;
        self.extend_retouch(brush, x, y, pressure);
        true
    }

    /// Begin a Smudge stroke. `paint` is the foreground colour, used only when
    /// Finger Painting is on.
    pub fn begin_smudge(
        &mut self,
        brush: &Brush,
        options: SmudgeOptions,
        paint: Rgba8,
        x: f32,
        y: f32,
        pressure: f32,
    ) -> bool {
        let Some(layer) = self.active_layer() else {
            return false;
        };
        if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
            return false;
        }

        let options = SmudgeOptions { preserve_alpha: layer.lock_transparency, ..options };
        self.stroke_undo_base = Some(self.stack.clone());
        self.smudge = Some(Smudge::new(options, paint));
        self.retouch_last = None;
        self.extend_retouch(brush, x, y, pressure);
        true
    }

    /// Begin a Dodge, Burn or Sponge stroke.
    ///
    /// Returns false if the active layer cannot be painted on.
    pub fn begin_tone(
        &mut self,
        brush: &Brush,
        options: ToneOptions,
        x: f32,
        y: f32,
        pressure: f32,
    ) -> bool {
        let Some(layer) = self.active_layer() else {
            return false;
        };
        if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
            return false;
        }

        let options = ToneOptions { preserve_alpha: layer.lock_transparency, ..options };
        let (w, h) = (layer.pixels.width(), layer.pixels.height());
        self.stroke_undo_base = Some(self.stack.clone());
        self.tone = Some(ToneStroke::new(options, w, h));
        self.retouch_last = None;
        self.extend_retouch(brush, x, y, pressure);
        true
    }

    /// Continue a retouch stroke, laying dabs to `(x, y)`.
    ///
    /// The six tools that work on what is already under the brush — Blur,
    /// Sharpen, Smudge, Dodge, Burn and Sponge — share this: the same spacing,
    /// the same per-dab application to the layer, the same marquee restore. Only
    /// what a dab *does* differs, and that is the one branch inside the loop.
    pub fn extend_retouch(&mut self, brush: &Brush, x: f32, y: f32, pressure: f32) -> Rect {
        let sample_all = match (&self.focus, &self.smudge, &self.tone) {
            (Some(options), ..) => options.sample_all_layers,
            (_, Some(smudge), _) => smudge.options().sample_all_layers,
            // The toning tools have no Sample All Layers in CS6: they read one
            // pixel's own tone, and there is nothing a lower layer could add.
            (.., Some(_)) => false,
            _ => return Rect::default(),
        };
        let id = self.active_layer;
        let offset = match self.stack.by_id(id) {
            Some(layer) => layer.offset,
            None => return Rect::default(),
        };

        // Dab positions along the segment, spaced as the brush asks — except for
        // the toning tools, which take their effect from the *maximum* coverage a
        // pixel reaches rather than from each dab in turn. For them spacing
        // decides only how finely that envelope is sampled, not how strong the
        // result is, and a quarter of a brush width samples it coarsely enough to
        // leave a visible ripple between dab centres. Sampling finer costs a
        // little time and removes it.
        let spacing = match self.tone {
            Some(_) => brush.spacing.min(0.08),
            None => brush.spacing,
        };
        let step = (brush.size * spacing.max(0.01)).max(0.5);
        let mut points = Vec::new();
        match self.retouch_last {
            None => points.push((x, y)),
            Some((lx, ly)) => {
                let (dx, dy) = (x - lx, y - ly);
                let distance = (dx * dx + dy * dy).sqrt();
                if distance < 1e-6 {
                    return Rect::default();
                }
                let mut travelled = step;
                while travelled <= distance {
                    let t = travelled / distance;
                    points.push((lx + dx * t, ly + dy * t));
                    travelled += step;
                }
                if points.is_empty() {
                    // Too short a move to warrant a dab; wait for the next one
                    // rather than bunching dabs up at the start.
                    return Rect::default();
                }
            }
        }

        let selection = if self.selection.is_empty() {
            None
        } else {
            Some(self.selection.clone())
        };
        let radius = brush.radius() * pressure.clamp(0.05, 1.0);
        let mut dirty = Rect::default();

        for (px, py) in points {
            // Sample All Layers reads the neighbourhood from the composite, and
            // recomposites per dab so a stroke sees its own softening as it goes
            // — the same as when it reads the layer directly.
            let sampled = if sample_all {
                let area = Rect::new(
                    (px - radius - 2.0).floor() as i32,
                    (py - radius - 2.0).floor() as i32,
                    (radius * 2.0 + 5.0) as u32,
                    (radius * 2.0 + 5.0) as u32,
                )
                .intersect(&Rect::from_size(self.width, self.height));
                if area.is_empty() {
                    None
                } else {
                    Some((self.composite_region(area), area))
                }
            } else {
                None
            };

            let Some(layer) = self.stack.by_id_mut(id) else {
                return dirty;
            };
            let source = sampled
                .as_ref()
                .map(|(pixels, area)| (pixels, (area.x - offset.0, area.y - offset.1)));
            let (lx, ly) = (px - offset.0 as f32, py - offset.1 as f32);

            let touched = match (self.focus.as_ref(), self.smudge.as_mut(), self.tone.as_mut())
            {
                (Some(options), ..) => {
                    focus::apply_dab(&mut layer.pixels, source, brush, lx, ly, pressure, options)
                }
                (_, Some(smudge), _) => {
                    smudge.apply_dab(&mut layer.pixels, source, brush, lx, ly, pressure)
                }
                (.., Some(tone)) => tone.apply_dab(&mut layer.pixels, brush, lx, ly, pressure),
                _ => Rect::default(),
            };
            if !touched.is_empty() {
                dirty = dirty.union(&Rect::new(
                    touched.x + offset.0,
                    touched.y + offset.1,
                    touched.width,
                    touched.height,
                ));
            }
        }

        // A marquee confines this as it confines painting, by the same
        // after-the-fact restore the replacer and the mixer use.
        if let Some(sel) = selection.as_ref() {
            if let (Some(base), Some(layer)) =
                (self.stroke_undo_base.as_ref(), self.stack.by_id_mut(id))
            {
                if let Some(original) = base.by_id(id) {
                    for y in dirty.y..dirty.bottom() {
                        for x in dirty.x..dirty.right() {
                            if sel.coverage_at(x, y) <= 0.0 {
                                let (lx, ly) = (x - offset.0, y - offset.1);
                                layer.pixels.set(lx, ly, original.pixels.get(lx, ly));
                            }
                        }
                    }
                }
            }
        }

        self.retouch_last = Some((x, y));
        dirty
    }

    /// Finish a retouch stroke, recording it as one undo step under the name of
    /// the tool that made it.
    pub fn end_retouch(&mut self) -> bool {
        let name = match (self.focus.take(), self.smudge.take(), self.tone.take()) {
            (Some(options), ..) => match options.focus {
                crate::focus::FocusMode::Blur => "Blur Tool",
                crate::focus::FocusMode::Sharpen => "Sharpen Tool",
            },
            (_, Some(_), _) => "Smudge Tool",
            (.., Some(tone)) => match tone.options().tool {
                crate::tone::ToneTool::Dodge => "Dodge Tool",
                crate::tone::ToneTool::Burn => "Burn Tool",
                crate::tone::ToneTool::Sponge => "Sponge Tool",
            },
            _ => return false,
        };
        self.retouch_last = None;
        self.stroke_undo_base = None;
        self.commit(name);
        true
    }

    /// Abandon one, restoring what it changed.
    pub fn cancel_retouch(&mut self) {
        self.focus = None;
        self.smudge = None;
        self.tone = None;
        self.retouch_last = None;
        if let Some(base) = self.stroke_undo_base.take() {
            self.stack = base;
        }
    }

    /// Finish a Mixer Brush stroke, recording it as one undo step.
    ///
    /// Returns the paint left on the brush, which the next stroke starts from
    /// unless the shell cleans or reloads it — the reservoir outlives the stroke
    /// in Photoshop too.
    pub fn end_mixer(&mut self) -> Option<Rgba8> {
        let mixer = self.mixer.take()?;
        self.mixer_last = None;
        self.stroke_undo_base = None;
        self.commit("Mixer Brush Tool");
        Some(mixer.reservoir())
    }

    /// Abandon a Mixer Brush stroke, restoring what it changed.
    pub fn cancel_mixer(&mut self) {
        self.mixer = None;
        self.mixer_last = None;
        if let Some(base) = self.stroke_undo_base.take() {
            self.stack = base;
        }
    }

    /// Abandon the in-progress stroke without applying it.
    pub fn cancel_stroke(&mut self) {
        self.stroke = None;
        self.clone = None;
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

    /// Draw a gradient over the active layer — the Gradient tool.
    ///
    /// `start` and `end` are the drag, in document space. The gradient covers
    /// the whole layer (or the whole selection): the ends of the ramp extend
    /// beyond the drag rather than stopping at it, which is what Photoshop does.
    pub fn draw_gradient(
        &mut self,
        ramp: &Gradient,
        options: &GradientOptions,
        start: (f32, f32),
        end: (f32, f32),
    ) -> Rect {
        let selection = if self.selection.is_empty() {
            None
        } else {
            Some(self.selection.clone())
        };

        let id = self.active_layer;
        let dirty = {
            let Some(layer) = self.stack.by_id_mut(id) else {
                return Rect::default();
            };
            if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
                return Rect::default();
            }
            let offset = layer.offset;
            // The transparency lock is the layer's business, not the options
            // bar's, so it is folded in here.
            let options = GradientOptions { preserve_alpha: layer.lock_transparency, ..*options };
            // The ramp is described in document space, so shift the drag into the
            // layer's own frame rather than asking the renderer to know about
            // offsets.
            let local_start = (start.0 - offset.0 as f32, start.1 - offset.1 as f32);
            let local_end = (end.0 - offset.0 as f32, end.1 - offset.1 as f32);

            let touched = gradient::draw(
                &mut layer.pixels,
                ramp,
                &options,
                local_start,
                local_end,
                offset,
                selection.as_ref(),
            );
            if touched.is_empty() {
                return Rect::default();
            }
            Rect::new(touched.x + offset.0, touched.y + offset.1, touched.width, touched.height)
        };

        self.commit("Gradient");
        dirty
    }

    /// Flood-fill from a clicked point — the Paint Bucket.
    ///
    /// `seed` is in document space. What matches is decided by the Magic Wand's
    /// own flood, so Tolerance, Contiguous and Anti-alias mean exactly what they
    /// mean for the wand. With **All Layers** the matching reads the composite;
    /// the fill lands on the active layer either way.
    pub fn fill_bucket(
        &mut self,
        seed: (i32, i32),
        options: &BucketOptions,
        color: Rgba8,
    ) -> Rect {
        let Some(layer) = self.active_layer() else {
            return Rect::default();
        };
        if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
            return Rect::default();
        }
        let offset = layer.offset;

        // The mask is built in whichever frame it is sampled from, and carries
        // its origin in the layer's coordinates so the fill needs to know nothing
        // about which mode was used.
        let (coverage, mask_size, mask_origin) = if options.all_layers {
            let composite = self.composite();
            let (w, h) = (composite.width(), composite.height());
            let mask = wand::magic_wand(
                &composite,
                seed,
                options.tolerance,
                options.contiguous,
                options.antialias,
            );
            (mask, (w, h), (-offset.0, -offset.1))
        } else {
            let local = (seed.0 - offset.0, seed.1 - offset.1);
            let pixels = &self.active_layer().unwrap().pixels;
            let (w, h) = (pixels.width(), pixels.height());
            let mask = wand::magic_wand(
                pixels,
                local,
                options.tolerance,
                options.contiguous,
                options.antialias,
            );
            (mask, (w, h), (0, 0))
        };

        let selection = if self.selection.is_empty() {
            None
        } else {
            Some(self.selection.clone())
        };

        let id = self.active_layer;
        let dirty = {
            let Some(layer) = self.stack.by_id_mut(id) else {
                return Rect::default();
            };
            // The transparency lock is the layer's, not the options bar's.
            let options = BucketOptions { preserve_alpha: layer.lock_transparency, ..*options };
            let mask = FloodMask {
                coverage: &coverage,
                width: mask_size.0,
                height: mask_size.1,
                origin: mask_origin,
            };
            let touched = bucket::fill(
                &mut layer.pixels,
                &mask,
                color,
                &options,
                offset,
                selection.as_ref(),
            );
            if touched.is_empty() {
                return Rect::default();
            }
            Rect::new(touched.x + offset.0, touched.y + offset.1, touched.width, touched.height)
        };

        self.commit("Paint Bucket");
        dirty
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

    // -- paths ------------------------------------------------------------

    pub fn paths(&self) -> &PathSet {
        &self.paths
    }

    pub fn paths_mut(&mut self) -> &mut PathSet {
        &mut self.paths
    }

    /// How finely a path is flattened before it becomes pixels — fine enough
    /// that no curve visibly facets at any zoom level a selection or a fill
    /// edge is actually inspected at.
    const PATH_FLATTEN_TOLERANCE: f32 = 0.35;

    /// Turn the active path into a selection — the Paths panel's "Make
    /// Selection". An open subpath is closed for this purpose, the same way
    /// Photoshop treats one: a selection has to enclose an area, so where the
    /// pen was lifted is implicitly joined back to where it started.
    ///
    /// Several subpaths combine under nonzero winding, so one wound the
    /// opposite way from the rest cuts a hole rather than adding a second
    /// region — see [`crate::selection::Selection::apply_polygons_feathered`].
    pub fn select_from_active_path(&mut self, op: SelectionOp, feather: u32) -> bool {
        let Some(path) = self.paths.active() else { return false };
        let contours: Vec<Vec<(f32, f32)>> = path
            .flatten(Self::PATH_FLATTEN_TOLERANCE)
            .into_iter()
            .map(|(points, _closed)| points)
            .collect();
        if contours.iter().all(|c| c.len() < 3) {
            return false;
        }
        self.selection.apply_polygons_feathered(&contours, op, feather);
        true
    }

    /// Add a subpath fitted to a freehand drag — the Freeform Pen tool.
    /// `points` is the raw mouse trail in document space; it is simplified to a
    /// handful of corner anchors before being appended (see
    /// [`crate::path::simplify_freehand`]). Creates the Work Path if none is
    /// active yet, the same as drawing with the ordinary Pen tool would.
    pub fn add_freeform_subpath(&mut self, points: &[(f32, f32)], tolerance: f32, close: bool) -> bool {
        let simplified = crate::path::simplify_freehand(points, tolerance);
        if simplified.len() < 2 {
            return false;
        }
        let path = self.paths.ensure_active();
        for &(x, y) in &simplified {
            path.append_corner(x, y);
        }
        if close {
            path.close_active_subpath();
        } else {
            path.finish_editing();
        }
        true
    }

    /// Fill the active path with a colour — the Paths panel's "Fill Path".
    /// Unlike [`Document::fill`] this ignores the current selection entirely:
    /// the path *is* the region, exactly as Photoshop's own command works.
    pub fn fill_active_path(&mut self, color: Rgba8, opacity: f32) -> Rect {
        let Some(path) = self.paths.active() else { return Rect::default() };
        let contours: Vec<Vec<(f32, f32)>> = path
            .flatten(Self::PATH_FLATTEN_TOLERANCE)
            .into_iter()
            .map(|(points, _closed)| points)
            .collect();
        if contours.iter().all(|c| c.len() < 3) {
            return Rect::default();
        }

        let mut coverage = Selection::new(self.width, self.height);
        coverage.apply_polygons_feathered(&contours, SelectionOp::Replace, 0);

        let id = self.active_layer;
        let dirty = if let Some(layer) = self.stack.by_id_mut(id) {
            if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
                return Rect::default();
            }
            let offset = layer.offset;
            let lock_alpha = layer.lock_transparency;
            let (w, h) = (layer.pixels.width(), layer.pixels.height());
            let mut touched = Rect::default();

            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let mut alpha = coverage.coverage_at(x + offset.0, y + offset.1) * opacity;
                    if alpha <= 0.0 {
                        continue;
                    }
                    let dst = layer.pixels.get(x, y);
                    if lock_alpha {
                        if dst.a == 0 {
                            continue;
                        }
                        alpha *= dst.a as f32 / 255.0;
                    }
                    let out = crate::brush::source_over(dst, color, alpha);
                    if out != dst {
                        layer.pixels.set(x, y, out);
                        touched = touched.union(&Rect::new(x, y, 1, 1));
                    }
                }
            }
            // Document space, matching every other pixel-editing call here —
            // `touched` was accumulated in the layer's own coordinates.
            Rect::new(touched.x + offset.0, touched.y + offset.1, touched.width, touched.height)
        } else {
            Rect::default()
        };

        if dirty.is_empty() {
            return Rect::default();
        }
        self.commit("Fill Path");
        dirty
    }

    /// Stroke the active path with a brush — the Paths panel's "Stroke Path".
    /// Each subpath is stroked independently (the pen lifts between them, so
    /// two separate loops do not get joined by a straight line), and a closed
    /// subpath's stroke returns all the way to its start.
    pub fn stroke_active_path(&mut self, brush: &Brush, color: Rgba8, opacity: f32) -> Rect {
        let Some(path) = self.paths.active() else { return Rect::default() };
        let flat = path.flatten(Self::PATH_FLATTEN_TOLERANCE);
        if flat.iter().all(|(points, _)| points.len() < 2) {
            return Rect::default();
        }

        let id = self.active_layer;
        let (offset, ..) = match self.stack.by_id(id) {
            Some(layer) if !layer.lock_pixels && matches!(layer.kind, LayerKind::Raster) => {
                (layer.offset, layer.pixels.width(), layer.pixels.height())
            }
            _ => return Rect::default(),
        };

        // The stroke mask works in document space, exactly as an interactive
        // brush stroke does (`begin_stroke` passes the cursor's document
        // coordinates straight through) — `composite_onto` below is what
        // converts into the layer's own frame.
        let mut mask = StrokeMask::new(self.width, self.height);
        for (points, closed) in &flat {
            if points.len() < 2 {
                continue;
            }
            let (x0, y0) = points[0];
            mask.begin(brush, x0, y0, 1.0);
            for &(x, y) in &points[1..] {
                mask.extend(brush, x, y, 1.0);
            }
            if *closed {
                mask.extend(brush, x0, y0, 1.0);
            }
        }

        let selection_empty = self.selection.is_empty();
        let selection = if selection_empty { None } else { Some(self.selection.clone()) };
        let dirty = if let Some(layer) = self.stack.by_id_mut(id) {
            let lock = layer.lock_transparency;
            mask.composite_onto(&mut layer.pixels, color, opacity, offset, selection.as_ref(), lock)
        } else {
            Rect::default()
        };

        if dirty.is_empty() {
            return Rect::default();
        }
        self.commit("Stroke Path");
        dirty
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
        // The restored stack carries its own visibility flags, so the one an
        // open type edit was holding on to no longer means anything.
        self.text_edit = None;

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
        self.text_edit = None;

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
    use crate::layer::{TextAlign, TextRun};

    fn doc() -> Document {
        Document::new(16, 16, Rgba8::WHITE)
    }

    #[test]
    fn make_selection_from_a_square_path() {
        let mut d = Document::new(40, 40, Rgba8::WHITE);
        {
            let path = d.paths_mut().ensure_active();
            path.append_corner(10.0, 10.0);
            path.append_corner(30.0, 10.0);
            path.append_corner(30.0, 30.0);
            path.append_corner(10.0, 30.0);
            path.close_active_subpath();
        }
        assert!(d.select_from_active_path(SelectionOp::Replace, 0));
        assert!(d.selection().coverage_at(20, 20) > 0.9, "the inside was not selected");
        assert_eq!(d.selection().coverage_at(2, 2), 0.0, "the outside was selected");
    }

    #[test]
    fn make_selection_closes_an_open_subpath() {
        // A selection has to enclose an area, so an unclosed subpath is treated
        // as if it had been closed back to its start.
        let mut d = Document::new(40, 40, Rgba8::WHITE);
        {
            let path = d.paths_mut().ensure_active();
            path.append_corner(10.0, 10.0);
            path.append_corner(30.0, 10.0);
            path.append_corner(30.0, 30.0);
            path.append_corner(10.0, 30.0);
            path.finish_editing(); // left open, not closed
        }
        assert!(d.select_from_active_path(SelectionOp::Replace, 0));
        assert!(d.selection().coverage_at(20, 20) > 0.9, "an open path did not enclose its area");
    }

    #[test]
    fn make_selection_with_no_active_path_does_nothing() {
        let mut d = doc();
        assert!(!d.select_from_active_path(SelectionOp::Replace, 0));
        assert!(d.selection().is_empty());
    }

    #[test]
    fn freeform_subpath_creates_a_work_path_and_simplifies() {
        let mut d = doc();
        let points: Vec<(f32, f32)> = (0..=40).map(|i| (i as f32, 0.0)).collect();
        assert!(d.add_freeform_subpath(&points, 1.0, false));
        assert_eq!(d.paths().len(), 1);
        assert_eq!(d.paths().entries()[0].name, "Work Path");
        let subpath = &d.paths().active().unwrap().subpaths[0];
        assert!(subpath.points.len() < points.len(), "the drag was not simplified");
        assert_eq!(subpath.points.first().unwrap().anchor, (0.0, 0.0));
        assert_eq!(subpath.points.last().unwrap().anchor, (40.0, 0.0));
    }

    #[test]
    fn fill_path_paints_only_the_enclosed_area() {
        let mut d = Document::new(40, 40, Rgba8::WHITE);
        {
            let path = d.paths_mut().ensure_active();
            path.append_corner(10.0, 10.0);
            path.append_corner(30.0, 10.0);
            path.append_corner(30.0, 30.0);
            path.append_corner(10.0, 30.0);
            path.close_active_subpath();
        }
        let dirty = d.fill_active_path(Rgba8::BLACK, 1.0);
        assert!(!dirty.is_empty());
        assert_eq!(d.composite().get(20, 20), Rgba8::BLACK);
        assert_eq!(d.composite().get(2, 2), Rgba8::WHITE, "the fill leaked outside the path");
    }

    #[test]
    fn fill_path_ignores_the_current_selection() {
        // Fill Path fills the path's own area; unlike Edit > Fill it does not
        // stop at whatever the marquee happens to be doing.
        let mut d = Document::new(40, 40, Rgba8::WHITE);
        d.select_rect(Rect::new(0, 0, 5, 5), SelectionOp::Replace, 0);
        {
            let path = d.paths_mut().ensure_active();
            path.append_corner(10.0, 10.0);
            path.append_corner(30.0, 10.0);
            path.append_corner(30.0, 30.0);
            path.append_corner(10.0, 30.0);
            path.close_active_subpath();
        }
        d.fill_active_path(Rgba8::BLACK, 1.0);
        assert_eq!(d.composite().get(20, 20), Rgba8::BLACK, "the fill was clipped to the marquee");
    }

    #[test]
    fn fill_path_is_one_undo_step() {
        let mut d = Document::new(40, 40, Rgba8::WHITE);
        {
            let path = d.paths_mut().ensure_active();
            path.append_corner(10.0, 10.0);
            path.append_corner(30.0, 10.0);
            path.append_corner(30.0, 30.0);
            path.append_corner(10.0, 30.0);
            path.close_active_subpath();
        }
        d.fill_active_path(Rgba8::BLACK, 1.0);
        assert_eq!(d.composite().get(20, 20), Rgba8::BLACK);
        assert!(d.undo());
        assert_eq!(d.composite().get(20, 20), Rgba8::WHITE);
    }

    #[test]
    fn a_hole_wound_the_other_way_is_not_filled() {
        let mut d = Document::new(60, 60, Rgba8::WHITE);
        {
            let path = d.paths_mut().ensure_active();
            path.append_corner(10.0, 10.0);
            path.append_corner(50.0, 10.0);
            path.append_corner(50.0, 50.0);
            path.append_corner(10.0, 50.0);
            path.close_active_subpath();
            // Wound the opposite way, so it cuts a hole instead of adding a
            // second filled region.
            path.append_corner(20.0, 20.0);
            path.append_corner(20.0, 40.0);
            path.append_corner(40.0, 40.0);
            path.append_corner(40.0, 20.0);
            path.close_active_subpath();
        }
        d.fill_active_path(Rgba8::BLACK, 1.0);
        assert_eq!(d.composite().get(15, 15), Rgba8::BLACK, "the ring was not filled");
        assert_eq!(d.composite().get(30, 30), Rgba8::WHITE, "the hole was filled in");
    }

    #[test]
    fn stroke_path_paints_along_the_outline_and_nowhere_else() {
        let mut d = Document::new(60, 60, Rgba8::WHITE);
        {
            let path = d.paths_mut().ensure_active();
            path.append_corner(10.0, 30.0);
            path.append_corner(50.0, 30.0);
            path.finish_editing();
        }
        let brush = Brush { size: 6.0, hardness: 1.0, ..Brush::default() };
        let dirty = d.stroke_active_path(&brush, Rgba8::BLACK, 1.0);
        assert!(!dirty.is_empty());
        assert_eq!(d.composite().get(30, 30), Rgba8::BLACK, "the stroke missed the path");
        assert_eq!(d.composite().get(30, 3), Rgba8::WHITE, "the stroke painted off the path");
    }

    #[test]
    fn stroke_path_closes_a_closed_subpath() {
        // The stroke has to reach every edge of a closed shape, including the
        // one that only exists because it is closed — the segment back to the
        // start.
        let mut d = Document::new(60, 60, Rgba8::WHITE);
        {
            let path = d.paths_mut().ensure_active();
            path.append_corner(15.0, 15.0);
            path.append_corner(45.0, 15.0);
            path.append_corner(45.0, 45.0);
            path.append_corner(15.0, 45.0);
            path.close_active_subpath();
        }
        let brush = Brush { size: 6.0, hardness: 1.0, ..Brush::default() };
        d.stroke_active_path(&brush, Rgba8::BLACK, 1.0);
        // The left edge, from (15,45) back to (15,15) — only present because
        // the subpath is closed.
        assert_eq!(d.composite().get(15, 30), Rgba8::BLACK, "the closing edge was not stroked");
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
    fn a_replacement_stroke_recolours_from_the_first_dab() {
        // The bug this guards: begin_replace used to apply its opening dab with
        // the *reference* colour rather than the replacement. In Color mode a
        // grey reference is a no-op, so those pixels were marked done and the
        // colour the user picked never reached them.
        let mut d = Document::new(60, 40, Rgba8::new(120, 120, 120, 255));
        d.commit("Setup");

        let brush = Brush { size: 30.0, hardness: 1.0, ..Brush::default() };
        let options = ReplaceOptions {
            mode: crate::replace::ReplaceMode::Color,
            sampling: ReplaceSampling::Continuous,
            limits: crate::replace::ReplaceLimits::Discontiguous,
            tolerance: 100,
            antialias: false,
        };
        let red = Rgba8::new(220, 30, 30, 255);
        assert!(d.begin_replace(&brush, options, None, red, 30.0, 20.0, 1.0));
        d.end_replace();

        let px = d.composite().get(30, 20);
        assert!(px.r > px.g + 20, "the very first dab did not recolour: {:?}", px);
    }

    #[test]
    fn a_replacement_stroke_is_one_undo_step() {
        let mut d = Document::new(60, 40, Rgba8::new(120, 120, 120, 255));
        d.commit("Setup");
        let before = d.composite().get(30, 20);

        let brush = Brush { size: 24.0, hardness: 1.0, ..Brush::default() };
        let options = ReplaceOptions { tolerance: 100, ..ReplaceOptions::default() };
        d.begin_replace(&brush, options, None, Rgba8::new(30, 30, 220, 255), 20.0, 20.0, 1.0);
        d.extend_replace(&brush, 30.0, 20.0, 1.0, Rgba8::new(30, 30, 220, 255));
        d.extend_replace(&brush, 40.0, 20.0, 1.0, Rgba8::new(30, 30, 220, 255));
        d.end_replace();
        assert_ne!(d.composite().get(30, 20), before);

        assert!(d.undo(), "nothing to undo");
        assert_eq!(d.composite().get(30, 20), before, "one undo did not restore the stroke");
    }

    #[test]
    fn cancelling_a_replacement_stroke_restores_the_layer() {
        let mut d = Document::new(40, 40, Rgba8::new(120, 120, 120, 255));
        d.commit("Setup");
        let before = d.composite().get(20, 20);

        let brush = Brush { size: 20.0, hardness: 1.0, ..Brush::default() };
        let options = ReplaceOptions { tolerance: 100, ..ReplaceOptions::default() };
        d.begin_replace(&brush, options, None, Rgba8::new(30, 220, 30, 255), 20.0, 20.0, 1.0);
        assert_ne!(d.composite().get(20, 20), before, "the stroke did nothing to cancel");

        d.cancel_replace();
        assert_eq!(d.composite().get(20, 20), before, "cancel left the change behind");
    }

    #[test]
    fn the_paint_bucket_fills_the_region_it_was_clicked_in() {
        let mut d = Document::new(40, 20, Rgba8::WHITE);
        if let Some(l) = d.active_layer_mut() {
            l.pixels.fill_rect(Rect::new(19, 0, 2, 20), Rgba8::BLACK);
        }
        d.commit("Setup");

        let options = crate::bucket::BucketOptions {
            antialias: false,
            ..crate::bucket::BucketOptions::default()
        };
        let red = Rgba8::opaque(220, 0, 0);
        assert!(!d.fill_bucket((5, 10), &options, red).is_empty());
        assert_eq!(d.composite().get(5, 10), red);
        assert_eq!(d.composite().get(30, 10), Rgba8::WHITE, "the fill crossed the wall");

        assert!(d.undo(), "the fill was not one undo step");
        assert_eq!(d.composite().get(5, 10), Rgba8::WHITE);
    }

    #[test]
    fn the_paint_bucket_can_match_on_every_layer_at_once() {
        // All Layers decides what matches from the composite — so a boundary that
        // only exists on the layer below still stops the fill — while the paint
        // lands on the active layer.
        let mut d = Document::new(40, 20, Rgba8::WHITE);
        if let Some(background) = d.active_layer_mut() {
            background.pixels.fill_rect(Rect::new(19, 0, 2, 20), Rgba8::BLACK);
        }
        let upper = d.add_layer(None);
        d.set_active_layer(upper);
        d.commit("Setup");

        let options = crate::bucket::BucketOptions {
            antialias: false,
            all_layers: true,
            ..crate::bucket::BucketOptions::default()
        };
        let red = Rgba8::opaque(220, 0, 0);
        assert!(!d.fill_bucket((5, 10), &options, red).is_empty());

        let painted = &d.active_layer().unwrap().pixels;
        assert_eq!(painted.get(5, 10), red, "nothing was filled on the active layer");
        assert_eq!(painted.get(30, 10).a, 0, "the fill crossed a wall it could see");
        // The layer it matched against is untouched.
        assert_eq!(d.layers().get(0).unwrap().pixels.get(5, 10), Rgba8::WHITE);
    }

    #[test]
    fn the_paint_bucket_is_refused_on_a_locked_layer() {
        let mut d = Document::new(20, 20, Rgba8::WHITE);
        d.active_layer_mut().unwrap().lock_pixels = true;
        let options = crate::bucket::BucketOptions::default();
        assert!(d.fill_bucket((10, 10), &options, Rgba8::BLACK).is_empty());
        assert_eq!(d.composite().get(10, 10), Rgba8::WHITE);
    }

    #[test]
    fn a_clone_stroke_copies_the_source_verbatim() {
        // The Clone Stamp's defining property, and what separates it from the
        // Healing Brush: the pixels land exactly as they were sampled.
        let mut d = Document::new(80, 40, Rgba8::WHITE);
        if let Some(layer) = d.active_layer_mut() {
            layer.pixels.fill_rect(Rect::new(0, 0, 20, 40), Rgba8::opaque(20, 40, 200));
        }
        d.commit("Setup");

        let brush = Brush { size: 20.0, hardness: 1.0, ..Brush::default() };
        // Sample 50px to the left of where the stroke paints.
        assert!(d.begin_clone_stroke(&brush, 60.0, 20.0, 1.0, (-50, 0),
                                     CloneSampling::CurrentLayer));
        d.end_clone_stroke(1.0);

        assert_eq!(d.composite().get(60, 20), Rgba8::opaque(20, 40, 200),
                   "the source colour was not copied exactly");
    }

    #[test]
    fn a_clone_stroke_is_one_undo_step() {
        let mut d = Document::new(80, 40, Rgba8::WHITE);
        if let Some(layer) = d.active_layer_mut() {
            layer.pixels.fill_rect(Rect::new(0, 0, 20, 40), Rgba8::BLACK);
        }
        d.commit("Setup");
        // Sampling 40px left of the stroke, so the paint here comes from inside
        // the black bar at x=10.
        let before = d.composite().get(50, 20);

        let brush = Brush { size: 20.0, hardness: 1.0, ..Brush::default() };
        d.begin_clone_stroke(&brush, 50.0, 20.0, 1.0, (-40, 0), CloneSampling::CurrentLayer);
        d.extend_stroke(&brush, 55.0, 20.0, 1.0);
        d.extend_stroke(&brush, 60.0, 20.0, 1.0);
        d.end_clone_stroke(1.0);
        assert_ne!(d.composite().get(50, 20), before);

        assert!(d.undo(), "nothing to undo");
        assert_eq!(d.composite().get(50, 20), before, "one undo did not restore the stroke");
    }

    #[test]
    fn a_clone_stroke_samples_the_state_it_began_in() {
        // With the source close behind the cursor, reading the layer live would
        // feed each dab the previous dab's output and smear the source along the
        // whole stroke. Sampling a snapshot copies it once, as Photoshop does.
        let mut d = Document::new(120, 20, Rgba8::WHITE);
        if let Some(layer) = d.active_layer_mut() {
            layer.pixels.fill_rect(Rect::new(0, 0, 10, 20), Rgba8::BLACK);
        }
        d.commit("Setup");

        let brush = Brush { size: 8.0, hardness: 1.0, ..Brush::default() };
        d.begin_clone_stroke(&brush, 14.0, 10.0, 1.0, (-10, 0), CloneSampling::CurrentLayer);
        for x in 15..100 {
            d.extend_stroke(&brush, x as f32, 10.0, 1.0);
        }
        d.end_clone_stroke(1.0);

        // The black bar is 10px wide, so cloning it 10px right reaches x≈19 and
        // no further. Anything past that must still be white.
        assert_eq!(d.composite().get(15, 10), Rgba8::BLACK, "the source was not cloned at all");
        assert_eq!(d.composite().get(60, 10), Rgba8::WHITE,
                   "the stroke smeared: it was reading its own output");
    }

    #[test]
    fn cloning_an_empty_layer_copies_nothing_but_all_layers_copies_what_is_visible() {
        // The confusing case, and CS6 behaves the same way: the material is on
        // one layer, the active layer is another, and Sample defaults to the
        // current layer — so there is genuinely nothing under the source point.
        let mut d = Document::new(80, 40, Rgba8::WHITE);
        if let Some(background) = d.active_layer_mut() {
            background.pixels.fill_rect(Rect::new(0, 0, 20, 40), Rgba8::BLACK);
        }
        let upper = d.add_layer(None);
        d.set_active_layer(upper);
        d.commit("Setup");

        let brush = Brush { size: 20.0, hardness: 1.0, ..Brush::default() };
        d.begin_clone_stroke(&brush, 50.0, 20.0, 1.0, (-40, 0), CloneSampling::CurrentLayer);
        d.end_clone_stroke(1.0);
        assert_eq!(d.active_layer().unwrap().pixels.get(50, 20).a, 0,
                   "an empty layer had something to clone from");

        d.begin_clone_stroke(&brush, 50.0, 20.0, 1.0, (-40, 0), CloneSampling::AllLayers);
        d.end_clone_stroke(1.0);
        assert_eq!(d.active_layer().unwrap().pixels.get(50, 20), Rgba8::BLACK,
                   "All Layers did not clone the black bar from the layer below");
        // The paint lands on the active layer, never on the one it sampled.
        assert_eq!(d.layers().get(0).unwrap().pixels.get(50, 20), Rgba8::WHITE,
                   "the sampled layer was written to");
    }

    #[test]
    fn a_clone_stroke_with_no_offset_is_refused() {
        // Sampling where it paints would copy every pixel onto itself, which is
        // the state before a source has been Alt-clicked.
        let mut d = Document::new(40, 40, Rgba8::WHITE);
        let brush = Brush::default();
        assert!(!d.begin_clone_stroke(&brush, 20.0, 20.0, 1.0, (0, 0),
                                      CloneSampling::CurrentLayer));
        assert!(!d.is_cloning());
    }

    #[test]
    fn cloning_from_off_canvas_leaves_the_layer_alone() {
        // Nothing to copy from out there, and painting transparency instead
        // would punch a hole in the layer.
        let mut d = Document::new(40, 40, Rgba8::WHITE);
        d.commit("Setup");
        let brush = Brush { size: 10.0, hardness: 1.0, ..Brush::default() };
        d.begin_clone_stroke(&brush, 5.0, 20.0, 1.0, (-100, 0), CloneSampling::CurrentLayer);
        d.end_clone_stroke(1.0);
        assert_eq!(d.composite().get(5, 20), Rgba8::WHITE);
    }

    #[test]
    fn a_clone_stroke_on_a_locked_layer_is_refused() {
        let mut d = Document::new(40, 40, Rgba8::WHITE);
        d.active_layer_mut().unwrap().lock_pixels = true;
        let brush = Brush::default();
        assert!(!d.begin_clone_stroke(&brush, 20.0, 20.0, 1.0, (-10, 0),
                                      CloneSampling::CurrentLayer));
    }

    #[test]
    fn a_clone_stroke_previews_the_pixels_it_will_copy() {
        // The live preview must show the cloned source, not the foreground
        // colour — the shell asks for one preview whatever the tool.
        let mut d = Document::new(80, 40, Rgba8::WHITE);
        if let Some(layer) = d.active_layer_mut() {
            layer.pixels.fill_rect(Rect::new(0, 0, 20, 40), Rgba8::opaque(10, 200, 10));
        }
        d.commit("Setup");

        let brush = Brush { size: 20.0, hardness: 1.0, ..Brush::default() };
        d.begin_clone_stroke(&brush, 60.0, 20.0, 1.0, (-50, 0), CloneSampling::CurrentLayer);
        let preview = d.preview_stroke(Rgba8::opaque(255, 0, 0), 1.0).expect("no preview");
        assert_eq!(preview.get(60, 20), Rgba8::opaque(10, 200, 10),
                   "the preview painted the foreground colour instead of the source");
    }

    #[test]
    fn cancelling_a_clone_stroke_drops_its_source() {
        let mut d = Document::new(40, 40, Rgba8::WHITE);
        let brush = Brush { size: 10.0, hardness: 1.0, ..Brush::default() };
        d.begin_clone_stroke(&brush, 20.0, 20.0, 1.0, (-10, 0), CloneSampling::CurrentLayer);
        assert!(d.is_cloning());
        d.cancel_stroke();
        assert!(!d.is_cloning(), "the snapshot outlived the stroke it belonged to");
    }

    #[test]
    fn a_fully_locked_layer_cannot_be_deleted_or_merged() {
        let mut d = Document::new(16, 16, Rgba8::WHITE);
        let upper = d.add_layer(None);
        if let Some(l) = d.stack.by_id_mut(upper) {
            l.lock_transparency = true;
            l.lock_pixels = true;
            l.lock_position = true;
        }
        assert!(!d.delete_layer(upper), "a fully locked layer was deleted");
        assert!(!d.merge_down(upper), "a fully locked layer was merged away");
        assert_eq!(d.layer_count(), 2);

        // One lock short of Lock All is not enough to protect it: Photoshop only
        // refuses on the full lock.
        if let Some(l) = d.stack.by_id_mut(upper) {
            l.lock_position = false;
        }
        assert!(d.delete_layer(upper), "a partly locked layer refused deletion");
    }

    #[test]
    fn merging_onto_a_fully_locked_layer_is_refused() {
        // The lower layer is the one rewritten by a merge, so locking it has to
        // stop the merge as surely as locking the upper one does.
        let mut d = Document::new(16, 16, Rgba8::WHITE);
        let lower = d.active_layer_id();
        if let Some(l) = d.stack.by_id_mut(lower) {
            l.lock_transparency = true;
            l.lock_pixels = true;
            l.lock_position = true;
        }
        let upper = d.add_layer(None);
        assert!(!d.merge_down(upper), "the merge overwrote a locked layer");
        assert_eq!(d.layer_count(), 2);
    }

    #[test]
    fn setting_the_locks_is_one_undo_step() {
        let mut d = Document::new(16, 16, Rgba8::WHITE);
        let id = d.active_layer_id();
        d.set_layer_locks(id, true, true, false);
        assert!(d.active_layer().unwrap().is_locked());
        assert!(!d.active_layer().unwrap().is_fully_locked());

        assert!(d.undo(), "locking left nothing to undo");
        assert!(!d.active_layer().unwrap().is_locked(), "undo did not unlock the layer");
    }

    #[test]
    fn a_mixer_stroke_respects_the_transparency_lock() {
        // Lock Transparent Pixels: the mixer may recolour what is there but must
        // not give an empty pixel any coverage.
        let mut d = Document::new(40, 40, Rgba8::TRANSPARENT);
        if let Some(layer) = d.active_layer_mut() {
            layer.pixels.fill_rect(Rect::new(0, 0, 40, 20), Rgba8::opaque(200, 200, 200));
            layer.lock_transparency = true;
        }
        d.commit("Setup");

        let brush = Brush { size: 30.0, hardness: 1.0, ..Brush::default() };
        let options = MixerOptions { load: 1.0, ..MixerOptions::default() };
        d.begin_mixer(&brush, options, Rgba8::BLACK, 20.0, 20.0, 1.0);
        d.end_mixer();

        let px = &d.active_layer().unwrap().pixels;
        assert_eq!(px.get(20, 30).a, 0, "paint reached a transparent pixel");
        assert!(px.get(20, 10).r < 200, "the opaque half was not painted");
        assert_eq!(px.get(20, 10).a, 255, "the opaque half lost its alpha");
    }

    #[test]
    fn a_mixer_stroke_is_one_undo_step() {
        let mut d = Document::new(60, 40, Rgba8::WHITE);
        d.commit("Setup");
        let before = d.composite().get(30, 20);

        let brush = Brush { size: 24.0, hardness: 1.0, ..Brush::default() };
        let options = MixerOptions { wet: 0.0, load: 1.0, mix: 0.0, ..MixerOptions::default() };
        assert!(d.begin_mixer(&brush, options, Rgba8::opaque(20, 20, 220), 20.0, 20.0, 1.0));
        d.extend_mixer(&brush, 30.0, 20.0, 1.0);
        d.extend_mixer(&brush, 40.0, 20.0, 1.0);
        assert!(d.end_mixer().is_some(), "the stroke reported no paint left on the brush");
        assert_ne!(d.composite().get(30, 20), before);

        assert!(d.undo(), "nothing to undo");
        assert_eq!(d.composite().get(30, 20), before, "one undo did not restore the stroke");
    }

    #[test]
    fn cancelling_a_mixer_stroke_restores_the_layer() {
        let mut d = Document::new(40, 40, Rgba8::WHITE);
        d.commit("Setup");
        let before = d.composite().get(20, 20);

        let brush = Brush { size: 20.0, hardness: 1.0, ..Brush::default() };
        let options = MixerOptions { load: 1.0, ..MixerOptions::default() };
        d.begin_mixer(&brush, options, Rgba8::BLACK, 20.0, 20.0, 1.0);
        assert_ne!(d.composite().get(20, 20), before, "the stroke did nothing to cancel");

        d.cancel_mixer();
        assert_eq!(d.composite().get(20, 20), before, "cancel left the change behind");
    }

    #[test]
    fn a_mixer_stroke_carries_paint_over_to_the_next_one() {
        // The reservoir outlives the stroke, so a second stroke starting on
        // white still lays down what the first one picked up.
        let mut d = Document::new(64, 32, Rgba8::WHITE);
        if let Some(layer) = d.active_layer_mut() {
            layer.pixels.fill_rect(Rect::new(0, 0, 16, 32), Rgba8::opaque(20, 20, 220));
        }
        d.commit("Setup");

        let brush = Brush { size: 16.0, hardness: 1.0, ..Brush::default() };
        let options = MixerOptions { wet: 0.8, load: 1.0, mix: 1.0, flow: 1.0, ..MixerOptions::default() };
        d.begin_mixer(&brush, options, Rgba8::WHITE, 8.0, 16.0, 1.0);
        for x in 9..20 {
            d.extend_mixer(&brush, x as f32, 16.0, 1.0);
        }
        let carried = d.end_mixer().expect("the stroke returned no reservoir");
        assert!(carried.b > carried.r + 10, "the brush did not pick the blue up: {carried:?}");
    }

    #[test]
    fn a_mixer_stroke_honours_the_selection() {
        let mut d = Document::new(40, 40, Rgba8::WHITE);
        d.commit("Setup");
        d.select_rect(Rect::new(0, 0, 20, 40), SelectionOp::Replace, 0);

        let brush = Brush { size: 20.0, hardness: 1.0, ..Brush::default() };
        let options = MixerOptions { load: 1.0, ..MixerOptions::default() };
        d.begin_mixer(&brush, options, Rgba8::BLACK, 20.0, 20.0, 1.0);
        d.end_mixer();

        assert!(d.composite().get(15, 20).r < 200, "nothing was painted inside the selection");
        assert_eq!(d.composite().get(25, 20), Rgba8::WHITE, "paint escaped the selection");
    }

    #[test]
    fn a_mixer_stroke_can_pick_colour_up_from_every_layer() {
        // Sample All Layers reads the composite, so a colour that lives on a
        // lower layer is picked up even though the paint lands above it.
        let mut d = Document::new(40, 40, Rgba8::opaque(20, 200, 20));
        d.commit("Setup");
        let upper = d.add_layer(Some("Layer 1".to_string()));
        d.set_active_layer(upper);

        let brush = Brush { size: 20.0, hardness: 1.0, ..Brush::default() };
        let options = MixerOptions {
            wet: 1.0,
            load: 1.0,
            mix: 1.0,
            flow: 1.0,
            sample_all_layers: true,
            preserve_alpha: false,
        };
        d.begin_mixer(&brush, options, Rgba8::TRANSPARENT, 20.0, 20.0, 1.0);
        d.end_mixer();

        let painted = d.active_layer().unwrap().pixels.get(20, 20);
        assert!(painted.g > painted.r + 20, "the green below was not picked up: {painted:?}");
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

    /// A type layer, 8x4 pixels at (4, 4), saying `text` in one run.
    fn type_layer(d: &mut Document, text: &str) -> LayerId {
        let mut pixels = Pixmap::new(8, 4);
        pixels.fill(Rgba8::BLACK);
        d.add_text_layer(pixels, (4, 4), text.to_string(), type_content(text))
    }

    fn type_run(text: &str, size: f32) -> TextRun {
        TextRun {
            text: text.to_string(),
            family: "Permanent Marker".to_string(),
            style: "Regular".to_string(),
            size,
            color: Rgba8::BLACK,
        }
    }

    fn type_content(text: &str) -> TextContent {
        TextContent {
            runs: vec![type_run(text, 12.0)],
            align: TextAlign::Left,
            antialias: true,
            vertical: false,
            origin: (4.0, 4.0),
        }
    }

    #[test]
    fn a_type_layer_remembers_what_it_was_typed_from() {
        let mut d = doc();
        let id = type_layer(&mut d, "hello");
        let text = d.layers().by_id(id).unwrap().text.as_ref().unwrap();
        assert_eq!(text.text(), "hello");
        assert_eq!(text.first_run().unwrap().family, "Permanent Marker");
        assert_eq!(text.origin, (4.0, 4.0));
    }

    #[test]
    fn runs_keep_their_own_formatting_and_join_back_into_one_string() {
        let mut d = doc();
        let mut pixels = Pixmap::new(8, 4);
        pixels.fill(Rgba8::BLACK);
        let content = TextContent {
            // "das" at 12pt, "ds" at 72pt, "dasdsd" back at 12pt — the mixed
            // sizes a selection-only size change leaves behind.
            runs: vec![type_run("das", 12.0), type_run("ds", 72.0), type_run("dasdsd", 12.0)],
            align: TextAlign::Left,
            antialias: true,
            vertical: false,
            origin: (4.0, 4.0),
        };
        let id = d.add_text_layer(pixels, (4, 4), "das".to_string(), content);

        let text = d.layers().by_id(id).unwrap().text.as_ref().unwrap();
        assert_eq!(text.text(), "dasdsdasdsd");
        assert_eq!(text.runs.len(), 3);
        assert_eq!(text.runs[1].size, 72.0, "the middle run lost its size");
        assert_eq!(text.runs[2].size, 12.0, "the size change spread past the selection");
    }

    #[test]
    fn clicking_in_a_type_layer_finds_it_and_clicking_outside_does_not() {
        let mut d = doc();
        let id = type_layer(&mut d, "hello");
        assert_eq!(d.text_layer_at(5, 5), Some(id));
        assert_eq!(d.text_layer_at(15, 15), None, "a click clear of the text found it anyway");
        assert_eq!(d.text_layer_at(4, 4), Some(id), "the top-left corner is inside");
        assert_eq!(d.text_layer_at(12, 8), None, "bounds are half-open");
    }

    #[test]
    fn a_hidden_type_layer_is_not_reopened_by_a_click() {
        let mut d = doc();
        let id = type_layer(&mut d, "hello");
        d.set_layer_visible(id, false);
        assert_eq!(d.text_layer_at(5, 5), None);
    }

    #[test]
    fn retyping_updates_the_layer_in_place() {
        let mut d = doc();
        let id = type_layer(&mut d, "hello");
        let count = d.layer_count();

        let mut wider = Pixmap::new(16, 4);
        wider.fill(Rgba8::BLACK);
        assert!(d.update_text_layer(
            id,
            wider,
            (4, 4),
            "hello there".to_string(),
            type_content("hello there")
        ));

        assert_eq!(d.layer_count(), count, "retyping stacked a second layer");
        let layer = d.layers().by_id(id).unwrap();
        assert_eq!(layer.name, "hello there");
        assert_eq!(layer.pixels.width(), 16);
        assert_eq!(layer.text.as_ref().unwrap().text(), "hello there");
    }

    #[test]
    fn retyping_a_layer_that_has_gone_reports_failure() {
        let mut d = doc();
        let id = type_layer(&mut d, "hello");
        d.delete_layer(id);
        assert!(!d.update_text_layer(
            id,
            Pixmap::new(8, 4),
            (4, 4),
            "hello".to_string(),
            type_content("hello")
        ));
    }

    #[test]
    fn moving_a_type_layer_carries_its_anchor_along() {
        let mut d = doc();
        let id = type_layer(&mut d, "hello");
        d.offset_layer(id, 3, -2);
        let text = d.layers().by_id(id).unwrap().text.as_ref().unwrap();
        assert_eq!(text.origin, (7.0, 2.0));
        assert_eq!(d.text_layer_at(8, 3), Some(id), "the moved text is not where it is drawn");
    }

    #[test]
    fn an_open_type_edit_hides_the_pixels_and_gives_them_back() {
        let mut d = doc();
        let id = type_layer(&mut d, "hello");
        let steps = d.history().len();

        assert!(d.begin_text_edit(id));
        assert!(!d.layers().by_id(id).unwrap().visible);
        assert_eq!(d.text_edit_layer(), Some((id, true)));
        assert_eq!(d.history().len(), steps, "opening an edit made a history state");

        d.end_text_edit();
        assert!(d.layers().by_id(id).unwrap().visible);
        assert_eq!(d.text_edit_layer(), None);
    }

    #[test]
    fn ending_an_edit_restores_a_layer_that_was_hidden_to_begin_with() {
        let mut d = doc();
        let id = type_layer(&mut d, "hello");
        d.set_layer_visible(id, false);

        assert!(d.begin_text_edit(id));
        d.end_text_edit();
        assert!(!d.layers().by_id(id).unwrap().visible, "the edit turned a hidden layer on");
    }
}

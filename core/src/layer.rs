//! The layer model.
//!
//! A [`Layer`] owns its pixels plus an optional mask, and carries the
//! compositing state the Layers panel exposes: blend mode, opacity, fill
//! opacity, visibility, lock flags and a clipping flag.

use crate::blend::BlendMode;
use crate::buffer::{Pixmap, Rect, Rgba8};

/// Stable identifier for a layer.
///
/// Indices shift whenever a layer is reordered or deleted, so anything that
/// outlives a single call — selection state, history records, the C++ panel —
/// refers to layers by `LayerId`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct LayerId(pub u64);

impl LayerId {
    /// Sentinel for "no layer".
    pub const NONE: LayerId = LayerId(0);

    pub fn is_none(&self) -> bool {
        *self == LayerId::NONE
    }
}

/// What a layer draws.
#[derive(Clone, Debug)]
pub enum LayerKind {
    /// Ordinary pixels.
    Raster,
    /// A non-destructive adjustment applied to everything beneath it.
    Adjustment(crate::filters::Adjustment),
    /// A uniform colour fill covering the whole canvas.
    SolidColor(Rgba8),
    /// A gradient, evaluated per pixel so it stays re-angleable.
    Gradient(crate::fill::GradientFill),
    /// A repeated tile, likewise.
    Pattern(crate::fill::PatternFill),
    /// A folder holding the layers beneath it — CS6's layer group.
    ///
    /// The group carries no pixels of its own. Its members are the run of
    /// layers immediately below it in the stack whose `parent` names it, and
    /// the compositor renders them into a buffer before blending that buffer
    /// with the group's own blend mode, opacity and mask.
    Group,
}

impl LayerKind {
    /// Whether the compositor pours this rather than reading pixels.
    pub fn is_fill(&self) -> bool {
        matches!(
            self,
            LayerKind::SolidColor(_) | LayerKind::Gradient(_) | LayerKind::Pattern(_)
        )
    }
}

/// CS6's **Blend If**: a per-pixel gate on where a layer shows.
///
/// Two ranges, read off the layer's own pixel and off what is beneath it. A
/// pixel outside a range is hidden; the two halves of each handle are what
/// makes the boundary a ramp rather than a cliff — Photoshop splits them with
/// Alt, and so does this.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlendIf {
    /// 0 = Gray (luminance), 1 = Red, 2 = Green, 3 = Blue.
    pub channel: u8,
    /// The layer's own range: where the dark handle starts and finishes fading
    /// in, then where the light handle starts and finishes fading out.
    pub this_layer: [u8; 4],
    /// The same, read off the backdrop.
    pub underlying: [u8; 4],
}

impl Default for BlendIf {
    /// Wide open: nothing is gated.
    fn default() -> Self {
        Self {
            channel: 0,
            this_layer: [0, 0, 255, 255],
            underlying: [0, 0, 255, 255],
        }
    }
}

impl BlendIf {
    /// Whether the ranges are still wide open, which is the common case and
    /// worth not paying for.
    pub fn is_open(&self) -> bool {
        self.this_layer == [0, 0, 255, 255] && self.underlying == [0, 0, 255, 255]
    }

    /// How much of a pixel survives the gate, `0.0..=1.0`.
    pub fn coverage(&self, source: [f32; 3], backdrop: [f32; 3]) -> f32 {
        if self.is_open() {
            return 1.0;
        }
        gate(self.this_layer, self.value_of(source)) * gate(self.underlying, self.value_of(backdrop))
    }

    /// The channel this gate reads, from a straight-alpha RGB triple in
    /// `0.0..=1.0`.
    fn value_of(&self, rgb: [f32; 3]) -> f32 {
        match self.channel {
            1 => rgb[0],
            2 => rgb[1],
            3 => rgb[2],
            // Rec.601 luminance, the same grey the rest of the engine uses.
            _ => 0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2],
        }
    }
}

/// One range's contribution: in fully between the inner handles, ramping over
/// the gap when the two halves have been split apart.
fn gate(range: [u8; 4], value: f32) -> f32 {
    let v = value.clamp(0.0, 1.0) * 255.0;
    let [dark_start, dark_end, light_start, light_end] =
        [range[0] as f32, range[1] as f32, range[2] as f32, range[3] as f32];

    // The "in" tests come first, and deliberately: with a handle unsplit at 0
    // its two halves sit on the same value, and a pixel of exactly 0 has to
    // count as inside the range — otherwise a wide-open Blend If would hide
    // every black pixel in the document. The same at 255 for the light handle.
    let rising = if v >= dark_end {
        1.0
    } else if v <= dark_start {
        0.0
    } else {
        (v - dark_start) / (dark_end - dark_start).max(1e-3)
    };
    let falling = if v <= light_start {
        1.0
    } else if v >= light_end {
        0.0
    } else {
        1.0 - (v - light_start) / (light_end - light_start).max(1e-3)
    };
    rising.min(falling)
}

/// Everything the Layer Style dialog can change, as one value.
///
/// The dialog edits live and undoes on cancel. Remembering each setting as it
/// is touched sounds equivalent and is not: it only puts back what the dialog
/// remembered to record, and one control writing a value the bookkeeping did
/// not know about leaves that value behind. Taking the lot on the way in and
/// putting the lot back is not something a new control can defeat.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StyleState {
    pub effects: crate::effects::LayerEffects,
    pub blend_mode: BlendMode,
    pub opacity: f32,
    pub fill_opacity: f32,
    pub channels: [bool; 3],
    pub blend_if: BlendIf,
    pub transparency_shapes: bool,
    pub mask_hides_effects: bool,
}

/// How a type layer's lines sit about its origin: left, centre or right for
/// ordinary type, and top, centre or bottom for vertical type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

/// What a type layer was rasterized *from*.
///
/// A type layer is an ordinary raster layer as far as the compositor, the
/// filters and the file writers are concerned — it carries real pixels. What
/// makes it a type layer is that it also remembers the text and the settings
/// those pixels came from, so the Type tool can reopen it, change a word and
/// render it again, the way clicking into existing text does in Photoshop.
///
/// Shaping and rasterizing stay in the shell (CLAUDE.md §2 — Qt's font engine
/// is the right tool and re-implementing one here would be a project of its
/// own), so this records the *choices* made there rather than anything
/// Qt-specific: a family and style by name, not a serialized `QFont`. That
/// keeps it something `.psd` type records can eventually be mapped onto.
///
/// The text is held as a list of [`TextRun`]s rather than one string plus one
/// font, because character formatting is per-character in Photoshop: select two
/// letters in the middle of a word, set them to 72pt, and only those two change.
/// A run carries its own text, so nothing here has to agree with anything else
/// about how a string is indexed.
#[derive(Clone, Debug)]
pub struct TextContent {
    /// The text in order, split wherever its formatting changes. Never empty
    /// for a live type layer — text with nothing in it is not kept as a layer.
    pub runs: Vec<TextRun>,
    /// Alignment is a paragraph property, so it belongs to the whole block
    /// rather than to a run, and so does antialiasing.
    pub align: TextAlign,
    pub antialias: bool,
    /// Set by the Vertical Type tool: characters run top to bottom and each
    /// new line starts a column to the left of the last, the way Photoshop's
    /// vertical type reads. `align` then means top, centre or bottom of the
    /// column rather than left, centre or right of the line.
    pub vertical: bool,
    /// The click that started the text, in document space. Lines are laid out
    /// from here per `align`, so reopening the layer resumes from the same
    /// anchor rather than having to work backwards from the pixel bounds.
    pub origin: (f32, f32),
}

/// A stretch of text set the same way — Photoshop's character run.
#[derive(Clone, Debug)]
pub struct TextRun {
    pub text: String,
    /// Font family name, e.g. "Permanent Marker".
    pub family: String,
    /// Style name within the family, e.g. "Regular" or "Bold Italic". A name
    /// rather than bold/italic bits, because that is what the family actually
    /// offers.
    pub style: String,
    /// Size in document pixels — the Type tool's point size.
    pub size: f32,
    pub color: Rgba8,
}

impl TextContent {
    /// The whole text, runs joined back together — what the layer says.
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }

    /// The formatting the text starts with, which is what a caller that can
    /// only deal with one font — a thumbnail, a future `.psd` writer's fallback
    /// — should use.
    pub fn first_run(&self) -> Option<&TextRun> {
        self.runs.first()
    }
}

/// A single layer.
#[derive(Clone, Debug)]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    pub kind: LayerKind,

    /// Pixel data. For adjustment and fill layers this is empty — they are
    /// evaluated procedurally by the compositor.
    pub pixels: Pixmap,

    /// Top-left of `pixels` in document space. Layers may hang off the canvas.
    pub offset: (i32, i32),

    /// Greyscale mask, same size as `pixels`, where 255 = fully visible.
    /// Stored in the alpha channel of a `Pixmap` for buffer reuse.
    pub mask: Option<Pixmap>,
    pub mask_enabled: bool,
    /// Whether the mask travels with the layer — CS6's chain between the two
    /// thumbnails. Recorded and shown; nothing here moves a mask on its own
    /// yet, since the mask shares the layer's origin.
    pub mask_linked: bool,

    pub blend_mode: BlendMode,
    /// Master opacity, `0.0..=1.0`.
    pub opacity: f32,
    /// Fill opacity, `0.0..=1.0`. Scales the layer's own pixels but — unlike
    /// `opacity` — leaves layer effects untouched. Effects are not implemented
    /// yet, so today it behaves as a second opacity multiplier.
    pub fill_opacity: f32,
    pub visible: bool,

    /// Clip to the layer below, forming a clipping group.
    pub clipping: bool,

    /// Painting may not change a transparent pixel's alpha — Photoshop's Lock
    /// Transparent Pixels.
    pub lock_transparency: bool,
    /// Nothing may edit the layer's pixels at all — Lock Image Pixels. This is
    /// the lock that makes a layer untouchable by the tools.
    pub lock_pixels: bool,
    /// The layer may not be moved — Lock Position.
    pub lock_position: bool,

    /// Set on a type layer: what its pixels were rendered from. `None` on
    /// every other layer.
    pub text: Option<TextContent>,

    /// Layer Style — drop shadow, stroke and the rest. Drawn by the compositor
    /// around the layer's own pixels; see [`crate::effects`].
    pub effects: crate::effects::LayerEffects,

    /// The row colour CS6 lets you tag a layer with: 0 for none, then Red,
    /// Orange, Yellow, Green, Blue, Violet, Gray. Nothing about the image
    /// depends on it — it is there to find a layer by in a tall stack.
    pub label: u8,

    /// Which colour channels the layer contributes to — CS6's Advanced
    /// Blending R/G/B. A channel switched off leaves the backdrop's own value
    /// showing through.
    pub channels: [bool; 3],
    /// Where the layer is allowed to show at all.
    pub blend_if: BlendIf,
    /// Whether the layer's transparency shapes its effects. Off, an overlay
    /// fills the layer's whole rectangle instead of following its content.
    pub transparency_shapes: bool,
    /// Whether the layer mask hides the effects as well as the pixels. Off,
    /// the effects are drawn from the unmasked shape, as CS6 does by default.
    pub mask_hides_effects: bool,

    /// The group this layer belongs to, if any.
    ///
    /// Membership is by id rather than by nesting the layers themselves: the
    /// stack stays one flat, ordered list, so every index-based operation in
    /// the engine and every panel index keeps working. What makes it a tree is
    /// that a group's members are the contiguous run directly beneath it —
    /// [`LayerStack::group_members`] is the only place that rule is written
    /// down, and everything that moves layers has to preserve it.
    pub parent: Option<LayerId>,
    /// Whether a group is open in the panel. Meaningless on anything else.
    pub expanded: bool,
}

impl Layer {
    /// A transparent raster layer of the given size.
    pub fn new_raster(id: LayerId, name: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            id,
            name: name.into(),
            kind: LayerKind::Raster,
            pixels: Pixmap::new(width, height),
            offset: (0, 0),
            mask: None,
            mask_enabled: true,
            mask_linked: true,
            blend_mode: BlendMode::Normal,
            opacity: 1.0,
            fill_opacity: 1.0,
            visible: true,
            clipping: false,
            lock_transparency: false,
            lock_pixels: false,
            lock_position: false,
            text: None,
            effects: crate::effects::LayerEffects::default(),
            label: 0,
            channels: [true; 3],
            blend_if: BlendIf::default(),
            transparency_shapes: true,
            mask_hides_effects: true,
            parent: None,
            expanded: true,
        }
    }

    /// An empty group — Layer ▸ Group Layers, before anything is put in it.
    pub fn new_group(id: LayerId, name: impl Into<String>) -> Self {
        Self {
            kind: LayerKind::Group,
            ..Layer::new_raster(id, name, 0, 0)
        }
    }

    /// Whether this layer is a group folder rather than something with pixels.
    pub fn is_group(&self) -> bool {
        matches!(self.kind, LayerKind::Group)
    }

    /// A raster layer pre-filled with `color` — this is what "Background" is.
    pub fn new_filled(
        id: LayerId,
        name: impl Into<String>,
        width: u32,
        height: u32,
        color: Rgba8,
    ) -> Self {
        let mut layer = Self::new_raster(id, name, width, height);
        layer.pixels.fill(color);
        layer
    }

    /// An adjustment layer. Carries no pixels of its own.
    pub fn new_adjustment(
        id: LayerId,
        name: impl Into<String>,
        adjustment: crate::filters::Adjustment,
    ) -> Self {
        let mut layer = Self::new_raster(id, name, 0, 0);
        layer.kind = LayerKind::Adjustment(adjustment);
        layer
    }

    /// The layer's bounds in document space.
    pub fn bounds(&self) -> Rect {
        Rect::new(
            self.offset.0,
            self.offset.1,
            self.pixels.width(),
            self.pixels.height(),
        )
    }

    /// Whether any lock is on, which is what puts the padlock badge on the
    /// layer's row.
    pub fn is_locked(&self) -> bool {
        self.lock_transparency || self.lock_pixels || self.lock_position
    }

    /// Whether every lock is on — Photoshop's Lock All. A fully locked layer
    /// cannot be deleted or merged either, not merely painted on.
    pub fn is_fully_locked(&self) -> bool {
        self.lock_transparency && self.lock_pixels && self.lock_position
    }

    /// Effective alpha multiplier: master opacity times fill opacity.
    /// The style settings as one value, for an edit that may be cancelled.
    pub fn style_state(&self) -> StyleState {
        StyleState {
            effects: self.effects,
            blend_mode: self.blend_mode,
            opacity: self.opacity,
            fill_opacity: self.fill_opacity,
            channels: self.channels,
            blend_if: self.blend_if,
            transparency_shapes: self.transparency_shapes,
            mask_hides_effects: self.mask_hides_effects,
        }
    }

    pub fn set_style_state(&mut self, state: StyleState) {
        self.effects = state.effects;
        self.blend_mode = state.blend_mode;
        self.opacity = state.opacity;
        self.fill_opacity = state.fill_opacity;
        self.channels = state.channels;
        self.blend_if = state.blend_if;
        self.transparency_shapes = state.transparency_shapes;
        self.mask_hides_effects = state.mask_hides_effects;
    }

    pub fn effective_alpha(&self) -> f32 {
        (self.opacity * self.fill_opacity).clamp(0.0, 1.0)
    }

    /// Whether the compositor should skip this layer entirely.
    pub fn is_invisible(&self) -> bool {
        !self.visible || self.effective_alpha() <= 0.0
    }

    /// Mask coverage at a document-space point, `0.0..=1.0`.
    ///
    /// Returns 1.0 when there is no mask or the mask is disabled. Points
    /// outside the mask read as fully masked *out*, matching how Photoshop
    /// treats an undefined mask area.
    pub fn mask_at(&self, doc_x: i32, doc_y: i32) -> f32 {
        let Some(mask) = &self.mask else { return 1.0 };
        if !self.mask_enabled {
            return 1.0;
        }
        let lx = doc_x - self.offset.0;
        let ly = doc_y - self.offset.1;
        if lx < 0 || ly < 0 || lx >= mask.width() as i32 || ly >= mask.height() as i32 {
            return 0.0;
        }
        mask.get(lx, ly).a as f32 / 255.0
    }

    /// Attach a fully-revealing (white) mask sized to the layer.
    pub fn add_reveal_all_mask(&mut self) {
        let mut m = Pixmap::new(self.pixels.width(), self.pixels.height());
        m.fill(Rgba8::new(255, 255, 255, 255));
        self.mask = Some(m);
        self.mask_enabled = true;
    }

    /// Attach a fully-hiding (black) mask sized to the layer.
    pub fn add_hide_all_mask(&mut self) {
        let mut m = Pixmap::new(self.pixels.width(), self.pixels.height());
        m.fill(Rgba8::new(0, 0, 0, 0));
        self.mask = Some(m);
        self.mask_enabled = true;
    }

    pub fn remove_mask(&mut self) {
        self.mask = None;
    }

    /// Approximate memory footprint, used by the history stack.
    pub fn byte_size(&self) -> usize {
        self.pixels.byte_size() + self.mask.as_ref().map_or(0, |m| m.byte_size())
    }
}

/// An ordered stack of layers.
///
/// Index 0 is the **bottom** of the stack (the Background), matching the order
/// the compositor walks. The Layers panel reverses this for display, since
/// Photoshop shows the topmost layer first.
#[derive(Clone, Debug, Default)]
pub struct LayerStack {
    layers: Vec<Layer>,
    next_id: u64,
}

impl LayerStack {
    pub fn new() -> Self {
        Self {
            layers: Vec::new(),
            // 0 is reserved for LayerId::NONE.
            next_id: 1,
        }
    }

    /// Mint a fresh, never-reused id.
    pub fn allocate_id(&mut self) -> LayerId {
        let id = LayerId(self.next_id);
        self.next_id += 1;
        id
    }

    pub fn len(&self) -> usize {
        self.layers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Layer> {
        self.layers.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, Layer> {
        self.layers.iter_mut()
    }

    pub fn as_slice(&self) -> &[Layer] {
        &self.layers
    }

    pub fn get(&self, index: usize) -> Option<&Layer> {
        self.layers.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut Layer> {
        self.layers.get_mut(index)
    }

    pub fn by_id(&self, id: LayerId) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    pub fn by_id_mut(&mut self, id: LayerId) -> Option<&mut Layer> {
        self.layers.iter_mut().find(|l| l.id == id)
    }

    pub fn index_of(&self, id: LayerId) -> Option<usize> {
        self.layers.iter().position(|l| l.id == id)
    }

    /// Push onto the top of the stack.
    pub fn push(&mut self, layer: Layer) {
        self.layers.push(layer);
    }

    /// Insert at a specific stack position, clamped to the valid range.
    ///
    /// A layer landing among a group's members joins that group. Doing it here
    /// rather than at the nine places that add a layer means a new layer, a
    /// duplicate, a Layer Via Copy and everything after them all end up inside
    /// the group the user was working in — and, more importantly, that the
    /// members either side of it cannot be cut off from their folder, which is
    /// what [`LayerStack::group_members`] would see if a stranger were left
    /// sitting between them.
    ///
    /// A layer that already names a parent is taken at its word, which is what
    /// lets a group be assembled a member at a time.
    pub fn insert(&mut self, index: usize, mut layer: Layer) {
        let index = index.min(self.layers.len());
        if layer.parent.is_none() && !layer.is_group() {
            layer.parent = self.group_at_position(index);
        }
        self.layers.insert(index, layer);
    }

    /// The group whose run of members encloses a stack position, if any.
    ///
    /// A position is inside a group when the layer that would sit above it
    /// belongs to that group (or is the folder itself) *and* so does the one
    /// below. Anywhere else is outside — including directly beneath the
    /// folder, which is the gap a layer is dropped into to leave a group.
    pub fn group_at_position(&self, position: usize) -> Option<LayerId> {
        let layer_above = self.layers.get(position)?;
        let above = if layer_above.is_group() {
            // The gap directly beneath an *empty* folder is the only way into
            // one: with no members there is no pair of them to land between.
            // The same gap under a group that has members is the top of its
            // run, which is inside it too, so this costs nothing.
            if self.group_members(position).is_empty() {
                return Some(layer_above.id);
            }
            layer_above.id
        } else {
            layer_above.parent?
        };
        let below = position
            .checked_sub(1)
            .and_then(|i| self.layers.get(i))
            .and_then(|l| l.parent);
        (below == Some(above)).then_some(above)
    }

    pub fn remove(&mut self, index: usize) -> Option<Layer> {
        if index < self.layers.len() {
            Some(self.layers.remove(index))
        } else {
            None
        }
    }

    pub fn remove_by_id(&mut self, id: LayerId) -> Option<Layer> {
        self.index_of(id).and_then(|i| self.remove(i))
    }

    /// The stack positions of a group's members: the run directly beneath it.
    ///
    /// Beneath, because the stack runs bottom-up while the panel runs top-down
    /// — a group's members sit at lower indices than the folder itself, which
    /// is what puts them *under* it on screen.
    ///
    /// A layer claiming a group it is not adjacent to is not a member. That
    /// cannot happen through the editing operations, all of which keep the run
    /// contiguous, and answering by adjacency rather than by scanning the whole
    /// stack means a stray `parent` degrades to a loose layer rather than to a
    /// group whose contents are somewhere else entirely.
    pub fn group_members(&self, group_index: usize) -> std::ops::Range<usize> {
        let Some(group) = self.layers.get(group_index) else {
            return 0..0;
        };
        if !group.is_group() {
            return group_index..group_index;
        }
        let mut first = group_index;
        while first > 0 && self.layers[first - 1].parent == Some(group.id) {
            first -= 1;
        }
        first..group_index
    }

    /// The group a layer belongs to, as a stack position.
    pub fn group_of(&self, index: usize) -> Option<usize> {
        let parent = self.layers.get(index)?.parent?;
        self.index_of(parent)
    }

    /// A group and its members as one run, for the operations that have to
    /// move or delete them together. For anything else, just that layer.
    pub fn run_at(&self, index: usize) -> std::ops::Range<usize> {
        match self.layers.get(index) {
            Some(layer) if layer.is_group() => {
                let members = self.group_members(index);
                members.start..index + 1
            }
            Some(_) => index..index + 1,
            None => 0..0,
        }
    }

    /// Move the layer at `from` to position `to`. No-op if either is invalid.
    pub fn reorder(&mut self, from: usize, to: usize) {
        if from >= self.layers.len() || to >= self.layers.len() || from == to {
            return;
        }
        let layer = self.layers.remove(from);
        self.layers.insert(to, layer);
    }

    /// Generate a unique "Layer N" name, skipping numbers already taken.
    pub fn suggest_name(&self) -> String {
        let mut n = self.layers.len();
        loop {
            n += 1;
            let candidate = format!("Layer {}", n);
            if !self.layers.iter().any(|l| l.name == candidate) {
                return candidate;
            }
        }
    }

    /// The next free "<shape> N", for a shape layer. Photoshop names these
    /// after the tool that drew them rather than "Layer N".
    pub fn suggest_shape_name(&self, shape: &str) -> String {
        let mut n = 0;
        loop {
            n += 1;
            let candidate = format!("{} {}", shape, n);
            if !self.layers.iter().any(|l| l.name == candidate) {
                return candidate;
            }
        }
    }

    pub fn byte_size(&self) -> usize {
        self.layers.iter().map(|l| l.byte_size()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack_with(n: usize) -> LayerStack {
        let mut s = LayerStack::new();
        for i in 0..n {
            let id = s.allocate_id();
            s.push(Layer::new_raster(id, format!("L{}", i), 4, 4));
        }
        s
    }

    #[test]
    fn ids_are_unique_and_never_zero() {
        let mut s = LayerStack::new();
        let ids: Vec<_> = (0..5).map(|_| s.allocate_id()).collect();
        for id in &ids {
            assert!(!id.is_none(), "allocated the NONE sentinel");
        }
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len());
    }

    #[test]
    fn ids_are_not_reused_after_removal() {
        let mut s = stack_with(2);
        let first = s.get(0).unwrap().id;
        s.remove(0);
        let fresh = s.allocate_id();
        assert_ne!(fresh, first);
    }

    #[test]
    fn index_of_tracks_reordering() {
        let mut s = stack_with(3);
        let bottom = s.get(0).unwrap().id;
        s.reorder(0, 2);
        assert_eq!(s.index_of(bottom), Some(2));
    }

    #[test]
    fn reorder_ignores_out_of_range() {
        let mut s = stack_with(2);
        let before: Vec<_> = s.iter().map(|l| l.id).collect();
        s.reorder(0, 99);
        s.reorder(99, 0);
        s.reorder(1, 1);
        let after: Vec<_> = s.iter().map(|l| l.id).collect();
        assert_eq!(before, after);
    }

    #[test]
    fn insert_clamps_past_the_end() {
        let mut s = stack_with(2);
        let id = s.allocate_id();
        s.insert(99, Layer::new_raster(id, "top", 4, 4));
        assert_eq!(s.len(), 3);
        assert_eq!(s.get(2).unwrap().id, id);
    }

    #[test]
    fn effective_alpha_combines_both_opacities() {
        let mut l = Layer::new_raster(LayerId(1), "x", 2, 2);
        l.opacity = 0.5;
        l.fill_opacity = 0.5;
        assert!((l.effective_alpha() - 0.25).abs() < 1e-6);
    }

    #[test]
    fn zero_opacity_counts_as_invisible() {
        let mut l = Layer::new_raster(LayerId(1), "x", 2, 2);
        assert!(!l.is_invisible());
        l.opacity = 0.0;
        assert!(l.is_invisible());
        l.opacity = 1.0;
        l.visible = false;
        assert!(l.is_invisible());
    }

    #[test]
    fn mask_absent_is_fully_visible() {
        let l = Layer::new_raster(LayerId(1), "x", 4, 4);
        assert_eq!(l.mask_at(0, 0), 1.0);
        // Even outside the layer, since there is no mask to consult.
        assert_eq!(l.mask_at(100, 100), 1.0);
    }

    #[test]
    fn hide_all_mask_masks_everything() {
        let mut l = Layer::new_raster(LayerId(1), "x", 4, 4);
        l.add_hide_all_mask();
        assert_eq!(l.mask_at(1, 1), 0.0);
    }

    #[test]
    fn reveal_all_mask_reveals_inside_and_hides_outside() {
        let mut l = Layer::new_raster(LayerId(1), "x", 4, 4);
        l.add_reveal_all_mask();
        assert_eq!(l.mask_at(1, 1), 1.0);
        // Outside the mask's extent there is no coverage defined.
        assert_eq!(l.mask_at(50, 50), 0.0);
    }

    #[test]
    fn disabled_mask_is_ignored() {
        let mut l = Layer::new_raster(LayerId(1), "x", 4, 4);
        l.add_hide_all_mask();
        l.mask_enabled = false;
        assert_eq!(l.mask_at(1, 1), 1.0);
    }

    #[test]
    fn mask_respects_layer_offset() {
        let mut l = Layer::new_raster(LayerId(1), "x", 4, 4);
        l.add_reveal_all_mask();
        l.offset = (10, 10);
        assert_eq!(l.mask_at(11, 11), 1.0);
        assert_eq!(l.mask_at(1, 1), 0.0);
    }

    #[test]
    fn suggest_name_avoids_collisions() {
        let mut s = LayerStack::new();
        let id = s.allocate_id();
        s.push(Layer::new_raster(id, "Layer 1", 1, 1));
        assert_eq!(s.suggest_name(), "Layer 2");
    }

    #[test]
    fn bounds_follow_offset() {
        let mut l = Layer::new_raster(LayerId(1), "x", 3, 5);
        l.offset = (-2, 7);
        assert_eq!(l.bounds(), Rect::new(-2, 7, 3, 5));
    }
}

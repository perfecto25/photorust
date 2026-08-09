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
            blend_mode: BlendMode::Normal,
            opacity: 1.0,
            fill_opacity: 1.0,
            visible: true,
            clipping: false,
            lock_transparency: false,
            lock_pixels: false,
            lock_position: false,
        }
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
    pub fn insert(&mut self, index: usize, layer: Layer) {
        let index = index.min(self.layers.len());
        self.layers.insert(index, layer);
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

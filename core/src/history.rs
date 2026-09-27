//! Undo/redo.
//!
//! Photoshop's History panel is a linear list of states with a movable cursor:
//! stepping back and then performing a new action discards everything after the
//! cursor. That is exactly what this models.
//!
//! # What a state costs
//!
//! Each state used to be a full copy of the layer stack. On a 16000-pixel map
//! that is a gigabyte a step: committing a brush stroke copied every layer
//! (0.7 s), and with a 1 GiB budget there was not room for the state before
//! the stroke as well as the one after, so the picture had no undo at all.
//!
//! Now history keeps **one** whole stack — `base`, the state at the cursor —
//! and, for every other state, only what differs: the layers' settings (small,
//! kept whole) and, for their pixels and masks, just the [`TILE`]-square tiles
//! that changed. A stroke on the map costs the tiles it touched.
//!
//! Committing compares the live stack with `base` tile by tile, records the
//! old contents of the tiles that differ, and copies the new ones into `base`.
//! Undoing swaps a state's tiles back into `base` — which gives the tiles to
//! redo with, for free — then brings the live stack into line with `base` the
//! same way, tile by tile. Nothing is ever told what changed: the comparison
//! finds it, so an operation cannot forget to record a change, and undo is
//! correct for anything the engine does.
//!
//! The byte budget covers the recorded changes, not `base`: `base` is the one
//! copy the document always needs, however large.
//!
//! States also carry **the canvas size**: Crop and Canvas Size change it, and
//! restoring the pixels without the dimensions they were taken at would leave
//! the document inconsistent.

use crate::buffer::{Pixmap, Rect};
use crate::document::ImageMode;
use crate::layer::{Layer, LayerId, LayerStack};
use rayon::prelude::*;
use std::collections::HashMap;

/// The side of the square tiles pixels are compared and recorded in. Small
/// enough that a brush stroke costs little more than the pixels it touched;
/// large enough that a whole gigabyte layer is a few thousand tiles to track.
pub const TILE: u32 = 256;

/// One entry in the History panel.
pub struct HistoryState {
    /// Label shown in the panel, e.g. "Brush Tool" or "New Layer".
    pub name: String,
    /// Canvas size when the state was recorded.
    pub size: (u32, u32),
    /// Color mode when the state was recorded.
    pub color_mode: ImageMode,
    /// Bit depth when the state was recorded.
    pub bit_depth: u8,
    /// How to get here from the state beside it, as deltas applied in order.
    /// For a state at or before the cursor these lead *back* to the state
    /// before it; for one after the cursor they lead *forward* from the state
    /// before it. Empty for the first state. More than one only where
    /// [`History::push_coalescing`] folded several steps into this one.
    change: Vec<Delta>,
}

/// What [`History::undo`], [`History::redo`] and [`History::jump_to`] report
/// about the state they arrived at, besides the layers they restored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StateInfo {
    pub size: (u32, u32),
    pub color_mode: ImageMode,
    pub bit_depth: u8,
    /// Where the restored layers' pixels differ from what the document had,
    /// in document pixels — the union of the tiles that were put back, each
    /// where its layer sits. `None` when more than pixels changed (a layer
    /// came, went, moved, was resized or had a setting changed), which is
    /// everywhere as far as the canvas is concerned.
    pub changed: Option<Rect>,
}

/// A bounded, linear undo history.
pub struct History {
    /// The whole layer stack of the state at the cursor. Every other state
    /// is reached from here through the deltas.
    base: LayerStack,
    states: Vec<HistoryState>,
    /// Index of the state the document currently reflects.
    cursor: usize,
    /// Maximum entries, matching Photoshop's "History States" preference.
    max_states: usize,
    /// Soft cap on the memory the recorded changes take.
    max_bytes: usize,
    /// When true, the next `push_coalescing` acts as a plain `push`.
    coalesce_sealed: bool,
}

impl History {
    /// Photoshop's factory default is 20 history states.
    pub const DEFAULT_MAX_STATES: usize = 20;
    /// 1 GiB of recorded changes before older states start being dropped.
    pub const DEFAULT_MAX_BYTES: usize = 1024 * 1024 * 1024;

    /// Create a history seeded with the document's opening state.
    pub fn new(initial: &LayerStack, size: (u32, u32)) -> Self {
        let mut base = initial.clone();
        for (copy, layer) in base.iter_mut().zip(initial.iter()) {
            share_stamps(copy, layer);
        }
        Self {
            base,
            states: vec![HistoryState {
                name: "Open".to_string(),
                size,
                color_mode: ImageMode::Rgb,
                bit_depth: 8,
                change: Vec::new(),
            }],
            cursor: 0,
            max_states: Self::DEFAULT_MAX_STATES,
            max_bytes: Self::DEFAULT_MAX_BYTES,
            coalesce_sealed: false,
        }
    }

    pub fn set_max_states(&mut self, n: usize) {
        // Always keep at least the current state.
        self.max_states = n.max(1);
        self.evict();
    }

    pub fn max_states(&self) -> usize {
        self.max_states
    }

    pub fn set_max_bytes(&mut self, n: usize) {
        self.max_bytes = n;
        self.evict();
    }

    /// Number of entries currently in the panel.
    pub fn len(&self) -> usize {
        self.states.len()
    }

    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_redo(&self) -> bool {
        self.cursor + 1 < self.states.len()
    }

    /// Label of the state that [`History::undo`] would step back *from*.
    /// Drives the "Undo Brush Tool" menu text.
    pub fn undo_name(&self) -> Option<&str> {
        if self.can_undo() {
            Some(&self.states[self.cursor].name)
        } else {
            None
        }
    }

    /// Label of the state [`History::redo`] would step forward to.
    pub fn redo_name(&self) -> Option<&str> {
        if self.can_redo() {
            Some(&self.states[self.cursor + 1].name)
        } else {
            None
        }
    }

    pub fn state_names(&self) -> Vec<&str> {
        self.states.iter().map(|s| s.name.as_str()).collect()
    }

    /// Record `live` as a new state on top of the current cursor position.
    ///
    /// Anything after the cursor is discarded — the redo branch is lost, which
    /// is what Photoshop does.
    pub fn push(
        &mut self,
        name: impl Into<String>,
        live: &LayerStack,
        size: (u32, u32),
        color_mode: ImageMode,
        bit_depth: u8,
    ) {
        self.states.truncate(self.cursor + 1);
        let back = follow(&mut self.base, live, true);
        self.states.push(HistoryState {
            name: name.into(),
            size,
            color_mode,
            bit_depth,
            change: vec![back],
        });
        self.cursor = self.states.len() - 1;
        self.evict();
    }

    /// Like [`push`](Self::push), but if the entry at the cursor already has
    /// the same name, fold into it instead of adding a new state. This is how
    /// Photoshop coalesces rapid slider-driven changes (opacity, fill) into a
    /// single undo step.
    pub fn push_coalescing(
        &mut self,
        name: impl Into<String>,
        live: &LayerStack,
        size: (u32, u32),
        color_mode: ImageMode,
        bit_depth: u8,
    ) {
        let name = name.into();
        if !self.coalesce_sealed && self.cursor > 0 && self.states[self.cursor].name == name {
            // The redo branch was built on the state being folded into; once
            // that changes, it leads nowhere.
            self.states.truncate(self.cursor + 1);
            // Back from the new state to the one being replaced, then on back
            // from there to the state before: undo takes both steps at once.
            let back = follow(&mut self.base, live, true);
            let state = &mut self.states[self.cursor];
            state.change.insert(0, back);
            state.size = size;
            state.color_mode = color_mode;
            state.bit_depth = bit_depth;
            self.evict();
        } else {
            self.coalesce_sealed = false;
            self.push(name, live, size, color_mode, bit_depth);
        }
    }

    pub fn seal_coalescing(&mut self) {
        self.coalesce_sealed = true;
    }

    /// Step back one state, bringing `live` to it. `None` at the start.
    pub fn undo(&mut self, live: &mut LayerStack) -> Option<StateInfo> {
        if !self.step_back() {
            return None;
        }
        let changed = follow_reach(live, &self.base);
        Some(self.info(changed))
    }

    /// Step forward one state, bringing `live` to it. `None` at the end.
    pub fn redo(&mut self, live: &mut LayerStack) -> Option<StateInfo> {
        if !self.step_forward() {
            return None;
        }
        let changed = follow_reach(live, &self.base);
        Some(self.info(changed))
    }

    /// Jump directly to a state, as clicking a row in the History panel does,
    /// bringing `live` to it.
    pub fn jump_to(&mut self, index: usize, live: &mut LayerStack) -> Option<StateInfo> {
        if index >= self.states.len() {
            return None;
        }
        while self.cursor > index {
            self.step_back();
        }
        while self.cursor < index {
            self.step_forward();
        }
        let changed = follow_reach(live, &self.base);
        Some(self.info(changed))
    }

    /// The stack at the cursor.
    pub fn current(&self) -> &LayerStack {
        &self.base
    }

    /// The canvas size at the cursor.
    pub fn current_size(&self) -> (u32, u32) {
        self.states[self.cursor].size
    }

    /// Bytes held by the recorded changes — not the stack at the cursor,
    /// which is the one copy the document always needs.
    pub fn byte_size(&self) -> usize {
        self.states
            .iter()
            .map(|s| s.change.iter().map(Delta::byte_size).sum::<usize>() + s.name.len())
            .sum()
    }

    fn info(&self, changed: Option<Rect>) -> StateInfo {
        let state = &self.states[self.cursor];
        StateInfo {
            size: state.size,
            color_mode: state.color_mode,
            bit_depth: state.bit_depth,
            changed,
        }
    }

    /// Move `base` back a state, keeping the way forward again.
    fn step_back(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let back = std::mem::take(&mut self.states[self.cursor].change);
        self.states[self.cursor].change = apply_all(&mut self.base, back);
        self.cursor -= 1;
        true
    }

    /// Move `base` forward a state, keeping the way back again.
    fn step_forward(&mut self) -> bool {
        if self.cursor + 1 >= self.states.len() {
            return false;
        }
        let forward = std::mem::take(&mut self.states[self.cursor + 1].change);
        self.states[self.cursor + 1].change = apply_all(&mut self.base, forward);
        self.cursor += 1;
        true
    }

    /// Drop the oldest states until both limits are satisfied.
    ///
    /// The state at the cursor is never evicted — without it the document
    /// would have nothing to restore from. The state that becomes the oldest
    /// loses its way back, since what it led back to is gone.
    fn evict(&mut self) {
        let over = |h: &History| {
            h.states.len() > h.max_states || h.byte_size() > h.max_bytes
        };
        while over(self) && self.states.len() > 1 && self.cursor > 0 {
            self.states.remove(0);
            self.states[0].change.clear();
            self.cursor -= 1;
        }
    }
}

// ------------------------------------------------------------------ deltas --

/// How one state differs from another: the settings of all its layers, in
/// order, and whichever of their pixels and masks are not what the other has.
struct Delta {
    /// The layers of the state this leads to, in order, with every setting
    /// but no pixels or masks. Small — a few hundred bytes a layer — so kept
    /// whole rather than diffed.
    shape: Vec<Layer>,
    next_id: u64,
    /// Per layer, its pixels where they are not what the other state has.
    pixels: HashMap<LayerId, Change<Pixmap>>,
    /// Per layer, its mask where it is not what the other state has.
    masks: HashMap<LayerId, Change<Option<Pixmap>>>,
}

/// A buffer as it was: only some tiles, or all of it — the second when its
/// size or depth changed, when the layer or mask came or went, or when most
/// of it changed anyway.
enum Change<T> {
    Tiles(Vec<(u32, Vec<u8>)>),
    Whole(T),
}

impl Delta {
    fn byte_size(&self) -> usize {
        let tiles = |ts: &Vec<(u32, Vec<u8>)>| ts.iter().map(|(_, b)| b.len()).sum::<usize>();
        let pixels: usize = self
            .pixels
            .values()
            .map(|c| match c {
                Change::Tiles(ts) => tiles(ts),
                Change::Whole(p) => p.byte_size(),
            })
            .sum();
        let masks: usize = self
            .masks
            .values()
            .map(|c| match c {
                Change::Tiles(ts) => tiles(ts),
                Change::Whole(m) => m.as_ref().map_or(0, Pixmap::byte_size),
            })
            .sum();
        pixels + masks + self.shape.iter().map(|l| l.name.len() + 256).sum::<usize>()
    }
}

/// Make `dst` identical to `src`, copying only the tiles that differ. With
/// `record`, returns the delta that turns the new `dst` back into the old
/// one; without, the delta is empty and nothing old is kept.
fn follow(dst: &mut LayerStack, src: &LayerStack, record: bool) -> Delta {
    follow_measured(dst, src, record).0
}

/// [`follow`] without recording, reporting instead where the document's
/// picture changed. See [`StateInfo::changed`].
fn follow_reach(dst: &mut LayerStack, src: &LayerStack) -> Option<Rect> {
    follow_measured(dst, src, false).1
}

fn follow_measured(dst: &mut LayerStack, src: &LayerStack, record: bool) -> (Delta, Option<Rect>) {
    // Pixels alone can be put back by rectangle; anything else about the
    // stack changing — which layers there are, their order, any setting —
    // can change the picture anywhere.
    let same_layers = dst.len() == src.len()
        && dst.iter().zip(src.iter()).all(|(a, b)| {
            a.id == b.id
                && format!("{:?}", a.clone_settings()) == format!("{:?}", b.clone_settings())
        });
    let mut changed = same_layers.then_some(Rect::default());
    let (old_layers, old_next) = dst.take_parts();
    let mut delta = Delta {
        shape: if record { old_layers.iter().map(Layer::clone_settings).collect() } else { Vec::new() },
        next_id: old_next,
        pixels: HashMap::new(),
        masks: HashMap::new(),
    };
    let mut old: HashMap<LayerId, Layer> = old_layers.into_iter().map(|l| (l.id, l)).collect();

    let mut layers = Vec::with_capacity(src.len());
    for wanted in src.iter() {
        let mut layer = wanted.clone_settings();
        match old.remove(&wanted.id) {
            Some(had) => {
                let (pixels, change, pixel_reach) = follow_buffer(had.pixels, &wanted.pixels, record);
                layer.pixels = pixels;
                if let Some(change) = change {
                    delta.pixels.insert(wanted.id, change);
                }
                let (mask, change, mask_reach) = follow_mask(had.mask, &wanted.mask, record);
                layer.mask = mask;
                if let Some(change) = change {
                    delta.masks.insert(wanted.id, change);
                }
                // In document pixels: a mask shares its layer's origin.
                let (dx, dy) = wanted.offset;
                changed = match (changed, pixel_reach, mask_reach) {
                    (Some(so_far), Some(p), Some(m)) => {
                        let here = p.union(&m);
                        let here = if here.is_empty() {
                            here
                        } else {
                            Rect::new(here.x + dx, here.y + dy, here.width, here.height)
                        };
                        Some(so_far.union(&here))
                    }
                    _ => None,
                };
            }
            // A layer that is new here: going back removes it, which needs
            // nothing recorded.
            None => {
                layer.pixels = wanted.pixels.clone();
                layer.mask = wanted.mask.clone();
                share_stamps(&mut layer, wanted);
            }
        }
        layers.push(layer);
    }
    // Layers gone from `src`: going back brings them back whole.
    if record {
        for (id, gone) in old {
            delta.pixels.insert(id, Change::Whole(gone.pixels));
            delta.masks.insert(id, Change::Whole(gone.mask));
        }
    }
    *dst = LayerStack::from_parts(layers, src.next_id());
    (delta, changed)
}

/// Which part of a buffer [`follow_buffer`] changed, in the buffer's own
/// pixels: `None` when its size or depth changed, so no rectangle of the old
/// one means anything.
type Reach = Option<Rect>;

/// `had` made to match `wanted`, what it was if it was different, and where.
fn follow_buffer(
    mut had: Pixmap,
    wanted: &Pixmap,
    record: bool,
) -> (Pixmap, Option<Change<Pixmap>>, Reach) {
    let copy = |wanted: &Pixmap| {
        let mut copy = wanted.clone();
        copy.share_stamp(wanted);
        copy
    };
    if !same_shape(&had, wanted) {
        return (copy(wanted), record.then_some(Change::Whole(had)), None);
    }
    // The same stamp is the same bytes: a layer this step did not touch,
    // which is most of them, costs nothing to compare. See `Pixmap::stamp`.
    if had.stamp() != 0 && had.stamp() == wanted.stamp() {
        return (had, None, Some(Rect::default()));
    }
    let changed = changed_tiles(&had, wanted);
    if changed.is_empty() {
        had.share_stamp(wanted);
        return (had, None, Some(Rect::default()));
    }
    let reach = changed
        .iter()
        .fold(Rect::default(), |r, &t| r.union(&tile_rect(&had, t)));
    // Most of it changed — a filter, a fill: keeping the old buffer whole is
    // no bigger than keeping all its tiles, and one copy rather than
    // thousands.
    if changed.len() * 2 >= tile_count(&had) {
        return (copy(wanted), record.then_some(Change::Whole(had)), Some(reach));
    }
    let recorded: Vec<(u32, Vec<u8>)> = if record {
        changed.iter().map(|&t| (t, read_tile(&had, t))).collect()
    } else {
        Vec::new()
    };
    for &t in &changed {
        copy_tile(&mut had, wanted, t);
    }
    had.share_stamp(wanted);
    (had, record.then_some(Change::Tiles(recorded)), Some(reach))
}

fn follow_mask(
    had: Option<Pixmap>,
    wanted: &Option<Pixmap>,
    record: bool,
) -> (Option<Pixmap>, Option<Change<Option<Pixmap>>>, Reach) {
    match (had, wanted) {
        (None, None) => (None, None, Some(Rect::default())),
        (Some(had), Some(wanted)) => {
            let (mask, change, reach) = follow_buffer(had, wanted, record);
            let change = change.map(|c| match c {
                Change::Tiles(ts) => Change::Tiles(ts),
                Change::Whole(p) => Change::Whole(Some(p)),
            });
            (Some(mask), change, reach)
        }
        (had, wanted) => {
            let mut mask = wanted.clone();
            if let (Some(copy), Some(wanted)) = (mask.as_mut(), wanted) {
                copy.share_stamp(wanted);
            }
            (mask, record.then_some(Change::Whole(had)), None)
        }
    }
}

/// Apply `delta` to `stack`, returning the delta that undoes it.
fn apply(stack: &mut LayerStack, mut delta: Delta) -> Delta {
    let (old_layers, old_next) = stack.take_parts();
    let mut inverse = Delta {
        shape: old_layers.iter().map(Layer::clone_settings).collect(),
        next_id: old_next,
        pixels: HashMap::new(),
        masks: HashMap::new(),
    };
    let mut old: HashMap<LayerId, Layer> = old_layers.into_iter().map(|l| (l.id, l)).collect();

    let mut layers = Vec::with_capacity(delta.shape.len());
    for mut layer in delta.shape {
        let id = layer.id;
        let had = old.remove(&id);
        let existed = had.is_some();
        let (mut pixels, mut mask) = had.map_or((Pixmap::new(0, 0), None), |l| (l.pixels, l.mask));

        match delta.pixels.remove(&id) {
            Some(Change::Tiles(tiles)) => {
                let back = swap_tiles(&mut pixels, tiles);
                inverse.pixels.insert(id, Change::Tiles(back));
            }
            Some(Change::Whole(whole)) => {
                let before = std::mem::replace(&mut pixels, whole);
                if existed {
                    inverse.pixels.insert(id, Change::Whole(before));
                }
            }
            None => {}
        }
        match delta.masks.remove(&id) {
            Some(Change::Tiles(tiles)) => {
                if let Some(m) = mask.as_mut() {
                    let back = swap_tiles(m, tiles);
                    inverse.masks.insert(id, Change::Tiles(back));
                }
            }
            Some(Change::Whole(whole)) => {
                let before = std::mem::replace(&mut mask, whole);
                if existed {
                    inverse.masks.insert(id, Change::Whole(before));
                }
            }
            None => {}
        }
        layer.pixels = pixels;
        layer.mask = mask;
        layers.push(layer);
    }
    // Layers the delta's state does not have: gone, and kept for coming back.
    for (id, gone) in old {
        inverse.pixels.insert(id, Change::Whole(gone.pixels));
        inverse.masks.insert(id, Change::Whole(gone.mask));
    }
    *stack = LayerStack::from_parts(layers, delta.next_id);
    inverse
}

/// Apply deltas in order, returning the deltas that undo them, in the order
/// they have to be applied.
fn apply_all(stack: &mut LayerStack, deltas: Vec<Delta>) -> Vec<Delta> {
    let mut inverse: Vec<Delta> = deltas.into_iter().map(|d| apply(stack, d)).collect();
    inverse.reverse();
    inverse
}

/// Mark a layer's copy as holding what the layer does, pixels and mask, so
/// the next commit need not compare them. See `Pixmap::stamp`.
fn share_stamps(copy: &mut Layer, of: &Layer) {
    copy.pixels.share_stamp(&of.pixels);
    if let (Some(copy), Some(of)) = (copy.mask.as_mut(), of.mask.as_ref()) {
        copy.share_stamp(of);
    }
}

// ------------------------------------------------------------------- tiles --

fn same_shape(a: &Pixmap, b: &Pixmap) -> bool {
    a.width() == b.width() && a.height() == b.height() && a.bpc() == b.bpc()
}

fn tiles_across(p: &Pixmap) -> u32 {
    p.width().div_ceil(TILE)
}

fn tile_count(p: &Pixmap) -> usize {
    (tiles_across(p) * p.height().div_ceil(TILE)) as usize
}

/// The byte span of each row of tile `t`: (first byte of the first row, bytes
/// per row, row count).
fn tile_span(p: &Pixmap, t: u32) -> (usize, usize, usize) {
    let across = tiles_across(p);
    let (tx, ty) = (t % across, t / across);
    let x0 = tx * TILE;
    let y0 = ty * TILE;
    let w = TILE.min(p.width() - x0) as usize;
    let h = TILE.min(p.height() - y0) as usize;
    let px = 4 * p.bpc() as usize;
    (y0 as usize * p.stride() + x0 as usize * px, w * px, h)
}

/// Which tiles of two buffers of the same shape differ. The comparison is
/// most of what a commit costs — it touches every byte — so it is spread
/// over every core, a tile at a time.
fn changed_tiles(a: &Pixmap, b: &Pixmap) -> Vec<u32> {
    let (aa, bb, stride) = (a.as_bytes(), b.as_bytes(), a.stride());
    (0..tile_count(a) as u32)
        .into_par_iter()
        .filter(|&t| {
            let (start, span, rows) = tile_span(a, t);
            (0..rows).any(|r| {
                let at = start + r * stride;
                aa[at..at + span] != bb[at..at + span]
            })
        })
        .collect()
}

/// Tile `t`'s rectangle, in the buffer's own pixels.
fn tile_rect(p: &Pixmap, t: u32) -> Rect {
    let across = tiles_across(p);
    let (x0, y0) = ((t % across) * TILE, (t / across) * TILE);
    Rect::new(x0 as i32, y0 as i32, TILE.min(p.width() - x0), TILE.min(p.height() - y0))
}

fn read_tile(p: &Pixmap, t: u32) -> Vec<u8> {
    let (start, span, rows) = tile_span(p, t);
    let stride = p.stride();
    let bytes = p.as_bytes();
    let mut out = Vec::with_capacity(span * rows);
    for r in 0..rows {
        out.extend_from_slice(&bytes[start + r * stride..][..span]);
    }
    out
}

fn write_tile(p: &mut Pixmap, t: u32, tile: &[u8]) {
    let (start, span, rows) = tile_span(p, t);
    let stride = p.stride();
    let bytes = p.as_bytes_mut();
    for r in 0..rows {
        bytes[start + r * stride..][..span].copy_from_slice(&tile[r * span..][..span]);
    }
}

fn copy_tile(dst: &mut Pixmap, src: &Pixmap, t: u32) {
    let (start, span, rows) = tile_span(src, t);
    let stride = src.stride();
    let from = src.as_bytes();
    let to = dst.as_bytes_mut();
    for r in 0..rows {
        let at = start + r * stride;
        to[at..at + span].copy_from_slice(&from[at..at + span]);
    }
}

/// Put `tiles` into `p`, returning what was there.
fn swap_tiles(p: &mut Pixmap, tiles: Vec<(u32, Vec<u8>)>) -> Vec<(u32, Vec<u8>)> {
    tiles
        .into_iter()
        .map(|(t, bytes)| {
            let before = read_tile(p, t);
            write_tile(p, t, &bytes);
            (t, before)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::{Rect, Rgba8};

    const SIZE: (u32, u32) = (16, 16);

    fn stack_with_name(name: &str) -> LayerStack {
        let mut s = LayerStack::new();
        let id = s.allocate_id();
        s.push(Layer::new_filled(id, name, 4, 4, Rgba8::WHITE));
        s
    }

    fn push(h: &mut History, name: &str, stack: &LayerStack) {
        h.push(name, stack, SIZE, ImageMode::Rgb, 8);
    }

    #[test]
    fn a_change_after_a_commit_is_found_though_the_stamps_matched() {
        // Commits skip layers whose stamp matches history's copy. Each of
        // these layers is committed untouched first, so the stamps do match;
        // then one is changed in place, and that change must still be
        // recorded, undone and redone — and the untouched one left alone.
        let mut live = LayerStack::new();
        let (a, b) = (live.allocate_id(), live.allocate_id());
        live.push(Layer::new_filled(a, "A", 600, 600, Rgba8::WHITE));
        live.push(Layer::new_filled(b, "B", 600, 600, Rgba8::BLACK));
        let mut h = History::new(&live, SIZE);
        push(&mut h, "Nothing", &live);
        let before = live.clone();

        live.by_id_mut(b).unwrap().pixels.set(300, 300, Rgba8::new(255, 0, 0, 255));
        push(&mut h, "Dab", &live);
        let after = live.clone();

        let changed = h.undo(&mut live).unwrap().changed.expect("only pixels changed");
        assert_same(&live, &before, "undone");
        assert!(changed.contains(300, 300));
        assert!(changed.width <= TILE && changed.height <= TILE, "{changed:?}");
        h.redo(&mut live).unwrap();
        assert_same(&live, &after, "redone");
    }

    /// Two stacks the same in every setting and every byte.
    fn assert_same(a: &LayerStack, b: &LayerStack, context: &str) {
        assert_eq!(a.len(), b.len(), "{context}: layer count");
        assert_eq!(a.next_id(), b.next_id(), "{context}: id counter");
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(
                format!("{:?}", x.clone_settings()),
                format!("{:?}", y.clone_settings()),
                "{context}: settings of layer {:?}",
                x.id
            );
            assert!(same_shape(&x.pixels, &y.pixels), "{context}: pixel buffer shape of {:?}", x.id);
            assert!(x.pixels.as_bytes() == y.pixels.as_bytes(), "{context}: pixels of {:?}", x.id);
            match (&x.mask, &y.mask) {
                (None, None) => {}
                (Some(m), Some(n)) => assert!(m.as_bytes() == n.as_bytes(), "{context}: mask of {:?}", x.id),
                _ => panic!("{context}: mask presence of {:?}", x.id),
            }
        }
    }

    #[test]
    fn starts_with_a_single_open_state() {
        let h = History::new(&LayerStack::new(), SIZE);
        assert_eq!(h.len(), 1);
        assert_eq!(h.cursor(), 0);
        assert!(!h.can_undo());
        assert!(!h.can_redo());
        assert_eq!(h.state_names(), vec!["Open"]);
    }

    #[test]
    fn push_advances_the_cursor() {
        let mut h = History::new(&LayerStack::new(), SIZE);
        push(&mut h, "New Layer", &stack_with_name("a"));
        assert_eq!(h.len(), 2);
        assert_eq!(h.cursor(), 1);
        assert!(h.can_undo());
        assert!(!h.can_redo());
    }

    #[test]
    fn undo_then_redo_returns_to_the_same_state() {
        let mut h = History::new(&LayerStack::new(), SIZE);
        let mut live = stack_with_name("a");
        push(&mut h, "New Layer", &live);

        assert!(h.undo(&mut live).is_some());
        assert_eq!(live.len(), 0);
        assert_eq!(h.cursor(), 0);
        assert!(h.can_redo());

        assert!(h.redo(&mut live).is_some());
        assert_eq!(live.len(), 1);
        assert_eq!(h.cursor(), 1);
    }

    #[test]
    fn undo_at_the_start_returns_none() {
        let mut h = History::new(&LayerStack::new(), SIZE);
        assert!(h.undo(&mut LayerStack::new()).is_none());
        assert_eq!(h.cursor(), 0);
    }

    #[test]
    fn redo_at_the_end_returns_none() {
        let mut h = History::new(&LayerStack::new(), SIZE);
        let mut live = stack_with_name("a");
        push(&mut h, "x", &live);
        assert!(h.redo(&mut live).is_none());
    }

    #[test]
    fn pushing_after_undo_discards_the_redo_branch() {
        let mut h = History::new(&LayerStack::new(), SIZE);
        let mut live = stack_with_name("a");
        push(&mut h, "first", &live);
        push(&mut h, "second", &stack_with_name("b"));
        h.undo(&mut live);
        assert!(h.can_redo());

        push(&mut h, "third", &stack_with_name("c"));
        assert!(!h.can_redo(), "redo branch survived a new action");
        assert_eq!(h.state_names(), vec!["Open", "first", "third"]);
    }

    #[test]
    fn undo_and_redo_names_describe_the_right_steps() {
        let mut h = History::new(&LayerStack::new(), SIZE);
        assert_eq!(h.undo_name(), None);

        let mut live = stack_with_name("a");
        push(&mut h, "Brush Tool", &live);
        assert_eq!(h.undo_name(), Some("Brush Tool"));
        assert_eq!(h.redo_name(), None);

        h.undo(&mut live);
        assert_eq!(h.redo_name(), Some("Brush Tool"));
        assert_eq!(h.undo_name(), None);
    }

    #[test]
    fn jump_to_moves_the_cursor_anywhere() {
        let mut h = History::new(&LayerStack::new(), SIZE);
        let mut live = LayerStack::new();
        push(&mut h, "a", &stack_with_name("a"));
        push(&mut h, "b", &stack_with_name("b"));

        assert!(h.jump_to(0, &mut live).is_some());
        assert_eq!(h.cursor(), 0);
        assert!(h.jump_to(2, &mut live).is_some());
        assert_eq!(h.cursor(), 2);
        assert!(h.jump_to(99, &mut live).is_none());
        assert_eq!(h.cursor(), 2, "failed jump moved the cursor");
    }

    #[test]
    fn state_count_limit_evicts_the_oldest() {
        let mut h = History::new(&LayerStack::new(), SIZE);
        h.set_max_states(3);
        for i in 0..5 {
            push(&mut h, &format!("s{}", i), &stack_with_name(&format!("x{i}")));
        }
        assert_eq!(h.len(), 3);
        // The oldest entries, including "Open", were dropped.
        assert_eq!(h.state_names(), vec!["s2", "s3", "s4"]);
        assert_eq!(h.cursor(), 2);
    }

    #[test]
    fn eviction_keeps_the_cursor_pointing_at_the_same_state() {
        let mut h = History::new(&LayerStack::new(), SIZE);
        h.set_max_states(2);
        push(&mut h, "a", &stack_with_name("a"));
        push(&mut h, "b", &stack_with_name("b"));
        // The cursor must still address the newest state.
        assert_eq!(h.state_names()[h.cursor()], "b");
    }

    #[test]
    fn the_oldest_state_left_cannot_be_undone_past() {
        // An evicted state's way back led to a state that no longer exists.
        let mut h = History::new(&LayerStack::new(), SIZE);
        h.set_max_states(2);
        let mut live = stack_with_name("a");
        push(&mut h, "a", &live);
        push(&mut h, "b", &stack_with_name("b"));
        assert!(h.undo(&mut live).is_some());
        assert_eq!(live.get(0).unwrap().name, "a");
        assert!(h.undo(&mut live).is_none());
    }

    #[test]
    fn max_states_is_never_below_one() {
        let mut h = History::new(&LayerStack::new(), SIZE);
        h.set_max_states(0);
        assert_eq!(h.max_states(), 1);
        assert!(!h.is_empty());
    }

    #[test]
    fn byte_budget_evicts_old_changes() {
        let mut h = History::new(&LayerStack::new(), SIZE);
        // Each state replaces the whole of a 64x64 RGBA layer = 16 KiB.
        let mut big = LayerStack::new();
        let id = big.allocate_id();
        big.push(Layer::new_filled(id, "big", 64, 64, Rgba8::WHITE));
        h.set_max_bytes(40 * 1024);
        for i in 0..6u8 {
            big.by_id_mut(id).unwrap().pixels.fill(Rgba8::new(i, i, i, 255));
            push(&mut h, "big", &big);
        }
        assert!(h.byte_size() <= 40 * 1024, "budget exceeded: {}", h.byte_size());
        assert!(h.len() >= 2);
    }

    #[test]
    fn current_reflects_the_cursor() {
        let mut h = History::new(&LayerStack::new(), SIZE);
        let mut live = stack_with_name("a");
        push(&mut h, "a", &live);
        assert_eq!(h.current().len(), 1);
        h.undo(&mut live);
        assert_eq!(h.current().len(), 0);
    }

    #[test]
    fn a_small_change_to_a_big_layer_costs_its_tiles() {
        // The point of all this: a dab on a large picture records a tile or
        // two, not the picture.
        let mut live = LayerStack::new();
        let id = live.allocate_id();
        live.push(Layer::new_filled(id, "big", 2000, 1500, Rgba8::WHITE));
        let mut h = History::new(&live, (2000, 1500));

        let pixels = &mut live.by_id_mut(id).unwrap().pixels;
        for y in 700..720 {
            for x in 1000..1020 {
                pixels.set(x, y, Rgba8::BLACK);
            }
        }
        h.push("Brush Tool", &live, (2000, 1500), ImageMode::Rgb, 8);
        let tile = (TILE * TILE * 4) as usize;
        assert!(h.byte_size() <= 2 * tile, "a dab recorded {} bytes", h.byte_size());

        h.undo(&mut live);
        assert_eq!(live.by_id(id).unwrap().pixels.get(1005, 705), Rgba8::WHITE);
        h.redo(&mut live);
        assert_eq!(live.by_id(id).unwrap().pixels.get(1005, 705), Rgba8::BLACK);
    }

    #[test]
    fn a_document_bigger_than_the_budget_can_still_be_undone() {
        // Full snapshots could not do this: before and after would not both
        // fit, so the state before every edit was dropped the moment it was
        // committed.
        let mut live = LayerStack::new();
        let id = live.allocate_id();
        live.push(Layer::new_filled(id, "big", 600, 600, Rgba8::WHITE));
        let mut h = History::new(&live, (600, 600));
        h.set_max_bytes(600 * 600 * 4 / 2);
        live.by_id_mut(id).unwrap().pixels.set(10, 10, Rgba8::BLACK);
        h.push("Brush Tool", &live, (600, 600), ImageMode::Rgb, 8);
        assert!(h.can_undo());
        h.undo(&mut live);
        assert_eq!(live.by_id(id).unwrap().pixels.get(10, 10), Rgba8::WHITE);
    }

    #[test]
    fn undo_says_where_the_picture_changed() {
        // What lets the canvas redraw only a stroke's tiles after an undo,
        // rather than the whole document.
        let mut live = LayerStack::new();
        let id = live.allocate_id();
        live.push(Layer::new_filled(id, "big", 2000, 1500, Rgba8::WHITE));
        live.by_id_mut(id).unwrap().offset = (100, 50);
        let mut h = History::new(&live, (2000, 1500));
        live.by_id_mut(id).unwrap().pixels.set(600, 300, Rgba8::BLACK);
        h.push("Brush Tool", &live, (2000, 1500), ImageMode::Rgb, 8);

        let changed = h.undo(&mut live).unwrap().changed.expect("only pixels changed");
        // The one tile holding (600, 300), where the layer sits.
        assert_eq!(changed, Rect::new(512 + 100, 256 + 50, 256, 256));
    }

    #[test]
    fn undoing_more_than_pixels_is_a_change_everywhere() {
        let mut live = stack_with_name("a");
        let mut h = History::new(&live, SIZE);
        live.get_mut(0).unwrap().opacity = 0.5;
        push(&mut h, "Opacity", &live);
        assert_eq!(h.undo(&mut live).unwrap().changed, None);

        let id = live.allocate_id();
        live.push(Layer::new_filled(id, "b", 4, 4, Rgba8::BLACK));
        push(&mut h, "New Layer", &live);
        assert_eq!(h.undo(&mut live).unwrap().changed, None);
    }

    /// A small deterministic generator for the model test.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self, n: u64) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 33) % n.max(1)
        }
    }

    /// Change `stack` at random in one of the ways the engine does: paint a
    /// patch, fill a layer, add, delete, reorder, resize (as Crop does), move,
    /// rename, add or drop a mask, paint the mask, change a setting.
    fn mutate(stack: &mut LayerStack, rng: &mut Rng) {
        let n = stack.len();
        let pick = |rng: &mut Rng| rng.next(n as u64) as usize;
        match if n == 0 { 2 } else { rng.next(11) } {
            0 => {
                let i = pick(rng);
                let colour = Rgba8::new(rng.next(256) as u8, rng.next(256) as u8, 7, 255);
                let layer = stack.get_mut(i).unwrap();
                let (w, h) = (layer.pixels.width(), layer.pixels.height());
                if w > 0 && h > 0 {
                    let (x, y) = (rng.next(w as u64) as i32, rng.next(h as u64) as i32);
                    layer.pixels.fill_rect(Rect::new(x, y, 30, 20), colour);
                }
            }
            1 => {
                let i = pick(rng);
                stack.get_mut(i).unwrap().pixels.fill(Rgba8::new(rng.next(256) as u8, 1, 2, 255));
            }
            2 => {
                let id = stack.allocate_id();
                let (w, h) = (100 + rng.next(500) as u32, 100 + rng.next(400) as u32);
                stack.push(Layer::new_filled(id, "new", w, h, Rgba8::new(9, 9, rng.next(256) as u8, 255)));
            }
            3 => {
                let i = pick(rng);
                stack.remove(i);
            }
            4 => {
                let (a, b) = (pick(rng), pick(rng));
                stack.reorder(a, b);
            }
            5 => {
                let i = pick(rng);
                let layer = stack.get_mut(i).unwrap();
                let (w, h) = (layer.pixels.width(), layer.pixels.height());
                if w > 2 && h > 2 {
                    layer.pixels = layer.pixels.crop(Rect::new(1, 1, w - 2, h - 2));
                }
            }
            6 => {
                let i = pick(rng);
                stack.get_mut(i).unwrap().offset.0 += 3;
            }
            7 => {
                let i = pick(rng);
                stack.get_mut(i).unwrap().name.push('*');
            }
            8 => {
                let i = pick(rng);
                let layer = stack.get_mut(i).unwrap();
                layer.mask = match layer.mask {
                    Some(_) => None,
                    None => Some(Pixmap::filled(layer.pixels.width(), layer.pixels.height(), Rgba8::WHITE)),
                };
            }
            9 => {
                let i = pick(rng);
                let layer = stack.get_mut(i).unwrap();
                if let Some(mask) = layer.mask.as_mut() {
                    let (w, h) = (mask.width().max(1), mask.height().max(1));
                    mask.fill_rect(
                        Rect::new(rng.next(w as u64) as i32, rng.next(h as u64) as i32, 40, 40),
                        Rgba8::new(0, 0, 0, 0),
                    );
                }
            }
            _ => {
                let i = pick(rng);
                let layer = stack.get_mut(i).unwrap();
                layer.opacity = rng.next(100) as f32 / 100.0;
                layer.visible = !layer.visible;
            }
        }
    }

    #[test]
    fn history_agrees_with_whole_snapshots_through_any_sequence() {
        // The model: the full-snapshot history this replaces, kept alongside.
        // Every undo, redo and jump — across layers added, deleted, cropped,
        // masked and unmasked, and steps coalesced — must land on exactly the
        // stack the model has for that state, byte for byte and setting for
        // setting.
        for seed in 0..12u64 {
            let mut rng = Rng(seed * 7919 + 1);
            let mut live = LayerStack::new();
            let id = live.allocate_id();
            live.push(Layer::new_filled(id, "background", 700, 520, Rgba8::WHITE));
            let mut h = History::new(&live, SIZE);
            // Eviction would drop states the model still has.
            h.set_max_states(1000);
            let mut model: Vec<LayerStack> = vec![live.clone()];
            let mut cursor = 0usize;
            let mut sealed = false;

            for step in 0..60 {
                let context = format!("seed {seed}, step {step}");
                match rng.next(10) {
                    0..=4 => {
                        for _ in 0..1 + rng.next(3) {
                            mutate(&mut live, &mut rng);
                        }
                        // Coalescing folds into the state at the cursor when
                        // it has the same name and nothing sealed it since.
                        if rng.next(4) == 0 {
                            let folds = !sealed && cursor > 0 && h.states[cursor].name == "same";
                            h.push_coalescing("same", &live, SIZE, ImageMode::Rgb, 8);
                            model.truncate(cursor + 1);
                            if folds {
                                model[cursor] = live.clone();
                            } else {
                                sealed = false;
                                model.push(live.clone());
                                cursor += 1;
                            }
                        } else {
                            if rng.next(2) == 0 {
                                h.seal_coalescing();
                                sealed = true;
                            }
                            h.push("step", &live, SIZE, ImageMode::Rgb, 8);
                            model.truncate(cursor + 1);
                            model.push(live.clone());
                            cursor += 1;
                        }
                    }
                    5 | 6 => {
                        if h.undo(&mut live).is_some() {
                            cursor -= 1;
                        }
                    }
                    7 | 8 => {
                        if h.redo(&mut live).is_some() {
                            cursor += 1;
                        }
                    }
                    _ => {
                        let target = rng.next(model.len() as u64) as usize;
                        h.jump_to(target, &mut live).unwrap();
                        cursor = target;
                    }
                }
                assert_eq!(h.cursor(), cursor, "{context}: cursor");
                assert_same(&live, &model[cursor], &context);
                assert_same(h.current(), &model[cursor], &format!("{context} (history's own copy)"));
            }
        }
    }
}

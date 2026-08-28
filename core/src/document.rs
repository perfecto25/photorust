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
use crate::erase::{self, BackgroundEraseOptions, BackgroundEraser};
use crate::sample::Sampling;
use crate::focus::{self, FocusOptions};
use crate::smudge::{Smudge, SmudgeOptions};
use crate::tone::{ToneOptions, ToneStroke};
use crate::bucket::{self, BucketOptions, FloodMask};
use crate::wand;
use crate::gradient::{self, Gradient, GradientOptions};
use crate::stamp::{self, CloneSampling, CloneStroke};
use crate::selection::{Selection, SelectionOp};
use crate::path::PathSet;
use crate::pattern;
use crate::slice::{Slice, SliceSet};

/// One colour picked with Replace Color's eyedropper: the RGB that was read
/// and the pixel it came from. The position is only consulted when Localized
/// Color Clusters is on.
#[derive(Clone, Copy, Debug)]
pub struct ColorSample {
    pub x: i32,
    pub y: i32,
    pub rgb: [u8; 3],
}

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

/// The image's color mode, matching Photoshop's Image > Mode submenu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageMode {
    Bitmap,
    Grayscale,
    Duotone,
    Indexed,
    Rgb,
    Cmyk,
    Lab,
    Multichannel,
}

impl ImageMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bitmap => "Bitmap",
            Self::Grayscale => "Grayscale",
            Self::Duotone => "Duotone",
            Self::Indexed => "Indexed Color",
            Self::Rgb => "RGB Color",
            Self::Cmyk => "CMYK Color",
            Self::Lab => "Lab Color",
            Self::Multichannel => "Multichannel",
        }
    }

    pub fn from_index(i: i32) -> Option<Self> {
        Some(match i {
            0 => Self::Bitmap,
            1 => Self::Grayscale,
            2 => Self::Duotone,
            3 => Self::Indexed,
            4 => Self::Rgb,
            5 => Self::Cmyk,
            6 => Self::Lab,
            7 => Self::Multichannel,
            _ => return None,
        })
    }

    pub fn to_index(self) -> i32 {
        match self {
            Self::Bitmap => 0,
            Self::Grayscale => 1,
            Self::Duotone => 2,
            Self::Indexed => 3,
            Self::Rgb => 4,
            Self::Cmyk => 5,
            Self::Lab => 6,
            Self::Multichannel => 7,
        }
    }
}

/// One open image.
pub struct Document {
    width: u32,
    height: u32,
    color_mode: ImageMode,
    bit_depth: u8,
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

    /// State for a Background Eraser stroke. Direct-to-layer for the same
    /// reason the replacer is: what a dab erases depends on what is under it.
    bg_eraser: Option<BackgroundEraser>,
    /// Where that stroke last reached, for even dab spacing.
    bg_erase_last: Option<(f32, f32)>,

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

    /// True while the document is in Quick Mask mode, where painting edits the
    /// selection instead of the image. See [`Document::set_quick_mask`].
    quick_mask: bool,

    /// The type layer the Type tool currently has open, and the visibility it
    /// had before the edit hid it. See [`Document::begin_text_edit`].
    text_edit: Option<(LayerId, bool)>,
}

// ---------------------------------------------------------------------------
// Median-cut color quantization
// ---------------------------------------------------------------------------

fn median_cut(samples: &[[u8; 3]], max_colors: usize) -> Vec<[u8; 3]> {
    if samples.is_empty() || max_colors == 0 {
        return vec![[0, 0, 0]];
    }

    let mut buckets: Vec<Vec<[u8; 3]>> = vec![samples.to_vec()];

    while buckets.len() < max_colors {
        let mut best = 0;
        let mut best_range = 0u32;
        for (i, bucket) in buckets.iter().enumerate() {
            if bucket.len() < 2 {
                continue;
            }
            for ch in 0..3 {
                let lo = bucket.iter().map(|p| p[ch]).min().unwrap_or(0);
                let hi = bucket.iter().map(|p| p[ch]).max().unwrap_or(0);
                let range = (hi - lo) as u32;
                if range > best_range {
                    best_range = range;
                    best = i;
                }
            }
        }
        if best_range == 0 {
            break;
        }

        let bucket = &buckets[best];
        let mut split_ch = 0;
        let mut split_range = 0u32;
        for ch in 0..3 {
            let lo = bucket.iter().map(|p| p[ch]).min().unwrap_or(0);
            let hi = bucket.iter().map(|p| p[ch]).max().unwrap_or(0);
            let r = (hi - lo) as u32;
            if r > split_range {
                split_range = r;
                split_ch = ch;
            }
        }

        let mut sorted = buckets.swap_remove(best);
        sorted.sort_unstable_by_key(|p| p[split_ch]);
        let mid = sorted.len() / 2;
        let right = sorted.split_off(mid);
        buckets.push(sorted);
        buckets.push(right);
    }

    buckets
        .iter()
        .map(|bucket| {
            if bucket.is_empty() {
                return [0, 0, 0];
            }
            let (mut sr, mut sg, mut sb) = (0u64, 0u64, 0u64);
            for p in bucket {
                sr += p[0] as u64;
                sg += p[1] as u64;
                sb += p[2] as u64;
            }
            let n = bucket.len() as u64;
            [(sr / n) as u8, (sg / n) as u8, (sb / n) as u8]
        })
        .collect()
}

fn nearest_color(palette: &[[u8; 3]], r: u8, g: u8, b: u8) -> [u8; 3] {
    let mut best = palette[0];
    let mut best_dist = i32::MAX;
    for &entry in palette {
        let dr = r as i32 - entry[0] as i32;
        let dg = g as i32 - entry[1] as i32;
        let db = b as i32 - entry[2] as i32;
        let dist = dr * dr + dg * dg + db * db;
        if dist < best_dist {
            best_dist = dist;
            best = entry;
        }
    }
    best
}

/// Paint a finished stroke into a selection — Quick Mask's whole mechanism.
///
/// How light the paint is decides what the pixel becomes: black masks it out,
/// white selects it, grey lands in between. That is Photoshop's rule, and it is
/// why the same brushes, erasers and gradients that paint an image can build a
/// selection.
fn paint_stroke_into_selection(
    selection: &mut Selection,
    mask: &StrokeMask,
    color: Rgba8,
    opacity: f32,
) {
    // Rec. 601 luma, the same weighting the rest of the engine uses to judge
    // brightness.
    let target = (0.299 * color.r as f32 + 0.587 * color.g as f32 + 0.114 * color.b as f32)
        / 255.0;
    let opacity = opacity.clamp(0.0, 1.0);

    let dirty = mask.dirty();
    for y in dirty.y..dirty.bottom() {
        for x in dirty.x..dirty.right() {
            let coverage = mask.coverage_at(x, y) * opacity;
            if coverage > 0.0 {
                selection.paint_at(x, y, coverage, target);
            }
        }
    }
}

/// What a paste does with the selection that was in place when it happened.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PasteMode {
    /// Ignore it: an ordinary paste lands as a layer of its own.
    Plain,
    /// Confine the pasted pixels to the selection — Photoshop's Paste Into.
    Into,
    /// Confine them to everything *but* the selection — Paste Outside.
    Outside,
}

/// Where the dabs of a direct-to-layer stroke fall between two mouse positions.
///
/// The tools that edit the layer as they go — the Color Replacement Brush and
/// the Background Eraser — have to place their own dabs, since they never build
/// a [`StrokeMask`]. `last` is where the previous dab landed, or `None` at the
/// start of a stroke. `None` comes back when the pointer has not moved far
/// enough yet: the dabs are then left for the next move rather than bunched up.
fn dab_points(brush: &Brush, last: Option<(f32, f32)>, x: f32, y: f32) -> Option<Vec<(f32, f32)>> {
    let Some((lx, ly)) = last else {
        return Some(vec![(x, y)]);
    };

    let step = (brush.size * brush.spacing.max(0.01)).max(0.5);
    let (dx, dy) = (x - lx, y - ly);
    let distance = (dx * dx + dy * dy).sqrt();
    if distance < 1e-6 {
        return None;
    }

    let mut points = Vec::new();
    let mut travelled = step;
    while travelled <= distance {
        let t = travelled / distance;
        points.push((lx + dx * t, ly + dy * t));
        travelled += step;
    }
    if points.is_empty() {
        return None;
    }
    Some(points)
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
            color_mode: ImageMode::Rgb,
            bit_depth: 8,
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
            bg_eraser: None,
            bg_erase_last: None,
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
            quick_mask: false,
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
    /// A document from a layer stack that was read from a file.
    ///
    /// The canvas size comes from the file rather than from the layers: a PSD's
    /// layers may hang off the edge of its canvas, and several may be smaller
    /// than it, so neither their union nor the largest of them is the document.
    /// An empty stack gets one transparent layer, since a document with no
    /// layers at all is a state nothing else here expects.
    pub fn from_layers(mut stack: LayerStack, width: u32, height: u32) -> Self {
        if stack.is_empty() {
            let id = stack.allocate_id();
            stack.push(Layer::new_raster(id, "Background", width, height));
        }
        // Photoshop selects the top layer on open, which is also the one the
        // panel highlights.
        let active_layer = stack
            .as_slice()
            .last()
            .map_or(LayerId::NONE, |layer| layer.id);

        let history = History::new(stack.clone(), (width, height));
        Self {
            width,
            height,
            color_mode: ImageMode::Rgb,
            bit_depth: 8,
            stack,
            selection: Selection::new(width, height),
            history,
            slices: SliceSet::new(),
            annotations: Annotations::new(),
            paths: PathSet::new(),
            active_layer,
            stroke: None,
            stroke_undo_base: None,
            replacer: None,
            replace_last: None,
            bg_eraser: None,
            bg_erase_last: None,
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
            quick_mask: false,
            text_edit: None,
        }
    }

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
            color_mode: ImageMode::Rgb,
            bit_depth: 8,
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
            bg_eraser: None,
            bg_erase_last: None,
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
            quick_mask: false,
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

    pub fn color_mode(&self) -> ImageMode {
        self.color_mode
    }

    pub fn bit_depth(&self) -> u8 {
        self.bit_depth
    }

    pub fn set_color_mode(&mut self, mode: ImageMode) {
        if mode == self.color_mode {
            return;
        }
        let old = self.color_mode;
        self.color_mode = mode;

        if mode == ImageMode::Grayscale && old != ImageMode::Grayscale {
            for layer in self.stack.iter_mut() {
                let (w, h) = (layer.pixels.width(), layer.pixels.height());
                for y in 0..h as i32 {
                    for x in 0..w as i32 {
                        let px = layer.pixels.get(x, y);
                        let gray = ((px.r as u32 * 299
                            + px.g as u32 * 587
                            + px.b as u32 * 114)
                            / 1000) as u8;
                        layer.pixels.set(x, y, Rgba8::new(gray, gray, gray, px.a));
                    }
                }
            }
        }

        if mode == ImageMode::Cmyk {
            for layer in self.stack.iter_mut() {
                let (w, h) = (layer.pixels.width(), layer.pixels.height());
                for y in 0..h as i32 {
                    for x in 0..w as i32 {
                        let px = layer.pixels.get(x, y);
                        if px.a == 0 {
                            continue;
                        }
                        let (r, g, b) = if old == ImageMode::Grayscale {
                            (px.r, px.r, px.r)
                        } else {
                            (px.r, px.g, px.b)
                        };
                        let rf = r as f32 / 255.0;
                        let gf = g as f32 / 255.0;
                        let bf = b as f32 / 255.0;
                        let k = 1.0 - rf.max(gf).max(bf);
                        let (c, m, y_val) = if k >= 1.0 {
                            (0.0, 0.0, 0.0)
                        } else {
                            let inv = 1.0 / (1.0 - k);
                            ((1.0 - rf - k) * inv, (1.0 - gf - k) * inv, (1.0 - bf - k) * inv)
                        };
                        let ro = ((1.0 - c) * (1.0 - k) * 255.0 + 0.5) as u8;
                        let go = ((1.0 - m) * (1.0 - k) * 255.0 + 0.5) as u8;
                        let bo = ((1.0 - y_val) * (1.0 - k) * 255.0 + 0.5) as u8;
                        layer.pixels.set(x, y, Rgba8::new(ro, go, bo, px.a));
                    }
                }
            }
        }

        self.commit(mode.as_str());
    }

    /// Convert to indexed color with median-cut quantization and optional
    /// Floyd-Steinberg dithering. `dither_amount` is 0–100 (0 = no dither).
    pub fn convert_to_indexed(&mut self, max_colors: u32, dither_amount: u32) {
        let old = self.color_mode;
        self.color_mode = ImageMode::Indexed;

        for layer in self.stack.iter_mut() {
            let (w, h) = (layer.pixels.width(), layer.pixels.height());
            if w == 0 || h == 0 {
                continue;
            }

            // Collect opaque pixels for palette building
            let mut samples: Vec<[u8; 3]> = Vec::new();
            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let px = layer.pixels.get(x, y);
                    if px.a > 0 {
                        samples.push([px.r, px.g, px.b]);
                    }
                }
            }

            let palette = median_cut(&samples, max_colors.min(256) as usize);

            if dither_amount > 0 {
                // Floyd-Steinberg dithering
                let strength = dither_amount as f32 / 100.0;
                let mut errors: Vec<[f32; 3]> = vec![[0.0; 3]; (w * h) as usize];

                for y in 0..h as i32 {
                    for x in 0..w as i32 {
                        let px = layer.pixels.get(x, y);
                        if px.a == 0 {
                            continue;
                        }
                        let idx = (y as usize) * (w as usize) + (x as usize);
                        let r = (px.r as f32 + errors[idx][0] * strength).clamp(0.0, 255.0);
                        let g = (px.g as f32 + errors[idx][1] * strength).clamp(0.0, 255.0);
                        let b = (px.b as f32 + errors[idx][2] * strength).clamp(0.0, 255.0);

                        let nearest = nearest_color(&palette, r as u8, g as u8, b as u8);
                        layer.pixels.set(x, y, Rgba8::new(nearest[0], nearest[1], nearest[2], px.a));

                        let er = r - nearest[0] as f32;
                        let eg = g - nearest[1] as f32;
                        let eb = b - nearest[2] as f32;

                        let distribute = |errors: &mut Vec<[f32; 3]>, idx: usize, f: f32| {
                            errors[idx][0] += er * f;
                            errors[idx][1] += eg * f;
                            errors[idx][2] += eb * f;
                        };

                        if x + 1 < w as i32 {
                            distribute(&mut errors, idx + 1, 7.0 / 16.0);
                        }
                        if y + 1 < h as i32 {
                            let next_row = idx + w as usize;
                            if x > 0 {
                                distribute(&mut errors, next_row - 1, 3.0 / 16.0);
                            }
                            distribute(&mut errors, next_row, 5.0 / 16.0);
                            if x + 1 < w as i32 {
                                distribute(&mut errors, next_row + 1, 1.0 / 16.0);
                            }
                        }
                    }
                }
            } else {
                // No dithering — snap each pixel to the nearest palette entry
                for y in 0..h as i32 {
                    for x in 0..w as i32 {
                        let px = layer.pixels.get(x, y);
                        if px.a == 0 {
                            continue;
                        }
                        let nearest = nearest_color(&palette, px.r, px.g, px.b);
                        layer.pixels.set(x, y, Rgba8::new(nearest[0], nearest[1], nearest[2], px.a));
                    }
                }
            }
        }

        self.commit(if old == self.color_mode {
            "Indexed Color"
        } else {
            ImageMode::Indexed.as_str()
        });
    }

    pub fn set_bit_depth(&mut self, depth: u8) {
        if depth == self.bit_depth {
            return;
        }
        let bpc: u8 = match depth {
            16 => 2,
            32 => 4,
            _ => 1,
        };
        for layer in self.stack.iter_mut() {
            layer.pixels = layer.pixels.convert_depth(bpc);
            if let Some(ref mask) = layer.mask {
                layer.mask = Some(mask.convert_depth(bpc));
            }
        }
        self.bit_depth = depth;
        self.commit(&format!("{} Bits/Channel", depth));
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
        let mode_label = match self.color_mode {
            ImageMode::Rgb => "RGB",
            ImageMode::Grayscale => "Gray",
            ImageMode::Cmyk => "CMYK",
            ImageMode::Lab => "Lab",
            ImageMode::Bitmap => "Bitmap",
            ImageMode::Duotone => "Duotone",
            ImageMode::Indexed => "Indexed",
            ImageMode::Multichannel => "Multi",
        };
        let dirty = if self.dirty { "*" } else { "" };
        format!("{}{} ({}/{})", base, dirty, mode_label, self.bit_depth)
    }

    // -- layers -------------------------------------------------------------

    pub fn layers(&self) -> &LayerStack {
        &self.stack
    }

    pub fn layers_mut_raw(&mut self) -> &mut LayerStack {
        &mut self.stack
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

    /// Add a shape layer above the active one — what the shape tools commit in
    /// CS6's Shape mode.
    ///
    /// Photoshop builds one of these as a solid colour clipped by a vector
    /// mask, and so does this: a [`LayerKind::SolidColor`] layer carrying a mask
    /// cut to the shape. The compositor already honours a mask on a solid
    /// colour, so nothing new is needed to draw it, and the layer stays a real
    /// fill layer — its colour can be changed after the fact.
    ///
    /// What is *not* kept is the geometry as a live path: the shape is
    /// rasterized into the mask, so it can be masked, moved and recoloured
    /// afterwards but not reshaped by dragging its corners. That needs a vector
    /// mask on the layer, which the layer model does not have yet.
    pub fn add_shape_layer(
        &mut self,
        points: &[(f32, f32)],
        color: Rgba8,
        name: &str,
    ) -> Option<LayerId> {
        if points.len() < 3 {
            return None;
        }

        let mut coverage = Selection::new(self.width, self.height);
        coverage.apply_polygons_feathered(&[points.to_vec()], SelectionOp::Replace, 0);

        let id = self.stack.allocate_id();
        let mut layer = Layer::new_raster(id, self.stack.suggest_shape_name(name), 0, 0);
        layer.kind = LayerKind::SolidColor(color);

        // The mask is canvas-sized and starts at the origin, so the layer's own
        // offset is zero and `mask_at` lines up with document space.
        let mut mask = Pixmap::new(self.width, self.height);
        for y in 0..self.height as i32 {
            for x in 0..self.width as i32 {
                let a = (coverage.coverage_at(x, y) * 255.0 + 0.5) as u8;
                mask.set(x, y, Rgba8::new(a, a, a, a));
            }
        }
        layer.mask = Some(mask);

        let at = self.active_index().map_or(self.stack.len(), |i| i + 1);
        self.stack.insert(at, layer);
        self.active_layer = id;
        self.commit("Shape Layer");
        Some(id)
    }

    /// Add a shape to the work path — the shape tools' Path mode.
    ///
    /// The subpath is closed, since a dragged shape encloses an area, and left
    /// not being edited: the Pen tool would otherwise carry on extending it
    /// from the last corner.
    pub fn append_shape_path(&mut self, points: &[(f32, f32)]) -> bool {
        if points.len() < 3 {
            return false;
        }
        let path = self.paths.ensure_active();
        for (x, y) in points {
            path.append_corner(*x, *y);
        }
        path.close_active_subpath();
        path.finish_editing();
        true
    }

    /// Paint a shape onto the active layer — the shape tools' Pixels mode.
    pub fn fill_shape(&mut self, points: &[(f32, f32)], color: Rgba8, opacity: f32) -> Rect {
        if points.len() < 3 {
            return Rect::default();
        }
        let dirty = self.fill_polygons(&[points.to_vec()], color, opacity);
        if dirty.is_empty() {
            return Rect::default();
        }
        self.commit("Shape Tool");
        dirty
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

    /// Add a layer of already-decoded pixels above the active one and select
    /// it — an animated GIF's frames, say.
    ///
    /// `offset` places the pixels in document space, so a layer smaller than
    /// the canvas can sit anywhere on it.
    pub fn add_image_layer(&mut self, pixels: Pixmap, offset: (i32, i32), name: String) -> LayerId {
        let id = self.stack.allocate_id();
        let mut layer = Layer::new_raster(id, name, 0, 0);
        layer.pixels = pixels;
        layer.offset = offset;

        let at = self.active_index().map_or(self.stack.len(), |i| i + 1);
        self.stack.insert(at, layer);
        self.active_layer = id;
        self.commit("New Layer");
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
        // An unnamed one is a layer that has nothing in it yet — the empty
        // layer Photoshop puts down the moment the Type tool is clicked, before
        // there is any text to name it after.
        let name = if name.is_empty() {
            self.stack.suggest_name()
        } else {
            name
        };
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
            self.commit_coalescing("Layer Opacity");
        }
    }

    pub fn set_layer_fill_opacity(&mut self, id: LayerId, opacity: f32) {
        if let Some(l) = self.stack.by_id_mut(id) {
            l.fill_opacity = opacity.clamp(0.0, 1.0);
            self.commit_coalescing("Fill Opacity");
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
    pub fn rasterize_type(&mut self, id: LayerId) {
        if let Some(l) = self.stack.by_id_mut(id) {
            if l.text.is_none() {
                return;
            }
            l.text = None;

            let (ox, oy) = l.offset;
            let old = &l.pixels;
            let ow = old.width();
            let oh = old.height();

            let mut expanded = Pixmap::new(self.width, self.height);
            for sy in 0..oh as i32 {
                let dy = sy + oy;
                if dy < 0 || dy >= self.height as i32 {
                    continue;
                }
                for sx in 0..ow as i32 {
                    let dx = sx + ox;
                    if dx < 0 || dx >= self.width as i32 {
                        continue;
                    }
                    expanded.set(dx, dy, old.get(sx, sy));
                }
            }
            l.pixels = expanded;
            l.offset = (0, 0);

            self.commit("Rasterize Type");
        }
    }

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
            self.commit_coalescing("Move Layer");
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

    /// Begin a Pattern Stamp stroke — the Clone Stamp's other half.
    ///
    /// It is the same stroke with a different thing under it: instead of a
    /// snapshot of the image offset by an Alt-click, the source is the chosen
    /// pattern repeated across the layer, read straight through at the pixel
    /// the brush is over. So this reuses [`CloneStroke`] with a zero offset,
    /// and everything downstream — dabs, opacity, flow, the selection, the
    /// transparency lock, the live preview, the single history state — is
    /// already right.
    ///
    /// `aligned` is CS6's checkbox: the tile is pinned to the document, so
    /// separate strokes join up as though uncovering one continuous sheet.
    /// Unaligned pins it to wherever each stroke starts instead, so every
    /// stroke begins mid-tile at the same place.
    pub fn begin_pattern_stroke(
        &mut self,
        brush: &Brush,
        x: f32,
        y: f32,
        pressure: f32,
        pattern: usize,
        aligned: bool,
    ) -> bool {
        let Some(layer) = self.active_layer() else {
            return false;
        };
        if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
            return false;
        }

        // The source lives in the layer's frame, so a tile pinned to the
        // document starts at wherever the document's origin falls in it.
        let offset = layer.offset;
        let size = (layer.pixels.width(), layer.pixels.height());
        let origin = if aligned {
            (-offset.0, -offset.1)
        } else {
            (x.round() as i32 - offset.0, y.round() as i32 - offset.1)
        };

        let Some(source) = pattern::tiled(pattern, size, origin) else {
            return false;
        };
        if !self.begin_stroke(brush, x, y, pressure) {
            return false;
        }
        self.clone = Some(CloneStroke { source, offset: (0, 0) });
        true
    }

    /// Whether a stroke that copies *pixels* is in progress — the Clone Stamp
    /// or the Pattern Stamp — and so whether ending it should composite those
    /// pixels rather than paint a colour.
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

        // In Quick Mask the stroke belongs to the selection, not to any layer,
        // and so does its undo step: the pixels are untouched.
        if self.quick_mask {
            paint_stroke_into_selection(&mut self.selection, &mask, color, opacity);
            self.dirty = true;
            return mask.dirty();
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

        let Some(points) = dab_points(brush, self.replace_last, x, y) else {
            return Rect::default();
        };

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

    // -- background eraser ---------------------------------------------------

    /// Begin a Background Eraser stroke.
    ///
    /// Built exactly like the Color Replacement Brush's, and for the same
    /// reason: what a dab erases depends on what the dabs before it left, so it
    /// edits the layer as it goes rather than accumulating into a stroke mask.
    /// The whole drag is still one history state.
    ///
    /// `reference` is the colour to erase for the Once and Background Swatch
    /// sampling modes; Continuous ignores it and reads under the crosshair.
    pub fn begin_background_erase(
        &mut self,
        brush: &Brush,
        options: BackgroundEraseOptions,
        reference: Option<Rgba8>,
        foreground: Rgba8,
        x: f32,
        y: f32,
        pressure: f32,
    ) -> bool {
        let Some(layer) = self.active_layer() else {
            return false;
        };
        if layer.lock_pixels || layer.lock_transparency || !matches!(layer.kind, LayerKind::Raster)
        {
            // Erasing is exactly what Lock Transparent Pixels forbids: it can
            // only ever change alpha.
            return false;
        }

        self.stroke_undo_base = Some(self.stack.clone());
        let reference = match options.sampling {
            Sampling::Continuous => None,
            _ => reference,
        };
        self.bg_eraser = Some(BackgroundEraser::new(options, reference));
        self.bg_erase_last = None;
        self.extend_background_erase(brush, x, y, pressure, foreground);
        true
    }

    /// Continue a Background Eraser stroke, laying dabs to `(x, y)`.
    pub fn extend_background_erase(
        &mut self,
        brush: &Brush,
        x: f32,
        y: f32,
        pressure: f32,
        foreground: Rgba8,
    ) -> Rect {
        if self.bg_eraser.is_none() {
            return Rect::default();
        }
        let id = self.active_layer;
        let offset = match self.stack.by_id(id) {
            Some(layer) => layer.offset,
            None => return Rect::default(),
        };

        let Some(points) = dab_points(brush, self.bg_erase_last, x, y) else {
            return Rect::default();
        };

        let selection = if self.selection.is_empty() {
            None
        } else {
            Some(self.selection.clone())
        };

        let mut dirty = Rect::default();
        let (Some(eraser), Some(layer)) = (self.bg_eraser.as_mut(), self.stack.by_id_mut(id))
        else {
            return Rect::default();
        };

        for (px, py) in points {
            let touched = eraser.apply_dab(
                &mut layer.pixels,
                brush,
                px - offset.0 as f32,
                py - offset.1 as f32,
                pressure,
                foreground,
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

        // A marquee confines this exactly as it confines painting: what fell
        // outside is put back, which keeps the eraser's own logic free of
        // selection handling.
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

        self.bg_erase_last = Some((x, y));
        dirty
    }

    /// Finish a Background Eraser stroke, recording it as one undo step.
    pub fn end_background_erase(&mut self) -> bool {
        if self.bg_eraser.take().is_none() {
            return false;
        }
        self.bg_erase_last = None;
        self.stroke_undo_base = None;
        self.commit("Background Eraser");
        true
    }

    /// Abandon a Background Eraser stroke, restoring what it changed.
    pub fn cancel_background_erase(&mut self) {
        self.bg_eraser = None;
        self.bg_erase_last = None;
        if let Some(base) = self.stroke_undo_base.take() {
            self.stack = base;
        }
    }

    // -- quick mask ----------------------------------------------------------

    pub fn quick_mask(&self) -> bool {
        self.quick_mask
    }

    /// Enter or leave Quick Mask mode.
    ///
    /// A selection and a greyscale mask are the same thing — coverage per pixel
    /// — so there is nothing to convert on the way in or out. What changes is
    /// where a brush stroke goes: in Quick Mask it paints the selection rather
    /// than the layer, which is what lets a selection be built with a soft
    /// brush, an eraser or a gradient.
    ///
    /// The two ends translate between the mask's world and the tools':
    ///
    /// - going in, "no marquee" becomes an explicit full selection, or the
    ///   first black stroke would have nothing to subtract from;
    /// - coming out, a mask that ended up covering everything becomes "no
    ///   marquee" again, since every tool treats those the same and the
    ///   marching ants round the whole canvas would be noise.
    pub fn set_quick_mask(&mut self, on: bool) {
        if self.quick_mask == on {
            return;
        }
        self.quick_mask = on;

        if on {
            if self.selection.is_empty() {
                self.selection.select_all();
            }
        } else if self.selection.is_full() {
            self.selection.clear();
        }
        self.dirty = true;
    }

    /// Paint the stroke in progress into a copy of the selection — what the
    /// Quick Mask overlay shows while a stroke is being drawn.
    ///
    /// Returns `None` when there is nothing in progress, so the caller can use
    /// the live selection and skip the copy.
    pub fn quick_mask_preview(&self, color: Rgba8, opacity: f32) -> Option<Selection> {
        let mask = self.stroke.as_ref()?;
        let mut preview = self.selection.clone();
        paint_stroke_into_selection(&mut preview, mask, color, opacity);
        Some(preview)
    }

    /// Erase the region a click lands in — the Magic Eraser.
    ///
    /// The same flood the Magic Wand selects, erased instead. `sample_all` reads
    /// the composite to decide the region, which lets a click follow what the
    /// user can see even though only the active layer is erased — Photoshop's
    /// Sample All Layers.
    #[allow(clippy::too_many_arguments)]
    pub fn magic_erase(
        &mut self,
        x: i32,
        y: i32,
        tolerance: u32,
        contiguous: bool,
        antialias: bool,
        sample_all: bool,
        opacity: f32,
    ) -> Rect {
        let Some(layer) = self.active_layer() else {
            return Rect::default();
        };
        if layer.lock_pixels || layer.lock_transparency || !matches!(layer.kind, LayerKind::Raster)
        {
            return Rect::default();
        }
        let (id, offset) = (layer.id, layer.offset);

        // What the click means is read off the image, either the composite or
        // the layer alone, placed in document space either way.
        let source = if sample_all {
            self.composite()
        } else {
            let mut placed = Pixmap::new(self.width, self.height);
            for py in 0..layer.pixels.height() as i32 {
                for px in 0..layer.pixels.width() as i32 {
                    placed.set(px + offset.0, py + offset.1, layer.pixels.get(px, py));
                }
            }
            placed
        };

        let mut mask =
            wand::magic_wand(&source, (x, y), tolerance.min(255), contiguous, antialias);

        // A marquee confines the erase the way it confines a fill. Applied to
        // the mask rather than to the pixels afterwards, so a partly selected
        // edge is partly erased.
        if !self.selection.is_empty() {
            for my in 0..self.height as i32 {
                for mx in 0..self.width as i32 {
                    let index = my as usize * self.width as usize + mx as usize;
                    let coverage = self.selection.coverage_at(mx, my);
                    mask[index] = (mask[index] as f32 * coverage) as u8;
                }
            }
        }

        let Some(layer) = self.stack.by_id_mut(id) else {
            return Rect::default();
        };
        let dirty = erase::erase_through_mask(&mut layer.pixels, &mask, self.width, offset, opacity);
        if !dirty.is_empty() {
            self.commit("Magic Eraser");
        }
        dirty
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

    /// Fill with a colour at a given opacity (0..1), respecting selection and
    /// lock-transparency. Used by the Fill dialog.
    pub fn fill_with_opacity(&mut self, color: Rgba8, opacity: f32, mode: BlendMode) {
        self.fill_pixels(opacity, mode, |_x, _y| color);
    }

    pub fn fill_with_pattern(&mut self, pattern_index: usize, opacity: f32, mode: BlendMode) {
        let tile = match pattern::tile(pattern_index) {
            Some(t) => t,
            None => return,
        };
        let (tw, th) = (tile.width() as i32, tile.height() as i32);
        if tw <= 0 || th <= 0 {
            return;
        }
        self.fill_pixels(opacity, mode, |x, y| {
            tile.get(x.rem_euclid(tw), y.rem_euclid(th))
        });
    }

    fn fill_pixels<F>(&mut self, opacity: f32, mode: BlendMode, src_at: F)
    where
        F: Fn(i32, i32) -> Rgba8,
    {
        use crate::blend::blend_rgb;

        let opacity = opacity.clamp(0.0, 1.0);
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
                    let mut alpha = opacity;
                    if let Some(sel) = &selection {
                        alpha *= sel.coverage_at(x + offset.0, y + offset.1);
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
                    let src = src_at(x, y);
                    let blended = if matches!(mode, BlendMode::Normal) {
                        src
                    } else {
                        let db = [dst.r as f32 / 255.0, dst.g as f32 / 255.0, dst.b as f32 / 255.0];
                        let sb = [src.r as f32 / 255.0, src.g as f32 / 255.0, src.b as f32 / 255.0];
                        let rb = blend_rgb(mode, db, sb);
                        Rgba8::new(
                            (rb[0] * 255.0 + 0.5) as u8,
                            (rb[1] * 255.0 + 0.5) as u8,
                            (rb[2] * 255.0 + 0.5) as u8,
                            src.a,
                        )
                    };
                    layer
                        .pixels
                        .set(x, y, crate::brush::source_over(dst, blended, alpha));
                }
            }
        }
        self.commit("Fill");
    }

    /// Stroke the selection boundary with a given colour, width, opacity, and
    /// location (0 = inside, 1 = center, 2 = outside).
    pub fn stroke_selection(
        &mut self,
        color: Rgba8,
        width: i32,
        opacity: f32,
        location: i32,
    ) {
        if width < 1 {
            return;
        }
        let opacity = opacity.clamp(0.0, 1.0);
        let id = self.active_layer;

        if self.selection.is_empty() {
            // No selection — stroke the canvas boundary (like Photoshop).
            if let Some(layer) = self.stack.by_id_mut(id) {
                if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
                    return;
                }
                let lock_alpha = layer.lock_transparency;
                let (lw, lh) = (layer.pixels.width(), layer.pixels.height());

                let effective = match location {
                    0 => width as f32,
                    2 => 0.0,
                    _ => width as f32 / 2.0,
                };

                for y in 0..lh as i32 {
                    for x in 0..lw as i32 {
                        let dist = (x as f32)
                            .min(y as f32)
                            .min((lw as i32 - 1 - x) as f32)
                            .min((lh as i32 - 1 - y) as f32);
                        if dist >= effective {
                            continue;
                        }
                        let mut alpha = opacity;
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
            self.commit("Stroke");
            return;
        }

        let sel = self.selection.clone();

        if let Some(layer) = self.stack.by_id_mut(id) {
            if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
                return;
            }
            let offset = layer.offset;
            let lock_alpha = layer.lock_transparency;
            let (lw, lh) = (layer.pixels.width(), layer.pixels.height());

            for y in 0..lh as i32 {
                for x in 0..lw as i32 {
                    let gx = x + offset.0;
                    let gy = y + offset.1;
                    let cov = sel.coverage_at(gx, gy);

                    let mut is_edge = false;
                    for dy in -1..=1_i32 {
                        for dx in -1..=1_i32 {
                            if dx == 0 && dy == 0 {
                                continue;
                            }
                            let nc = sel.coverage_at(gx + dx, gy + dy);
                            if (cov > 0.5) != (nc > 0.5) {
                                is_edge = true;
                                break;
                            }
                        }
                        if is_edge {
                            break;
                        }
                    }

                    if !is_edge {
                        if width > 1 {
                            let half = width as f32 / 2.0;
                            let mut min_dist = half + 1.0;
                            let search = width + 1;
                            for dy in -search..=search {
                                for dx in -search..=search {
                                    let nc = sel.coverage_at(gx + dx, gy + dy);
                                    if (cov > 0.5) != (nc > 0.5) {
                                        let d = ((dx * dx + dy * dy) as f32).sqrt();
                                        if d < min_dist {
                                            min_dist = d;
                                        }
                                    }
                                }
                            }
                            let inside_sel = cov > 0.5;
                            let in_stroke = match location {
                                0 => inside_sel && min_dist <= width as f32,
                                2 => !inside_sel && min_dist <= width as f32,
                                _ => min_dist <= half,
                            };
                            if !in_stroke {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    } else {
                        let inside_sel = cov > 0.5;
                        match location {
                            0 if !inside_sel => continue,
                            2 if inside_sel => continue,
                            _ => {}
                        }
                    }

                    let mut alpha = opacity;
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
        self.commit("Stroke");
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
    // -- the clipboard --------------------------------------------------------

    /// The pixels a Copy takes: what the selection covers, cut out of the
    /// active layer or of the whole visible image.
    ///
    /// Comes back with its place in the document, since where a copy came from
    /// is what Paste in Place needs. Partly selected pixels come out partly
    /// transparent, so a feathered selection copies with a soft edge — the same
    /// coverage that governs every other selection-aware operation here.
    ///
    /// `None` when there is nothing to copy: no selection, or a selection that
    /// falls entirely outside the layer.
    pub fn copy_selection(&mut self, merged: bool) -> Option<(Pixmap, (i32, i32))> {
        if self.selection.is_empty() {
            return None;
        }
        let bounds = self.selection.bounds().intersect(&Rect::from_size(self.width, self.height));
        if bounds.is_empty() {
            return None;
        }

        // Merged copies the composite; an ordinary copy takes the active layer
        // alone, which is the difference between the two menu entries.
        let source = if merged {
            self.composite()
        } else {
            let layer = self.active_layer()?;
            let offset = layer.offset;
            let mut placed = Pixmap::new(self.width, self.height);
            for y in 0..layer.pixels.height() as i32 {
                for x in 0..layer.pixels.width() as i32 {
                    placed.set(x + offset.0, y + offset.1, layer.pixels.get(x, y));
                }
            }
            placed
        };

        let mut out = Pixmap::new(bounds.width, bounds.height);
        for y in 0..bounds.height as i32 {
            for x in 0..bounds.width as i32 {
                let (doc_x, doc_y) = (bounds.x + x, bounds.y + y);
                let coverage = self.selection.coverage_at(doc_x, doc_y);
                if coverage <= 0.0 {
                    continue;
                }
                let mut px = source.get(doc_x, doc_y);
                px.a = (px.a as f32 * coverage).round().clamp(0.0, 255.0) as u8;
                out.set(x, y, px);
            }
        }
        Some((out, (bounds.x, bounds.y)))
    }

    /// How a paste is confined by the selection that was in place.
    pub fn paste_into(&mut self, pixels: Pixmap, offset: (i32, i32), mode: PasteMode) -> LayerId {
        let id = self.stack.allocate_id();
        let mut layer = Layer::new_raster(id, self.stack.suggest_shape_name("Layer"), 0, 0);
        layer.pixels = pixels;
        layer.offset = offset;

        // Paste Into and Paste Outside are the same paste wearing the selection
        // as a mask — which is exactly what Photoshop makes: a layer plus a
        // layer mask, so the pasted pixels can still be moved about inside it
        // afterwards.
        if mode != PasteMode::Plain && !self.selection.is_empty() {
            let mut mask = Pixmap::new(self.width, self.height);
            for y in 0..self.height as i32 {
                for x in 0..self.width as i32 {
                    let mut coverage = self.selection.coverage_at(x, y);
                    if mode == PasteMode::Outside {
                        coverage = 1.0 - coverage;
                    }
                    let a = (coverage * 255.0 + 0.5) as u8;
                    mask.set(x, y, Rgba8::new(a, a, a, a));
                }
            }
            // The mask is canvas-sized and starts at the origin, so the layer's
            // own offset has to be too, and the pixels move instead.
            let mut placed = Pixmap::new(self.width, self.height);
            for y in 0..layer.pixels.height() as i32 {
                for x in 0..layer.pixels.width() as i32 {
                    placed.set(x + offset.0, y + offset.1, layer.pixels.get(x, y));
                }
            }
            layer.pixels = placed;
            layer.offset = (0, 0);
            layer.mask = Some(mask);
        }

        let at = self.active_index().map_or(self.stack.len(), |i| i + 1);
        self.stack.insert(at, layer);
        self.active_layer = id;
        self.commit("Paste");
        id
    }

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
    /// Apply an adjustment to the active layer.
    ///
    /// A selection confines it: only the selected pixels change, blended by
    /// the selection's coverage so a feathered edge fades the adjustment in
    /// rather than showing a hard border.
    pub fn apply_adjustment(&mut self, adjustment: Adjustment) {
        let selection = if self.selection.is_empty() {
            None
        } else {
            Some(&self.selection)
        };

        let id = self.active_layer;
        if let Some(layer) = self.stack.by_id_mut(id) {
            if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
                return;
            }
            match selection {
                None => adjustment.apply_to(&mut layer.pixels),
                Some(sel) => {
                    // The layer may hang off the canvas, so its pixels have to
                    // be put back into document space before the selection can
                    // be asked about them.
                    let (ox, oy) = layer.offset;
                    let w = layer.pixels.width();
                    for (i, px) in layer.pixels.as_bytes_mut().chunks_exact_mut(4).enumerate() {
                        if px[3] == 0 {
                            continue;
                        }
                        let x = (i as u32 % w) as i32 + ox;
                        let y = (i as u32 / w) as i32 + oy;
                        let coverage = sel.coverage_at(x, y);
                        if coverage <= 0.0 {
                            continue;
                        }

                        let c = [
                            px[0] as f32 / 255.0,
                            px[1] as f32 / 255.0,
                            px[2] as f32 / 255.0,
                        ];
                        let out = adjustment.apply_rgb(c);
                        for ch in 0..3 {
                            let v = c[ch] + (out[ch] - c[ch]) * coverage;
                            px[ch] = (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                        }
                    }
                }
            }
        }
        self.commit(adjustment.name());
    }

    /// Apply levels to a single channel (0=R, 1=G, 2=B) on the active layer.
    pub fn apply_levels_channel(
        &mut self,
        channel: usize,
        in_black: f32,
        in_white: f32,
        gamma: f32,
        out_black: f32,
        out_white: f32,
    ) {
        let id = self.active_layer;
        if let Some(layer) = self.stack.by_id_mut(id) {
            if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
                return;
            }
            let span = (in_white - in_black).max(1e-6);
            let inv_gamma = 1.0 / gamma.max(1e-6);
            for px in layer.pixels.as_bytes_mut().chunks_exact_mut(4) {
                if px[3] == 0 {
                    continue;
                }
                let v = px[channel] as f32 / 255.0;
                let n = ((v - in_black) / span).clamp(0.0, 1.0).powf(inv_gamma);
                let out = (out_black + n * (out_white - out_black)).clamp(0.0, 1.0);
                px[channel] = (out * 255.0 + 0.5) as u8;
            }
        }
        self.commit("Levels");
    }

    /// Apply a 256-entry LUT curve to the active layer.
    /// channel: 0 = all RGB, 1 = R only, 2 = G only, 3 = B only.
    pub fn apply_curves_lut(&mut self, lut: &[u8], channel: i32) {
        let id = self.active_layer;
        if let Some(layer) = self.stack.by_id_mut(id) {
            if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
                return;
            }
            for px in layer.pixels.as_bytes_mut().chunks_exact_mut(4) {
                if px[3] == 0 {
                    continue;
                }
                match channel {
                    1 => px[0] = lut[px[0] as usize],
                    2 => px[1] = lut[px[1] as usize],
                    3 => px[2] = lut[px[2] as usize],
                    _ => {
                        px[0] = lut[px[0] as usize];
                        px[1] = lut[px[1] as usize];
                        px[2] = lut[px[2] as usize];
                    }
                }
            }
        }
        self.commit("Curves");
    }

    /// CS6-style Black & White conversion with per-hue-range weights.
    /// Each weight is a percentage (-200..300); default is Reds=40, Yellows=60,
    /// Greens=40, Cyans=60, Blues=20, Magentas=80.
    /// Optional tint applies a colorize pass with the given hue (0..360) and
    /// saturation (0..100).
    pub fn apply_black_and_white(
        &mut self,
        reds: f32,
        yellows: f32,
        greens: f32,
        cyans: f32,
        blues: f32,
        magentas: f32,
        tint: bool,
        tint_hue: f32,
        tint_saturation: f32,
    ) {
        use crate::filters::adjust::{rgb_to_hsl, hsl_to_rgb};

        let weights = [reds, yellows, greens, cyans, blues, magentas];
        // Centres at 0°, 60°, 120°, 180°, 240°, 300° (in 0..1 space)
        let centres: [f32; 6] = [0.0, 1.0/6.0, 2.0/6.0, 3.0/6.0, 4.0/6.0, 5.0/6.0];

        let id = self.active_layer;
        if let Some(layer) = self.stack.by_id_mut(id) {
            if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
                return;
            }
            for px in layer.pixels.as_bytes_mut().chunks_exact_mut(4) {
                if px[3] == 0 {
                    continue;
                }
                let r = px[0] as f32 / 255.0;
                let g = px[1] as f32 / 255.0;
                let b = px[2] as f32 / 255.0;
                let (h, s, l) = rgb_to_hsl([r, g, b]);

                // Base luminance
                let base_lum = 0.299 * r + 0.587 * g + 0.114 * b;

                // Compute weighted contribution from each range.
                // Each range is a triangle centered at its hue, 1/6 wide (60°)
                // on each side, overlapping neighbours. Weights sum to ~1 for
                // any hue, so the default (40,60,40,60,20,80) reproduces the
                // classic channel-mixer B&W look.
                let mut total_weight = 0.0_f32;
                let mut weighted_pct = 0.0_f32;

                for i in 0..6 {
                    let mut dist = (h - centres[i]).abs();
                    if dist > 0.5 {
                        dist = 1.0 - dist;
                    }
                    let w = (1.0 - dist * 6.0).max(0.0);
                    total_weight += w;
                    weighted_pct += w * weights[i];
                }

                let pct = if total_weight > 1e-6 {
                    weighted_pct / total_weight
                } else {
                    0.0
                };

                // Mix: for saturated pixels the slider has full effect;
                // for desaturated pixels it fades to base luminance.
                let gray = (base_lum + s * (pct / 100.0) * base_lum).clamp(0.0, 1.0);

                if tint {
                    let th = (tint_hue / 360.0).rem_euclid(1.0);
                    let ts = (tint_saturation / 100.0).clamp(0.0, 1.0);
                    let out = hsl_to_rgb(th, ts, gray);
                    px[0] = (out[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    px[1] = (out[1].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    px[2] = (out[2].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                } else {
                    let v = (gray * 255.0 + 0.5) as u8;
                    px[0] = v;
                    px[1] = v;
                    px[2] = v;
                }
            }
        }
        self.commit("Black & White");
    }

    /// Apply a CS6-style photo filter: blend a solid colour onto the image at
    /// a given density, optionally preserving luminosity.
    pub fn apply_photo_filter(
        &mut self,
        r: f32,
        g: f32,
        b: f32,
        density: f32,
        preserve_luminosity: bool,
    ) {
        use crate::filters::adjust::rgb_to_hsl;

        let d = (density / 100.0).clamp(0.0, 1.0);
        let id = self.active_layer;
        if let Some(layer) = self.stack.by_id_mut(id) {
            if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
                return;
            }
            for px in layer.pixels.as_bytes_mut().chunks_exact_mut(4) {
                if px[3] == 0 {
                    continue;
                }
                let orig_r = px[0] as f32 / 255.0;
                let orig_g = px[1] as f32 / 255.0;
                let orig_b = px[2] as f32 / 255.0;

                let mut nr = orig_r * (1.0 - d) + r * d;
                let mut ng = orig_g * (1.0 - d) + g * d;
                let mut nb = orig_b * (1.0 - d) + b * d;

                if preserve_luminosity {
                    let orig_lum = 0.299 * orig_r + 0.587 * orig_g + 0.114 * orig_b;
                    let new_lum = 0.299 * nr + 0.587 * ng + 0.114 * nb;
                    if new_lum > 1e-6 {
                        let scale = orig_lum / new_lum;
                        nr *= scale;
                        ng *= scale;
                        nb *= scale;
                    }
                }

                px[0] = (nr.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                px[1] = (ng.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                px[2] = (nb.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
        }
        self.commit("Photo Filter");
    }

    /// CS6-style Channel Mixer.
    ///
    /// `matrix` is row-major 3×3: `[rr, rg, rb, gr, gg, gb, br, bg, bb]` where
    /// e.g. `rr` is the Red source weight for the Red output channel (in percent,
    /// -200..+200). `constants` are the per-channel constant offsets (percent).
    /// When `monochrome` is true, only the first row is used and the result is
    /// written to all three channels.
    pub fn apply_channel_mixer(
        &mut self,
        matrix: &[f32; 9],
        constants: &[f32; 3],
        monochrome: bool,
    ) {
        let id = self.active_layer;
        if let Some(layer) = self.stack.by_id_mut(id) {
            if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
                return;
            }
            for px in layer.pixels.as_bytes_mut().chunks_exact_mut(4) {
                if px[3] == 0 {
                    continue;
                }
                let r = px[0] as f32 / 255.0;
                let g = px[1] as f32 / 255.0;
                let b = px[2] as f32 / 255.0;

                if monochrome {
                    let v = r * matrix[0] / 100.0
                          + g * matrix[1] / 100.0
                          + b * matrix[2] / 100.0
                          + constants[0] / 100.0;
                    let v = v.clamp(0.0, 1.0);
                    let out = (v * 255.0 + 0.5) as u8;
                    px[0] = out;
                    px[1] = out;
                    px[2] = out;
                } else {
                    let nr = r * matrix[0] / 100.0
                           + g * matrix[1] / 100.0
                           + b * matrix[2] / 100.0
                           + constants[0] / 100.0;
                    let ng = r * matrix[3] / 100.0
                           + g * matrix[4] / 100.0
                           + b * matrix[5] / 100.0
                           + constants[1] / 100.0;
                    let nb = r * matrix[6] / 100.0
                           + g * matrix[7] / 100.0
                           + b * matrix[8] / 100.0
                           + constants[2] / 100.0;
                    px[0] = (nr.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    px[1] = (ng.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    px[2] = (nb.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
            }
        }
        self.commit("Channel Mixer");
    }

    /// CS6-style Gradient Map: replace each pixel's colour with the gradient
    /// colour at its luminance position.
    pub fn apply_gradient_map(
        &mut self,
        gradient: &crate::gradient::Gradient,
        dither: bool,
    ) {
        let id = self.active_layer;
        if let Some(layer) = self.stack.by_id_mut(id) {
            if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
                return;
            }
            // Build a 256-entry LUT for speed.
            let lut: Vec<Rgba8> = (0..256)
                .map(|i| gradient.sample(i as f32 / 255.0))
                .collect();

            let mut rng_state: u32 = 0x12345678;

            for px in layer.pixels.as_bytes_mut().chunks_exact_mut(4) {
                if px[3] == 0 {
                    continue;
                }
                let lum = (0.299 * px[0] as f32
                         + 0.587 * px[1] as f32
                         + 0.114 * px[2] as f32)
                    .clamp(0.0, 255.0);

                if dither {
                    // Ordered dither: use the fractional luminance to
                    // probabilistically round up or down, giving smoother
                    // transitions.
                    let frac = lum - lum.floor();
                    // Simple xorshift PRNG for per-pixel noise.
                    rng_state ^= rng_state << 13;
                    rng_state ^= rng_state >> 17;
                    rng_state ^= rng_state << 5;
                    let rand01 = (rng_state & 0xFFFF) as f32 / 65535.0;
                    let idx = if rand01 < frac {
                        (lum as usize + 1).min(255)
                    } else {
                        lum as usize
                    };
                    let c = lut[idx];
                    px[0] = c.r;
                    px[1] = c.g;
                    px[2] = c.b;
                } else {
                    let idx = (lum + 0.5) as usize;
                    let idx = idx.min(255);
                    let c = lut[idx];
                    px[0] = c.r;
                    px[1] = c.g;
                    px[2] = c.b;
                }
            }
        }
        self.commit("Gradient Map");
    }

    /// CS6-style Selective Color adjustment.
    ///
    /// `adjustments` is a 9×4 array: 9 color ranges (Reds, Yellows, Greens,
    /// Cyans, Blues, Magentas, Whites, Neutrals, Blacks), each with 4 CMYK
    /// adjustment values in -100..+100. `relative` selects relative mode
    /// (adjusts proportionally to existing CMY amounts) vs absolute mode.
    pub fn apply_selective_color(
        &mut self,
        adjustments: &[[f32; 4]; 9],
        relative: bool,
    ) {
        let id = self.active_layer;
        if let Some(layer) = self.stack.by_id_mut(id) {
            if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
                return;
            }
            for px in layer.pixels.as_bytes_mut().chunks_exact_mut(4) {
                if px[3] == 0 {
                    continue;
                }
                let r = px[0] as f32 / 255.0;
                let g = px[1] as f32 / 255.0;
                let b = px[2] as f32 / 255.0;

                // Convert to CMY (not CMYK — Selective Color works in CMY space)
                let mut c = 1.0 - r;
                let mut m = 1.0 - g;
                let mut y = 1.0 - b;

                let max = r.max(g).max(b);
                let min = r.min(g).min(b);

                // Determine how much this pixel belongs to each color range.
                // Photoshop uses the dominant hue and the secondary overlaps.
                let ranges = Self::selective_color_weights(r, g, b, max, min);

                for (range_idx, weight) in ranges.iter().enumerate() {
                    if *weight <= 0.0 {
                        continue;
                    }
                    let adj = &adjustments[range_idx];
                    if adj[0] == 0.0 && adj[1] == 0.0 && adj[2] == 0.0 && adj[3] == 0.0 {
                        continue;
                    }

                    let dc = adj[0] / 100.0;
                    let dm = adj[1] / 100.0;
                    let dy = adj[2] / 100.0;
                    let dk = adj[3] / 100.0;

                    let w = *weight;

                    if relative {
                        c += (dc * c + dk * c) * w;
                        m += (dm * m + dk * m) * w;
                        y += (dy * y + dk * y) * w;
                    } else {
                        c += (dc + dk) * w;
                        m += (dm + dk) * w;
                        y += (dy + dk) * w;
                    }
                }

                px[0] = ((1.0 - c.clamp(0.0, 1.0)) * 255.0 + 0.5) as u8;
                px[1] = ((1.0 - m.clamp(0.0, 1.0)) * 255.0 + 0.5) as u8;
                px[2] = ((1.0 - y.clamp(0.0, 1.0)) * 255.0 + 0.5) as u8;
            }
        }
        self.commit("Selective Color");
    }

    fn selective_color_weights(r: f32, g: f32, b: f32, max: f32, min: f32) -> [f32; 9] {
        // Ranges: 0=Reds, 1=Yellows, 2=Greens, 3=Cyans, 4=Blues, 5=Magentas,
        //         6=Whites, 7=Neutrals, 8=Blacks
        let mut w = [0.0f32; 9];

        // Chromatic ranges — weight by how dominant the hue is
        let chroma = max - min;
        if chroma > 0.0 {
            // Reds: R dominant, hue near 0° or 360°
            if r >= g && r >= b {
                // Red is max
                if g >= b {
                    // hue 0–60° (red–yellow)
                    let red_w = (chroma * (1.0 - (g - b) / chroma)).min(chroma);
                    w[0] = red_w;   // Reds
                    w[1] = chroma - red_w; // Yellows
                } else {
                    // hue 300–360° (magenta–red)
                    let red_w = (chroma * (1.0 - (b - g) / chroma)).min(chroma);
                    w[0] = red_w;   // Reds
                    w[5] = chroma - red_w; // Magentas
                }
            } else if g >= r && g >= b {
                // Green is max
                if r >= b {
                    // hue 60–120° (yellow–green)
                    let grn_w = (chroma * (1.0 - (r - b) / chroma)).min(chroma);
                    w[2] = grn_w;   // Greens
                    w[1] = chroma - grn_w; // Yellows
                } else {
                    // hue 120–180° (green–cyan)
                    let grn_w = (chroma * (1.0 - (b - r) / chroma)).min(chroma);
                    w[2] = grn_w;   // Greens
                    w[3] = chroma - grn_w; // Cyans
                }
            } else {
                // Blue is max
                if g >= r {
                    // hue 180–240° (cyan–blue)
                    let blu_w = (chroma * (1.0 - (g - r) / chroma)).min(chroma);
                    w[4] = blu_w;   // Blues
                    w[3] = chroma - blu_w; // Cyans
                } else {
                    // hue 240–300° (blue–magenta)
                    let blu_w = (chroma * (1.0 - (r - g) / chroma)).min(chroma);
                    w[4] = blu_w;   // Blues
                    w[5] = chroma - blu_w; // Magentas
                }
            }
        }

        // Tonal ranges — Whites, Neutrals, Blacks
        // These use the min-of-RGB approach that Photoshop uses.
        w[6] = min;                         // Whites
        w[8] = 1.0 - max;                   // Blacks
        w[7] = 1.0 - (w[6] + w[8] + chroma).min(1.0); // Neutrals

        w
    }

    /// CS6-style Shadows/Highlights.
    ///
    /// `shadow_amount` (0..100) lifts dark pixels, `highlight_amount` (0..100)
    /// darkens bright pixels. Works in luminance space to avoid hue shifts,
    /// using an additive delta that is capped to prevent colour fringing.
    pub fn apply_shadows_highlights(
        &mut self,
        shadow_amount: f32,
        highlight_amount: f32,
    ) {
        let sa = (shadow_amount / 100.0).clamp(0.0, 1.0);
        let ha = (highlight_amount / 100.0).clamp(0.0, 1.0);
        if sa == 0.0 && ha == 0.0 {
            return;
        }

        // Build a LUT: luminance index → additive delta for each channel.
        // Shadows lift dark luminances, highlights pull down bright ones.
        // The magnitudes are calibrated to match CS6's default-radius output.
        let delta_lut: [f32; 256] = std::array::from_fn(|i| {
            let l = i as f32 / 255.0;

            // Shadow: smoothstep weight peaks at black, fades to zero by ~0.5.
            let st = (1.0 - l * 2.0).clamp(0.0, 1.0);
            let sw = st * st * (3.0 - 2.0 * st);
            let shadow_delta = sw * sa * 0.35;

            // Highlight: smoothstep weight peaks at white, fades to zero by ~0.5.
            let ht = ((l - 0.5) * 2.0).clamp(0.0, 1.0);
            let hw = ht * ht * (3.0 - 2.0 * ht);
            let highlight_delta = hw * ha * 0.30;

            shadow_delta - highlight_delta
        });

        let id = self.active_layer;
        if let Some(layer) = self.stack.by_id_mut(id) {
            if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
                return;
            }
            for px in layer.pixels.as_bytes_mut().chunks_exact_mut(4) {
                if px[3] == 0 {
                    continue;
                }
                let r = px[0] as f32 / 255.0;
                let g = px[1] as f32 / 255.0;
                let b = px[2] as f32 / 255.0;

                let lum = 0.299 * r + 0.587 * g + 0.114 * b;
                let idx = (lum * 255.0 + 0.5).clamp(0.0, 255.0) as usize;
                let delta = delta_lut[idx];

                if delta.abs() < 1e-6 {
                    continue;
                }

                // Add delta uniformly to all channels — preserves hue and
                // saturation while shifting lightness.
                px[0] = ((r + delta).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                px[1] = ((g + delta).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                px[2] = ((b + delta).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
        }
        self.commit("Shadows/Highlights");
    }

    /// CS6-style HDR Toning (local adaptation).
    ///
    /// This is a local tone-mapping operator that uses a blurred version of the
    /// image as a local luminance estimate, then adjusts each pixel relative to
    /// its neighbourhood.
    ///
    /// Parameters match CS6's "Local Adaptation" controls:
    /// - `radius`: edge glow radius in pixels (1–500)
    /// - `strength`: edge glow strength (0.01–4.0)
    /// - `gamma`: tone-and-detail gamma (0.01–9.99)
    /// - `exposure`: tone-and-detail exposure (-5.0..+5.0)
    /// - `detail`: tone-and-detail detail enhancement (-100..+300 %)
    /// - `shadow`: advanced shadow (-100..+100 %)
    /// - `highlight`: advanced highlight (-100..+100 %)
    /// - `vibrance`: advanced vibrance (-100..+100 %)
    /// - `saturation`: advanced saturation (-100..+100 %)
    pub fn apply_hdr_toning(
        &mut self,
        radius: f32,
        strength: f32,
        gamma: f32,
        exposure: f32,
        detail: f32,
        shadow: f32,
        highlight: f32,
        vibrance: f32,
        saturation: f32,
    ) {
        use crate::filters::convolve::gaussian_blur_accelerated;
        use crate::filters::adjust::rgb_to_hsl;

        let id = self.active_layer;
        if let Some(layer) = self.stack.by_id_mut(id) {
            if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
                return;
            }

            let (w, h) = (layer.pixels.width(), layer.pixels.height());

            // Build a luminance-only blurred copy for local adaptation.
            // Accelerated: this is the dominant cost of HDR Toning, and it is
            // re-run on every slider move while the dialog is open.
            let mut blurred = layer.pixels.clone();
            if radius >= 1.0 {
                gaussian_blur_accelerated(&mut blurred, radius);
            }

            let strength = strength.clamp(0.01, 4.0);
            let gamma = gamma.clamp(0.01, 9.99);
            let exposure = exposure.clamp(-5.0, 5.0);
            let detail_amt = detail.clamp(-100.0, 300.0) / 100.0;
            let shadow_amt = shadow.clamp(-100.0, 100.0) / 100.0;
            let highlight_amt = highlight.clamp(-100.0, 100.0) / 100.0;
            let vib_amt = vibrance.clamp(-100.0, 100.0) / 100.0;
            let sat_amt = saturation.clamp(-100.0, 100.0) / 100.0;

            // Local Adaptation works on a log-domain base/detail split:
            // the blurred copy is the base layer, the per-pixel residual is
            // the detail layer. Gamma compresses the base layer's dynamic
            // range (so a large gamma flattens rather than brightens), while
            // Detail scales the residual independently. That separation is
            // what lets Detail -100% give a smooth image and +300% give the
            // heavy texture and edge glow.
            const EPS: f32 = 1e-3;
            // Clamped so an extreme Gamma cannot blow the log-domain
            // deviation up into exp() overflow.
            let compress = (1.0 / gamma).clamp(0.1, 4.0);
            // Edge Glow Strength modulates how much of the detail layer
            // survives, normalised so the mid of the slider is neutral.
            let detail_scale = (1.0 + detail_amt * (strength * 0.5).min(1.0)).max(0.0);
            let exposure_ln = exposure * std::f32::consts::LN_2;

            // Anchor the compression around the image's log-average
            // luminance (the "key"), so flattening pulls toward the
            // picture's own midtone instead of an arbitrary constant.
            let pivot = {
                let bytes = layer.pixels.as_bytes();
                let mut sum = 0.0f64;
                let mut count = 0u64;
                for px in bytes.chunks_exact(4) {
                    if px[3] == 0 {
                        continue;
                    }
                    let l = (0.299 * px[0] as f32
                        + 0.587 * px[1] as f32
                        + 0.114 * px[2] as f32) / 255.0;
                    sum += (l + EPS).ln() as f64;
                    count += 1;
                }
                if count > 0 {
                    (sum / count as f64) as f32
                } else {
                    (0.18f32).ln()
                }
            };

            for y in 0..h as i32 {
                for x in 0..w as i32 {
                    let px_idx = (y as u32 * w + x as u32) as usize * 4;
                    let bytes = layer.pixels.as_bytes_mut();
                    if bytes[px_idx + 3] == 0 {
                        continue;
                    }

                    let r = bytes[px_idx] as f32 / 255.0;
                    let g = bytes[px_idx + 1] as f32 / 255.0;
                    let b = bytes[px_idx + 2] as f32 / 255.0;

                    let lum = 0.299 * r + 0.587 * g + 0.114 * b;

                    let bp = blurred.get(x, y);
                    let local_lum = (0.299 * bp.r as f32 + 0.587 * bp.g as f32
                        + 0.114 * bp.b as f32) / 255.0;

                    // 1. Split into base (blurred) and detail (residual)
                    //    in the log domain.
                    let log_lum = (lum + EPS).ln();
                    let log_base = (local_lum + EPS).ln();
                    let log_detail = log_lum - log_base;

                    // 2. Shrink very small residuals before boosting them.
                    //    In a smooth region (sky) the real detail is the
                    //    same magnitude as the source's JPEG block noise,
                    //    so amplifying it uniformly makes the 8x8 blocks
                    //    visible. Gating by magnitude keeps genuine edges
                    //    and drops the noise floor.
                    const DETAIL_KNEE: f32 = 0.06;
                    let t = (log_detail.abs() / DETAIL_KNEE).min(1.0);
                    let gate = t * t * (3.0 - 2.0 * t);

                    // 3. Compress the base around the key, scale the
                    //    detail, then apply exposure as a log-domain shift.
                    let base_dev = ((log_base - pivot) * compress).clamp(-8.0, 8.0);
                    let log_out = pivot
                        + base_dev
                        + log_detail * gate * detail_scale
                        + exposure_ln;
                    let mut target = log_out.exp();

                    // 3. Shadow/highlight recovery.
                    if shadow_amt.abs() > 1e-4 {
                        let st = (1.0 - target * 2.0).clamp(0.0, 1.0);
                        let sw = st * st * (3.0 - 2.0 * st);
                        target += sw * shadow_amt * 0.15;
                    }
                    if highlight_amt.abs() > 1e-4 {
                        let ht = ((target - 0.5) * 2.0).clamp(0.0, 1.0);
                        let hw = ht * ht * (3.0 - 2.0 * ht);
                        target += hw * highlight_amt * 0.15;
                    }

                    // 4. Filmic S-curve: soft shoulder + toe to simulate
                    //    32-bit headroom. Values that exceed [0,1] get
                    //    smoothly compressed instead of hard-clipping.
                    if target > 1.0 {
                        let ex = target - 1.0;
                        target = 1.0 - (-ex * 2.0).exp() * 0.15;
                    } else if target > 0.85 {
                        let t = (target - 0.85) / 0.15;
                        let shoulder = 0.85 + 0.15 * (1.0 - (1.0 - t).powi(2));
                        target = shoulder;
                    }
                    if target < 0.0 {
                        let ex = -target;
                        target = (-ex * 2.0).exp() * 0.05;
                    } else if target < 0.10 {
                        let t = target / 0.10;
                        target = 0.10 * t * t;
                    }

                    // 4. Additive delta preserves chrominance exactly.
                    let delta = target - lum;
                    let mut nr = (r + delta).clamp(0.0, 1.0);
                    let mut ng = (g + delta).clamp(0.0, 1.0);
                    let mut nb = (b + delta).clamp(0.0, 1.0);

                    // 5. Vibrance + saturation in HSL, with a gate
                    //    that skips near-gray pixels whose hue is
                    //    unreliable (prevents JPEG noise amplification).
                    if vib_amt.abs() > 1e-4 || sat_amt.abs() > 1e-4 {
                        let (h_hsl, mut s_hsl, l_hsl) = rgb_to_hsl([nr, ng, nb]);
                        let orig_s = s_hsl;

                        if vib_amt.abs() > 1e-4 {
                            let boost = (1.0 - s_hsl) * vib_amt;
                            s_hsl = (s_hsl + boost).clamp(0.0, 1.0);
                        }
                        if sat_amt >= 0.0 {
                            s_hsl += (1.0 - s_hsl) * sat_amt;
                        } else {
                            s_hsl += s_hsl * sat_amt;
                        }
                        s_hsl = s_hsl.clamp(0.0, 1.0);

                        let gate = (orig_s / 0.15).min(1.0);
                        s_hsl = orig_s + (s_hsl - orig_s) * gate;

                        let rgb = crate::filters::adjust::hsl_to_rgb(h_hsl, s_hsl, l_hsl);
                        nr = rgb[0];
                        ng = rgb[1];
                        nb = rgb[2];
                    }

                    bytes[px_idx]     = (nr * 255.0 + 0.5) as u8;
                    bytes[px_idx + 1] = (ng * 255.0 + 0.5) as u8;
                    bytes[px_idx + 2] = (nb * 255.0 + 0.5) as u8;
                }
            }
        }
        self.commit("HDR Toning");
    }

    /// Selection weight for one pixel under Replace Color's rules.
    ///
    /// Distance is the largest per-channel difference, matching the way
    /// Photoshop's Fuzziness reads as a tolerance in 0..255 levels. Several
    /// samples are combined by taking the best match, so "Add to Sample"
    /// widens the selection rather than averaging it away.
    fn replace_color_weight(
        r: u8,
        g: u8,
        b: u8,
        x: i32,
        y: i32,
        samples: &[ColorSample],
        fuzziness: f32,
        localized: bool,
        sigma_sq: f32,
    ) -> f32 {
        let mut best = 0.0f32;
        for s in samples {
            let d = (r as i32 - s.rgb[0] as i32)
                .abs()
                .max((g as i32 - s.rgb[1] as i32).abs())
                .max((b as i32 - s.rgb[2] as i32).abs()) as f32;
            let mut w = if fuzziness <= 0.0 {
                if d == 0.0 { 1.0 } else { 0.0 }
            } else {
                (1.0 - d / fuzziness).clamp(0.0, 1.0)
            };
            // Localized Color Clusters keeps the selection near where the
            // colour was actually picked, so a colour that recurs elsewhere
            // in the frame is not dragged in with it. This is a spatial
            // falloff around each sample, not Photoshop's exact clustering.
            if localized && w > 0.0 {
                let dx = (x - s.x) as f32;
                let dy = (y - s.y) as f32;
                w *= (-(dx * dx + dy * dy) / (2.0 * sigma_sq)).exp();
            }
            best = best.max(w);
        }
        best
    }

    /// Spatial falloff width for Localized Color Clusters, as a fraction of
    /// the image diagonal.
    fn replace_color_sigma_sq(w: u32, h: u32) -> f32 {
        let diag = ((w * w + h * h) as f32).sqrt().max(1.0);
        let sigma = diag * 0.18;
        sigma * sigma
    }

    /// The Replace Color selection mask, fitted into a `size` box for the
    /// dialog's preview. White is fully selected.
    pub fn replace_color_mask(
        &self,
        samples: &[ColorSample],
        fuzziness: f32,
        localized: bool,
        size: u32,
    ) -> Pixmap {
        let id = self.active_layer;
        let Some(layer) = self.stack.by_id(id) else {
            return Pixmap::new(1, 1);
        };
        let (sw, sh) = (layer.pixels.width(), layer.pixels.height());
        if sw == 0 || sh == 0 || size == 0 {
            return Pixmap::new(1, 1);
        }

        let scale = (size as f32 / sw as f32).min(size as f32 / sh as f32);
        let tw = ((sw as f32 * scale).round() as u32).max(1);
        let th = ((sh as f32 * scale).round() as u32).max(1);
        let sigma_sq = Self::replace_color_sigma_sq(sw, sh);

        let mut out = Pixmap::new(tw, th);
        for y in 0..th {
            for x in 0..tw {
                let sx = (x as f32 / scale) as i32;
                let sy = (y as f32 / scale) as i32;
                let px = layer.pixels.get(sx, sy);
                let w = if px.a == 0 {
                    0.0
                } else {
                    Self::replace_color_weight(
                        px.r, px.g, px.b, sx, sy, samples, fuzziness, localized, sigma_sq,
                    )
                };
                let v = (w * 255.0 + 0.5) as u8;
                out.set(x as i32, y as i32, Rgba8::opaque(v, v, v));
            }
        }
        out
    }

    /// Photoshop's Image > Adjustments > Replace Color: shift the sampled
    /// colour range in HSL, feathered by the selection mask.
    ///
    /// `hue` is in degrees (-180..180); `saturation` and `lightness` are
    /// -100..100.
    pub fn apply_replace_color(
        &mut self,
        samples: &[ColorSample],
        fuzziness: f32,
        localized: bool,
        hue: f32,
        saturation: f32,
        lightness: f32,
    ) {
        use crate::filters::adjust::{hsl_to_rgb, rgb_to_hsl};

        if samples.is_empty() {
            return;
        }

        let hue_shift = hue.clamp(-180.0, 180.0) / 360.0;
        let sat_amt = saturation.clamp(-100.0, 100.0) / 100.0;
        let light_amt = lightness.clamp(-100.0, 100.0) / 100.0;

        let id = self.active_layer;
        if let Some(layer) = self.stack.by_id_mut(id) {
            if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
                return;
            }
            let (w, h) = (layer.pixels.width(), layer.pixels.height());
            let sigma_sq = Self::replace_color_sigma_sq(w, h);

            let bytes = layer.pixels.as_bytes_mut();
            for (i, px) in bytes.chunks_exact_mut(4).enumerate() {
                if px[3] == 0 {
                    continue;
                }
                let x = (i as u32 % w) as i32;
                let y = (i as u32 / w) as i32;
                let weight = Self::replace_color_weight(
                    px[0], px[1], px[2], x, y, samples, fuzziness, localized, sigma_sq,
                );
                if weight <= 0.0 {
                    continue;
                }

                let r = px[0] as f32 / 255.0;
                let g = px[1] as f32 / 255.0;
                let b = px[2] as f32 / 255.0;
                let (hh, ss, ll) = rgb_to_hsl([r, g, b]);

                let nh = (hh + hue_shift).rem_euclid(1.0);
                let ns = if sat_amt >= 0.0 {
                    ss + (1.0 - ss) * sat_amt
                } else {
                    ss * (1.0 + sat_amt)
                };
                let nl = if light_amt >= 0.0 {
                    ll + (1.0 - ll) * light_amt
                } else {
                    ll * (1.0 + light_amt)
                };

                let out = hsl_to_rgb(nh, ns.clamp(0.0, 1.0), nl.clamp(0.0, 1.0));
                // Feather by the mask so the edge of the range blends in
                // rather than showing a hard cut.
                let pairs = [(r, out[0]), (g, out[1]), (b, out[2])];
                for c in 0..3 {
                    let (orig, new) = pairs[c];
                    let v = orig + (new - orig) * weight;
                    px[c] = (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
            }
        }
        self.commit("Replace Color");
    }

    /// Apply CS6-style color balance with tone-weighted shifts and optional
    /// luminosity preservation.
    /// `cyan_red`, `magenta_green`, `yellow_blue` are each -100..100.
    /// `tone`: 0=Shadows, 1=Midtones, 2=Highlights.
    pub fn apply_color_balance(
        &mut self,
        cyan_red: f32,
        magenta_green: f32,
        yellow_blue: f32,
        tone: i32,
        preserve_luminosity: bool,
    ) {
        use crate::filters::adjust::{rgb_to_hsl, hsl_to_rgb};

        // GIMP/PS-compatible scaling: ±100 maps to roughly ±0.39 (100/256).
        let cr = cyan_red / 256.0;
        let mg = magenta_green / 256.0;
        let yb = yellow_blue / 256.0;

        let id = self.active_layer;
        if let Some(layer) = self.stack.by_id_mut(id) {
            if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
                return;
            }
            for px in layer.pixels.as_bytes_mut().chunks_exact_mut(4) {
                if px[3] == 0 {
                    continue;
                }
                let r = px[0] as f32 / 255.0;
                let g = px[1] as f32 / 255.0;
                let b = px[2] as f32 / 255.0;

                let lum = 0.299 * r + 0.587 * g + 0.114 * b;

                // GIMP-style smooth transfer functions per tone range.
                let (shadows, midtones, highlights) = {
                    let s = if lum <= 0.5 {
                        (0.5 - lum) / 0.5
                    } else {
                        0.0
                    };
                    let h = if lum >= 0.5 {
                        (lum - 0.5) / 0.5
                    } else {
                        0.0
                    };
                    let m = 1.0 - (s + h);
                    (s, m, h)
                };

                let weight = match tone {
                    0 => shadows,
                    1 => midtones,
                    2 => highlights,
                    _ => 1.0,
                };

                if weight < 1e-6 {
                    continue;
                }

                let nr = (r + cr * weight).clamp(0.0, 1.0);
                let ng = (g + mg * weight).clamp(0.0, 1.0);
                let nb = (b + yb * weight).clamp(0.0, 1.0);

                if preserve_luminosity {
                    let (h, s, _) = rgb_to_hsl([nr, ng, nb]);
                    let (_, _, orig_l) = rgb_to_hsl([r, g, b]);
                    let out = hsl_to_rgb(h, s, orig_l);
                    px[0] = (out[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    px[1] = (out[1].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    px[2] = (out[2].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                } else {
                    px[0] = (nr * 255.0 + 0.5) as u8;
                    px[1] = (ng * 255.0 + 0.5) as u8;
                    px[2] = (nb * 255.0 + 0.5) as u8;
                }
            }
        }
        self.commit("Color Balance");
    }

    /// Apply hue/saturation/lightness only to pixels whose hue falls in a
    /// specific colour range (channel 0=Master, 1=Reds, 2=Yellows, 3=Greens,
    /// 4=Cyans, 5=Blues, 6=Magentas).
    pub fn apply_hue_saturation_range(
        &mut self,
        hue_shift: f32,
        saturation: f32,
        lightness: f32,
        channel: i32,
    ) {
        use crate::filters::adjust::{rgb_to_hsl, hsl_to_rgb};

        if channel == 0 {
            let adj = crate::filters::adjust::Adjustment::HueSaturation {
                hue: hue_shift,
                saturation,
                lightness,
            };
            self.apply_adjustment(adj);
            return;
        }

        // Centre hue (in 0..1) and half-widths for each range.
        // Inner: full effect. Outer: feather zone.
        let (center, inner_half, outer_half) = match channel {
            1 => (0.0_f32, 15.0 / 360.0, 45.0 / 360.0),   // Reds
            2 => (60.0 / 360.0, 15.0 / 360.0, 45.0 / 360.0),  // Yellows
            3 => (120.0 / 360.0, 15.0 / 360.0, 45.0 / 360.0), // Greens
            4 => (180.0 / 360.0, 15.0 / 360.0, 45.0 / 360.0), // Cyans
            5 => (240.0 / 360.0, 15.0 / 360.0, 45.0 / 360.0), // Blues
            6 => (300.0 / 360.0, 15.0 / 360.0, 45.0 / 360.0), // Magentas
            _ => return,
        };

        let id = self.active_layer;
        if let Some(layer) = self.stack.by_id_mut(id) {
            if layer.lock_pixels || !matches!(layer.kind, LayerKind::Raster) {
                return;
            }
            for px in layer.pixels.as_bytes_mut().chunks_exact_mut(4) {
                if px[3] == 0 {
                    continue;
                }
                let c = [
                    px[0] as f32 / 255.0,
                    px[1] as f32 / 255.0,
                    px[2] as f32 / 255.0,
                ];
                let (h, s, l) = rgb_to_hsl(c);

                // Hue distance on the circle
                let mut dist = (h - center).abs();
                if dist > 0.5 {
                    dist = 1.0 - dist;
                }

                let weight = if dist <= inner_half {
                    1.0
                } else if dist <= outer_half {
                    1.0 - (dist - inner_half) / (outer_half - inner_half)
                } else {
                    0.0
                };

                if weight < 1e-6 {
                    continue;
                }

                let new_h = (h + hue_shift * weight).rem_euclid(1.0);
                let new_s = if saturation >= 0.0 {
                    s + (1.0 - s) * saturation.min(1.0) * weight
                } else {
                    s * (1.0 + saturation.max(-1.0) * weight)
                };
                let new_l = if lightness >= 0.0 {
                    l + (1.0 - l) * lightness.min(1.0) * weight
                } else {
                    l * (1.0 + lightness.max(-1.0) * weight)
                };

                let out = hsl_to_rgb(new_h, new_s.clamp(0.0, 1.0), new_l.clamp(0.0, 1.0));
                px[0] = (out[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                px[1] = (out[1].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                px[2] = (out[2].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
        }
        self.commit("Hue/Saturation");
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

        let dirty = self.fill_polygons(&contours, color, opacity);
        if dirty.is_empty() {
            return Rect::default();
        }
        self.commit("Fill Path");
        dirty
    }

    /// Paint closed polygons onto the active layer, antialiased at their edges.
    ///
    /// Shared by Fill Path and the shape tools' Pixels mode: both turn an
    /// outline into coverage and lay one colour through it. Returns the region
    /// changed, in document space, and records no history state — the caller
    /// names the action.
    fn fill_polygons(&mut self, contours: &[Vec<(f32, f32)>], color: Rgba8, opacity: f32) -> Rect {
        if contours.iter().all(|c| c.len() < 3) {
            return Rect::default();
        }

        let mut coverage = Selection::new(self.width, self.height);
        coverage.apply_polygons_feathered(contours, SelectionOp::Replace, 0);

        let id = self.active_layer;
        let Some(layer) = self.stack.by_id_mut(id) else {
            return Rect::default();
        };
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

        if touched.is_empty() {
            return Rect::default();
        }
        // Document space, matching every other pixel-editing call here —
        // `touched` was accumulated in the layer's own coordinates.
        Rect::new(touched.x + offset.0, touched.y + offset.1, touched.width, touched.height)
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
    ///
    /// Goes through the active backend: this runs on every repaint, so it is
    /// the single hottest path in the engine.
    pub fn composite(&self) -> Pixmap {
        crate::gpu::shared().composite(&self.stack, self.width, self.height)
    }

    /// Composite only `region`.
    ///
    /// Stays on the CPU deliberately. Partial repaints are already small — a
    /// brush dab's bounding box — so they sit below the size where the GPU
    /// wins, and uploading the whole stack to recompute a few hundred pixels
    /// would be slower than doing it here.
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
            self.color_mode = state.color_mode;
            self.bit_depth = state.bit_depth;
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
            self.color_mode = state.color_mode;
            self.bit_depth = state.bit_depth;
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
            self.color_mode = state.color_mode;
            self.bit_depth = state.bit_depth;
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
        self.history.push(
            name,
            self.stack.clone(),
            (self.width, self.height),
            self.color_mode,
            self.bit_depth,
        );
        self.dirty = true;
    }

    pub fn commit_coalescing(&mut self, name: impl Into<String>) {
        self.history.push_coalescing(
            name,
            self.stack.clone(),
            (self.width, self.height),
            self.color_mode,
            self.bit_depth,
        );
        self.dirty = true;
    }

    pub fn seal_history(&mut self) {
        self.history.seal_coalescing();
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
    use crate::sample::Limits;

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
    fn a_selection_confines_a_destructive_adjustment() {
        let mut d = doc();
        d.select_rect(
            Rect { x: 0, y: 0, width: 8, height: 16 },
            SelectionOp::Replace,
            0,
        );
        d.apply_adjustment(Adjustment::Invert);

        let pixels = &d.layers().get(0).unwrap().pixels;
        // Inside the marquee the white background inverts to black.
        assert!(pixels.get(4, 8).r < 5, "selected pixels were not adjusted");
        // Outside it nothing moved.
        assert_eq!(pixels.get(12, 8), Rgba8::WHITE);
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

    /// A document with a blue field and a yellow bar down the right — a
    /// stand-in for a subject against a background.
    fn erase_fixture() -> Document {
        let mut d = Document::new(40, 20, Rgba8::opaque(30, 120, 220));
        if let Some(layer) = d.active_layer_mut() {
            layer
                .pixels
                .fill_rect(Rect::new(20, 0, 20, 20), Rgba8::opaque(220, 200, 40));
        }
        d.commit("Setup");
        d
    }

    fn erase_options() -> BackgroundEraseOptions {
        BackgroundEraseOptions {
            sampling: Sampling::Continuous,
            limits: Limits::Contiguous,
            tolerance: 40,
            protect_foreground: false,
        }
    }

    /// A red document with a blue square in the middle of the active layer.
    fn clipboard_fixture() -> Document {
        let mut d = Document::new(20, 20, Rgba8::opaque(200, 0, 0));
        if let Some(layer) = d.active_layer_mut() {
            layer.pixels.fill_rect(Rect::new(5, 5, 10, 10), Rgba8::opaque(0, 0, 200));
        }
        d.commit("Setup");
        d
    }

    #[test]
    fn copying_takes_the_selection_and_says_where_it_came_from() {
        let mut d = clipboard_fixture();
        d.select_rect(Rect::new(5, 5, 10, 10), SelectionOp::Replace, 0);

        let (pixels, origin) = d.copy_selection(false).expect("nothing was copied");
        assert_eq!(origin, (5, 5), "the copy forgot where it came from");
        assert_eq!((pixels.width(), pixels.height()), (10, 10));
        assert_eq!(pixels.get(0, 0), Rgba8::opaque(0, 0, 200));
    }

    #[test]
    fn copying_without_a_selection_copies_nothing() {
        let mut d = clipboard_fixture();
        assert!(d.copy_selection(false).is_none());
    }

    #[test]
    fn what_falls_outside_the_selection_comes_out_transparent() {
        // A copy is the shape of the selection, not of its bounding box.
        let mut d = clipboard_fixture();
        d.select_ellipse(Rect::new(0, 0, 20, 20), SelectionOp::Replace, 0);

        let (pixels, _) = d.copy_selection(true).unwrap();
        assert_eq!(pixels.get(10, 10).a, 255, "the middle of the ellipse was dropped");
        assert_eq!(pixels.get(0, 0).a, 0, "a corner outside the ellipse was copied");
    }

    #[test]
    fn a_merged_copy_sees_the_layers_below() {
        let mut d = clipboard_fixture();
        // A second layer with a hole in it: merged sees red through the hole,
        // an ordinary copy sees nothing there.
        d.add_layer(None);
        if let Some(layer) = d.active_layer_mut() {
            layer.pixels.fill_rect(Rect::new(0, 0, 5, 5), Rgba8::opaque(0, 200, 0));
        }
        d.select_rect(Rect::new(0, 0, 20, 20), SelectionOp::Replace, 0);

        let (plain, _) = d.copy_selection(false).unwrap();
        assert_eq!(plain.get(10, 10).a, 0, "the empty part of the layer was not empty");

        let (merged, _) = d.copy_selection(true).unwrap();
        assert_eq!(merged.get(10, 10), Rgba8::opaque(0, 0, 200), "merged missed the layer below");
        assert_eq!(merged.get(2, 2), Rgba8::opaque(0, 200, 0));
    }

    #[test]
    fn pasting_puts_the_pixels_on_a_layer_of_their_own() {
        let mut d = clipboard_fixture();
        let before = d.layer_count();
        let patch = Pixmap::filled(4, 4, Rgba8::opaque(0, 255, 0));

        d.paste_into(patch, (2, 3), PasteMode::Plain);
        assert_eq!(d.layer_count(), before + 1);
        assert_eq!(d.composite().get(2, 3), Rgba8::opaque(0, 255, 0));
        assert_eq!(d.composite().get(1, 3), Rgba8::opaque(200, 0, 0), "the paste spread");

        assert!(d.undo(), "the paste left no undo step");
        assert_eq!(d.layer_count(), before);
    }

    #[test]
    fn pasting_into_a_selection_is_masked_to_it() {
        let mut d = clipboard_fixture();
        d.select_rect(Rect::new(0, 0, 4, 4), SelectionOp::Replace, 0);

        // A patch bigger than the selection: only the part inside shows.
        let patch = Pixmap::filled(10, 10, Rgba8::opaque(0, 255, 0));
        d.paste_into(patch, (0, 0), PasteMode::Into);

        assert_eq!(d.composite().get(1, 1), Rgba8::opaque(0, 255, 0));
        assert_eq!(d.composite().get(6, 6), Rgba8::opaque(0, 0, 200), "it leaked outside");
        // As a mask, so the pasted pixels are still all there underneath.
        let layer = d.active_layer().unwrap();
        assert!(layer.mask.is_some(), "Paste Into did not make a mask");
    }

    #[test]
    fn pasting_outside_a_selection_is_the_other_way_round() {
        let mut d = clipboard_fixture();
        d.select_rect(Rect::new(0, 0, 4, 4), SelectionOp::Replace, 0);

        let patch = Pixmap::filled(10, 10, Rgba8::opaque(0, 255, 0));
        d.paste_into(patch, (0, 0), PasteMode::Outside);

        assert_eq!(d.composite().get(1, 1), Rgba8::opaque(200, 0, 0), "it landed inside");
        assert_eq!(d.composite().get(6, 6), Rgba8::opaque(0, 255, 0));
    }

    #[test]
    fn entering_quick_mask_with_no_marquee_selects_everything() {
        // Otherwise the first black stroke would have nothing to subtract from.
        let mut d = Document::new(16, 16, Rgba8::WHITE);
        assert!(!d.has_selection());
        d.set_quick_mask(true);
        assert!(d.selection().is_full());
    }

    #[test]
    fn leaving_quick_mask_with_everything_masked_in_drops_the_marquee() {
        // A mask covering the whole canvas and no marquee at all mean the same
        // thing to every tool, and the ants round the edge would be noise.
        let mut d = Document::new(16, 16, Rgba8::WHITE);
        d.set_quick_mask(true);
        d.set_quick_mask(false);
        assert!(!d.has_selection());
    }

    #[test]
    fn painting_black_in_quick_mask_masks_and_leaves_the_pixels_alone() {
        let mut d = Document::new(40, 40, Rgba8::WHITE);
        d.set_quick_mask(true);

        let brush = Brush { size: 12.0, hardness: 1.0, ..Brush::default() };
        d.begin_stroke(&brush, 20.0, 20.0, 1.0);
        d.end_stroke(Rgba8::BLACK, 1.0);

        assert_eq!(d.selection().coverage_at(20, 20), 0.0, "black did not mask");
        assert_eq!(d.selection().coverage_at(2, 2), 1.0, "the mask spread beyond the brush");
        assert_eq!(d.composite().get(20, 20), Rgba8::WHITE, "the image was painted on");
    }

    #[test]
    fn painting_white_in_quick_mask_selects_again() {
        let mut d = Document::new(40, 40, Rgba8::WHITE);
        d.set_quick_mask(true);
        let brush = Brush { size: 20.0, hardness: 1.0, ..Brush::default() };

        d.begin_stroke(&brush, 20.0, 20.0, 1.0);
        d.end_stroke(Rgba8::BLACK, 1.0);
        assert_eq!(d.selection().coverage_at(20, 20), 0.0);

        let small = Brush { size: 8.0, hardness: 1.0, ..Brush::default() };
        d.begin_stroke(&small, 20.0, 20.0, 1.0);
        d.end_stroke(Rgba8::WHITE, 1.0);
        assert_eq!(d.selection().coverage_at(20, 20), 1.0, "white did not select");
        // Well inside the black stroke and clear of both the white one and the
        // antialiased edge either brush leaves.
        assert_eq!(d.selection().coverage_at(20, 26), 0.0, "the smaller brush reached too far");
    }

    #[test]
    fn a_soft_brush_in_quick_mask_gives_a_soft_selection() {
        // The reason to build a selection this way rather than with the lasso.
        let mut d = Document::new(80, 80, Rgba8::WHITE);
        d.set_quick_mask(true);
        let brush = Brush { size: 40.0, hardness: 0.0, ..Brush::default() };
        d.begin_stroke(&brush, 40.0, 40.0, 1.0);
        d.end_stroke(Rgba8::BLACK, 1.0);

        // Half way out along the falloff of a 40px soft brush, where a hard
        // brush would have left either fully masked or fully selected.
        let edge = d.selection().coverage_at(40, 30);
        assert!(edge > 0.1 && edge < 0.9, "the mask edge came out hard: {edge}");
    }

    #[test]
    fn a_shape_layer_is_a_colour_poured_through_a_mask() {
        let mut d = Document::new(60, 40, Rgba8::WHITE);
        let points = crate::shape::rectangle_points((10.0, 10.0, 20.0, 20.0));
        let red = Rgba8::opaque(220, 30, 30);
        let id = d.add_shape_layer(&points, red, "Rectangle").expect("no shape layer was added");

        let layer = d.layers().by_id(id).unwrap();
        assert!(matches!(layer.kind, LayerKind::SolidColor(c) if c == red));
        assert!(layer.mask.is_some(), "the shape was not cut into a mask");

        // Inside the rectangle the colour shows; outside, the layer below does.
        assert_eq!(d.composite().get(20, 20), red);
        assert_eq!(d.composite().get(2, 2), Rgba8::WHITE, "the shape covered the whole canvas");
        assert!(d.undo(), "the shape layer left no undo step");
        assert_eq!(d.layer_count(), 1);
    }

    #[test]
    fn a_shape_layers_colour_can_be_changed_after_the_fact() {
        // The point of committing a fill layer rather than pixels: it stays a
        // colour, so the Layers panel can recolour it.
        let mut d = Document::new(40, 40, Rgba8::WHITE);
        let points = crate::shape::rectangle_points((5.0, 5.0, 20.0, 20.0));
        let id = d.add_shape_layer(&points, Rgba8::opaque(10, 10, 200), "Rectangle").unwrap();

        // The shape layer is the active one, having just been added.
        assert_eq!(d.active_layer_id(), id);
        if let Some(layer) = d.active_layer_mut() {
            layer.kind = LayerKind::SolidColor(Rgba8::opaque(10, 200, 10));
        }
        assert_eq!(d.composite().get(10, 10), Rgba8::opaque(10, 200, 10));
    }

    #[test]
    fn a_shape_in_pixels_mode_paints_the_active_layer() {
        let mut d = Document::new(40, 40, Rgba8::WHITE);
        let points = crate::shape::rectangle_points((10.0, 10.0, 10.0, 10.0));
        let blue = Rgba8::opaque(20, 40, 200);
        assert!(!d.fill_shape(&points, blue, 1.0).is_empty());

        assert_eq!(d.layer_count(), 1, "pixels mode added a layer");
        assert_eq!(d.composite().get(15, 15), blue);
        assert_eq!(d.composite().get(2, 2), Rgba8::WHITE);
        assert!(d.undo());
        assert_eq!(d.composite().get(15, 15), Rgba8::WHITE);
    }

    #[test]
    fn a_shape_in_path_mode_draws_nothing_and_leaves_a_closed_path() {
        let mut d = Document::new(40, 40, Rgba8::WHITE);
        let points = crate::shape::rectangle_points((10.0, 10.0, 10.0, 10.0));
        assert!(d.append_shape_path(&points));

        assert_eq!(d.composite().get(15, 15), Rgba8::WHITE, "path mode painted pixels");
        let path = d.paths().active().expect("no work path was made");
        assert!(!path.is_empty());
        assert!(!path.is_editing(), "the path was left open for the Pen to extend");

        // And it can then be filled, which is what Path mode is for.
        assert!(!d.fill_active_path(Rgba8::BLACK, 1.0).is_empty());
        assert_eq!(d.composite().get(15, 15), Rgba8::BLACK);
    }

    #[test]
    fn a_shape_needs_more_than_a_line() {
        let mut d = Document::new(40, 40, Rgba8::WHITE);
        let degenerate = [(10.0, 10.0), (20.0, 10.0)];
        assert!(d.add_shape_layer(&degenerate, Rgba8::BLACK, "Rectangle").is_none());
        assert!(!d.append_shape_path(&degenerate));
        assert!(d.fill_shape(&degenerate, Rgba8::BLACK, 1.0).is_empty());
    }

    #[test]
    fn a_background_erase_stroke_is_one_undo_step() {
        let mut d = erase_fixture();
        let brush = Brush { size: 12.0, hardness: 1.0, ..Brush::default() };
        assert!(d.begin_background_erase(&brush, erase_options(), None, Rgba8::BLACK, 6.0, 10.0,
                                         1.0));
        d.extend_background_erase(&brush, 10.0, 10.0, 1.0, Rgba8::BLACK);
        d.extend_background_erase(&brush, 14.0, 10.0, 1.0, Rgba8::BLACK);
        assert!(d.end_background_erase());

        assert_eq!(d.layers().by_id(d.active_layer_id()).unwrap().pixels.get(10, 10).a, 0,
                   "the background was not erased");
        assert!(d.undo(), "nothing to undo");
        assert_eq!(d.layers().by_id(d.active_layer_id()).unwrap().pixels.get(10, 10).a, 255,
                   "one undo did not take the whole stroke back");
    }

    #[test]
    fn a_background_erase_leaves_what_it_did_not_sample() {
        // The point of the tool: the crosshair rides the background while the
        // brush overlaps the subject, and the subject survives.
        let mut d = erase_fixture();
        let brush = Brush { size: 20.0, hardness: 1.0, ..Brush::default() };
        d.begin_background_erase(&brush, erase_options(), None, Rgba8::BLACK, 16.0, 10.0, 1.0);
        d.end_background_erase();

        let pixels = &d.layers().by_id(d.active_layer_id()).unwrap().pixels;
        assert_eq!(pixels.get(14, 10).a, 0, "the sampled background survived");
        assert_eq!(pixels.get(24, 10).a, 255, "the subject was erased too");
    }

    #[test]
    fn a_background_erase_refuses_a_layer_locked_against_it() {
        let mut d = erase_fixture();
        if let Some(layer) = d.active_layer_mut() {
            // Erasing only ever changes alpha, which is exactly what this lock
            // forbids.
            layer.lock_transparency = true;
        }
        let brush = Brush { size: 10.0, ..Brush::default() };
        assert!(!d.begin_background_erase(&brush, erase_options(), None, Rgba8::BLACK, 6.0, 10.0,
                                          1.0));
    }

    #[test]
    fn the_magic_eraser_clears_the_region_it_is_clicked_in() {
        let mut d = erase_fixture();
        let dirty = d.magic_erase(5, 10, 32, true, true, false, 1.0);
        assert!(!dirty.is_empty());

        let pixels = &d.layers().by_id(d.active_layer_id()).unwrap().pixels;
        assert_eq!(pixels.get(5, 10).a, 0, "the clicked region was not erased");
        assert_eq!(pixels.get(30, 10).a, 255, "the erase crossed into the subject");

        assert!(d.undo(), "the magic eraser left no undo step");
        assert_eq!(
            d.layers().by_id(d.active_layer_id()).unwrap().pixels.get(5, 10).a,
            255
        );
    }

    #[test]
    fn the_magic_eraser_at_half_opacity_leaves_the_region_half_there() {
        let mut d = erase_fixture();
        d.magic_erase(5, 10, 32, true, false, false, 0.5);
        let alpha = d.layers().by_id(d.active_layer_id()).unwrap().pixels.get(5, 10).a;
        assert!((alpha as i32 - 128).abs() <= 2, "half an erase left alpha {alpha}");
    }

    #[test]
    fn the_magic_eraser_refuses_a_locked_layer() {
        let mut d = erase_fixture();
        if let Some(layer) = d.active_layer_mut() {
            layer.lock_pixels = true;
        }
        assert!(d.magic_erase(5, 10, 32, true, true, false, 1.0).is_empty());
    }

    #[test]
    fn a_pattern_stroke_lays_down_the_pattern() {
        // The Pattern Stamp paints the tile itself, so what lands under the
        // brush is whatever the pattern has at that document pixel.
        let mut d = Document::new(80, 80, Rgba8::WHITE);
        let brush = Brush { size: 30.0, hardness: 1.0, ..Brush::default() };
        assert!(d.begin_pattern_stroke(&brush, 40.0, 40.0, 1.0, 0, true));
        d.end_clone_stroke(1.0);

        let tile = pattern::tile(0).unwrap();
        let painted = d.composite().get(40, 40);
        let expected = tile.get(40 % pattern::TILE as i32, 40 % pattern::TILE as i32);
        assert_eq!(painted, expected, "the pattern was not laid down as it is");
        assert_ne!(d.composite().get(2, 2), painted, "the stroke painted outside the brush");
    }

    #[test]
    fn an_aligned_pattern_joins_up_across_strokes() {
        // Aligned pins the tile to the document, so two separate strokes over
        // neighbouring ground continue one sheet rather than each starting the
        // pattern afresh.
        let brush = Brush { size: 24.0, hardness: 1.0, ..Brush::default() };

        let mut aligned = Document::new(80, 80, Rgba8::WHITE);
        aligned.begin_pattern_stroke(&brush, 20.0, 40.0, 1.0, 0, true);
        aligned.end_clone_stroke(1.0);
        aligned.begin_pattern_stroke(&brush, 44.0, 40.0, 1.0, 0, true);
        aligned.end_clone_stroke(1.0);

        let mut single = Document::new(80, 80, Rgba8::WHITE);
        single.begin_pattern_stroke(&brush, 20.0, 40.0, 1.0, 0, true);
        single.extend_stroke(&brush, 44.0, 40.0, 1.0);
        single.end_clone_stroke(1.0);

        assert_eq!(
            aligned.composite().get(44, 40),
            single.composite().get(44, 40),
            "the second aligned stroke did not continue the same sheet"
        );
    }

    #[test]
    fn an_unaligned_pattern_restarts_at_each_stroke() {
        // Unaligned pins the tile to where the stroke began, so the tile's own
        // corner lands under the first dab and the same ground comes out
        // shifted depending on where the user started.
        let brush = Brush { size: 24.0, hardness: 1.0, ..Brush::default() };
        let corner = pattern::tile(0).unwrap().get(0, 0);

        let mut from_left = Document::new(96, 80, Rgba8::WHITE);
        from_left.begin_pattern_stroke(&brush, 20.0, 40.0, 1.0, 0, false);
        from_left.extend_stroke(&brush, 76.0, 40.0, 1.0);
        from_left.end_clone_stroke(1.0);
        assert_eq!(from_left.composite().get(20, 40), corner, "the tile did not start at the dab");

        let mut from_the_middle = Document::new(96, 80, Rgba8::WHITE);
        from_the_middle.begin_pattern_stroke(&brush, 44.0, 40.0, 1.0, 0, false);
        from_the_middle.extend_stroke(&brush, 76.0, 40.0, 1.0);
        from_the_middle.end_clone_stroke(1.0);
        assert_eq!(from_the_middle.composite().get(44, 40), corner);

        // Over the ground both strokes covered, the two must disagree
        // somewhere: they are the same pattern laid 24 pixels apart.
        let (left, middle) = (from_left.composite(), from_the_middle.composite());
        let shifted = (48..72).any(|x| left.get(x, 40) != middle.get(x, 40));
        assert!(shifted, "unaligned strokes started the pattern at the same place");
    }

    #[test]
    fn a_pattern_stroke_refuses_a_locked_layer() {
        let mut d = Document::new(40, 40, Rgba8::WHITE);
        if let Some(layer) = d.active_layer_mut() {
            layer.lock_pixels = true;
        }
        let brush = Brush { size: 10.0, ..Brush::default() };
        assert!(!d.begin_pattern_stroke(&brush, 20.0, 20.0, 1.0, 0, true));
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
        assert_eq!(d.display_name(), "Untitled-1 (RGB/8)");
        d.add_layer(None);
        assert_eq!(d.display_name(), "Untitled-1* (RGB/8)");
        d.mark_saved();
        assert_eq!(d.display_name(), "Untitled-1 (RGB/8)");
    }

    #[test]
    fn display_name_uses_the_file_name() {
        let mut d = doc();
        d.path = Some("/home/user/pictures/sunset.psd".to_string());
        assert_eq!(d.display_name(), "sunset.psd (RGB/8)");
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

    #[test]
    fn grayscale_converts_pixels() {
        let red = Rgba8::new(255, 0, 0, 255);
        let mut d = Document::new(4, 4, red);
        assert_eq!(d.color_mode(), ImageMode::Rgb);
        let px_before = d.layers().as_slice()[0].pixels.get(0, 0);
        assert_eq!(px_before.r, 255);
        assert_eq!(px_before.g, 0);

        d.set_color_mode(ImageMode::Grayscale);
        assert_eq!(d.color_mode(), ImageMode::Grayscale);
        let px_after = d.layers().as_slice()[0].pixels.get(0, 0);
        assert_eq!(px_after.r, px_after.g);
        assert_eq!(px_after.g, px_after.b);
        assert!(px_after.r > 0 && px_after.r < 255, "should be a mid gray, got {}", px_after.r);
    }
}

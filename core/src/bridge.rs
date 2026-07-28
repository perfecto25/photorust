//! The CXX-Qt bridge — the engine's entire public face to the C++ shell.
//!
//! Design rules for this file (see CLAUDE.md §3):
//!
//! * Keep it **thin**. Nothing here does image maths; it translates between Qt
//!   types and [`Document`] calls.
//! * Keep it **stable**. Changing a signature costs a rebuild on both sides.
//! * **Never copy pixel buffers across the boundary.** The composited image is
//!   moved into a `QImage` via `from_raw_bytes`, which takes ownership of the
//!   Rust allocation — the bytes are never duplicated.
//!
//! Indices used here are **panel indices**: 0 is the *topmost* layer, matching
//! what the user sees in the Layers panel. The engine stores the stack the
//! other way up, so every index is flipped at this boundary and nowhere else.

use crate::blend::BlendMode;
use crate::brush::Brush;
use crate::buffer::{Pixmap, Rect, Rgba8};
use crate::document::Document;
use crate::filters::{Adjustment, Filter};
use crate::layer::LayerId;
use crate::selection::SelectionOp;
// `rust_mut()` on a generated QObject comes from this trait.
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QColor, QImage, QImageFormat, QString};

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qimage.h");
        type QImage = cxx_qt_lib::QImage;

        include!("cxx-qt-lib/qcolor.h");
        type QColor = cxx_qt_lib::QColor;
    }

    unsafe extern "RustQt" {
        /// The engine handle. One instance per application; it owns the open
        /// document.
        #[qobject]
        #[qproperty(i32, layer_count, cxx_name = "layerCount")]
        #[qproperty(i32, active_layer_index, cxx_name = "activeLayerIndex")]
        #[qproperty(i32, canvas_width, cxx_name = "canvasWidth")]
        #[qproperty(i32, canvas_height, cxx_name = "canvasHeight")]
        #[qproperty(bool, modified)]
        #[qproperty(QString, document_title, cxx_name = "documentTitle")]
        type Engine = super::EngineRust;

        /// The document's pixels changed and the canvas should repaint.
        #[qsignal]
        #[cxx_name = "canvasChanged"]
        fn canvas_changed(self: Pin<&mut Engine>);

        /// The layer stack's structure or properties changed.
        #[qsignal]
        #[cxx_name = "layersChanged"]
        fn layers_changed(self: Pin<&mut Engine>);

        /// The undo history changed.
        #[qsignal]
        #[cxx_name = "historyChanged"]
        fn history_changed(self: Pin<&mut Engine>);

        /// The selection changed.
        #[qsignal]
        #[cxx_name = "selectionChanged"]
        fn selection_changed(self: Pin<&mut Engine>);
    }

    // -- document ----------------------------------------------------------
    unsafe extern "RustQt" {
        /// Replace the document with a new one.
        /// `fill`: 0 = white, 1 = transparent, 2 = background colour.
        #[qinvokable]
        #[cxx_name = "newDocument"]
        fn new_document(self: Pin<&mut Engine>, width: i32, height: i32, fill: i32);

        /// Open an image file. Returns false if it could not be read.
        #[qinvokable]
        #[cxx_name = "openFile"]
        fn open_file(self: Pin<&mut Engine>, path: &QString) -> bool;

        /// Replace the document with an image the shell already decoded.
        /// Used for formats Qt's plugins handle better than we would.
        #[qinvokable]
        #[cxx_name = "loadImage"]
        fn load_image(self: Pin<&mut Engine>, image: &QImage, path: &QString) -> bool;

        /// Save to `path`. Returns false for anything but `.psd` — the shell
        /// writes other formats through `QImage::save`.
        #[qinvokable]
        #[cxx_name = "saveFile"]
        fn save_file(self: Pin<&mut Engine>, path: &QString) -> bool;

        /// Record that the shell saved the document to `path`, clearing the
        /// modified flag.
        #[qinvokable]
        #[cxx_name = "markSavedAs"]
        fn mark_saved_as(self: Pin<&mut Engine>, path: &QString);

        /// The composited document as a premultiplied ARGB image.
        #[qinvokable]
        #[cxx_name = "compositeImage"]
        fn composite_image(self: &Engine) -> QImage;

        /// The composite with the in-progress brush stroke drawn on top.
        /// Falls back to [`Engine::composite_image`] when no stroke is active.
        #[qinvokable]
        #[cxx_name = "previewImage"]
        fn preview_image(self: &Engine) -> QImage;

        /// Resize the canvas without scaling the content.
        #[qinvokable]
        #[cxx_name = "resizeCanvas"]
        fn resize_canvas(self: Pin<&mut Engine>, width: i32, height: i32);
    }

    // -- layers ------------------------------------------------------------
    unsafe extern "RustQt" {
        /// Name of the layer at a panel index (0 = topmost).
        #[qinvokable]
        #[cxx_name = "layerName"]
        fn layer_name(self: &Engine, index: i32) -> QString;

        #[qinvokable]
        #[cxx_name = "layerVisible"]
        fn layer_visible(self: &Engine, index: i32) -> bool;

        /// Layer opacity as a percentage, 0-100, as the panel displays it.
        #[qinvokable]
        #[cxx_name = "layerOpacity"]
        fn layer_opacity(self: &Engine, index: i32) -> i32;

        #[qinvokable]
        #[cxx_name = "layerFillOpacity"]
        fn layer_fill_opacity(self: &Engine, index: i32) -> i32;

        /// Blend mode as a [`BlendMode`] discriminant.
        #[qinvokable]
        #[cxx_name = "layerBlendMode"]
        fn layer_blend_mode(self: &Engine, index: i32) -> i32;

        #[qinvokable]
        #[cxx_name = "layerIsClipping"]
        fn layer_is_clipping(self: &Engine, index: i32) -> bool;

        #[qinvokable]
        #[cxx_name = "layerHasMask"]
        fn layer_has_mask(self: &Engine, index: i32) -> bool;

        /// A square thumbnail of the layer's content, for the panel.
        #[qinvokable]
        #[cxx_name = "layerThumbnail"]
        fn layer_thumbnail(self: &Engine, index: i32, size: i32) -> QImage;

        #[qinvokable]
        #[cxx_name = "setActiveLayer"]
        fn set_active_layer(self: Pin<&mut Engine>, index: i32);

        #[qinvokable]
        #[cxx_name = "setLayerVisible"]
        fn set_layer_visible(self: Pin<&mut Engine>, index: i32, visible: bool);

        #[qinvokable]
        #[cxx_name = "setLayerOpacity"]
        fn set_layer_opacity(self: Pin<&mut Engine>, index: i32, percent: i32);

        #[qinvokable]
        #[cxx_name = "setLayerFillOpacity"]
        fn set_layer_fill_opacity(self: Pin<&mut Engine>, index: i32, percent: i32);

        #[qinvokable]
        #[cxx_name = "setLayerBlendMode"]
        fn set_layer_blend_mode(self: Pin<&mut Engine>, index: i32, mode: i32);

        #[qinvokable]
        #[cxx_name = "setLayerName"]
        fn set_layer_name(self: Pin<&mut Engine>, index: i32, name: &QString);

        #[qinvokable]
        #[cxx_name = "setLayerClipping"]
        fn set_layer_clipping(self: Pin<&mut Engine>, index: i32, clipping: bool);

        #[qinvokable]
        #[cxx_name = "addLayer"]
        fn add_layer(self: Pin<&mut Engine>);

        /// Add an adjustment layer by menu name, e.g. "Levels".
        #[qinvokable]
        #[cxx_name = "addAdjustmentLayer"]
        fn add_adjustment_layer(self: Pin<&mut Engine>, kind: &QString);

        #[qinvokable]
        #[cxx_name = "duplicateLayer"]
        fn duplicate_layer(self: Pin<&mut Engine>, index: i32);

        #[qinvokable]
        #[cxx_name = "deleteLayer"]
        fn delete_layer(self: Pin<&mut Engine>, index: i32);

        /// Move a layer to a new panel position.
        #[qinvokable]
        #[cxx_name = "moveLayer"]
        fn move_layer(self: Pin<&mut Engine>, from: i32, to: i32);

        #[qinvokable]
        #[cxx_name = "mergeLayerDown"]
        fn merge_layer_down(self: Pin<&mut Engine>, index: i32);

        #[qinvokable]
        #[cxx_name = "flattenImage"]
        fn flatten_image(self: Pin<&mut Engine>);

        #[qinvokable]
        #[cxx_name = "addLayerMask"]
        fn add_layer_mask(self: Pin<&mut Engine>, index: i32, reveal_all: bool);

        /// Nudge a layer by a pixel delta, as the Move tool does.
        #[qinvokable]
        #[cxx_name = "offsetLayer"]
        fn offset_layer(self: Pin<&mut Engine>, index: i32, dx: i32, dy: i32);
    }

    // -- painting ----------------------------------------------------------
    unsafe extern "RustQt" {
        /// Configure the brush from the tool options bar.
        /// `hardness`, `opacity` and `flow` are percentages, 0-100.
        #[qinvokable]
        #[cxx_name = "setBrush"]
        fn set_brush(
            self: Pin<&mut Engine>,
            size: f32,
            hardness: i32,
            opacity: i32,
            flow: i32,
            spacing: i32,
        );

        #[qinvokable]
        #[cxx_name = "setForegroundColor"]
        fn set_foreground_color(self: Pin<&mut Engine>, color: &QColor);

        #[qinvokable]
        #[cxx_name = "foregroundColor"]
        fn foreground_color(self: &Engine) -> QColor;

        #[qinvokable]
        #[cxx_name = "setBackgroundColor"]
        fn set_background_color(self: Pin<&mut Engine>, color: &QColor);

        #[qinvokable]
        #[cxx_name = "backgroundColor"]
        fn background_color(self: &Engine) -> QColor;

        /// Swap foreground and background — the X shortcut.
        #[qinvokable]
        #[cxx_name = "swapColors"]
        fn swap_colors(self: Pin<&mut Engine>);

        /// Reset to black/white — the D shortcut.
        #[qinvokable]
        #[cxx_name = "resetColors"]
        fn reset_colors(self: Pin<&mut Engine>);

        /// Begin a stroke at a document-space point. Returns false when the
        /// active layer cannot be painted on.
        #[qinvokable]
        #[cxx_name = "beginStroke"]
        fn begin_stroke(self: Pin<&mut Engine>, x: f32, y: f32, pressure: f32) -> bool;

        #[qinvokable]
        #[cxx_name = "extendStroke"]
        fn extend_stroke(self: Pin<&mut Engine>, x: f32, y: f32, pressure: f32);

        /// Finish the stroke and bake it into the layer.
        #[qinvokable]
        #[cxx_name = "endStroke"]
        fn end_stroke(self: Pin<&mut Engine>);

        #[qinvokable]
        #[cxx_name = "cancelStroke"]
        fn cancel_stroke(self: Pin<&mut Engine>);

        /// True while a stroke is in progress.
        #[qinvokable]
        #[cxx_name = "isStroking"]
        fn is_stroking(self: &Engine) -> bool;

        /// Set to true to paint with the background colour (the eraser uses
        /// this to clear instead).
        #[qinvokable]
        #[cxx_name = "setEraseMode"]
        fn set_erase_mode(self: Pin<&mut Engine>, erasing: bool);

        /// Fill the selection with the foreground colour.
        #[qinvokable]
        #[cxx_name = "fillForeground"]
        fn fill_foreground(self: Pin<&mut Engine>);

        /// Fill the selection with the background colour.
        #[qinvokable]
        #[cxx_name = "fillBackground"]
        fn fill_background(self: Pin<&mut Engine>);

        /// Erase the selected pixels.
        #[qinvokable]
        #[cxx_name = "clearSelection"]
        fn clear_selection(self: Pin<&mut Engine>);

        /// Sample the composited colour at a point, for the eyedropper.
        #[qinvokable]
        #[cxx_name = "pickColor"]
        fn pick_color(self: &Engine, x: i32, y: i32) -> QColor;
    }

    // -- selection ---------------------------------------------------------
    unsafe extern "RustQt" {
        /// `op`: 0 = replace, 1 = add, 2 = subtract, 3 = intersect.
        #[qinvokable]
        #[cxx_name = "selectRect"]
        fn select_rect(
            self: Pin<&mut Engine>,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            op: i32,
        );

        #[qinvokable]
        #[cxx_name = "selectEllipse"]
        fn select_ellipse(
            self: Pin<&mut Engine>,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            op: i32,
        );

        #[qinvokable]
        #[cxx_name = "selectAll"]
        fn select_all(self: Pin<&mut Engine>);

        #[qinvokable]
        fn deselect(self: Pin<&mut Engine>);

        #[qinvokable]
        #[cxx_name = "invertSelection"]
        fn invert_selection(self: Pin<&mut Engine>);

        #[qinvokable]
        #[cxx_name = "featherSelection"]
        fn feather_selection(self: Pin<&mut Engine>, radius: i32);

        #[qinvokable]
        #[cxx_name = "hasSelection"]
        fn has_selection(self: &Engine) -> bool;

        /// Bounding box of the selection, as `[x, y, width, height]`. All
        /// zeroes when nothing is selected.
        #[qinvokable]
        #[cxx_name = "selectionBounds"]
        fn selection_bounds(self: Pin<&mut Engine>) -> Vec<i32>;

        /// The selection mask as a greyscale image, for marching ants.
        #[qinvokable]
        #[cxx_name = "selectionMask"]
        fn selection_mask(self: &Engine) -> QImage;
    }

    // -- filters and adjustments -------------------------------------------
    unsafe extern "RustQt" {
        /// Apply a filter by menu name. `p1`/`p2` are filter-specific.
        #[qinvokable]
        #[cxx_name = "applyFilter"]
        fn apply_filter(self: Pin<&mut Engine>, name: &QString, p1: f32, p2: f32);

        /// Apply an adjustment destructively by menu name.
        #[qinvokable]
        #[cxx_name = "applyAdjustment"]
        fn apply_adjustment(self: Pin<&mut Engine>, name: &QString, p1: f32, p2: f32, p3: f32);
    }

    // -- history -----------------------------------------------------------
    unsafe extern "RustQt" {
        #[qinvokable]
        fn undo(self: Pin<&mut Engine>) -> bool;

        #[qinvokable]
        fn redo(self: Pin<&mut Engine>) -> bool;

        #[qinvokable]
        #[cxx_name = "canUndo"]
        fn can_undo(self: &Engine) -> bool;

        #[qinvokable]
        #[cxx_name = "canRedo"]
        fn can_redo(self: &Engine) -> bool;

        /// Label for the Edit ▸ Undo menu item, e.g. "Brush Tool".
        #[qinvokable]
        #[cxx_name = "undoName"]
        fn undo_name(self: &Engine) -> QString;

        #[qinvokable]
        #[cxx_name = "redoName"]
        fn redo_name(self: &Engine) -> QString;

        /// Number of rows in the History panel.
        #[qinvokable]
        #[cxx_name = "historyCount"]
        fn history_count(self: &Engine) -> i32;

        #[qinvokable]
        #[cxx_name = "historyName"]
        fn history_name(self: &Engine, index: i32) -> QString;

        /// Index of the row the document currently reflects.
        #[qinvokable]
        #[cxx_name = "historyCursor"]
        fn history_cursor(self: &Engine) -> i32;

        #[qinvokable]
        #[cxx_name = "jumpToHistory"]
        fn jump_to_history(self: Pin<&mut Engine>, index: i32);
    }

    // -- static metadata ---------------------------------------------------
    unsafe extern "RustQt" {
        /// Blend mode names in Layers-panel order. Populates the combo box so
        /// the two sides cannot drift apart.
        #[qinvokable]
        #[cxx_name = "blendModeNames"]
        fn blend_mode_names(self: &Engine) -> QString;

        /// Indices at which the blend-mode combo box draws a separator,
        /// comma-separated.
        #[qinvokable]
        #[cxx_name = "blendModeSeparators"]
        fn blend_mode_separators(self: &Engine) -> QString;
    }
}

/// The Rust side of the [`ffi::Engine`] QObject.
pub struct EngineRust {
    // -- Q_PROPERTY backing fields --
    layer_count: i32,
    active_layer_index: i32,
    canvas_width: i32,
    canvas_height: i32,
    modified: bool,
    document_title: QString,

    // -- engine state --
    doc: Document,
    brush: Brush,
    foreground: Rgba8,
    background: Rgba8,
    erasing: bool,
}

impl Default for EngineRust {
    fn default() -> Self {
        // Photoshop's default new-document size.
        let doc = Document::new(1280, 800, Rgba8::WHITE);
        Self {
            layer_count: doc.layer_count() as i32,
            active_layer_index: 0,
            canvas_width: doc.width() as i32,
            canvas_height: doc.height() as i32,
            modified: false,
            document_title: QString::from("Untitled-1"),
            doc,
            brush: Brush::default(),
            foreground: Rgba8::BLACK,
            background: Rgba8::WHITE,
            erasing: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Conversions
// ---------------------------------------------------------------------------

fn qcolor_to_rgba(c: &QColor) -> Rgba8 {
    Rgba8::new(
        c.red().clamp(0, 255) as u8,
        c.green().clamp(0, 255) as u8,
        c.blue().clamp(0, 255) as u8,
        c.alpha().clamp(0, 255) as u8,
    )
}

fn rgba_to_qcolor(c: Rgba8) -> QColor {
    QColor::from_rgba(c.r as i32, c.g as i32, c.b as i32, c.a as u32 as i32)
}

/// Convert a [`Pixmap`] into a `QImage` that owns its pixels.
///
/// The obvious implementation wraps the Rust allocation with
/// `QImage::from_raw_bytes` and lets Qt free it through a Rust deleter — no
/// copy at all. That is **not safe here**: the wrapper `QImage` is a temporary
/// that dies when this function returns across the bridge, and its destructor
/// runs the deleter and frees the Rust buffer. The `QImage` the C++ side ends
/// up holding then points at freed memory.
///
/// The failure is easy to miss because it scales with allocation size: a
/// multi-megabyte composite usually still reads back as the right pixels
/// because nothing has reused the pages yet, while a two-kilobyte layer
/// thumbnail fills with whatever was allocated next and visibly corrupts.
///
/// So the borrowed image is deep-copied while it is unquestionably still
/// alive, and the copy — backed by Qt-owned memory — is what crosses the
/// boundary.
///
/// This costs one memcpy per composite, against CLAUDE.md §8's "avoid copying
/// buffers across the FFI bridge". Removing it means giving Qt a buffer whose
/// lifetime is genuinely independent of this call; the natural fix is to keep
/// a persistent back-buffer in [`EngineRust`] and hand out a `QImage` that
/// borrows it, which is worth doing once the canvas renderer lands.
fn pixmap_to_qimage(mut pm: Pixmap) -> QImage {
    if pm.is_empty() {
        return QImage::default();
    }
    let (w, h) = (pm.width() as i32, pm.height() as i32);
    // Qt paints premultiplied ARGB fastest; convert once here rather than
    // making Qt do it on every repaint.
    pm.premultiply();

    // SAFETY: the buffer is exactly `w * h * 4` bytes with no row padding,
    // which is what `Format_RGBA8888_Premultiplied` describes. `borrowed` owns
    // the allocation and stays alive until the end of this function.
    let borrowed = unsafe {
        QImage::from_raw_bytes(
            pm.into_bytes(),
            w,
            h,
            QImageFormat::Format_RGBA8888_Premultiplied,
        )
    };

    // Deep copy into Qt-owned storage before `borrowed` (and the Rust
    // allocation behind it) is dropped.
    borrowed.copy(&borrowed.rect())
}

/// Copy a `QImage` into a [`Pixmap`].
///
/// This one *does* copy: the source is owned by Qt and may be in any format,
/// so per-pixel conversion is unavoidable. Only used on file open.
fn qimage_to_pixmap(img: &QImage) -> Option<Pixmap> {
    let (w, h) = (img.width(), img.height());
    if w <= 0 || h <= 0 {
        return None;
    }
    let mut pm = Pixmap::new(w as u32, h as u32);
    for y in 0..h {
        for x in 0..w {
            pm.set(x, y, qcolor_to_rgba(&img.pixel_color(x, y)));
        }
    }
    Some(pm)
}

impl EngineRust {
    /// Translate a panel index (0 = topmost) to a [`LayerId`].
    fn layer_id_at(&self, panel_index: i32) -> Option<LayerId> {
        let count = self.doc.layer_count();
        if panel_index < 0 || panel_index as usize >= count {
            return None;
        }
        // The panel lists top-first; the stack is bottom-first.
        let stack_index = count - 1 - panel_index as usize;
        self.doc.layers().get(stack_index).map(|l| l.id)
    }

    /// The inverse of [`EngineRust::layer_id_at`].
    fn panel_index_of(&self, id: LayerId) -> i32 {
        let count = self.doc.layer_count();
        self.doc
            .layers()
            .index_of(id)
            .map_or(-1, |i| (count - 1 - i) as i32)
    }

    /// The colour a stroke should paint with, honouring erase mode.
    fn paint_color(&self) -> Rgba8 {
        if self.erasing {
            self.background
        } else {
            self.foreground
        }
    }
}

impl ffi::Engine {
    // -- internal helpers ---------------------------------------------------

    /// Re-read every Q_PROPERTY from the document and emit the change signals.
    ///
    /// Called after any mutation so the C++ side never has to remember to
    /// refresh — a single funnel is what keeps the two sides consistent.
    fn sync(mut self: core::pin::Pin<&mut Self>) {
        let count = self.doc.layer_count() as i32;
        let active = self.panel_index_of(self.doc.active_layer_id());
        let (w, h) = self.doc.size();
        let modified = self.doc.is_dirty();
        let title = QString::from(self.doc.display_name().as_str());

        self.as_mut().set_layer_count(count);
        self.as_mut().set_active_layer_index(active);
        self.as_mut().set_canvas_width(w as i32);
        self.as_mut().set_canvas_height(h as i32);
        self.as_mut().set_modified(modified);
        self.as_mut().set_document_title(title);

        self.as_mut().canvas_changed();
        self.as_mut().layers_changed();
        self.as_mut().history_changed();
    }

    // -- document -----------------------------------------------------------

    fn new_document(mut self: core::pin::Pin<&mut Self>, width: i32, height: i32, fill: i32) {
        let w = width.clamp(1, 30_000) as u32;
        let h = height.clamp(1, 30_000) as u32;
        let background = self.background;

        let doc = match fill {
            1 => Document::new_transparent(w, h),
            2 => Document::new(w, h, background),
            _ => Document::new(w, h, Rgba8::WHITE),
        };
        self.as_mut().rust_mut().doc = doc;
        self.sync();
    }

    fn open_file(mut self: core::pin::Pin<&mut Self>, path: &QString) -> bool {
        let path = path.to_string();
        let Ok(bytes) = std::fs::read(&path) else {
            return false;
        };

        let pixmap = if path.to_lowercase().ends_with(".psd") {
            match crate::psd::parse(&bytes) {
                Ok(file) => crate::psd::to_pixmap(&file),
                Err(_) => return false,
            }
        } else {
            // Everything else goes through Qt's image plugins, which already
            // cover PNG/JPEG/TIFF/WebP — no reason to reimplement them.
            let Some(pm) = QImage::from_data(&bytes, None).as_ref().and_then(qimage_to_pixmap)
            else {
                return false;
            };
            pm
        };

        let mut doc = Document::from_pixmap(pixmap);
        doc.path = Some(path);
        doc.mark_saved();
        self.as_mut().rust_mut().doc = doc;
        self.sync();
        true
    }

    fn load_image(mut self: core::pin::Pin<&mut Self>, image: &QImage, path: &QString) -> bool {
        let Some(pm) = qimage_to_pixmap(image) else {
            return false;
        };
        let mut doc = Document::from_pixmap(pm);
        let path = path.to_string();
        if !path.is_empty() {
            doc.path = Some(path);
        }
        doc.mark_saved();
        self.as_mut().rust_mut().doc = doc;
        self.sync();
        true
    }

    fn save_file(mut self: core::pin::Pin<&mut Self>, path: &QString) -> bool {
        let path = path.to_string();
        if !path.to_lowercase().ends_with(".psd") {
            // Other formats are written by the shell via `QImage::save`, which
            // reaches Qt's image plugins. Only PSD is ours to write.
            return false;
        }

        let flat = self.doc.flattened(Rgba8::WHITE);
        if std::fs::write(&path, crate::psd::write_psd(&flat)).is_err() {
            return false;
        }

        self.as_mut().rust_mut().doc.path = Some(path);
        self.as_mut().rust_mut().doc.mark_saved();
        self.sync();
        true
    }

    fn mark_saved_as(mut self: core::pin::Pin<&mut Self>, path: &QString) {
        let path = path.to_string();
        self.as_mut().rust_mut().doc.path = Some(path);
        self.as_mut().rust_mut().doc.mark_saved();
        self.sync();
    }

    fn composite_image(&self) -> QImage {
        pixmap_to_qimage(self.doc.composite())
    }

    fn preview_image(&self) -> QImage {
        let color = self.paint_color();
        let opacity = self.brush.opacity;
        match self.doc.preview_stroke(color, opacity) {
            Some(pm) => pixmap_to_qimage(pm),
            None => self.composite_image(),
        }
    }

    fn resize_canvas(mut self: core::pin::Pin<&mut Self>, width: i32, height: i32) {
        let w = width.clamp(1, 30_000) as u32;
        let h = height.clamp(1, 30_000) as u32;
        self.as_mut().rust_mut().doc.resize_canvas(w, h);
        self.sync();
    }

    // -- layers -------------------------------------------------------------

    fn layer_name(&self, index: i32) -> QString {
        self.layer_id_at(index)
            .and_then(|id| self.doc.layers().by_id(id))
            .map_or_else(QString::default, |l| QString::from(l.name.as_str()))
    }

    fn layer_visible(&self, index: i32) -> bool {
        self.layer_id_at(index)
            .and_then(|id| self.doc.layers().by_id(id))
            .is_some_and(|l| l.visible)
    }

    fn layer_opacity(&self, index: i32) -> i32 {
        self.layer_id_at(index)
            .and_then(|id| self.doc.layers().by_id(id))
            .map_or(100, |l| (l.opacity * 100.0).round() as i32)
    }

    fn layer_fill_opacity(&self, index: i32) -> i32 {
        self.layer_id_at(index)
            .and_then(|id| self.doc.layers().by_id(id))
            .map_or(100, |l| (l.fill_opacity * 100.0).round() as i32)
    }

    fn layer_blend_mode(&self, index: i32) -> i32 {
        self.layer_id_at(index)
            .and_then(|id| self.doc.layers().by_id(id))
            .map_or(0, |l| l.blend_mode as i32)
    }

    fn layer_is_clipping(&self, index: i32) -> bool {
        self.layer_id_at(index)
            .and_then(|id| self.doc.layers().by_id(id))
            .is_some_and(|l| l.clipping)
    }

    fn layer_has_mask(&self, index: i32) -> bool {
        self.layer_id_at(index)
            .and_then(|id| self.doc.layers().by_id(id))
            .is_some_and(|l| l.mask.is_some())
    }

    fn layer_thumbnail(&self, index: i32, size: i32) -> QImage {
        let size = size.clamp(1, 512);
        let Some(layer) = self
            .layer_id_at(index)
            .and_then(|id| self.doc.layers().by_id(id))
        else {
            return QImage::default();
        };

        let (sw, sh) = (layer.pixels.width(), layer.pixels.height());
        if sw == 0 || sh == 0 {
            return QImage::default();
        }

        // Fit the layer inside a square, preserving aspect ratio.
        let scale = (size as f32 / sw as f32).min(size as f32 / sh as f32);
        let tw = ((sw as f32 * scale).round() as u32).max(1);
        let th = ((sh as f32 * scale).round() as u32).max(1);

        let mut thumb = Pixmap::new(tw, th);
        for y in 0..th {
            for x in 0..tw {
                // Nearest-neighbour is fine at thumbnail sizes and avoids
                // allocating an intermediate mip chain on every panel repaint.
                let sx = (x as f32 / scale) as i32;
                let sy = (y as f32 / scale) as i32;
                thumb.set(x as i32, y as i32, layer.pixels.get(sx, sy));
            }
        }
        pixmap_to_qimage(thumb)
    }

    fn set_active_layer(mut self: core::pin::Pin<&mut Self>, index: i32) {
        if let Some(id) = self.layer_id_at(index) {
            self.as_mut().rust_mut().doc.set_active_layer(id);
            self.sync();
        }
    }

    fn set_layer_visible(mut self: core::pin::Pin<&mut Self>, index: i32, visible: bool) {
        if let Some(id) = self.layer_id_at(index) {
            self.as_mut().rust_mut().doc.set_layer_visible(id, visible);
            self.sync();
        }
    }

    fn set_layer_opacity(mut self: core::pin::Pin<&mut Self>, index: i32, percent: i32) {
        if let Some(id) = self.layer_id_at(index) {
            let v = percent.clamp(0, 100) as f32 / 100.0;
            self.as_mut().rust_mut().doc.set_layer_opacity(id, v);
            self.sync();
        }
    }

    fn set_layer_fill_opacity(mut self: core::pin::Pin<&mut Self>, index: i32, percent: i32) {
        if let Some(id) = self.layer_id_at(index) {
            let v = percent.clamp(0, 100) as f32 / 100.0;
            self.as_mut().rust_mut().doc.set_layer_fill_opacity(id, v);
            self.sync();
        }
    }

    fn set_layer_blend_mode(mut self: core::pin::Pin<&mut Self>, index: i32, mode: i32) {
        if let Some(id) = self.layer_id_at(index) {
            let mode = BlendMode::from_i32(mode);
            self.as_mut().rust_mut().doc.set_layer_blend_mode(id, mode);
            self.sync();
        }
    }

    fn set_layer_name(mut self: core::pin::Pin<&mut Self>, index: i32, name: &QString) {
        if let Some(id) = self.layer_id_at(index) {
            let name = name.to_string();
            self.as_mut().rust_mut().doc.set_layer_name(id, name);
            self.sync();
        }
    }

    fn set_layer_clipping(mut self: core::pin::Pin<&mut Self>, index: i32, clipping: bool) {
        if let Some(id) = self.layer_id_at(index) {
            self.as_mut().rust_mut().doc.set_layer_clipping(id, clipping);
            self.sync();
        }
    }

    fn add_layer(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().rust_mut().doc.add_layer(None);
        self.sync();
    }

    fn add_adjustment_layer(mut self: core::pin::Pin<&mut Self>, kind: &QString) {
        let name = kind.to_string();
        let adjustment = Adjustment::default_for(&name).unwrap_or_default();
        self.as_mut()
            .rust_mut()
            .doc
            .add_adjustment_layer(adjustment);
        self.sync();
    }

    fn duplicate_layer(mut self: core::pin::Pin<&mut Self>, index: i32) {
        if let Some(id) = self.layer_id_at(index) {
            self.as_mut().rust_mut().doc.duplicate_layer(id);
            self.sync();
        }
    }

    fn delete_layer(mut self: core::pin::Pin<&mut Self>, index: i32) {
        if let Some(id) = self.layer_id_at(index) {
            self.as_mut().rust_mut().doc.delete_layer(id);
            self.sync();
        }
    }

    fn move_layer(mut self: core::pin::Pin<&mut Self>, from: i32, to: i32) {
        let count = self.doc.layer_count();
        let Some(id) = self.layer_id_at(from) else {
            return;
        };
        if to < 0 || to as usize >= count {
            return;
        }
        // Flip the destination from panel order to stack order.
        let stack_to = count - 1 - to as usize;
        self.as_mut().rust_mut().doc.reorder_layer(id, stack_to);
        self.sync();
    }

    fn merge_layer_down(mut self: core::pin::Pin<&mut Self>, index: i32) {
        if let Some(id) = self.layer_id_at(index) {
            self.as_mut().rust_mut().doc.merge_down(id);
            self.sync();
        }
    }

    fn flatten_image(mut self: core::pin::Pin<&mut Self>) {
        let bg = self.background;
        self.as_mut().rust_mut().doc.flatten(bg);
        self.sync();
    }

    fn add_layer_mask(mut self: core::pin::Pin<&mut Self>, index: i32, reveal_all: bool) {
        if let Some(id) = self.layer_id_at(index) {
            self.as_mut()
                .rust_mut()
                .doc
                .add_layer_mask(id, reveal_all);
            self.sync();
        }
    }

    fn offset_layer(mut self: core::pin::Pin<&mut Self>, index: i32, dx: i32, dy: i32) {
        if let Some(id) = self.layer_id_at(index) {
            self.as_mut().rust_mut().doc.offset_layer(id, dx, dy);
            self.sync();
        }
    }

    // -- painting -----------------------------------------------------------

    fn set_brush(
        mut self: core::pin::Pin<&mut Self>,
        size: f32,
        hardness: i32,
        opacity: i32,
        flow: i32,
        spacing: i32,
    ) {
        let brush = Brush {
            size: size.clamp(1.0, 5000.0),
            hardness: hardness.clamp(0, 100) as f32 / 100.0,
            opacity: opacity.clamp(0, 100) as f32 / 100.0,
            flow: flow.clamp(0, 100) as f32 / 100.0,
            spacing: spacing.clamp(1, 1000) as f32 / 100.0,
        };
        self.as_mut().rust_mut().brush = brush;
    }

    fn set_foreground_color(mut self: core::pin::Pin<&mut Self>, color: &QColor) {
        let c = qcolor_to_rgba(color);
        self.as_mut().rust_mut().foreground = c;
    }

    fn foreground_color(&self) -> QColor {
        rgba_to_qcolor(self.foreground)
    }

    fn set_background_color(mut self: core::pin::Pin<&mut Self>, color: &QColor) {
        let c = qcolor_to_rgba(color);
        self.as_mut().rust_mut().background = c;
    }

    fn background_color(&self) -> QColor {
        rgba_to_qcolor(self.background)
    }

    fn swap_colors(mut self: core::pin::Pin<&mut Self>) {
        let fg = self.foreground;
        let bg = self.background;
        self.as_mut().rust_mut().foreground = bg;
        self.as_mut().rust_mut().background = fg;
    }

    fn reset_colors(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().rust_mut().foreground = Rgba8::BLACK;
        self.as_mut().rust_mut().background = Rgba8::WHITE;
    }

    fn begin_stroke(mut self: core::pin::Pin<&mut Self>, x: f32, y: f32, pressure: f32) -> bool {
        let brush = self.brush;
        self.as_mut()
            .rust_mut()
            .doc
            .begin_stroke(&brush, x, y, pressure)
    }

    fn extend_stroke(mut self: core::pin::Pin<&mut Self>, x: f32, y: f32, pressure: f32) {
        let brush = self.brush;
        self.as_mut()
            .rust_mut()
            .doc
            .extend_stroke(&brush, x, y, pressure);
    }

    fn end_stroke(mut self: core::pin::Pin<&mut Self>) {
        let color = self.paint_color();
        let opacity = self.brush.opacity;
        self.as_mut().rust_mut().doc.end_stroke(color, opacity);
        self.sync();
    }

    fn cancel_stroke(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().rust_mut().doc.cancel_stroke();
        self.sync();
    }

    fn is_stroking(&self) -> bool {
        !self.doc.stroke_dirty().is_empty()
    }

    fn set_erase_mode(mut self: core::pin::Pin<&mut Self>, erasing: bool) {
        self.as_mut().rust_mut().erasing = erasing;
    }

    fn fill_foreground(mut self: core::pin::Pin<&mut Self>) {
        let c = self.foreground;
        self.as_mut().rust_mut().doc.fill(c);
        self.sync();
    }

    fn fill_background(mut self: core::pin::Pin<&mut Self>) {
        let c = self.background;
        self.as_mut().rust_mut().doc.fill(c);
        self.sync();
    }

    fn clear_selection(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().rust_mut().doc.clear_selection_pixels();
        self.sync();
    }

    fn pick_color(&self, x: i32, y: i32) -> QColor {
        let px = self.doc.composite().get(x, y);
        rgba_to_qcolor(px)
    }

    // -- selection ----------------------------------------------------------

    fn select_rect(
        mut self: core::pin::Pin<&mut Self>,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        op: i32,
    ) {
        let rect = Rect::new(x, y, width.max(0) as u32, height.max(0) as u32);
        let op = SelectionOp::from_i32(op);
        self.as_mut().rust_mut().doc.select_rect(rect, op);
        self.as_mut().selection_changed();
        self.as_mut().canvas_changed();
    }

    fn select_ellipse(
        mut self: core::pin::Pin<&mut Self>,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        op: i32,
    ) {
        let rect = Rect::new(x, y, width.max(0) as u32, height.max(0) as u32);
        let op = SelectionOp::from_i32(op);
        self.as_mut().rust_mut().doc.select_ellipse(rect, op);
        self.as_mut().selection_changed();
        self.as_mut().canvas_changed();
    }

    fn select_all(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().rust_mut().doc.select_all();
        self.as_mut().selection_changed();
        self.as_mut().canvas_changed();
    }

    fn deselect(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().rust_mut().doc.deselect();
        self.as_mut().selection_changed();
        self.as_mut().canvas_changed();
    }

    fn invert_selection(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().rust_mut().doc.invert_selection();
        self.as_mut().selection_changed();
        self.as_mut().canvas_changed();
    }

    fn feather_selection(mut self: core::pin::Pin<&mut Self>, radius: i32) {
        let r = radius.clamp(0, 1000) as u32;
        self.as_mut().rust_mut().doc.selection_mut().feather(r);
        self.as_mut().selection_changed();
        self.as_mut().canvas_changed();
    }

    fn has_selection(&self) -> bool {
        self.doc.has_selection()
    }

    fn selection_bounds(mut self: core::pin::Pin<&mut Self>) -> Vec<i32> {
        let b = self.as_mut().rust_mut().doc.selection_mut().bounds();
        vec![b.x, b.y, b.width as i32, b.height as i32]
    }

    fn selection_mask(&self) -> QImage {
        pixmap_to_qimage(self.doc.selection().to_pixmap())
    }

    // -- filters ------------------------------------------------------------

    fn apply_filter(mut self: core::pin::Pin<&mut Self>, name: &QString, p1: f32, p2: f32) {
        let filter = match name.to_string().as_str() {
            "Gaussian Blur" => Filter::GaussianBlur { radius: p1.max(0.0) },
            "Box Blur" => Filter::BoxBlur {
                radius: p1.max(0.0) as u32,
            },
            "Sharpen" => Filter::Sharpen,
            "Unsharp Mask" => Filter::UnsharpMask {
                amount: p1,
                radius: p2,
                threshold: 0,
            },
            "Add Noise" => Filter::Noise {
                amount: p1.clamp(0.0, 1.0),
                monochromatic: p2 != 0.0,
            },
            // Unknown names are ignored rather than guessed at.
            _ => return,
        };
        self.as_mut().rust_mut().doc.apply_filter(filter);
        self.sync();
    }

    fn apply_adjustment(
        mut self: core::pin::Pin<&mut Self>,
        name: &QString,
        p1: f32,
        p2: f32,
        p3: f32,
    ) {
        let name = name.to_string();
        let adjustment = match name.as_str() {
            "Brightness/Contrast" => Adjustment::BrightnessContrast {
                brightness: p1,
                contrast: p2,
            },
            "Hue/Saturation" => Adjustment::HueSaturation {
                hue: p1,
                saturation: p2,
                lightness: p3,
            },
            "Levels" => Adjustment::Levels {
                in_black: p1,
                in_white: p2,
                gamma: p3,
                out_black: 0.0,
                out_white: 1.0,
            },
            "Posterize" => Adjustment::Posterize {
                levels: p1.max(2.0) as u32,
            },
            "Threshold" => Adjustment::Threshold {
                level: p1.clamp(0.0, 255.0) as u8,
            },
            "Exposure" => Adjustment::Exposure {
                exposure: p1,
                offset: p2,
                gamma: p3.max(0.01),
            },
            // Parameterless adjustments fall through to their defaults.
            other => match Adjustment::default_for(other) {
                Some(a) => a,
                None => return,
            },
        };
        self.as_mut().rust_mut().doc.apply_adjustment(adjustment);
        self.sync();
    }

    // -- history ------------------------------------------------------------

    fn undo(mut self: core::pin::Pin<&mut Self>) -> bool {
        let ok = self.as_mut().rust_mut().doc.undo();
        if ok {
            self.sync();
        }
        ok
    }

    fn redo(mut self: core::pin::Pin<&mut Self>) -> bool {
        let ok = self.as_mut().rust_mut().doc.redo();
        if ok {
            self.sync();
        }
        ok
    }

    fn can_undo(&self) -> bool {
        self.doc.can_undo()
    }

    fn can_redo(&self) -> bool {
        self.doc.can_redo()
    }

    fn undo_name(&self) -> QString {
        QString::from(self.doc.history().undo_name().unwrap_or(""))
    }

    fn redo_name(&self) -> QString {
        QString::from(self.doc.history().redo_name().unwrap_or(""))
    }

    fn history_count(&self) -> i32 {
        self.doc.history().len() as i32
    }

    fn history_name(&self, index: i32) -> QString {
        if index < 0 {
            return QString::default();
        }
        self.doc
            .history()
            .state_names()
            .get(index as usize)
            .map_or_else(QString::default, |s| QString::from(*s))
    }

    fn history_cursor(&self) -> i32 {
        self.doc.history().cursor() as i32
    }

    fn jump_to_history(mut self: core::pin::Pin<&mut Self>, index: i32) {
        if index < 0 {
            return;
        }
        self.as_mut().rust_mut().doc.jump_to_history(index as usize);
        self.sync();
    }

    // -- static metadata ----------------------------------------------------

    fn blend_mode_names(&self) -> QString {
        // Newline-separated so C++ can split without a list type crossing the
        // bridge — the surface stays small and stable.
        let joined = BlendMode::ALL
            .iter()
            .map(|m| m.name())
            .collect::<Vec<_>>()
            .join("\n");
        QString::from(joined.as_str())
    }

    fn blend_mode_separators(&self) -> QString {
        let joined = BlendMode::GROUP_BREAKS
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",");
        QString::from(joined.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The Q_PROPERTY setters and signals need a live QObject, so these tests
    // exercise the pure logic on `EngineRust` directly.

    fn engine_with(layer_names: &[&str]) -> EngineRust {
        let mut e = EngineRust {
            doc: Document::new_transparent(8, 8),
            ..Default::default()
        };
        // The document starts with one layer; rename it and add the rest.
        if let Some(first) = layer_names.first() {
            let id = e.doc.layers().get(0).unwrap().id;
            e.doc.set_layer_name(id, *first);
        }
        for name in layer_names.iter().skip(1) {
            e.doc.add_layer(Some((*name).to_string()));
        }
        e
    }

    #[test]
    fn panel_index_zero_is_the_topmost_layer() {
        // Stack order is bottom-first: bottom, middle, top.
        let e = engine_with(&["bottom", "middle", "top"]);
        assert_eq!(e.doc.layer_count(), 3);

        let top_id = e.layer_id_at(0).unwrap();
        assert_eq!(e.doc.layers().by_id(top_id).unwrap().name, "top");

        let bottom_id = e.layer_id_at(2).unwrap();
        assert_eq!(e.doc.layers().by_id(bottom_id).unwrap().name, "bottom");
    }

    #[test]
    fn panel_index_round_trips() {
        let e = engine_with(&["a", "b", "c"]);
        for i in 0..3 {
            let id = e.layer_id_at(i).unwrap();
            assert_eq!(e.panel_index_of(id), i, "index {} did not round trip", i);
        }
    }

    #[test]
    fn out_of_range_panel_indices_are_rejected() {
        let e = engine_with(&["only"]);
        assert!(e.layer_id_at(-1).is_none());
        assert!(e.layer_id_at(1).is_none());
        assert!(e.layer_id_at(999).is_none());
    }

    #[test]
    fn panel_index_of_unknown_layer_is_negative() {
        let e = engine_with(&["a"]);
        assert_eq!(e.panel_index_of(LayerId(9999)), -1);
    }

    #[test]
    fn erase_mode_switches_the_paint_color() {
        let mut e = EngineRust::default();
        e.foreground = Rgba8::new(1, 2, 3, 255);
        e.background = Rgba8::new(9, 8, 7, 255);

        assert_eq!(e.paint_color(), e.foreground);
        e.erasing = true;
        assert_eq!(e.paint_color(), e.background);
    }

    #[test]
    fn default_engine_matches_its_document() {
        let e = EngineRust::default();
        assert_eq!(e.canvas_width, e.doc.width() as i32);
        assert_eq!(e.canvas_height, e.doc.height() as i32);
        assert_eq!(e.layer_count, e.doc.layer_count() as i32);
        assert!(!e.modified);
    }

    #[test]
    fn blend_mode_names_cover_every_mode() {
        // The C++ combo box splits on newlines, so the count must match
        // exactly or the indices sent back would be wrong.
        let joined = BlendMode::ALL
            .iter()
            .map(|m| m.name())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(joined.split('\n').count(), BlendMode::ALL.len());
    }

    #[test]
    fn blend_mode_separators_are_in_range() {
        for i in BlendMode::GROUP_BREAKS {
            assert!(i < BlendMode::ALL.len(), "separator {} out of range", i);
        }
    }
}

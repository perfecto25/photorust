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

use crate::annotation::{MarkerKind, Ruler};
use crate::blend::BlendMode;
use crate::brush::{Brush, StrokeMask};
use crate::buffer::{Pixmap, Rect, Rgba8};
use crate::document::{Document, ImageMode, PasteMode, PatchOptions};
use crate::filters::{Adjustment, Filter};
use crate::healing::{HealMode, MoveOptions};
use crate::layer::{LayerId, LayerKind, TextAlign, TextContent, TextRun};
use crate::mixer::MixerOptions;
use crate::replace::{ReplaceLimits, ReplaceMode, ReplaceOptions, ReplaceSampling};
use crate::erase::BackgroundEraseOptions;
use crate::sample::{Limits, Sampling};
use crate::focus::{FocusMode, FocusOptions};
use crate::smudge::SmudgeOptions;
use crate::tone::{SpongeMode, ToneOptions, ToneRange, ToneTool};
use crate::bucket::BucketOptions;
use crate::gradient::{self, GradientOptions, GradientType};
use crate::metadata;
use crate::pattern;
use crate::shape::{self, ShapeKind, ShapeMode, ShapeOptions};
use crate::stamp::CloneSampling;
use crate::magnetic::EdgeMap;
use crate::selection::{Selection, SelectionOp};
use crate::wand::{self, QuickSelector};
// `rust_mut()` on a generated QObject comes from this trait.
use cxx_qt::CxxQtType;
use cxx_qt_lib::{QColor, QImage, QImageFormat, QPointF, QPolygonF, QRect, QString};

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qimage.h");
        type QImage = cxx_qt_lib::QImage;

        include!("cxx-qt-lib/qcolor.h");
        type QColor = cxx_qt_lib::QColor;

        include!("cxx-qt-lib/qpointf.h");
        type QPointF = cxx_qt_lib::QPointF;

        include!("cxx-qt-lib/qpolygonf.h");
        /// Carries a shape's outline out — the points the canvas previews and
        /// the engine commits, in one Qt container rather than a builder.
        type QPolygonF = cxx_qt_lib::QPolygonF;

        include!("cxx-qt-lib/qrect.h");
        type QRect = cxx_qt_lib::QRect;

        include!("cxx-qt-lib/qvector.h");
        /// Carries lasso vertices in. Qt's own container, so the shell builds
        /// it directly and nothing is copied into a Rust `Vec` on the way.
        type QVector_f32 = cxx_qt_lib::QVector<f32>;
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
        /// How many documents are open. Each gets a tab in the shell.
        #[qinvokable]
        #[cxx_name = "documentCount"]
        fn document_count(self: &Engine) -> i32;

        /// Which document is active, as an index into the tab order.
        #[qinvokable]
        #[cxx_name = "activeDocument"]
        fn active_document(self: &Engine) -> i32;

        /// Title of the document at a tab index, with its modified marker.
        #[qinvokable]
        #[cxx_name = "documentTitleAt"]
        fn document_title_at(self: &Engine, index: i32) -> QString;

        /// Whether the document at a tab index has unsaved changes.
        #[qinvokable]
        #[cxx_name = "documentModifiedAt"]
        fn document_modified_at(self: &Engine, index: i32) -> bool;

        /// Switch to a document. Everything downstream — canvas, panels,
        /// history — follows the usual change signals.
        #[qinvokable]
        #[cxx_name = "setActiveDocument"]
        fn set_active_document(self: Pin<&mut Engine>, index: i32);

        /// Close a document. Refuses to close the last one, since there is no
        /// such thing here as an open application with no document.
        #[qinvokable]
        #[cxx_name = "closeDocument"]
        fn close_document(self: Pin<&mut Engine>, index: i32) -> bool;

        /// The open documents changed: one was added, closed, or switched to.
        #[qsignal]
        #[cxx_name = "documentsChanged"]
        fn documents_changed(self: Pin<&mut Engine>);

        /// Open a new document in its own tab.
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

        /// Where the active document was opened from or last saved to, or an
        /// empty string for one that has never been saved.
        #[qinvokable]
        #[cxx_name = "documentPath"]
        fn document_path(self: &Engine) -> QString;

        /// What a file on disk says about itself, for File ▸ File Info.
        ///
        /// One record per line, each `category<TAB>label<TAB>value` — the same
        /// newline-separated shape the preset lists use, since the dialog only
        /// ever lays these out as rows under a heading. An unreadable file
        /// gives an empty string.
        #[qinvokable]
        #[cxx_name = "fileMetadata"]
        fn file_metadata(self: &Engine, path: &QString) -> QString;

        /// The XMP packet embedded in a file, verbatim, for the Raw Data pane.
        /// Empty when the file carries none.
        #[qinvokable]
        #[cxx_name = "fileXmp"]
        fn file_xmp(self: &Engine, path: &QString) -> QString;

        /// The composited document as a premultiplied ARGB image.
        #[qinvokable]
        #[cxx_name = "compositeImage"]
        fn composite_image(self: &Engine) -> QImage;

        /// The composite with the in-progress brush stroke drawn on top.
        /// Falls back to [`Engine::composite_image`] when no stroke is active.
        #[qinvokable]
        #[cxx_name = "previewImage"]
        fn preview_image(self: &Engine) -> QImage;

        /// Memory footprint as `[flattened, withLayers]` in bytes — the two
        /// numbers behind the Info panel's "Doc:" line.
        #[qinvokable]
        #[cxx_name = "documentSizeBytes"]
        fn document_size_bytes(self: &Engine) -> Vec<f64>;

        /// Current color mode index (0=Bitmap .. 7=Multichannel).
        #[qinvokable]
        #[cxx_name = "colorMode"]
        fn color_mode(self: &Engine) -> i32;

        /// Set the color mode by index.
        #[qinvokable]
        #[cxx_name = "setColorMode"]
        fn set_color_mode(self: Pin<&mut Engine>, mode: i32);

        /// Convert to indexed color with quantization and optional dithering.
        #[qinvokable]
        #[cxx_name = "convertToIndexed"]
        fn convert_to_indexed(self: Pin<&mut Engine>, max_colors: i32, dither_amount: i32);

        /// Current bit depth (8, 16, or 32).
        #[qinvokable]
        #[cxx_name = "bitDepth"]
        fn bit_depth(self: &Engine) -> i32;

        /// Set the bit depth.
        #[qinvokable]
        #[cxx_name = "setBitDepth"]
        fn set_bit_depth(self: Pin<&mut Engine>, depth: i32);

        /// Resize the canvas without scaling the content.
        #[qinvokable]
        #[cxx_name = "resizeCanvas"]
        fn resize_canvas(self: Pin<&mut Engine>, width: i32, height: i32);

        /// Straighten a quadrilateral into a rectangle and crop to it — the
        /// Perspective Crop tool.
        ///
        /// `corners` is eight floats: x,y for top-left, top-right,
        /// bottom-right, bottom-left, in document coordinates. Returns false
        /// for a degenerate quad, having changed nothing.
        #[qinvokable]
        #[cxx_name = "perspectiveCrop"]
        fn perspective_crop(self: Pin<&mut Engine>, corners: &QVector_f32) -> bool;

        /// Crop to a rectangle in document coordinates.
        ///
        /// `deleteCropped` mirrors CS6's checkbox: when false the pixels
        /// outside the new canvas are kept, hanging off the edge, and reappear
        /// if the canvas is enlarged again.
        #[qinvokable]
        #[cxx_name = "cropTo"]
        fn crop_to(
            self: Pin<&mut Engine>,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            delete_cropped: bool,
        );
    }

    // -- annotations -----------------------------------------------------------
    //
    // Colour samplers, notes and count markers are all points, so they share
    // one set of calls keyed by `kind` (0 = colour sampler, 1 = note,
    // 2 = count) rather than three near-identical families. The ruler is a
    // line and gets its own.
    unsafe extern "RustQt" {
        #[qinvokable]
        #[cxx_name = "markerCount"]
        fn marker_count(self: &Engine, kind: i32) -> i32;

        /// One marker as `[x, y]`, or empty for an out-of-range index.
        #[qinvokable]
        #[cxx_name = "markerAt"]
        fn marker_at(self: &Engine, kind: i32, index: i32) -> Vec<i32>;

        /// Place a marker, returning its index or -1 if the kind is full.
        /// Only colour samplers have a limit — four, as in Photoshop.
        #[qinvokable]
        #[cxx_name = "addMarker"]
        fn add_marker(self: Pin<&mut Engine>, kind: i32, x: i32, y: i32) -> i32;

        #[qinvokable]
        #[cxx_name = "moveMarker"]
        fn move_marker(self: Pin<&mut Engine>, kind: i32, index: i32, x: i32, y: i32) -> bool;

        #[qinvokable]
        #[cxx_name = "removeMarker"]
        fn remove_marker(self: Pin<&mut Engine>, kind: i32, index: i32) -> bool;

        #[qinvokable]
        #[cxx_name = "clearMarkers"]
        fn clear_markers(self: Pin<&mut Engine>, kind: i32);

        /// The marker of `kind` within `radius` document pixels, nearest
        /// first, or -1. The shell scales `radius` by zoom so the grab area
        /// stays a constant size on screen.
        #[qinvokable]
        #[cxx_name = "markerNear"]
        fn marker_near(self: &Engine, kind: i32, x: i32, y: i32, radius: f32) -> i32;

        /// A note's text. Empty for the other marker kinds.
        #[qinvokable]
        #[cxx_name = "markerText"]
        fn marker_text(self: &Engine, kind: i32, index: i32) -> QString;

        #[qinvokable]
        #[cxx_name = "setMarkerText"]
        fn set_marker_text(self: Pin<&mut Engine>, kind: i32, index: i32, text: &QString) -> bool;

        #[qinvokable]
        #[cxx_name = "hasRuler"]
        fn has_ruler(self: &Engine) -> bool;

        #[qinvokable]
        #[cxx_name = "setRuler"]
        fn set_ruler(self: Pin<&mut Engine>, ax: f32, ay: f32, bx: f32, by: f32);

        #[qinvokable]
        #[cxx_name = "clearRuler"]
        fn clear_ruler(self: Pin<&mut Engine>);

        /// The ruler's endpoints as `[ax, ay, bx, by]`, or empty.
        #[qinvokable]
        #[cxx_name = "rulerLine"]
        fn ruler_line(self: &Engine) -> Vec<f32>;

        /// Photoshop's readout: `[X, Y, W, H, A, D1]` — origin, deltas, angle
        /// in degrees and length. Empty when there is no ruler.
        #[qinvokable]
        #[cxx_name = "rulerMeasurement"]
        fn ruler_measurement(self: &Engine) -> Vec<f32>;

        /// An annotation was added, moved, edited or removed.
        #[qsignal]
        #[cxx_name = "annotationsChanged"]
        fn annotations_changed(self: Pin<&mut Engine>);
    }

    // -- slices --------------------------------------------------------------
    //
    // The list the shell sees is the *resolved* one: the user's own slices plus
    // the auto slices filling the rest of the canvas. It is recomputed on every
    // query because moving one user slice changes every auto slice around it.
    unsafe extern "RustQt" {
        /// Number of slices, user and auto together.
        #[qinvokable]
        #[cxx_name = "sliceCount"]
        fn slice_count(self: &Engine) -> i32;

        /// One slice as `[x, y, width, height, number, userIndex]`, where
        /// `userIndex` is -1 for an auto slice. Empty for an out-of-range
        /// index. Packed into one call because the shell reads every field of
        /// every slice on each repaint.
        #[qinvokable]
        #[cxx_name = "sliceAt"]
        fn slice_at(self: &Engine, index: i32) -> Vec<i32>;

        /// Add a user slice. Returns its index, or -1 if the rect was empty.
        #[qinvokable]
        #[cxx_name = "addSlice"]
        fn add_slice(self: Pin<&mut Engine>, x: i32, y: i32, width: i32, height: i32) -> i32;

        /// Move or resize an existing user slice.
        #[qinvokable]
        #[cxx_name = "setUserSlice"]
        fn set_user_slice(
            self: Pin<&mut Engine>,
            index: i32,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
        ) -> bool;

        #[qinvokable]
        #[cxx_name = "removeUserSlice"]
        fn remove_user_slice(self: Pin<&mut Engine>, index: i32) -> bool;

        #[qinvokable]
        #[cxx_name = "clearSlices"]
        fn clear_slices(self: Pin<&mut Engine>);

        /// The composite cropped to one slice, ready for the shell to write
        /// out. The shell owns file writing for everything but `.psd`.
        #[qinvokable]
        #[cxx_name = "sliceImage"]
        fn slice_image(self: &Engine, index: i32) -> QImage;

        /// The slices changed and the canvas should repaint its overlay.
        #[qsignal]
        #[cxx_name = "slicesChanged"]
        fn slices_changed(self: Pin<&mut Engine>);
    }

    // -- paths -----------------------------------------------------------
    //
    // The Pen tool and the Paths panel both act on whichever path is
    // *active*; there is no per-call path handle, only an index into this
    // list, mirroring how the Layers panel addresses layers by panel index.
    // Geometry edits are not undoable (see `path.rs`'s module comment) — only
    // Fill Path, Stroke Path and the pixels a Make Selection goes on to affect
    // are, and those already go through the ordinary commit machinery.
    unsafe extern "RustQt" {
        /// Number of paths in the panel.
        #[qinvokable]
        #[cxx_name = "pathCount"]
        fn path_count(self: &Engine) -> i32;

        /// A path's display name, or empty for an out-of-range index.
        #[qinvokable]
        #[cxx_name = "pathName"]
        fn path_name(self: &Engine, index: i32) -> QString;

        /// The active path's panel index, or -1 if none is active.
        #[qinvokable]
        #[cxx_name = "activePathIndex"]
        fn active_path_index(self: &Engine) -> i32;

        #[qinvokable]
        #[cxx_name = "setActivePathIndex"]
        fn set_active_path_index(self: Pin<&mut Engine>, index: i32) -> bool;

        /// Create an empty path named "Path N" and make it active — the
        /// panel's "New Path". Returns its index.
        #[qinvokable]
        #[cxx_name = "addPath"]
        fn add_path(self: Pin<&mut Engine>) -> i32;

        #[qinvokable]
        #[cxx_name = "duplicatePath"]
        fn duplicate_path(self: Pin<&mut Engine>, index: i32) -> i32;

        #[qinvokable]
        #[cxx_name = "deletePath"]
        fn delete_path(self: Pin<&mut Engine>, index: i32) -> bool;

        #[qinvokable]
        #[cxx_name = "renamePath"]
        fn rename_path(self: Pin<&mut Engine>, index: i32, name: &QString) -> bool;

        /// Whether a subpath is currently being extended by the Pen tool —
        /// what decides whether Enter/Escape/a tool switch has anything to
        /// finish.
        #[qinvokable]
        #[cxx_name = "pathIsEditing"]
        fn path_is_editing(self: &Engine) -> bool;

        /// Append a corner anchor to the active path, starting the Work Path
        /// if none is active yet and starting a new subpath within it if the
        /// last one was finished or closed.
        #[qinvokable]
        #[cxx_name = "pathAppendCorner"]
        fn path_append_corner(self: Pin<&mut Engine>, x: f32, y: f32);

        /// Live-update the handle of the point last appended, as a Pen tool
        /// drag moves away from where it clicked. `independent` is CS6's
        /// Alt-drag-while-placing: it curves only the segment about to be
        /// drawn, leaving the one already drawn into this point untouched.
        #[qinvokable]
        #[cxx_name = "pathUpdateLastHandle"]
        fn path_update_last_handle(self: Pin<&mut Engine>, x: f32, y: f32, independent: bool) -> bool;

        /// Close the subpath being drawn back to its first anchor. Refused
        /// with fewer than two points.
        #[qinvokable]
        #[cxx_name = "pathCloseActiveSubpath"]
        fn path_close_active_subpath(self: Pin<&mut Engine>) -> bool;

        /// Stop extending the current subpath without closing it — Enter,
        /// double-click, Escape, or switching away from the Pen tool.
        #[qinvokable]
        #[cxx_name = "pathFinishEditing"]
        fn path_finish_editing(self: Pin<&mut Engine>);

        /// The nearest anchor on the active path within `radius` document
        /// units, as `[subpath, point]`, or empty if none is that close.
        #[qinvokable]
        #[cxx_name = "pathHitAnchor"]
        fn path_hit_anchor(self: &Engine, x: f32, y: f32, radius: f32) -> Vec<i32>;

        /// The nearest handle within `radius`, as `[subpath, point, side]`
        /// (`side` 0 = in, 1 = out), or empty.
        #[qinvokable]
        #[cxx_name = "pathHitHandle"]
        fn path_hit_handle(self: &Engine, x: f32, y: f32, radius: f32) -> Vec<i32>;

        /// The nearest point on any segment within `radius`, as `[subpath,
        /// segment, t]` — `t` packed as a float alongside the two integers, or
        /// empty if nothing is that close. What Auto Add and the Add Anchor
        /// Point tool hit-test against.
        #[qinvokable]
        #[cxx_name = "pathHitSegment"]
        fn path_hit_segment(self: &Engine, x: f32, y: f32, radius: f32) -> Vec<f32>;

        /// The subpath nearest `(x, y)` — on one of its segments, or anywhere
        /// inside it if closed — or -1. What the Path Selection tool picks.
        #[qinvokable]
        #[cxx_name = "pathHitSubpath"]
        fn path_hit_subpath(self: &Engine, x: f32, y: f32, radius: f32) -> i32;

        /// Move an anchor to an absolute position, carrying its handles with
        /// it — Direct Selection dragging a point.
        #[qinvokable]
        #[cxx_name = "pathMoveAnchor"]
        fn path_move_anchor(self: Pin<&mut Engine>, sp: i32, pt: i32, x: f32, y: f32) -> bool;

        /// Drag one handle to an absolute position. `side` 0 = in, 1 = out.
        /// `independent` (Alt) moves just that handle and permanently breaks
        /// the point's smoothness; otherwise a smooth point's opposite handle
        /// follows the angle, keeping its own length.
        #[qinvokable]
        #[cxx_name = "pathMoveHandle"]
        fn path_move_handle(
            self: Pin<&mut Engine>,
            sp: i32,
            pt: i32,
            side: i32,
            x: f32,
            y: f32,
            independent: bool,
        ) -> bool;

        /// Convert Point's click: strip both handles, leaving a plain corner.
        #[qinvokable]
        #[cxx_name = "pathSetCorner"]
        fn path_set_corner(self: Pin<&mut Engine>, sp: i32, pt: i32) -> bool;

        /// Convert Point's drag from a corner: pull out a fresh symmetric pair
        /// of handles, turning the point smooth.
        #[qinvokable]
        #[cxx_name = "pathDragNewHandles"]
        fn path_drag_new_handles(self: Pin<&mut Engine>, sp: i32, pt: i32, x: f32, y: f32) -> bool;

        /// Split a segment at `t`, inserting a new anchor exactly on the
        /// curve — the Add Anchor Point tool, and Auto Add hovering a segment.
        #[qinvokable]
        #[cxx_name = "pathInsertAnchor"]
        fn path_insert_anchor(self: Pin<&mut Engine>, sp: i32, seg: i32, t: f32) -> bool;

        /// Remove an anchor, deleting its subpath too if that empties it — the
        /// Delete Anchor Point tool, and Auto Add hovering an anchor.
        #[qinvokable]
        #[cxx_name = "pathDeleteAnchor"]
        fn path_delete_anchor(self: Pin<&mut Engine>, sp: i32, pt: i32) -> bool;

        /// Move a whole subpath by a delta — the Path Selection tool.
        #[qinvokable]
        #[cxx_name = "pathMoveSubpath"]
        fn path_move_subpath(self: Pin<&mut Engine>, sp: i32, dx: f32, dy: f32) -> bool;

        /// Number of subpaths in the active path.
        #[qinvokable]
        #[cxx_name = "pathSubpathCount"]
        fn path_subpath_count(self: &Engine) -> i32;

        #[qinvokable]
        #[cxx_name = "pathIsClosed"]
        fn path_is_closed(self: &Engine, sp: i32) -> bool;

        #[qinvokable]
        #[cxx_name = "pathAnchorCount"]
        fn path_anchor_count(self: &Engine, sp: i32) -> i32;

        /// One anchor's full geometry, packed as `[x, y, smooth, hasIn, inX,
        /// inY, hasOut, outX, outY]` (`smooth`/`hasIn`/`hasOut` are 0 or 1) —
        /// everything the canvas needs to draw it and build a `QPainterPath`
        /// segment either side of it. Empty for an out-of-range index.
        #[qinvokable]
        #[cxx_name = "pathAnchorAt"]
        fn path_anchor_at(self: &Engine, sp: i32, pt: i32) -> Vec<f32>;

        /// Reduce a freehand drag to a handful of corner anchors and append it
        /// as a new subpath — the Freeform Pen tool. `points` is the raw mouse
        /// trail in document coordinates, interleaved x, y.
        #[qinvokable]
        #[cxx_name = "pathAddFreeformSubpath"]
        fn path_add_freeform_subpath(
            self: Pin<&mut Engine>,
            points: &QVector_f32,
            tolerance: f32,
            close: bool,
        ) -> bool;

        /// Turn the active path into a selection — the Paths panel's "Make
        /// Selection". `op` is a `SelectionOp` discriminant.
        #[qinvokable]
        #[cxx_name = "pathMakeSelection"]
        fn path_make_selection(self: Pin<&mut Engine>, op: i32, feather: i32) -> bool;

        /// Fill the active path with the foreground colour — "Fill Path".
        #[qinvokable]
        #[cxx_name = "pathFill"]
        fn path_fill(self: Pin<&mut Engine>) -> bool;

        /// Stroke the active path with the current brush and foreground
        /// colour — "Stroke Path". Photoshop lets Stroke Path use any tool's
        /// settings; this always uses the Brush's, the overwhelmingly common
        /// choice.
        #[qinvokable]
        #[cxx_name = "pathStroke"]
        fn path_stroke(self: Pin<&mut Engine>) -> bool;

        /// The paths changed and the canvas/panel should repaint.
        #[qsignal]
        #[cxx_name = "pathsChanged"]
        fn paths_changed(self: Pin<&mut Engine>);
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

        /// What the layer is: 0 = raster (pixels), 1 = adjustment, 2 = type.
        /// The panel draws these differently and filters on them, as CS6's Kind
        /// row does.
        #[qinvokable]
        #[cxx_name = "layerKind"]
        fn layer_kind(self: &Engine, index: i32) -> i32;

        /// The layer's Lock Transparent Pixels flag.
        #[qinvokable]
        #[cxx_name = "layerLockTransparency"]
        fn layer_lock_transparency(self: &Engine, index: i32) -> bool;

        /// Its Lock Image Pixels flag — the one that makes a layer untouchable.
        #[qinvokable]
        #[cxx_name = "layerLockPixels"]
        fn layer_lock_pixels(self: &Engine, index: i32) -> bool;

        /// Its Lock Position flag.
        #[qinvokable]
        #[cxx_name = "layerLockPosition"]
        fn layer_lock_position(self: &Engine, index: i32) -> bool;

        /// Whether any lock is on, which is what puts a padlock on the row.
        #[qinvokable]
        #[cxx_name = "layerIsLocked"]
        fn layer_is_locked(self: &Engine, index: i32) -> bool;

        /// Whether every lock is on — Lock All. Such a layer cannot be deleted
        /// or merged either.
        #[qinvokable]
        #[cxx_name = "layerIsFullyLocked"]
        fn layer_is_fully_locked(self: &Engine, index: i32) -> bool;

        /// Whether the layer the tools would act on has its pixels locked.
        ///
        /// This is exactly the condition every painting entry point refuses on,
        /// so the shell can tell a refusal caused by the lock from one caused by
        /// the layer having no pixels to paint (an adjustment layer).
        #[qinvokable]
        #[cxx_name = "activeLayerIsLocked"]
        fn active_layer_is_locked(self: &Engine) -> bool;

        /// A square thumbnail of the layer's content, for the panel.
        #[qinvokable]
        #[cxx_name = "layerThumbnail"]
        fn layer_thumbnail(self: &Engine, index: i32, size: i32) -> QImage;

        /// The full-size image of a layer's pixels, for Free Transform.
        #[qinvokable]
        #[cxx_name = "layerImage"]
        fn layer_image(self: &Engine, index: i32) -> QImage;

        /// Tight bounding box of non-transparent pixels in document space:
        /// (x, y, width, height). Returns (0,0,0,0) when the layer is empty.
        #[qinvokable]
        #[cxx_name = "layerContentBounds"]
        fn layer_content_bounds(self: &Engine, index: i32) -> QRect;

        /// The layer's offset (top-left of its pixels in document space).
        #[qinvokable]
        #[cxx_name = "layerOffsetX"]
        fn layer_offset_x(self: &Engine, index: i32) -> i32;

        #[qinvokable]
        #[cxx_name = "layerOffsetY"]
        fn layer_offset_y(self: &Engine, index: i32) -> i32;

        /// Replace a layer's pixels and offset after a transform.
        #[qinvokable]
        #[cxx_name = "replaceLayerPixels"]
        fn replace_layer_pixels(
            self: Pin<&mut Engine>,
            index: i32,
            image: &QImage,
            x: i32,
            y: i32,
        );

        /// Rotate the active layer by 90°, 180°, or 270°.
        #[qinvokable]
        #[cxx_name = "rotateLayer"]
        fn rotate_layer(self: Pin<&mut Engine>, degrees: i32);

        /// Flip the active layer horizontally or vertically.
        #[qinvokable]
        #[cxx_name = "flipLayer"]
        fn flip_layer(self: Pin<&mut Engine>, horizontal: bool);

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

        /// Set all three locks on a layer at once — the panel's Lock row.
        #[qinvokable]
        #[cxx_name = "setLayerLocks"]
        fn set_layer_locks(
            self: Pin<&mut Engine>,
            index: i32,
            transparency: bool,
            pixels: bool,
            position: bool,
        );

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

        /// Seal history coalescing so the next coalescing commit starts a new entry.
        #[qinvokable]
        #[cxx_name = "sealHistory"]
        fn seal_history(self: Pin<&mut Engine>);

        /// Strip the text record from a type layer, turning it into plain
        /// raster pixels. The layer keeps its image, position, blend mode and
        /// everything else — it just stops being editable as text.
        #[qinvokable]
        #[cxx_name = "rasterizeLayer"]
        fn rasterize_layer(self: Pin<&mut Engine>, index: i32);

        /// Start describing a piece of text, run by run.
        ///
        /// Character formatting is per-character — two letters in the middle of
        /// a word can be a different size — so the text crosses the bridge as a
        /// list, not as a string plus one font. A list is awkward to pass here
        /// as an argument, so the shell builds it up instead: `beginTextRuns`,
        /// an `addTextRun` per run, then `addTextLayer` or `updateTextLayer`,
        /// which consume what was built.
        #[qinvokable]
        #[cxx_name = "beginTextRuns"]
        fn begin_text_runs(self: Pin<&mut Engine>);

        /// Append one run of same-formatted text: its characters, the font they
        /// are set in (family and style by name, `size` in document pixels) and
        /// their colour.
        #[qinvokable]
        #[cxx_name = "addTextRun"]
        fn add_text_run(
            self: Pin<&mut Engine>,
            text: &QString,
            family: &QString,
            style: &QString,
            size: f32,
            color: &QColor,
        );

        /// Add a layer from an image the shell already decoded — the frames of
        /// an animated GIF, which arrive one at a time from Qt's reader.
        /// `x`/`y` place the image's top-left in document space.
        #[qinvokable]
        #[cxx_name = "addImageLayer"]
        fn add_image_layer(
            self: Pin<&mut Engine>,
            image: &QImage,
            x: i32,
            y: i32,
            name: &QString,
        ) -> bool;

        /// Add a layer from pixels the shell already rasterized — what the
        /// Type tool commits. `x`/`y` place the image's top-left in document
        /// space; the image may hang off the canvas, like any other layer.
        ///
        /// The type record stored alongside those pixels is the runs built up
        /// since `beginTextRuns`, plus what belongs to the block rather than to
        /// any run: `align` (0 left, 1 centre, 2 right — top, centre, bottom for
        /// vertical type), whether it was antialiased, whether it is `vertical`,
        /// and `origin_x`/`origin_y` — the document-space point the lines are
        /// laid out from, which is where the user first clicked rather than the
        /// image's corner. Fails if no runs were built.
        #[qinvokable]
        #[cxx_name = "addTextLayer"]
        fn add_text_layer(
            self: Pin<&mut Engine>,
            image: &QImage,
            x: i32,
            y: i32,
            name: &QString,
            align: i32,
            antialias: bool,
            vertical: bool,
            origin_x: f32,
            origin_y: f32,
        ) -> bool;

        /// Re-render the type layer at a panel index from new pixels and the
        /// runs built up since `beginTextRuns` — committing an edit of text that
        /// already exists.
        ///
        /// Unlike adding, this keeps the layer itself: its place in the stack,
        /// its blend mode, opacity, mask and locks all survive being retyped.
        /// Returns false if that layer is gone, which means the edit should be
        /// committed as a new layer instead.
        #[qinvokable]
        #[cxx_name = "updateTextLayer"]
        fn update_text_layer(
            self: Pin<&mut Engine>,
            index: i32,
            image: &QImage,
            x: i32,
            y: i32,
            name: &QString,
            align: i32,
            antialias: bool,
            vertical: bool,
            origin_x: f32,
            origin_y: f32,
        ) -> bool;

        /// Make a selection out of an image's alpha — what the Type Mask tools
        /// commit. `x`/`y` place the image's top-left in document space, and
        /// anything of it that falls off the canvas is ignored. `op` is a
        /// [`SelectionOp`] code, as the other selection calls take.
        ///
        /// Mask type produces no layer at all: the letterforms become the
        /// selection and the text itself is gone, which is the whole point of
        /// the tool.
        #[qinvokable]
        #[cxx_name = "selectFromAlpha"]
        fn select_from_alpha(self: Pin<&mut Engine>, image: &QImage, x: i32, y: i32, op: i32)
            -> bool;

        /// Panel index of the topmost visible type layer whose bounds contain a
        /// document-space point, or -1. This is what makes clicking existing
        /// text with the Type tool reopen it rather than start new text.
        #[qinvokable]
        #[cxx_name = "textLayerAt"]
        fn text_layer_at(self: &Engine, x: i32, y: i32) -> i32;

        /// The type record of the layer at a panel index, read back run by run
        /// the way it was written. Each getter returns a default when the layer
        /// is not a type layer or the run does not exist, so a caller that has
        /// already hit-tested with `textLayerAt` needs no further checks.
        #[qinvokable]
        #[cxx_name = "layerTextRunCount"]
        fn layer_text_run_count(self: &Engine, index: i32) -> i32;

        #[qinvokable]
        #[cxx_name = "layerTextRunText"]
        fn layer_text_run_text(self: &Engine, index: i32, run: i32) -> QString;

        #[qinvokable]
        #[cxx_name = "layerTextRunFamily"]
        fn layer_text_run_family(self: &Engine, index: i32, run: i32) -> QString;

        #[qinvokable]
        #[cxx_name = "layerTextRunStyle"]
        fn layer_text_run_style(self: &Engine, index: i32, run: i32) -> QString;

        #[qinvokable]
        #[cxx_name = "layerTextRunSize"]
        fn layer_text_run_size(self: &Engine, index: i32, run: i32) -> f32;

        #[qinvokable]
        #[cxx_name = "layerTextRunColor"]
        fn layer_text_run_color(self: &Engine, index: i32, run: i32) -> QColor;

        #[qinvokable]
        #[cxx_name = "layerTextAlign"]
        fn layer_text_align(self: &Engine, index: i32) -> i32;

        #[qinvokable]
        #[cxx_name = "layerTextAntialias"]
        fn layer_text_antialias(self: &Engine, index: i32) -> bool;

        /// Whether the layer is vertical type — which the Type tool takes on
        /// when it reopens it, since the orientation belongs to the text and
        /// not to whichever of the two tools is in hand.
        #[qinvokable]
        #[cxx_name = "layerTextVertical"]
        fn layer_text_vertical(self: &Engine, index: i32) -> bool;

        #[qinvokable]
        #[cxx_name = "layerTextOriginX"]
        fn layer_text_origin_x(self: &Engine, index: i32) -> f32;

        #[qinvokable]
        #[cxx_name = "layerTextOriginY"]
        fn layer_text_origin_y(self: &Engine, index: i32) -> f32;

        /// Hold a type layer's pixels back while the Type tool has it open, so
        /// the user sees the live overlay instead of it over the old rendering.
        /// Not an edit: it makes no history state and does not disturb what the
        /// Layers panel reports about the layer.
        #[qinvokable]
        #[cxx_name = "beginTextEdit"]
        fn begin_text_edit(self: Pin<&mut Engine>, index: i32) -> bool;

        /// Give those pixels back — the end of the edit, committed or not.
        #[qinvokable]
        #[cxx_name = "endTextEdit"]
        fn end_text_edit(self: Pin<&mut Engine>);
    }

    // -- painting ----------------------------------------------------------
    unsafe extern "RustQt" {
        /// Configure the brush from the tool options bar.
        /// `hardness`, `opacity` and `flow` are percentages, 0-100.
        /// Tip shape and scattering — Photoshop's Brush Tip Shape, Scattering
        /// and Shape Dynamics, as far as the engine models them.
        ///
        /// `roundness` 5-100 (%), `angle` in degrees, `scatter` as a percentage
        /// of diameter, `count` dabs per step, and the three jitter amounts.
        #[qinvokable]
        #[cxx_name = "setBrushShape"]
        fn set_brush_shape(
            self: Pin<&mut Engine>,
            roundness: i32,
            angle: i32,
            scatter: i32,
            count: i32,
            size_jitter: i32,
            angle_jitter: i32,
            roundness_jitter: i32,
        );

        /// One step of the current brush, rendered centred in a `width` by
        /// `height` image — the preset thumbnails and the tip preview.
        ///
        /// Drawn by the brush engine itself, so a thumbnail cannot drift from
        /// what the brush actually paints.
        #[qinvokable]
        #[cxx_name = "brushPreview"]
        fn brush_preview(self: &Engine, width: i32, height: i32) -> QImage;

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

        /// Put the brush into healing mode: `endStroke` then rebuilds what the
        /// stroke covered from its surroundings instead of painting a colour.
        ///
        /// `mode` is CS6's Type: 0 = Proximity Match, 1 = Create Texture,
        /// 2 = Content-Aware. A negative value turns healing off.
        /// Whether dab edges are antialiased. False is the Pencil, which paints
        /// whole pixels only.
        #[qinvokable]
        #[cxx_name = "setBrushAntialias"]
        fn set_brush_antialias(self: Pin<&mut Engine>, antialias: bool);

        /// The Pencil's Auto Erase: a stroke begun on a pixel already the
        /// foreground colour paints the background colour instead.
        #[qinvokable]
        #[cxx_name = "setAutoErase"]
        fn set_auto_erase(self: Pin<&mut Engine>, enabled: bool);

        /// The Color Replacement Brush's options bar.
        ///
        /// `mode` 0-3 (Hue, Saturation, Color, Luminosity), `sampling` 0-2
        /// (Continuous, Once, Background Swatch), `limits` 0-2 (Discontiguous,
        /// Contiguous, Find Edges), `tolerance` 0-255.
        #[qinvokable]
        #[cxx_name = "setReplaceOptions"]
        fn set_replace_options(
            self: Pin<&mut Engine>,
            mode: i32,
            sampling: i32,
            limits: i32,
            tolerance: i32,
            antialias: bool,
        );

        /// The Background Eraser's options bar. `sampling` 0-2 (Continuous,
        /// Once, Background Swatch), `limits` 0-2 (Discontiguous, Contiguous,
        /// Find Edges), `tolerance` as CS6 shows it — a percentage — and
        /// Protect Foreground Color.
        #[qinvokable]
        #[cxx_name = "setBackgroundEraseOptions"]
        fn set_background_erase_options(
            self: Pin<&mut Engine>,
            sampling: i32,
            limits: i32,
            tolerance_percent: i32,
            protect_foreground: bool,
        );

        /// Begin a Background Eraser stroke. Returns false if the active layer
        /// cannot be erased — including one with Lock Transparent Pixels on,
        /// since erasing does nothing else.
        #[qinvokable]
        #[cxx_name = "beginBackgroundErase"]
        fn begin_background_erase(self: Pin<&mut Engine>, x: f32, y: f32, pressure: f32) -> bool;

        /// Continue one.
        #[qinvokable]
        #[cxx_name = "extendBackgroundErase"]
        fn extend_background_erase(self: Pin<&mut Engine>, x: f32, y: f32, pressure: f32);

        /// Finish one, recording a single undo step.
        #[qinvokable]
        #[cxx_name = "endBackgroundErase"]
        fn end_background_erase(self: Pin<&mut Engine>);

        /// Abandon one, restoring what it changed.
        #[qinvokable]
        #[cxx_name = "cancelBackgroundErase"]
        fn cancel_background_erase(self: Pin<&mut Engine>);

        /// Erase the region a click lands in — the Magic Eraser. One undo step,
        /// and no stroke: it is the Magic Wand's flood, erased.
        ///
        /// `tolerance` 0-255, `opacity` a percentage.
        #[qinvokable]
        #[cxx_name = "magicErase"]
        fn magic_erase(
            self: Pin<&mut Engine>,
            x: i32,
            y: i32,
            tolerance: i32,
            contiguous: bool,
            antialias: bool,
            sample_all: bool,
            opacity: i32,
        ) -> bool;

        /// Begin a colour-replacement stroke. Returns false if the active layer
        /// cannot be painted.
        #[qinvokable]
        #[cxx_name = "beginReplace"]
        fn begin_replace(self: Pin<&mut Engine>, x: f32, y: f32, pressure: f32) -> bool;

        /// Continue one.
        #[qinvokable]
        #[cxx_name = "extendReplace"]
        fn extend_replace(self: Pin<&mut Engine>, x: f32, y: f32, pressure: f32);

        /// Finish one, recording a single undo step.
        #[qinvokable]
        #[cxx_name = "endReplace"]
        fn end_replace(self: Pin<&mut Engine>);

        /// Abandon one, restoring what it changed.
        #[qinvokable]
        #[cxx_name = "cancelReplace"]
        fn cancel_replace(self: Pin<&mut Engine>);

        /// The Mixer Brush's options bar. `wet`, `load`, `mix` and `flow` are
        /// percentages, 0-100.
        #[qinvokable]
        #[cxx_name = "setMixerOptions"]
        fn set_mixer_options(
            self: Pin<&mut Engine>,
            wet: i32,
            load: i32,
            mix: i32,
            flow: i32,
            sample_all_layers: bool,
            load_after_stroke: bool,
            clean_after_stroke: bool,
        );

        /// Fill the brush's reservoir with the foreground colour — CS6's "Load
        /// Brush".
        #[qinvokable]
        #[cxx_name = "loadMixerBrush"]
        fn load_mixer_brush(self: Pin<&mut Engine>);

        /// Load the reservoir from the image at `(x, y)` — CS6's Alt-click,
        /// which is how paint is picked up to mix with.
        #[qinvokable]
        #[cxx_name = "loadMixerBrushFrom"]
        fn load_mixer_brush_from(self: Pin<&mut Engine>, x: i32, y: i32);

        /// Empty the reservoir — CS6's "Clean Brush". A clean wet brush smears
        /// without adding colour of its own.
        #[qinvokable]
        #[cxx_name = "cleanMixerBrush"]
        fn clean_mixer_brush(self: Pin<&mut Engine>);

        /// The paint currently on the brush, for the options bar's load swatch.
        /// Fully transparent means clean.
        #[qinvokable]
        #[cxx_name = "mixerLoadColor"]
        fn mixer_load_color(self: &Engine) -> QColor;

        /// Begin a Mixer Brush stroke. Returns false if the active layer cannot
        /// be painted.
        #[qinvokable]
        #[cxx_name = "beginMixer"]
        fn begin_mixer(self: Pin<&mut Engine>, x: f32, y: f32, pressure: f32) -> bool;

        /// Continue one.
        #[qinvokable]
        #[cxx_name = "extendMixer"]
        fn extend_mixer(self: Pin<&mut Engine>, x: f32, y: f32, pressure: f32);

        /// Finish one, recording a single undo step. The reservoir carries over
        /// to the next stroke unless Clean or Load after each stroke is on.
        #[qinvokable]
        #[cxx_name = "endMixer"]
        fn end_mixer(self: Pin<&mut Engine>);

        /// Abandon one, restoring what it changed.
        #[qinvokable]
        #[cxx_name = "cancelMixer"]
        fn cancel_mixer(self: Pin<&mut Engine>);

        /// The Clone Stamp's options bar: Aligned, and Sample (0 = current
        /// layer, 1 = current and below, 2 = all layers).
        #[qinvokable]
        #[cxx_name = "setCloneOptions"]
        fn set_clone_options(self: Pin<&mut Engine>, aligned: bool, sampling: i32);

        /// Set the point the Clone Stamp copies from — CS6's Alt-click. Also
        /// forgets the offset an aligned stroke had established, so the next
        /// stroke measures afresh from the new point.
        ///
        /// Returns whether there is anything to copy there *under the current
        /// Sample mode*. Sampling the current layer alone — CS6's default — finds
        /// nothing if the material is on another layer, and a stroke that copies
        /// transparency looks to the user like a tool that does not work.
        #[qinvokable]
        #[cxx_name = "setCloneSource"]
        fn set_clone_source(self: Pin<&mut Engine>, x: i32, y: i32) -> bool;

        /// Forget the source, so the tool asks for one again.
        #[qinvokable]
        #[cxx_name = "clearCloneSource"]
        fn clear_clone_source(self: Pin<&mut Engine>);

        /// Whether a source has been set. Without one the Clone Stamp has
        /// nothing to copy and refuses to paint, as Photoshop's does.
        #[qinvokable]
        #[cxx_name = "hasCloneSource"]
        fn has_clone_source(self: &Engine) -> bool;

        /// Begin a Clone Stamp stroke. Returns false if there is no source or
        /// the active layer cannot be painted on. The stroke is then extended
        /// and ended through `extendStroke` / `endStroke` like any other.
        #[qinvokable]
        #[cxx_name = "beginCloneStroke"]
        fn begin_clone_stroke(self: Pin<&mut Engine>, x: f32, y: f32, pressure: f32) -> bool;

        /// The built-in patterns, newline-separated and in picker order. The
        /// engine owns the list, so the picker cannot offer one it cannot paint.
        #[qinvokable]
        #[cxx_name = "patternNames"]
        fn pattern_names(self: &Engine) -> QString;

        /// A pattern's tile as an image, for the picker's swatches. Rendered by
        /// the engine, so a swatch cannot drift from what the tool paints.
        #[qinvokable]
        #[cxx_name = "patternPreview"]
        fn pattern_preview(self: &Engine, index: i32, size: i32) -> QImage;

        /// The Pattern Stamp's options bar: which pattern, and whether the tile
        /// is pinned to the document (Aligned) or to each stroke's start.
        #[qinvokable]
        #[cxx_name = "setPatternOptions"]
        fn set_pattern_options(self: Pin<&mut Engine>, index: i32, aligned: bool);

        /// Begin a Pattern Stamp stroke. Returns false if the active layer
        /// cannot be painted on. Extended and ended through `extendStroke` /
        /// `endStroke` like any other stroke — unlike the Clone Stamp it needs
        /// no source point, since the pattern is the source.
        #[qinvokable]
        #[cxx_name = "beginPatternStroke"]
        fn begin_pattern_stroke(self: Pin<&mut Engine>, x: f32, y: f32, pressure: f32) -> bool;

        /// The shape tools' options bar. `kind` 0-5 in flyout order
        /// (Rectangle, Rounded Rectangle, Ellipse, Polygon, Line, Custom
        /// Shape), then the setting each of them owns: the rounded
        /// rectangle's Radius, the polygon's Sides, the line's Weight, and
        /// which custom shape is chosen.
        #[qinvokable]
        #[cxx_name = "setShapeOptions"]
        fn set_shape_options(
            self: Pin<&mut Engine>,
            kind: i32,
            corner_radius: f32,
            sides: i32,
            line_weight: f32,
            custom: i32,
        );

        /// The outline a drag from `x0, y0` to `x1, y1` marks out, for the
        /// canvas to preview while the button is down.
        ///
        /// The same call `drawShape` commits from, so what is previewed and
        /// what lands cannot disagree — and the shell needs no geometry of its
        /// own to draw a rounded corner or an ellipse. `shift` and `alt` are
        /// CS6's modifiers, which mean different things to different tools.
        #[qinvokable]
        #[cxx_name = "shapeOutline"]
        fn shape_outline(
            self: &Engine,
            x0: f32,
            y0: f32,
            x1: f32,
            y1: f32,
            shift: bool,
            alt: bool,
        ) -> QPolygonF;

        /// Commit a dragged shape. `mode` is CS6's Mode menu: 0 a shape layer
        /// of its own, 1 a path on the work path, 2 pixels on the active layer.
        /// Shape and Pixels use the foreground colour. Returns false if nothing
        /// came of it — a drag that went nowhere, or a layer that refuses
        /// pixels.
        #[qinvokable]
        #[cxx_name = "drawShape"]
        fn draw_shape(
            self: Pin<&mut Engine>,
            x0: f32,
            y0: f32,
            x1: f32,
            y1: f32,
            shift: bool,
            alt: bool,
            mode: i32,
        ) -> bool;

        /// The custom shapes, newline-separated and in picker order.
        #[qinvokable]
        #[cxx_name = "customShapeNames"]
        fn custom_shape_names(self: &Engine) -> QString;

        /// One custom shape drawn on its own, for the picker's swatch.
        #[qinvokable]
        #[cxx_name = "customShapePreview"]
        fn custom_shape_preview(self: &Engine, index: i32, size: i32) -> QImage;

        /// The gradient presets, newline-separated and in CS6's order. The engine
        /// owns the list so the options bar cannot offer a name it cannot draw.
        #[qinvokable]
        #[cxx_name = "gradientPresetNames"]
        fn gradient_preset_names(self: &Engine) -> QString;

        /// A preview strip of a preset, for the options bar's swatch and the
        /// preset menu. Rendered by the engine, so a preview cannot drift from
        /// what the tool paints. An unknown name gives an empty image.
        #[qinvokable]
        #[cxx_name = "gradientPreview"]
        fn gradient_preview(self: &Engine, name: &QString, width: i32, height: i32) -> QImage;

        /// The Gradient tool's options bar. `kind` 0-4 (Linear, Radial, Angle,
        /// Reflected, Diamond), `mode` a blend-mode discriminant, `opacity` a
        /// percentage.
        #[qinvokable]
        #[cxx_name = "setGradientOptions"]
        fn set_gradient_options(
            self: Pin<&mut Engine>,
            preset: &QString,
            kind: i32,
            mode: i32,
            opacity: i32,
            reverse: bool,
            dither: bool,
            transparency: bool,
        );

        /// Draw the current gradient along a drag, in document coordinates.
        /// Returns false if the active layer cannot be painted on, or the drag
        /// was too short to have a direction.
        #[qinvokable]
        #[cxx_name = "drawGradient"]
        fn draw_gradient(self: Pin<&mut Engine>, x0: f32, y0: f32, x1: f32, y1: f32) -> bool;

        /// Which of the Blur button's three tools strokes: 0 = Blur,
        /// 1 = Sharpen, 2 = Smudge.
        #[qinvokable]
        #[cxx_name = "setFocusTool"]
        fn set_focus_tool(self: Pin<&mut Engine>, tool: i32);

        /// Which of the Dodge button's three tools strokes: 0 = Dodge, 1 = Burn,
        /// 2 = Sponge. Picking one switches the retouch stroke to the toning
        /// family, as `setFocusTool` switches it back.
        #[qinvokable]
        #[cxx_name = "setToneTool"]
        fn set_tone_tool(self: Pin<&mut Engine>, tool: i32);

        /// The toning tools' options bar. `range` 0-2 (Shadows, Midtones,
        /// Highlights) and `protect_tones` belong to Dodge and Burn; `sponge`
        /// 0-1 (Desaturate, Saturate) and `vibrance` to the Sponge; `amount` is
        /// CS6's Exposure on the first two and Flow on the third.
        #[qinvokable]
        #[cxx_name = "setToneOptions"]
        fn set_tone_options(
            self: Pin<&mut Engine>,
            amount: i32,
            range: i32,
            sponge: i32,
            protect_tones: bool,
            vibrance: bool,
        );

        /// The options bar for whichever of the three is active. They share
        /// Strength, Mode and Sample All Layers; `protect_detail` is Sharpen's
        /// alone and `finger_painting` is Smudge's, and each is ignored by the
        /// tools it does not belong to. One call rather than three keeps the
        /// bridge surface small, which matters more here than tidiness on the
        /// C++ side (CLAUDE.md §3).
        #[qinvokable]
        #[cxx_name = "setFocusOptions"]
        fn set_focus_options(
            self: Pin<&mut Engine>,
            strength: i32,
            mode: i32,
            sample_all_layers: bool,
            protect_detail: bool,
            finger_painting: bool,
        );

        /// Begin a stroke with whichever retouch tool was last selected — the
        /// three behind the Blur button or the three behind Dodge. Returns false
        /// if the active layer cannot be painted on.
        ///
        /// One set of entry points for six tools: they differ in what a dab does
        /// and in nothing else the shell can see, and a thin bridge is worth more
        /// than a symmetrical one (CLAUDE.md §3).
        #[qinvokable]
        #[cxx_name = "beginRetouchStroke"]
        fn begin_retouch_stroke(self: Pin<&mut Engine>, x: f32, y: f32, pressure: f32) -> bool;

        /// Continue one.
        #[qinvokable]
        #[cxx_name = "extendRetouchStroke"]
        fn extend_retouch_stroke(self: Pin<&mut Engine>, x: f32, y: f32, pressure: f32);

        /// Finish one, recording a single undo step.
        #[qinvokable]
        #[cxx_name = "endRetouchStroke"]
        fn end_retouch_stroke(self: Pin<&mut Engine>);

        /// Abandon one, restoring what it changed.
        #[qinvokable]
        #[cxx_name = "cancelRetouchStroke"]
        fn cancel_retouch_stroke(self: Pin<&mut Engine>);

        /// The Paint Bucket's options bar. `mode` is a blend-mode discriminant,
        /// `opacity` a percentage, `tolerance` 0-255 as the Magic Wand's is.
        #[qinvokable]
        #[cxx_name = "setBucketOptions"]
        fn set_bucket_options(
            self: Pin<&mut Engine>,
            mode: i32,
            opacity: i32,
            tolerance: i32,
            antialias: bool,
            contiguous: bool,
            all_layers: bool,
        );

        /// Flood-fill from a click, in document coordinates. Returns false if the
        /// active layer cannot be painted on, or nothing matched.
        #[qinvokable]
        #[cxx_name = "fillBucket"]
        fn fill_bucket(self: Pin<&mut Engine>, x: i32, y: i32) -> bool;

        #[qinvokable]
        #[cxx_name = "setHealMode"]
        fn set_heal_mode(self: Pin<&mut Engine>, mode: i32);

        /// Point the Healing Brush at a source. `dx`/`dy` are added to each
        /// destination pixel to find where it samples from, so they are the
        /// offset from the stroke to the Alt-clicked source point.
        ///
        /// With `active` false the brush inpaints from its own surroundings,
        /// which is the Spot Healing Brush's behaviour.
        #[qinvokable]
        #[cxx_name = "setHealSource"]
        fn set_heal_source(self: Pin<&mut Engine>, active: bool, dx: i32, dy: i32);

        /// Apply the Patch tool. `(dx, dy)` is the drag; the flags are CS6's
        /// options bar — Content-Aware ignores the drag and rebuilds the
        /// selection in place, `destination` reverses which end of the drag is
        /// repaired, and `transparent` transfers texture without colour.
        #[qinvokable]
        #[cxx_name = "patchSelection"]
        fn patch_selection(
            self: Pin<&mut Engine>,
            dx: i32,
            dy: i32,
            content_aware: bool,
            destination: bool,
            transparent: bool,
        );

        /// Move the selection's contents and heal the hole — the Content-Aware
        /// Move tool.
        ///
        /// `extend` duplicates instead of moving. `structure` (1-7) and `color`
        /// (0-10) are CS6's two adaptation sliders, and `sampleAllLayers` reads
        /// the composite rather than the active layer.
        #[qinvokable]
        #[cxx_name = "contentAwareMove"]
        fn content_aware_move(
            self: Pin<&mut Engine>,
            dx: i32,
            dy: i32,
            extend: bool,
            structure: i32,
            color: i32,
            sample_all_layers: bool,
        );

        /// Neutralise red-eye inside a rectangle. `pupil` and `darken` are
        /// CS6's Pupil Size and Darken Amount, both 0-100.
        #[qinvokable]
        #[cxx_name = "removeRedEye"]
        fn remove_red_eye(
            self: Pin<&mut Engine>,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            pupil: i32,
            darken: i32,
        );

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

        /// Whether the document is in Quick Mask mode.
        #[qinvokable]
        #[cxx_name = "quickMask"]
        fn quick_mask(self: &Engine) -> bool;

        /// Enter or leave Quick Mask mode, where painting edits the selection
        /// instead of the image and what is *not* selected wears a red veil.
        #[qinvokable]
        #[cxx_name = "setQuickMask"]
        fn set_quick_mask(self: Pin<&mut Engine>, on: bool);

        /// The pixels a Copy takes: what the selection covers, from the active
        /// layer or — when `merged` — from the whole visible image.
        ///
        /// An empty image when there is nothing to copy. `copyOriginX`/`Y` say
        /// where in the document it came from, which is what Paste in Place
        /// needs and what an image on the system clipboard cannot carry.
        #[qinvokable]
        #[cxx_name = "copySelection"]
        fn copy_selection(self: Pin<&mut Engine>, merged: bool) -> QImage;

        #[qinvokable]
        #[cxx_name = "copyOriginX"]
        fn copy_origin_x(self: &Engine) -> i32;

        #[qinvokable]
        #[cxx_name = "copyOriginY"]
        fn copy_origin_y(self: &Engine) -> i32;

        /// Paste an image as a new layer with its top-left at `x`, `y`.
        ///
        /// `mode` is what the current selection does to it: 0 nothing, 1 keeps
        /// the paste inside the selection, 2 keeps it outside — Photoshop's
        /// Paste, Paste Into and Paste Outside. Both of the latter make a layer
        /// mask, so what was pasted can still be moved within it.
        #[qinvokable]
        #[cxx_name = "pasteImage"]
        fn paste_image(self: Pin<&mut Engine>, image: &QImage, x: i32, y: i32, mode: i32)
            -> bool;

        /// Sample the composited colour at a point, for the eyedropper.
        #[qinvokable]
        #[cxx_name = "pickColor"]
        fn pick_color(self: &Engine, x: i32, y: i32) -> QColor;
    }

    // -- selection ---------------------------------------------------------
    unsafe extern "RustQt" {
        /// `op`: 0 = replace, 1 = add, 2 = subtract, 3 = intersect.
        /// `feather`: radius in pixels applied to the new region before it
        /// combines — the options bar's Feather field. 0 for a hard edge.
        #[qinvokable]
        #[cxx_name = "selectRect"]
        fn select_rect(
            self: Pin<&mut Engine>,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            op: i32,
            feather: i32,
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
            feather: i32,
        );

        /// Combine a closed polygon — what the lasso family produces. `points`
        /// is x,y interleaved in document coordinates; the shape closes back
        /// to the first vertex. A flat vector rather than a point list keeps
        /// the bridge to types `cxx` already carries (CLAUDE.md §3).
        #[qinvokable]
        #[cxx_name = "selectPolygon"]
        fn select_polygon(
            self: Pin<&mut Engine>,
            points: &QVector_f32,
            op: i32,
            feather: i32,
        );

        /// Build the edge-cost field the Magnetic Lasso snaps to, from the
        /// current composite. Called once when the gesture starts; the field
        /// is cached until `endMagnetic`, because rebuilding it per mouse move
        /// would not be interactive.
        ///
        /// `contrast` is CS6's 1–100 Contrast option.
        #[qinvokable]
        #[cxx_name = "beginMagnetic"]
        fn begin_magnetic(self: Pin<&mut Engine>, contrast: i32);

        /// The magnetic wire from one point to another, as flat x,y pairs in
        /// document coordinates. `width` is CS6's detection width.
        ///
        /// Returns the straight line between the points when no edge map is
        /// live, so a caller that forgot `beginMagnetic` still gets a usable
        /// path rather than nothing.
        #[qinvokable]
        #[cxx_name = "magneticTrace"]
        fn magnetic_trace(
            self: &Engine,
            x0: i32,
            y0: i32,
            x1: i32,
            y1: i32,
            width: i32,
        ) -> Vec<i32>;

        /// Release the cached edge field.
        #[qinvokable]
        #[cxx_name = "endMagnetic"]
        fn end_magnetic(self: Pin<&mut Engine>);

        /// Magic Wand: select the pixels matching the one clicked.
        ///
        /// `tolerance` is 0–255 per channel, `contiguous` limits it to the
        /// connected region, `antialias` softens the boundary.
        #[qinvokable]
        #[cxx_name = "magicWand"]
        fn magic_wand(
            self: Pin<&mut Engine>,
            x: i32,
            y: i32,
            tolerance: i32,
            contiguous: bool,
            antialias: bool,
            op: i32,
            feather: i32,
        );

        /// Start a Quick Selection drag: snapshot the selection to combine
        /// against and build the edge field the dabs grow within.
        #[qinvokable]
        #[cxx_name = "beginQuickSelect"]
        fn begin_quick_select(self: Pin<&mut Engine>, op: i32, feather: i32);

        /// One brush dab of a Quick Selection drag. The selection is updated
        /// live, so the marching ants follow the brush.
        #[qinvokable]
        #[cxx_name = "quickSelectDab"]
        fn quick_select_dab(
            self: Pin<&mut Engine>,
            x: f32,
            y: f32,
            radius: f32,
            subtract: bool,
        );

        /// Finish a Quick Selection drag, releasing the cached state. The
        /// selection already holds the result.
        #[qinvokable]
        #[cxx_name = "endQuickSelect"]
        fn end_quick_select(self: Pin<&mut Engine>);

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

        /// The selection's contour, for the marching ants.
        ///
        /// Flattened as a run of loops, each `[n, x0, y0, … x(n-1), y(n-1)]`
        /// in document coordinates, closing back to its first point. A flat
        /// `Vec<i32>` rather than a list of polygons keeps the bridge to types
        /// `cxx` already carries (CLAUDE.md §3).
        #[qinvokable]
        #[cxx_name = "selectionOutline"]
        fn selection_outline(self: &Engine) -> Vec<i32>;

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

        /// channel: 0=RGB (all), 1=Red, 2=Green, 3=Blue
        #[qinvokable]
        #[cxx_name = "applyLevels"]
        fn apply_levels(self: Pin<&mut Engine>, in_black: f32, in_white: f32, gamma: f32, out_black: f32, out_white: f32, channel: i32);

        /// Apply a curves lookup table. `lut` is 256 u8 values. channel: 0=RGB, 1=R, 2=G, 3=B
        #[qinvokable]
        #[cxx_name = "applyCurvesLut"]
        fn apply_curves_lut(self: Pin<&mut Engine>, lut: &[u8], channel: i32);
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
    /// The document the user is working on. Kept as a plain field rather than an
    /// index into a list so every operation in this file reads `self.doc`
    /// unchanged; the other open documents wait on the shelf.
    doc: Document,
    /// The open documents *other* than the active one, in tab order.
    ///
    /// Switching tabs puts `doc` back where it belongs in this list and takes
    /// the requested one out. The full tab order is therefore `shelf` with `doc`
    /// inserted at `active`.
    shelf: Vec<Document>,
    /// Which tab `doc` occupies.
    active: usize,
    /// Next "Untitled-N" number to hand out, so new documents get distinct
    /// names the way Photoshop's do.
    next_untitled: u32,
    brush: Brush,
    foreground: Rgba8,
    background: Rgba8,
    erasing: bool,
    /// The Pencil's Auto Erase option, and whether it applies to the stroke in
    /// progress — decided at the first dab, since it depends on what was under
    /// the cursor then.
    auto_erase: bool,
    auto_erase_active: bool,
    /// The Color Replacement Brush's options, held between strokes.
    replace_options: ReplaceOptions,
    /// The Mixer Brush's options, held between strokes.
    mixer_options: MixerOptions,
    /// The paint on the Mixer Brush. It survives strokes — and tool switches, as
    /// in Photoshop, where the brush stays loaded until it is cleaned.
    mixer_reservoir: Rgba8,
    /// CS6's two toggles beside the load swatch: reload from the foreground, or
    /// clean off, once each stroke ends.
    mixer_load_after_stroke: bool,
    mixer_clean_after_stroke: bool,
    /// Where the last Copy came from in the document. The system clipboard
    /// carries pixels and nothing else, so Paste in Place remembers it here.
    copy_origin: (i32, i32),

    /// The shape tools' options bar, held between drags like every other
    /// tool's.
    shape_options: ShapeOptions,

    /// The Background Eraser's options, held between strokes like the brush.
    bg_erase_options: BackgroundEraseOptions,

    /// The Pattern Stamp's chosen pattern and its Aligned checkbox. Held here
    /// rather than in the document: they are tool settings that outlive any one
    /// stroke, like the brush itself.
    pattern_index: usize,
    pattern_aligned: bool,

    /// Text runs the shell is describing right now, between `beginTextRuns`
    /// and the `addTextLayer`/`updateTextLayer` that consumes them. Empty at
    /// rest — this is a builder's scratch space, not document state.
    pending_runs: Vec<TextRun>,

    /// Set while the Spot Healing Brush is active. `None` for every other tool,
    /// which is what makes `end_stroke` paint normally.
    heal_mode: Option<HealMode>,
    /// Source offset for the Healing Brush, which samples an explicit point
    /// rather than inpainting from the stroke's own surroundings.
    heal_source: Option<(i32, i32)>,

    /// The Clone Stamp's Alt-clicked source point, in document space.
    clone_source: Option<(i32, i32)>,
    /// The offset the last clone stroke used, kept so an **aligned** stroke
    /// carries on from where the previous one left off instead of jumping back
    /// to the source point. Cleared whenever a new source is set.
    clone_offset: Option<(i32, i32)>,
    clone_aligned: bool,
    clone_sampling: CloneSampling,

    /// The Gradient tool's options. The preset is held by *name*: the ramps for
    /// the first few depend on the current foreground and background, so they
    /// are built at the moment of drawing rather than cached here.
    gradient_preset: String,
    gradient_options: GradientOptions,
    /// The Paint Bucket's options, held between clicks.
    bucket_options: BucketOptions,
    /// Which of the Blur button's three tools is active, and the options each
    /// keeps between strokes. Smudge carries different state mid-stroke, so it
    /// gets its own settings rather than sharing a struct.
    focus_tool: i32,
    focus_options: FocusOptions,
    smudge_options: SmudgeOptions,
    /// The toning tools' settings, and whether one of them is the retouch tool
    /// in hand. The two families are picked from different strip buttons, so the
    /// last `setFocusTool` or `setToneTool` decides which strokes.
    tone_active: bool,
    tone_options: ToneOptions,
    /// Edge field for the Magnetic Lasso, live only for the duration of one
    /// gesture. `None` the rest of the time so a large document is not paying
    /// for a float per pixel it is not using.
    edge_map: Option<EdgeMap>,

    /// Quick Selection state, live only during a drag for the same reason.
    /// `quick_base` is the selection as it stood when the drag began; every
    /// dab recombines against it, so a subtract mid-drag can give pixels back.
    quick_select: Option<QuickSelector>,
    quick_base: Option<Selection>,
    quick_op: SelectionOp,
    quick_feather: u32,
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
            shelf: Vec::new(),
            active: 0,
            next_untitled: 2,
            brush: Brush::default(),
            foreground: Rgba8::BLACK,
            background: Rgba8::WHITE,
            erasing: false,
            auto_erase: false,
            auto_erase_active: false,
            replace_options: ReplaceOptions::default(),
            mixer_options: MixerOptions::default(),
            // An unloaded brush would make the tool do nothing on first use, so
            // it starts loaded with the default foreground, as CS6's does.
            mixer_reservoir: Rgba8::BLACK,
            mixer_load_after_stroke: false,
            mixer_clean_after_stroke: false,
            copy_origin: (0, 0),
            shape_options: ShapeOptions::default(),
            bg_erase_options: BackgroundEraseOptions {
                // CS6's defaults: sample as you go, keep to what the crosshair
                // is connected to, half tolerance.
                sampling: Sampling::Continuous,
                limits: Limits::Contiguous,
                tolerance: 50 * 255 / 100,
                protect_foreground: false,
            },
            pattern_index: 0,
            pattern_aligned: true,
            pending_runs: Vec::new(),
            heal_mode: None,
            heal_source: None,
            clone_source: None,
            clone_offset: None,
            // CS6 ships with Aligned on and Sample set to the current layer.
            clone_aligned: true,
            clone_sampling: CloneSampling::CurrentLayer,
            gradient_preset: gradient::PRESET_NAMES[0].to_string(),
            gradient_options: GradientOptions::default(),
            bucket_options: BucketOptions::default(),
            focus_tool: 0,
            focus_options: FocusOptions::default(),
            smudge_options: SmudgeOptions::default(),
            tone_active: false,
            tone_options: ToneOptions::default(),
            edge_map: None,
            quick_select: None,
            quick_base: None,
            quick_op: SelectionOp::Replace,
            quick_feather: 0,
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

/// Photoshop's rubylith, laid over what the selection does *not* cover.
///
/// Display only, and deliberately applied here rather than in the document's
/// own compositing: the veil is a way of showing a selection, not part of the
/// image. Saving, flattening and every filter go on seeing the picture without
/// it.
fn apply_quick_mask_veil(composite: &mut Pixmap, selection: &Selection) {
    // Nothing selected means nothing masked — entering Quick Mask selects all
    // (see `Document::set_quick_mask`), so an empty mask here can only mean a
    // document that has none, and covering it in red would be a lie.
    if selection.is_empty() {
        return;
    }

    const VEIL: Rgba8 = Rgba8 { r: 255, g: 0, b: 0, a: 255 };
    const STRENGTH: f32 = 0.5;

    for y in 0..composite.height() as i32 {
        for x in 0..composite.width() as i32 {
            let masked = 1.0 - selection.coverage_at(x, y);
            if masked <= 0.0 {
                continue;
            }
            let under = composite.get(x, y);
            composite.set(x, y, crate::brush::source_over(under, VEIL, masked * STRENGTH));
        }
    }
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
fn pixmap_to_qimage(pm: Pixmap) -> QImage {
    if pm.is_empty() {
        return QImage::default();
    }
    let mut pm = if pm.bpc() != 1 { pm.to_8bit() } else { pm };
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

    /// The type record of the layer at a panel index, if it has one.
    fn text_content_at(&self, panel_index: i32) -> Option<&TextContent> {
        self.layer_id_at(panel_index)
            .and_then(|id| self.doc.layers().by_id(id))
            .and_then(|l| l.text.as_ref())
    }

    /// One run of that record.
    fn text_run_at(&self, panel_index: i32, run: i32) -> Option<&TextRun> {
        if run < 0 {
            return None;
        }
        self.text_content_at(panel_index)
            .and_then(|t| t.runs.get(run as usize))
    }

    /// Take the runs built up since `beginTextRuns` and make a type record of
    /// them. `None` when nothing was built, which is the shell failing to
    /// describe its text rather than an empty piece of text — that case never
    /// reaches here, because committing empty text deletes the layer.
    fn take_text_content(
        &mut self,
        align: i32,
        antialias: bool,
        vertical: bool,
        origin_x: f32,
        origin_y: f32,
    ) -> Option<TextContent> {
        if self.pending_runs.is_empty() {
            return None;
        }
        Some(TextContent {
            runs: std::mem::take(&mut self.pending_runs),
            align: match align {
                1 => TextAlign::Center,
                2 => TextAlign::Right,
                _ => TextAlign::Left,
            },
            antialias,
            vertical,
            origin: (origin_x, origin_y),
        })
    }

    /// The inverse of [`EngineRust::layer_id_at`].
    fn panel_index_of(&self, id: LayerId) -> i32 {
        let count = self.doc.layer_count();
        self.doc
            .layers()
            .index_of(id)
            .map_or(-1, |i| (count - 1 - i) as i32)
    }

    /// How many documents are open: the shelved ones plus the active one.
    fn tab_count(&self) -> usize {
        self.shelf.len() + 1
    }

    /// The document at a tab index, active or shelved.
    fn document_at(&self, index: i32) -> Option<&Document> {
        let index = usize::try_from(index).ok()?;
        if index == self.active {
            return Some(&self.doc);
        }
        if index >= self.tab_count() {
            return None;
        }
        // The shelf is the tab order with the active document taken out, so
        // anything past it shifts down by one.
        let shelf_index = if index < self.active { index } else { index - 1 };
        self.shelf.get(shelf_index)
    }

    /// The colour a stroke should paint with, honouring erase mode.
    fn paint_color(&self) -> Rgba8 {
        if self.erasing || self.auto_erase_active {
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
        // Every caller of `sync` has replaced or resized the document, which
        // takes the selection with it. `canvasChanged` deliberately does not
        // imply this — it fires on every brush dab, and re-tracing the
        // selection contour that often is what made painting inside a marquee
        // crawl.
        self.as_mut().selection_changed();
    }

    // -- document -----------------------------------------------------------

    fn new_document(mut self: core::pin::Pin<&mut Self>, width: i32, height: i32, fill: i32) {
        let w = width.clamp(1, 30_000) as u32;
        let h = height.clamp(1, 30_000) as u32;
        let background = self.background;

        let mut doc = match fill {
            1 => Document::new_transparent(w, h),
            2 => Document::new(w, h, background),
            _ => Document::new(w, h, Rgba8::WHITE),
        };
        doc.untitled_number = self.next_untitled;
        self.as_mut().rust_mut().next_untitled += 1;
        self.as_mut().add_document(doc);
        self.sync();
    }

    fn open_file(mut self: core::pin::Pin<&mut Self>, path: &QString) -> bool {
        let path = path.to_string();
        let Ok(bytes) = std::fs::read(&path) else {
            return false;
        };

        if path.to_lowercase().ends_with(".psd") {
            let Ok(file) = crate::psd::parse(&bytes) else {
                return false;
            };

            // A PSD is a stack, not a picture. The flattened composite it also
            // carries is only used when the file has no layer section — a
            // flattened save, or one written by something that does not store
            // layers — because opening a layered file as one layer throws the
            // work away.
            let mut doc = if file.layers.is_empty() {
                Document::from_pixmap(crate::psd::to_pixmap(&file))
            } else {
                Document::from_layers(file.layers, file.header.width, file.header.height)
            };
            doc.path = Some(path);
            doc.mark_saved();
            self.as_mut().add_document(doc);
            self.sync();
            return true;
        }

        let pixmap = {
            // Everything else goes through Qt's image plugins, which already
            // cover PNG/JPEG/TIFF/WebP — no reason to reimplement them.
            let Some(pm) = QImage::from_data(&bytes, None).as_ref().and_then(qimage_to_pixmap)
            else {
                return false;
            };

            // A camera held sideways writes the pixels as the sensor read them
            // and records which way up they go in EXIF. Applying it here, once,
            // is what makes a portrait photograph open portrait — and means
            // nothing downstream has to carry an orientation around.
            match metadata::read(&bytes).orientation {
                Some(value) => pm.transformed(metadata::Orientation::from_exif(value)),
                None => pm,
            }
        };

        let mut doc = Document::from_pixmap(pixmap);
        doc.path = Some(path);
        doc.mark_saved();
        self.as_mut().add_document(doc);
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
        self.as_mut().add_document(doc);
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

        // The whole stack, not the flattened picture: a PSD that comes back as
        // one layer is a project that has been thrown away. The composite goes
        // in alongside it, which is what other programs display.
        let composite = self.doc.composite();
        let bytes = crate::psd::write_layered_psd(self.doc.layers(), &composite);
        if std::fs::write(&path, bytes).is_err() {
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

    fn document_path(&self) -> QString {
        QString::from(self.doc.path.clone().unwrap_or_default().as_str())
    }

    fn file_metadata(&self, path: &QString) -> QString {
        let Ok(bytes) = std::fs::read(path.to_string()) else {
            return QString::default();
        };
        let meta = metadata::read(&bytes);
        let records: Vec<String> = meta
            .fields
            .iter()
            .map(|f| format!("{}\t{}\t{}", f.category, f.label, f.value))
            .collect();
        QString::from(records.join("\n").as_str())
    }

    fn file_xmp(&self, path: &QString) -> QString {
        let Ok(bytes) = std::fs::read(path.to_string()) else {
            return QString::default();
        };
        QString::from(metadata::read(&bytes).xmp.unwrap_or_default().as_str())
    }

    fn composite_image(&self) -> QImage {
        let mut composite = self.doc.composite();
        if self.doc.quick_mask() {
            apply_quick_mask_veil(&mut composite, self.doc.selection());
        }
        pixmap_to_qimage(composite)
    }

    fn preview_image(&self) -> QImage {
        // While healing, the stroke is only a marker for where to work — it is
        // not paint. Showing it as a translucent grey wash tells the user what
        // the brush has covered without implying the foreground colour is about
        // to be applied, which is how CS6 previews it too.
        let (color, opacity) = if self.heal_mode.is_some() {
            (Rgba8::new(128, 128, 128, 255), 0.45)
        } else {
            (self.paint_color(), self.brush.opacity)
        };
        // In Quick Mask the stroke is not paint, so the image underneath does
        // not change as it is drawn — what changes is the mask over it.
        if self.doc.quick_mask() {
            let mut composite = self.doc.composite();
            match self.doc.quick_mask_preview(color, opacity) {
                Some(preview) => apply_quick_mask_veil(&mut composite, &preview),
                None => apply_quick_mask_veil(&mut composite, self.doc.selection()),
            }
            return pixmap_to_qimage(composite);
        }

        match self.doc.preview_stroke(color, opacity) {
            Some(pm) => pixmap_to_qimage(pm),
            None => self.composite_image(),
        }
    }

    // -- annotations ------------------------------------------------------------

    fn marker_count(&self, kind: i32) -> i32 {
        MarkerKind::from_i32(kind).map_or(0, |k| self.doc.annotations().count(k) as i32)
    }

    fn marker_at(&self, kind: i32, index: i32) -> Vec<i32> {
        let Some(kind) = MarkerKind::from_i32(kind) else {
            return Vec::new();
        };
        let Some(marker) = usize::try_from(index)
            .ok()
            .and_then(|i| self.doc.annotations().marker(kind, i))
        else {
            return Vec::new();
        };
        vec![marker.x, marker.y]
    }

    fn add_marker(mut self: core::pin::Pin<&mut Self>, kind: i32, x: i32, y: i32) -> i32 {
        let Some(kind) = MarkerKind::from_i32(kind) else {
            return -1;
        };
        let index = self.as_mut().rust_mut().doc.annotations_mut().add(kind, x, y);
        match index {
            Some(i) => {
                self.as_mut().annotations_changed();
                i as i32
            }
            None => -1,
        }
    }

    fn move_marker(
        mut self: core::pin::Pin<&mut Self>,
        kind: i32,
        index: i32,
        x: i32,
        y: i32,
    ) -> bool {
        let (Some(kind), Ok(index)) = (MarkerKind::from_i32(kind), usize::try_from(index)) else {
            return false;
        };
        if !self
            .as_mut()
            .rust_mut()
            .doc
            .annotations_mut()
            .move_marker(kind, index, x, y)
        {
            return false;
        }
        self.as_mut().annotations_changed();
        true
    }

    fn remove_marker(mut self: core::pin::Pin<&mut Self>, kind: i32, index: i32) -> bool {
        let (Some(kind), Ok(index)) = (MarkerKind::from_i32(kind), usize::try_from(index)) else {
            return false;
        };
        if !self.as_mut().rust_mut().doc.annotations_mut().remove(kind, index) {
            return false;
        }
        self.as_mut().annotations_changed();
        true
    }

    fn clear_markers(mut self: core::pin::Pin<&mut Self>, kind: i32) {
        let Some(kind) = MarkerKind::from_i32(kind) else {
            return;
        };
        self.as_mut().rust_mut().doc.annotations_mut().clear(kind);
        self.as_mut().annotations_changed();
    }

    fn marker_near(&self, kind: i32, x: i32, y: i32, radius: f32) -> i32 {
        MarkerKind::from_i32(kind)
            .and_then(|k| self.doc.annotations().marker_at(k, x, y, radius.max(0.0)))
            .map_or(-1, |i| i as i32)
    }

    fn marker_text(&self, kind: i32, index: i32) -> QString {
        MarkerKind::from_i32(kind)
            .and_then(|k| usize::try_from(index).ok().and_then(|i| self.doc.annotations().marker(k, i)))
            .map_or_else(QString::default, |m| QString::from(m.text.as_str()))
    }

    fn set_marker_text(
        mut self: core::pin::Pin<&mut Self>,
        kind: i32,
        index: i32,
        text: &QString,
    ) -> bool {
        let (Some(kind), Ok(index)) = (MarkerKind::from_i32(kind), usize::try_from(index)) else {
            return false;
        };
        let text = text.to_string();
        if !self
            .as_mut()
            .rust_mut()
            .doc
            .annotations_mut()
            .set_text(kind, index, text)
        {
            return false;
        }
        self.as_mut().annotations_changed();
        true
    }

    fn has_ruler(&self) -> bool {
        self.doc.annotations().ruler().is_some()
    }

    fn set_ruler(mut self: core::pin::Pin<&mut Self>, ax: f32, ay: f32, bx: f32, by: f32) {
        self.as_mut()
            .rust_mut()
            .doc
            .annotations_mut()
            .set_ruler(Ruler::new(ax, ay, bx, by));
        self.as_mut().annotations_changed();
    }

    fn clear_ruler(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().rust_mut().doc.annotations_mut().clear_ruler();
        self.as_mut().annotations_changed();
    }

    fn ruler_line(&self) -> Vec<f32> {
        match self.doc.annotations().ruler() {
            Some(r) => vec![r.ax, r.ay, r.bx, r.by],
            None => Vec::new(),
        }
    }

    fn ruler_measurement(&self) -> Vec<f32> {
        match self.doc.annotations().ruler() {
            Some(r) => {
                let m = r.measure();
                vec![m.x, m.y, m.width, m.height, m.angle, m.distance]
            }
            None => Vec::new(),
        }
    }

    // -- slices ---------------------------------------------------------------

    fn slice_count(&self) -> i32 {
        self.doc.resolved_slices().len() as i32
    }

    fn slice_at(&self, index: i32) -> Vec<i32> {
        let slices = self.doc.resolved_slices();
        let Some(slice) = usize::try_from(index).ok().and_then(|i| slices.get(i)) else {
            return Vec::new();
        };
        vec![
            slice.rect.x,
            slice.rect.y,
            slice.rect.width as i32,
            slice.rect.height as i32,
            slice.number as i32,
            slice.user_index.map_or(-1, |i| i as i32),
        ]
    }

    fn add_slice(
        mut self: core::pin::Pin<&mut Self>,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> i32 {
        let rect = Rect::new(x, y, width.max(0) as u32, height.max(0) as u32);
        let index = {
            let mut rust = self.as_mut().rust_mut();
            let slices = rust.doc.slices_mut();
            if !slices.add(rect) {
                return -1;
            }
            slices.len() as i32 - 1
        };
        self.as_mut().slices_changed();
        index
    }

    fn set_user_slice(
        mut self: core::pin::Pin<&mut Self>,
        index: i32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> bool {
        let Ok(index) = usize::try_from(index) else {
            return false;
        };
        let rect = Rect::new(x, y, width.max(0) as u32, height.max(0) as u32);
        if !self.as_mut().rust_mut().doc.slices_mut().set(index, rect) {
            return false;
        }
        self.as_mut().slices_changed();
        true
    }

    fn remove_user_slice(mut self: core::pin::Pin<&mut Self>, index: i32) -> bool {
        let Ok(index) = usize::try_from(index) else {
            return false;
        };
        if !self.as_mut().rust_mut().doc.slices_mut().remove(index) {
            return false;
        }
        self.as_mut().slices_changed();
        true
    }

    fn clear_slices(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().rust_mut().doc.slices_mut().clear();
        self.as_mut().slices_changed();
    }

    fn slice_image(&self, index: i32) -> QImage {
        let slices = self.doc.resolved_slices();
        let Some(slice) = usize::try_from(index).ok().and_then(|i| slices.get(i)) else {
            return QImage::default();
        };
        pixmap_to_qimage(self.doc.composite().crop(slice.rect))
    }

    // -- paths ----------------------------------------------------------------

    fn path_count(&self) -> i32 {
        self.doc.paths().len() as i32
    }

    fn path_name(&self, index: i32) -> QString {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.doc.paths().entries().get(i))
            .map_or_else(QString::default, |e| QString::from(e.name.as_str()))
    }

    fn active_path_index(&self) -> i32 {
        self.doc.paths().active_index().map_or(-1, |i| i as i32)
    }

    fn set_active_path_index(mut self: core::pin::Pin<&mut Self>, index: i32) -> bool {
        let Ok(index) = usize::try_from(index) else {
            return false;
        };
        if !self.as_mut().rust_mut().doc.paths_mut().set_active(index) {
            return false;
        }
        self.as_mut().paths_changed();
        true
    }

    fn add_path(mut self: core::pin::Pin<&mut Self>) -> i32 {
        let index = self.as_mut().rust_mut().doc.paths_mut().add_named() as i32;
        self.as_mut().paths_changed();
        index
    }

    fn duplicate_path(mut self: core::pin::Pin<&mut Self>, index: i32) -> i32 {
        let Ok(index) = usize::try_from(index) else {
            return -1;
        };
        let result = self
            .as_mut()
            .rust_mut()
            .doc
            .paths_mut()
            .duplicate(index)
            .map_or(-1, |i| i as i32);
        if result >= 0 {
            self.as_mut().paths_changed();
        }
        result
    }

    fn delete_path(mut self: core::pin::Pin<&mut Self>, index: i32) -> bool {
        let Ok(index) = usize::try_from(index) else {
            return false;
        };
        if !self.as_mut().rust_mut().doc.paths_mut().remove(index) {
            return false;
        }
        self.as_mut().paths_changed();
        true
    }

    fn rename_path(mut self: core::pin::Pin<&mut Self>, index: i32, name: &QString) -> bool {
        let Ok(index) = usize::try_from(index) else {
            return false;
        };
        let name = name.to_string();
        if name.is_empty() {
            return false;
        }
        if !self.as_mut().rust_mut().doc.paths_mut().rename(index, name) {
            return false;
        }
        self.as_mut().paths_changed();
        true
    }

    fn path_is_editing(&self) -> bool {
        self.doc.paths().active().is_some_and(|p| p.is_editing())
    }

    fn path_append_corner(mut self: core::pin::Pin<&mut Self>, x: f32, y: f32) {
        self.as_mut()
            .rust_mut()
            .doc
            .paths_mut()
            .ensure_active()
            .append_corner(x, y);
        self.as_mut().paths_changed();
    }

    fn path_update_last_handle(
        mut self: core::pin::Pin<&mut Self>,
        x: f32,
        y: f32,
        independent: bool,
    ) -> bool {
        let mut rust = self.as_mut().rust_mut();
        let Some(path) = rust.doc.paths_mut().active_mut() else {
            return false;
        };
        let changed = path.update_last_handle(x, y, independent);
        if changed {
            self.as_mut().paths_changed();
        }
        changed
    }

    fn path_close_active_subpath(mut self: core::pin::Pin<&mut Self>) -> bool {
        let mut rust = self.as_mut().rust_mut();
        let Some(path) = rust.doc.paths_mut().active_mut() else {
            return false;
        };
        let closed = path.close_active_subpath();
        if closed {
            self.as_mut().paths_changed();
        }
        closed
    }

    fn path_finish_editing(mut self: core::pin::Pin<&mut Self>) {
        if let Some(path) = self.as_mut().rust_mut().doc.paths_mut().active_mut() {
            path.finish_editing();
        }
        self.as_mut().paths_changed();
    }

    fn path_hit_anchor(&self, x: f32, y: f32, radius: f32) -> Vec<i32> {
        self.doc
            .paths()
            .active()
            .and_then(|p| p.hit_anchor(x, y, radius))
            .map_or_else(Vec::new, |(sp, pt)| vec![sp as i32, pt as i32])
    }

    fn path_hit_handle(&self, x: f32, y: f32, radius: f32) -> Vec<i32> {
        self.doc
            .paths()
            .active()
            .and_then(|p| p.hit_handle(x, y, radius))
            .map_or_else(Vec::new, |(sp, pt, side)| {
                let side = if side == crate::path::HandleSide::In { 0 } else { 1 };
                vec![sp as i32, pt as i32, side]
            })
    }

    fn path_hit_segment(&self, x: f32, y: f32, radius: f32) -> Vec<f32> {
        self.doc
            .paths()
            .active()
            .and_then(|p| p.hit_segment(x, y, radius))
            .map_or_else(Vec::new, |(sp, seg, t)| vec![sp as f32, seg as f32, t])
    }

    fn path_hit_subpath(&self, x: f32, y: f32, radius: f32) -> i32 {
        self.doc
            .paths()
            .active()
            .and_then(|p| p.hit_subpath(x, y, radius))
            .map_or(-1, |i| i as i32)
    }

    fn path_move_anchor(mut self: core::pin::Pin<&mut Self>, sp: i32, pt: i32, x: f32, y: f32) -> bool {
        let (Ok(sp), Ok(pt)) = (usize::try_from(sp), usize::try_from(pt)) else {
            return false;
        };
        let mut rust = self.as_mut().rust_mut();
        let Some(path) = rust.doc.paths_mut().active_mut() else {
            return false;
        };
        let moved = path.move_anchor(sp, pt, x, y);
        if moved {
            self.as_mut().paths_changed();
        }
        moved
    }

    #[allow(clippy::too_many_arguments)]
    fn path_move_handle(
        mut self: core::pin::Pin<&mut Self>,
        sp: i32,
        pt: i32,
        side: i32,
        x: f32,
        y: f32,
        independent: bool,
    ) -> bool {
        let (Ok(sp), Ok(pt)) = (usize::try_from(sp), usize::try_from(pt)) else {
            return false;
        };
        let side = if side == 0 { crate::path::HandleSide::In } else { crate::path::HandleSide::Out };
        let mut rust = self.as_mut().rust_mut();
        let Some(path) = rust.doc.paths_mut().active_mut() else {
            return false;
        };
        let moved = path.move_handle(sp, pt, side, x, y, independent);
        if moved {
            self.as_mut().paths_changed();
        }
        moved
    }

    fn path_set_corner(mut self: core::pin::Pin<&mut Self>, sp: i32, pt: i32) -> bool {
        let (Ok(sp), Ok(pt)) = (usize::try_from(sp), usize::try_from(pt)) else {
            return false;
        };
        let mut rust = self.as_mut().rust_mut();
        let Some(path) = rust.doc.paths_mut().active_mut() else {
            return false;
        };
        let changed = path.set_corner(sp, pt);
        if changed {
            self.as_mut().paths_changed();
        }
        changed
    }

    fn path_drag_new_handles(mut self: core::pin::Pin<&mut Self>, sp: i32, pt: i32, x: f32, y: f32) -> bool {
        let (Ok(sp), Ok(pt)) = (usize::try_from(sp), usize::try_from(pt)) else {
            return false;
        };
        let mut rust = self.as_mut().rust_mut();
        let Some(path) = rust.doc.paths_mut().active_mut() else {
            return false;
        };
        let changed = path.drag_new_handles(sp, pt, x, y);
        if changed {
            self.as_mut().paths_changed();
        }
        changed
    }

    fn path_insert_anchor(mut self: core::pin::Pin<&mut Self>, sp: i32, seg: i32, t: f32) -> bool {
        let (Ok(sp), Ok(seg)) = (usize::try_from(sp), usize::try_from(seg)) else {
            return false;
        };
        let mut rust = self.as_mut().rust_mut();
        let Some(path) = rust.doc.paths_mut().active_mut() else {
            return false;
        };
        let changed = path.insert_anchor(sp, seg, t);
        if changed {
            self.as_mut().paths_changed();
        }
        changed
    }

    fn path_delete_anchor(mut self: core::pin::Pin<&mut Self>, sp: i32, pt: i32) -> bool {
        let (Ok(sp), Ok(pt)) = (usize::try_from(sp), usize::try_from(pt)) else {
            return false;
        };
        let mut rust = self.as_mut().rust_mut();
        let Some(path) = rust.doc.paths_mut().active_mut() else {
            return false;
        };
        let changed = path.delete_anchor(sp, pt);
        if changed {
            self.as_mut().paths_changed();
        }
        changed
    }

    fn path_move_subpath(mut self: core::pin::Pin<&mut Self>, sp: i32, dx: f32, dy: f32) -> bool {
        let Ok(sp) = usize::try_from(sp) else {
            return false;
        };
        let mut rust = self.as_mut().rust_mut();
        let Some(path) = rust.doc.paths_mut().active_mut() else {
            return false;
        };
        let moved = path.move_subpath(sp, dx, dy);
        if moved {
            self.as_mut().paths_changed();
        }
        moved
    }

    fn path_subpath_count(&self) -> i32 {
        self.doc.paths().active().map_or(0, |p| p.subpaths.len() as i32)
    }

    fn path_is_closed(&self, sp: i32) -> bool {
        usize::try_from(sp)
            .ok()
            .and_then(|sp| self.doc.paths().active()?.subpaths.get(sp))
            .is_some_and(|s| s.closed)
    }

    fn path_anchor_count(&self, sp: i32) -> i32 {
        usize::try_from(sp)
            .ok()
            .and_then(|sp| self.doc.paths().active()?.subpaths.get(sp))
            .map_or(0, |s| s.points.len() as i32)
    }

    fn path_anchor_at(&self, sp: i32, pt: i32) -> Vec<f32> {
        let (Ok(sp), Ok(pt)) = (usize::try_from(sp), usize::try_from(pt)) else {
            return Vec::new();
        };
        let Some(point) = self
            .doc
            .paths()
            .active()
            .and_then(|p| p.subpaths.get(sp))
            .and_then(|s| s.points.get(pt))
        else {
            return Vec::new();
        };
        let (in_flag, in_x, in_y) = point.in_handle.map_or((0.0, 0.0, 0.0), |h| (1.0, h.0, h.1));
        let (out_flag, out_x, out_y) =
            point.out_handle.map_or((0.0, 0.0, 0.0), |h| (1.0, h.0, h.1));
        vec![
            point.anchor.0,
            point.anchor.1,
            if point.smooth { 1.0 } else { 0.0 },
            in_flag,
            in_x,
            in_y,
            out_flag,
            out_x,
            out_y,
        ]
    }

    fn path_add_freeform_subpath(
        mut self: core::pin::Pin<&mut Self>,
        points: &ffi::QVector_f32,
        tolerance: f32,
        close: bool,
    ) -> bool {
        let pairs: Vec<(f32, f32)> = points
            .iter()
            .copied()
            .collect::<Vec<f32>>()
            .chunks_exact(2)
            .map(|p| (p[0], p[1]))
            .collect();
        let added = self
            .as_mut()
            .rust_mut()
            .doc
            .add_freeform_subpath(&pairs, tolerance.max(0.1), close);
        if added {
            self.as_mut().paths_changed();
        }
        added
    }

    fn path_make_selection(mut self: core::pin::Pin<&mut Self>, op: i32, feather: i32) -> bool {
        let op = SelectionOp::from_i32(op);
        let feather = feather.clamp(0, 1000) as u32;
        let made = self
            .as_mut()
            .rust_mut()
            .doc
            .select_from_active_path(op, feather);
        if made {
            self.as_mut().selection_changed();
            self.as_mut().canvas_changed();
        }
        made
    }

    fn path_fill(mut self: core::pin::Pin<&mut Self>) -> bool {
        let color = self.foreground;
        let dirty = self.as_mut().rust_mut().doc.fill_active_path(color, 1.0);
        if dirty.is_empty() {
            return false;
        }
        self.sync();
        true
    }

    fn path_stroke(mut self: core::pin::Pin<&mut Self>) -> bool {
        let brush = self.brush;
        let color = self.paint_color();
        let dirty = self
            .as_mut()
            .rust_mut()
            .doc
            .stroke_active_path(&brush, color, brush.opacity);
        if dirty.is_empty() {
            return false;
        }
        self.sync();
        true
    }

    /// Open `doc` in a new tab, at the end, and make it active.
    fn add_document(mut self: core::pin::Pin<&mut Self>, doc: Document) {
        {
            let mut rust = self.as_mut().rust_mut();
            let previous = rust.active;
            let current = std::mem::replace(&mut rust.doc, doc);
            rust.shelf.insert(previous, current);
            rust.active = rust.shelf.len();
        }
        self.as_mut().documents_changed();
    }

    fn document_count(&self) -> i32 {
        self.tab_count() as i32
    }

    fn active_document(&self) -> i32 {
        self.active as i32
    }

    fn document_title_at(&self, index: i32) -> QString {
        match self.document_at(index) {
            Some(doc) => QString::from(doc.display_name().as_str()),
            None => QString::default(),
        }
    }

    fn document_modified_at(&self, index: i32) -> bool {
        self.document_at(index).is_some_and(|doc| doc.is_dirty())
    }

    fn set_active_document(mut self: core::pin::Pin<&mut Self>, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        if index >= self.tab_count() || index == self.active {
            return;
        }

        {
            let mut rust = self.as_mut().rust_mut();
            let previous = rust.active;
            // Put the active document back in its slot, then lift out the one
            // being switched to. Doing it in this order keeps the tab order
            // stable: every other document stays where it was.
            let doc = std::mem::replace(&mut rust.doc, Document::new(1, 1, Rgba8::WHITE));
            rust.shelf.insert(previous, doc);
            rust.doc = rust.shelf.remove(index);
            rust.active = index;
        }

        // A different document means different pixels, layers, history and
        // selection, so this is the same broadcast as opening a file.
        self.as_mut().documents_changed();
        self.sync();
    }

    fn close_document(mut self: core::pin::Pin<&mut Self>, index: i32) -> bool {
        let Ok(index) = usize::try_from(index) else {
            return false;
        };
        if index >= self.tab_count() || self.shelf.is_empty() {
            // The last document stays open: the rest of the interface assumes
            // there is always one to act on.
            return false;
        }

        if index == self.active {
            // Closing the active tab moves to its neighbour, preferring the one
            // to the left as Photoshop does.
            let next = if index > 0 { index - 1 } else { 0 };
            let mut rust = self.as_mut().rust_mut();
            rust.doc = rust.shelf.remove(next);
            rust.active = next;
        } else {
            let mut rust = self.as_mut().rust_mut();
            let shelf_index = if index < rust.active { index } else { index - 1 };
            rust.shelf.remove(shelf_index);
            if index < rust.active {
                rust.active -= 1;
            }
        }

        self.as_mut().documents_changed();
        self.sync();
        true
    }

    fn document_size_bytes(&self) -> Vec<f64> {
        let (w, h) = self.doc.size();
        // Flattened is one RGBA8 buffer; the layered figure is what the stack
        // actually holds, masks included.
        let flat = (w as f64) * (h as f64) * 4.0;
        vec![flat, self.doc.layers().byte_size() as f64]
    }

    fn color_mode(&self) -> i32 {
        self.doc.color_mode().to_index()
    }

    fn set_color_mode(mut self: core::pin::Pin<&mut Self>, mode: i32) {
        if let Some(m) = ImageMode::from_index(mode) {
            self.as_mut().rust_mut().doc.set_color_mode(m);
            self.sync();
        }
    }

    fn convert_to_indexed(mut self: core::pin::Pin<&mut Self>, max_colors: i32, dither_amount: i32) {
        let colors = max_colors.clamp(2, 256) as u32;
        let dither = dither_amount.clamp(0, 100) as u32;
        self.as_mut().rust_mut().doc.convert_to_indexed(colors, dither);
        self.sync();
    }

    fn bit_depth(&self) -> i32 {
        self.doc.bit_depth() as i32
    }

    fn set_bit_depth(mut self: core::pin::Pin<&mut Self>, depth: i32) {
        let d = match depth {
            8 | 16 | 32 => depth as u8,
            _ => return,
        };
        self.as_mut().rust_mut().doc.set_bit_depth(d);
        self.sync();
    }

    fn resize_canvas(mut self: core::pin::Pin<&mut Self>, width: i32, height: i32) {
        let w = width.clamp(1, 30_000) as u32;
        let h = height.clamp(1, 30_000) as u32;
        self.as_mut().rust_mut().doc.resize_canvas(w, h);
        self.sync();
    }

    fn perspective_crop(mut self: core::pin::Pin<&mut Self>, corners: &ffi::QVector_f32) -> bool {
        if corners.len() != 8 {
            return false;
        }
        let at = |i| corners.get(i).copied().unwrap_or(0.0);
        let quad = [
            (at(0), at(1)),
            (at(2), at(3)),
            (at(4), at(5)),
            (at(6), at(7)),
        ];

        if !self.as_mut().rust_mut().doc.perspective_crop(&quad) {
            return false;
        }
        self.sync();
        true
    }

    fn crop_to(
        mut self: core::pin::Pin<&mut Self>,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        delete_cropped: bool,
    ) {
        let rect = Rect::new(x, y, width.max(0) as u32, height.max(0) as u32);
        self.as_mut().rust_mut().doc.crop(rect, delete_cropped);
        // `sync` covers the rest: the size properties, the repaint, and the
        // selection re-trace the crop needs because the ants moved with the
        // canvas.
        self.sync();
    }

    // -- layers -------------------------------------------------------------

    fn layer_name(&self, index: i32) -> QString {
        self.layer_id_at(index)
            .and_then(|id| self.doc.layers().by_id(id))
            .map_or_else(QString::default, |l| QString::from(l.name.as_str()))
    }

    fn layer_visible(&self, index: i32) -> bool {
        let Some(id) = self.layer_id_at(index) else {
            return false;
        };
        // A type layer open in the Type tool has its pixels held back for the
        // duration (see `beginTextEdit`), but that is the tool's business and
        // not a change the user made: the panel goes on showing the eye the way
        // they left it.
        if let Some((editing, was_visible)) = self.doc.text_edit_layer() {
            if editing == id {
                return was_visible;
            }
        }
        self.doc.layers().by_id(id).is_some_and(|l| l.visible)
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

    fn layer_kind(&self, index: i32) -> i32 {
        self.layer_id_at(index)
            .and_then(|id| self.doc.layers().by_id(id))
            .map_or(0, |l| match l.kind {
                LayerKind::Adjustment(_) => 1,
                // A type layer is raster underneath — it carries the pixels its
                // text was rendered to — so what makes it type is the record
                // beside them, not its kind.
                _ if l.text.is_some() => 2,
                _ => 0,
            })
    }

    fn layer_lock_transparency(&self, index: i32) -> bool {
        self.layer_id_at(index)
            .and_then(|id| self.doc.layers().by_id(id))
            .is_some_and(|l| l.lock_transparency)
    }

    fn layer_lock_pixels(&self, index: i32) -> bool {
        self.layer_id_at(index)
            .and_then(|id| self.doc.layers().by_id(id))
            .is_some_and(|l| l.lock_pixels)
    }

    fn layer_lock_position(&self, index: i32) -> bool {
        self.layer_id_at(index)
            .and_then(|id| self.doc.layers().by_id(id))
            .is_some_and(|l| l.lock_position)
    }

    fn layer_is_locked(&self, index: i32) -> bool {
        self.layer_id_at(index)
            .and_then(|id| self.doc.layers().by_id(id))
            .is_some_and(|l| l.is_locked())
    }

    fn layer_is_fully_locked(&self, index: i32) -> bool {
        self.layer_id_at(index)
            .and_then(|id| self.doc.layers().by_id(id))
            .is_some_and(|l| l.is_fully_locked())
    }

    fn active_layer_is_locked(&self) -> bool {
        // Lock Position does not stop a brush, and Lock Transparency only
        // constrains one — neither refuses a stroke, so neither belongs here.
        self.doc.active_layer().is_some_and(|l| l.lock_pixels)
    }

    fn layer_thumbnail(&self, index: i32, size: i32) -> QImage {
        let size = size.clamp(1, 512);
        let Some(layer) = self
            .layer_id_at(index)
            .and_then(|id| self.doc.layers().by_id(id))
        else {
            return QImage::default();
        };

        // A fill layer — a shape layer is one — has no pixels of its own: it is
        // a colour the compositor pours through the layer's mask. So the
        // thumbnail is built the same way, or the panel would show a shape
        // layer as an empty row.
        let solid = match layer.kind {
            LayerKind::SolidColor(color) => Some(color),
            _ => None,
        };

        let (sw, sh) = match (solid, layer.mask.as_ref()) {
            (Some(_), Some(mask)) => (mask.width(), mask.height()),
            (Some(_), None) => (self.doc.width(), self.doc.height()),
            _ => (layer.pixels.width(), layer.pixels.height()),
        };
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
                let px = match solid {
                    Some(color) => {
                        let coverage = layer
                            .mask
                            .as_ref()
                            .map_or(255, |mask| mask.get(sx, sy).a);
                        Rgba8::new(
                            color.r,
                            color.g,
                            color.b,
                            ((color.a as u32 * coverage as u32) / 255) as u8,
                        )
                    }
                    None => layer.pixels.get(sx, sy),
                };
                thumb.set(x as i32, y as i32, px);
            }
        }
        pixmap_to_qimage(thumb)
    }

    fn layer_image(&self, index: i32) -> QImage {
        let Some(layer) = self
            .layer_id_at(index)
            .and_then(|id| self.doc.layers().by_id(id))
        else {
            return QImage::default();
        };
        pixmap_to_qimage(layer.pixels.clone())
    }

    fn layer_content_bounds(&self, index: i32) -> QRect {
        let Some(layer) = self
            .layer_id_at(index)
            .and_then(|id| self.doc.layers().by_id(id))
        else {
            return QRect::new(0, 0, 0, 0);
        };
        let w = layer.pixels.width() as i32;
        let h = layer.pixels.height() as i32;
        if w == 0 || h == 0 {
            return QRect::new(0, 0, 0, 0);
        }
        let mut min_x = w;
        let mut min_y = h;
        let mut max_x = 0i32;
        let mut max_y = 0i32;
        for y in 0..h {
            for x in 0..w {
                if layer.pixels.get(x, y).a > 0 {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }
        if max_x < min_x {
            return QRect::new(0, 0, 0, 0);
        }
        QRect::new(
            layer.offset.0 + min_x,
            layer.offset.1 + min_y,
            max_x - min_x + 1,
            max_y - min_y + 1,
        )
    }

    fn layer_offset_x(&self, index: i32) -> i32 {
        self.layer_id_at(index)
            .and_then(|id| self.doc.layers().by_id(id))
            .map_or(0, |l| l.offset.0)
    }

    fn layer_offset_y(&self, index: i32) -> i32 {
        self.layer_id_at(index)
            .and_then(|id| self.doc.layers().by_id(id))
            .map_or(0, |l| l.offset.1)
    }

    fn replace_layer_pixels(
        mut self: core::pin::Pin<&mut Self>,
        index: i32,
        image: &QImage,
        x: i32,
        y: i32,
    ) {
        let Some(id) = self.layer_id_at(index) else {
            return;
        };
        let Some(pixels) = qimage_to_pixmap(image) else {
            return;
        };
        if let Some(layer) = self.as_mut().rust_mut().doc.layers_mut_raw().by_id_mut(id) {
            layer.pixels = pixels;
            layer.offset = (x, y);
        }
        self.as_mut().rust_mut().doc.commit("Free Transform");
        self.sync();
    }

    fn rotate_layer(mut self: core::pin::Pin<&mut Self>, degrees: i32) {
        use crate::metadata::Orientation;
        let orient = match degrees.rem_euclid(360) {
            90 => Orientation::Rotate90Cw,
            180 => Orientation::Rotate180,
            270 => Orientation::Rotate90Ccw,
            _ => return,
        };
        let id = self.doc.active_layer_id();
        let (cw, ch) = self.doc.size();
        let cw = cw as i32;
        let ch = ch as i32;
        if let Some(layer) = self.as_mut().rust_mut().doc.layers_mut_raw().by_id_mut(id) {
            let (ox, oy) = layer.offset;
            let lw = layer.pixels.width() as i32;
            let lh = layer.pixels.height() as i32;
            layer.pixels = layer.pixels.transformed(orient);
            match degrees.rem_euclid(360) {
                90 => layer.offset = (cw - oy - lh, ox),
                180 => layer.offset = (cw - ox - lw, ch - oy - lh),
                270 => layer.offset = (oy, ch - ox - lw),
                _ => {}
            }
        }
        let label = match degrees.rem_euclid(360) {
            90 => "Rotate 90\u{b0} CW",
            180 => "Rotate 180\u{b0}",
            270 => "Rotate 90\u{b0} CCW",
            _ => "Rotate",
        };
        self.as_mut().rust_mut().doc.commit(label);
        self.sync();
    }

    fn flip_layer(mut self: core::pin::Pin<&mut Self>, horizontal: bool) {
        use crate::metadata::Orientation;
        let id = self.doc.active_layer_id();
        let (cw, ch) = self.doc.size();
        let cw = cw as i32;
        let ch = ch as i32;
        if let Some(layer) = self.as_mut().rust_mut().doc.layers_mut_raw().by_id_mut(id) {
            let (ox, oy) = layer.offset;
            let lw = layer.pixels.width() as i32;
            let lh = layer.pixels.height() as i32;
            if horizontal {
                layer.pixels = layer.pixels.transformed(Orientation::FlipHorizontal);
                layer.offset = (cw - ox - lw, oy);
            } else {
                layer.pixels = layer.pixels.transformed(Orientation::FlipVertical);
                layer.offset = (ox, ch - oy - lh);
            }
        }
        let label = if horizontal { "Flip Horizontal" } else { "Flip Vertical" };
        self.as_mut().rust_mut().doc.commit(label);
        self.sync();
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

    fn set_layer_locks(
        mut self: core::pin::Pin<&mut Self>,
        index: i32,
        transparency: bool,
        pixels: bool,
        position: bool,
    ) {
        if let Some(id) = self.layer_id_at(index) {
            self.as_mut()
                .rust_mut()
                .doc
                .set_layer_locks(id, transparency, pixels, position);
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

    fn seal_history(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().rust_mut().doc.seal_history();
    }

    fn rasterize_layer(mut self: core::pin::Pin<&mut Self>, index: i32) {
        if let Some(id) = self.layer_id_at(index) {
            self.as_mut().rust_mut().doc.rasterize_type(id);
            self.sync();
        }
    }

    fn add_image_layer(
        mut self: core::pin::Pin<&mut Self>,
        image: &QImage,
        x: i32,
        y: i32,
        name: &QString,
    ) -> bool {
        let Some(pixels) = qimage_to_pixmap(image) else {
            return false;
        };
        self.as_mut()
            .rust_mut()
            .doc
            .add_image_layer(pixels, (x, y), name.to_string());
        self.sync();
        true
    }

    fn begin_text_runs(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().rust_mut().pending_runs.clear();
    }

    fn add_text_run(
        mut self: core::pin::Pin<&mut Self>,
        text: &QString,
        family: &QString,
        style: &QString,
        size: f32,
        color: &QColor,
    ) {
        let run = TextRun {
            text: text.to_string(),
            family: family.to_string(),
            style: style.to_string(),
            size,
            color: qcolor_to_rgba(color),
        };
        self.as_mut().rust_mut().pending_runs.push(run);
    }

    fn add_text_layer(
        mut self: core::pin::Pin<&mut Self>,
        image: &QImage,
        x: i32,
        y: i32,
        name: &QString,
        align: i32,
        antialias: bool,
        vertical: bool,
        origin_x: f32,
        origin_y: f32,
    ) -> bool {
        let Some(content) = self
            .as_mut()
            .rust_mut()
            .take_text_content(align, antialias, vertical, origin_x, origin_y)
        else {
            return false;
        };
        let Some(pixels) = qimage_to_pixmap(image) else {
            return false;
        };
        self.as_mut()
            .rust_mut()
            .doc
            .add_text_layer(pixels, (x, y), name.to_string(), content);
        self.sync();
        true
    }

    fn update_text_layer(
        mut self: core::pin::Pin<&mut Self>,
        index: i32,
        image: &QImage,
        x: i32,
        y: i32,
        name: &QString,
        align: i32,
        antialias: bool,
        vertical: bool,
        origin_x: f32,
        origin_y: f32,
    ) -> bool {
        let Some(content) = self
            .as_mut()
            .rust_mut()
            .take_text_content(align, antialias, vertical, origin_x, origin_y)
        else {
            return false;
        };
        let Some(id) = self.layer_id_at(index) else {
            return false;
        };
        let Some(pixels) = qimage_to_pixmap(image) else {
            return false;
        };
        // The edit is over either way, so the layer gets its pixels back before
        // they are replaced — otherwise a failed update would leave it hidden.
        self.as_mut().rust_mut().doc.end_text_edit();
        let updated = self
            .as_mut()
            .rust_mut()
            .doc
            .update_text_layer(id, pixels, (x, y), name.to_string(), content);
        self.sync();
        updated
    }

    fn select_from_alpha(
        mut self: core::pin::Pin<&mut Self>,
        image: &QImage,
        x: i32,
        y: i32,
        op: i32,
    ) -> bool {
        let (width, height) = (self.doc.width(), self.doc.height());
        let (image_width, image_height) = (image.width(), image.height());
        if image_width <= 0 || image_height <= 0 {
            return false;
        }

        // The selection is canvas-sized, so the image is stamped into a blank
        // coverage map at its offset rather than combined where it lies.
        let mut coverage = vec![0u8; (width * height) as usize];
        for iy in 0..image_height {
            let doc_y = y + iy;
            if doc_y < 0 || doc_y >= height as i32 {
                continue;
            }
            for ix in 0..image_width {
                let doc_x = x + ix;
                if doc_x < 0 || doc_x >= width as i32 {
                    continue;
                }
                let alpha = image.pixel_color(ix, iy).alpha().clamp(0, 255) as u8;
                coverage[doc_y as usize * width as usize + doc_x as usize] = alpha;
            }
        }

        let op = SelectionOp::from_i32(op);
        self.as_mut().rust_mut().doc.select_mask(&coverage, op, 0);
        self.as_mut().selection_changed();
        self.as_mut().canvas_changed();
        true
    }

    fn text_layer_at(&self, x: i32, y: i32) -> i32 {
        self.doc
            .text_layer_at(x, y)
            .map_or(-1, |id| self.panel_index_of(id))
    }

    fn layer_text_run_count(&self, index: i32) -> i32 {
        self.text_content_at(index)
            .map_or(0, |t| t.runs.len() as i32)
    }

    fn layer_text_run_text(&self, index: i32, run: i32) -> QString {
        self.text_run_at(index, run)
            .map_or_else(QString::default, |r| QString::from(r.text.as_str()))
    }

    fn layer_text_run_family(&self, index: i32, run: i32) -> QString {
        self.text_run_at(index, run)
            .map_or_else(QString::default, |r| QString::from(r.family.as_str()))
    }

    fn layer_text_run_style(&self, index: i32, run: i32) -> QString {
        self.text_run_at(index, run)
            .map_or_else(QString::default, |r| QString::from(r.style.as_str()))
    }

    fn layer_text_run_size(&self, index: i32, run: i32) -> f32 {
        self.text_run_at(index, run).map_or(0.0, |r| r.size)
    }

    fn layer_text_run_color(&self, index: i32, run: i32) -> QColor {
        rgba_to_qcolor(self.text_run_at(index, run).map_or(Rgba8::BLACK, |r| r.color))
    }

    fn layer_text_align(&self, index: i32) -> i32 {
        self.text_content_at(index)
            .map_or(0, |t| match t.align {
                TextAlign::Left => 0,
                TextAlign::Center => 1,
                TextAlign::Right => 2,
            })
    }

    fn layer_text_antialias(&self, index: i32) -> bool {
        self.text_content_at(index).is_some_and(|t| t.antialias)
    }

    fn layer_text_vertical(&self, index: i32) -> bool {
        self.text_content_at(index).is_some_and(|t| t.vertical)
    }

    fn layer_text_origin_x(&self, index: i32) -> f32 {
        self.text_content_at(index).map_or(0.0, |t| t.origin.0)
    }

    fn layer_text_origin_y(&self, index: i32) -> f32 {
        self.text_content_at(index).map_or(0.0, |t| t.origin.1)
    }

    fn begin_text_edit(mut self: core::pin::Pin<&mut Self>, index: i32) -> bool {
        let Some(id) = self.layer_id_at(index) else {
            return false;
        };
        let started = self.as_mut().rust_mut().doc.begin_text_edit(id);
        self.sync();
        started
    }

    fn end_text_edit(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().rust_mut().doc.end_text_edit();
        self.sync();
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
        // Tip shape and scattering are set separately, so a size or opacity
        // change does not silently reset the chosen preset's character.
        let mut rust = self.as_mut().rust_mut();
        rust.brush.size = size.clamp(1.0, 5000.0);
        rust.brush.hardness = hardness.clamp(0, 100) as f32 / 100.0;
        rust.brush.opacity = opacity.clamp(0, 100) as f32 / 100.0;
        rust.brush.flow = flow.clamp(0, 100) as f32 / 100.0;
        rust.brush.spacing = spacing.clamp(1, 1000) as f32 / 100.0;
    }

    fn set_brush_shape(
        mut self: core::pin::Pin<&mut Self>,
        roundness: i32,
        angle: i32,
        scatter: i32,
        count: i32,
        size_jitter: i32,
        angle_jitter: i32,
        roundness_jitter: i32,
    ) {
        let mut rust = self.as_mut().rust_mut();
        rust.brush.roundness = (roundness.clamp(5, 100) as f32) / 100.0;
        rust.brush.angle = angle as f32;
        rust.brush.scatter = (scatter.clamp(0, 1000) as f32) / 100.0;
        rust.brush.count = count.clamp(1, 16) as u32;
        rust.brush.size_jitter = (size_jitter.clamp(0, 100) as f32) / 100.0;
        rust.brush.angle_jitter = angle_jitter.clamp(0, 180) as f32;
        rust.brush.roundness_jitter = (roundness_jitter.clamp(0, 100) as f32) / 100.0;
    }

    fn brush_preview(&self, width: i32, height: i32) -> QImage {
        let w = width.clamp(1, 512) as u32;
        let h = height.clamp(1, 512) as u32;

        // Render through the real stroke machinery, so a thumbnail is exactly
        // what the brush deposits rather than an approximation drawn twice.
        let mut mask = StrokeMask::new(w, h);
        let mut brush = self.brush;
        brush.opacity = 1.0;
        brush.flow = 1.0;
        // Fit the tip inside the thumbnail, keeping scatter proportional.
        let fit = (w.min(h) as f32 - 4.0).max(1.0);
        if brush.size + brush.size * brush.scatter * 2.0 > fit {
            brush.size = (fit / (1.0 + brush.scatter * 2.0)).max(1.0);
        }
        mask.begin(&brush, w as f32 / 2.0, h as f32 / 2.0, 1.0);

        let mut pm = Pixmap::new(w, h);
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let a = mask.coverage_at(x, y);
                if a > 0.0 {
                    let v = (a * 255.0 + 0.5) as u8;
                    pm.set(x, y, Rgba8::new(240, 240, 240, v));
                }
            }
        }
        pixmap_to_qimage(pm)
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
        // Auto Erase is decided once, from the pixel the stroke starts on: begin
        // on the foreground colour and the whole stroke paints the background
        // instead. Photoshop's Pencil works exactly this way, which is what makes
        // it usable for touching up 1px lines.
        let erase = self.auto_erase && {
            let under = self.doc.composite().get(x as i32, y as i32);
            let fg = self.foreground;
            under.r == fg.r && under.g == fg.g && under.b == fg.b && under.a == fg.a
        };
        self.as_mut().rust_mut().auto_erase_active = erase;

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
        // The Spot Healing Brush uses the same stroke machinery but a different
        // ending: the covered region is reconstructed rather than filled.
        if let Some(mode) = self.heal_mode {
            // With a source set this is the Healing Brush: transplant from
            // there. Without one it is the Spot Healing Brush, which works out
            // what belongs from the surroundings alone.
            match self.heal_source {
                Some((dx, dy)) => {
                    self.as_mut().rust_mut().doc.end_heal_clone_stroke(dx, dy);
                }
                None => {
                    self.as_mut().rust_mut().doc.end_heal_stroke(mode);
                }
            }
            self.sync();
            return;
        }
        let opacity = self.brush.opacity;
        // A clone stroke copies the snapshot it took when it began instead of
        // filling with a colour. Everything up to here — dabs, spacing, flow —
        // was the same stroke machinery.
        if self.doc.is_cloning() {
            self.as_mut().rust_mut().doc.end_clone_stroke(opacity);
            self.sync();
            return;
        }
        let color = self.paint_color();
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

    fn set_brush_antialias(mut self: core::pin::Pin<&mut Self>, antialias: bool) {
        self.as_mut().rust_mut().brush.antialias = antialias;
    }

    fn set_auto_erase(mut self: core::pin::Pin<&mut Self>, enabled: bool) {
        self.as_mut().rust_mut().auto_erase = enabled;
    }

    fn set_replace_options(
        mut self: core::pin::Pin<&mut Self>,
        mode: i32,
        sampling: i32,
        limits: i32,
        tolerance: i32,
        antialias: bool,
    ) {
        self.as_mut().rust_mut().replace_options = ReplaceOptions {
            mode: ReplaceMode::from_i32(mode),
            sampling: ReplaceSampling::from_i32(sampling),
            limits: ReplaceLimits::from_i32(limits),
            tolerance: tolerance.clamp(0, 255) as u32,
            antialias,
        };
    }

    fn set_background_erase_options(
        mut self: core::pin::Pin<&mut Self>,
        sampling: i32,
        limits: i32,
        tolerance_percent: i32,
        protect_foreground: bool,
    ) {
        self.as_mut().rust_mut().bg_erase_options = BackgroundEraseOptions {
            sampling: Sampling::from_i32(sampling),
            limits: Limits::from_i32(limits),
            // CS6's bar is a percentage of the channel range.
            tolerance: (tolerance_percent.clamp(0, 100) * 255 / 100) as u32,
            protect_foreground,
        };
    }

    fn begin_background_erase(
        mut self: core::pin::Pin<&mut Self>,
        x: f32,
        y: f32,
        pressure: f32,
    ) -> bool {
        let brush = self.brush;
        let options = self.bg_erase_options;
        // Once reads the pixel the stroke starts on; Background Swatch uses the
        // swatch itself and samples nothing.
        let reference = match options.sampling {
            Sampling::Once => Some(self.doc.composite().get(x as i32, y as i32)),
            Sampling::BackgroundSwatch => Some(self.background),
            Sampling::Continuous => None,
        };
        let foreground = self.foreground;
        let started = self.as_mut().rust_mut().doc.begin_background_erase(
            &brush, options, reference, foreground, x, y, pressure,
        );
        if started {
            self.as_mut().canvas_changed();
        }
        started
    }

    fn extend_background_erase(mut self: core::pin::Pin<&mut Self>, x: f32, y: f32, pressure: f32) {
        let brush = self.brush;
        let foreground = self.foreground;
        let dirty = self
            .as_mut()
            .rust_mut()
            .doc
            .extend_background_erase(&brush, x, y, pressure, foreground);
        if !dirty.is_empty() {
            self.as_mut().canvas_changed();
        }
    }

    fn end_background_erase(mut self: core::pin::Pin<&mut Self>) {
        if self.as_mut().rust_mut().doc.end_background_erase() {
            self.sync();
        }
    }

    fn cancel_background_erase(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().rust_mut().doc.cancel_background_erase();
        self.as_mut().canvas_changed();
    }

    #[allow(clippy::too_many_arguments)]
    fn magic_erase(
        mut self: core::pin::Pin<&mut Self>,
        x: i32,
        y: i32,
        tolerance: i32,
        contiguous: bool,
        antialias: bool,
        sample_all: bool,
        opacity: i32,
    ) -> bool {
        let dirty = self.as_mut().rust_mut().doc.magic_erase(
            x,
            y,
            tolerance.clamp(0, 255) as u32,
            contiguous,
            antialias,
            sample_all,
            (opacity.clamp(0, 100) as f32) / 100.0,
        );
        if dirty.is_empty() {
            return false;
        }
        self.sync();
        true
    }

    fn begin_replace(mut self: core::pin::Pin<&mut Self>, x: f32, y: f32, pressure: f32) -> bool {
        let brush = self.brush;
        let options = self.replace_options;
        // Once sampling reads the pixel the stroke starts on; Background Swatch
        // uses the swatch itself and samples nothing.
        let reference = match options.sampling {
            ReplaceSampling::Once => Some(self.doc.composite().get(x as i32, y as i32)),
            ReplaceSampling::BackgroundSwatch => Some(self.background),
            ReplaceSampling::Continuous => None,
        };
        let replacement = self.foreground;
        let started = self.as_mut().rust_mut().doc.begin_replace(
            &brush, options, reference, replacement, x, y, pressure,
        );
        if started {
            self.as_mut().canvas_changed();
        }
        started
    }

    fn extend_replace(mut self: core::pin::Pin<&mut Self>, x: f32, y: f32, pressure: f32) {
        let brush = self.brush;
        let replacement = self.foreground;
        let dirty = self
            .as_mut()
            .rust_mut()
            .doc
            .extend_replace(&brush, x, y, pressure, replacement);
        if !dirty.is_empty() {
            self.as_mut().canvas_changed();
        }
    }

    fn end_replace(mut self: core::pin::Pin<&mut Self>) {
        if self.as_mut().rust_mut().doc.end_replace() {
            self.sync();
        }
    }

    fn cancel_replace(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().rust_mut().doc.cancel_replace();
        self.sync();
    }

    fn set_mixer_options(
        mut self: core::pin::Pin<&mut Self>,
        wet: i32,
        load: i32,
        mix: i32,
        flow: i32,
        sample_all_layers: bool,
        load_after_stroke: bool,
        clean_after_stroke: bool,
    ) {
        let percent = |v: i32| v.clamp(0, 100) as f32 / 100.0;
        let mut rust = self.as_mut().rust_mut();
        rust.mixer_options = MixerOptions {
            wet: percent(wet),
            load: percent(load),
            mix: percent(mix),
            flow: percent(flow),
            sample_all_layers,
            // Set from the layer when the stroke begins: the transparency lock
            // belongs to the layer, not to the options bar.
            preserve_alpha: false,
        };
        rust.mixer_load_after_stroke = load_after_stroke;
        rust.mixer_clean_after_stroke = clean_after_stroke;
    }

    fn load_mixer_brush(mut self: core::pin::Pin<&mut Self>) {
        let colour = self.foreground;
        self.as_mut().rust_mut().mixer_reservoir = colour;
    }

    fn load_mixer_brush_from(mut self: core::pin::Pin<&mut Self>, x: i32, y: i32) {
        // The composite, not the active layer: what the user is aiming at is
        // what they see.
        let colour = self.doc.composite().get(x, y);
        self.as_mut().rust_mut().mixer_reservoir = colour;
    }

    fn clean_mixer_brush(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().rust_mut().mixer_reservoir = Rgba8::TRANSPARENT;
    }

    fn mixer_load_color(&self) -> QColor {
        rgba_to_qcolor(self.mixer_reservoir)
    }

    fn begin_mixer(mut self: core::pin::Pin<&mut Self>, x: f32, y: f32, pressure: f32) -> bool {
        let brush = self.brush;
        let options = self.mixer_options;
        let reservoir = self.mixer_reservoir;
        let started = self
            .as_mut()
            .rust_mut()
            .doc
            .begin_mixer(&brush, options, reservoir, x, y, pressure);
        if started {
            self.as_mut().canvas_changed();
        }
        started
    }

    fn extend_mixer(mut self: core::pin::Pin<&mut Self>, x: f32, y: f32, pressure: f32) {
        let brush = self.brush;
        let dirty = self.as_mut().rust_mut().doc.extend_mixer(&brush, x, y, pressure);
        if !dirty.is_empty() {
            self.as_mut().canvas_changed();
        }
    }

    fn end_mixer(mut self: core::pin::Pin<&mut Self>) {
        let Some(carried) = self.as_mut().rust_mut().doc.end_mixer() else {
            return;
        };
        // What the brush ends the stroke holding, unless a toggle says
        // otherwise. Clean wins over Load when both are on, which is what CS6
        // does — a cleaned brush is not then reloaded.
        let foreground = self.foreground;
        let (clean, reload) = (self.mixer_clean_after_stroke, self.mixer_load_after_stroke);
        self.as_mut().rust_mut().mixer_reservoir = if clean {
            Rgba8::TRANSPARENT
        } else if reload {
            foreground
        } else {
            carried
        };
        self.sync();
    }

    fn cancel_mixer(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().rust_mut().doc.cancel_mixer();
        self.sync();
    }

    fn set_clone_options(mut self: core::pin::Pin<&mut Self>, aligned: bool, sampling: i32) {
        let mut rust = self.as_mut().rust_mut();
        rust.clone_aligned = aligned;
        rust.clone_sampling = CloneSampling::from_i32(sampling);
    }

    fn set_clone_source(mut self: core::pin::Pin<&mut Self>, x: i32, y: i32) -> bool {
        let sampling = self.clone_sampling;
        let has_content = match sampling {
            CloneSampling::CurrentLayer => self.doc.active_layer().is_some_and(|layer| {
                layer
                    .pixels
                    .get(x - layer.offset.0, y - layer.offset.1)
                    .a
                    > 0
            }),
            // Both the wider modes read the composite; one pixel of it is enough
            // to answer the question.
            _ => {
                let px = self.doc.composite_region(Rect::new(x, y, 1, 1));
                px.get(0, 0).a > 0
            }
        };

        let mut rust = self.as_mut().rust_mut();
        rust.clone_source = Some((x, y));
        // A fresh source means the aligned offset has to be measured again from
        // the next stroke, or the tool would go on cloning from the old place.
        rust.clone_offset = None;
        has_content
    }

    fn clear_clone_source(mut self: core::pin::Pin<&mut Self>) {
        let mut rust = self.as_mut().rust_mut();
        rust.clone_source = None;
        rust.clone_offset = None;
    }

    fn has_clone_source(&self) -> bool {
        self.clone_source.is_some()
    }

    fn begin_clone_stroke(
        mut self: core::pin::Pin<&mut Self>,
        x: f32,
        y: f32,
        pressure: f32,
    ) -> bool {
        let Some(source) = self.clone_source else {
            return false;
        };
        let brush = self.brush;
        let sampling = self.clone_sampling;

        // Aligned keeps the offset the previous stroke established, so the sample
        // point travels with the cursor across strokes. Unaligned measures
        // afresh, so every stroke starts copying from the source point again.
        let offset = match (self.clone_aligned, self.clone_offset) {
            (true, Some(offset)) => offset,
            _ => (source.0 - x.round() as i32, source.1 - y.round() as i32),
        };

        let started = self
            .as_mut()
            .rust_mut()
            .doc
            .begin_clone_stroke(&brush, x, y, pressure, offset, sampling);
        if started {
            self.as_mut().rust_mut().clone_offset = Some(offset);
            self.as_mut().canvas_changed();
        }
        started
    }

    fn set_shape_options(
        mut self: core::pin::Pin<&mut Self>,
        kind: i32,
        corner_radius: f32,
        sides: i32,
        line_weight: f32,
        custom: i32,
    ) {
        self.as_mut().rust_mut().shape_options = ShapeOptions {
            kind: ShapeKind::from_i32(kind),
            corner_radius: corner_radius.max(0.0),
            sides: sides.clamp(3, 100) as u32,
            line_weight: line_weight.max(1.0),
            custom: custom.max(0) as usize,
        };
    }

    fn shape_outline(&self, x0: f32, y0: f32, x1: f32, y1: f32, shift: bool, alt: bool)
        -> QPolygonF
    {
        let points = shape::outline(self.shape_options, (x0, y0), (x1, y1), shift, alt);
        let mut polygon = QPolygonF::default();
        for (x, y) in points {
            polygon.append(QPointF::new(x as f64, y as f64));
        }
        polygon
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_shape(
        mut self: core::pin::Pin<&mut Self>,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        shift: bool,
        alt: bool,
        mode: i32,
    ) -> bool {
        let options = self.shape_options;
        let points = shape::outline(options, (x0, y0), (x1, y1), shift, alt);
        if points.len() < 3 {
            return false;
        }
        let color = self.foreground;
        let mode = ShapeMode::from_i32(mode);

        let drawn = match mode {
            ShapeMode::Shape => self
                .as_mut()
                .rust_mut()
                .doc
                .add_shape_layer(&points, color, options.kind.layer_name())
                .is_some(),
            ShapeMode::Path => self.as_mut().rust_mut().doc.append_shape_path(&points),
            ShapeMode::Pixels => !self
                .as_mut()
                .rust_mut()
                .doc
                .fill_shape(&points, color, 1.0)
                .is_empty(),
        };

        if drawn {
            // A path is overlay geometry rather than pixels, so it repaints the
            // canvas and the Paths panel without touching the layer stack.
            if mode == ShapeMode::Path {
                self.as_mut().paths_changed();
                self.as_mut().canvas_changed();
            } else {
                self.sync();
            }
        }
        drawn
    }

    fn custom_shape_names(&self) -> QString {
        QString::from(shape::CUSTOM_SHAPE_NAMES.join("\n").as_str())
    }

    fn custom_shape_preview(&self, index: i32, size: i32) -> QImage {
        if index < 0 {
            return QImage::default();
        }
        let side = size.clamp(1, 512) as u32;
        let points = shape::custom_shape_preview_points(index as usize, side);
        if points.len() < 3 {
            return QImage::default();
        }

        // Filled through the same polygon coverage the tool itself uses, so a
        // swatch cannot drift from what gets drawn.
        let mut coverage = Selection::new(side, side);
        coverage.apply_polygons_feathered(&[points], SelectionOp::Replace, 0);

        let mut preview = Pixmap::new(side, side);
        for y in 0..side as i32 {
            for x in 0..side as i32 {
                let a = (coverage.coverage_at(x, y) * 255.0 + 0.5) as u8;
                preview.set(x, y, Rgba8::new(0xd4, 0xd4, 0xd4, a));
            }
        }
        pixmap_to_qimage(preview)
    }

    fn pattern_names(&self) -> QString {
        QString::from(pattern::PATTERN_NAMES.join("\n").as_str())
    }

    fn pattern_preview(&self, index: i32, size: i32) -> QImage {
        if index < 0 {
            return QImage::default();
        }
        let Some(tile) = pattern::tile(index as usize) else {
            return QImage::default();
        };
        // The tile repeated to fill the swatch, so a small swatch shows the
        // repeat rather than one enlarged tile.
        let side = size.clamp(1, 512) as u32;
        let Some(filled) = pattern::tiled(index as usize, (side, side), (0, 0)) else {
            return pixmap_to_qimage(tile);
        };
        pixmap_to_qimage(filled)
    }

    fn set_pattern_options(mut self: core::pin::Pin<&mut Self>, index: i32, aligned: bool) {
        let mut engine = self.as_mut().rust_mut();
        engine.pattern_index = index.max(0) as usize;
        engine.pattern_aligned = aligned;
    }

    fn begin_pattern_stroke(
        mut self: core::pin::Pin<&mut Self>,
        x: f32,
        y: f32,
        pressure: f32,
    ) -> bool {
        let brush = self.brush;
        let index = self.pattern_index;
        let aligned = self.pattern_aligned;
        let started = self
            .as_mut()
            .rust_mut()
            .doc
            .begin_pattern_stroke(&brush, x, y, pressure, index, aligned);
        if started {
            self.as_mut().canvas_changed();
        }
        started
    }

    fn gradient_preset_names(&self) -> QString {
        QString::from(gradient::PRESET_NAMES.join("\n").as_str())
    }

    fn gradient_preview(&self, name: &QString, width: i32, height: i32) -> QImage {
        let Some(ramp) = gradient::preset(&name.to_string(), self.foreground, self.background)
        else {
            return QImage::default();
        };
        pixmap_to_qimage(ramp.preview(
            width.clamp(1, 4096) as u32,
            height.clamp(1, 4096) as u32,
        ))
    }

    fn set_gradient_options(
        mut self: core::pin::Pin<&mut Self>,
        preset: &QString,
        kind: i32,
        mode: i32,
        opacity: i32,
        reverse: bool,
        dither: bool,
        transparency: bool,
    ) {
        let name = preset.to_string();
        let mut rust = self.as_mut().rust_mut();
        // An unknown name would leave the tool with nothing to draw, so keep the
        // one that is already set.
        if gradient::preset(&name, Rgba8::BLACK, Rgba8::WHITE).is_some() {
            rust.gradient_preset = name;
        }
        rust.gradient_options = GradientOptions {
            kind: GradientType::from_i32(kind),
            mode: BlendMode::from_i32(mode),
            opacity: opacity.clamp(0, 100) as f32 / 100.0,
            reverse,
            dither,
            transparency,
            // Set from the layer when the gradient is drawn.
            preserve_alpha: false,
        };
    }

    fn draw_gradient(mut self: core::pin::Pin<&mut Self>, x0: f32, y0: f32, x1: f32, y1: f32)
        -> bool
    {
        // Built here rather than held: "Foreground to Background" has to mean the
        // colours as they are now, not as they were when the preset was picked.
        let Some(ramp) =
            gradient::preset(&self.gradient_preset, self.foreground, self.background)
        else {
            return false;
        };
        let options = self.gradient_options;
        let dirty = self
            .as_mut()
            .rust_mut()
            .doc
            .draw_gradient(&ramp, &options, (x0, y0), (x1, y1));
        if dirty.is_empty() {
            return false;
        }
        self.sync();
        true
    }

    fn set_focus_tool(mut self: core::pin::Pin<&mut Self>, tool: i32) {
        let mut rust = self.as_mut().rust_mut();
        rust.focus_tool = tool.clamp(0, 2);
        rust.tone_active = false;
    }

    fn set_tone_tool(mut self: core::pin::Pin<&mut Self>, tool: i32) {
        let mut rust = self.as_mut().rust_mut();
        rust.tone_options.tool = ToneTool::from_i32(tool);
        rust.tone_active = true;
    }

    fn set_tone_options(
        mut self: core::pin::Pin<&mut Self>,
        amount: i32,
        range: i32,
        sponge: i32,
        protect_tones: bool,
        vibrance: bool,
    ) {
        let tool = self.tone_options.tool;
        self.as_mut().rust_mut().tone_options = ToneOptions {
            tool,
            range: ToneRange::from_i32(range),
            sponge: SpongeMode::from_i32(sponge),
            amount: amount.clamp(0, 100) as f32 / 100.0,
            protect_tones,
            vibrance,
            // Set from the layer when the stroke begins.
            preserve_alpha: false,
        };
    }

    fn set_focus_options(
        mut self: core::pin::Pin<&mut Self>,
        strength: i32,
        mode: i32,
        sample_all_layers: bool,
        protect_detail: bool,
        finger_painting: bool,
    ) {
        let strength = strength.clamp(0, 100) as f32 / 100.0;
        let mode = BlendMode::from_i32(mode);
        let focus = FocusMode::from_i32(self.focus_tool);
        let mut rust = self.as_mut().rust_mut();
        rust.focus_options = FocusOptions {
            focus,
            strength,
            mode,
            sample_all_layers,
            protect_detail,
            // Set from the layer when the stroke begins.
            preserve_alpha: false,
        };
        rust.smudge_options = SmudgeOptions {
            strength,
            mode,
            sample_all_layers,
            finger_painting,
            preserve_alpha: false,
        };
    }

    fn begin_retouch_stroke(
        mut self: core::pin::Pin<&mut Self>,
        x: f32,
        y: f32,
        pressure: f32,
    ) -> bool {
        let brush = self.brush;
        // Three shapes of stroke behind one entry point: toning reads a pixel's
        // own tone, smudge carries pixels along, and the focus pair reads a
        // neighbourhood.
        let started = if self.tone_active {
            let options = self.tone_options;
            self.as_mut()
                .rust_mut()
                .doc
                .begin_tone(&brush, options, x, y, pressure)
        } else if self.focus_tool == 2 {
            let options = self.smudge_options;
            let paint = self.foreground;
            self.as_mut()
                .rust_mut()
                .doc
                .begin_smudge(&brush, options, paint, x, y, pressure)
        } else {
            let options = self.focus_options;
            self.as_mut()
                .rust_mut()
                .doc
                .begin_focus(&brush, options, x, y, pressure)
        };
        if started {
            self.as_mut().canvas_changed();
        }
        started
    }

    fn extend_retouch_stroke(
        mut self: core::pin::Pin<&mut Self>,
        x: f32,
        y: f32,
        pressure: f32,
    ) {
        let brush = self.brush;
        let dirty = self.as_mut().rust_mut().doc.extend_retouch(&brush, x, y, pressure);
        if !dirty.is_empty() {
            self.as_mut().canvas_changed();
        }
    }

    fn end_retouch_stroke(mut self: core::pin::Pin<&mut Self>) {
        if self.as_mut().rust_mut().doc.end_retouch() {
            self.sync();
        }
    }

    fn cancel_retouch_stroke(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().rust_mut().doc.cancel_retouch();
        self.sync();
    }

    fn set_bucket_options(
        mut self: core::pin::Pin<&mut Self>,
        mode: i32,
        opacity: i32,
        tolerance: i32,
        antialias: bool,
        contiguous: bool,
        all_layers: bool,
    ) {
        self.as_mut().rust_mut().bucket_options = BucketOptions {
            mode: BlendMode::from_i32(mode),
            opacity: opacity.clamp(0, 100) as f32 / 100.0,
            tolerance: tolerance.clamp(0, 255) as u32,
            antialias,
            contiguous,
            all_layers,
            // Set from the layer when the fill happens.
            preserve_alpha: false,
        };
    }

    fn fill_bucket(mut self: core::pin::Pin<&mut Self>, x: i32, y: i32) -> bool {
        let options = self.bucket_options;
        // The Paint Bucket fills with the foreground colour, but honours erase
        // mode the same way a stroke does.
        let colour = self.paint_color();
        let dirty = self
            .as_mut()
            .rust_mut()
            .doc
            .fill_bucket((x, y), &options, colour);
        if dirty.is_empty() {
            return false;
        }
        self.sync();
        true
    }

    fn set_heal_mode(mut self: core::pin::Pin<&mut Self>, mode: i32) {
        self.as_mut().rust_mut().heal_mode = if mode < 0 {
            None
        } else {
            Some(HealMode::from_i32(mode))
        };
    }

    fn set_heal_source(mut self: core::pin::Pin<&mut Self>, active: bool, dx: i32, dy: i32) {
        self.as_mut().rust_mut().heal_source = if active { Some((dx, dy)) } else { None };
    }

    fn patch_selection(
        mut self: core::pin::Pin<&mut Self>,
        dx: i32,
        dy: i32,
        content_aware: bool,
        destination: bool,
        transparent: bool,
    ) {
        let options = PatchOptions {
            dx,
            dy,
            content_aware,
            destination,
            transparent,
        };
        self.as_mut().rust_mut().doc.patch_selection(options);
        self.sync();
    }

    fn content_aware_move(
        mut self: core::pin::Pin<&mut Self>,
        dx: i32,
        dy: i32,
        extend: bool,
        structure: i32,
        color: i32,
        sample_all_layers: bool,
    ) {
        let options = MoveOptions {
            dx,
            dy,
            extend,
            structure: structure.clamp(1, 7) as u32,
            color: color.clamp(0, 10) as u32,
        };
        self.as_mut()
            .rust_mut()
            .doc
            .content_aware_move(&options, sample_all_layers);
        self.sync();
    }

    fn remove_red_eye(
        mut self: core::pin::Pin<&mut Self>,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        pupil: i32,
        darken: i32,
    ) {
        let rect = Rect::new(x, y, width.max(0) as u32, height.max(0) as u32);
        self.as_mut().rust_mut().doc.remove_red_eye(
            rect,
            pupil.clamp(0, 100) as u32,
            darken.clamp(0, 100) as u32,
        );
        self.sync();
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

    fn quick_mask(&self) -> bool {
        self.doc.quick_mask()
    }

    fn set_quick_mask(mut self: core::pin::Pin<&mut Self>, on: bool) {
        self.as_mut().rust_mut().doc.set_quick_mask(on);
        // Both the canvas and the selection outline change: the veil goes on or
        // off, and the marching ants give way to it.
        self.as_mut().selection_changed();
        self.as_mut().canvas_changed();
    }

    fn copy_selection(mut self: core::pin::Pin<&mut Self>, merged: bool) -> QImage {
        let Some((pixels, origin)) = self.as_mut().rust_mut().doc.copy_selection(merged) else {
            return QImage::default();
        };
        self.as_mut().rust_mut().copy_origin = origin;
        pixmap_to_qimage(pixels)
    }

    fn copy_origin_x(&self) -> i32 {
        self.copy_origin.0
    }

    fn copy_origin_y(&self) -> i32 {
        self.copy_origin.1
    }

    fn paste_image(
        mut self: core::pin::Pin<&mut Self>,
        image: &QImage,
        x: i32,
        y: i32,
        mode: i32,
    ) -> bool {
        let Some(pixels) = qimage_to_pixmap(image) else {
            return false;
        };
        let mode = match mode {
            1 => PasteMode::Into,
            2 => PasteMode::Outside,
            _ => PasteMode::Plain,
        };
        self.as_mut().rust_mut().doc.paste_into(pixels, (x, y), mode);
        self.sync();
        true
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
        feather: i32,
    ) {
        let rect = Rect::new(x, y, width.max(0) as u32, height.max(0) as u32);
        let op = SelectionOp::from_i32(op);
        let feather = feather.clamp(0, 1000) as u32;
        self.as_mut().rust_mut().doc.select_rect(rect, op, feather);
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
        feather: i32,
    ) {
        let rect = Rect::new(x, y, width.max(0) as u32, height.max(0) as u32);
        let op = SelectionOp::from_i32(op);
        let feather = feather.clamp(0, 1000) as u32;
        self.as_mut().rust_mut().doc.select_ellipse(rect, op, feather);
        self.as_mut().selection_changed();
        self.as_mut().canvas_changed();
    }

    fn select_polygon(
        mut self: core::pin::Pin<&mut Self>,
        points: &ffi::QVector_f32,
        op: i32,
        feather: i32,
    ) {
        // Interleaved x,y. An odd trailing value would be a malformed call
        // from the shell; drop it rather than reading past the end.
        let pairs: Vec<(f32, f32)> = points
            .iter()
            .copied()
            .collect::<Vec<f32>>()
            .chunks_exact(2)
            .map(|p| (p[0], p[1]))
            .collect();

        let op = SelectionOp::from_i32(op);
        let feather = feather.clamp(0, 1000) as u32;
        self.as_mut().rust_mut().doc.select_polygon(&pairs, op, feather);
        self.as_mut().selection_changed();
        self.as_mut().canvas_changed();
    }

    fn begin_magnetic(mut self: core::pin::Pin<&mut Self>, contrast: i32) {
        let composite = self.doc.composite();
        let map = EdgeMap::from_pixmap(&composite, contrast.clamp(1, 100) as u32);
        self.as_mut().rust_mut().edge_map = Some(map);
    }

    fn magnetic_trace(&self, x0: i32, y0: i32, x1: i32, y1: i32, width: i32) -> Vec<i32> {
        let Some(map) = self.edge_map.as_ref() else {
            return vec![x0, y0, x1, y1];
        };
        let path = map.trace((x0, y0), (x1, y1), width.clamp(1, 256) as u32);

        let mut flat = Vec::with_capacity(path.len() * 2);
        for (x, y) in path {
            flat.push(x);
            flat.push(y);
        }
        flat
    }

    fn end_magnetic(mut self: core::pin::Pin<&mut Self>) {
        self.as_mut().rust_mut().edge_map = None;
    }

    #[allow(clippy::too_many_arguments)]
    fn magic_wand(
        mut self: core::pin::Pin<&mut Self>,
        x: i32,
        y: i32,
        tolerance: i32,
        contiguous: bool,
        antialias: bool,
        op: i32,
        feather: i32,
    ) {
        let composite = self.doc.composite();
        let mask = wand::magic_wand(
            &composite,
            (x, y),
            tolerance.clamp(0, 255) as u32,
            contiguous,
            antialias,
        );

        let op = SelectionOp::from_i32(op);
        let feather = feather.clamp(0, 1000) as u32;
        self.as_mut().rust_mut().doc.select_mask(&mask, op, feather);
        self.as_mut().selection_changed();
        self.as_mut().canvas_changed();
    }

    fn begin_quick_select(mut self: core::pin::Pin<&mut Self>, op: i32, feather: i32) {
        let composite = self.doc.composite();
        let selector = QuickSelector::new(&composite);
        let base = self.doc.selection().clone();

        let mut rust = self.as_mut().rust_mut();
        rust.quick_select = Some(selector);
        rust.quick_base = Some(base);
        rust.quick_op = SelectionOp::from_i32(op);
        rust.quick_feather = feather.clamp(0, 1000) as u32;
    }

    fn quick_select_dab(
        mut self: core::pin::Pin<&mut Self>,
        x: f32,
        y: f32,
        radius: f32,
        subtract: bool,
    ) {
        let (op, feather) = (self.quick_op, self.quick_feather);

        let mask = {
            let mut rust = self.as_mut().rust_mut();
            let Some(selector) = rust.quick_select.as_mut() else {
                return;
            };
            if subtract {
                selector.subtract_dab(x, y, radius);
            } else {
                selector.add_dab(x, y, radius);
            }
            selector.mask().to_vec()
        };

        // Rebuild from the pre-drag snapshot each time rather than combining
        // onto the running result: the drag as a whole is one operation, so
        // subtracting a dab has to be able to give pixels back.
        let Some(base) = self.quick_base.clone() else {
            return;
        };
        let mut updated = base;
        updated.apply_mask_feathered(&mask, op, feather);

        self.as_mut().rust_mut().doc.set_selection(updated);
        self.as_mut().selection_changed();
        self.as_mut().canvas_changed();
    }

    fn end_quick_select(mut self: core::pin::Pin<&mut Self>) {
        let mut rust = self.as_mut().rust_mut();
        rust.quick_select = None;
        rust.quick_base = None;
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

    fn selection_outline(&self) -> Vec<i32> {
        let loops = self.doc.selection().outline();
        let mut flat = Vec::with_capacity(loops.iter().map(|l| l.len() * 2 + 1).sum());
        for points in loops {
            flat.push(points.len() as i32);
            for (x, y) in points {
                flat.push(x);
                flat.push(y);
            }
        }
        flat
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

    fn apply_levels(
        mut self: core::pin::Pin<&mut Self>,
        in_black: f32,
        in_white: f32,
        gamma: f32,
        out_black: f32,
        out_white: f32,
        channel: i32,
    ) {
        if channel >= 1 && channel <= 3 {
            self.as_mut().rust_mut().doc.apply_levels_channel(
                (channel - 1) as usize,
                in_black,
                in_white,
                gamma.max(0.01),
                out_black,
                out_white,
            );
        } else {
            let adjustment = Adjustment::Levels {
                in_black,
                in_white,
                gamma: gamma.max(0.01),
                out_black,
                out_white,
            };
            self.as_mut().rust_mut().doc.apply_adjustment(adjustment);
        }
        self.sync();
    }

    fn apply_curves_lut(
        mut self: core::pin::Pin<&mut Self>,
        lut: &[u8],
        channel: i32,
    ) {
        if lut.len() != 256 {
            return;
        }
        self.as_mut().rust_mut().doc.apply_curves_lut(lut, channel);
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

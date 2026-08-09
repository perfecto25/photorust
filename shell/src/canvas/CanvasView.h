#pragma once

#include <QImage>
#include <QPainterPath>
#include <QPoint>
#include <QPointF>
#include <QPolygonF>
#include <QWidget>

#include "../tools/ToolId.h"

class Engine;

/// The document viewport.
///
/// Owns zoom/pan and translates mouse input into document-space coordinates
/// before handing it to the engine. All pixel work happens in Rust — this
/// widget only presents the `QImage` the engine hands back, so the painting
/// path stays a blit plus the checkerboard beneath it.
///
/// Rendering today is `QPainter` onto a widget. The GPU backend described in
/// CLAUDE.md §7 will replace `paintEvent` without changing this interface.
class CanvasView : public QWidget
{
    Q_OBJECT

public:
    explicit CanvasView(Engine *engine, QWidget *parent = nullptr);

    /// Current zoom factor; 1.0 is 100%.
    double zoom() const { return m_zoom; }

    /// Set zoom, keeping the view centre fixed. Clamped to the range CS6
    /// allows (0.1% to 3200%).
    void setZoom(double zoom);

    /// Set zoom, keeping `focusWidgetPos` pinned to the same document pixel.
    /// This is what makes scroll-wheel zoom feel anchored to the cursor.
    void setZoomAt(double zoom, const QPointF &focusWidgetPos);

    /// Scale so the whole document fits, then centre it (Ctrl+0).
    void fitToWindow();

    /// Jump to 100% (Ctrl+1).
    void actualPixels();

    void zoomIn();
    void zoomOut();

    /// The tool that receives mouse input.
    void setActiveTool(ToolId tool);
    ToolId activeTool() const { return m_tool; }

    /// Which marquee variant the Marquee tool draws.
    void setMarqueeType(MarqueeType type);
    MarqueeType marqueeType() const { return m_marqueeType; }

    /// Which lasso variant the Lasso tool uses.
    void setLassoType(LassoType type);
    LassoType lassoType() const { return m_lassoType; }

    /// The Magnetic Lasso's options-bar settings: detection width in pixels,
    /// edge contrast 1–100, and how often a fastening point is dropped.
    void setMagneticOptions(int width, int contrast, int frequency);

    /// Which of the two colour-selection tools the Quick Selection button
    /// currently holds.
    void setQuickSelectType(QuickSelectType type);
    QuickSelectType quickSelectType() const { return m_quickSelectType; }

    /// Options-bar settings for those two: the brush diameter the Quick
    /// Selection tool grows from, and the wand's tolerance and checkboxes.
    void setQuickSelectOptions(int brushSize, int tolerance, bool antialias, bool contiguous);

    /// The Spot Healing Brush's Type — how the covered region is rebuilt.
    void setHealType(HealType type);
    HealType healType() const { return m_healType; }

    /// Route strokes through the Color Replacement path, which recolours the
    /// layer per dab instead of compositing a stroke at the end.
    void setReplaceMode(bool active);

    /// Route strokes through the Mixer Brush path, which mixes the brush's paint
    /// with what is already on the layer, dab by dab.
    void setMixerMode(bool active);

    /// Route strokes through the retouch path, which works on what is under the
    /// brush instead of painting on it. True for the Blur button's three tools
    /// and the Dodge button's three; which of the six strokes is the engine's
    /// business, so the canvas has one path for all of them.
    void setRetouchMode(bool active);

    /// Which of the two tools the Gradient button currently holds. The Paint
    /// Bucket fills on a click; the Gradient tool drags out an axis.
    void setGradientTool(GradientTool tool);
    GradientTool gradientTool() const { return m_gradientTool; }

    /// Which of the five tools the Pen button currently holds.
    void setPenTool(PenTool tool);
    PenTool penTool() const { return m_penTool; }

    /// Which of the two tools the Path Selection button currently holds.
    void setPathSelectTool(PathSelectTool tool);
    PathSelectTool pathSelectTool() const { return m_pathSelectTool; }

    /// The Pen tool's options: Auto Add/Delete lets hovering the finished part
    /// of the active path add or remove an anchor without switching tools;
    /// Rubber Band previews the next segment before it is placed.
    void setPenOptions(bool autoAddDelete, bool rubberBand);

    /// How coarsely the Freeform Pen simplifies a drag into corner anchors, in
    /// document pixels.
    void setFreeformPenTolerance(double tolerance) { m_freeformTolerance = tolerance; }

    /// The Clone Stamp's options: Aligned keeps the sample point travelling
    /// with the cursor across strokes, and Sample picks which layers it reads.
    void setCloneOptions(bool aligned, CloneSampling sampling);

    /// Which healing-group variant is active.
    void setHealingType(HealingType type);
    HealingType healingType() const { return m_healingType; }

    /// Patch options, from CS6's bar: Content-Aware rebuilds in place and
    /// ignores the drag, Destination reverses which end of the drag is
    /// repaired, and Transparent transfers texture without colour.
    void setPatchOptions(bool contentAware, bool destination, bool transparent);

    /// Red Eye options: CS6's Pupil Size and Darken Amount, both 0-100.
    void setRedEyeOptions(int pupilSize, int darkenAmount);
    /// Content-Aware Move options, from CS6's bar: Extend duplicates instead
    /// of moving, Structure (1-7) sets how strictly the fill follows edges,
    /// Color (0-10) how far the moved pixels adapt to their new surroundings,
    /// and Sample All Layers reads the composite rather than one layer.
    void setContentAwareMoveOptions(bool extend, int structure, int color,
                                    bool sampleAllLayers);

    /// Apply the pending Patch / Content-Aware Move drag. Nothing happens
    /// unless a region has been dragged.
    void commitHealingDrag();

    /// Which eyedropper-group variant is active.
    void setEyedropperType(EyedropperType type);
    EyedropperType eyedropperType() const { return m_eyedropperType; }

    /// Re-fetch annotations (samplers, notes, counts, ruler) and repaint.
    /// Driven by the engine's `annotationsChanged` signal.
    void refreshAnnotations();

    /// Which crop variant the Crop button currently holds.
    void setCropType(CropType type);
    CropType cropType() const { return m_cropType; }

    /// Crop options: the aspect ratio the box is locked to (width / height,
    /// or 0 for unconstrained) and whether committing discards the pixels that
    /// fall outside.
    void setCropOptions(double aspectRatio, bool deleteCropped);

    /// Apply the current crop box to the document. Does nothing unless the
    /// Crop tool is active.
    void commitCrop();
    /// Reset the crop box to the whole canvas, CS6's Esc behaviour.
    void resetCrop();

    /// Re-fetch the slice list from the engine and repaint the overlay.
    /// Driven by the engine's `slicesChanged` signal.
    void refreshSlices();
    /// Delete the selected user slice, if any.
    void deleteSelectedSlice();
    /// The selected user slice's index, or -1.
    int selectedSlice() const { return m_selectedSlice; }

    /// How the next selection combines with the current one — the options
    /// bar's new/add/subtract/intersect buttons. Modifiers held during a drag
    /// override this for that drag only, as they do in CS6.
    void setSelectionMode(SelectionMode mode) { m_selectionMode = mode; }
    SelectionMode selectionMode() const { return m_selectionMode; }

    /// Feather radius in pixels for new selections — the options bar's
    /// Feather field. Softens the incoming region only, so it does not
    /// re-soften what the selection already holds.
    void setFeatherRadius(int pixels) { m_featherRadius = qMax(0, pixels); }
    int featherRadius() const { return m_featherRadius; }

    /// Re-fetch the composited image from the engine and repaint.
    void refresh();

    /// Drop the Clone Stamp's and Healing Brush's sampled source points.
    ///
    /// Called when the document changes. Photoshop holds a clone source per
    /// document; ours is one per engine, so carrying it into another image would
    /// mean cloning from coordinates that mean nothing there.
    void forgetSampleSources();

    /// Re-trace the selection outline from the engine and repaint.
    ///
    /// Deliberately *not* called by `refresh()`. Tracing the contour walks the
    /// whole mask, and `refresh()` runs on every brush dab; only a real change
    /// to the selection should pay for it. The engine's `selectionChanged`
    /// signal is the trigger.
    void refreshSelection();

    /// Convert a widget point to document space.
    QPointF widgetToDocument(const QPointF &pos) const;
    /// Convert a document point to widget space.
    QPointF documentToWidget(const QPointF &pos) const;

signals:
    /// Emitted as the cursor moves, for the status bar and Info panel.
    void cursorMoved(const QPointF &documentPos);
    /// The cursor left the canvas, so readouts of what is under it should
    /// blank rather than hold their last value.
    void cursorLeft();
    /// Emitted whenever the zoom factor changes.
    void zoomChanged(double zoom);
    /// The user picked a colour with the eyedropper.
    void colorPicked(const QColor &color);
    /// Right-click with a selection tool active. `globalPos` is where the
    /// menu should open. The canvas does not build the menu itself: the
    /// commands on it belong to the registry, which `MainWindow` owns.
    void contextMenuRequested(const QPoint &globalPos);
    /// A note needs its text edited. The canvas does not own dialogs, so
    /// `MainWindow` puts one up and writes the result back to the engine.
    void noteEditRequested(int index);
    /// Something worth telling the user, for the status bar — a tool that
    /// needs a step taken first, for instance.
    void statusMessage(const QString &text);
    /// The Healing Brush was used before a source was set. `MainWindow` puts up
    /// the warning; the canvas does not own dialogs.
    void healingSourceRequired();
    /// The Clone Stamp was used before a source was Alt-clicked.
    void cloneSourceRequired();
    /// A tool was used on a layer whose pixels or position are locked.
    /// `MainWindow` names the tool and puts the dialog up — it holds the active
    /// variant, and the canvas does not own dialogs.
    void lockedLayerRefused();
    /// A mixer stroke ended, so the paint on the brush has changed. The options
    /// bar's load swatch reads it back from the engine.
    void mixerLoadChanged();

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;
    void mouseDoubleClickEvent(QMouseEvent *event) override;
    void wheelEvent(QWheelEvent *event) override;
    void keyPressEvent(QKeyEvent *event) override;
    void keyReleaseEvent(QKeyEvent *event) override;
    void contextMenuEvent(QContextMenuEvent *event) override;
    void resizeEvent(QResizeEvent *event) override;
    void enterEvent(QEnterEvent *event) override;
    void leaveEvent(QEvent *event) override;

private:
    /// Where the document's top-left sits in widget coordinates.
    QPointF documentOrigin() const;
    /// The document's on-screen rectangle at the current zoom.
    QRectF documentRect() const;
    /// Keep the document from being panned entirely out of view.
    void clampPan();
    /// Cursor appropriate to the active tool and modifier state.
    void updateCursor();
    /// Draw the Gradient tool's drag line.
    void paintGradientDrag(QPainter &painter);
    /// The drag's end point, snapped to 45° steps while `Shift` is held — the
    /// same constraint CS6 puts on a gradient drag.
    QPointF constrainedGradientEnd(const QPointF &doc, Qt::KeyboardModifiers modifiers) const;

    /// Handle a press with the Clone Stamp. Returns true when the press was
    /// consumed — an Alt-click that sets the source, or a stroke refused for
    /// want of one — and false to fall through to the ordinary stroke path.
    bool clonePress(const QPointF &doc, Qt::KeyboardModifiers modifiers);
    /// Draw the Clone Stamp's sampled source point.
    void paintCloneSource(QPainter &painter);

    /// Emit `lockedLayerRefused` if the engine turned an edit down because the
    /// layer's pixels are locked. Silent when it refused for any other reason.
    void reportIfLocked();
    /// Marching-ants outline of the current selection.
    void paintSelection(QPainter &painter);
    /// The combine operation for a gesture made with `modifiers` held: the
    /// options-bar mode unless a modifier overrides it.
    SelectionMode effectiveSelectionMode(Qt::KeyboardModifiers modifiers) const;
    /// Send the in-progress marquee to the engine.
    void commitMarquee(const QRectF &documentRect, Qt::KeyboardModifiers modifiers);
    /// Send the traced lasso path to the engine as a closed polygon.
    void commitLasso(Qt::KeyboardModifiers modifiers);
    /// Abandon an in-progress lasso without touching the selection.
    void cancelLasso();
    /// Close a click-driven lasso and commit it.
    void closeLasso();
    /// Re-run the magnetic wire from the last anchor to `doc`, and drop a
    /// fastening point if the segment has grown long enough.
    void updateMagneticWire(const QPointF &doc);
    /// True when the active marquee variant is a click rather than a drag.
    bool marqueeIsLineSelect() const;
    /// True when the active tool traces an outline rather than dragging a
    /// rectangle out.
    bool toolIsLasso() const { return m_tool == ToolId::Lasso; }
    /// True when the lasso is entered by dragging with the button held.
    bool lassoIsDragged() const
    {
        return toolIsLasso() && m_lassoType == LassoType::Freehand;
    }
    /// True when the lasso is entered by clicking anchors, with the button
    /// released between them.
    bool lassoIsClicked() const { return toolIsLasso() && !lassoIsDragged(); }
    /// Whether `doc` is close enough to the first anchor to close the shape.
    bool nearLassoStart(const QPointF &doc) const;

    /// A fixed screen-pixel hit radius, converted to document units at the
    /// current zoom — the same idea as `nearLassoStart`'s, generalised for
    /// anchor/handle/segment hit-testing.
    float pathHitRadius() const;
    /// Handle a press with one of the Pen button's five tools. Alt at press
    /// time is not consulted — unlike real Photoshop, this Pen tool does not
    /// temporarily borrow Convert Point's press behaviour under Alt; that
    /// functionality is still fully available through the dedicated Convert
    /// Point tool in the flyout.
    void penPress(const QPointF &doc);
    void penMove(const QPointF &doc, Qt::KeyboardModifiers modifiers);
    void penRelease(const QPointF &doc);
    /// Handle a press with one of the Path Selection button's two tools.
    void pathSelectPress(const QPointF &doc);
    void pathSelectMove(const QPointF &doc, Qt::KeyboardModifiers modifiers);
    void pathSelectRelease();
    /// Draw the active path: its curve, anchors and handles, and the Rubber
    /// Band preview of the segment about to be placed.
    void paintPathOverlay(QPainter &painter);
    /// True when the Quick Selection button is holding the drag-a-brush tool
    /// rather than the click-once wand.
    bool toolIsQuickBrush() const
    {
        return m_tool == ToolId::QuickSelect && m_quickSelectType == QuickSelectType::Brush;
    }
    /// End a Quick Selection drag if one is running.
    void finishQuickSelect();

    /// Which part of the crop box the cursor is over, for hit-testing and for
    /// the resize cursor. Ordered so the corners and edges can be tested as a
    /// group.
    enum class CropGrip {
        None,
        Move,
        TopLeft,
        Top,
        TopRight,
        Right,
        BottomRight,
        Bottom,
        BottomLeft,
        Left,
    };

    /// The grip under a widget-space point, or `None` outside the box.
    CropGrip cropGripAt(const QPointF &widgetPos) const;
    /// As `cropGripAt`, against any document-space rectangle. Shared with the
    /// slice tools, whose handles behave the same way.
    CropGrip gripAt(const QRectF &docRect, const QPointF &widgetPos) const;
    /// Cursor shape for a grip.
    Qt::CursorShape cropCursor(CropGrip grip) const;
    /// Move or resize the crop box for a drag to `doc`.
    void dragCrop(const QPointF &doc);
    /// Force the box back to the locked aspect ratio, pivoting on the corner
    /// opposite the one being dragged.
    void applyCropRatio(CropGrip grip);
    /// Paint the crop box: shield, thirds overlay, border and handles.
    void paintCrop(QPainter &painter);

    /// True when the Crop button is holding the perspective variant, which
    /// marks out a free quadrilateral rather than an aligned rectangle.
    bool cropIsPerspective() const
    {
        return m_tool == ToolId::Crop && m_cropType == CropType::Perspective;
    }
    /// Index of the quad corner under a widget-space point, or -1.
    int cropCornerAt(const QPointF &widgetPos) const;
    /// Warp the marked quadrilateral into a rectangle and crop to it.
    void commitPerspectiveCrop();
    /// Paint the perspective quad: shield, grid, edges and corner handles.
    void paintCropQuad(QPainter &painter);

    /// True when the active healing variant is stroked with the brush.
    bool healingIsStroked() const
    {
        return m_tool == ToolId::Healing && healingIsBrush(m_healingType);
    }
    /// True when the active healing variant works on a dragged region.
    bool healingIsRegion() const
    {
        return m_tool == ToolId::Healing
            && (m_healingType == HealingType::Patch
                || m_healingType == HealingType::ContentAwareMove);
    }
    /// True while a freehand outline is being traced, by the lasso or by one of
    /// the region-based healing tools.
    bool freehandTracing() const { return lassoIsDragged() || m_healingTracing; }
    /// Press, drag and release for the healing group's non-brush variants.
    /// Returns true when the event was consumed.
    bool healingPress(const QPointF &doc, Qt::KeyboardModifiers modifiers);
    bool healingDrag(const QPointF &doc);
    bool healingRelease();
    /// Paint the healing tools' overlays: the sampled source, the region drag
    /// and the red-eye rectangle.
    void paintHealing(QPainter &painter);

    /// True when the eyedropper button is holding one of the annotation
    /// tools rather than the eyedropper itself.
    bool toolIsAnnotation() const
    {
        return m_tool == ToolId::Eyedropper
            && m_eyedropperType != EyedropperType::Eyedropper;
    }
    /// The marker kind the active variant places, if it places one.
    bool activeMarkerKind(MarkerKind *kind) const;
    /// Press, drag and release for the annotation tools.
    void annotationPress(const QPointF &doc, Qt::KeyboardModifiers modifiers);
    void annotationDrag(const QPointF &doc);
    /// Paint markers and the ruler.
    void paintAnnotations(QPainter &painter);
    /// Grab radius in document pixels for a fixed on-screen size.
    float grabRadiusDoc() const;

    /// True when the Crop button is holding one of the two slice tools, which
    /// edit web-export cut lines rather than the canvas.
    bool toolIsSlice() const
    {
        return m_tool == ToolId::Crop
            && (m_cropType == CropType::Slice || m_cropType == CropType::SliceSelect);
    }
    /// Index into `m_slices` of the topmost slice containing `doc`, preferring
    /// user slices over the auto slices beneath them. -1 if none.
    int sliceAt(const QPointF &doc) const;
    /// Paint the slice overlay: cut lines and numbered badges.
    void paintSlices(QPainter &painter);

    Engine *m_engine = nullptr;

    /// Cached composite. Refreshed from the engine, never edited here.
    QImage m_image;

    double m_zoom = 1.0;
    /// Pan offset in widget pixels, from the centred position.
    QPointF m_pan{0.0, 0.0};

    ToolId m_tool = ToolId::Brush;
    MarqueeType m_marqueeType = MarqueeType::Rectangular;
    LassoType m_lassoType = LassoType::Freehand;
    QuickSelectType m_quickSelectType = QuickSelectType::Brush;
    SelectionMode m_selectionMode = SelectionMode::New;
    int m_featherRadius = 0;

    // -- interaction state --
    bool m_dragging = false;
    bool m_panning = false;
    /// True while space is held, which temporarily activates the Hand tool.
    bool m_spacePanOverride = false;
    QPointF m_lastMousePos;
    QPointF m_dragStartDoc;

    /// Live marquee rectangle while dragging a selection tool.
    QRectF m_marquee;
    bool m_marqueeActive = false;
    /// The lasso outline committed so far, in document coordinates.
    ///
    /// For the freehand variant this is the traced path; points are appended
    /// only once the cursor has moved a visible distance, so a slow drag does
    /// not pile up thousands of coincident vertices. For the click-driven
    /// variants it is the anchors placed so far, plus — for magnetic — every
    /// wire point between them.
    QPolygonF m_lassoPath;
    /// The segment between the last committed point and the cursor: a rubber
    /// band for the polygonal lasso, the live wire for the magnetic one.
    /// Not yet part of the outline, and redrawn on every move.
    QPolygonF m_lassoPreview;
    /// Where the cursor last was, in document space, while a click-driven
    /// lasso is open. Used to keep the preview correct across repaints.
    QPointF m_lassoCursor;

    // -- magnetic lasso options, from the options bar --
    int m_magneticWidth = MagneticDefaults::kWidth;
    int m_magneticContrast = MagneticDefaults::kContrast;
    int m_magneticFrequency = MagneticDefaults::kFrequency;

    // -- quick selection and magic wand options --
    int m_quickBrushSize = WandDefaults::kBrushSize;
    int m_wandTolerance = WandDefaults::kTolerance;
    bool m_wandAntialias = WandDefaults::kAntialias;
    bool m_wandContiguous = WandDefaults::kContiguous;
    /// True between press and release of a Quick Selection drag, so the engine
    /// gets exactly one begin/end pair around the dabs.
    bool m_quickSelecting = false;

    // -- crop --
    /// The crop box in document coordinates. Only meaningful while the Crop
    /// tool is active, where it starts out as the whole canvas.
    QRectF m_cropRect;
    /// The grip being dragged, or `None` when the box is at rest.
    CropGrip m_cropGrip = CropGrip::None;
    /// The box and cursor position when the drag began, so a move is computed
    /// from the press rather than accumulated per motion event.
    QRectF m_cropStartRect;
    QPointF m_cropStartDoc;
    /// Width / height the box is locked to; 0 leaves it free.
    double m_cropRatio = 0.0;
    bool m_cropDeletePixels = true;

    CropType m_cropType = CropType::Rectangular;
    /// The perspective quad's four corners in document coordinates, ordered
    /// top-left, top-right, bottom-right, bottom-left. That order is the
    /// contract with the engine, which uses it to work out which pair of edges
    /// gives the output width and which the height.
    QPolygonF m_cropQuad;
    QPolygonF m_cropStartQuad;
    /// Corner being dragged, or -1.
    int m_cropCorner = -1;
    /// True while the whole quad is being dragged rather than one corner.
    bool m_cropQuadMoving = false;
    /// True while the initial rectangle is being dragged out, before any
    /// corner has been pulled off square.
    bool m_cropQuadNew = false;

    // -- slices --
    /// One entry of the engine's resolved slice list, cached for painting and
    /// hit-testing. Auto slices have `userIndex == -1`.
    struct SliceInfo {
        QRectF rect;
        int number = 0;
        int userIndex = -1;
    };
    QList<SliceInfo> m_slices;
    /// The selected user slice's index, or -1. Slice Select sets this.
    int m_selectedSlice = -1;
    /// The rectangle being dragged out by the Slice tool.
    QRectF m_sliceDrag;
    bool m_sliceDragging = false;
    /// Which grip of the selected slice is being dragged, and the state the
    /// drag began from.
    CropGrip m_sliceGrip = CropGrip::None;
    QRectF m_sliceStartRect;
    QPointF m_sliceStartDoc;

    // -- annotations --
    EyedropperType m_eyedropperType = EyedropperType::Eyedropper;
    /// CS6's default healing type.
    HealType m_healType = HealType::ContentAware;
    HealingType m_healingType = HealingType::SpotHealing;
    /// True while the Mixer Brush is the active tool, and while one of its
    /// strokes is in progress.
    bool m_mixerMode = false;
    bool m_mixing = false;

    /// True while the Color Replacement Brush is the active tool, and while one
    /// of its strokes is in progress.
    bool m_replaceMode = false;
    bool m_replacing = false;
    /// The Healing Brush's Alt-clicked source, in document coordinates.
    QPointF m_healSource;
    bool m_healSourceValid = false;

    /// True while one of the six retouch tools is active, and while one of their
    /// strokes is in progress.
    bool m_retouchMode = false;
    bool m_retouching = false;

    // -- Gradient --
    GradientTool m_gradientTool = GradientTool::Gradient;
    /// The drag that defines the ramp's axis, live only while dragging.
    bool m_gradientDragging = false;
    QPointF m_gradientStart;
    QPointF m_gradientEnd;

    // -- Pen / Path Selection --
    PenTool m_penTool = PenTool::Pen;
    PathSelectTool m_pathSelectTool = PathSelectTool::PathSelection;
    bool m_penAutoAddDelete = PenDefaults::kAutoAddDelete;
    bool m_penRubberBand = PenDefaults::kRubberBand;
    double m_freeformTolerance = PenDefaults::kFreeformTolerance;
    /// The cursor's last known document position while the Pen tool is
    /// active, for the Rubber Band preview — tracked on every hover, not only
    /// while dragging.
    QPointF m_penHoverDoc;

    /// What a Pen-tool press is in the middle of, so the matching move/release
    /// know what to do. `PlacingHandle` covers the ordinary case: a fresh
    /// corner was just appended, and a drag before release turns it smooth.
    enum class PenGesture { None, PlacingHandle, ConvertHandle, ConvertNewHandles, Freeform };
    PenGesture m_penGesture = PenGesture::None;
    int m_penSubpath = -1;
    int m_penPoint = -1;
    int m_penHandleSide = -1;
    /// Where the current Pen-tool press started, to tell a Convert Point click
    /// from a Convert Point drag.
    QPointF m_penPressDoc;
    /// The raw drag trail for the Freeform Pen, simplified into anchors on
    /// release.
    QPolygonF m_freeformPoints;

    /// What a Path Selection / Direct Selection press grabbed.
    enum class PathSelectGesture { None, Subpath, Anchor, Handle };
    PathSelectGesture m_pathSelectGesture = PathSelectGesture::None;
    int m_pathSelectSubpath = -1;
    int m_pathSelectPoint = -1;
    int m_pathSelectHandleSide = -1;
    /// The subpath drag's last position, since `pathMoveSubpath` takes a delta
    /// rather than an absolute position.
    QPointF m_pathSelectLastDoc;

    // -- Clone Stamp --
    /// The Alt-clicked source point, and whether one has been set. Held here as
    /// well as in the engine so the overlay can draw it.
    QPointF m_cloneSource;
    bool m_cloneSourceValid = false;
    bool m_cloneAligned = CloneDefaults::kAligned;
    CloneSampling m_cloneSampling = CloneDefaults::kSampling;
    /// True while one of the region healing tools is tracing its outline; it
    /// borrows the lasso's freehand path.
    bool m_healingTracing = false;
    /// The Patch / Content-Aware Move drag: where it started and where it is.
    bool m_regionDragging = false;
    QPointF m_regionDragStart;
    QPointF m_regionDragNow;
    bool m_camExtend = false;
    int m_camStructure = CamDefaults::kStructure;
    int m_camColor = CamDefaults::kColor;
    bool m_camSampleAllLayers = false;
    bool m_patchContentAware = false;
    bool m_patchDestination = false;
    bool m_patchTransparent = false;
    /// The Red Eye tool's rectangle while it is being dragged.
    QRectF m_redEyeRect;
    bool m_redEyeDragging = false;
    int m_pupilSize = RedEyeDefaults::kPupilSize;
    int m_darkenAmount = RedEyeDefaults::kDarkenAmount;
    /// Cached marker positions, indexed by MarkerKind.
    QList<QPointF> m_markers[3];
    /// The ruler's two endpoints, empty when there is none.
    QPolygonF m_ruler;
    /// Marker being dragged, or -1, and which end of the ruler (0, 1 or -1).
    int m_draggedMarker = -1;
    int m_draggedRulerEnd = -1;
    /// Modifiers held when the selection drag began. Photoshop samples these
    /// at press, so letting go of Shift mid-drag does not change the mode.
    Qt::KeyboardModifiers m_gestureModifiers = Qt::NoModifier;

    /// Animation phase for the marching ants.
    int m_antsOffset = 0;
    /// The selection contour in document coordinates, one subpath per loop.
    /// Cached; rebuilt only by `refreshSelection()`.
    QPainterPath m_selectionPath;
};

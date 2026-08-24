#pragma once

#include <QColor>
#include <QFont>
#include <QHash>
#include <QImage>
#include <QList>
#include <QPainterPath>
#include <QPoint>
#include <QPointF>
#include <QPolygonF>
#include <QTransform>
#include <QWidget>

#include "../tools/ToolId.h"

class Engine;
class QScrollBar;

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

    /// Turn the whole view, the way the Rotate View tool does — the canvas is
    /// rotated on screen and nothing about the image changes. Degrees
    /// clockwise; 0 is upright.
    void setViewRotation(double degrees);
    double viewRotation() const { return m_viewRotation; }

    /// Scale so the whole document fits, then centre it (Ctrl+0).
    void fitToWindow();

    /// Jump to 100% (Ctrl+1).
    void actualPixels();

    void zoomIn();
    void zoomOut();

    /// The tool that receives mouse input.
    void setActiveTool(ToolId tool);
    ToolId activeTool() const { return m_tool; }

    /// Current brush diameter, used for the brush-circle cursor.
    void setBrushSize(double size);

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

    /// What a dragged shape commits as — CS6's Mode menu.
    void setShapeMode(ShapeMode mode) { m_shapeMode = mode; }
    ShapeMode shapeMode() const { return m_shapeMode; }

    /// Which of the three tools the Eraser button currently holds. The plain
    /// Eraser strokes like a brush; the Background Eraser erases by colour per
    /// dab, and the Magic Eraser erases a region on one click.
    void setEraserType(EraserType type);
    EraserType eraserType() const { return m_eraserType; }

    /// The Background Eraser's options bar: Sampling (0 Continuous, 1 Once,
    /// 2 Background Swatch), Limits (0 Discontiguous, 1 Contiguous, 2 Find
    /// Edges), Tolerance as a percentage, and Protect Foreground Color.
    void setBackgroundEraseOptions(int sampling, int limits, int tolerance,
                                   bool protectForeground);
    /// The Magic Eraser's: Tolerance 0-255, Anti-alias, Contiguous, Sample All
    /// Layers and Opacity as a percentage.
    void setMagicEraseOptions(int tolerance, bool antialias, bool contiguous,
                              bool sampleAllLayers, int opacity);

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

    /// Which of the two tools the Hand button currently holds. Rotate View
    /// turns the canvas on screen instead of sliding it.
    void setHandTool(HandTool tool);
    HandTool handTool() const { return m_handTool; }

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

    /// Which of the two tools the Clone Stamp button currently holds. The
    /// Pattern Stamp paints a repeating pattern instead of sampled pixels, so
    /// it needs no Alt-clicked source.
    void setCloneTool(CloneType tool);
    CloneType cloneTool() const { return m_cloneTool; }

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

    /// Channel visibility mask from the Channels panel.
    /// Bits: 0=Red/Cyan, 1=Green/Magenta, 2=Blue/Yellow, 3=Black(K).
    /// 0xFF (default) = all visible. The composite channel's eye toggles
    /// all bits at once; individual channel eyes toggle one bit each.
    void setChannelMask(uint8_t mask);
    uint8_t channelMask() const { return m_channelMask; }

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

    /// The composited colour under a point on *screen*, or an invalid colour
    /// if that point is not over the image.
    ///
    /// For the Color Picker's eyedropper, which samples while a modal dialog
    /// holds the mouse and so only has global coordinates to go on. It reads
    /// the document rather than the screen, so what it picks is unaffected by
    /// zoom, the transparency checkerboard or any overlay drawn on top.
    QColor colorAtGlobal(const QPoint &globalPos) const;

    /// Convert a widget point to document space.
    QPointF widgetToDocument(const QPointF &pos) const;
    /// A movement measured on screen, turned back into the frame the pan is
    /// kept in. The identity while the view is upright.
    QPointF uprightDelta(const QPointF &screenDelta) const;
    /// The angle, in degrees, from the middle of the viewport to a point in
    /// it — what a Rotate View drag follows.
    double angleToPointer(const QPointF &widgetPos) const;
    /// The view rotation as a transform about the widget's centre, which is
    /// what everything drawn in unrotated widget space has to be drawn under.
    /// The identity while the view is upright.
    QTransform viewTransform() const;
    /// Convert a document point to widget space.
    QPointF documentToWidget(const QPointF &pos) const;

    /// The Type options bar's settings: font, the style name it was chosen by
    /// ("Bold Italic" and so on — kept alongside the resolved font because it
    /// is what the type record stores), colour, paragraph alignment and whether
    /// to antialias. Live — there is no notion of a partial text selection
    /// here, so changing any of these while text is being composed restyles the
    /// whole thing, not just what is typed from here on the way real Photoshop
    /// would with nothing selected.
    void setTypeOptions(const QFont &font, const QString &styleName, const QColor &color,
                        Qt::Alignment alignment, bool antialias);
    /// True while the Type tool has an edit in progress.
    bool isTyping() const { return m_typing; }
    /// Switch between the Horizontal and Vertical Type tools. Vertical type
    /// stacks characters downward and starts each new line as a column to the
    /// left of the last. Any edit in progress is committed first: the two are
    /// separate tools in Photoshop, not a setting on one piece of text.
    void setTypeVertical(bool vertical);
    /// Switch between the Type tools and the Type Mask tools. Mask type is
    /// composed the same way but commits as a *selection* cut to the shape of
    /// the letters, leaving no layer behind — so while it is being typed the
    /// canvas wears the rubylith veil Quick Mask uses, with the text knocked
    /// out of it. Any edit in progress is committed first.
    void setTypeMask(bool mask);
    /// Show a yellow highlight over matched text on the canvas.
    void setSearchHighlight(int layerIndex, int charOffset, int charLength);
    void clearSearchHighlight();

    enum class TransformMode { Free, Scale, Rotate, Skew, Distort, Perspective, Warp };

    /// Enter Free Transform mode on the active layer.
    void beginFreeTransform(TransformMode mode = TransformMode::Free);
    void commitFreeTransform();
    void cancelFreeTransform();
    bool isFreeTransforming() const { return m_freeTransform; }
    TransformMode transformMode() const { return m_ftMode; }
    QRectF transformBounds() const { return m_ftBounds; }
    QRectF transformOrigBounds() const { return QRectF(m_ftOrigOffset, QSizeF(m_ftOrigImage.width(), m_ftOrigImage.height())); }
    double transformRotation() const { return m_ftRotation; }
    QPolygonF transformQuad() const { return m_ftQuad; }

    /// Rasterize the composed text into its layer: the one being re-edited, or
    /// a new one. Does nothing if the Type tool is not mid-edit.
    void commitTypeEdit();
    /// Abandon the in-progress edit without adding a layer. Safe to call at
    /// any time, typing or not.
    void cancelTypeEdit();

signals:
    /// Emitted as the cursor moves, for the status bar and Info panel.
    void cursorMoved(const QPointF &documentPos);
    /// The cursor left the canvas, so readouts of what is under it should
    /// blank rather than hold their last value.
    void cursorLeft();
    /// Emitted whenever the zoom factor changes.
    void zoomChanged(double zoom);
    /// Emitted whenever the view is turned, so the Rotate View tool's angle
    /// field follows a drag.
    void viewRotationChanged(double degrees);
    /// The user picked a colour with the eyedropper.
    void colorPicked(const QColor &color);
    /// Right-click with a selection tool active. `globalPos` is where the
    /// menu should open. The canvas does not build the menu itself: the
    /// commands on it belong to the registry, which `MainWindow` owns.
    void contextMenuRequested(const QPoint &globalPos);
    /// Right-click with the Zoom tool active. Like `contextMenuRequested`, the
    /// menu itself is MainWindow's to build.
    void zoomContextMenuRequested(const QPoint &globalPos);
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
    void transformStarted();
    void transformCommitted();
    void transformCancelled();
    void transformChanged();
    /// The Type tool reopened existing text, whose own font, colour, alignment
    /// and antialiasing are now what is being edited. `MainWindow` adopts them
    /// into the options bar, the way clicking into text in Photoshop makes the
    /// bar describe *that* text rather than what was last set up.
    void typeStyleAdopted(const QString &family, const QString &style, qreal pointSize,
                          const QColor &color, Qt::Alignment alignment, bool antialias,
                          bool vertical);
    /// A mixer stroke ended, so the paint on the brush has changed. The options
    /// bar's load swatch reads it back from the engine.
    void mixerLoadChanged();

protected:
    /// Steals `QEvent::ShortcutOverride` while text is being composed, so a
    /// letter typed into the Type tool is not matched against the single-
    /// letter tool shortcuts (registered as `QAction`s on the main window)
    /// before `keyPressEvent` ever sees it. Without this, typing "e" would
    /// switch to the Eraser instead of inserting the character.
    bool event(QEvent *event) override;
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
    bool promptRasterizeIfType();
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

    /// Handle a Type tool click. Inside the text being composed it places the
    /// caret and begins a drag-selection; inside other text it reopens that
    /// layer; anywhere else it commits what was being typed and starts new
    /// text. `modifiers` carries Shift, which extends the selection.
    void typePress(const QPointF &doc, Qt::KeyboardModifiers modifiers);
    /// Put down the empty type layer a click with the Type tool makes, and
    /// return its panel index — or -1 if it could not be made.
    int createEmptyTypeLayer();
    /// Reopen the type layer at a panel index: its text becomes the edit in
    /// progress and its own font, colour and alignment become the Type tool's.
    /// The caret lands where in the text `doc` fell.
    void beginTypeEdit(int layerIndex, const QPointF &doc);
    /// Draw the outline of the shape being dragged out, so the user sees what
    /// they are about to commit.
    void paintSearchHighlight(QPainter &painter);
    void paintFreeTransform(QPainter &painter);
    void paintShapeOverlay(QPainter &painter);
    /// Draw the rectangle a Zoom drag is marking out.
    void paintZoomOverlay(QPainter &painter);
    /// The two-tone dashed outline a pending gesture is drawn with — nothing
    /// has been committed until the button comes up, and this is what says so.
    void paintPendingOutline(QPainter &painter, const QPolygonF &widgetOutline) const;
    /// Zoom so a document-space rectangle fills the viewport, and centre it.
    void zoomToRect(const QRectF &docRect);
    /// The outline a drag to `doc` marks out, asked of the engine. The
    /// modifiers go with it: what Shift and Alt mean depends on the tool, and
    /// the engine is where that is decided.
    QPolygonF shapeOutlineFor(const QPointF &doc, Qt::KeyboardModifiers modifiers) const;


    /// Draw the text composed so far, its selection and its insertion caret.
    void paintTypeOverlay(QPainter &painter);
    /// The bounding rectangle of the composed text in document coordinates,
    /// anchored at `m_typeOrigin` per `m_typeAlignment`.
    QRectF typeBounds() const;

    /// A stretch of the text being composed that is set the same way — the
    /// character run Photoshop formats in. Selecting two letters and changing
    /// the size splits the run they were in and gives the middle piece its own.
    struct TypeRun
    {
        int length = 0;
        QString family;
        QString style;
        qreal size = 12.0;
        QColor color = Qt::black;

        bool sameStyle(const TypeRun &other) const
        {
            return family == other.family && style == other.style
                && qFuzzyCompare(size, other.size) && color == other.color;
        }
    };

    /// One stretch of one line, in one font: what a single `drawText` call
    /// draws. `x` and `y` place it within its line, and `ascent` is the drop
    /// from there to its baseline.
    ///
    /// Horizontal type puts a whole run in one segment; vertical type gives
    /// each character its own, since they are stacked rather than shaped into a
    /// row.
    struct TypeSegment
    {
        int start = 0;
        int length = 0;
        QFont font;
        QColor color;
        qreal x = 0.0;
        qreal y = 0.0;
        qreal ascent = 0.0;
        qreal width = 0.0;
        qreal height = 0.0;
    };

    /// One line of the text — a column, for vertical type — with its segments
    /// in order. `x` and `top` are offsets from the origin and already carry
    /// the alignment.
    struct TypeLineBox
    {
        int start = 0;
        int length = 0;
        qreal x = 0.0;
        qreal top = 0.0;
        qreal height = 0.0;
        qreal width = 0.0;
        QList<TypeSegment> segments;
    };

    /// Where every character of the text being composed sits, relative to
    /// `m_typeOrigin`.
    ///
    /// The caret, the selection highlight, click-to-place-caret, the bounding
    /// box and the committed rasterization all have to agree with the glyphs
    /// and with each other, so they are all measured from this one description.
    /// It is built at a scale: 1 for document space — the size the text is
    /// rasterized at — and the zoom factor for what is drawn on screen.
    struct TypeLayout
    {
        QList<TypeLineBox> lines;
        /// Everything the text covers, relative to the origin.
        QRectF box;
        qreal scale = 1.0;
        /// Which way the text runs, copied from `m_typeVertical` when it was
        /// built so everything measured against it agrees.
        bool vertical = false;
    };
    TypeLayout typeLayout(qreal scale) const;
    /// How far a line or column is slid along by the alignment: nothing for
    /// left/top, half its extent for centred, all of it for right/bottom.
    qreal typeAlignOffset(qreal extent) const;
    /// Which line a character index falls on.
    int typeLineOf(const TypeLayout &layout, int index) const;
    /// How far along its line the gap before `index` sits: rightwards for
    /// horizontal type, downwards for vertical.
    qreal typeFlowOffset(const TypeLayout &layout, int line, int index) const;
    /// The caret at a character index, relative to the origin — a thin upright
    /// bar between letters, or a flat one between stacked characters.
    QRectF typeCaretRect(const TypeLayout &layout, int index) const;
    /// The rectangle covering `[from, to)` of one line, for the selection
    /// highlight. Both ends must already be clamped to that line.
    QRectF typeRangeRect(const TypeLayout &layout, int line, int from, int to) const;
    /// The character index nearest a point given as an offset from the origin —
    /// where a click there should put the caret.
    int typeIndexAt(const QPointF &widgetPos) const;
    /// The font a run is set in, resolved through the font database and cached:
    /// laying the text out asks for the same few fonts over and over.
    QFont typeRunFont(const TypeRun &run, qreal scale) const;
    /// Draw a laid-out block of text, segment by segment, with each run's own
    /// font and colour — or all in `forcedColor` when one is given, which is how
    /// the inverted selection is drawn. `origin` is where `m_typeOrigin` falls
    /// in whatever the painter is drawing on: the widget for the overlay, the
    /// image being rasterized for a commit.
    void paintTypeRuns(QPainter &painter, const TypeLayout &layout, const QPointF &origin,
                       const QColor &forcedColor = QColor()) const;
    /// Draw mask type: the rubylith veil over the document with the letters
    /// knocked out of it, which is how Photoshop previews the selection the
    /// text is about to become.
    void paintTypeMaskVeil(QPainter &painter, const TypeLayout &layout,
                           const QPointF &origin) const;
    /// Rasterize the text being composed into `image`, whose top-left is at
    /// `imageOrigin` in document space. `forcedColor` overrides every run's own
    /// colour, which is what the mask commit wants — there it is the alpha that
    /// matters, not the ink.
    void renderTypeToImage(QImage &image, const QPoint &imageOrigin,
                           const QColor &forcedColor = QColor()) const;

    /// The run covering a character index, and the run the next character typed
    /// at the caret should join.
    int typeRunIndexAt(int index) const;
    TypeRun typeRunAt(int index) const;
    /// The style the options bar currently holds, as a run.
    TypeRun typePendingRun() const;
    /// Give `[from, to)` the options bar's current style, splitting runs at the
    /// edges of the range so nothing outside it changes.
    void typeApplyStyle(int from, int to);
    /// Keep the runs consistent with the text: no empty runs, no two adjacent
    /// runs set the same way, and lengths that add up to the text's.
    void typeNormalizeRuns();
    /// Make the options bar describe the text at the caret, so moving into a
    /// differently-set word updates it — but only when that is unambiguous:
    /// with a mixed selection there is no one style to show.
    void typeSyncStyleToCaret();

    /// True when some of the text is selected rather than just a caret.
    bool typeHasSelection() const { return m_typeCaret != m_typeAnchor; }
    int typeSelectionStart() const { return qMin(m_typeCaret, m_typeAnchor); }
    int typeSelectionEnd() const { return qMax(m_typeCaret, m_typeAnchor); }
    /// Put the caret at `index`, keeping the selection's other end where it is
    /// when `extend` is set (Shift) and collapsing the selection when it is not.
    void typeMoveCaret(int index, bool extend);
    /// Delete the selected characters. False if there was no selection.
    bool typeDeleteSelection();
    /// Delete `length` characters from `at`, taking them off the runs too.
    void typeRemove(int at, int length);
    /// Insert at the caret, replacing the selection if there is one.
    void typeInsert(const QString &text);
    /// Select the word around `index`, as a double-click does.
    void typeSelectWord(int index);
    /// Every key press while an edit is open belongs to the text: typing,
    /// caret movement, selection and the two keys that end the edit.
    void typeKeyPress(QKeyEvent *event);

    Engine *m_engine = nullptr;

    /// Cached composite. Refreshed from the engine, never edited here.
    QImage m_image;

    /// Channel visibility bitmask. 0xFF = all visible.
    uint8_t m_channelMask = 0xFF;

    double m_zoom = 1.0;
    /// Degrees the view is turned by, clockwise. Purely a way of looking at
    /// the document: no pixel, layer or coordinate in the engine knows about
    /// it, and it is not saved with the file.
    double m_viewRotation = 0.0;
    /// The angle the Rotate View drag started from, and the pointer's angle at
    /// that moment — the drag turns the view by the difference.
    double m_rotateStartAngle = 0.0;
    double m_rotateStartRotation = 0.0;
    bool m_rotatingView = false;
    HandTool m_handTool = HandTool::Hand;

    /// The Zoom tool's marquee: where the drag began, and the rectangle it has
    /// marked out so far, both in document space. Empty unless a drag is in
    /// progress.
    bool m_zoomDragging = false;
    QPointF m_zoomStartDoc;
    QRectF m_zoomRectDoc;
    /// Pan offset in widget pixels, from the centred position.
    QPointF m_pan{0.0, 0.0};

    QScrollBar *m_hScroll = nullptr;
    QScrollBar *m_vScroll = nullptr;
    bool m_scrollBarUpdating = false;
    void syncScrollBars();
    void layoutScrollBars();

    ToolId m_tool = ToolId::Brush;
    double m_brushDiameter = 20.0;
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

    // -- Free Transform --
    bool m_freeTransform = false;
    TransformMode m_ftMode = TransformMode::Free;
    int m_ftLayerIndex = -1;
    QImage m_ftOrigImage;
    QPointF m_ftOrigOffset;
    QRectF m_ftBounds;
    double m_ftRotation = 0.0;
    QPointF m_ftScale{1.0, 1.0};
    QPolygonF m_ftQuad;
    enum class FTHandle { None, Move, TopLeft, Top, TopRight, Right,
                          BottomRight, Bottom, BottomLeft, Left, Rotate };
    FTHandle m_ftHandle = FTHandle::None;
    QPointF m_ftDragStart;
    QRectF m_ftDragStartBounds;
    double m_ftDragStartRotation = 0.0;
    QPolygonF m_ftDragStartQuad;

    // Warp: 4x4 bicubic Bezier control points (row-major).
    // [0][0]=TL corner, [0][3]=TR, [3][0]=BL, [3][3]=BR.
    // [0][1],[0][2] = top edge tangents, etc.
    QPointF m_warpPts[4][4];
    QPointF m_warpPtsDragStart[4][4];
    int m_warpDragI = -1, m_warpDragJ = -1;

    // -- Search highlight --
    int m_searchHighlightLayer = -1;
    int m_searchHighlightChar = -1;
    int m_searchHighlightLen = 0;

    // -- Type --
    /// True while text is being composed on the canvas, between a click with
    /// the Type tool and Enter/the checkmark committing it or Esc cancelling.
    bool m_typing = false;
    /// Where the click landed, in document coordinates — the anchor the text
    /// grows from. Which edge of the text sits there depends on
    /// `m_typeAlignment`.
    QPointF m_typeOrigin;
    /// The text composed so far. Lines are separated by '\n'.
    QString m_typeText;
    /// How that text is formatted, in order. The lengths add up to the text's
    /// own — `typeNormalizeRuns` keeps that true after every edit.
    QList<TypeRun> m_typeRuns;
    /// Resolved fonts, keyed by family, style and size. Cleared with the edit.
    mutable QHash<QString, QFont> m_typeFontCache;
    /// Where the caret sits, as a character index into `m_typeText`, and where
    /// the selection it may be dragging out started. Equal means no selection —
    /// the two are the ends of the selected range, and which is which depends
    /// on whether it was made forwards or backwards.
    int m_typeCaret = 0;
    int m_typeAnchor = 0;
    /// True between pressing and releasing while sweeping out a selection.
    bool m_typeSelecting = false;
    QFont m_typeFont;
    /// The style name `m_typeFont` was resolved from — see `setTypeOptions`.
    QString m_typeStyleName = QStringLiteral("Regular");
    QColor m_typeColor = Qt::black;
    Qt::Alignment m_typeAlignment = Qt::AlignLeft;
    bool m_typeAntialias = true;
    /// Set by the Vertical Type tool, and taken on from a layer reopened with
    /// either — the orientation belongs to the text, not to the tool in hand.
    bool m_typeVertical = false;
    /// Set by the two Type Mask tools: this edit becomes a selection, not a
    /// layer.
    bool m_typeMask = false;
    /// Panel index of the type layer being re-edited, or -1 when the edit will
    /// commit as a new layer. Clicking existing text with the Type tool reopens
    /// it, and committing then re-renders that layer in place instead of
    /// stacking a second copy of the text on top of it.
    int m_typeLayer = -1;
    /// Whether that layer was put down by this edit. A layer made by clicking
    /// with the Type tool goes away again if the edit is abandoned; one that
    /// was reopened stays exactly as it was.
    bool m_typeLayerIsNew = false;

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
    /// The Shape tools' Mode, and the outline being dragged out — empty unless
    /// a drag is in progress. The outline comes from the engine rather than
    /// being worked out here, so what is previewed is what will land.
    ShapeMode m_shapeMode = ShapeDefaults::kMode;
    QPolygonF m_shapeOutline;
    bool m_shapeDragging = false;

    /// Which eraser is in hand, and the two colour erasers' settings.
    EraserType m_eraserType = EraserType::Eraser;
    int m_bgEraseSampling = BackgroundEraseDefaults::kSampling;
    int m_bgEraseLimits = BackgroundEraseDefaults::kLimits;
    int m_bgEraseTolerance = BackgroundEraseDefaults::kTolerance;
    bool m_bgEraseProtectForeground = BackgroundEraseDefaults::kProtectForeground;
    int m_magicEraseTolerance = MagicEraseDefaults::kTolerance;
    bool m_magicEraseAntialias = MagicEraseDefaults::kAntialias;
    bool m_magicEraseContiguous = MagicEraseDefaults::kContiguous;
    bool m_magicEraseSampleAll = MagicEraseDefaults::kSampleAllLayers;
    int m_magicEraseOpacity = MagicEraseDefaults::kOpacity;
    /// True while a Background Eraser drag is in progress.
    bool m_backgroundErasing = false;

    CloneType m_cloneTool = CloneType::CloneStamp;
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

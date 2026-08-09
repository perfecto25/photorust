#include "CanvasView.h"

#include "cxx-qt-lib/qcolor.h"
#include "photorust_core/src/bridge.cxxqt.h"

#include <QContextMenuEvent>
#include <QGuiApplication>
#include <algorithm>
#include <QEnterEvent>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QPainter>
#include <QResizeEvent>
#include <QTransform>
#include <QWheelEvent>
#include <QtMath>

namespace {

/// Zoom stops, matching the sequence CS6's View menu steps through.
const double kZoomSteps[] = {0.0067, 0.01, 0.0167, 0.025, 0.0333, 0.05, 0.0667,
                             0.10,   0.125, 0.1667, 0.25, 0.3333, 0.50, 0.6667,
                             1.0,    2.0,   3.0,    4.0,  5.0,    6.0,  7.0,
                             8.0,    12.0,  16.0,   32.0};
constexpr int kZoomStepCount = sizeof(kZoomSteps) / sizeof(kZoomSteps[0]);

constexpr double kMinZoom = 0.0067; //  0.67%
constexpr double kMaxZoom = 32.0;   // 3200%

/// Side of one checkerboard square, in widget pixels. Fixed on screen rather
/// than in document space, exactly as Photoshop draws it.
constexpr int kCheckerSize = 8;

} // namespace

CanvasView::CanvasView(Engine *engine, QWidget *parent)
    : QWidget(parent)
    , m_engine(engine)
{
    setObjectName(QStringLiteral("canvasSurround"));
    setFocusPolicy(Qt::StrongFocus);
    setMouseTracking(true);
    setAttribute(Qt::WA_OpaquePaintEvent);

    refresh();
}

void CanvasView::refresh()
{
    if (m_engine) {
        // The engine hands back a QImage that borrows a Rust-owned buffer; it
        // is cheap to take because QImage is implicitly shared.
        m_image = m_engine->compositeImage();
    }
    update();
}

void CanvasView::refreshSelection()
{
    m_selectionPath = QPainterPath();

    if (m_engine && m_engine->hasSelection()) {
        // The engine hands back the contour flattened as a run of loops, each
        // prefixed by its point count (see bridge.rs::selection_outline).
        const rust::Vec<::std::int32_t> flat = m_engine->selectionOutline();
        for (std::size_t i = 0; i + 1 <= flat.size();) {
            const int count = flat[i++];
            if (count <= 0 || i + std::size_t(count) * 2 > flat.size()) {
                break;
            }
            m_selectionPath.moveTo(flat[i], flat[i + 1]);
            for (int p = 1; p < count; ++p) {
                m_selectionPath.lineTo(flat[i + p * 2], flat[i + p * 2 + 1]);
            }
            m_selectionPath.closeSubpath();
            i += std::size_t(count) * 2;
        }
    }
    update();
}

// ---------------------------------------------------------------- geometry --

QPointF CanvasView::documentOrigin() const
{
    const double w = m_image.width() * m_zoom;
    const double h = m_image.height() * m_zoom;
    // Centre the document, then apply the pan offset.
    return QPointF((width() - w) / 2.0 + m_pan.x(), (height() - h) / 2.0 + m_pan.y());
}

QRectF CanvasView::documentRect() const
{
    const QPointF origin = documentOrigin();
    return QRectF(origin, QSizeF(m_image.width() * m_zoom, m_image.height() * m_zoom));
}

QPointF CanvasView::widgetToDocument(const QPointF &pos) const
{
    const QPointF origin = documentOrigin();
    return QPointF((pos.x() - origin.x()) / m_zoom, (pos.y() - origin.y()) / m_zoom);
}

QPointF CanvasView::documentToWidget(const QPointF &pos) const
{
    const QPointF origin = documentOrigin();
    return QPointF(pos.x() * m_zoom + origin.x(), pos.y() * m_zoom + origin.y());
}

void CanvasView::clampPan()
{
    // Allow panning until only a sliver of the document remains visible, so it
    // can never be lost entirely off-screen.
    const double marginX = width() / 2.0 + m_image.width() * m_zoom / 2.0 - 32.0;
    const double marginY = height() / 2.0 + m_image.height() * m_zoom / 2.0 - 32.0;

    m_pan.setX(qBound(-qMax(marginX, 0.0), m_pan.x(), qMax(marginX, 0.0)));
    m_pan.setY(qBound(-qMax(marginY, 0.0), m_pan.y(), qMax(marginY, 0.0)));
}

// -------------------------------------------------------------------- zoom --

void CanvasView::setZoom(double zoom)
{
    setZoomAt(zoom, QPointF(width() / 2.0, height() / 2.0));
}

void CanvasView::setZoomAt(double zoom, const QPointF &focusWidgetPos)
{
    const double next = qBound(kMinZoom, zoom, kMaxZoom);
    if (qFuzzyCompare(next, m_zoom)) {
        return;
    }

    // Remember which document pixel is under the focus point, then move the pan
    // so that same pixel stays there after the scale change.
    const QPointF docBefore = widgetToDocument(focusWidgetPos);
    m_zoom = next;
    const QPointF widgetAfter = documentToWidget(docBefore);
    m_pan += focusWidgetPos - widgetAfter;

    clampPan();
    emit zoomChanged(m_zoom);
    update();
}

void CanvasView::zoomIn()
{
    for (int i = 0; i < kZoomStepCount; ++i) {
        if (kZoomSteps[i] > m_zoom * 1.001) {
            setZoom(kZoomSteps[i]);
            return;
        }
    }
    setZoom(kMaxZoom);
}

void CanvasView::zoomOut()
{
    for (int i = kZoomStepCount - 1; i >= 0; --i) {
        if (kZoomSteps[i] < m_zoom * 0.999) {
            setZoom(kZoomSteps[i]);
            return;
        }
    }
    setZoom(kMinZoom);
}

void CanvasView::fitToWindow()
{
    if (m_image.isNull() || m_image.width() == 0 || m_image.height() == 0) {
        return;
    }
    // Leave a small margin so the document does not touch the window edge.
    const double margin = 24.0;
    const double sx = (width() - margin) / double(m_image.width());
    const double sy = (height() - margin) / double(m_image.height());

    m_pan = QPointF(0.0, 0.0);
    setZoom(qMin(sx, sy));
    update();
}

void CanvasView::actualPixels()
{
    m_pan = QPointF(0.0, 0.0);
    setZoom(1.0);
}

// ------------------------------------------------------------------- tools --

void CanvasView::setActiveTool(ToolId tool)
{
    if (m_tool == tool) {
        return;
    }
    const ToolId previous = m_tool;
    m_tool = tool;

    // Switching tools mid-gesture would leave the engine holding a half-built
    // stroke, so end the gesture cleanly first.
    if (m_replacing && m_engine) {
        m_engine->cancelReplace();
        m_replacing = false;
    }
    if (m_mixing && m_engine) {
        m_engine->cancelMixer();
        m_mixing = false;
    }
    m_gradientDragging = false;
    if (m_retouching && m_engine) {
        m_engine->cancelRetouchStroke();
        m_retouching = false;
    }
    if (m_dragging && m_engine) {
        m_engine->cancelStroke();
    }
    m_dragging = false;
    // Leaving the Pen tool finishes whatever subpath was open rather than
    // discarding it — switching tools is not a way to lose your place, only
    // Esc is. Path Selection's drag, having no pending data of its own, just
    // needs its gesture state cleared.
    if (m_engine && previous == ToolId::Pen) {
        m_engine->pathFinishEditing();
    }
    m_penGesture = PenGesture::None;
    m_pathSelectGesture = PathSelectGesture::None;
    m_freeformPoints.clear();
    // An outline left open on the old tool is discarded, not committed —
    // switching tools is not a way to close a shape.
    cancelLasso();
    finishQuickSelect();

    // Picking up the Crop tool opens a box over the whole image, as CS6 does;
    // putting it down abandons whatever box was pending.
    m_sliceDragging = false;
    m_sliceDrag = QRectF();
    m_sliceGrip = CropGrip::None;
    m_healingTracing = false;
    m_regionDragging = false;
    m_redEyeDragging = false;
    m_redEyeRect = QRectF();

    m_draggedMarker = -1;
    m_draggedRulerEnd = -1;
    if (tool == ToolId::Eyedropper) {
        refreshAnnotations();
    }

    if (tool == ToolId::Crop) {
        resetCrop();
        refreshSlices();
    } else {
        m_cropRect = QRectF();
        m_cropGrip = CropGrip::None;
        m_cropQuad.clear();
    }

    if (m_engine) {
        m_engine->setEraseMode(tool == ToolId::Eraser);
        // Healing runs through the same stroke path as a brush; this is what
        // tells the engine to rebuild the region at the end instead of filling
        // it with the foreground colour.
        m_engine->setHealMode(toolHeals(tool) ? static_cast<int>(m_healType) : -1);
    }
    updateCursor();
    update();
}

void CanvasView::setMarqueeType(MarqueeType type)
{
    if (m_marqueeType == type) {
        return;
    }
    m_marqueeType = type;
    m_marqueeActive = false;
    m_marquee = QRectF();
    update();
}

void CanvasView::setLassoType(LassoType type)
{
    if (m_lassoType == type) {
        return;
    }
    cancelLasso();
    m_lassoType = type;
    update();
}

void CanvasView::setMagneticOptions(int width, int contrast, int frequency)
{
    m_magneticWidth = qBound(1, width, 256);
    m_magneticContrast = qBound(1, contrast, 100);
    m_magneticFrequency = qBound(0, frequency, 100);
}

void CanvasView::setCropOptions(double aspectRatio, bool deleteCropped)
{
    m_cropDeletePixels = deleteCropped;
    if (qFuzzyCompare(m_cropRatio, aspectRatio)) {
        return;
    }
    m_cropRatio = aspectRatio > 0.0 ? aspectRatio : 0.0;
    // Snap the box to the new ratio straight away rather than waiting for the
    // next drag, which is what CS6 does when you pick a preset.
    if (m_tool == ToolId::Crop && m_cropRatio > 0.0) {
        applyCropRatio(CropGrip::BottomRight);
        update();
    }
}

void CanvasView::setReplaceMode(bool active)
{
    if (m_replaceMode == active) {
        return;
    }
    if (m_replacing && m_engine) {
        m_engine->cancelReplace();
        m_replacing = false;
    }
    m_replaceMode = active;
}

void CanvasView::setMixerMode(bool active)
{
    if (m_mixerMode == active) {
        return;
    }
    if (m_mixing && m_engine) {
        m_engine->cancelMixer();
        m_mixing = false;
    }
    m_mixerMode = active;
}

QPointF CanvasView::constrainedGradientEnd(const QPointF &doc,
                                           Qt::KeyboardModifiers modifiers) const
{
    if (!modifiers.testFlag(Qt::ShiftModifier)) {
        return doc;
    }
    // Shift snaps the axis to 45° steps, as it does for every other drag in
    // Photoshop. The length is kept; only the direction is rounded.
    const QPointF delta = doc - m_gradientStart;
    const double length = std::hypot(delta.x(), delta.y());
    if (length < 1e-6) {
        return doc;
    }
    const double step = M_PI / 4.0;
    const double angle = std::round(std::atan2(delta.y(), delta.x()) / step) * step;
    return m_gradientStart + QPointF(std::cos(angle) * length, std::sin(angle) * length);
}

void CanvasView::paintGradientDrag(QPainter &painter)
{
    if (!m_gradientDragging) {
        return;
    }
    // CS6 shows the axis as a plain line with a marker at each end while you
    // drag, and draws nothing on the layer until the mouse is released.
    const QPointF a = documentToWidget(m_gradientStart);
    const QPointF b = documentToWidget(m_gradientEnd);

    painter.save();
    painter.setRenderHint(QPainter::Antialiasing, true);
    painter.setBrush(Qt::NoBrush);
    // White under black, so the line reads on any image.
    painter.setPen(QPen(Qt::white, 3.0));
    painter.drawLine(a, b);
    painter.setPen(QPen(Qt::black, 1.2));
    painter.drawLine(a, b);
    painter.drawEllipse(a, 3.0, 3.0);
    painter.drawEllipse(b, 3.0, 3.0);
    painter.restore();
}

void CanvasView::paintPathOverlay(QPainter &painter)
{
    if (!m_engine || !toolEditsPaths(m_tool)) {
        return;
    }
    const int subpathCount = m_engine->pathSubpathCount();
    if (subpathCount == 0) {
        return;
    }

    painter.save();
    painter.setRenderHint(QPainter::Antialiasing, true);

    // The curve itself: white under blue, the same two-pass trick the other
    // overlays use so it reads against any part of the image beneath it.
    QPainterPath curve;
    // Anchors and handles, collected while building the curve so both walk the
    // point data exactly once.
    QVector<QPointF> anchors;
    QVector<QPair<QPointF, QPointF>> handleLines; // anchor -> handle

    for (int sp = 0; sp < subpathCount; ++sp) {
        const int count = m_engine->pathAnchorCount(sp);
        if (count == 0) {
            continue;
        }
        const bool closed = m_engine->pathIsClosed(sp);

        struct Anchor {
            QPointF pos;
            bool hasIn = false, hasOut = false;
            QPointF in, out;
        };
        QVector<Anchor> points;
        points.reserve(count);
        for (int pt = 0; pt < count; ++pt) {
            const rust::Vec<float> a = m_engine->pathAnchorAt(sp, pt);
            if (a.size() != 9) {
                continue;
            }
            Anchor entry;
            entry.pos = documentToWidget(QPointF(a[0], a[1]));
            entry.hasIn = a[3] > 0.5f;
            entry.in = documentToWidget(QPointF(a[4], a[5]));
            entry.hasOut = a[6] > 0.5f;
            entry.out = documentToWidget(QPointF(a[7], a[8]));
            points.append(entry);
            anchors.append(entry.pos);
            if (entry.hasIn) {
                handleLines.append({entry.pos, entry.in});
            }
            if (entry.hasOut) {
                handleLines.append({entry.pos, entry.out});
            }
        }
        if (points.isEmpty()) {
            continue;
        }

        curve.moveTo(points[0].pos);
        const int segments = closed ? points.size() : points.size() - 1;
        for (int i = 0; i < segments; ++i) {
            const Anchor &a = points[i];
            const Anchor &b = points[(i + 1) % points.size()];
            const QPointF c1 = a.hasOut ? a.out : a.pos;
            const QPointF c2 = b.hasIn ? b.in : b.pos;
            curve.cubicTo(c1, c2, b.pos);
        }
    }

    painter.setBrush(Qt::NoBrush);
    painter.setPen(QPen(Qt::white, 2.6));
    painter.drawPath(curve);
    painter.setPen(QPen(QColor(0x2c, 0x6f, 0xd6), 1.1));
    painter.drawPath(curve);

    // Handles: thin lines from anchor to handle, then a small hollow circle.
    painter.setPen(QPen(QColor(0x2c, 0x6f, 0xd6), 1.0));
    for (const auto &line : handleLines) {
        painter.drawLine(line.first, line.second);
    }
    painter.setBrush(Qt::white);
    for (const auto &line : handleLines) {
        painter.drawEllipse(line.second, 2.6, 2.6);
    }

    // Anchors: small hollow squares, matching CS6's own anchor glyph.
    painter.setPen(QPen(QColor(0x2c, 0x6f, 0xd6), 1.2));
    painter.setBrush(Qt::white);
    for (const QPointF &p : anchors) {
        painter.drawRect(QRectF(p.x() - 2.5, p.y() - 2.5, 5.0, 5.0));
    }

    // Rubber Band: the segment about to be drawn, previewed live from the last
    // anchor to the cursor. Only the ordinary Pen tool has this, and only
    // while a subpath is actually open — the editing subpath is always the
    // last one, since a new one is only ever appended at the end.
    if (m_penTool == PenTool::Pen && m_penRubberBand && m_engine->pathIsEditing()
        && m_penGesture != PenGesture::Freeform) {
        const int lastSubpath = subpathCount - 1;
        const int count = m_engine->pathAnchorCount(lastSubpath);
        if (count > 0) {
            const rust::Vec<float> last = m_engine->pathAnchorAt(lastSubpath, count - 1);
            if (last.size() == 9) {
                const QPointF anchor(last[0], last[1]);
                const QPointF start = last[6] > 0.5f ? QPointF(last[7], last[8]) : anchor;
                QPainterPath preview(documentToWidget(anchor));
                preview.cubicTo(documentToWidget(start), documentToWidget(m_penHoverDoc),
                                documentToWidget(m_penHoverDoc));
                painter.setPen(QPen(QColor(0x2c, 0x6f, 0xd6), 1.0, Qt::DashLine));
                painter.drawPath(preview);
            }
        }
    }

    // Freeform Pen's own live trail.
    if (m_penGesture == PenGesture::Freeform && m_freeformPoints.size() > 1) {
        QPainterPath trail(documentToWidget(m_freeformPoints.first()));
        for (int i = 1; i < m_freeformPoints.size(); ++i) {
            trail.lineTo(documentToWidget(m_freeformPoints.at(i)));
        }
        painter.setPen(QPen(Qt::white, 2.6));
        painter.setBrush(Qt::NoBrush);
        painter.drawPath(trail);
        painter.setPen(QPen(QColor(0x2c, 0x6f, 0xd6), 1.1));
        painter.drawPath(trail);
    }

    painter.restore();
}

void CanvasView::forgetSampleSources()
{
    m_cloneSourceValid = false;
    m_healSourceValid = false;
    if (m_engine) {
        m_engine->clearCloneSource();
        m_engine->setHealSource(false, 0, 0);
    }
    update();
}

void CanvasView::setRetouchMode(bool active)
{
    if (m_retouchMode == active) {
        return;
    }
    if (m_retouching && m_engine) {
        m_engine->cancelRetouchStroke();
        m_retouching = false;
    }
    m_retouchMode = active;
}

void CanvasView::setGradientTool(GradientTool tool)
{
    if (m_gradientTool == tool) {
        return;
    }
    // Switching away mid-drag would leave an axis on screen that nothing will
    // ever draw.
    m_gradientDragging = false;
    m_gradientTool = tool;
    update();
}

void CanvasView::setPenTool(PenTool tool)
{
    m_penTool = tool;
    m_penGesture = PenGesture::None;
    m_freeformPoints.clear();
}

void CanvasView::setPathSelectTool(PathSelectTool tool)
{
    m_pathSelectTool = tool;
    m_pathSelectGesture = PathSelectGesture::None;
}

void CanvasView::setPenOptions(bool autoAddDelete, bool rubberBand)
{
    m_penAutoAddDelete = autoAddDelete;
    m_penRubberBand = rubberBand;
}

float CanvasView::pathHitRadius() const
{
    // The same fixed-screen-size idea as `nearLassoStart`: reachable at any
    // zoom, rather than shrinking to nothing zoomed out or ballooning zoomed
    // in.
    return float(8.0 / std::max(m_zoom, 0.01));
}

void CanvasView::penPress(const QPointF &doc)
{
    if (!m_engine) {
        return;
    }
    const float radius = pathHitRadius();

    switch (m_penTool) {
    case PenTool::Pen: {
        // Auto Add/Delete: hovering the already-finished part of the active
        // path adds or removes an anchor instead of starting a new one. Only
        // while nothing is currently being drawn — mid-subpath, a click always
        // extends it (or closes it, just below).
        if (m_penAutoAddDelete && !m_engine->pathIsEditing()) {
            const rust::Vec<::std::int32_t> anchor = m_engine->pathHitAnchor(doc.x(), doc.y(), radius);
            if (anchor.size() == 2) {
                m_engine->pathDeleteAnchor(anchor[0], anchor[1]);
                update();
                return;
            }
            const rust::Vec<float> segment = m_engine->pathHitSegment(doc.x(), doc.y(), radius);
            if (segment.size() == 3) {
                m_engine->pathInsertAnchor(int(segment[0]), int(segment[1]), segment[2]);
                update();
                return;
            }
        }

        // Closing: click back on the subpath's own first anchor.
        if (m_engine->pathIsEditing()) {
            const rust::Vec<::std::int32_t> anchor = m_engine->pathHitAnchor(doc.x(), doc.y(), radius);
            if (anchor.size() == 2 && anchor[1] == 0) {
                m_engine->pathCloseActiveSubpath();
                update();
                return;
            }
        }

        // Otherwise: a fresh anchor. Whether it ends up a corner or a smooth
        // point is decided by whether the next mouseMove drags it before
        // release — see `penMove`.
        m_engine->pathAppendCorner(float(doc.x()), float(doc.y()));
        m_penGesture = PenGesture::PlacingHandle;
        update();
        return;
    }

    case PenTool::FreeformPen:
        m_freeformPoints.clear();
        m_freeformPoints.append(doc);
        m_penGesture = PenGesture::Freeform;
        return;

    case PenTool::AddAnchor: {
        const rust::Vec<float> segment = m_engine->pathHitSegment(doc.x(), doc.y(), radius);
        if (segment.size() == 3) {
            m_engine->pathInsertAnchor(int(segment[0]), int(segment[1]), segment[2]);
            update();
        }
        return;
    }

    case PenTool::DeleteAnchor: {
        const rust::Vec<::std::int32_t> anchor = m_engine->pathHitAnchor(doc.x(), doc.y(), radius);
        if (anchor.size() == 2) {
            m_engine->pathDeleteAnchor(anchor[0], anchor[1]);
            update();
        }
        return;
    }

    case PenTool::ConvertPoint: {
        // A handle takes priority — it is the smaller, more specific target,
        // and dragging one always means "break this handle free" regardless
        // of what the anchor underneath it would have done.
        const rust::Vec<::std::int32_t> handle = m_engine->pathHitHandle(doc.x(), doc.y(), radius);
        if (handle.size() == 3) {
            m_penSubpath = handle[0];
            m_penPoint = handle[1];
            m_penHandleSide = handle[2];
            m_penGesture = PenGesture::ConvertHandle;
            m_penPressDoc = doc;
            return;
        }
        const rust::Vec<::std::int32_t> anchor = m_engine->pathHitAnchor(doc.x(), doc.y(), radius);
        if (anchor.size() == 2) {
            m_penSubpath = anchor[0];
            m_penPoint = anchor[1];
            m_penGesture = PenGesture::ConvertNewHandles;
            m_penPressDoc = doc;
        }
        return;
    }
    }
}

void CanvasView::penMove(const QPointF &doc, Qt::KeyboardModifiers modifiers)
{
    m_penHoverDoc = doc;
    if (!m_engine) {
        return;
    }
    const bool independent = modifiers.testFlag(Qt::AltModifier);

    switch (m_penGesture) {
    case PenGesture::PlacingHandle:
        m_engine->pathUpdateLastHandle(float(doc.x()), float(doc.y()), independent);
        update();
        return;
    case PenGesture::ConvertHandle:
        // Convert Point's handle-drag always breaks it independent, which is
        // what makes it useful for un-mirroring one side of a smooth point
        // without hunting for the Alt key.
        m_engine->pathMoveHandle(m_penSubpath, m_penPoint, m_penHandleSide, float(doc.x()),
                                 float(doc.y()), true);
        update();
        return;
    case PenGesture::ConvertNewHandles:
        m_engine->pathDragNewHandles(m_penSubpath, m_penPoint, float(doc.x()), float(doc.y()));
        update();
        return;
    case PenGesture::Freeform:
        // Throttled the same way the freehand lasso is: one vertex per
        // document pixel of travel keeps the trail small enough to simplify
        // instantly on release.
        if (m_freeformPoints.isEmpty()
            || (doc - m_freeformPoints.last()).manhattanLength() >= 1.0) {
            m_freeformPoints.append(doc);
            update();
        }
        return;
    case PenGesture::None:
        if (m_tool == ToolId::Pen && m_penRubberBand) {
            update();
        }
        return;
    }
}

void CanvasView::penRelease(const QPointF &doc)
{
    if (!m_engine) {
        m_penGesture = PenGesture::None;
        return;
    }

    switch (m_penGesture) {
    case PenGesture::ConvertNewHandles: {
        // A click rather than a drag: Convert Point strips the handles back
        // off instead of leaving the sliver a near-zero drag would have
        // pulled out.
        const QPointF delta = doc - m_penPressDoc;
        if (QPointF::dotProduct(delta, delta) < 4.0) {
            m_engine->pathSetCorner(m_penSubpath, m_penPoint);
            update();
        }
        break;
    }
    case PenGesture::Freeform: {
        // Interleaved x,y, matching every other point-list call to the bridge.
        QVector<float> flat;
        flat.reserve(m_freeformPoints.size() * 2);
        for (const QPointF &p : m_freeformPoints) {
            flat.append(float(p.x()));
            flat.append(float(p.y()));
        }
        // Dragging back near the start closes the loop, the same gesture the
        // ordinary Pen tool uses.
        const QPointF toStart = documentToWidget(doc) - documentToWidget(m_freeformPoints.first());
        const bool close = m_freeformPoints.size() > 2
            && QPointF::dotProduct(toStart, toStart) <= 8.0 * 8.0;
        if (m_engine->pathAddFreeformSubpath(flat, float(m_freeformTolerance), close)) {
            update();
        }
        m_freeformPoints.clear();
        break;
    }
    default:
        break;
    }
    m_penGesture = PenGesture::None;
}

void CanvasView::pathSelectPress(const QPointF &doc)
{
    if (!m_engine) {
        return;
    }
    const float radius = pathHitRadius();

    if (m_pathSelectTool == PathSelectTool::DirectSelection) {
        const rust::Vec<::std::int32_t> handle = m_engine->pathHitHandle(doc.x(), doc.y(), radius);
        if (handle.size() == 3) {
            m_pathSelectSubpath = handle[0];
            m_pathSelectPoint = handle[1];
            m_pathSelectHandleSide = handle[2];
            m_pathSelectGesture = PathSelectGesture::Handle;
            return;
        }
        const rust::Vec<::std::int32_t> anchor = m_engine->pathHitAnchor(doc.x(), doc.y(), radius);
        if (anchor.size() == 2) {
            m_pathSelectSubpath = anchor[0];
            m_pathSelectPoint = anchor[1];
            m_pathSelectGesture = PathSelectGesture::Anchor;
        }
        return;
    }

    // Path Selection: grabs the whole subpath.
    const int sp = m_engine->pathHitSubpath(doc.x(), doc.y(), radius);
    if (sp >= 0) {
        m_pathSelectSubpath = sp;
        m_pathSelectGesture = PathSelectGesture::Subpath;
        m_pathSelectLastDoc = doc;
    }
}

void CanvasView::pathSelectMove(const QPointF &doc, Qt::KeyboardModifiers modifiers)
{
    if (!m_engine) {
        return;
    }
    switch (m_pathSelectGesture) {
    case PathSelectGesture::Anchor:
        m_engine->pathMoveAnchor(m_pathSelectSubpath, m_pathSelectPoint, float(doc.x()),
                                 float(doc.y()));
        update();
        break;
    case PathSelectGesture::Handle:
        m_engine->pathMoveHandle(m_pathSelectSubpath, m_pathSelectPoint, m_pathSelectHandleSide,
                                 float(doc.x()), float(doc.y()),
                                 modifiers.testFlag(Qt::AltModifier));
        update();
        break;
    case PathSelectGesture::Subpath: {
        const QPointF delta = doc - m_pathSelectLastDoc;
        m_engine->pathMoveSubpath(m_pathSelectSubpath, float(delta.x()), float(delta.y()));
        m_pathSelectLastDoc = doc;
        update();
        break;
    }
    case PathSelectGesture::None:
        break;
    }
}

void CanvasView::pathSelectRelease()
{
    m_pathSelectGesture = PathSelectGesture::None;
}

void CanvasView::setCloneOptions(bool aligned, CloneSampling sampling)
{
    m_cloneAligned = aligned;
    m_cloneSampling = sampling;
    if (m_engine) {
        m_engine->setCloneOptions(aligned, int(sampling));
    }
}

bool CanvasView::clonePress(const QPointF &doc, Qt::KeyboardModifiers modifiers)
{
    if (!m_engine) {
        return false;
    }

    // Alt-click sets what to copy from, exactly as CS6 does. A one-off action,
    // not the start of a stroke.
    if (modifiers.testFlag(Qt::AltModifier)) {
        m_cloneSource = QPointF(std::round(doc.x()), std::round(doc.y()));
        m_cloneSourceValid = true;
        const bool hasContent =
            m_engine->setCloneSource(int(m_cloneSource.x()), int(m_cloneSource.y()));
        // Sampling the current layer alone is CS6's default, and it finds
        // nothing if what the user is pointing at lives on another layer. The
        // stroke would then copy transparency and look like a broken tool, so say
        // so here instead.
        if (hasContent) {
            emit statusMessage(tr("Clone source set to %1, %2")
                                   .arg(int(m_cloneSource.x()))
                                   .arg(int(m_cloneSource.y())));
        } else {
            emit statusMessage(tr("Clone source set to %1, %2 — but this layer is empty "
                                  "there. Set Sample to All Layers to clone what you can "
                                  "see.")
                                   .arg(int(m_cloneSource.x()))
                                   .arg(int(m_cloneSource.y())));
        }
        update();
        return true;
    }

    // Without a source there is nothing to copy, and Photoshop refuses the
    // stroke rather than painting the foreground colour. Consuming the press
    // means no stroke begins and no undo state is created.
    if (!m_engine->hasCloneSource()) {
        emit cloneSourceRequired();
        return true;
    }

    if (m_engine->beginCloneStroke(float(doc.x()), float(doc.y()), 1.0f)) {
        m_dragging = true;
        m_image = m_engine->previewImage();
        update();
    } else {
        reportIfLocked();
    }
    return true;
}

void CanvasView::paintCloneSource(QPainter &painter)
{
    if (m_tool != ToolId::CloneStamp || !m_cloneSourceValid) {
        return;
    }
    // The same crosshair the Healing Brush marks its source with, so the two
    // read as the same idea.
    painter.save();
    painter.setRenderHint(QPainter::Antialiasing, true);
    const QPointF p = documentToWidget(m_cloneSource);
    painter.setBrush(Qt::NoBrush);
    painter.setPen(QPen(Qt::white, 2.5));
    painter.drawLine(QPointF(p.x() - 7, p.y()), QPointF(p.x() + 7, p.y()));
    painter.drawLine(QPointF(p.x(), p.y() - 7), QPointF(p.x(), p.y() + 7));
    painter.setPen(QPen(QColor(0x2c, 0x6f, 0xd6), 1.2));
    painter.drawLine(QPointF(p.x() - 7, p.y()), QPointF(p.x() + 7, p.y()));
    painter.drawLine(QPointF(p.x(), p.y() - 7), QPointF(p.x(), p.y() + 7));
    painter.drawEllipse(p, 4.0, 4.0);
    painter.restore();
}

void CanvasView::reportIfLocked()
{
    // Every painting entry point refuses on the same condition, so asking the
    // engine once here keeps four call sites from each having to work out why
    // they were turned down. A refusal for any other reason — an adjustment
    // layer, say — is silent, as it is in Photoshop.
    if (m_engine && m_engine->activeLayerIsLocked()) {
        emit lockedLayerRefused();
    }
}

void CanvasView::setHealingType(HealingType type)
{
    if (m_healingType == type) {
        return;
    }
    m_healingType = type;
    // Each variant is a different gesture, so nothing half-done should survive
    // the switch.
    m_healingTracing = false;
    m_regionDragging = false;
    m_redEyeDragging = false;
    m_redEyeRect = QRectF();
    m_lassoPath.clear();
    m_marqueeActive = false;
    // The source belongs to the Healing Brush; picking another variant drops it.
    if (type != HealingType::Healing) {
        m_healSourceValid = false;
    }
    if (m_engine) {
        m_engine->setHealSource(false, 0, 0);
    }
    updateCursor();
    update();
}

void CanvasView::setContentAwareMoveOptions(bool extend, int structure, int color,
                                            bool sampleAllLayers)
{
    m_camExtend = extend;
    m_camStructure = qBound(1, structure, 7);
    m_camColor = qBound(0, color, 10);
    m_camSampleAllLayers = sampleAllLayers;
}

void CanvasView::setPatchOptions(bool contentAware, bool destination, bool transparent)
{
    m_patchContentAware = contentAware;
    m_patchDestination = destination;
    m_patchTransparent = transparent;
}

void CanvasView::setRedEyeOptions(int pupilSize, int darkenAmount)
{
    m_pupilSize = qBound(0, pupilSize, 100);
    m_darkenAmount = qBound(0, darkenAmount, 100);
}

bool CanvasView::healingPress(const QPointF &doc, Qt::KeyboardModifiers modifiers)
{
    if (!m_engine) {
        return false;
    }

    switch (m_healingType) {
    case HealingType::Healing:
        // Alt-click sets where to sample from, exactly as CS6 does. It is a
        // one-off action, not the start of a stroke.
        if (modifiers.testFlag(Qt::AltModifier)) {
            m_healSource = doc;
            m_healSourceValid = true;
            emit statusMessage(tr("Healing source set to %1, %2")
                                   .arg(int(doc.x()))
                                   .arg(int(doc.y())));
            update();
            return true;
        }
        // Without a source there is nothing to repair from, and Photoshop
        // refuses the stroke rather than guessing. Consuming the press means no
        // stroke begins and no undo state is created.
        if (!m_healSourceValid) {
            emit healingSourceRequired();
            return true;
        }
        // Tell the engine where the source sits relative to this stroke.
        m_engine->setHealSource(true, int(std::round(m_healSource.x() - doc.x())),
                                int(std::round(m_healSource.y() - doc.y())));
        // Fall through to the ordinary stroke path.
        return false;

    case HealingType::Patch:
    case HealingType::ContentAwareMove: {
        // Inside an existing selection, a drag moves it; anywhere else starts a
        // fresh outline. That is CS6's two-step: define the region, then drag.
        const rust::Vec<::std::int32_t> bounds = m_engine->selectionBounds();
        const bool haveSelection = bounds.size() >= 4 && (bounds[2] > 0 || bounds[3] > 0);
        const QRectF selectionRect =
            haveSelection ? QRectF(bounds[0], bounds[1], bounds[2], bounds[3]) : QRectF();

        if (haveSelection && selectionRect.contains(doc)) {
            m_regionDragging = true;
            m_regionDragStart = doc;
            m_regionDragNow = doc;
        } else {
            m_healingTracing = true;
            m_marqueeActive = true;
            m_gestureModifiers = modifiers;
            m_lassoPath.clear();
            m_lassoPath.append(doc);
        }
        update();
        return true;
    }

    case HealingType::RedEye:
        m_redEyeDragging = true;
        m_redEyeRect = QRectF(doc, doc);
        update();
        return true;

    case HealingType::SpotHealing:
        // Nothing special: the plain stroke path handles it.
        m_engine->setHealSource(false, 0, 0);
        return false;
    }
    return false;
}

bool CanvasView::healingDrag(const QPointF &doc)
{
    if (m_regionDragging) {
        m_regionDragNow = doc;
        update();
        return true;
    }
    if (m_redEyeDragging) {
        m_redEyeRect = QRectF(m_redEyeRect.topLeft(), doc).normalized();
        update();
        return true;
    }
    return false;
}

bool CanvasView::healingRelease()
{
    if (m_regionDragging) {
        m_regionDragging = false;
        commitHealingDrag();
        update();
        return true;
    }

    if (m_redEyeDragging) {
        m_redEyeDragging = false;
        const QRectF r = m_redEyeRect.normalized();
        if (m_engine) {
            // A click rather than a drag still means "the eye is here", so a
            // bare click gets a default-sized box around it.
            QRectF target = r;
            if (target.width() < 2.0 || target.height() < 2.0) {
                target = QRectF(r.center().x() - 12, r.center().y() - 12, 24, 24);
            }
            m_engine->removeRedEye(int(std::floor(target.x())), int(std::floor(target.y())),
                                   int(std::round(target.width())),
                                   int(std::round(target.height())), m_pupilSize,
                                   m_darkenAmount);
        }
        m_redEyeRect = QRectF();
        update();
        return true;
    }
    return false;
}

void CanvasView::commitHealingDrag()
{
    if (!m_engine) {
        return;
    }
    const QPointF delta = m_regionDragNow - m_regionDragStart;
    const int dx = int(std::round(delta.x()));
    const int dy = int(std::round(delta.y()));
    if (dx == 0 && dy == 0) {
        return;
    }

    // These reconstruct every pixel of the region and run on the GUI thread, so
    // a large area takes long enough to notice. A wait cursor is the difference
    // between "working" and "hung".
    QGuiApplication::setOverrideCursor(Qt::WaitCursor);
    const struct CursorGuard {
        ~CursorGuard() { QGuiApplication::restoreOverrideCursor(); }
    } guard;

    if (m_healingType == HealingType::Patch) {
        // CS6's Patch drags the selection *to* the area to sample from, so the
        // drag delta is the source offset. Destination mode reverses that, and
        // the engine handles the swap.
        m_engine->patchSelection(dx, dy, m_patchContentAware, m_patchDestination,
                                 m_patchTransparent);
    } else if (m_healingType == HealingType::ContentAwareMove) {
        m_engine->contentAwareMove(dx, dy, m_camExtend, m_camStructure, m_camColor,
                                   m_camSampleAllLayers);
    }
}

void CanvasView::paintHealing(QPainter &painter)
{
    if (m_tool != ToolId::Healing) {
        return;
    }

    painter.save();
    painter.setRenderHint(QPainter::Antialiasing, true);

    // The Healing Brush's sampled source, so the user can see what it will
    // transplant from.
    if (m_healingType == HealingType::Healing && m_healSourceValid) {
        const QPointF p = documentToWidget(m_healSource);
        painter.setBrush(Qt::NoBrush);
        painter.setPen(QPen(Qt::white, 2.5));
        painter.drawLine(QPointF(p.x() - 7, p.y()), QPointF(p.x() + 7, p.y()));
        painter.drawLine(QPointF(p.x(), p.y() - 7), QPointF(p.x(), p.y() + 7));
        painter.setPen(QPen(QColor(0x2c, 0x6f, 0xd6), 1.2));
        painter.drawLine(QPointF(p.x() - 7, p.y()), QPointF(p.x() + 7, p.y()));
        painter.drawLine(QPointF(p.x(), p.y() - 7), QPointF(p.x(), p.y() + 7));
        painter.drawEllipse(p, 4.0, 4.0);
    }

    // The region drag: the selection outline shown at the offset it would take.
    if (m_regionDragging && !m_selectionPath.isEmpty()) {
        const QPointF delta = m_regionDragNow - m_regionDragStart;
        const QPointF origin = documentOrigin();
        QTransform toWidget;
        toWidget.translate(origin.x() + delta.x() * m_zoom, origin.y() + delta.y() * m_zoom);
        toWidget.scale(m_zoom, m_zoom);

        painter.setBrush(Qt::NoBrush);
        painter.setPen(QPen(Qt::white, 1));
        painter.drawPath(toWidget.map(m_selectionPath));
        QPen dashed(Qt::black, 1, Qt::DashLine);
        dashed.setDashPattern({4, 4});
        painter.setPen(dashed);
        painter.drawPath(toWidget.map(m_selectionPath));
    }

    // The red-eye box.
    if (m_redEyeDragging && !m_redEyeRect.isNull()) {
        const QRectF r(documentToWidget(m_redEyeRect.topLeft()),
                       documentToWidget(m_redEyeRect.bottomRight()));
        painter.setBrush(Qt::NoBrush);
        painter.setPen(QPen(QColor(0xd6, 0x3c, 0x2c), 1));
        painter.drawRect(r);
    }

    painter.restore();
}

void CanvasView::setHealType(HealType type)
{
    m_healType = type;
    if (m_engine && toolHeals(m_tool)) {
        m_engine->setHealMode(static_cast<int>(type));
    }
}

void CanvasView::setEyedropperType(EyedropperType type)
{
    if (m_eyedropperType == type) {
        return;
    }
    m_eyedropperType = type;
    m_draggedMarker = -1;
    m_draggedRulerEnd = -1;
    refreshAnnotations();
}

void CanvasView::refreshAnnotations()
{
    for (int kind = 0; kind < 3; ++kind) {
        m_markers[kind].clear();
        if (!m_engine) {
            continue;
        }
        const int count = m_engine->markerCount(kind);
        for (int i = 0; i < count; ++i) {
            const rust::Vec<::std::int32_t> p = m_engine->markerAt(kind, i);
            if (p.size() >= 2) {
                m_markers[kind].append(QPointF(p[0], p[1]));
            }
        }
    }

    m_ruler.clear();
    if (m_engine) {
        const rust::Vec<float> line = m_engine->rulerLine();
        if (line.size() >= 4) {
            m_ruler.append(QPointF(line[0], line[1]));
            m_ruler.append(QPointF(line[2], line[3]));
        }
    }
    update();
}

float CanvasView::grabRadiusDoc() const
{
    // Eight widget pixels, converted to document units, so the grab area is a
    // constant size on screen at any zoom.
    return float(8.0 / qMax(m_zoom, 1e-6));
}

bool CanvasView::activeMarkerKind(MarkerKind *kind) const
{
    switch (m_eyedropperType) {
    case EyedropperType::ColorSampler:
        *kind = MarkerKind::ColorSampler;
        return true;
    case EyedropperType::Note:
        *kind = MarkerKind::Note;
        return true;
    case EyedropperType::Count:
        *kind = MarkerKind::Count;
        return true;
    case EyedropperType::Ruler:
    case EyedropperType::Eyedropper:
        break;
    }
    return false;
}

void CanvasView::annotationPress(const QPointF &doc, Qt::KeyboardModifiers modifiers)
{
    if (!m_engine) {
        return;
    }
    const int x = int(std::round(doc.x()));
    const int y = int(std::round(doc.y()));

    if (m_eyedropperType == EyedropperType::Ruler) {
        // Grab an endpoint if one is close, otherwise start a fresh line.
        m_draggedRulerEnd = -1;
        const double grab = grabRadiusDoc();
        for (int end = 0; end < m_ruler.size(); ++end) {
            const QPointF delta = m_ruler.at(end) - doc;
            if (QPointF::dotProduct(delta, delta) <= grab * grab) {
                m_draggedRulerEnd = end;
                return;
            }
        }
        m_engine->setRuler(float(doc.x()), float(doc.y()), float(doc.x()), float(doc.y()));
        m_draggedRulerEnd = 1;
        return;
    }

    MarkerKind kind = MarkerKind::ColorSampler;
    if (!activeMarkerKind(&kind)) {
        return;
    }
    const int k = static_cast<int>(kind);
    const int hit = m_engine->markerNear(k, x, y, grabRadiusDoc());

    // Alt-click removes, matching how Photoshop deletes a sampler or a count.
    if (hit >= 0 && modifiers.testFlag(Qt::AltModifier)) {
        m_engine->removeMarker(k, hit);
        return;
    }

    if (hit >= 0) {
        if (kind == MarkerKind::Note) {
            emit noteEditRequested(hit);
            return;
        }
        m_draggedMarker = hit;
        return;
    }

    const int added = m_engine->addMarker(k, x, y);
    if (added < 0) {
        // Only colour samplers refuse, and only when all four are placed.
        return;
    }
    if (kind == MarkerKind::Note) {
        emit noteEditRequested(added);
    } else {
        m_draggedMarker = added;
    }
}

void CanvasView::annotationDrag(const QPointF &doc)
{
    if (!m_engine) {
        return;
    }
    if (m_draggedRulerEnd >= 0 && m_ruler.size() == 2) {
        const QPointF other = m_ruler.at(m_draggedRulerEnd == 0 ? 1 : 0);
        const QPointF a = m_draggedRulerEnd == 0 ? doc : other;
        const QPointF b = m_draggedRulerEnd == 0 ? other : doc;
        m_engine->setRuler(float(a.x()), float(a.y()), float(b.x()), float(b.y()));
        return;
    }

    MarkerKind kind = MarkerKind::ColorSampler;
    if (m_draggedMarker >= 0 && activeMarkerKind(&kind)) {
        m_engine->moveMarker(static_cast<int>(kind), m_draggedMarker,
                             int(std::round(doc.x())), int(std::round(doc.y())));
    }
}

void CanvasView::paintAnnotations(QPainter &painter)
{
    painter.save();
    painter.setRenderHint(QPainter::Antialiasing, true);

    QFont badgeFont = painter.font();
    badgeFont.setPixelSize(9);
    painter.setFont(badgeFont);

    // The ruler: a line with square handles and its length beside it.
    if (m_ruler.size() == 2) {
        const QPointF a = documentToWidget(m_ruler.at(0));
        const QPointF b = documentToWidget(m_ruler.at(1));

        // Drawn twice so it stays visible over light and dark content alike,
        // the same trick the marching ants use.
        painter.setPen(QPen(Qt::white, 3));
        painter.drawLine(a, b);
        painter.setPen(QPen(Qt::black, 1));
        painter.drawLine(a, b);

        painter.setBrush(Qt::white);
        painter.setPen(QPen(Qt::black, 1));
        for (const QPointF &p : {a, b}) {
            painter.drawRect(QRectF(p.x() - 3, p.y() - 3, 6, 6));
        }
    }

    // Markers. Colour samplers and counts get a numbered badge; notes get the
    // page glyph the tool's own icon uses.
    struct Style {
        MarkerKind kind;
        QColor color;
    };
    const Style styles[] = {
        {MarkerKind::ColorSampler, QColor(0x2c, 0x6f, 0xd6)},
        {MarkerKind::Note, QColor(0xe8, 0xc0, 0x3a)},
        {MarkerKind::Count, QColor(0xd6, 0x3c, 0x2c)},
    };

    for (const Style &style : styles) {
        const QList<QPointF> &list = m_markers[static_cast<int>(style.kind)];
        for (int i = 0; i < list.size(); ++i) {
            const QPointF p = documentToWidget(list.at(i));

            if (style.kind == MarkerKind::Note) {
                painter.setPen(QPen(Qt::black, 1));
                painter.setBrush(style.color);
                painter.drawRect(QRectF(p.x() - 5, p.y() - 6, 10, 12));
                painter.setPen(QPen(QColor(0x40, 0x35, 0x10), 1));
                for (int line = 0; line < 3; ++line) {
                    const double y = p.y() - 3 + line * 3;
                    painter.drawLine(QPointF(p.x() - 3, y), QPointF(p.x() + 3, y));
                }
                continue;
            }

            // A crosshair over a filled disc, so the exact sampled pixel is
            // visible rather than hidden under the badge.
            painter.setPen(QPen(Qt::white, 2.5));
            painter.drawLine(QPointF(p.x() - 6, p.y()), QPointF(p.x() + 6, p.y()));
            painter.drawLine(QPointF(p.x(), p.y() - 6), QPointF(p.x(), p.y() + 6));
            painter.setPen(QPen(style.color, 1.2));
            painter.drawLine(QPointF(p.x() - 6, p.y()), QPointF(p.x() + 6, p.y()));
            painter.drawLine(QPointF(p.x(), p.y() - 6), QPointF(p.x(), p.y() + 6));

            const QString label = QString::number(i + 1);
            const QRectF badge(p.x() + 5, p.y() - 15,
                               painter.fontMetrics().horizontalAdvance(label) + 6, 12);
            painter.setPen(Qt::NoPen);
            painter.setBrush(style.color);
            painter.drawRect(badge);
            painter.setPen(Qt::white);
            painter.drawText(badge, Qt::AlignCenter, label);
        }
    }

    painter.restore();
}

void CanvasView::refreshSlices()
{
    m_slices.clear();
    if (m_engine) {
        const int count = m_engine->sliceCount();
        m_slices.reserve(count);
        for (int i = 0; i < count; ++i) {
            // Packed as [x, y, w, h, number, userIndex]; see bridge.rs.
            const rust::Vec<::std::int32_t> f = m_engine->sliceAt(i);
            if (f.size() < 6) {
                continue;
            }
            m_slices.append(SliceInfo{QRectF(f[0], f[1], f[2], f[3]), f[4], f[5]});
        }
    }

    // The selected slice may have been removed, or the user list renumbered by
    // a deletion, so drop a selection that no longer names anything.
    const bool stillThere = std::any_of(m_slices.cbegin(), m_slices.cend(),
                                        [this](const SliceInfo &s) {
                                            return s.userIndex == m_selectedSlice;
                                        });
    if (!stillThere) {
        m_selectedSlice = -1;
    }
    update();
}

int CanvasView::sliceAt(const QPointF &doc) const
{
    // User slices win over the auto slices they sit on top of, so search them
    // first rather than taking whatever comes earliest in the list.
    for (int pass = 0; pass < 2; ++pass) {
        const bool wantUser = pass == 0;
        for (int i = 0; i < m_slices.size(); ++i) {
            const SliceInfo &slice = m_slices.at(i);
            if ((slice.userIndex >= 0) == wantUser && slice.rect.contains(doc)) {
                return i;
            }
        }
    }
    return -1;
}

void CanvasView::deleteSelectedSlice()
{
    if (m_selectedSlice < 0 || !m_engine) {
        return;
    }
    m_engine->removeUserSlice(m_selectedSlice);
    m_selectedSlice = -1;
    // The engine's slicesChanged signal refreshes the cache and repaints.
}

void CanvasView::paintSlices(QPainter &painter)
{
    if (m_slices.isEmpty()) {
        return;
    }

    painter.save();
    painter.setRenderHint(QPainter::Antialiasing, false);

    QFont badgeFont = painter.font();
    badgeFont.setPixelSize(9);
    painter.setFont(badgeFont);

    // CS6's colours: user slices in a saturated blue with solid lines, auto
    // slices in grey with dotted ones.
    const QColor userColor(0x2c, 0x6f, 0xd6);
    const QColor autoColor(0x8a, 0x8a, 0x8a);

    for (const SliceInfo &slice : m_slices) {
        const bool isUser = slice.userIndex >= 0;
        const bool isSelected = isUser && slice.userIndex == m_selectedSlice;
        const QPointF topLeft = documentToWidget(slice.rect.topLeft());
        const QPointF bottomRight = documentToWidget(slice.rect.bottomRight());
        const QRectF box = QRectF(topLeft, bottomRight).normalized();

        QPen pen(isUser ? userColor : autoColor, 1);
        if (!isUser) {
            pen.setStyle(Qt::DotLine);
        }
        painter.setPen(pen);
        painter.setBrush(Qt::NoBrush);
        painter.drawRect(box);

        // The numbered badge sits just inside the slice's top-left corner.
        const QString label = QStringLiteral("%1").arg(slice.number, 2, 10, QLatin1Char('0'));
        const QRectF badge(box.left() + 1, box.top() + 1,
                           painter.fontMetrics().horizontalAdvance(label) + 6, 12);
        // Skip the badge on a slice too small to hold it, rather than letting
        // it spill over the neighbours.
        if (box.width() >= badge.width() + 2 && box.height() >= badge.height() + 2) {
            painter.setPen(Qt::NoPen);
            painter.setBrush(isUser ? userColor : autoColor);
            painter.drawRect(badge);
            painter.setPen(Qt::white);
            painter.drawText(badge, Qt::AlignCenter, label);
        }

        if (isSelected) {
            // CS6 marks the selected slice with orange handles.
            const QColor handleColor(0xf5, 0x9e, 0x0b);
            painter.setPen(QPen(handleColor, 1));
            painter.setBrush(handleColor);
            const double cx = box.center().x();
            const double cy = box.center().y();
            const QPointF handles[] = {
                box.topLeft(),  QPointF(cx, box.top()),    box.topRight(),
                QPointF(box.right(), cy),                  box.bottomRight(),
                QPointF(cx, box.bottom()),                 box.bottomLeft(),
                QPointF(box.left(), cy),
            };
            for (const QPointF &h : handles) {
                painter.drawRect(QRectF(h.x() - 2.5, h.y() - 2.5, 5, 5));
            }
        }
    }

    // The slice being dragged out, before it exists in the engine.
    if (m_sliceDragging && !m_sliceDrag.isNull()) {
        const QPointF topLeft = documentToWidget(m_sliceDrag.normalized().topLeft());
        const QPointF bottomRight = documentToWidget(m_sliceDrag.normalized().bottomRight());
        painter.setPen(QPen(userColor, 1));
        painter.setBrush(Qt::NoBrush);
        painter.drawRect(QRectF(topLeft, bottomRight));
    }

    painter.restore();
}

void CanvasView::setCropType(CropType type)
{
    if (m_cropType == type) {
        return;
    }
    m_cropType = type;
    if (m_tool == ToolId::Crop) {
        resetCrop();
        refreshSlices();
    }
}

void CanvasView::resetCrop()
{
    if (!m_engine) {
        return;
    }
    const double w = m_engine->getCanvasWidth();
    const double h = m_engine->getCanvasHeight();

    m_cropRect = QRectF(0, 0, w, h);
    m_cropGrip = CropGrip::None;
    if (m_cropRatio > 0.0) {
        applyCropRatio(CropGrip::BottomRight);
    }

    // The quad starts as the same rectangle, in the corner order the engine
    // expects.
    m_cropQuad = QPolygonF({QPointF(0, 0), QPointF(w, 0), QPointF(w, h), QPointF(0, h)});
    m_cropCorner = -1;
    m_cropQuadMoving = false;
    m_cropQuadNew = false;
    update();
}

int CanvasView::cropCornerAt(const QPointF &widgetPos) const
{
    if (m_cropQuad.size() != 4) {
        return -1;
    }
    // A fixed grab radius on screen, as the rectangular crop uses, so corners
    // stay reachable at any zoom.
    constexpr double kGrab = 8.0;
    for (int i = 0; i < 4; ++i) {
        const QPointF delta = documentToWidget(m_cropQuad.at(i)) - widgetPos;
        if (QPointF::dotProduct(delta, delta) <= kGrab * kGrab) {
            return i;
        }
    }
    return -1;
}

void CanvasView::commitPerspectiveCrop()
{
    if (!m_engine || m_cropQuad.size() != 4) {
        return;
    }

    QVector<float> corners;
    corners.reserve(8);
    for (const QPointF &p : m_cropQuad) {
        corners.append(float(p.x()));
        corners.append(float(p.y()));
    }

    // The engine refuses a degenerate quad rather than producing nonsense, so
    // a failure here just leaves the box up for the user to fix.
    if (!m_engine->perspectiveCrop(corners)) {
        return;
    }
    refresh();
    refreshSlices();
    resetCrop();
    clampPan();
}

void CanvasView::paintCropQuad(QPainter &painter)
{
    if (m_cropQuad.size() != 4) {
        return;
    }

    QPolygonF widgetQuad;
    widgetQuad.reserve(4);
    for (const QPointF &p : m_cropQuad) {
        widgetQuad.append(documentToWidget(p));
    }

    painter.save();
    painter.setRenderHint(QPainter::Antialiasing, true);

    // Shield everything outside the quad, as the rectangular crop does.
    QPainterPath shield;
    shield.addRect(rect());
    QPainterPath inside;
    inside.addPolygon(widgetQuad);
    inside.closeSubpath();
    painter.setPen(Qt::NoPen);
    painter.setBrush(QColor(0, 0, 0, 190));
    painter.drawPath(shield.subtracted(inside));

    // A 3×3 grid, its lines interpolated along the edges rather than drawn
    // straight across, so they follow the perspective and show how the warp
    // will land.
    painter.setBrush(Qt::NoBrush);
    painter.setPen(QPen(QColor(255, 255, 255, 90), 1));
    for (int i = 1; i <= 2; ++i) {
        const double t = i / 3.0;
        const QPointF top = widgetQuad[0] + (widgetQuad[1] - widgetQuad[0]) * t;
        const QPointF bottom = widgetQuad[3] + (widgetQuad[2] - widgetQuad[3]) * t;
        painter.drawLine(top, bottom);

        const QPointF left = widgetQuad[0] + (widgetQuad[3] - widgetQuad[0]) * t;
        const QPointF right = widgetQuad[1] + (widgetQuad[2] - widgetQuad[1]) * t;
        painter.drawLine(left, right);
    }

    painter.setPen(QPen(QColor(255, 255, 255, 220), 1));
    painter.drawPolygon(widgetQuad);

    painter.setPen(QPen(QColor(40, 40, 40), 1));
    painter.setBrush(QColor(255, 255, 255));
    for (const QPointF &p : widgetQuad) {
        painter.drawRect(QRectF(p.x() - 3.5, p.y() - 3.5, 7, 7));
    }

    painter.restore();
}

void CanvasView::commitCrop()
{
    if (m_tool != ToolId::Crop || !m_engine) {
        return;
    }
    if (cropIsPerspective()) {
        commitPerspectiveCrop();
        return;
    }
    const QRectF r = m_cropRect.normalized();
    const int x = int(std::floor(r.x()));
    const int y = int(std::floor(r.y()));
    const int w = int(std::round(r.width()));
    const int h = int(std::round(r.height()));
    if (w < 1 || h < 1) {
        return;
    }
    // A box covering everything is not a crop; committing it would still cost
    // a history state.
    if (x <= 0 && y <= 0 && w >= m_engine->getCanvasWidth() && h >= m_engine->getCanvasHeight()) {
        return;
    }

    m_engine->cropTo(x, y, w, h, m_cropDeletePixels);
    refresh();
    // Slices are clipped to the canvas, which just changed under them.
    refreshSlices();
    // The document is a different size now, so the box has to be rebuilt
    // against the new canvas rather than kept in old coordinates.
    resetCrop();
    clampPan();
}

CanvasView::CropGrip CanvasView::cropGripAt(const QPointF &widgetPos) const
{
    return gripAt(m_cropRect, widgetPos);
}

CanvasView::CropGrip CanvasView::gripAt(const QRectF &docRect, const QPointF &widgetPos) const
{
    const QPointF topLeft = documentToWidget(docRect.topLeft());
    const QPointF bottomRight = documentToWidget(docRect.bottomRight());
    const QRectF box = QRectF(topLeft, bottomRight).normalized();

    // A fixed grab radius on screen, so the handles stay usable when zoomed
    // far out and do not become huge when zoomed in.
    constexpr double kGrab = 6.0;
    const bool nearLeft = std::abs(widgetPos.x() - box.left()) <= kGrab;
    const bool nearRight = std::abs(widgetPos.x() - box.right()) <= kGrab;
    const bool nearTop = std::abs(widgetPos.y() - box.top()) <= kGrab;
    const bool nearBottom = std::abs(widgetPos.y() - box.bottom()) <= kGrab;
    const bool withinX = widgetPos.x() >= box.left() - kGrab
        && widgetPos.x() <= box.right() + kGrab;
    const bool withinY = widgetPos.y() >= box.top() - kGrab
        && widgetPos.y() <= box.bottom() + kGrab;

    if (!withinX || !withinY) {
        return CropGrip::None;
    }
    // Corners first: they overlap the edges, and a corner is what the user
    // means when they aim at one.
    if (nearLeft && nearTop) return CropGrip::TopLeft;
    if (nearRight && nearTop) return CropGrip::TopRight;
    if (nearLeft && nearBottom) return CropGrip::BottomLeft;
    if (nearRight && nearBottom) return CropGrip::BottomRight;
    if (nearLeft) return CropGrip::Left;
    if (nearRight) return CropGrip::Right;
    if (nearTop) return CropGrip::Top;
    if (nearBottom) return CropGrip::Bottom;

    return box.contains(widgetPos) ? CropGrip::Move : CropGrip::None;
}

Qt::CursorShape CanvasView::cropCursor(CropGrip grip) const
{
    switch (grip) {
    case CropGrip::Move:        return Qt::SizeAllCursor;
    case CropGrip::TopLeft:
    case CropGrip::BottomRight: return Qt::SizeFDiagCursor;
    case CropGrip::TopRight:
    case CropGrip::BottomLeft:  return Qt::SizeBDiagCursor;
    case CropGrip::Left:
    case CropGrip::Right:       return Qt::SizeHorCursor;
    case CropGrip::Top:
    case CropGrip::Bottom:      return Qt::SizeVerCursor;
    case CropGrip::None:        break;
    }
    return Qt::CrossCursor;
}

void CanvasView::dragCrop(const QPointF &doc)
{
    const QPointF delta = doc - m_cropStartDoc;
    QRectF r = m_cropStartRect;

    switch (m_cropGrip) {
    case CropGrip::Move:
        r.translate(delta);
        break;
    case CropGrip::TopLeft:
        r.setTopLeft(r.topLeft() + delta);
        break;
    case CropGrip::Top:
        r.setTop(r.top() + delta.y());
        break;
    case CropGrip::TopRight:
        r.setTopRight(r.topRight() + delta);
        break;
    case CropGrip::Right:
        r.setRight(r.right() + delta.x());
        break;
    case CropGrip::BottomRight:
        r.setBottomRight(r.bottomRight() + delta);
        break;
    case CropGrip::Bottom:
        r.setBottom(r.bottom() + delta.y());
        break;
    case CropGrip::BottomLeft:
        r.setBottomLeft(r.bottomLeft() + delta);
        break;
    case CropGrip::Left:
        r.setLeft(r.left() + delta.x());
        break;
    case CropGrip::None:
        return;
    }

    m_cropRect = r.normalized();
    if (m_cropRatio > 0.0 && m_cropGrip != CropGrip::Move) {
        applyCropRatio(m_cropGrip);
    }
}

void CanvasView::applyCropRatio(CropGrip grip)
{
    if (m_cropRatio <= 0.0 || m_cropRect.isEmpty()) {
        return;
    }
    QRectF r = m_cropRect.normalized();

    // Fit the ratio inside what the drag asked for, so the box never grows
    // beyond where the cursor is.
    double width = r.width();
    double height = r.height();
    if (width / height > m_cropRatio) {
        width = height * m_cropRatio;
    } else {
        height = width / m_cropRatio;
    }

    // Pivot on the corner opposite the grip, so the edge under the cursor is
    // the one that moves.
    const bool anchorLeft = grip != CropGrip::TopLeft && grip != CropGrip::Left
        && grip != CropGrip::BottomLeft;
    const bool anchorTop = grip != CropGrip::TopLeft && grip != CropGrip::Top
        && grip != CropGrip::TopRight;

    const double x = anchorLeft ? r.left() : r.right() - width;
    const double y = anchorTop ? r.top() : r.bottom() - height;
    m_cropRect = QRectF(x, y, width, height);
}

void CanvasView::paintCrop(QPainter &painter)
{
    if (m_tool != ToolId::Crop || !m_engine) {
        return;
    }
    if (toolIsSlice()) {
        paintSlices(painter);
        return;
    }
    if (cropIsPerspective()) {
        paintCropQuad(painter);
        return;
    }
    if (m_cropRect.isEmpty()) {
        return;
    }

    const QPointF topLeft = documentToWidget(m_cropRect.topLeft());
    const QPointF bottomRight = documentToWidget(m_cropRect.bottomRight());
    const QRectF box = QRectF(topLeft, bottomRight).normalized();

    painter.save();
    painter.setRenderHint(QPainter::Antialiasing, false);

    // The crop shield: everything outside the box dimmed, which is how CS6
    // shows what is about to be thrown away.
    QPainterPath shield;
    shield.addRect(rect());
    shield.addRect(box);
    painter.setPen(Qt::NoPen);
    painter.setBrush(QColor(0, 0, 0, 190));
    painter.drawPath(shield);

    // Rule-of-thirds guides, CS6's default overlay.
    painter.setBrush(Qt::NoBrush);
    painter.setPen(QPen(QColor(255, 255, 255, 90), 1));
    for (int i = 1; i <= 2; ++i) {
        const double fx = box.left() + box.width() * i / 3.0;
        const double fy = box.top() + box.height() * i / 3.0;
        painter.drawLine(QPointF(fx, box.top()), QPointF(fx, box.bottom()));
        painter.drawLine(QPointF(box.left(), fy), QPointF(box.right(), fy));
    }

    painter.setPen(QPen(QColor(255, 255, 255, 220), 1));
    painter.drawRect(box);

    // Eight handles, drawn as small filled squares on the corners and edge
    // midpoints.
    const double cx = box.center().x();
    const double cy = box.center().y();
    const QPointF handles[] = {
        box.topLeft(),  QPointF(cx, box.top()),    box.topRight(),
        QPointF(box.right(), cy),                  box.bottomRight(),
        QPointF(cx, box.bottom()),                 box.bottomLeft(),
        QPointF(box.left(), cy),
    };
    painter.setPen(QPen(QColor(40, 40, 40), 1));
    painter.setBrush(QColor(255, 255, 255));
    for (const QPointF &h : handles) {
        painter.drawRect(QRectF(h.x() - 3, h.y() - 3, 6, 6));
    }

    painter.restore();
}

void CanvasView::setQuickSelectType(QuickSelectType type)
{
    if (m_quickSelectType == type) {
        return;
    }
    finishQuickSelect();
    m_quickSelectType = type;
    update();
}

void CanvasView::setQuickSelectOptions(int brushSize, int tolerance, bool antialias,
                                       bool contiguous)
{
    m_quickBrushSize = qBound(1, brushSize, 5000);
    m_wandTolerance = qBound(0, tolerance, 255);
    m_wandAntialias = antialias;
    m_wandContiguous = contiguous;
}

void CanvasView::finishQuickSelect()
{
    if (!m_quickSelecting) {
        return;
    }
    m_quickSelecting = false;
    if (m_engine) {
        m_engine->endQuickSelect();
    }
}

bool CanvasView::nearLassoStart(const QPointF &doc) const
{
    if (m_lassoPath.size() < 2) {
        return false;
    }
    // Photoshop's hit zone is a fixed size on screen, not in the document, so
    // it stays reachable at any zoom.
    const QPointF delta = documentToWidget(doc) - documentToWidget(m_lassoPath.front());
    return QPointF::dotProduct(delta, delta) <= 8.0 * 8.0;
}

bool CanvasView::marqueeIsLineSelect() const
{
    return m_marqueeType == MarqueeType::SingleRow
        || m_marqueeType == MarqueeType::SingleColumn;
}

SelectionMode CanvasView::effectiveSelectionMode(Qt::KeyboardModifiers modifiers) const
{
    // A held modifier overrides the options-bar mode for this gesture only,
    // exactly as CS6 does — the bar's buttons do not change while you hold it.
    //
    // CS6 uses plain Shift to add and plain Alt to subtract, but bare Alt+drag
    // never reaches us on Linux: Cinnamon (and Mutter, KWin, …) grab it for
    // move-window. So Ctrl+Shift and Ctrl+Alt are the primary bindings here —
    // window managers leave those alone. Plain Shift still adds, for the
    // muscle memory of anyone coming from Photoshop.
    const bool shift = modifiers.testFlag(Qt::ShiftModifier);
    const bool alt = modifiers.testFlag(Qt::AltModifier);

    // Both together is intersect, the same combination CS6 uses.
    if (shift && alt) {
        return SelectionMode::Intersect;
    }
    if (alt) {
        return SelectionMode::Subtract;
    }
    if (shift) {
        return SelectionMode::Add;
    }
    return m_selectionMode;
}

void CanvasView::commitMarquee(const QRectF &documentRect, Qt::KeyboardModifiers modifiers)
{
    if (!m_engine) {
        return;
    }

    const int op = static_cast<int>(effectiveSelectionMode(modifiers));

    const int x = int(std::floor(documentRect.x()));
    const int y = int(std::floor(documentRect.y()));
    const int w = int(std::round(documentRect.width()));
    const int h = int(std::round(documentRect.height()));

    // The engine softens the incoming region before combining it, so the
    // feather never reaches what the selection already held.
    const int feather = m_featherRadius;

    switch (m_marqueeType) {
    case MarqueeType::Rectangular:
        m_engine->selectRect(x, y, w, h, op, feather);
        break;
    case MarqueeType::Elliptical:
        m_engine->selectEllipse(x, y, w, h, op, feather);
        break;
    case MarqueeType::SingleRow:
        // One full-width scanline through the click point.
        m_engine->selectRect(0, y, m_engine->getCanvasWidth(), 1, op, feather);
        break;
    case MarqueeType::SingleColumn:
        m_engine->selectRect(x, 0, 1, m_engine->getCanvasHeight(), op, feather);
        break;
    }
}

void CanvasView::commitLasso(Qt::KeyboardModifiers modifiers)
{
    if (!m_engine) {
        return;
    }

    // Fewer than three vertices encloses no area, so treat it as a click: in
    // New mode that clears the selection, in the combining modes it does
    // nothing — the same rule the marquee follows.
    if (m_lassoPath.size() < 3) {
        if (effectiveSelectionMode(modifiers) == SelectionMode::New) {
            m_engine->deselect();
        }
        return;
    }

    // Interleaved x,y, which is what the bridge takes. The engine closes the
    // shape back to the first vertex itself.
    QVector<float> flat;
    flat.reserve(m_lassoPath.size() * 2);
    for (const QPointF &p : m_lassoPath) {
        flat.append(float(p.x()));
        flat.append(float(p.y()));
    }

    m_engine->selectPolygon(flat, static_cast<int>(effectiveSelectionMode(modifiers)),
                            m_featherRadius);
}

void CanvasView::cancelLasso()
{
    if (m_engine && m_lassoType == LassoType::Magnetic) {
        // Drop the edge field; it is one float per pixel and only worth
        // keeping for the duration of a gesture.
        m_engine->endMagnetic();
    }
    m_marqueeActive = false;
    m_lassoPath.clear();
    m_lassoPreview.clear();
}

void CanvasView::closeLasso()
{
    // The preview segment is part of the shape once the user closes it: the
    // magnetic wire the cursor is currently pulling should not be discarded.
    for (const QPointF &p : m_lassoPreview) {
        m_lassoPath.append(p);
    }
    const Qt::KeyboardModifiers modifiers = m_gestureModifiers;
    commitLasso(modifiers);
    cancelLasso();
    update();
}

void CanvasView::updateMagneticWire(const QPointF &doc)
{
    m_lassoPreview.clear();
    if (!m_engine || m_lassoPath.isEmpty()) {
        return;
    }

    const QPointF anchor = m_lassoPath.back();
    const rust::Vec<::std::int32_t> flat =
        m_engine->magneticTrace(int(anchor.x()), int(anchor.y()), int(doc.x()), int(doc.y()),
                                m_magneticWidth);

    // The engine returns the wire including its start point, which the path
    // already holds — skip it so the two do not overlap.
    for (std::size_t i = 2; i + 1 < flat.size(); i += 2) {
        m_lassoPreview.append(QPointF(flat[i], flat[i + 1]));
    }

    // Fastening points: CS6 drops one automatically as the wire lengthens, so
    // an early part of the trace stops re-computing (and stops wobbling) once
    // the user has moved past it. Frequency 0 means never — every point stays
    // live until the user clicks one down by hand.
    if (m_magneticFrequency > 0) {
        // 100 → every ~8 px of wire, 1 → every ~100 px.
        const int limit = qMax(8, 108 - m_magneticFrequency);
        if (m_lassoPreview.size() >= limit) {
            for (const QPointF &p : m_lassoPreview) {
                m_lassoPath.append(p);
            }
            m_lassoPreview.clear();
        }
    }
}

void CanvasView::updateCursor()
{
    const ToolId effective = m_spacePanOverride ? ToolId::Hand : m_tool;
    switch (effective) {
    case ToolId::Hand:
        setCursor(m_panning ? Qt::ClosedHandCursor : Qt::OpenHandCursor);
        break;
    case ToolId::Move:
        setCursor(Qt::SizeAllCursor);
        break;
    case ToolId::Marquee:
    case ToolId::Lasso:
    case ToolId::QuickSelect:
    case ToolId::Crop:
        setCursor(Qt::CrossCursor);
        break;
    case ToolId::Zoom:
    case ToolId::Eyedropper:
        setCursor(Qt::CrossCursor);
        break;
    case ToolId::Type:
        setCursor(Qt::IBeamCursor);
        break;
    default:
        // Brush-family tools use a crosshair; a real brush-outline cursor is a
        // later refinement.
        setCursor(Qt::CrossCursor);
        break;
    }
}

// ----------------------------------------------------------------- painting --

void CanvasView::paintEvent(QPaintEvent *event)
{
    Q_UNUSED(event)
    QPainter painter(this);

    // The near-black surround.
    painter.fillRect(rect(), QColor(0x1e, 0x1e, 0x1e));

    if (m_image.isNull()) {
        return;
    }

    const QRectF target = documentRect();

    // Transparency checkerboard, clipped to the document.
    painter.save();
    painter.setClipRect(target);
    const QColor light(0xcc, 0xcc, 0xcc);
    const QColor dark(0x99, 0x99, 0x99);
    painter.fillRect(target, light);

    const int x0 = int(std::floor(target.left() / kCheckerSize));
    const int x1 = int(std::ceil(target.right() / kCheckerSize));
    const int y0 = int(std::floor(target.top() / kCheckerSize));
    const int y1 = int(std::ceil(target.bottom() / kCheckerSize));
    for (int cy = y0; cy <= y1; ++cy) {
        for (int cx = x0; cx <= x1; ++cx) {
            if ((cx + cy) % 2 == 0) {
                continue;
            }
            painter.fillRect(QRectF(cx * kCheckerSize, cy * kCheckerSize,
                                    kCheckerSize, kCheckerSize),
                             dark);
        }
    }
    painter.restore();

    // At 200% and above Photoshop switches to nearest-neighbour so individual
    // pixels stay crisp; below that it smooths.
    painter.setRenderHint(QPainter::SmoothPixmapTransform, m_zoom < 2.0);
    painter.drawImage(target, m_image);

    // A thin border so the document edge reads against the surround.
    painter.setPen(QPen(QColor(0x00, 0x00, 0x00, 160), 1));
    painter.setBrush(Qt::NoBrush);
    painter.drawRect(target.adjusted(-0.5, -0.5, 0.5, 0.5));

    paintSelection(painter);
    paintCrop(painter);
    if (toolIsAnnotation()) {
        paintAnnotations(painter);
    }
    paintHealing(painter);
    paintCloneSource(painter);
    paintGradientDrag(painter);
    paintPathOverlay(painter);

    // Live marquee while the user is dragging one out. Drawn as the shape the
    // active variant will actually produce, so an elliptical drag previews an
    // ellipse rather than its bounding box.
    if (m_marqueeActive && (toolIsLasso() || m_healingTracing)) {
        // The outline so far, plus whatever segment the cursor is currently
        // pulling — the rubber band for polygonal, the live wire for
        // magnetic — and the straight line back to the start that closing it
        // will use. CS6 shows all three.
        QPolygonF widgetPath;
        widgetPath.reserve(m_lassoPath.size() + m_lassoPreview.size() + 1);
        for (const QPointF &p : m_lassoPath) {
            widgetPath.append(documentToWidget(p));
        }
        for (const QPointF &p : m_lassoPreview) {
            widgetPath.append(documentToWidget(p));
        }
        if (lassoIsClicked() && !widgetPath.isEmpty()) {
            widgetPath.append(documentToWidget(m_lassoCursor));
        }

        if (widgetPath.size() > 1) {
            painter.save();
            painter.setRenderHint(QPainter::Antialiasing, true);
            painter.setBrush(Qt::NoBrush);

            painter.setPen(QPen(Qt::white, 1, Qt::SolidLine));
            painter.drawPolygon(widgetPath);

            QPen dashed(Qt::black, 1, Qt::DashLine);
            dashed.setDashPattern({4, 4});
            painter.setPen(dashed);
            painter.drawPolygon(widgetPath);

            // Anchors, so the user can see where a click landed and what
            // Backspace would take back. Only the ones they placed by hand —
            // marking every magnetic wire point would be a solid line.
            if (lassoIsClicked()) {
                painter.setPen(QPen(Qt::black, 1));
                painter.setBrush(Qt::white);
                const QPointF first = documentToWidget(m_lassoPath.front());
                painter.drawRect(QRectF(first.x() - 2.5, first.y() - 2.5, 5, 5));
            }
            painter.restore();
        }
    } else if (m_marqueeActive && !m_marquee.isNull()) {
        const QPointF topLeft = documentToWidget(m_marquee.topLeft());
        const QPointF bottomRight = documentToWidget(m_marquee.bottomRight());
        const QRectF r(topLeft, bottomRight);
        const bool ellipse =
            m_tool == ToolId::Marquee && m_marqueeType == MarqueeType::Elliptical;

        painter.setBrush(Qt::NoBrush);
        painter.setPen(QPen(Qt::white, 1, Qt::SolidLine));
        if (ellipse) {
            painter.drawEllipse(r);
        } else {
            painter.drawRect(r);
        }

        QPen dashed(Qt::black, 1, Qt::DashLine);
        dashed.setDashPattern({4, 4});
        painter.setPen(dashed);
        if (ellipse) {
            painter.drawEllipse(r);
        } else {
            painter.drawRect(r);
        }
    }
}

void CanvasView::paintSelection(QPainter &painter)
{
    if (m_selectionPath.isEmpty()) {
        return;
    }

    // The cached path is in document coordinates; map it once rather than
    // transforming every point by hand.
    const QPointF origin = documentOrigin();
    QTransform toWidget;
    toWidget.translate(origin.x(), origin.y());
    toWidget.scale(m_zoom, m_zoom);
    const QPainterPath path = toWidget.map(m_selectionPath);

    painter.save();
    // The contour follows pixel edges, so it is all axis-aligned lines.
    // Antialiasing them only blurs the ants across two rows of pixels.
    painter.setRenderHint(QPainter::Antialiasing, false);
    painter.setBrush(Qt::NoBrush);

    // Photoshop's ants are black dashes over white, so the outline stays
    // visible on light and dark image content alike.
    painter.setPen(QPen(Qt::white, 1, Qt::SolidLine));
    painter.drawPath(path);

    QPen ants(Qt::black, 1, Qt::CustomDashLine);
    ants.setDashPattern({4, 4});
    ants.setDashOffset(m_antsOffset);
    painter.setPen(ants);
    painter.drawPath(path);
    painter.restore();
}

// ------------------------------------------------------------------- input --

void CanvasView::mousePressEvent(QMouseEvent *event)
{
    setFocus(Qt::MouseFocusReason);
    m_lastMousePos = event->position();
    const QPointF doc = widgetToDocument(event->position());

    // Middle-drag and space-drag pan from any tool, as in Photoshop.
    const bool wantsPan = m_spacePanOverride || event->button() == Qt::MiddleButton
        || m_tool == ToolId::Hand;
    if (wantsPan) {
        m_panning = true;
        updateCursor();
        return;
    }

    if (event->button() != Qt::LeftButton) {
        return;
    }

    switch (m_tool) {
    case ToolId::Zoom:
        // Alt inverts the zoom direction, matching CS6.
        if (event->modifiers() & Qt::AltModifier) {
            zoomOut();
        } else {
            zoomIn();
        }
        return;

    case ToolId::Eyedropper: {
        if (toolIsAnnotation()) {
            annotationPress(doc, event->modifiers());
            return;
        }
        if (m_engine) {
            const QColor picked = m_engine->pickColor(int(doc.x()), int(doc.y()));
            m_engine->setForegroundColor(picked);
            emit colorPicked(picked);
        }
        return;
    }

    case ToolId::Marquee:
        // Single row and column are a click, not a drag: select immediately,
        // then keep following the cursor while the button is held.
        if (marqueeIsLineSelect()) {
            m_marqueeActive = true;
            m_gestureModifiers = event->modifiers();
            m_dragStartDoc = doc;
            commitMarquee(QRectF(doc, doc), m_gestureModifiers);
            update();
            return;
        }
        [[fallthrough]];
    case ToolId::Lasso:
        if (lassoIsClicked()) {
            // Polygonal and magnetic are click-driven: the outline is built up
            // across several clicks with the button released between them, so
            // a press either opens a new shape, adds an anchor, or closes it.
            if (!m_marqueeActive) {
                m_marqueeActive = true;
                m_gestureModifiers = event->modifiers();
                m_lassoPath.clear();
                m_lassoPreview.clear();
                m_lassoPath.append(doc);
                m_lassoCursor = doc;
                if (m_engine && m_lassoType == LassoType::Magnetic) {
                    m_engine->beginMagnetic(m_magneticContrast);
                }
            } else if (nearLassoStart(doc)) {
                closeLasso();
                return;
            } else {
                // Clicking lays the live segment down as a fastening point.
                for (const QPointF &p : m_lassoPreview) {
                    m_lassoPath.append(p);
                }
                m_lassoPreview.clear();
                m_lassoPath.append(doc);
                m_lassoCursor = doc;
            }
            update();
            return;
        }
        [[fallthrough]];
    case ToolId::QuickSelect:
        if (m_tool == ToolId::QuickSelect) {
            m_gestureModifiers = event->modifiers();
            if (!m_engine) {
                return;
            }
            if (m_quickSelectType == QuickSelectType::MagicWand) {
                // One click is the whole gesture.
                m_engine->magicWand(int(doc.x()), int(doc.y()), m_wandTolerance,
                                    m_wandContiguous, m_wandAntialias,
                                    static_cast<int>(effectiveSelectionMode(m_gestureModifiers)),
                                    m_featherRadius);
                return;
            }
            // Quick Selection: a drag made of brush dabs, each growing the
            // region. The engine snapshots the selection now so a subtract
            // part-way through the drag can give pixels back.
            m_engine->beginQuickSelect(
                static_cast<int>(effectiveSelectionMode(m_gestureModifiers)), m_featherRadius);
            m_quickSelecting = true;
            m_engine->quickSelectDab(float(doc.x()), float(doc.y()),
                                     float(m_quickBrushSize) / 2.0f,
                                     effectiveSelectionMode(m_gestureModifiers)
                                         == SelectionMode::Subtract);
            return;
        }
        m_marqueeActive = true;
        m_gestureModifiers = event->modifiers();
        m_dragStartDoc = doc;
        m_marquee = QRectF(doc, doc);
        m_lassoPath.clear();
        if (lassoIsDragged()) {
            m_lassoPath.append(doc);
        }
        update();
        return;

    case ToolId::Crop: {
        if (toolIsSlice()) {
            m_sliceStartDoc = doc;
            if (m_cropType == CropType::Slice) {
                // Drag out a new user slice.
                m_sliceDragging = true;
                m_sliceDrag = QRectF(doc, doc);
                update();
                return;
            }

            // Slice Select: grab a handle of the selected slice, otherwise
            // select whatever is under the cursor.
            if (m_selectedSlice >= 0) {
                for (const SliceInfo &slice : m_slices) {
                    if (slice.userIndex != m_selectedSlice) {
                        continue;
                    }
                    const CropGrip grip = gripAt(slice.rect, event->position());
                    if (grip != CropGrip::None) {
                        m_sliceGrip = grip;
                        m_sliceStartRect = slice.rect;
                        update();
                        return;
                    }
                }
            }

            const int hit = sliceAt(doc);
            // Auto slices are not editable — they exist only as the leftovers
            // between user slices — so clicking one clears the selection.
            m_selectedSlice = hit >= 0 ? m_slices.at(hit).userIndex : -1;
            if (m_selectedSlice >= 0) {
                m_sliceGrip = CropGrip::Move;
                m_sliceStartRect = m_slices.at(hit).rect;
            }
            update();
            return;
        }

        if (cropIsPerspective()) {
            m_cropStartDoc = doc;
            m_cropStartQuad = m_cropQuad;
            m_cropCorner = cropCornerAt(event->position());
            m_cropQuadMoving = false;
            m_cropQuadNew = false;

            if (m_cropCorner < 0) {
                if (m_cropQuad.containsPoint(doc, Qt::OddEvenFill)) {
                    m_cropQuadMoving = true;
                } else {
                    // Pressing outside starts a fresh quad, dragged out as a
                    // rectangle first; the corners are pulled off square
                    // afterwards. That is CS6's order of operations.
                    m_cropQuadNew = true;
                    m_cropQuad = QPolygonF({doc, doc, doc, doc});
                }
            }
            update();
            return;
        }

        // Grabbing a handle adjusts the existing box; pressing outside it
        // starts a new one from scratch, as CS6 does.
        const CropGrip grip = cropGripAt(event->position());
        if (grip == CropGrip::None) {
            m_cropRect = QRectF(doc, doc);
            m_cropGrip = CropGrip::BottomRight;
        } else {
            m_cropGrip = grip;
        }
        m_cropStartRect = m_cropRect;
        m_cropStartDoc = doc;
        update();
        return;
    }

    case ToolId::Gradient:
        // The Paint Bucket fills where it is clicked; the Gradient tool drags out
        // the ramp's axis instead.
        if (m_gradientTool == GradientTool::PaintBucket) {
            if (m_engine && !m_engine->fillBucket(int(std::floor(doc.x())),
                                                  int(std::floor(doc.y())))) {
                // Refused: a locked layer, or nothing under the click matched
                // within Tolerance — Photoshop is silent about the latter.
                reportIfLocked();
            }
            refresh();
            return;
        }
        // Nothing is drawn until the drag is released, which is how CS6 behaves:
        // only the axis line follows the cursor.
        m_gradientDragging = true;
        m_gradientStart = doc;
        m_gradientEnd = doc;
        update();
        return;

    case ToolId::Move:
        // A position-locked layer cannot be dragged, so say so rather than
        // letting the user push at a layer that will not budge.
        if (m_engine && m_engine->layerLockPosition(m_engine->getActiveLayerIndex())) {
            emit lockedLayerRefused();
            return;
        }
        m_dragging = true;
        m_dragStartDoc = doc;
        return;

    case ToolId::Pen:
        penPress(doc);
        return;

    case ToolId::PathSelect:
        pathSelectPress(doc);
        return;

    default:
        break;
    }

    // The healing group's non-brush variants have their own gestures; the two
    // brushes fall through to the stroke path below.
    if (m_tool == ToolId::Healing && healingPress(doc, event->modifiers())) {
        return;
    }

    // The six retouch tools — Blur, Sharpen and Smudge, Dodge, Burn and Sponge —
    // all work on what they pass over, dab by dab: each dab has to act on what
    // the last one left, which is what makes dwelling deepen the effect and what
    // lets a smudge drag colour along.
    if (m_retouchMode && m_engine) {
        if (m_engine->beginRetouchStroke(float(doc.x()), float(doc.y()), 1.0f)) {
            m_retouching = true;
            m_dragging = true;
            refresh();
        } else {
            reportIfLocked();
        }
        return;
    }

    // The Mixer Brush mixes with what is already there, so like colour
    // replacement it edits the layer per dab rather than accumulating a stroke.
    if (m_mixerMode && m_engine) {
        // Alt-click loads the brush from the image, as CS6 does — picking paint
        // up off the canvas is how the tool is meant to be filled. A one-off
        // action, not the start of a stroke.
        if (event->modifiers() & Qt::AltModifier) {
            m_engine->loadMixerBrushFrom(int(doc.x()), int(doc.y()));
            emit mixerLoadChanged();
            emit statusMessage(tr("Brush loaded from %1, %2")
                                   .arg(int(doc.x()))
                                   .arg(int(doc.y())));
            return;
        }
        if (m_engine->beginMixer(float(doc.x()), float(doc.y()), 1.0f)) {
            m_mixing = true;
            m_dragging = true;
            refresh();
        } else {
            reportIfLocked();
        }
        return;
    }

    // The Color Replacement Brush recolours what is already there, so it edits
    // the layer per dab rather than accumulating a stroke to composite.
    if (m_replaceMode && m_engine) {
        if (m_engine->beginReplace(float(doc.x()), float(doc.y()), 1.0f)) {
            m_replacing = true;
            m_dragging = true;
            refresh();
        } else {
            reportIfLocked();
        }
        return;
    }

    // The Clone Stamp copies pixels rather than painting a colour, and needs a
    // source Alt-clicked first. Its stroke is otherwise an ordinary one, so this
    // only replaces the *beginning* of it.
    if (m_tool == ToolId::CloneStamp && clonePress(doc, event->modifiers())) {
        return;
    }

    if (toolPaints(m_tool) && m_engine) {
        // A tablet would supply real pressure here; a mouse reports full.
        if (m_engine->beginStroke(float(doc.x()), float(doc.y()), 1.0f)) {
            m_dragging = true;
            m_image = m_engine->previewImage();
            update();
        } else {
            reportIfLocked();
        }
    }
}

void CanvasView::mouseMoveEvent(QMouseEvent *event)
{
    const QPointF pos = event->position();
    const QPointF doc = widgetToDocument(pos);
    emit cursorMoved(doc);

    if (m_panning) {
        m_pan += pos - m_lastMousePos;
        m_lastMousePos = pos;
        clampPan();
        update();
        return;
    }

    if (toolIsAnnotation()) {
        annotationDrag(doc);
        update();
        m_lastMousePos = pos;
        return;
    }

    if (toolIsSlice()) {
        if (m_sliceDragging) {
            m_sliceDrag = QRectF(m_sliceStartDoc, doc).normalized();
        } else if (m_sliceGrip != CropGrip::None && m_selectedSlice >= 0 && m_engine) {
            // Reuse the crop box's move/resize maths, then push the result
            // straight to the engine so the auto slices around it keep up.
            const QRectF saveRect = m_cropRect;
            const QRectF saveStart = m_cropStartRect;
            const QPointF saveDoc = m_cropStartDoc;
            const CropGrip saveGrip = m_cropGrip;

            m_cropStartRect = m_sliceStartRect;
            m_cropStartDoc = m_sliceStartDoc;
            m_cropGrip = m_sliceGrip;
            dragCrop(doc);
            const QRectF moved = m_cropRect.normalized();

            m_cropRect = saveRect;
            m_cropStartRect = saveStart;
            m_cropStartDoc = saveDoc;
            m_cropGrip = saveGrip;

            if (moved.width() >= 1.0 && moved.height() >= 1.0) {
                m_engine->setUserSlice(m_selectedSlice, int(std::round(moved.x())),
                                       int(std::round(moved.y())),
                                       int(std::round(moved.width())),
                                       int(std::round(moved.height())));
            }
        } else if (m_selectedSlice >= 0) {
            for (const SliceInfo &slice : m_slices) {
                if (slice.userIndex == m_selectedSlice) {
                    setCursor(cropCursor(gripAt(slice.rect, pos)));
                    break;
                }
            }
        } else {
            setCursor(Qt::CrossCursor);
        }
        update();
        m_lastMousePos = pos;
        return;
    }

    if (m_tool == ToolId::Pen) {
        penMove(doc, event->modifiers());
        m_lastMousePos = pos;
        return;
    }

    if (m_tool == ToolId::PathSelect) {
        pathSelectMove(doc, event->modifiers());
        m_lastMousePos = pos;
        return;
    }

    if (cropIsPerspective()) {
        if (m_cropQuadNew) {
            // Still dragging the initial rectangle out.
            const QRectF r = QRectF(m_cropStartDoc, doc).normalized();
            m_cropQuad = QPolygonF({r.topLeft(), r.topRight(), r.bottomRight(), r.bottomLeft()});
        } else if (m_cropCorner >= 0) {
            m_cropQuad[m_cropCorner] = m_cropStartQuad.at(m_cropCorner) + (doc - m_cropStartDoc);
        } else if (m_cropQuadMoving) {
            m_cropQuad = m_cropStartQuad.translated(doc - m_cropStartDoc);
        } else {
            setCursor(cropCornerAt(pos) >= 0 ? Qt::SizeAllCursor : Qt::CrossCursor);
        }
        update();
        m_lastMousePos = pos;
        return;
    }

    if (m_tool == ToolId::Crop) {
        if (m_cropGrip != CropGrip::None) {
            dragCrop(doc);
            update();
        } else {
            // Not dragging: the cursor advertises what each handle would do.
            setCursor(cropCursor(cropGripAt(pos)));
        }
        m_lastMousePos = pos;
        return;
    }

    if (m_quickSelecting && m_engine) {
        m_engine->quickSelectDab(float(doc.x()), float(doc.y()),
                                 float(m_quickBrushSize) / 2.0f,
                                 effectiveSelectionMode(m_gestureModifiers)
                                     == SelectionMode::Subtract);
        m_lastMousePos = pos;
        return;
    }

    if (m_tool == ToolId::Healing && healingDrag(doc)) {
        m_lastMousePos = pos;
        return;
    }

    // The click-driven lassos track the cursor with no button held, so this
    // runs before the drag-gesture branch below.
    if (m_marqueeActive && lassoIsClicked()) {
        m_lassoCursor = doc;
        if (m_lassoType == LassoType::Magnetic) {
            updateMagneticWire(doc);
        }
        update();
        m_lastMousePos = pos;
        return;
    }

    if (m_marqueeActive) {
        if (freehandTracing()) {
            // Sample the path rather than recording every motion event: one
            // vertex per document pixel of travel is finer than the mask can
            // represent, and keeps the polygon small enough to rasterise
            // instantly on release.
            const QPointF last = m_lassoPath.isEmpty() ? m_dragStartDoc : m_lassoPath.back();
            const QPointF step = doc - last;
            if (step.manhattanLength() >= 1.0) {
                m_lassoPath.append(doc);
            }
        } else if (m_tool == ToolId::Marquee && marqueeIsLineSelect()) {
            // Dragging slides the selected line under the cursor: in New mode
            // each commit replaces the last, so no band accumulates. In the
            // combining modes it does accumulate, which is what CS6 does too.
            commitMarquee(QRectF(doc, doc), m_gestureModifiers);
        } else {
            m_marquee = QRectF(m_dragStartDoc, doc).normalized();
        }
        update();
        m_lastMousePos = pos;
        return;
    }

    if (m_gradientDragging) {
        m_gradientEnd = constrainedGradientEnd(doc, event->modifiers());
        m_lastMousePos = pos;
        update();
        return;
    }

    if (m_retouching && m_engine) {
        m_engine->extendRetouchStroke(float(doc.x()), float(doc.y()), 1.0f);
        m_lastMousePos = pos;
        return;
    }

    if (m_mixing && m_engine) {
        m_engine->extendMixer(float(doc.x()), float(doc.y()), 1.0f);
        m_lastMousePos = pos;
        return;
    }

    if (m_replacing && m_engine) {
        m_engine->extendReplace(float(doc.x()), float(doc.y()), 1.0f);
        m_lastMousePos = pos;
        return;
    }

    if (m_dragging && m_engine) {
        if (m_tool == ToolId::Move) {
            // Nudge by whole pixels only; sub-pixel layer offsets would need
            // resampling on every drag step.
            const QPointF delta = doc - m_dragStartDoc;
            const int dx = int(delta.x());
            const int dy = int(delta.y());
            if (dx != 0 || dy != 0) {
                m_engine->offsetLayer(m_engine->getActiveLayerIndex(), dx, dy);
                m_dragStartDoc += QPointF(dx, dy);
            }
        } else if (toolPaints(m_tool)) {
            m_engine->extendStroke(float(doc.x()), float(doc.y()), 1.0f);
            // Show the stroke live without committing it to the layer.
            m_image = m_engine->previewImage();
            update();
        }
    }

    m_lastMousePos = pos;
}

void CanvasView::mouseDoubleClickEvent(QMouseEvent *event)
{
    // Double-clicking closes a polygonal or magnetic outline wherever the
    // cursor is, joining back to the first anchor — CS6's own gesture.
    if (m_marqueeActive && lassoIsClicked()) {
        closeLasso();
        event->accept();
        return;
    }

    // Double-clicking finishes an open path the same way Enter does. The
    // click that started this double-click already placed its anchor through
    // the ordinary press/release pair just before it; there is nothing left
    // to do here but stop extending it.
    if (m_tool == ToolId::Pen && m_engine && m_engine->pathIsEditing()) {
        m_engine->pathFinishEditing();
        update();
        event->accept();
        return;
    }

    // The slice tools have no commit gesture — slices are document data, not
    // a pending edit — so a double-click there is just two clicks.
    if (toolIsSlice()) {
        QWidget::mouseDoubleClickEvent(event);
        return;
    }

    // Double-clicking inside the crop box commits it, as CS6 does.
    if (cropIsPerspective()) {
        if (m_cropQuad.containsPoint(widgetToDocument(event->position()), Qt::OddEvenFill)) {
            commitCrop();
            event->accept();
            return;
        }
    } else if (m_tool == ToolId::Crop && cropGripAt(event->position()) != CropGrip::None) {
        commitCrop();
        event->accept();
        return;
    }

    QWidget::mouseDoubleClickEvent(event);
}

void CanvasView::mouseReleaseEvent(QMouseEvent *event)
{
    const QPointF doc = widgetToDocument(event->position());

    if (m_panning) {
        m_panning = false;
        updateCursor();
        return;
    }

    if (toolIsSlice()) {
        if (m_sliceDragging) {
            m_sliceDragging = false;
            const QRectF r = m_sliceDrag.normalized();
            if (m_engine && r.width() >= 1.0 && r.height() >= 1.0) {
                const int index = m_engine->addSlice(int(std::round(r.x())),
                                                     int(std::round(r.y())),
                                                     int(std::round(r.width())),
                                                     int(std::round(r.height())));
                // Drawing a slice selects it, so it can be nudged or deleted
                // straight away with the Slice Select tool.
                m_selectedSlice = index;
            }
            m_sliceDrag = QRectF();
        }
        m_sliceGrip = CropGrip::None;
        update();
        return;
    }

    if (m_tool == ToolId::Pen) {
        penRelease(doc);
        update();
        return;
    }

    if (m_tool == ToolId::PathSelect) {
        pathSelectRelease();
        return;
    }

    if (cropIsPerspective()) {
        const bool wasNew = m_cropQuadNew;
        m_cropCorner = -1;
        m_cropQuadMoving = false;
        m_cropQuadNew = false;

        // A click rather than a drag would leave a quad with no area and four
        // coincident handles, so put the whole canvas back.
        if (wasNew) {
            const QRectF bounds = m_cropQuad.boundingRect();
            if (bounds.width() < 1.0 || bounds.height() < 1.0) {
                resetCrop();
            }
        }
        update();
        return;
    }

    if (m_tool == ToolId::Crop && m_cropGrip != CropGrip::None) {
        m_cropGrip = CropGrip::None;
        m_cropRect = m_cropRect.normalized();
        // A stray click rather than a drag would leave a zero-size box with no
        // handles to grab, so put the whole canvas back.
        if (m_cropRect.width() < 1.0 || m_cropRect.height() < 1.0) {
            resetCrop();
        }
        // The box is clipped to the canvas: cropping to somewhere the document
        // does not reach is not a crop.
        if (m_engine) {
            const QRectF canvas(0, 0, m_engine->getCanvasWidth(), m_engine->getCanvasHeight());
            m_cropRect = m_cropRect.intersected(canvas);
            if (m_cropRect.width() < 1.0 || m_cropRect.height() < 1.0) {
                resetCrop();
            }
        }
        update();
        return;
    }

    if (m_tool == ToolId::Healing && healingRelease()) {
        return;
    }

    if (toolIsAnnotation()) {
        m_draggedMarker = -1;
        m_draggedRulerEnd = -1;
        return;
    }

    if (m_quickSelecting) {
        finishQuickSelect();
        return;
    }

    // A click-driven lasso stays open across the release — the press handler
    // already did the work, and the shape is not finished until the user
    // closes it. Checked before the flag is cleared below.
    if (m_marqueeActive && lassoIsClicked()) {
        return;
    }

    if (m_marqueeActive) {
        m_marqueeActive = false;

        // The line variants already committed on press and drag.
        if (m_tool == ToolId::Marquee && marqueeIsLineSelect()) {
            m_marquee = QRectF();
            update();
            return;
        }

        if (freehandTracing()) {
            // Photoshop closes the lasso with a straight line from where you
            // let go back to where you started, however far apart they are.
            if (!m_lassoPath.isEmpty() && (doc - m_lassoPath.back()).manhattanLength() >= 1.0) {
                m_lassoPath.append(doc);
            }
            commitLasso(m_gestureModifiers);
            m_lassoPath.clear();
            m_healingTracing = false;
            m_marquee = QRectF();
            update();
            return;
        }

        const QRectF r = QRectF(m_dragStartDoc, doc).normalized();
        if (m_engine) {
            if (r.width() >= 1.0 && r.height() >= 1.0) {
                commitMarquee(r, m_gestureModifiers);
            } else if (effectiveSelectionMode(m_gestureModifiers) == SelectionMode::New) {
                // A click without a drag clears the selection, as in Photoshop
                // — but only in New mode; a stray click while adding or
                // subtracting leaves the selection alone.
                m_engine->deselect();
            }
        }
        m_marquee = QRectF();
        update();
        return;
    }

    if (m_gradientDragging) {
        m_gradientDragging = false;
        m_gradientEnd = constrainedGradientEnd(doc, event->modifiers());
        if (m_engine
            && !m_engine->drawGradient(float(m_gradientStart.x()), float(m_gradientStart.y()),
                                       float(m_gradientEnd.x()), float(m_gradientEnd.y()))) {
            // Refused: either the layer is locked, or the drag was too short to
            // have a direction — a click alone draws nothing, as in Photoshop.
            reportIfLocked();
        }
        refresh();
        return;
    }

    if (m_retouching && m_engine) {
        m_engine->endRetouchStroke();
        m_retouching = false;
        m_dragging = false;
        refresh();
        return;
    }

    if (m_mixing && m_engine) {
        m_engine->endMixer();
        m_mixing = false;
        m_dragging = false;
        refresh();
        // The stroke leaves the brush carrying different paint, unless it was
        // cleaned or reloaded — either way the load swatch needs re-reading.
        emit mixerLoadChanged();
        return;
    }

    if (m_replacing && m_engine) {
        m_engine->endReplace();
        m_replacing = false;
        m_dragging = false;
        refresh();
        return;
    }

    if (m_dragging && m_engine && toolPaints(m_tool)) {
        m_engine->endStroke();
        refresh();
    }
    m_dragging = false;
}

void CanvasView::wheelEvent(QWheelEvent *event)
{
    // Ctrl+wheel zooms; plain wheel scrolls, matching Photoshop's default.
    if (event->modifiers() & Qt::ControlModifier) {
        const double factor = event->angleDelta().y() > 0 ? 1.15 : 1.0 / 1.15;
        setZoomAt(m_zoom * factor, event->position());
        event->accept();
        return;
    }

    const QPoint delta = event->angleDelta();
    if (event->modifiers() & Qt::ShiftModifier) {
        m_pan += QPointF(delta.y() / 2.0, 0.0);
    } else {
        m_pan += QPointF(delta.x() / 2.0, delta.y() / 2.0);
    }
    clampPan();
    update();
    event->accept();
}

void CanvasView::keyPressEvent(QKeyEvent *event)
{
    // Holding space temporarily swaps in the Hand tool.
    if (event->key() == Qt::Key_Space && !event->isAutoRepeat()) {
        m_spacePanOverride = true;
        updateCursor();
        event->accept();
        return;
    }

    // Arrow keys nudge the active layer by one pixel, ten with Shift.
    if (m_engine && m_tool == ToolId::Move) {
        const int step = (event->modifiers() & Qt::ShiftModifier) ? 10 : 1;
        int dx = 0;
        int dy = 0;
        switch (event->key()) {
        case Qt::Key_Left:  dx = -step; break;
        case Qt::Key_Right: dx = step;  break;
        case Qt::Key_Up:    dy = -step; break;
        case Qt::Key_Down:  dy = step;  break;
        default: break;
        }
        if (dx != 0 || dy != 0) {
            m_engine->offsetLayer(m_engine->getActiveLayerIndex(), dx, dy);
            event->accept();
            return;
        }
    }

    // Crop takes Enter to commit and Esc to abandon the box — the Esc branch
    // below would otherwise only clear the gesture state.
    if (toolIsSlice()) {
        // Delete removes the selected slice; Esc just drops the selection.
        if (event->key() == Qt::Key_Delete || event->key() == Qt::Key_Backspace) {
            deleteSelectedSlice();
            event->accept();
            return;
        }
        if (event->key() == Qt::Key_Escape) {
            m_selectedSlice = -1;
            m_sliceDragging = false;
            m_sliceDrag = QRectF();
            update();
            event->accept();
            return;
        }
    } else if (m_tool == ToolId::Crop) {
        if (event->key() == Qt::Key_Return || event->key() == Qt::Key_Enter) {
            commitCrop();
            event->accept();
            return;
        }
        if (event->key() == Qt::Key_Escape) {
            resetCrop();
            event->accept();
            return;
        }
    } else if (m_tool == ToolId::Pen && m_engine && m_engine->pathIsEditing()) {
        // Enter and Esc both stop extending the open subpath without closing
        // it — Esc does not throw the points away, matching Photoshop.
        if (event->key() == Qt::Key_Return || event->key() == Qt::Key_Enter
            || event->key() == Qt::Key_Escape) {
            m_engine->pathFinishEditing();
            update();
            event->accept();
            return;
        }
    }

    // An open polygonal or magnetic outline takes the keys CS6 gives it:
    // Enter closes it, Backspace takes back the last fastening point.
    if (m_marqueeActive && lassoIsClicked()) {
        if (event->key() == Qt::Key_Return || event->key() == Qt::Key_Enter) {
            closeLasso();
            event->accept();
            return;
        }
        if (event->key() == Qt::Key_Backspace || event->key() == Qt::Key_Delete) {
            if (m_lassoPath.size() > 1) {
                m_lassoPath.removeLast();
                m_lassoPreview.clear();
                if (m_lassoType == LassoType::Magnetic) {
                    updateMagneticWire(m_lassoCursor);
                }
            } else {
                // Backspacing off the first anchor abandons the shape.
                cancelLasso();
            }
            update();
            event->accept();
            return;
        }
    }

    // Escape abandons an in-progress stroke or marquee.
    if (event->key() == Qt::Key_Escape) {
        if (m_replacing && m_engine) {
            m_engine->cancelReplace();
            m_replacing = false;
            m_dragging = false;
            refresh();
        }
        if (m_gradientDragging) {
            m_gradientDragging = false;
            update();
        }
        if (m_retouching && m_engine) {
            m_engine->cancelRetouchStroke();
            m_retouching = false;
            m_dragging = false;
            refresh();
        }
        if (m_mixing && m_engine) {
            m_engine->cancelMixer();
            m_mixing = false;
            m_dragging = false;
            refresh();
        }
        if (m_dragging && m_engine) {
            m_engine->cancelStroke();
            m_dragging = false;
            refresh();
        }
        m_pathSelectGesture = PathSelectGesture::None;
        cancelLasso();
        finishQuickSelect();
        update();
        event->accept();
        return;
    }

    QWidget::keyPressEvent(event);
}

void CanvasView::keyReleaseEvent(QKeyEvent *event)
{
    if (event->key() == Qt::Key_Space && !event->isAutoRepeat()) {
        m_spacePanOverride = false;
        m_panning = false;
        updateCursor();
        event->accept();
        return;
    }
    QWidget::keyReleaseEvent(event);
}

void CanvasView::contextMenuEvent(QContextMenuEvent *event)
{
    // CS6 gives every tool its own right-click menu; the selection tools are
    // the ones that have one here so far.
    if (!toolSelects(m_tool)) {
        QWidget::contextMenuEvent(event);
        return;
    }

    // Mid-gesture a right-click is not a request for a menu — it would open
    // on top of the marquee being dragged.
    if (m_marqueeActive || m_dragging || m_panning) {
        event->ignore();
        return;
    }

    emit contextMenuRequested(event->globalPos());
    event->accept();
}

void CanvasView::resizeEvent(QResizeEvent *event)
{
    QWidget::resizeEvent(event);
    clampPan();
}

void CanvasView::enterEvent(QEnterEvent *event)
{
    QWidget::enterEvent(event);
    updateCursor();
}

void CanvasView::leaveEvent(QEvent *event)
{
    QWidget::leaveEvent(event);
    unsetCursor();
    emit cursorLeft();
}

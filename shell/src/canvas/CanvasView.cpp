#include "CanvasView.h"

#include "../tools/ToolIcons.h"
#include "cxx-qt-lib/qcolor.h"
#include "photorust_core/src/bridge.cxxqt.h"

#include <QContextMenuEvent>
#include <QScrollBar>
#include <QCursor>
#include <QGuiApplication>
#include <algorithm>
#include <QEnterEvent>
#include <QEvent>
#include <QFontDatabase>
#include <QFontMetrics>
#include <QKeyEvent>
#include <QMessageBox>
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

/// How far a Zoom drag has to run, in screen pixels, before it counts as a
/// rectangle rather than a click. Below this a press-and-twitch would zoom to a
/// few pixels instead of stepping in, which is never what was meant.
constexpr double kZoomMarqueeMinimum = 8.0;

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

    m_hScroll = new QScrollBar(Qt::Horizontal, this);
    m_vScroll = new QScrollBar(Qt::Vertical, this);
    m_hScroll->setObjectName(QStringLiteral("canvasHScroll"));
    m_vScroll->setObjectName(QStringLiteral("canvasVScroll"));

    connect(m_hScroll, &QScrollBar::valueChanged, this, [this](int value) {
        if (m_scrollBarUpdating) return;
        m_pan.setX(-value);
        update();
    });
    connect(m_vScroll, &QScrollBar::valueChanged, this, [this](int value) {
        if (m_scrollBarUpdating) return;
        m_pan.setY(-value);
        update();
    });

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

QColor CanvasView::colorAtGlobal(const QPoint &globalPos) const
{
    if (!m_engine || m_image.isNull()) {
        return {};
    }
    const QPoint local = mapFromGlobal(globalPos);
    if (!rect().contains(local)) {
        return {};
    }

    const QPointF doc = widgetToDocument(QPointF(local));
    if (doc.x() < 0.0 || doc.y() < 0.0 || doc.x() >= m_image.width()
        || doc.y() >= m_image.height()) {
        return {};
    }
    return m_engine->pickColor(int(doc.x()), int(doc.y()));
}

void CanvasView::setHandTool(HandTool tool)
{
    if (m_handTool == tool) {
        return;
    }
    m_handTool = tool;
    m_rotatingView = false;
    updateCursor();
}

QPointF CanvasView::uprightDelta(const QPointF &screenDelta) const
{
    // The pan is kept in the frame the document is laid out in, which the view
    // rotation then turns. So a movement measured on screen — a hand drag, or
    // the correction that keeps a pixel under the cursor while zooming — has to
    // be turned back before it can be added to it. Without this both slide off
    // at an angle to the hand once the canvas is turned.
    if (qFuzzyIsNull(m_viewRotation)) {
        return screenDelta;
    }
    QTransform unrotate;
    unrotate.rotate(-m_viewRotation);
    return unrotate.map(screenDelta);
}

double CanvasView::angleToPointer(const QPointF &widgetPos) const
{
    // Measured from the middle of the viewport, which is what the view turns
    // about.
    const QPointF centre(width() / 2.0, height() / 2.0);
    const QPointF delta = widgetPos - centre;
    return std::atan2(delta.y(), delta.x()) * 180.0 / M_PI;
}

QTransform CanvasView::viewTransform() const
{
    if (qFuzzyIsNull(m_viewRotation)) {
        return {};
    }
    // About the middle of the viewport rather than the middle of the document:
    // what the user is looking at stays where it is as the canvas turns, which
    // is what makes rotating feel like turning a sheet of paper under your hand
    // rather than watching it swing away.
    const QPointF centre(width() / 2.0, height() / 2.0);
    QTransform transform;
    transform.translate(centre.x(), centre.y());
    transform.rotate(m_viewRotation);
    transform.translate(-centre.x(), -centre.y());
    return transform;
}

QPointF CanvasView::widgetToDocument(const QPointF &pos) const
{
    // Undo the view rotation first: everything below it works in the upright
    // frame the document is laid out in.
    const QPointF upright = viewTransform().inverted().map(pos);
    const QPointF origin = documentOrigin();
    return QPointF((upright.x() - origin.x()) / m_zoom, (upright.y() - origin.y()) / m_zoom);
}

QPointF CanvasView::documentToWidget(const QPointF &pos) const
{
    const QPointF origin = documentOrigin();
    // Rotation last, so every overlay that places itself through this function
    // — marching ants, the crop box, a type caret — turns with the canvas
    // without knowing the view can turn at all.
    return viewTransform().map(
        QPointF(pos.x() * m_zoom + origin.x(), pos.y() * m_zoom + origin.y()));
}

void CanvasView::zoomToRect(const QRectF &docRect)
{
    if (docRect.width() <= 0.0 || docRect.height() <= 0.0 || width() <= 0 || height() <= 0) {
        return;
    }

    // Whichever axis runs out first decides the zoom, so the whole rectangle
    // fits rather than being cropped to the wider one.
    const double fit = qMin(width() / docRect.width(), height() / docRect.height());
    m_zoom = qBound(kMinZoom, fit, kMaxZoom);

    // Then put what was marked out in the middle of the viewport. Solving
    // `documentToWidget(centre) == viewport centre` for the pan gives this;
    // the view rotation turns about that same centre, so it drops out.
    const QPointF centre = docRect.center();
    m_pan = QPointF((m_image.width() * m_zoom) / 2.0 - centre.x() * m_zoom,
                    (m_image.height() * m_zoom) / 2.0 - centre.y() * m_zoom);

    clampPan();
    emit zoomChanged(m_zoom);
    updateCursor();
    update();
}

void CanvasView::setViewRotation(double degrees)
{
    // Kept in 0..360 so the options bar's field never shows -720°.
    double wrapped = std::fmod(degrees, 360.0);
    if (wrapped < 0.0) {
        wrapped += 360.0;
    }
    if (qFuzzyCompare(m_viewRotation + 1.0, wrapped + 1.0)) {
        return;
    }
    m_viewRotation = wrapped;
    emit viewRotationChanged(m_viewRotation);
    update();
}

void CanvasView::clampPan()
{
    // Allow panning until only a sliver of the document remains visible, so it
    // can never be lost entirely off-screen.
    const double marginX = width() / 2.0 + m_image.width() * m_zoom / 2.0 - 32.0;
    const double marginY = height() / 2.0 + m_image.height() * m_zoom / 2.0 - 32.0;

    m_pan.setX(qBound(-qMax(marginX, 0.0), m_pan.x(), qMax(marginX, 0.0)));
    m_pan.setY(qBound(-qMax(marginY, 0.0), m_pan.y(), qMax(marginY, 0.0)));
    syncScrollBars();
}

void CanvasView::layoutScrollBars()
{
    const int sbw = m_vScroll->sizeHint().width();
    const int sbh = m_hScroll->sizeHint().height();
    m_hScroll->setGeometry(0, height() - sbh, width() - sbw, sbh);
    m_vScroll->setGeometry(width() - sbw, 0, sbw, height() - sbh);
}

void CanvasView::syncScrollBars()
{
    m_scrollBarUpdating = true;

    const double docW = m_image.width() * m_zoom;
    const double docH = m_image.height() * m_zoom;
    const double marginX = width() / 2.0 + docW / 2.0 - 32.0;
    const double marginY = height() / 2.0 + docH / 2.0 - 32.0;
    const double maxX = qMax(marginX, 0.0);
    const double maxY = qMax(marginY, 0.0);

    const bool needH = docW > width();
    const bool needV = docH > height();

    m_hScroll->setVisible(needH);
    m_vScroll->setVisible(needV);

    if (needH) {
        m_hScroll->setRange(int(-maxX), int(maxX));
        m_hScroll->setPageStep(width());
        m_hScroll->setSingleStep(20);
        m_hScroll->setValue(int(-m_pan.x()));
    }
    if (needV) {
        m_vScroll->setRange(int(-maxY), int(maxY));
        m_vScroll->setPageStep(height());
        m_vScroll->setSingleStep(20);
        m_vScroll->setValue(int(-m_pan.y()));
    }

    layoutScrollBars();
    m_scrollBarUpdating = false;
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
    m_pan += uprightDelta(focusWidgetPos - widgetAfter);

    clampPan();
    emit zoomChanged(m_zoom);
    updateCursor();
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
    if (m_backgroundErasing && m_engine) {
        m_engine->cancelBackgroundErase();
        m_backgroundErasing = false;
    }
    m_shapeDragging = false;
    m_shapeOutline.clear();
    m_zoomDragging = false;
    m_zoomRectDoc = QRectF();
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
        m_engine->setEraseMode(tool == ToolId::Eraser
                               && m_eraserType == EraserType::Eraser);
        // Healing runs through the same stroke path as a brush; this is what
        // tells the engine to rebuild the region at the end instead of filling
        // it with the foreground colour.
        m_engine->setHealMode(toolHeals(tool) ? static_cast<int>(m_healType) : -1);
    }
    updateCursor();
    update();
}

void CanvasView::setBrushSize(double size)
{
    m_brushDiameter = qBound(1.0, size, 5000.0);
    updateCursor();
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

void CanvasView::setEraserType(EraserType type)
{
    if (m_eraserType == type) {
        return;
    }
    if (m_backgroundErasing && m_engine) {
        m_engine->cancelBackgroundErase();
        m_backgroundErasing = false;
    }
    m_eraserType = type;
    if (m_engine) {
        // Only the plain Eraser rubs out through the ordinary stroke path; the
        // other two have paths of their own.
        m_engine->setEraseMode(m_tool == ToolId::Eraser && type == EraserType::Eraser);
    }
    updateCursor();
}

void CanvasView::setBackgroundEraseOptions(int sampling, int limits, int tolerance,
                                           bool protectForeground)
{
    m_bgEraseSampling = sampling;
    m_bgEraseLimits = limits;
    m_bgEraseTolerance = tolerance;
    m_bgEraseProtectForeground = protectForeground;
    if (m_engine) {
        m_engine->setBackgroundEraseOptions(sampling, limits, tolerance, protectForeground);
    }
}

void CanvasView::setMagicEraseOptions(int tolerance, bool antialias, bool contiguous,
                                      bool sampleAllLayers, int opacity)
{
    // The Magic Eraser has no stroke to configure, so its settings are held
    // here and passed with the click.
    m_magicEraseTolerance = tolerance;
    m_magicEraseAntialias = antialias;
    m_magicEraseContiguous = contiguous;
    m_magicEraseSampleAll = sampleAllLayers;
    m_magicEraseOpacity = opacity;
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

void CanvasView::setCloneTool(CloneType tool)
{
    if (m_cloneTool == tool) {
        return;
    }
    m_cloneTool = tool;
    updateCursor();
    // The clone source crosshair belongs to the Clone Stamp; the Pattern Stamp
    // has no source to mark.
    update();
}

bool CanvasView::clonePress(const QPointF &doc, Qt::KeyboardModifiers modifiers)
{
    if (!m_engine) {
        return false;
    }

    // The Pattern Stamp is the same stroke over a different source: no
    // Alt-click, nothing to sample, so it starts painting straight away.
    if (m_cloneTool == CloneType::PatternStamp) {
        if (m_engine->beginPatternStroke(float(doc.x()), float(doc.y()), 1.0f)) {
            m_dragging = true;
            m_image = m_engine->previewImage();
            update();
        } else {
            reportIfLocked();
        }
        return true;
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
    if (m_tool != ToolId::CloneStamp || m_cloneTool != CloneType::CloneStamp
        || !m_cloneSourceValid) {
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

bool CanvasView::promptRasterizeIfType()
{
    if (!m_engine) return false;
    const int active = m_engine->getActiveLayerIndex();
    if (m_engine->layerKind(active) != 2) return false;

    auto answer = QMessageBox::warning(
        this, tr("PhotoRust"),
        tr("This type layer must be rasterized before proceeding.  "
           "Its text will no longer be editable.  Rasterize the type?"),
        QMessageBox::Ok | QMessageBox::Cancel);
    if (answer != QMessageBox::Ok) return true;

    m_engine->rasterizeLayer(active);
    refresh();
    return false;
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

namespace {

/// The Rotate View cursor: the tool's own icon, drawn dark-behind-pale so it
/// reads over both the image and the surround. Built once — a cursor is asked
/// for on every pointer move.
const QCursor &rotateViewCursor()
{
    static const QCursor cursor = [] {
        const QPixmap pale =
            ToolIcons::icon(ToolId::Hand, int(HandTool::RotateView), Qt::white).pixmap(24, 24);
        const QPixmap dark =
            ToolIcons::icon(ToolId::Hand, int(HandTool::RotateView), QColor(0, 0, 0, 190))
                .pixmap(24, 24);

        QPixmap art(pale.size());
        art.setDevicePixelRatio(pale.devicePixelRatio());
        art.fill(Qt::transparent);
        QPainter painter(&art);
        painter.drawPixmap(QPointF(1, 1), dark);
        painter.drawPixmap(QPointF(0, 0), pale);
        painter.end();

        return QCursor(art, 12, 12);
    }();
    return cursor;
}

} // namespace

void CanvasView::updateCursor()
{
    const ToolId effective = m_spacePanOverride ? ToolId::Hand : m_tool;
    switch (effective) {
    case ToolId::Hand:
        if (m_handTool == HandTool::RotateView && !m_spacePanOverride) {
            // No stock cursor says "turn this", so the tool's own icon does —
            // the same artwork the flyout shows, which is how the user got here.
            setCursor(rotateViewCursor());
        } else {
            setCursor(m_panning ? Qt::ClosedHandCursor : Qt::OpenHandCursor);
        }
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
    default: {
        const double screenDiam = m_brushDiameter * m_zoom;
        if (screenDiam >= 4.0 && screenDiam <= 300.0) {
            const int sz = qMax(int(std::ceil(screenDiam)) + 2, 8);
            QPixmap pix(sz, sz);
            pix.fill(Qt::transparent);
            QPainter p(&pix);
            p.setRenderHint(QPainter::Antialiasing);
            p.setPen(QPen(Qt::black, 1.0));
            p.setBrush(Qt::NoBrush);
            const double r = screenDiam / 2.0;
            p.drawEllipse(QPointF(sz / 2.0, sz / 2.0), r, r);
            p.end();
            setCursor(QCursor(pix, sz / 2, sz / 2));
        } else {
            setCursor(Qt::CrossCursor);
        }
        break;
    }
    }
}

bool CanvasView::event(QEvent *event)
{
    // Every registered tool shortcut is a QAction on the main window with the
    // default Qt::WindowShortcut context, so it fires for a key press anywhere
    // in the window unless the focused widget claims the ShortcutOverride
    // event first. CanvasView never did, so a letter typed into the Type tool
    // — "e", say — matched the Eraser's shortcut and switched tools instead of
    // reaching keyPressEvent as a character. Accepting the override while
    // text is being composed is what real Photoshop does too: its own
    // single-letter shortcuts go quiet the moment there is a text cursor.
    if (event->type() == QEvent::ShortcutOverride && m_tool == ToolId::Type && m_typing) {
        event->accept();
        return true;
    }

    // Delete and Backspace belong to the canvas whenever it has something of
    // its own to remove — a selected slice, or the last point of an unfinished
    // outline. Without this the window's Edit ▸ Clear takes the key first and
    // erases the layer instead, which is not what pressing Delete on a slice
    // means.
    if (event->type() == QEvent::ShortcutOverride) {
        auto *key = static_cast<QKeyEvent *>(event);
        const bool deleting = key->key() == Qt::Key_Delete || key->key() == Qt::Key_Backspace;
        const bool hasOwnTarget = (toolIsSlice() && m_selectedSlice >= 0)
            || (m_marqueeActive && lassoIsClicked());
        if (deleting && hasOwnTarget) {
            event->accept();
            return true;
        }
    }
    return QWidget::event(event);
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

    // Everything from here to the document's border is laid out upright and
    // then turned as a whole. The overlays below are not: they place themselves
    // through `documentToWidget`, which already carries the rotation.
    painter.save();
    painter.setTransform(viewTransform(), true);

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
    // A turned canvas is resampled whatever the zoom, so nearest-neighbour
    // would leave every edge in the image jagged.
    painter.setRenderHint(QPainter::SmoothPixmapTransform,
                          m_zoom < 2.0 || !qFuzzyIsNull(m_viewRotation));
    painter.drawImage(target, m_image);

    // A thin border so the document edge reads against the surround.
    painter.setPen(QPen(QColor(0x00, 0x00, 0x00, 160), 1));
    painter.setBrush(Qt::NoBrush);
    painter.drawRect(target.adjusted(-0.5, -0.5, 0.5, 0.5));
    painter.restore();

    paintSelection(painter);
    paintCrop(painter);
    if (toolIsAnnotation()) {
        paintAnnotations(painter);
    }
    paintHealing(painter);
    paintCloneSource(painter);
    paintGradientDrag(painter);
    paintPathOverlay(painter);
    paintTypeOverlay(painter);
    paintFreeTransform(painter);
    paintSearchHighlight(painter);
    paintShapeOverlay(painter);
    paintZoomOverlay(painter);

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

    // In Quick Mask the red veil *is* the selection, drawn per pixel by the
    // engine. Marching ants over it would say the same thing twice, and worse
    // than the veil does — a soft-edged mask has no one outline.
    if (m_engine && m_engine->quickMask()) {
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

    // Rotate View turns the canvas by dragging it round, like a hand on a
    // sheet of paper: the angle from the centre to the pointer is followed, so
    // wherever the drag starts stays under the finger as it goes.
    if (m_tool == ToolId::Hand && m_handTool == HandTool::RotateView
        && !m_spacePanOverride && event->button() == Qt::LeftButton) {
        m_rotatingView = true;
        m_rotateStartAngle = angleToPointer(event->position());
        m_rotateStartRotation = m_viewRotation;
        updateCursor();
        return;
    }

    // Middle-drag and space-drag pan from any tool, as in Photoshop. Space
    // still pans while Rotate View is in hand, which is how CS6 gets around a
    // turned canvas without switching tools.
    const bool wantsPan = m_spacePanOverride || event->button() == Qt::MiddleButton
        || (m_tool == ToolId::Hand && m_handTool == HandTool::Hand);
    if (wantsPan) {
        m_panning = true;
        updateCursor();
        return;
    }

    if (event->button() != Qt::LeftButton) {
        return;
    }

    // Free Transform intercepts all left-clicks while active.
    if (m_freeTransform) {
        const QPointF wpos = event->position();

        // Warp mode: hit-test all 16 control points.
        if (m_ftMode == TransformMode::Warp) {
            constexpr double hitDist = 10.0;
            m_warpDragI = m_warpDragJ = -1;
            for (int r = 0; r < 4; ++r) {
                for (int c = 0; c < 4; ++c) {
                    if (QLineF(wpos, documentToWidget(m_warpPts[r][c])).length() <= hitDist) {
                        m_warpDragI = r;
                        m_warpDragJ = c;
                        break;
                    }
                }
                if (m_warpDragI >= 0) break;
            }
            if (m_warpDragI < 0) return;
            m_ftDragStart = doc;
            for (int r = 0; r < 4; ++r)
                for (int c = 0; c < 4; ++c)
                    m_warpPtsDragStart[r][c] = m_warpPts[r][c];
            m_dragging = true;
            return;
        }

        const bool isQuadMode = m_ftMode == TransformMode::Skew
                                || m_ftMode == TransformMode::Distort
                                || m_ftMode == TransformMode::Perspective;

        QPointF corners[4];
        if (isQuadMode) {
            for (int i = 0; i < 4; ++i)
                corners[i] = documentToWidget(m_ftQuad.at(i));
        } else {
            const QPointF center = m_ftBounds.center();
            QTransform xf;
            xf.translate(center.x(), center.y());
            xf.rotate(m_ftRotation);
            xf.translate(-center.x(), -center.y());
            corners[0] = documentToWidget(xf.map(m_ftBounds.topLeft()));
            corners[1] = documentToWidget(xf.map(m_ftBounds.topRight()));
            corners[2] = documentToWidget(xf.map(m_ftBounds.bottomRight()));
            corners[3] = documentToWidget(xf.map(m_ftBounds.bottomLeft()));
        }
        QPointF mids[4] = {
            (corners[0] + corners[1]) / 2.0,
            (corners[1] + corners[2]) / 2.0,
            (corners[2] + corners[3]) / 2.0,
            (corners[3] + corners[0]) / 2.0
        };
        FTHandle handleIds[] = {
            FTHandle::TopLeft, FTHandle::TopRight,
            FTHandle::BottomRight, FTHandle::BottomLeft
        };
        FTHandle midHandleIds[] = {
            FTHandle::Top, FTHandle::Right, FTHandle::Bottom, FTHandle::Left
        };

        constexpr double hitDist = 8.0;
        m_ftHandle = FTHandle::None;

        for (int i = 0; i < 4; ++i) {
            if (QLineF(wpos, corners[i]).length() <= hitDist) {
                m_ftHandle = handleIds[i];
                break;
            }
        }
        if (m_ftHandle == FTHandle::None) {
            for (int i = 0; i < 4; ++i) {
                if (QLineF(wpos, mids[i]).length() <= hitDist) {
                    m_ftHandle = midHandleIds[i];
                    break;
                }
            }
        }

        if (m_ftHandle == FTHandle::None) {
            QPolygonF poly;
            for (auto &c : corners) poly << c;
            if (poly.containsPoint(wpos, Qt::WindingFill)) {
                m_ftHandle = FTHandle::Move;
            } else if (m_ftMode == TransformMode::Free
                       || m_ftMode == TransformMode::Rotate) {
                m_ftHandle = FTHandle::Rotate;
            }
        }

        // Mode constraints: Scale allows only handles+move, Rotate allows
        // only rotate+move.
        if (m_ftMode == TransformMode::Scale
            && (m_ftHandle == FTHandle::Rotate)) {
            m_ftHandle = FTHandle::None;
        }
        if (m_ftMode == TransformMode::Rotate
            && m_ftHandle != FTHandle::Rotate
            && m_ftHandle != FTHandle::Move
            && m_ftHandle != FTHandle::None) {
            m_ftHandle = FTHandle::None;
        }

        if (m_ftHandle == FTHandle::None) {
            return;
        }

        m_ftDragStart = doc;
        m_ftDragStartBounds = m_ftBounds;
        m_ftDragStartRotation = m_ftRotation;
        m_ftDragStartQuad = m_ftQuad;
        m_dragging = true;
        return;
    }

    switch (m_tool) {
    case ToolId::Zoom:
        // The zoom happens on *release*: a drag marks out a rectangle to zoom
        // into, and only a press that never became one is a plain click.
        m_zoomDragging = true;
        m_zoomStartDoc = doc;
        m_zoomRectDoc = QRectF();
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

    case ToolId::Type:
        typePress(doc, event->modifiers());
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

    // The shape tools drag out a rectangle and commit it on release, so the
    // press only records where the drag began.
    if (m_tool == ToolId::Shape) {
        m_shapeDragging = true;
        m_dragStartDoc = doc;
        m_shapeOutline.clear();
        update();
        return;
    }

    // The two colour erasers: one clicks, one drags, and neither goes through
    // the ordinary stroke path.
    if (m_tool == ToolId::Eraser && m_eraserType != EraserType::Eraser && m_engine) {
        if (m_eraserType == EraserType::MagicEraser) {
            if (!m_engine->magicErase(qRound(doc.x()), qRound(doc.y()), m_magicEraseTolerance,
                                      m_magicEraseContiguous, m_magicEraseAntialias,
                                      m_magicEraseSampleAll, m_magicEraseOpacity)) {
                // Nothing came of it: either the layer refuses to be erased, or
                // the click landed where there is already nothing.
                reportIfLocked();
            }
            refresh();
            return;
        }

        if (m_engine->beginBackgroundErase(float(doc.x()), float(doc.y()), 1.0f)) {
            m_backgroundErasing = true;
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
        if (promptRasterizeIfType()) return;
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
        if (promptRasterizeIfType()) return;
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
        // The pan is in the upright frame the document is laid out in, so a
        // drag on a turned canvas has to be turned back the same way — without
        // this the image would slide off at an angle to the hand.
        m_pan += uprightDelta(pos - m_lastMousePos);
        m_lastMousePos = pos;
        clampPan();
        update();
        return;
    }

    if (m_rotatingView) {
        setViewRotation(m_rotateStartRotation + angleToPointer(pos) - m_rotateStartAngle);
        m_lastMousePos = pos;
        return;
    }

    // Warp drag.
    if (m_freeTransform && m_dragging && m_ftMode == TransformMode::Warp
        && m_warpDragI >= 0) {
        const QPointF delta = doc - m_ftDragStart;
        const int r = m_warpDragI, c = m_warpDragJ;
        m_warpPts[r][c] = m_warpPtsDragStart[r][c] + delta;

        // Corner points drag their adjacent tangent handles too.
        const bool isCorner = (r == 0 || r == 3) && (c == 0 || c == 3);
        if (isCorner) {
            // Horizontal neighbor (same row).
            int adjC = (c == 0) ? 1 : 2;
            m_warpPts[r][adjC] = m_warpPtsDragStart[r][adjC] + delta;
            // Vertical neighbor (same column).
            int adjR = (r == 0) ? 1 : 2;
            m_warpPts[adjR][c] = m_warpPtsDragStart[adjR][c] + delta;
        }

        m_lastMousePos = pos;
        update();
        emit transformChanged();
        return;
    }

    // Free Transform drag.
    if (m_freeTransform && m_dragging && m_ftHandle != FTHandle::None) {
        const QPointF delta = doc - m_ftDragStart;
        const bool isQuadMode = m_ftMode == TransformMode::Skew
                                || m_ftMode == TransformMode::Distort
                                || m_ftMode == TransformMode::Perspective;

        if (isQuadMode) {
            auto q = m_ftDragStartQuad;
            const bool persp = m_ftMode == TransformMode::Perspective;
            switch (m_ftHandle) {
            case FTHandle::Move:
                for (int i = 0; i < 4; ++i) q[i] += delta;
                break;
            case FTHandle::TopLeft:
                q[0] += delta;
                if (persp) q[1] += QPointF(-delta.x(), delta.y());
                break;
            case FTHandle::TopRight:
                q[1] += delta;
                if (persp) q[0] += QPointF(-delta.x(), delta.y());
                break;
            case FTHandle::BottomRight:
                q[2] += delta;
                if (persp) q[3] += QPointF(-delta.x(), delta.y());
                break;
            case FTHandle::BottomLeft:
                q[3] += delta;
                if (persp) q[2] += QPointF(-delta.x(), delta.y());
                break;
            case FTHandle::Top:
                if (persp) {
                    q[0] += QPointF(delta.x(), delta.y());
                    q[1] += QPointF(-delta.x(), delta.y());
                } else {
                    q[0] += QPointF(0, delta.y());
                    q[1] += QPointF(0, delta.y());
                }
                break;
            case FTHandle::Bottom:
                if (persp) {
                    q[3] += QPointF(delta.x(), delta.y());
                    q[2] += QPointF(-delta.x(), delta.y());
                } else {
                    q[2] += QPointF(0, delta.y());
                    q[3] += QPointF(0, delta.y());
                }
                break;
            case FTHandle::Left:
                if (persp) {
                    q[0] += QPointF(delta.x(), delta.y());
                    q[3] += QPointF(delta.x(), -delta.y());
                } else {
                    q[0] += QPointF(delta.x(), 0);
                    q[3] += QPointF(delta.x(), 0);
                }
                break;
            case FTHandle::Right:
                if (persp) {
                    q[1] += QPointF(delta.x(), delta.y());
                    q[2] += QPointF(delta.x(), -delta.y());
                } else {
                    q[1] += QPointF(delta.x(), 0);
                    q[2] += QPointF(delta.x(), 0);
                }
                break;
            default:
                break;
            }
            m_ftQuad = q;
        } else {
            // Rectangle-based transforms (Free, Scale, Rotate).
            const bool shift = event->modifiers().testFlag(Qt::ShiftModifier);
            const double origW = m_ftDragStartBounds.width();
            const double origH = m_ftDragStartBounds.height();
            const double aspect = (origH > 0) ? origW / origH : 1.0;

            switch (m_ftHandle) {
            case FTHandle::Move:
                m_ftBounds = m_ftDragStartBounds.translated(delta);
                break;
            case FTHandle::Rotate: {
                QPointF center = m_ftBounds.center();
                double startAngle = std::atan2(m_ftDragStart.y() - center.y(),
                                               m_ftDragStart.x() - center.x());
                double curAngle = std::atan2(doc.y() - center.y(),
                                             doc.x() - center.x());
                double degrees = (curAngle - startAngle) * 180.0 / M_PI;
                if (shift) degrees = std::round(degrees / 15.0) * 15.0;
                m_ftRotation = m_ftDragStartRotation + degrees;
                break;
            }
            case FTHandle::TopLeft: {
                QRectF b = m_ftDragStartBounds;
                if (shift) {
                    double dx = delta.x();
                    double dy = dx / aspect;
                    b.setTopLeft(b.topLeft() + QPointF(dx, dy));
                } else {
                    b.setTopLeft(b.topLeft() + delta);
                }
                m_ftBounds = b;
                break;
            }
            case FTHandle::TopRight: {
                QRectF b = m_ftDragStartBounds;
                if (shift) {
                    double dx = delta.x();
                    double dy = -dx / aspect;
                    b.setTopRight(b.topRight() + QPointF(dx, dy));
                } else {
                    b.setTopRight(b.topRight() + delta);
                }
                m_ftBounds = b;
                break;
            }
            case FTHandle::BottomRight: {
                QRectF b = m_ftDragStartBounds;
                if (shift) {
                    double dx = delta.x();
                    double dy = dx / aspect;
                    b.setBottomRight(b.bottomRight() + QPointF(dx, dy));
                } else {
                    b.setBottomRight(b.bottomRight() + delta);
                }
                m_ftBounds = b;
                break;
            }
            case FTHandle::BottomLeft: {
                QRectF b = m_ftDragStartBounds;
                if (shift) {
                    double dx = delta.x();
                    double dy = -dx / aspect;
                    b.setBottomLeft(b.bottomLeft() + QPointF(dx, dy));
                } else {
                    b.setBottomLeft(b.bottomLeft() + delta);
                }
                m_ftBounds = b;
                break;
            }
            case FTHandle::Top: {
                QRectF b = m_ftDragStartBounds;
                b.setTop(b.top() + delta.y());
                m_ftBounds = b;
                break;
            }
            case FTHandle::Bottom: {
                QRectF b = m_ftDragStartBounds;
                b.setBottom(b.bottom() + delta.y());
                m_ftBounds = b;
                break;
            }
            case FTHandle::Left: {
                QRectF b = m_ftDragStartBounds;
                b.setLeft(b.left() + delta.x());
                m_ftBounds = b;
                break;
            }
            case FTHandle::Right: {
                QRectF b = m_ftDragStartBounds;
                b.setRight(b.right() + delta.x());
                m_ftBounds = b;
                break;
            }
            case FTHandle::None:
                break;
            }
        }
        m_lastMousePos = pos;
        update();
        emit transformChanged();
        return;
    }

    // Warp hover: show pointer near control points.
    if (m_freeTransform && !m_dragging && m_ftMode == TransformMode::Warp) {
        constexpr double hitDist = 10.0;
        bool nearHandle = false;
        for (int r = 0; r < 4 && !nearHandle; ++r)
            for (int c = 0; c < 4 && !nearHandle; ++c)
                if (QLineF(pos, documentToWidget(m_warpPts[r][c])).length() <= hitDist)
                    nearHandle = true;
        setCursor(nearHandle ? Qt::CrossCursor : Qt::ArrowCursor);
        m_lastMousePos = pos;
        return;
    }

    // Free Transform hover: update cursor based on proximity to handles.
    if (m_freeTransform && !m_dragging) {
        const bool isQuadMode = m_ftMode == TransformMode::Skew
                                || m_ftMode == TransformMode::Distort
                                || m_ftMode == TransformMode::Perspective;
        QPointF corners[4];
        if (isQuadMode) {
            for (int i = 0; i < 4; ++i)
                corners[i] = documentToWidget(m_ftQuad.at(i));
        } else {
            const QPointF center = m_ftBounds.center();
            QTransform xf;
            xf.translate(center.x(), center.y());
            xf.rotate(m_ftRotation);
            xf.translate(-center.x(), -center.y());
            corners[0] = documentToWidget(xf.map(m_ftBounds.topLeft()));
            corners[1] = documentToWidget(xf.map(m_ftBounds.topRight()));
            corners[2] = documentToWidget(xf.map(m_ftBounds.bottomRight()));
            corners[3] = documentToWidget(xf.map(m_ftBounds.bottomLeft()));
        }
        QPointF mids[4] = {
            (corners[0] + corners[1]) / 2.0,
            (corners[1] + corners[2]) / 2.0,
            (corners[2] + corners[3]) / 2.0,
            (corners[3] + corners[0]) / 2.0
        };

        constexpr double handleHit = 6.0;

        static QCursor rotateCursor = [] {
            QPixmap pm(24, 24);
            pm.fill(Qt::transparent);
            QPainter p(&pm);
            p.setRenderHint(QPainter::Antialiasing);
            p.setPen(QPen(Qt::black, 1.5));
            QRectF arc(4, 4, 16, 16);
            p.drawArc(arc, 30 * 16, 270 * 16);
            p.setBrush(Qt::black);
            QPolygonF arrow;
            arrow << QPointF(18, 8) << QPointF(21, 11) << QPointF(15, 11);
            p.drawPolygon(arrow);
            p.end();
            return QCursor(pm, 12, 12);
        }();

        bool handled = false;

        Qt::CursorShape cornerCursors[] = {
            Qt::SizeFDiagCursor, Qt::SizeBDiagCursor,
            Qt::SizeFDiagCursor, Qt::SizeBDiagCursor
        };
        for (int i = 0; i < 4; ++i) {
            if (QLineF(pos, corners[i]).length() <= handleHit) {
                if (m_ftMode != TransformMode::Rotate) {
                    setCursor(cornerCursors[i]);
                    handled = true;
                }
                break;
            }
        }

        if (!handled) {
            Qt::CursorShape midCursors[] = {
                Qt::SizeVerCursor, Qt::SizeHorCursor,
                Qt::SizeVerCursor, Qt::SizeHorCursor
            };
            for (int i = 0; i < 4; ++i) {
                if (QLineF(pos, mids[i]).length() <= handleHit) {
                    if (m_ftMode != TransformMode::Rotate) {
                        setCursor(midCursors[i]);
                        handled = true;
                    }
                    break;
                }
            }
        }

        if (!handled) {
            QPolygonF poly;
            for (auto &c : corners) poly << c;
            if (poly.containsPoint(pos, Qt::WindingFill)) {
                setCursor(Qt::SizeAllCursor);
            } else if (m_ftMode == TransformMode::Free
                       || m_ftMode == TransformMode::Rotate) {
                setCursor(rotateCursor);
            } else {
                setCursor(Qt::ArrowCursor);
            }
        }

        m_lastMousePos = pos;
        return;
    }

    // Dragging within text being composed sweeps out a selection.
    if (m_typeSelecting) {
        typeMoveCaret(typeIndexAt(pos), true);
        m_lastMousePos = pos;
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

    if (m_backgroundErasing && m_engine) {
        m_engine->extendBackgroundErase(float(doc.x()), float(doc.y()), 1.0f);
        m_lastMousePos = pos;
        return;
    }

    if (m_shapeDragging) {
        m_shapeOutline = shapeOutlineFor(doc, event->modifiers());
        m_lastMousePos = pos;
        update();
        return;
    }

    if (m_zoomDragging) {
        m_zoomRectDoc = QRectF(m_zoomStartDoc, doc).normalized();
        m_lastMousePos = pos;
        update();
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
    // Double-clicking a word in text being composed selects it, as it does in
    // any text field. The press that opened this double-click has already put
    // the caret there, so the word to take is the one around it.
    if (m_typing && typeBounds().adjusted(-2, -2, 2, 2)
                        .contains(widgetToDocument(event->position()))) {
        m_typeSelecting = false;
        typeSelectWord(typeIndexAt(event->position()));
        event->accept();
        return;
    }

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

    if (m_rotatingView) {
        m_rotatingView = false;
        updateCursor();
        return;
    }

    // Free Transform: end of drag.
    if (m_freeTransform && m_dragging) {
        m_ftHandle = FTHandle::None;
        m_warpDragI = m_warpDragJ = -1;
        m_dragging = false;
        return;
    }

    // The end of a drag-selection through text. The caret and its anchor stay
    // where the drag left them.
    if (m_typeSelecting) {
        m_typeSelecting = false;
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

    if (m_backgroundErasing && m_engine) {
        m_engine->endBackgroundErase();
        m_backgroundErasing = false;
        m_dragging = false;
        refresh();
        return;
    }

    if (m_zoomDragging) {
        m_zoomDragging = false;
        const QRectF marked = m_zoomRectDoc;
        m_zoomRectDoc = QRectF();

        // A rectangle only counts once it is big enough to have been meant:
        // below that the press was a click, and a click zooms a step about
        // where it landed.
        const QRectF widgetRect(documentToWidget(marked.topLeft()),
                                documentToWidget(marked.bottomRight()));
        if (marked.isValid() && qAbs(widgetRect.width()) >= kZoomMarqueeMinimum
            && qAbs(widgetRect.height()) >= kZoomMarqueeMinimum) {
            zoomToRect(marked);
        } else if (event->modifiers().testFlag(Qt::AltModifier)
                   && event->modifiers().testFlag(Qt::ControlModifier)) {
            // Ctrl+Alt inverts the direction. Alt alone is left free: it is the
            // modifier half the other tools sample or subtract with, and a
            // stray Alt should not quietly zoom the wrong way.
            zoomOut();
        } else {
            zoomIn();
        }
        update();
        return;
    }

    if (m_shapeDragging) {
        m_shapeDragging = false;
        m_shapeOutline.clear();
        if (m_engine) {
            const bool drawn = m_engine->drawShape(
                float(m_dragStartDoc.x()), float(m_dragStartDoc.y()), float(doc.x()),
                float(doc.y()), event->modifiers().testFlag(Qt::ShiftModifier),
                event->modifiers().testFlag(Qt::AltModifier), int(m_shapeMode));
            if (!drawn) {
                // Either the drag went nowhere — a click rather than a drag,
                // where CS6 opens a size dialog we do not have — or the layer
                // refuses pixels.
                reportIfLocked();
            }
            refresh();
        } else {
            update();
        }
        return;
    }

    if (m_dragging && m_engine && toolPaints(m_tool)) {
        m_engine->endStroke();
        refresh();
    }
    if (m_dragging && m_engine && m_tool == ToolId::Move) {
        m_engine->sealHistory();
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
    // Free Transform: Enter commits, Escape cancels.
    if (m_freeTransform) {
        if (event->key() == Qt::Key_Return || event->key() == Qt::Key_Enter) {
            commitFreeTransform();
            event->accept();
            return;
        }
        if (event->key() == Qt::Key_Escape) {
            cancelFreeTransform();
            event->accept();
            return;
        }
    }

    // While composing text, every key is the Type tool's — including Space,
    // which the branch below would otherwise steal for a pan override.
    if (m_tool == ToolId::Type && m_typing) {
        typeKeyPress(event);
        return;
    }

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
        if (m_zoomDragging) {
            m_zoomDragging = false;
            m_zoomRectDoc = QRectF();
            update();
        }
        if (m_replacing && m_engine) {
            m_engine->cancelReplace();
            m_replacing = false;
            m_dragging = false;
            refresh();
        }
        if (m_backgroundErasing && m_engine) {
            m_engine->cancelBackgroundErase();
            m_backgroundErasing = false;
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
    // Mid-gesture a right-click is not a request for a menu — it would open on
    // top of the marquee being dragged.
    if (m_marqueeActive || m_dragging || m_panning) {
        event->ignore();
        return;
    }

    // CS6 gives every tool its own right-click menu; the selection tools and
    // the Zoom tool are the ones that have one here so far. The canvas does not
    // build either: the commands on them belong to the registry, which
    // MainWindow owns.
    if (m_tool == ToolId::Zoom) {
        emit zoomContextMenuRequested(event->globalPos());
        event->accept();
        return;
    }
    if (!toolSelects(m_tool)) {
        QWidget::contextMenuEvent(event);
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

// -------------------------------------------------------------------- Type --

namespace {

/// Photoshop's rubylith: the half-transparent red a masked area wears, which
/// mask type spreads over the document while the letters are being cut out
/// of it.
const QColor kQuickMaskVeil(255, 0, 0, 128);

/// The type record's alignment code, 0-2, as the engine stores it.
int typeAlignCode(Qt::Alignment alignment)
{
    if (alignment & Qt::AlignHCenter) {
        return 1;
    }
    if (alignment & Qt::AlignRight) {
        return 2;
    }
    return 0;
}

Qt::Alignment typeAlignFromCode(int code)
{
    switch (code) {
    case 1:
        return Qt::AlignHCenter;
    case 2:
        return Qt::AlignRight;
    default:
        return Qt::AlignLeft;
    }
}

/// Whether a character counts as part of a word, for double-click and for
/// Ctrl+arrow. Underscores go with letters and digits, as they do in every
/// other text field.
bool isWordChar(QChar c)
{
    return c.isLetterOrNumber() || c == QLatin1Char('_');
}

} // namespace

void CanvasView::typePress(const QPointF &doc, Qt::KeyboardModifiers modifiers)
{
    // A click within the text being composed works it the way any text field
    // does: it places the caret, and holding the button sweeps out a selection.
    // Only a click that lands elsewhere ends the edit.
    if (m_typing && typeBounds().adjusted(-2, -2, 2, 2).contains(doc)) {
        typeMoveCaret(typeIndexAt(documentToWidget(doc)), modifiers & Qt::ShiftModifier);
        m_typeSelecting = true;
        setFocus(Qt::MouseFocusReason);
        return;
    }

    // Finish the previous edit first, so the hit test below sees a document
    // that already includes whatever was just typed — clicking straight from
    // one piece of text into another works, and lands on the right one.
    if (m_typing) {
        commitTypeEdit();
    }

    // Mask type has nothing to reopen — it leaves a selection, not a layer —
    // and clicking a type layer with it should start a new mask over that text
    // rather than take the layer over.
    const int existing = m_engine && !m_typeMask
        ? m_engine->textLayerAt(qRound(doc.x()), qRound(doc.y()))
        : -1;
    if (existing >= 0) {
        beginTypeEdit(existing, doc);
        return;
    }

    m_typing = true;
    m_typeOrigin = doc;
    m_typeText.clear();
    m_typeRuns.clear();
    m_typeCaret = 0;
    m_typeAnchor = 0;
    // The layer appears now, empty, the way Photoshop's does — clicking with
    // the Type tool *is* the act that makes a type layer, and waiting until
    // the text is committed leaves the Layers panel disagreeing with the caret
    // blinking on the canvas.
    m_typeLayer = createEmptyTypeLayer();
    m_typeLayerIsNew = m_typeLayer >= 0;
    setFocus(Qt::MouseFocusReason);
    refresh();
}

int CanvasView::createEmptyTypeLayer()
{
    if (!m_engine) {
        return -1;
    }

    // One run holding nothing: the layer is type from the outset, so the panel
    // marks it with a T before a single character is typed.
    m_engine->beginTextRuns();
    m_engine->addTextRun(QString(), m_typeFont.family(), m_typeStyleName,
                         float(m_typeFont.pointSizeF()), m_typeColor);

    // A pixel of nothing, since a layer with no pixels at all has no place in
    // the document. What is committed later replaces it wholesale.
    QImage empty(1, 1, QImage::Format_ARGB32_Premultiplied);
    empty.fill(Qt::transparent);

    // No name: the engine falls back to "Layer N", which is what Photoshop
    // calls a type layer until there is text to name it after.
    if (!m_engine->addTextLayer(empty, qRound(m_typeOrigin.x()), qRound(m_typeOrigin.y()),
                                QString(), typeAlignCode(m_typeAlignment), m_typeAntialias,
                                m_typeVertical, float(m_typeOrigin.x()),
                                float(m_typeOrigin.y()))) {
        return -1;
    }

    const int index = m_engine->getActiveLayerIndex();
    // Hold its pixels back for the edit, exactly as reopening one does.
    m_engine->beginTextEdit(index);
    return index;
}

void CanvasView::beginTypeEdit(int layerIndex, const QPointF &doc)
{
    if (!m_engine) {
        return;
    }

    // Read the text back run by run, which is how its character formatting
    // survives: a word set at 72pt in the middle of 12pt text is its own run.
    m_typeText.clear();
    m_typeRuns.clear();
    m_typeFontCache.clear();
    const int runCount = m_engine->layerTextRunCount(layerIndex);
    for (int i = 0; i < runCount; ++i) {
        const QString text = m_engine->layerTextRunText(layerIndex, i);
        TypeRun run;
        run.length = text.size();
        run.family = m_engine->layerTextRunFamily(layerIndex, i);
        run.style = m_engine->layerTextRunStyle(layerIndex, i);
        run.size = m_engine->layerTextRunSize(layerIndex, i);
        run.color = m_engine->layerTextRunColor(layerIndex, i);
        m_typeRuns.append(run);
        m_typeText += text;
    }

    m_typeOrigin = QPointF(m_engine->layerTextOriginX(layerIndex),
                           m_engine->layerTextOriginY(layerIndex));
    m_typeAlignment = typeAlignFromCode(m_engine->layerTextAlign(layerIndex));
    m_typeAntialias = m_engine->layerTextAntialias(layerIndex);
    // Orientation belongs to the text, not to the tool in hand: clicking into
    // vertical type edits it vertically whichever of the two tools opened it.
    m_typeVertical = m_engine->layerTextVertical(layerIndex);

    // The reopened text is edited in its own type, not in whatever the options
    // bar happened to be left set to, so the bar takes on the formatting the
    // caret lands in — `typeSyncStyleToCaret` below, once the caret is placed.
    const TypeRun first = m_typeRuns.isEmpty() ? typePendingRun() : m_typeRuns.first();
    m_typeStyleName = first.style;
    m_typeFont = typeRunFont(first, 1.0);
    m_typeColor = first.color;

    // Hold the layer's own pixels back for the duration, or the live overlay
    // would be drawn over the rendering it is replacing — deleting a word would
    // leave it still showing underneath.
    m_engine->beginTextEdit(layerIndex);

    m_typing = true;
    m_typeLayer = layerIndex;
    // Reopened, not made here: abandoning this edit must leave the layer where
    // it was rather than deleting someone's text.
    m_typeLayerIsNew = false;
    // The caret goes where the click fell, now that there is a layout to
    // measure it against — clicking mid-word puts it mid-word.
    m_typeCaret = typeIndexAt(documentToWidget(doc));
    m_typeAnchor = m_typeCaret;
    setFocus(Qt::MouseFocusReason);
    // Tell the options bar what it is now editing. The caret's own run may
    // differ from the first, so this goes through the same path a caret move
    // does rather than announcing the first run.
    emit typeStyleAdopted(first.family, first.style, first.size, first.color, m_typeAlignment,
                          m_typeAntialias, m_typeVertical);
    typeSyncStyleToCaret();
    update();
}

QFont CanvasView::typeRunFont(const TypeRun &run, qreal scale) const
{
    const qreal size = qMax(1.0, run.size * scale);
    const QString key = QStringLiteral("%1|%2|%3").arg(run.family, run.style)
                            .arg(size, 0, 'f', 3);
    const auto hit = m_typeFontCache.constFind(key);
    if (hit != m_typeFontCache.constEnd()) {
        return *hit;
    }

    // Resolved by style *name* rather than by setting bold/italic bits, so a
    // family's own styles ("Semibold", "Condensed Light") come out right.
    QFont font = QFontDatabase::font(run.family, run.style, int(size));
    font.setPointSizeF(size);
    m_typeFontCache.insert(key, font);
    return font;
}

CanvasView::TypeLayout CanvasView::typeLayout(qreal scale) const
{
    TypeLayout layout;
    layout.scale = scale;
    layout.vertical = m_typeVertical;

    // Walk the text once, cutting it at newlines and again wherever the
    // formatting changes. Horizontal type draws a whole same-formatted stretch
    // in one go; vertical type stacks characters, so each gets its own segment.
    int runIndex = 0;
    int runStart = 0;
    int at = 0;

    while (true) {
        const int newline = m_typeText.indexOf(QLatin1Char('\n'), at);
        const int lineEnd = newline < 0 ? m_typeText.size() : newline;

        TypeLineBox line;
        line.start = at;
        line.length = lineEnd - at;

        int cursor = at;
        while (true) {
            // Which run covers `cursor`, and how far it reaches.
            while (runIndex < m_typeRuns.size()
                   && cursor >= runStart + m_typeRuns.at(runIndex).length) {
                runStart += m_typeRuns.at(runIndex).length;
                ++runIndex;
            }
            const TypeRun run = runIndex < m_typeRuns.size() ? m_typeRuns.at(runIndex)
                                                            : typePendingRun();
            const QFont font = typeRunFont(run, scale);
            const QFontMetricsF metrics(font);

            const int runEnd = runIndex < m_typeRuns.size()
                ? runStart + m_typeRuns.at(runIndex).length
                : lineEnd;
            const int segmentEnd = qMin(lineEnd, qMax(runEnd, cursor));

            TypeSegment segment;
            segment.start = cursor;
            segment.length = segmentEnd - cursor;
            segment.font = font;
            segment.color = run.color;
            segment.ascent = metrics.ascent();
            segment.height = metrics.lineSpacing();

            if (!m_typeVertical) {
                segment.x = line.width;
                segment.width = segment.length > 0
                    ? metrics.horizontalAdvance(m_typeText.mid(cursor, segment.length))
                    : 0.0;
                line.width += segment.width;
                // Every segment of a line shares one baseline, set by the
                // tallest run in it, so a large word does not sit low.
                line.height = qMax(line.height, segment.height);
                line.segments.append(segment);
            } else if (segment.length == 0) {
                // An empty line still has to know how tall its caret is.
                segment.y = line.height;
                segment.width = metrics.horizontalAdvance(QLatin1Char('W'));
                line.height += segment.height;
                line.width = qMax(line.width, segment.width);
                line.segments.append(segment);
            } else {
                for (int i = cursor; i < segmentEnd; ++i) {
                    TypeSegment glyph = segment;
                    glyph.start = i;
                    glyph.length = 1;
                    glyph.width = metrics.horizontalAdvance(m_typeText.at(i));
                    glyph.y = line.height;
                    line.height += glyph.height;
                    line.width = qMax(line.width, glyph.width);
                    line.segments.append(glyph);
                }
            }

            cursor = segmentEnd;
            if (cursor >= lineEnd) {
                break;
            }
        }

        // A line with no segments would have nowhere to put a caret; only an
        // empty horizontal line reaches this, since the vertical branch above
        // always appends one.
        if (line.segments.isEmpty()) {
            TypeSegment empty;
            empty.start = line.start;
            const QFontMetricsF metrics(typeRunFont(typePendingRun(), scale));
            empty.font = typeRunFont(typePendingRun(), scale);
            empty.ascent = metrics.ascent();
            empty.height = metrics.lineSpacing();
            line.height = qMax(line.height, empty.height);
            line.segments.append(empty);
        }

        layout.lines.append(line);

        if (newline < 0) {
            break;
        }
        at = newline + 1;
        // Step the run walk over the newline itself, which belongs to whichever
        // run holds it.
        while (runIndex < m_typeRuns.size()
               && at >= runStart + m_typeRuns.at(runIndex).length) {
            runStart += m_typeRuns.at(runIndex).length;
            ++runIndex;
        }
    }

    // Place the lines against the origin. Horizontal type stacks them
    // downward and the alignment slides each one sideways; vertical type lays
    // them out as columns leading away to the left, with the alignment sliding
    // each column up or down instead.
    qreal cross = 0.0;
    for (TypeLineBox &line : layout.lines) {
        if (!m_typeVertical) {
            line.top = cross;
            cross += line.height;
            line.x = typeAlignOffset(line.width);
        } else {
            cross += line.width;
            line.x = -cross;
            line.top = typeAlignOffset(line.height);
            // Stacked characters are centred in their column, so a narrow
            // letter still reads as part of the same line as a wide one.
            for (TypeSegment &segment : line.segments) {
                segment.x = (line.width - segment.width) / 2.0;
            }
        }
        const QRectF box(line.x, line.top, qMax(line.width, 1.0), qMax(line.height, 1.0));
        layout.box = layout.box.isNull() ? box : layout.box.united(box);
    }

    return layout;
}

qreal CanvasView::typeAlignOffset(qreal extent) const
{
    // The same three settings, read against whichever axis the text runs
    // across: left/centre/right for horizontal type, top/centre/bottom for
    // vertical.
    if (m_typeAlignment & Qt::AlignHCenter) {
        return -extent / 2.0;
    }
    if (m_typeAlignment & Qt::AlignRight) {
        return -extent;
    }
    return 0.0;
}

int CanvasView::typeLineOf(const TypeLayout &layout, int index) const
{
    for (int i = layout.lines.size() - 1; i >= 0; --i) {
        if (index >= layout.lines.at(i).start) {
            return i;
        }
    }
    return 0;
}

qreal CanvasView::typeFlowOffset(const TypeLayout &layout, int lineIndex, int index) const
{
    const TypeLineBox &line = layout.lines.at(lineIndex);
    const int at = qBound(line.start, index, line.start + line.length);

    for (const TypeSegment &segment : line.segments) {
        if (at > segment.start + segment.length) {
            continue;
        }
        if (layout.vertical) {
            // Stacked characters are one segment each, so the gap is either
            // above this character or below it.
            return at > segment.start ? segment.y + segment.height : segment.y;
        }
        return segment.x
            + QFontMetricsF(segment.font)
                  .horizontalAdvance(m_typeText.mid(segment.start, at - segment.start));
    }
    return layout.vertical ? line.height : line.width;
}

QRectF CanvasView::typeCaretRect(const TypeLayout &layout, int index) const
{
    const int lineIndex = typeLineOf(layout, index);
    const TypeLineBox &line = layout.lines.at(lineIndex);
    const qreal flow = typeFlowOffset(layout, lineIndex, index);

    // The caret lies across the direction the text runs: upright between
    // letters, flat between stacked characters.
    if (layout.vertical) {
        return QRectF(line.x, line.top + flow, qMax(line.width, 1.0), 1.0);
    }
    return QRectF(line.x + flow, line.top, 1.0, qMax(line.height, 1.0));
}

QRectF CanvasView::typeRangeRect(const TypeLayout &layout, int lineIndex, int from, int to) const
{
    const TypeLineBox &line = layout.lines.at(lineIndex);
    const qreal a = typeFlowOffset(layout, lineIndex, from);
    const qreal b = typeFlowOffset(layout, lineIndex, to);
    if (layout.vertical) {
        return QRectF(line.x, line.top + a, line.width, b - a);
    }
    return QRectF(line.x + a, line.top, b - a, line.height);
}

int CanvasView::typeIndexAt(const QPointF &widgetPos) const
{
    const TypeLayout layout = typeLayout(m_zoom);
    if (layout.lines.isEmpty()) {
        return 0;
    }

    // Both are offsets from the origin, which is what the layout is measured
    // from and where it is drawn.
    const QPointF offset = widgetPos - documentToWidget(m_typeOrigin);

    // Which line the point is on. Vertical columns run right to left, so the
    // match is the last one whose right edge the point has passed.
    int lineIndex = 0;
    for (int i = 0; i < layout.lines.size(); ++i) {
        const TypeLineBox &line = layout.lines.at(i);
        const bool hit = layout.vertical ? offset.x() <= line.x + line.width
                                         : offset.y() >= line.top;
        if (hit) {
            lineIndex = i;
        }
    }
    const TypeLineBox &line = layout.lines.at(lineIndex);

    // Then whichever gap between characters the click fell nearest to, so
    // clicking the near half of a letter puts the caret before it and the far
    // half after it.
    const qreal target = layout.vertical ? offset.y() - line.top : offset.x() - line.x;
    int nearest = line.start;
    qreal shortest = qAbs(target - typeFlowOffset(layout, lineIndex, line.start));
    for (int i = line.start + 1; i <= line.start + line.length; ++i) {
        const qreal distance = qAbs(target - typeFlowOffset(layout, lineIndex, i));
        if (distance < shortest) {
            shortest = distance;
            nearest = i;
        }
    }
    return nearest;
}

int CanvasView::typeRunIndexAt(int index) const
{
    int start = 0;
    for (int i = 0; i < m_typeRuns.size(); ++i) {
        start += m_typeRuns.at(i).length;
        if (index < start) {
            return i;
        }
    }
    return m_typeRuns.size() - 1;
}

CanvasView::TypeRun CanvasView::typeRunAt(int index) const
{
    const int run = typeRunIndexAt(index);
    return run >= 0 ? m_typeRuns.at(run) : typePendingRun();
}

CanvasView::TypeRun CanvasView::typePendingRun() const
{
    TypeRun run;
    run.family = m_typeFont.family();
    run.style = m_typeStyleName;
    run.size = m_typeFont.pointSizeF();
    run.color = m_typeColor;
    return run;
}

void CanvasView::typeApplyStyle(int from, int to)
{
    if (from >= to || m_typeRuns.isEmpty()) {
        return;
    }

    const TypeRun style = typePendingRun();
    QList<TypeRun> rebuilt;
    int at = 0;
    for (const TypeRun &run : std::as_const(m_typeRuns)) {
        const int runStart = at;
        const int runEnd = at + run.length;
        at = runEnd;

        // Three pieces at most: what falls before the range keeps its old
        // style, what falls inside takes the new one, what falls after keeps
        // the old. Runs clear of the range come through untouched.
        const int inFrom = qMax(runStart, from);
        const int inTo = qMin(runEnd, to);
        if (inFrom >= inTo) {
            rebuilt.append(run);
            continue;
        }
        if (inFrom > runStart) {
            TypeRun before = run;
            before.length = inFrom - runStart;
            rebuilt.append(before);
        }
        TypeRun inside = style;
        inside.length = inTo - inFrom;
        rebuilt.append(inside);
        if (inTo < runEnd) {
            TypeRun after = run;
            after.length = runEnd - inTo;
            rebuilt.append(after);
        }
    }

    m_typeRuns = rebuilt;
    typeNormalizeRuns();
}

void CanvasView::typeNormalizeRuns()
{
    QList<TypeRun> merged;
    for (const TypeRun &run : std::as_const(m_typeRuns)) {
        if (run.length <= 0) {
            continue;
        }
        if (!merged.isEmpty() && merged.last().sameStyle(run)) {
            merged.last().length += run.length;
        } else {
            merged.append(run);
        }
    }

    // The runs describe the text, so their lengths have to add up to it. Any
    // shortfall goes to the last run and any excess comes off it, which is
    // where an edit that ran past the end of the runs would have landed.
    int total = 0;
    for (const TypeRun &run : std::as_const(merged)) {
        total += run.length;
    }
    if (merged.isEmpty()) {
        TypeRun whole = typePendingRun();
        whole.length = m_typeText.size();
        if (whole.length > 0) {
            merged.append(whole);
        }
    } else if (total != m_typeText.size()) {
        merged.last().length += m_typeText.size() - total;
        if (merged.last().length <= 0) {
            merged.removeLast();
        }
    }

    m_typeRuns = merged;
}

void CanvasView::typeSyncStyleToCaret()
{
    if (m_typeRuns.isEmpty()) {
        return;
    }

    // The style at the caret is the one the character *before* it is set in —
    // typing continues what you just typed rather than what is ahead.
    const int at = typeHasSelection() ? typeSelectionStart()
                                      : qMax(0, m_typeCaret - 1);
    // A selection spanning more than one style has no single style to show, so
    // the options bar is left alone rather than made to pick one — picking one
    // would then be pushed back over the selection and flatten it.
    if (typeHasSelection()
        && typeRunIndexAt(typeSelectionStart()) != typeRunIndexAt(typeSelectionEnd() - 1)) {
        return;
    }

    const TypeRun run = typeRunAt(at);
    if (run.sameStyle(typePendingRun())) {
        return;
    }

    m_typeFont.setFamily(run.family);
    m_typeFont.setPointSizeF(run.size);
    m_typeStyleName = run.style;
    m_typeColor = run.color;
    emit typeStyleAdopted(run.family, run.style, run.size, run.color, m_typeAlignment,
                          m_typeAntialias, m_typeVertical);
}

void CanvasView::typeMoveCaret(int index, bool extend)
{
    m_typeCaret = qBound(0, index, m_typeText.size());
    if (!extend) {
        m_typeAnchor = m_typeCaret;
    }
    typeSyncStyleToCaret();
    update();
}

bool CanvasView::typeDeleteSelection()
{
    if (!typeHasSelection()) {
        return false;
    }
    const int from = typeSelectionStart();
    typeRemove(from, typeSelectionEnd() - from);
    m_typeCaret = from;
    m_typeAnchor = from;
    return true;
}

void CanvasView::typeRemove(int at, int length)
{
    // Take the deleted characters off whichever runs held them, in order.
    int remaining = length;
    int runStart = 0;
    for (int i = 0; i < m_typeRuns.size() && remaining > 0; ++i) {
        TypeRun &run = m_typeRuns[i];
        const int runEnd = runStart + run.length;
        const int from = qMax(runStart, at);
        const int to = qMin(runEnd, at + length);
        if (from < to) {
            run.length -= to - from;
            remaining -= to - from;
        }
        runStart = runEnd;
    }

    m_typeText.remove(at, length);
    typeNormalizeRuns();
}

void CanvasView::typeInsert(const QString &text)
{
    typeDeleteSelection();

    // Typed characters take the options bar's current style, which is what
    // makes setting a size with nothing selected apply to what you type next.
    const TypeRun style = typePendingRun();
    QList<TypeRun> rebuilt;
    TypeRun inserted = style;
    inserted.length = text.size();

    int at = 0;
    bool placed = false;
    for (const TypeRun &run : std::as_const(m_typeRuns)) {
        const int runStart = at;
        at += run.length;
        if (placed || m_typeCaret > at) {
            rebuilt.append(run);
            continue;
        }
        // Split the run the caret sits inside; a caret on a boundary lands
        // between two runs and splits neither.
        const int before = m_typeCaret - runStart;
        if (before > 0) {
            TypeRun head = run;
            head.length = before;
            rebuilt.append(head);
        }
        rebuilt.append(inserted);
        if (run.length - before > 0) {
            TypeRun tail = run;
            tail.length = run.length - before;
            rebuilt.append(tail);
        }
        placed = true;
    }
    if (!placed) {
        rebuilt.append(inserted);
    }
    m_typeRuns = rebuilt;

    m_typeText.insert(m_typeCaret, text);
    m_typeCaret += text.size();
    m_typeAnchor = m_typeCaret;
    typeNormalizeRuns();
}

void CanvasView::typeKeyPress(QKeyEvent *event)
{
    const bool extend = event->modifiers() & Qt::ShiftModifier;
    const bool byWord = event->modifiers() & Qt::ControlModifier;
    const TypeLayout layout = typeLayout(m_zoom);

    // Step over a word rather than a character, for Ctrl+arrow: past any run of
    // separators, then past the word itself, the way every text field moves.
    auto wordStep = [this](int from, int direction) {
        int at = from;
        while (at + qMin(direction, 0) >= 0 && at + qMax(direction, 0) < m_typeText.size()
               && !isWordChar(m_typeText.at(at + qMin(direction, 0)))) {
            at += direction;
        }
        while (at + qMin(direction, 0) >= 0 && at + qMax(direction, 0) < m_typeText.size()
               && isWordChar(m_typeText.at(at + qMin(direction, 0)))) {
            at += direction;
        }
        return at;
    };

    // Up and down keep the caret's distance along the line, so running down a
    // block of text does not drag it to the start.
    auto verticalStep = [this, &layout](int direction) -> int {
        const int line = typeLineOf(layout, m_typeCaret);
        const int target = line + direction;
        if (target < 0 || target >= layout.lines.size()) {
            return direction < 0 ? 0 : m_typeText.size();
        }
        const int column = m_typeCaret - layout.lines.at(line).start;
        return int(layout.lines.at(target).start
                   + qMin(column, layout.lines.at(target).length));
    };

    switch (event->key()) {
    case Qt::Key_Escape:
        cancelTypeEdit();
        event->accept();
        return;

    case Qt::Key_Return:
    case Qt::Key_Enter:
        // Plain Enter inserts a newline, as it does in real Photoshop;
        // Ctrl+Enter (its numpad Enter, which Qt does not distinguish from a
        // modifier-free press here) commits instead.
        if (event->modifiers() & Qt::ControlModifier) {
            commitTypeEdit();
        } else {
            typeInsert(QStringLiteral("\n"));
            update();
        }
        event->accept();
        return;

    case Qt::Key_Backspace:
        // With a selection, Backspace deletes it rather than a character.
        if (!typeDeleteSelection() && m_typeCaret > 0) {
            m_typeText.remove(m_typeCaret - 1, 1);
            --m_typeCaret;
            m_typeAnchor = m_typeCaret;
        }
        update();
        event->accept();
        return;

    case Qt::Key_Delete:
        if (!typeDeleteSelection() && m_typeCaret < m_typeText.size()) {
            m_typeText.remove(m_typeCaret, 1);
        }
        update();
        event->accept();
        return;

    case Qt::Key_Left:
    case Qt::Key_Right:
    case Qt::Key_Up:
    case Qt::Key_Down: {
        // The arrows follow the text rather than the screen: along the line for
        // horizontal type, down the column for vertical, with the crosswise
        // pair stepping between lines. Vertical columns run right to left, so
        // Left is the *next* one.
        const bool alongTheText = m_typeVertical
            ? (event->key() == Qt::Key_Up || event->key() == Qt::Key_Down)
            : (event->key() == Qt::Key_Left || event->key() == Qt::Key_Right);
        int direction = (event->key() == Qt::Key_Right || event->key() == Qt::Key_Down) ? 1 : -1;
        if (m_typeVertical && !alongTheText) {
            direction = -direction;
        }

        if (!alongTheText) {
            typeMoveCaret(verticalStep(direction), extend);
        } else if (!extend && typeHasSelection()) {
            // Without Shift, an arrow key collapses a selection to its near
            // edge instead of stepping on from the caret.
            typeMoveCaret(direction < 0 ? typeSelectionStart() : typeSelectionEnd(), false);
        } else {
            typeMoveCaret(byWord ? wordStep(m_typeCaret, direction) : m_typeCaret + direction,
                          extend);
        }
        event->accept();
        return;
    }

    case Qt::Key_Home:
        typeMoveCaret(byWord ? 0 : layout.lines.at(typeLineOf(layout, m_typeCaret)).start, extend);
        event->accept();
        return;

    case Qt::Key_End: {
        const TypeLineBox &line = layout.lines.at(typeLineOf(layout, m_typeCaret));
        typeMoveCaret(byWord ? m_typeText.size() : line.start + line.length, extend);
        event->accept();
        return;
    }

    default:
        break;
    }

    // Ctrl+A selects all the text being composed, as it does while editing type
    // in Photoshop — it is not the canvas-wide Select All while the Type tool
    // has an edit open.
    if (event->key() == Qt::Key_A && (event->modifiers() & Qt::ControlModifier)) {
        m_typeAnchor = 0;
        m_typeCaret = m_typeText.size();
        typeSyncStyleToCaret();
        update();
        event->accept();
        return;
    }

    const QString text = event->text();
    if (!text.isEmpty() && text.at(0).isPrint() && !(event->modifiers() & Qt::ControlModifier)) {
        typeInsert(text);
        update();
    }
    event->accept();
}

void CanvasView::typeSelectWord(int index)
{
    const int at = qBound(0, index, m_typeText.size());
    // A double-click between two words — on a space — selects the run of
    // spaces, so the gesture always grabs something.
    const bool word = at < m_typeText.size() && isWordChar(m_typeText.at(at));
    auto matches = [word](QChar c) { return isWordChar(c) == word && c != QLatin1Char('\n'); };

    int from = at;
    while (from > 0 && matches(m_typeText.at(from - 1))) {
        --from;
    }
    int to = at;
    while (to < m_typeText.size() && matches(m_typeText.at(to))) {
        ++to;
    }

    m_typeAnchor = from;
    m_typeCaret = to;
    typeSyncStyleToCaret();
    update();
}

void CanvasView::setTypeVertical(bool vertical)
{
    if (m_typeVertical == vertical) {
        return;
    }
    // Horizontal and Vertical Type are two tools, so switching between them
    // finishes the text in progress rather than turning it on its side.
    commitTypeEdit();
    m_typeVertical = vertical;
}

void CanvasView::setTypeMask(bool mask)
{
    if (m_typeMask == mask) {
        return;
    }
    // Type and Type Mask are separate tools, and what an edit commits to — a
    // layer or a selection — is decided when it starts, so the one in progress
    // finishes under the tool it was begun with.
    commitTypeEdit();
    m_typeMask = mask;
}

void CanvasView::setTypeOptions(const QFont &font, const QString &styleName, const QColor &color,
                                Qt::Alignment alignment, bool antialias)
{
    m_typeFont = font;
    m_typeStyleName = styleName;
    m_typeColor = color;
    m_typeAlignment = alignment;
    m_typeAntialias = antialias;

    if (!m_typing) {
        return;
    }
    // Character formatting applies to the selection, the way it does in
    // Photoshop: selecting two letters and setting 72pt changes those two and
    // nothing else. With no selection there is nothing to restyle — the new
    // setting is what the next characters typed will be set in.
    if (typeHasSelection()) {
        typeApplyStyle(typeSelectionStart(), typeSelectionEnd());
    }
    update();
}

QRectF CanvasView::typeBounds() const
{
    // Document space, so the layout is measured at scale 1 — the size the text
    // will be rasterized at, whatever the view is zoomed to. The layout's box
    // already carries the alignment and, for vertical type, the columns
    // trailing away to the left of the origin.
    const QRectF box = typeLayout(1.0).box;
    return QRectF(m_typeOrigin + box.topLeft(),
                  QSizeF(qMax(box.width(), 1.0), qMax(box.height(), 1.0)));
}

void CanvasView::commitTypeEdit()
{
    if (!m_typing) {
        return;
    }
    m_typing = false;
    m_typeSelecting = false;
    m_typeCaret = 0;
    m_typeAnchor = 0;
    const int editedLayer = m_typeLayer;
    m_typeLayer = -1;
    m_typeLayerIsNew = false;

    if (!m_engine) {
        m_typeText.clear();
        m_typeRuns.clear();
        update();
        return;
    }

    if (m_typeText.trimmed().isEmpty()) {
        // Text emptied out and committed is text deleted: Photoshop drops the
        // type layer rather than leaving an empty one behind. A new edit that
        // never got any text simply goes away, and mask type that was never
        // typed into leaves the selection alone.
        m_engine->endTextEdit();
        if (editedLayer >= 0) {
            m_engine->deleteLayer(editedLayer);
        }
        m_typeText.clear();
        m_typeRuns.clear();
        update();
        return;
    }

    // A pixel of padding on every side keeps antialiased strokes from being
    // clipped at the rasterized image's edge.
    const int pad = 2;
    const QRectF bounds = typeBounds();
    const QRect pixelBounds = bounds.toAlignedRect().adjusted(-pad, -pad, pad, pad);

    QImage image(qMax(1, pixelBounds.width()), qMax(1, pixelBounds.height()),
                QImage::Format_ARGB32_Premultiplied);
    image.fill(Qt::transparent);

    if (m_typeMask) {
        // Mask type commits the letterforms as a selection and nothing else:
        // no layer, no type record, so no reopening it later — exactly like
        // Photoshop, where a committed type mask is only a selection. Drawn in
        // flat opaque black because only the alpha is read.
        renderTypeToImage(image, pixelBounds.topLeft(), Qt::black);
        m_engine->selectFromAlpha(image, pixelBounds.left(), pixelBounds.top(),
                                  int(SelectionMode::New));
        m_typeText.clear();
        m_typeRuns.clear();
        update();
        return;
    }

    renderTypeToImage(image, pixelBounds.topLeft());

    // CS6 names a text layer after what it says, one line's worth — and
    // renames it as the text changes, so this is recomputed on every commit.
    QString name = m_typeText.section(QLatin1Char('\n'), 0, 0).trimmed();
    if (name.isEmpty()) {
        name = tr("Type Layer");
    }

    // The text crosses the bridge run by run — see `beginTextRuns` — so the
    // engine keeps the character formatting rather than one font for the lot.
    m_engine->beginTextRuns();
    int at = 0;
    for (const TypeRun &run : std::as_const(m_typeRuns)) {
        m_engine->addTextRun(m_typeText.mid(at, run.length), run.family, run.style,
                             float(run.size), run.color);
        at += run.length;
    }

    // Re-rendering an existing layer keeps everything else about it — its place
    // in the stack, its blend mode, its mask. Only if that layer has gone (undo
    // during the edit, say) does this fall back to adding a new one.
    bool updated = false;
    if (editedLayer >= 0) {
        updated = m_engine->updateTextLayer(editedLayer, image, pixelBounds.left(),
                                            pixelBounds.top(), name,
                                            typeAlignCode(m_typeAlignment), m_typeAntialias,
                                            m_typeVertical, float(m_typeOrigin.x()),
                                            float(m_typeOrigin.y()));
    }
    if (!updated) {
        m_engine->endTextEdit();
        m_engine->addTextLayer(image, pixelBounds.left(), pixelBounds.top(), name,
                               typeAlignCode(m_typeAlignment), m_typeAntialias, m_typeVertical,
                               float(m_typeOrigin.x()), float(m_typeOrigin.y()));
    }

    m_typeRuns.clear();
    m_typeText.clear();
    update();
}

void CanvasView::cancelTypeEdit()
{
    m_typing = false;
    m_typeSelecting = false;
    m_typeCaret = 0;
    m_typeAnchor = 0;
    const int editedLayer = m_typeLayer;
    const bool wasNew = m_typeLayerIsNew;
    m_typeLayer = -1;
    m_typeLayerIsNew = false;
    m_typeText.clear();
    m_typeRuns.clear();

    if (m_engine) {
        // Reopened text goes back to showing its committed rendering,
        // untouched.
        m_engine->endTextEdit();
        // A layer this edit put down has nothing to go back to: abandoning the
        // edit abandons the layer, which is what Esc does in Photoshop.
        if (wasNew && editedLayer >= 0) {
            m_engine->deleteLayer(editedLayer);
        }
    }
    refresh();
}

void CanvasView::paintTypeRuns(QPainter &painter, const TypeLayout &layout,
                               const QPointF &origin, const QColor &forcedColor) const
{
    for (const TypeLineBox &line : layout.lines) {
        for (const TypeSegment &segment : line.segments) {
            if (segment.length <= 0) {
                continue;
            }
            painter.setFont(segment.font);
            painter.setPen(forcedColor.isValid() ? forcedColor : segment.color);
            painter.drawText(QPointF(origin.x() + line.x + segment.x,
                                     origin.y() + line.top + segment.y + segment.ascent),
                             m_typeText.mid(segment.start, segment.length));
        }
    }
}

void CanvasView::renderTypeToImage(QImage &image, const QPoint &imageOrigin,
                                   const QColor &forcedColor) const
{
    QPainter painter(&image);
    painter.setRenderHint(QPainter::Antialiasing, m_typeAntialias);
    painter.setRenderHint(QPainter::TextAntialiasing, m_typeAntialias);
    // The same layout and the same routine the overlay draws from, at scale 1 —
    // so what is committed is what was on screen, segment for segment.
    paintTypeRuns(painter, typeLayout(1.0), m_typeOrigin - QPointF(imageOrigin), forcedColor);
}

void CanvasView::paintTypeMaskVeil(QPainter &painter, const TypeLayout &layout,
                                   const QPointF &origin) const
{
    // The bounding box of the document *as seen*: with the view turned, the
    // veil still has to cover every part of the canvas on screen.
    const QRect area =
        viewTransform().mapRect(documentRect()).toAlignedRect().intersected(rect());
    if (area.isEmpty()) {
        return;
    }

    // Built off-screen because the letters are knocked *out* of the veil: the
    // widget's own backing store is opaque, so there is nothing to erase into.
    // Only the part of the document actually on screen is covered, so zooming
    // in does not size this by the whole canvas.
    const qreal ratio = devicePixelRatioF();
    QImage veil(area.size() * ratio, QImage::Format_ARGB32_Premultiplied);
    veil.setDevicePixelRatio(ratio);
    veil.fill(kQuickMaskVeil);

    QPainter cut(&veil);
    cut.setRenderHint(QPainter::TextAntialiasing, m_typeAntialias);
    // DestinationOut subtracts what is drawn from what is there, so an
    // antialiased glyph edge thins the veil by exactly its own coverage.
    cut.setCompositionMode(QPainter::CompositionMode_DestinationOut);
    paintTypeRuns(cut, layout, origin - QPointF(area.topLeft()), Qt::black);
    cut.end();

    painter.drawImage(area.topLeft(), veil);
}

QPolygonF CanvasView::shapeOutlineFor(const QPointF &doc,
                                      Qt::KeyboardModifiers modifiers) const
{
    if (!m_engine) {
        return QPolygonF();
    }
    // The engine owns every shape's geometry, including what Shift and Alt mean
    // to each of them, and this is the same call the commit goes through — so
    // the dashed preview is exactly the shape that will land.
    return m_engine->shapeOutline(float(m_dragStartDoc.x()), float(m_dragStartDoc.y()),
                                  float(doc.x()), float(doc.y()),
                                  modifiers.testFlag(Qt::ShiftModifier),
                                  modifiers.testFlag(Qt::AltModifier));
}

void CanvasView::paintPendingOutline(QPainter &painter, const QPolygonF &widgetOutline) const
{
    // The same two-tone dashed outline the marquee uses, so a gesture in
    // progress reads as pending rather than as something already done —
    // nothing is committed until the button comes up.
    painter.save();
    painter.setRenderHint(QPainter::Antialiasing, true);
    painter.setBrush(Qt::NoBrush);
    painter.setPen(QPen(Qt::black, 1));
    painter.drawPolygon(widgetOutline);
    QPen dashed(Qt::white, 1, Qt::DashLine);
    dashed.setDashPattern({4, 4});
    painter.setPen(dashed);
    painter.drawPolygon(widgetOutline);
    painter.restore();
}

// --------------------------------------------------------- Free Transform --

void CanvasView::beginFreeTransform(TransformMode mode)
{
    if (m_freeTransform || !m_engine) return;

    const int idx = m_engine->getActiveLayerIndex();
    if (idx < 0) return;

    m_ftLayerIndex = idx;
    QImage fullImage = m_engine->layerImage(idx);
    if (fullImage.isNull()) return;

    const int ox = m_engine->layerOffsetX(idx);
    const int oy = m_engine->layerOffsetY(idx);
    m_ftOrigOffset = QPointF(ox, oy);

    QRect cb = m_engine->layerContentBounds(idx);
    if (cb.width() <= 0 || cb.height() <= 0) return;
    m_ftBounds = QRectF(cb);

    QRect cropRect(cb.x() - ox, cb.y() - oy, cb.width(), cb.height());
    m_ftOrigImage = fullImage.copy(cropRect);

    m_ftRotation = 0.0;
    m_ftScale = {1.0, 1.0};
    m_ftHandle = FTHandle::None;
    m_ftMode = mode;
    m_ftQuad = QPolygonF({m_ftBounds.topLeft(), m_ftBounds.topRight(),
                          m_ftBounds.bottomRight(), m_ftBounds.bottomLeft()});

    if (mode == TransformMode::Warp) {
        const QRectF &b = m_ftBounds;
        for (int r = 0; r < 4; ++r)
            for (int c = 0; c < 4; ++c)
                m_warpPts[r][c] = QPointF(b.left() + b.width() * c / 3.0,
                                          b.top() + b.height() * r / 3.0);
        m_warpDragI = m_warpDragJ = -1;
    }

    m_freeTransform = true;
    update();
    emit transformStarted();
    emit transformChanged();
}

static QPointF evalBezier(const QPointF p[4], double t)
{
    const double u = 1.0 - t;
    return u*u*u*p[0] + 3*u*u*t*p[1] + 3*u*t*t*p[2] + t*t*t*p[3];
}

static QPointF evalPatch(const QPointF grid[4][4], double u, double v)
{
    QPointF col[4];
    for (int r = 0; r < 4; ++r) {
        col[r] = evalBezier(grid[r], u);
    }
    return evalBezier(col, v);
}

void CanvasView::commitFreeTransform()
{
    if (!m_freeTransform || !m_engine) return;
    m_freeTransform = false;

    QRectF srcRect(0, 0, m_ftOrigImage.width(), m_ftOrigImage.height());

    if (m_ftMode == TransformMode::Warp) {
        const int N = 30;
        const double sw = m_ftOrigImage.width();
        const double sh = m_ftOrigImage.height();

        // Compute bounding rect of warped patch.
        double minX = 1e9, minY = 1e9, maxX = -1e9, maxY = -1e9;
        for (int j = 0; j <= N; ++j)
            for (int i = 0; i <= N; ++i) {
                QPointF p = evalPatch(m_warpPts, double(i)/N, double(j)/N);
                minX = std::min(minX, p.x());
                minY = std::min(minY, p.y());
                maxX = std::max(maxX, p.x());
                maxY = std::max(maxY, p.y());
            }
        QPointF origin(std::floor(minX), std::floor(minY));
        int rw = qMax(1, int(std::ceil(maxX - origin.x())) + 1);
        int rh = qMax(1, int(std::ceil(maxY - origin.y())) + 1);

        QImage result(rw, rh, QImage::Format_ARGB32_Premultiplied);
        result.fill(Qt::transparent);

        QPainter p(&result);
        p.setRenderHint(QPainter::SmoothPixmapTransform);
        for (int j = 0; j < N; ++j) {
            for (int i = 0; i < N; ++i) {
                const double u0 = double(i)/N, u1 = double(i+1)/N;
                const double v0 = double(j)/N, v1 = double(j+1)/N;

                QPolygonF sp;
                sp << QPointF(u0*sw, v0*sh) << QPointF(u1*sw, v0*sh)
                   << QPointF(u1*sw, v1*sh) << QPointF(u0*sw, v1*sh);

                QPolygonF dp;
                dp << (evalPatch(m_warpPts, u0, v0) - origin)
                   << (evalPatch(m_warpPts, u1, v0) - origin)
                   << (evalPatch(m_warpPts, u1, v1) - origin)
                   << (evalPatch(m_warpPts, u0, v1) - origin);

                QTransform xf;
                if (QTransform::quadToQuad(sp, dp, xf)) {
                    p.save();
                    QPainterPath clip;
                    clip.addPolygon(dp);
                    p.setClipPath(clip);
                    p.setTransform(xf);
                    p.drawImage(0, 0, m_ftOrigImage);
                    p.restore();
                }
            }
        }
        p.end();

        m_engine->replaceLayerPixels(m_ftLayerIndex, result,
                                      int(origin.x()), int(origin.y()));
    } else

    {
    const bool isQuadMode = m_ftMode == TransformMode::Skew
                            || m_ftMode == TransformMode::Distort
                            || m_ftMode == TransformMode::Perspective;

    if (isQuadMode) {
        QRectF mapped = m_ftQuad.boundingRect();
        QImage result(qRound(mapped.width()), qRound(mapped.height()),
                      QImage::Format_ARGB32_Premultiplied);
        result.fill(Qt::transparent);

        QPolygonF srcPoly;
        srcPoly << srcRect.topLeft() << srcRect.topRight()
                << srcRect.bottomRight() << srcRect.bottomLeft();
        QPolygonF dstPoly;
        for (int i = 0; i < 4; ++i)
            dstPoly << (m_ftQuad.at(i) - mapped.topLeft());

        QPainter p(&result);
        p.setRenderHint(QPainter::SmoothPixmapTransform);
        QTransform quadXf;
        if (QTransform::quadToQuad(srcPoly, dstPoly, quadXf)) {
            p.setTransform(quadXf);
            p.drawImage(0, 0, m_ftOrigImage);
        }
        p.end();

        m_engine->replaceLayerPixels(m_ftLayerIndex, result,
                                      qRound(mapped.x()), qRound(mapped.y()));
    } else {
        QPointF center = m_ftBounds.center();
        QTransform xf;
        xf.translate(center.x(), center.y());
        xf.rotate(m_ftRotation);
        xf.translate(-center.x(), -center.y());

        QPolygonF dstQuad;
        dstQuad << xf.map(m_ftBounds.topLeft())
                << xf.map(m_ftBounds.topRight())
                << xf.map(m_ftBounds.bottomRight())
                << xf.map(m_ftBounds.bottomLeft());
        QRectF mapped = dstQuad.boundingRect();

        QImage result(qMax(1, qRound(mapped.width())),
                      qMax(1, qRound(mapped.height())),
                      QImage::Format_ARGB32_Premultiplied);
        result.fill(Qt::transparent);

        QPolygonF srcPoly;
        srcPoly << srcRect.topLeft() << srcRect.topRight()
                << srcRect.bottomRight() << srcRect.bottomLeft();
        QPolygonF localDst;
        for (int i = 0; i < 4; ++i)
            localDst << (dstQuad.at(i) - mapped.topLeft());

        QPainter p(&result);
        p.setRenderHint(QPainter::SmoothPixmapTransform);
        QTransform quadXf;
        if (QTransform::quadToQuad(srcPoly, localDst, quadXf)) {
            p.setTransform(quadXf);
            p.drawImage(0, 0, m_ftOrigImage);
        }
        p.end();

        m_engine->replaceLayerPixels(m_ftLayerIndex, result,
                                      qRound(mapped.x()), qRound(mapped.y()));
    }
    } // end else (non-warp)

    m_ftOrigImage = QImage();
    updateCursor();
    refresh();
    emit transformCommitted();
}

void CanvasView::cancelFreeTransform()
{
    if (!m_freeTransform) return;
    m_freeTransform = false;
    m_ftOrigImage = QImage();
    updateCursor();
    update();
    emit transformCancelled();
}

void CanvasView::paintFreeTransform(QPainter &painter)
{
    if (!m_freeTransform) return;

    if (m_ftMode == TransformMode::Warp) {
        const int N = 20;
        const double sw = m_ftOrigImage.width();
        const double sh = m_ftOrigImage.height();

        painter.save();
        painter.setRenderHint(QPainter::SmoothPixmapTransform);
        for (int j = 0; j < N; ++j) {
            for (int i = 0; i < N; ++i) {
                const double u0 = double(i) / N, u1 = double(i + 1) / N;
                const double v0 = double(j) / N, v1 = double(j + 1) / N;

                QPolygonF srcPoly;
                srcPoly << QPointF(u0 * sw, v0 * sh)
                        << QPointF(u1 * sw, v0 * sh)
                        << QPointF(u1 * sw, v1 * sh)
                        << QPointF(u0 * sw, v1 * sh);

                QPolygonF dstPoly;
                dstPoly << documentToWidget(evalPatch(m_warpPts, u0, v0))
                        << documentToWidget(evalPatch(m_warpPts, u1, v0))
                        << documentToWidget(evalPatch(m_warpPts, u1, v1))
                        << documentToWidget(evalPatch(m_warpPts, u0, v1));

                QTransform xf;
                if (QTransform::quadToQuad(srcPoly, dstPoly, xf)) {
                    painter.save();
                    QPainterPath clip;
                    clip.addPolygon(dstPoly);
                    painter.setClipPath(clip);
                    painter.setTransform(xf);
                    painter.drawImage(0, 0, m_ftOrigImage);
                    painter.restore();
                }
            }
        }
        painter.restore();

        painter.save();
        painter.setRenderHint(QPainter::Antialiasing);

        // Grid lines.
        painter.setPen(QPen(QColor(255, 255, 255, 160), 1));
        const int gridSteps = 3;
        for (int line = 0; line <= gridSteps; ++line) {
            double t = double(line) / gridSteps;
            QPainterPath hPath, vPath;
            for (int seg = 0; seg <= 40; ++seg) {
                double s = double(seg) / 40.0;
                QPointF hp = documentToWidget(evalPatch(m_warpPts, s, t));
                QPointF vp = documentToWidget(evalPatch(m_warpPts, t, s));
                if (seg == 0) { hPath.moveTo(hp); vPath.moveTo(vp); }
                else { hPath.lineTo(hp); vPath.lineTo(vp); }
            }
            painter.drawPath(hPath);
            painter.drawPath(vPath);
        }

        // Control point tangent lines and handles.
        const double hs = 4.0;
        const double hsc = 3.0;
        auto drawSquare = [&](const QPointF &pt) {
            painter.setPen(QPen(Qt::black, 1));
            painter.setBrush(Qt::white);
            painter.drawRect(QRectF(pt.x() - hs, pt.y() - hs, hs * 2, hs * 2));
        };
        auto drawCircle = [&](const QPointF &pt) {
            painter.setPen(QPen(Qt::black, 1));
            painter.setBrush(Qt::white);
            painter.drawEllipse(pt, hsc, hsc);
        };

        for (int r = 0; r < 4; ++r) {
            for (int c = 0; c < 4; ++c) {
                QPointF wp = documentToWidget(m_warpPts[r][c]);
                bool isCorner = (r == 0 || r == 3) && (c == 0 || c == 3);
                if (isCorner) {
                    // Draw tangent lines from corner to adjacent control points.
                    painter.setPen(QPen(QColor(100, 100, 100), 1));
                    if (c == 0) {
                        painter.drawLine(wp, documentToWidget(m_warpPts[r][1]));
                    } else {
                        painter.drawLine(wp, documentToWidget(m_warpPts[r][2]));
                    }
                    if (r == 0) {
                        painter.drawLine(wp, documentToWidget(m_warpPts[1][c]));
                    } else {
                        painter.drawLine(wp, documentToWidget(m_warpPts[2][c]));
                    }
                    drawSquare(wp);
                } else {
                    drawCircle(wp);
                }
            }
        }

        painter.restore();
        return;
    }

    const bool isQuadMode = m_ftMode == TransformMode::Skew
                            || m_ftMode == TransformMode::Distort
                            || m_ftMode == TransformMode::Perspective;

    QPolygonF corners;
    QPointF centerDoc;

    if (isQuadMode) {
        for (int i = 0; i < 4; ++i)
            corners << documentToWidget(m_ftQuad.at(i));
        QPointF qc;
        for (int i = 0; i < 4; ++i) qc += m_ftQuad.at(i);
        centerDoc = qc / 4.0;
    } else {
        const QPointF center = m_ftBounds.center();
        QTransform xf;
        xf.translate(center.x(), center.y());
        xf.rotate(m_ftRotation);
        xf.translate(-center.x(), -center.y());
        corners << documentToWidget(xf.map(m_ftBounds.topLeft()))
                << documentToWidget(xf.map(m_ftBounds.topRight()))
                << documentToWidget(xf.map(m_ftBounds.bottomRight()))
                << documentToWidget(xf.map(m_ftBounds.bottomLeft()));
        centerDoc = center;
    }

    // Draw the transformed layer preview.
    painter.save();
    painter.setRenderHint(QPainter::SmoothPixmapTransform);
    {
        QRectF srcRect(0, 0, m_ftOrigImage.width(), m_ftOrigImage.height());
        QPolygonF srcPoly;
        srcPoly << srcRect.topLeft() << srcRect.topRight()
                << srcRect.bottomRight() << srcRect.bottomLeft();

        QTransform quadXf;
        if (QTransform::quadToQuad(srcPoly, corners, quadXf)) {
            painter.setTransform(quadXf);
            painter.drawImage(0, 0, m_ftOrigImage);
            painter.resetTransform();
        }
    }
    painter.restore();

    // Draw the bounding box and handles.
    painter.save();
    painter.setRenderHint(QPainter::Antialiasing);
    painter.setPen(QPen(Qt::black, 1));
    painter.setBrush(Qt::NoBrush);
    painter.drawPolygon(corners);

    QPointF midTop = (corners[0] + corners[1]) / 2.0;
    QPointF midRight = (corners[1] + corners[2]) / 2.0;
    QPointF midBottom = (corners[2] + corners[3]) / 2.0;
    QPointF midLeft = (corners[3] + corners[0]) / 2.0;

    const double hs = 4.0;
    auto drawHandle = [&](const QPointF &pt) {
        painter.fillRect(QRectF(pt.x() - hs, pt.y() - hs, hs * 2, hs * 2), Qt::white);
        painter.drawRect(QRectF(pt.x() - hs, pt.y() - hs, hs * 2, hs * 2));
    };

    for (int i = 0; i < corners.size(); ++i) drawHandle(corners[i]);
    drawHandle(midTop);
    drawHandle(midRight);
    drawHandle(midBottom);
    drawHandle(midLeft);

    // Center pivot.
    QPointF cp = documentToWidget(centerDoc);
    painter.setPen(QPen(Qt::black, 1));
    painter.drawLine(cp - QPointF(6, 0), cp + QPointF(6, 0));
    painter.drawLine(cp - QPointF(0, 6), cp + QPointF(0, 6));
    painter.drawEllipse(cp, 4.0, 4.0);

    painter.restore();
}

void CanvasView::setSearchHighlight(int layerIndex, int charOffset, int charLength)
{
    m_searchHighlightLayer = layerIndex;
    m_searchHighlightChar = charOffset;
    m_searchHighlightLen = charLength;
    update();
}

void CanvasView::clearSearchHighlight()
{
    m_searchHighlightLayer = -1;
    m_searchHighlightChar = -1;
    m_searchHighlightLen = 0;
    update();
}

void CanvasView::paintSearchHighlight(QPainter &painter)
{
    if (m_searchHighlightLayer < 0 || !m_engine || m_searchHighlightLen <= 0) {
        return;
    }

    const int idx = m_searchHighlightLayer;
    if (m_engine->layerKind(idx) != 2) {
        return;
    }

    const int runCount = m_engine->layerTextRunCount(idx);
    if (runCount <= 0) {
        return;
    }

    const float originX = m_engine->layerTextOriginX(idx);
    const float originY = m_engine->layerTextOriginY(idx);
    const bool vertical = m_engine->layerTextVertical(idx);

    // Gather runs and full text.
    struct RunInfo {
        QString text;
        QFont font;
        int start;
    };
    QList<RunInfo> runs;
    QString fullText;
    for (int r = 0; r < runCount; ++r) {
        RunInfo ri;
        ri.text = m_engine->layerTextRunText(idx, r);
        ri.start = fullText.length();
        QString family = m_engine->layerTextRunFamily(idx, r);
        QString style = m_engine->layerTextRunStyle(idx, r);
        float size = m_engine->layerTextRunSize(idx, r);
        ri.font = QFont(family);
        ri.font.setStyleName(style);
        ri.font.setPixelSize(qRound(size));
        runs.append(ri);
        fullText += ri.text;
    }

    // Split into lines and find which line(s) the match is on.
    const int matchStart = m_searchHighlightChar;
    const int matchEnd = matchStart + m_searchHighlightLen;

    QStringList lines = fullText.split(QLatin1Char('\n'));
    int lineCharStart = 0;
    qreal yOffset = 0;

    for (const QString &lineStr : lines) {
        const int lineEnd = lineCharStart + lineStr.length();

        // Check if the match overlaps this line.
        const int overlapStart = qMax(matchStart, lineCharStart);
        const int overlapEnd = qMin(matchEnd, lineEnd);

        if (overlapStart < overlapEnd) {
            // Find the font for the match start to get metrics.
            QFont matchFont;
            for (const RunInfo &ri : std::as_const(runs)) {
                int runEnd = ri.start + ri.text.length();
                if (overlapStart >= ri.start && overlapStart < runEnd) {
                    matchFont = ri.font;
                    break;
                }
            }
            QFontMetricsF fm(matchFont);

            // Measure x offset: advance of text before the match on this line.
            qreal xBefore = 0;
            for (const RunInfo &ri : std::as_const(runs)) {
                int runStart = ri.start;
                int runEnd = runStart + ri.text.length();
                int segStart = qMax(runStart, lineCharStart);
                int segEnd = qMin(runEnd, overlapStart);
                if (segStart < segEnd) {
                    QFontMetricsF sfm(ri.font);
                    xBefore += sfm.horizontalAdvance(fullText.mid(segStart, segEnd - segStart));
                }
            }

            // Measure the matched portion's width.
            qreal matchWidth = 0;
            for (const RunInfo &ri : std::as_const(runs)) {
                int runStart = ri.start;
                int runEnd = runStart + ri.text.length();
                int segStart = qMax(runStart, overlapStart);
                int segEnd = qMin(runEnd, overlapEnd);
                if (segStart < segEnd) {
                    QFontMetricsF sfm(ri.font);
                    matchWidth += sfm.horizontalAdvance(fullText.mid(segStart, segEnd - segStart));
                }
            }

            qreal lineHeight = fm.height();

            QRectF highlightRect;
            if (vertical) {
                highlightRect = QRectF(originX + yOffset, originY + xBefore,
                                       lineHeight, matchWidth);
            } else {
                highlightRect = QRectF(originX + xBefore, originY + yOffset,
                                       matchWidth, lineHeight);
            }

            // Transform to widget coordinates and draw.
            QPointF topLeft = documentToWidget(highlightRect.topLeft());
            QPointF bottomRight = documentToWidget(highlightRect.bottomRight());
            QRectF widgetRect(topLeft, bottomRight);

            painter.save();
            painter.setRenderHint(QPainter::Antialiasing, false);
            painter.fillRect(widgetRect, QColor(255, 255, 0, 160));
            painter.restore();
        }

        if (vertical) {
            // Vertical text: each line is a column.
            QFont lineFont;
            for (const RunInfo &ri : std::as_const(runs)) {
                if (ri.start <= lineCharStart && ri.start + ri.text.length() > lineCharStart) {
                    lineFont = ri.font;
                    break;
                }
            }
            QFontMetricsF lfm(lineFont);
            yOffset -= lfm.height();
        } else {
            QFont lineFont;
            for (const RunInfo &ri : std::as_const(runs)) {
                if (ri.start <= lineCharStart && ri.start + ri.text.length() > lineCharStart) {
                    lineFont = ri.font;
                    break;
                }
            }
            QFontMetricsF lfm(lineFont);
            yOffset += lfm.height();
        }

        lineCharStart = lineEnd + 1; // +1 for \n
    }
}

void CanvasView::paintShapeOverlay(QPainter &painter)
{
    if (!m_shapeDragging || m_shapeOutline.size() < 2) {
        return;
    }

    QPolygonF widgetOutline;
    widgetOutline.reserve(m_shapeOutline.size());
    for (const QPointF &point : m_shapeOutline) {
        widgetOutline.append(documentToWidget(point));
    }
    paintPendingOutline(painter, widgetOutline);
}

void CanvasView::paintZoomOverlay(QPainter &painter)
{
    if (!m_zoomDragging || !m_zoomRectDoc.isValid()) {
        return;
    }

    // Through `documentToWidget`, so the rectangle sits on the image it marks
    // out even when the view is turned.
    QPolygonF outline;
    outline.append(documentToWidget(m_zoomRectDoc.topLeft()));
    outline.append(documentToWidget(m_zoomRectDoc.topRight()));
    outline.append(documentToWidget(m_zoomRectDoc.bottomRight()));
    outline.append(documentToWidget(m_zoomRectDoc.bottomLeft()));
    paintPendingOutline(painter, outline);
}

void CanvasView::paintTypeOverlay(QPainter &painter)
{
    if (!m_typing) {
        return;
    }

    const TypeLayout layout = typeLayout(m_zoom);
    const QPointF origin = documentToWidget(m_typeOrigin);

    painter.save();
    painter.setRenderHint(QPainter::TextAntialiasing, m_typeAntialias);
    if (m_typeMask) {
        paintTypeMaskVeil(painter, layout, origin);
    } else {
        paintTypeRuns(painter, layout, origin);
    }

    // Against the veil the text has no ink of its own to invert, so mask type
    // marks the caret and the selection in white instead.
    const QColor marker = m_typeMask ? QColor(Qt::white) : m_typeColor;

    if (typeHasSelection()) {
        // Selected text is drawn inverted, the way Photoshop shows it: the run
        // is filled with the text's own colour and the glyphs over it in the
        // opposite. The glyphs are redrawn by the *same* routine that drew them
        // the first time, clipped to the highlight — measuring them a second
        // way could not be relied on to land on the same pixels.
        const QColor inverse = m_typeColor.lightnessF() > 0.5 ? Qt::black : Qt::white;
        const int from = typeSelectionStart();
        const int to = typeSelectionEnd();

        for (int i = 0; i < layout.lines.size(); ++i) {
            const TypeLineBox &line = layout.lines.at(i);
            const int selFrom = qMax(from, line.start);
            const int selTo = qMin(to, line.start + line.length);
            if (selFrom >= selTo) {
                continue;
            }

            const QRectF highlight =
                typeRangeRect(layout, i, selFrom, selTo).translated(origin);
            if (m_typeMask) {
                // A wash rather than an inversion: the knocked-out letters have
                // to stay readable through it.
                painter.fillRect(highlight, QColor(255, 255, 255, 90));
                continue;
            }
            painter.fillRect(highlight, m_typeColor);
            painter.save();
            painter.setClipRect(highlight);
            paintTypeRuns(painter, layout, origin, inverse);
            painter.restore();
        }
    } else {
        // The caret: as tall as the line it sits on for horizontal type — so it
        // grows beside a larger word — and as wide as the column for vertical.
        // CS6's blinks; this does not, to avoid a timer for what is otherwise a
        // static overlay.
        painter.fillRect(typeCaretRect(layout, m_typeCaret).translated(origin), marker);
    }

    painter.restore();
}

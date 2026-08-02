#include "CanvasView.h"

#include "cxx-qt-lib/qcolor.h"
#include "photorust_core/src/bridge.cxxqt.h"

#include <QContextMenuEvent>
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
    m_tool = tool;

    // Switching tools mid-gesture would leave the engine holding a half-built
    // stroke, so end the gesture cleanly first.
    if (m_dragging && m_engine) {
        m_engine->cancelStroke();
    }
    m_dragging = false;
    m_marqueeActive = false;

    if (m_engine) {
        m_engine->setEraseMode(tool == ToolId::Eraser);
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

    // Live marquee while the user is dragging one out. Drawn as the shape the
    // active variant will actually produce, so an elliptical drag previews an
    // ellipse rather than its bounding box.
    if (m_marqueeActive && !m_marquee.isNull()) {
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
    case ToolId::QuickSelect:
        m_marqueeActive = true;
        m_gestureModifiers = event->modifiers();
        m_dragStartDoc = doc;
        m_marquee = QRectF(doc, doc);
        update();
        return;

    case ToolId::Move:
        m_dragging = true;
        m_dragStartDoc = doc;
        return;

    default:
        break;
    }

    if (toolPaints(m_tool) && m_engine) {
        // A tablet would supply real pressure here; a mouse reports full.
        if (m_engine->beginStroke(float(doc.x()), float(doc.y()), 1.0f)) {
            m_dragging = true;
            m_image = m_engine->previewImage();
            update();
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

    if (m_marqueeActive) {
        if (m_tool == ToolId::Marquee && marqueeIsLineSelect()) {
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

void CanvasView::mouseReleaseEvent(QMouseEvent *event)
{
    const QPointF doc = widgetToDocument(event->position());

    if (m_panning) {
        m_panning = false;
        updateCursor();
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

    // Escape abandons an in-progress stroke or marquee.
    if (event->key() == Qt::Key_Escape) {
        if (m_dragging && m_engine) {
            m_engine->cancelStroke();
            m_dragging = false;
            refresh();
        }
        m_marqueeActive = false;
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
}

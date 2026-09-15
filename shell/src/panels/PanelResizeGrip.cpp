#include "PanelResizeGrip.h"

#include <QDockWidget>
#include <QEvent>
#include <QMouseEvent>
#include <QPainter>

namespace {

/// The grip's box. Small, as CS6's is — it sits in the corner of the panel
/// footer and must not swallow the buttons along it.
constexpr int kGripSize = 14;
/// How far in from the panel's edges it sits.
constexpr int kInset = 1;

/// The hatch: diagonal strokes stepping away from the corner.
constexpr int kStrokes = 3;
constexpr int kStep = 4;

const QColor kHatchColor(0x9a, 0x9a, 0x9a);
const QColor kHatchShadow(0x2a, 0x2a, 0x2a);

} // namespace

PanelResizeGrip::PanelResizeGrip(QDockWidget *dock)
    : QWidget(dock)
    , m_dock(dock)
{
    setObjectName(QStringLiteral("panelResizeGrip"));
    // Nothing of its own to paint but the hatch: the footer it sits over
    // shows through around the strokes.
    setAttribute(Qt::WA_NoSystemBackground, true);
    setFixedSize(kGripSize, kGripSize);
    setCursor(Qt::SizeFDiagCursor);
    setToolTip(tr("Drag to resize the panel"));
    // The panel's contents are laid out by the dock and know nothing about
    // this, so it has to be kept on top by hand.
    raise();
    setVisible(dock->isFloating());

    connect(dock, &QDockWidget::topLevelChanged, this, [this](bool floating) {
        setVisible(floating);
        if (floating) {
            raise();
            moveToCorner();
        }
    });
    dock->installEventFilter(this);
    moveToCorner();
}

bool PanelResizeGrip::eventFilter(QObject *watched, QEvent *event)
{
    if (watched == m_dock && event->type() == QEvent::Resize) {
        moveToCorner();
    }
    return QWidget::eventFilter(watched, event);
}

void PanelResizeGrip::moveToCorner()
{
    if (!m_dock) {
        return;
    }
    move(m_dock->width() - width() - kInset, m_dock->height() - height() - kInset);
}

void PanelResizeGrip::paintEvent(QPaintEvent *event)
{
    Q_UNUSED(event)
    QPainter painter(this);
    // Three strokes parallel to the corner's diagonal, stepping away from it:
    // the shortest sits in the corner itself and each one out is longer.
    const int right = width() - kInset;
    const int bottom = height() - kInset;
    for (int step = 1; step <= kStrokes; ++step) {
        const QLine stroke(right - step * kStep, bottom, right, bottom - step * kStep);
        // A dark stroke under the light one, so the hatch reads over a pale
        // footer as well as over the dark theme.
        painter.setPen(QPen(kHatchShadow, 1.0));
        painter.drawLine(stroke.translated(1, 1));
        painter.setPen(QPen(kHatchColor, 1.0));
        painter.drawLine(stroke);
    }
}

void PanelResizeGrip::mousePressEvent(QMouseEvent *event)
{
    if (event->button() != Qt::LeftButton || !m_dock || !m_dock->isFloating()) {
        QWidget::mousePressEvent(event);
        return;
    }
    m_dragging = true;
    m_from = event->globalPosition().toPoint();
    m_was = m_dock->size();
}

void PanelResizeGrip::mouseMoveEvent(QMouseEvent *event)
{
    if (!m_dragging || !m_dock) {
        return;
    }
    const QPoint travelled = event->globalPosition().toPoint() - m_from;
    // Never smaller than the panel says it can be — dragging past that would
    // otherwise leave the contents clipped with no way to see them again.
    const QSize least = m_dock->minimumSizeHint().expandedTo(m_dock->minimumSize());
    m_dock->resize(QSize(m_was.width() + travelled.x(), m_was.height() + travelled.y())
                       .expandedTo(least));
}

void PanelResizeGrip::mouseReleaseEvent(QMouseEvent *event)
{
    Q_UNUSED(event)
    m_dragging = false;
}

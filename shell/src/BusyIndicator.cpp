#include "BusyIndicator.h"

#include <QPainter>

namespace {

/// How many spokes, and how often the lead moves on: a full turn a second.
constexpr int kSpokes = 12;
constexpr int kStepMs = 1000 / kSpokes;

} // namespace

BusyIndicator::BusyIndicator(QWidget *parent)
    : QWidget(parent)
{
    setFixedSize(sizeHint());
    m_timer.setInterval(kStepMs);
    connect(&m_timer, &QTimer::timeout, this, [this] {
        m_step = (m_step + 1) % kSpokes;
        update();
    });
    hide();
}

void BusyIndicator::start()
{
    m_step = 0;
    m_timer.start();
    show();
}

void BusyIndicator::stop()
{
    m_timer.stop();
    hide();
}

void BusyIndicator::paintEvent(QPaintEvent *)
{
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.translate(width() / 2.0, height() / 2.0);

    const double outer = qMin(width(), height()) / 2.0 - 1.0;
    const double inner = outer * 0.45;
    QColor colour = palette().color(QPalette::WindowText);
    QPen pen(colour, qMax(1.5, outer * 0.24), Qt::SolidLine, Qt::RoundCap);

    for (int i = 0; i < kSpokes; ++i) {
        // The lead spoke is solid; the ones behind it fade out round the
        // wheel, which is what reads as motion.
        const int behind = (m_step - i + kSpokes) % kSpokes;
        colour.setAlphaF(1.0 - 0.85 * behind / double(kSpokes - 1));
        pen.setColor(colour);
        painter.setPen(pen);
        painter.save();
        painter.rotate(i * 360.0 / kSpokes);
        painter.drawLine(QPointF(0, -inner), QPointF(0, -outer));
        painter.restore();
    }
}

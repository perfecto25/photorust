#pragma once

#include <QMouseEvent>
#include <QPainter>
#include <QWidget>
#include <QtMath>

#include <cmath>

/// CS6's angle dial: a circle with a radius line, dragged to set the angle.
///
/// The angle names the direction the light comes *from*, so the line points at
/// it — the same convention the engine reads, and the reason 90° points up
/// rather than down: screen y counts the other way.
class AngleDial : public QWidget
{
    Q_OBJECT
public:
    explicit AngleDial(QWidget *parent = nullptr)
        : QWidget(parent)
    {
        setFixedSize(34, 34);
        setCursor(Qt::CrossCursor);
    }

    void setAngle(double degrees)
    {
        const double wrapped = std::fmod(std::fmod(degrees, 360.0) + 360.0, 360.0);
        if (qFuzzyCompare(wrapped + 1.0, m_angle + 1.0)) {
            return;
        }
        m_angle = wrapped;
        update();
    }

signals:
    void angleChanged(double degrees);

protected:
    void paintEvent(QPaintEvent *) override
    {
        QPainter painter(this);
        painter.setRenderHint(QPainter::Antialiasing, true);

        const QRectF face = QRectF(rect()).adjusted(2, 2, -2, -2);
        painter.setPen(QPen(QColor(0x88, 0x88, 0x88), 1.0));
        painter.setBrush(QColor(0x3a, 0x3a, 0x3a));
        painter.drawEllipse(face);

        const QPointF centre = face.center();
        const double radians = qDegreesToRadians(m_angle);
        const QPointF tip(centre.x() + std::cos(radians) * face.width() / 2.2,
                          centre.y() - std::sin(radians) * face.height() / 2.2);
        painter.setPen(QPen(QColor(0xe8, 0xe8, 0xe8), 1.4));
        painter.drawLine(centre, tip);
        painter.setPen(Qt::NoPen);
        painter.setBrush(QColor(0xe8, 0xe8, 0xe8));
        painter.drawEllipse(centre, 1.6, 1.6);
    }

    void mousePressEvent(QMouseEvent *event) override { aim(event->position()); }
    void mouseMoveEvent(QMouseEvent *event) override { aim(event->position()); }

private:
    void aim(const QPointF &pos)
    {
        const QPointF centre = QRectF(rect()).center();
        const double degrees =
            qRadiansToDegrees(std::atan2(centre.y() - pos.y(), pos.x() - centre.x()));
        setAngle(degrees);
        emit angleChanged(m_angle);
    }

    double m_angle = 0.0;
};

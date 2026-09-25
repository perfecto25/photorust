#pragma once

#include <QTimer>
#include <QWidget>

/// A small spinning wheel of spokes, the one Photoshop and most desktop
/// applications show while they work: each spoke fades behind the one
/// leading it, and the lead steps round a notch at a time.
///
/// Painted rather than an animated image, so it takes the palette's text
/// colour and sits right in the dark theme at any size. It only turns while
/// the event loop is running — which is why long work that wants it shown
/// runs off the GUI thread.
class BusyIndicator : public QWidget
{
    Q_OBJECT

public:
    explicit BusyIndicator(QWidget *parent = nullptr);

    /// Start turning and show; stop and hide.
    void start();
    void stop();
    bool isSpinning() const { return m_timer.isActive(); }

    QSize sizeHint() const override { return QSize(16, 16); }

protected:
    void paintEvent(QPaintEvent *event) override;

private:
    QTimer m_timer;
    int m_step = 0;
};

#pragma once

#include <QColor>
#include <QLabel>
#include <QSlider>
#include <QSpinBox>
#include <QWidget>

class Engine;

/// A colour swatch that draws the foreground/background pair the way the tool
/// strip does: two overlapping squares, with a small reset and swap control.
class ColorSwatchWidget : public QWidget
{
    Q_OBJECT

public:
    explicit ColorSwatchWidget(QWidget *parent = nullptr);

    QColor foreground() const { return m_foreground; }
    QColor background() const { return m_background; }
    void setForeground(const QColor &c);
    void setBackground(const QColor &c);

    /// Swap the pair — the X shortcut.
    void swap();
    /// Reset to black on white — the D shortcut.
    void reset();

signals:
    void foregroundChanged(const QColor &color);
    void backgroundChanged(const QColor &color);

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    QSize sizeHint() const override;

private:
    /// Hit rectangles, computed from the widget size so they stay in sync with
    /// what paintEvent draws.
    QRect foregroundRect() const;
    QRect backgroundRect() const;
    QRect swapRect() const;
    QRect resetRect() const;

    QColor m_foreground{Qt::black};
    QColor m_background{Qt::white};
};

/// The Color panel: RGB sliders plus the foreground/background swatch.
class ColorPanel : public QWidget
{
    Q_OBJECT

public:
    explicit ColorPanel(Engine *engine, QWidget *parent = nullptr);

    /// Push a colour in from elsewhere (the eyedropper, say) without emitting
    /// a change back out.
    void setForegroundColor(const QColor &color);

signals:
    void foregroundChanged(const QColor &color);

private slots:
    void onSliderChanged();
    void onSwatchForegroundChanged(const QColor &color);

private:
    void syncSlidersTo(const QColor &color);
    void pushToEngine(const QColor &color);

    Engine *m_engine = nullptr;
    ColorSwatchWidget *m_swatch = nullptr;

    QSlider *m_red = nullptr;
    QSlider *m_green = nullptr;
    QSlider *m_blue = nullptr;
    QSpinBox *m_redValue = nullptr;
    QSpinBox *m_greenValue = nullptr;
    QSpinBox *m_blueValue = nullptr;
    QLabel *m_hex = nullptr;

    bool m_updating = false;
};

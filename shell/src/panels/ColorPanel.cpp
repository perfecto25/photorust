#include "ColorPanel.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QColorDialog>
#include <QGridLayout>
#include <QMouseEvent>
#include <QPainter>
#include <QVBoxLayout>

namespace {

/// Side of each colour square in the swatch widget.
constexpr int kSwatchSize = 26;
/// How far the background square is offset down-right from the foreground one.
constexpr int kSwatchOffset = 14;
/// Side of the little swap / reset affordances.
constexpr int kMiniSize = 10;

} // namespace

// =========================================================== ColorSwatchWidget

ColorSwatchWidget::ColorSwatchWidget(QWidget *parent)
    : QWidget(parent)
{
    setObjectName(QStringLiteral("colorSwatches"));
    setToolTip(tr("Foreground and background colors\nX to swap, D to reset"));
}

QSize ColorSwatchWidget::sizeHint() const
{
    // Room for both squares plus the mini controls in the corners.
    return QSize(kSwatchSize + kSwatchOffset + kMiniSize + 4,
                 kSwatchSize + kSwatchOffset + kMiniSize + 4);
}

QRect ColorSwatchWidget::foregroundRect() const
{
    return QRect(kMiniSize + 2, 0, kSwatchSize, kSwatchSize);
}

QRect ColorSwatchWidget::backgroundRect() const
{
    return QRect(kMiniSize + 2 + kSwatchOffset, kSwatchOffset, kSwatchSize, kSwatchSize);
}

QRect ColorSwatchWidget::swapRect() const
{
    // Top-right, as in the tool strip.
    return QRect(width() - kMiniSize - 1, 0, kMiniSize, kMiniSize);
}

QRect ColorSwatchWidget::resetRect() const
{
    // Bottom-left.
    return QRect(0, height() - kMiniSize - 1, kMiniSize, kMiniSize);
}

void ColorSwatchWidget::setForeground(const QColor &c)
{
    if (m_foreground == c) {
        return;
    }
    m_foreground = c;
    update();
    emit foregroundChanged(c);
}

void ColorSwatchWidget::setBackground(const QColor &c)
{
    if (m_background == c) {
        return;
    }
    m_background = c;
    update();
    emit backgroundChanged(c);
}

void ColorSwatchWidget::swap()
{
    const QColor fg = m_foreground;
    m_foreground = m_background;
    m_background = fg;
    update();
    emit foregroundChanged(m_foreground);
    emit backgroundChanged(m_background);
}

void ColorSwatchWidget::reset()
{
    m_foreground = Qt::black;
    m_background = Qt::white;
    update();
    emit foregroundChanged(m_foreground);
    emit backgroundChanged(m_background);
}

void ColorSwatchWidget::paintEvent(QPaintEvent *)
{
    QPainter p(this);
    p.setRenderHint(QPainter::Antialiasing, false);

    // Background square first — the foreground one overlaps it.
    const QRect bg = backgroundRect();
    p.fillRect(bg, m_background);
    p.setPen(QColor(0x2a, 0x2a, 0x2a));
    p.drawRect(bg.adjusted(0, 0, -1, -1));

    const QRect fg = foregroundRect();
    p.fillRect(fg, m_foreground);
    p.setPen(QColor(0x2a, 0x2a, 0x2a));
    p.drawRect(fg.adjusted(0, 0, -1, -1));
    // A light inner edge lifts the square off the panel, as CS6 does.
    p.setPen(QColor(0xa0, 0xa0, 0xa0));
    p.drawRect(fg.adjusted(1, 1, -2, -2));

    // Swap arrows: two small squares with a corner elbow.
    const QRect swap = swapRect();
    p.setPen(QColor(0xd4, 0xd4, 0xd4));
    p.drawLine(swap.left() + 2, swap.top() + 2, swap.right() - 2, swap.top() + 2);
    p.drawLine(swap.right() - 2, swap.top() + 2, swap.right() - 2, swap.bottom() - 2);

    // Reset: a miniature of the default black-on-white pair.
    const QRect reset = resetRect();
    p.fillRect(QRect(reset.left() + 3, reset.top() + 3, 6, 6), Qt::white);
    p.setPen(QColor(0x2a, 0x2a, 0x2a));
    p.drawRect(reset.left() + 3, reset.top() + 3, 6, 6);
    p.fillRect(QRect(reset.left(), reset.top(), 6, 6), Qt::black);
    p.drawRect(reset.left(), reset.top(), 5, 5);
}

void ColorSwatchWidget::mousePressEvent(QMouseEvent *event)
{
    const QPoint pos = event->pos();

    if (swapRect().contains(pos)) {
        swap();
        return;
    }
    if (resetRect().contains(pos)) {
        reset();
        return;
    }

    // Test foreground first — it is drawn on top where the two overlap.
    if (foregroundRect().contains(pos)) {
        const QColor picked =
            QColorDialog::getColor(m_foreground, this, tr("Color Picker (Foreground Color)"));
        if (picked.isValid()) {
            setForeground(picked);
        }
        return;
    }
    if (backgroundRect().contains(pos)) {
        const QColor picked =
            QColorDialog::getColor(m_background, this, tr("Color Picker (Background Color)"));
        if (picked.isValid()) {
            setBackground(picked);
        }
    }
}

// ================================================================= ColorPanel

ColorPanel::ColorPanel(Engine *engine, QWidget *parent)
    : QWidget(parent)
    , m_engine(engine)
{
    auto *root = new QVBoxLayout(this);
    root->setContentsMargins(6, 6, 6, 6);
    root->setSpacing(5);

    m_swatch = new ColorSwatchWidget(this);
    root->addWidget(m_swatch, 0, Qt::AlignLeft);

    auto *grid = new QGridLayout();
    grid->setSpacing(3);
    grid->setContentsMargins(0, 0, 0, 0);

    auto addChannel = [&](int row, const QString &label, QSlider *&slider,
                          QSpinBox *&spin, const QColor &tint) {
        auto *name = new QLabel(label, this);
        name->setFixedWidth(12);
        grid->addWidget(name, row, 0);

        slider = new QSlider(Qt::Horizontal, this);
        slider->setRange(0, 255);
        // Tint the filled portion so each channel reads at a glance.
        slider->setStyleSheet(
            QStringLiteral("QSlider::sub-page:horizontal { background-color: %1; }")
                .arg(tint.name()));
        grid->addWidget(slider, row, 1);

        spin = new QSpinBox(this);
        spin->setRange(0, 255);
        spin->setFixedWidth(46);
        spin->setButtonSymbols(QAbstractSpinBox::NoButtons);
        grid->addWidget(spin, row, 2);
    };

    addChannel(0, tr("R"), m_red, m_redValue, QColor(0xd0, 0x50, 0x50));
    addChannel(1, tr("G"), m_green, m_greenValue, QColor(0x50, 0xc0, 0x50));
    addChannel(2, tr("B"), m_blue, m_blueValue, QColor(0x50, 0x70, 0xd0));
    grid->setColumnStretch(1, 1);
    root->addLayout(grid);

    m_hex = new QLabel(QStringLiteral("#000000"), this);
    m_hex->setAlignment(Qt::AlignRight);
    root->addWidget(m_hex);
    root->addStretch(1);

    // Keep each slider and its spin box locked together.
    for (auto [slider, spin] : {std::pair{m_red, m_redValue},
                                std::pair{m_green, m_greenValue},
                                std::pair{m_blue, m_blueValue}}) {
        connect(slider, &QSlider::valueChanged, spin, &QSpinBox::setValue);
        connect(spin, &QSpinBox::valueChanged, slider, &QSlider::setValue);
        connect(slider, &QSlider::valueChanged, this, &ColorPanel::onSliderChanged);
    }

    connect(m_swatch, &ColorSwatchWidget::foregroundChanged,
            this, &ColorPanel::onSwatchForegroundChanged);
    connect(m_swatch, &ColorSwatchWidget::backgroundChanged, this,
            [this](const QColor &c) {
                if (m_engine) {
                    m_engine->setBackgroundColor(c);
                }
            });

    syncSlidersTo(m_swatch->foreground());
}

void ColorPanel::syncSlidersTo(const QColor &color)
{
    m_updating = true;
    m_red->setValue(color.red());
    m_green->setValue(color.green());
    m_blue->setValue(color.blue());
    m_hex->setText(color.name().toUpper());
    m_updating = false;
}

void ColorPanel::pushToEngine(const QColor &color)
{
    if (m_engine) {
        m_engine->setForegroundColor(color);
    }
    emit foregroundChanged(color);
}

void ColorPanel::onSliderChanged()
{
    if (m_updating) {
        return;
    }
    const QColor color(m_red->value(), m_green->value(), m_blue->value());
    m_hex->setText(color.name().toUpper());

    // Update the swatch without letting it echo back into the sliders.
    m_updating = true;
    m_swatch->setForeground(color);
    m_updating = false;

    pushToEngine(color);
}

void ColorPanel::onSwatchForegroundChanged(const QColor &color)
{
    if (!m_updating) {
        syncSlidersTo(color);
    }
    pushToEngine(color);
}

void ColorPanel::setForegroundColor(const QColor &color)
{
    if (!color.isValid()) {
        return;
    }
    m_updating = true;
    m_swatch->setForeground(color);
    m_updating = false;
    syncSlidersTo(color);
    if (m_engine) {
        m_engine->setForegroundColor(color);
    }
}

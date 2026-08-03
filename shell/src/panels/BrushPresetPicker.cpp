#include "BrushPresetPicker.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QGridLayout>
#include <QLabel>
#include <QListWidget>
#include <QPainter>
#include <QScreen>
#include <QSlider>
#include <QSpinBox>
#include <QVBoxLayout>

namespace {

/// Side of one preset cell, in pixels.
constexpr int kCellSize = 32;

using Preset = BrushPresetPicker::Preset;

/// CS6's default brush set, in its order.
///
/// Each row of the original picker is a family, and the number printed under a
/// thumbnail is its diameter. The fields after hardness are what turn a round
/// tip into the rest: roundness and angle give the chisel and flat brushes,
/// scatter and count give spatter and grass, and the jitters give chalk and
/// charcoal their broken edges.
///
/// The truly bitmap-based tips (oil, texture comb) are approximated here with
/// scatter and jitter rather than omitted — an approximation that paints is more
/// use than an empty slot, and the numbers still match CS6's.
const Preset kPresets[] = {
    // Soft round — the blurred discs the picker opens with.
    {"Soft Round 5",       5,   0, 100,   0,   0,  1,  0,   0,  0, 25},
    {"Soft Round 9",       9,   0, 100,   0,   0,  1,  0,   0,  0, 25},
    {"Soft Round 13",     13,   0, 100,   0,   0,  1,  0,   0,  0, 25},
    {"Soft Round 17",     17,   0, 100,   0,   0,  1,  0,   0,  0, 25},
    {"Soft Round 21",     21,   0, 100,   0,   0,  1,  0,   0,  0, 25},
    {"Soft Round 27",     27,   0, 100,   0,   0,  1,  0,   0,  0, 25},

    // Hard round.
    {"Hard Round 1",       1, 100, 100,   0,   0,  1,  0,   0,  0, 25},
    {"Hard Round 3",       3, 100, 100,   0,   0,  1,  0,   0,  0, 25},
    {"Hard Round 5",       5, 100, 100,   0,   0,  1,  0,   0,  0, 25},
    {"Hard Round 9",       9, 100, 100,   0,   0,  1,  0,   0,  0, 25},
    {"Hard Round 13",     13, 100, 100,   0,   0,  1,  0,   0,  0, 25},
    {"Hard Round 19",     19, 100, 100,   0,   0,  1,  0,   0,  0, 25},

    // Flat and chisel tips: a squashed, rotated ellipse. These are the small
    // wedge-shaped thumbnails across CS6's upper rows.
    {"Flat 14",           14,  90,  20,   0,   0,  1,  0,   0,  0, 15},
    {"Flat Angled 25",    25,  90,  22,  45,   0,  1,  0,   0,  0, 15},
    {"Flat Angled 50",    50,  90,  20, 135,   0,  1,  0,   0,  0, 15},
    {"Chisel 25",         25,  85,  30, 315,   0,  1,  0,   0,  0, 15},
    {"Chisel 36",         36,  85,  25,  20,   0,  1,  0,   0,  0, 15},
    {"Chisel Hard 30",    30, 100,  18, 300,   0,  1,  0,   0,  0, 12},

    // Charcoal and chalk: broad tips with the edge broken up by jitter.
    {"Charcoal 9",         9,  70,  60,  40,  25,  2, 40,  25, 25, 20},
    {"Chalk 23",          23,  60,  55,  20,  30,  2, 45,  30, 30, 22},
    {"Chalk 36",          36,  55,  50, 330,  35,  3, 50,  35, 30, 25},
    {"Charcoal 46",       46,  50,  45,  15,  40,  3, 55,  40, 35, 25},
    {"Chalk 59",          59,  45,  50, 200,  40,  3, 55,  45, 35, 28},
    {"Charcoal 60",       60,  40,  40, 100,  45,  4, 60,  50, 40, 30},

    // Spatter: small dabs thrown well off the path.
    {"Spatter 14",        14,  85, 100,   0, 140,  6, 60,   0,  0, 40},
    {"Spatter 24",        24,  80, 100,   0, 150,  7, 65,   0,  0, 45},
    {"Spatter 27",        27,  80,  85,   0, 160,  8, 70,  90, 20, 45},
    {"Spatter 39",        39,  75, 100,   0, 170,  9, 70,   0,  0, 50},
    {"Spatter 45",        45,  70,  90,   0, 180, 10, 75,  90, 25, 55},
    {"Spatter 59",        59,  70, 100,   0, 190, 11, 75,   0,  0, 60},

    // Star and rough: heavy angle jitter on a flattened tip, which reads as a
    // burst rather than a dot.
    {"Star 33",           33,  90,  22,   0,  60,  8, 45, 180, 30, 40},
    {"Star 74",           74,  85,  20,   0,  70, 10, 50, 180, 35, 45},
    {"Rough Round 42",    42,  60,  70,   0,  70,  5, 60,  90, 40, 35},
    {"Rough Round 55",    55,  55,  65,   0,  80,  6, 65,  90, 45, 38},

    // Grass and foliage: tall thin tips, scattered, with strong size variation.
    {"Grass 63",          63,  85,  25,  90, 100,  8, 70,  45, 30, 45},
    {"Dune Grass 112",   112,  80,  22,  90, 120, 10, 75,  50, 35, 50},
    {"Dune Grass 134",   134,  80,  20,  90, 130, 11, 80,  55, 35, 55},
    {"Scattered Leaves 95", 95, 70,  55,   0, 110,  9, 70, 180, 40, 50},

    // Airbrush-like soft, and the largest round tips CS6 lists.
    {"Soft Round 48",     48,  10, 100,   0,   0,  1,  0,   0,  0, 20},
    {"Soft Round 66",     66,   5, 100,   0,   0,  1,  0,   0,  0, 20},
    {"Soft Round 90",     90,   0, 100,   0,   0,  1,  0,   0,  0, 20},
    {"Hard Round 100",   100, 100, 100,   0,   0,  1,  0,   0,  0, 25},
    {"Soft Round 200",   200,   0, 100,   0,   0,  1,  0,   0,  0, 25},
    {"Soft Round 300",   300,   0, 100,   0,   0,  1,  0,   0,  0, 25},
};

/// The tip a fresh session starts on.
const Preset kDefaultPreset{"Hard Round 20", 20, 100, 100, 0, 0, 1, 0, 0, 0, 25};

} // namespace

BrushPresetPicker::BrushPresetPicker(Engine *engine, QWidget *parent)
    : QWidget(parent, Qt::Popup)
    , m_engine(engine)
    , m_current(kDefaultPreset)
{
    setObjectName(QStringLiteral("brushPicker"));

    auto *outer = new QVBoxLayout(this);
    outer->setContentsMargins(8, 8, 8, 8);
    outer->setSpacing(8);

    // -- top block: preview, then the two sliders ---------------------------
    auto *top = new QGridLayout;
    top->setHorizontalSpacing(10);
    top->setVerticalSpacing(4);

    m_preview = new QLabel(this);
    m_preview->setObjectName(QStringLiteral("brushPickerPreview"));
    m_preview->setFixedSize(64, 64);
    m_preview->setAlignment(Qt::AlignCenter);
    top->addWidget(m_preview, 0, 0, 4, 1);

    const auto addRow = [&](int row, const QString &label, int min, int max,
                            const QString &suffix, QSlider **slider, QSpinBox **value) {
        top->addWidget(new QLabel(label, this), row, 1);

        *value = new QSpinBox(this);
        (*value)->setRange(min, max);
        (*value)->setSuffix(suffix);
        (*value)->setFixedWidth(74);
        top->addWidget(*value, row, 2);

        *slider = new QSlider(Qt::Horizontal, this);
        (*slider)->setRange(min, max);
        top->addWidget(*slider, row + 1, 1, 1, 2);
    };

    addRow(0, tr("Size:"), 1, 5000, tr(" px"), &m_sizeSlider, &m_sizeValue);
    addRow(2, tr("Hardness:"), 0, 100, QStringLiteral("%"), &m_hardnessSlider,
           &m_hardnessValue);
    top->setColumnStretch(1, 1);
    outer->addLayout(top);

    // -- the current tip, named as CS6 names it -----------------------------
    m_currentLabel = new QLabel(this);
    m_currentLabel->setObjectName(QStringLiteral("brushPickerCurrent"));
    outer->addWidget(m_currentLabel);

    // -- the preset grid ----------------------------------------------------
    m_presets = new QListWidget(this);
    m_presets->setObjectName(QStringLiteral("brushPickerGrid"));
    m_presets->setViewMode(QListView::IconMode);
    m_presets->setMovement(QListView::Static);
    m_presets->setResizeMode(QListView::Adjust);
    m_presets->setUniformItemSizes(true);
    m_presets->setIconSize(QSize(kCellSize, kCellSize));
    m_presets->setGridSize(QSize(kCellSize + 8, kCellSize + 8));
    m_presets->setSelectionMode(QAbstractItemView::SingleSelection);
    m_presets->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    m_presets->setFixedHeight(7 * (kCellSize + 8) + 8);
    m_presets->setMinimumWidth(6 * (kCellSize + 8) + 24);
    buildPresetGrid();
    outer->addWidget(m_presets);

    // -- wiring -------------------------------------------------------------
    // Size and hardness edit the current tip and keep everything else about it,
    // which is how CS6 behaves: nudging Size does not turn a spatter brush back
    // into a plain circle.
    connect(m_sizeSlider, &QSlider::valueChanged, this, [this](int v) {
        if (!m_updating) {
            Preset next = m_current;
            next.size = v;
            apply(next, true);
        }
    });
    connect(m_sizeValue, &QSpinBox::valueChanged, this, [this](int v) {
        if (!m_updating) {
            Preset next = m_current;
            next.size = v;
            apply(next, true);
        }
    });
    connect(m_hardnessSlider, &QSlider::valueChanged, this, [this](int v) {
        if (!m_updating) {
            Preset next = m_current;
            next.hardness = v;
            apply(next, true);
        }
    });
    connect(m_hardnessValue, &QSpinBox::valueChanged, this, [this](int v) {
        if (!m_updating) {
            Preset next = m_current;
            next.hardness = v;
            apply(next, true);
        }
    });
    connect(m_presets, &QListWidget::itemClicked, this, [this](QListWidgetItem *item) {
        const int index = item->data(Qt::UserRole).toInt();
        if (index >= 0 && index < int(std::size(kPresets))) {
            apply(kPresets[index], true);
        }
    });

    apply(m_current, false);
}

void BrushPresetPicker::pushToEngine(const Preset &preset) const
{
    if (!m_engine) {
        return;
    }
    m_engine->setBrush(float(preset.size), preset.hardness, 100, 100, preset.spacing);
    m_engine->setBrushShape(preset.roundness, preset.angle, preset.scatter, preset.count,
                            preset.sizeJitter, preset.angleJitter, preset.roundnessJitter);
}

void BrushPresetPicker::buildPresetGrid()
{
    if (!m_engine) {
        return;
    }
    // Remember the brush so building thumbnails does not leave the engine set to
    // whatever the last preset was.
    const Preset restore = m_current;

    for (int i = 0; i < int(std::size(kPresets)); ++i) {
        const Preset &preset = kPresets[i];
        pushToEngine(preset);

        QPixmap pm(kCellSize, kCellSize);
        pm.fill(Qt::transparent);
        {
            // The engine lays one step of the brush; that image *is* the
            // thumbnail, so what is shown is what gets painted.
            const QImage tip = m_engine->brushPreview(kCellSize, kCellSize - 8);
            QPainter painter(&pm);
            painter.drawImage(0, 0, tip);

            QFont small = painter.font();
            small.setPixelSize(8);
            painter.setFont(small);
            painter.setPen(QColor(0xc0, 0xc0, 0xc0));
            painter.drawText(QRect(0, kCellSize - 10, kCellSize, 10),
                             Qt::AlignHCenter | Qt::AlignBottom,
                             QString::number(int(preset.size)));
        }

        auto *item = new QListWidgetItem(QIcon(pm), QString(), m_presets);
        item->setToolTip(tr("%1 — %2 px").arg(QString::fromUtf8(preset.name))
                             .arg(int(preset.size)));
        item->setData(Qt::UserRole, i);
        item->setSizeHint(QSize(kCellSize + 4, kCellSize + 4));
    }

    pushToEngine(restore);
}

QPixmap BrushPresetPicker::tipPreview(int edge)
{
    QPixmap pm(edge, edge);
    pm.fill(Qt::transparent);
    if (!m_engine) {
        return pm;
    }
    pushToEngine(m_current);
    QPainter painter(&pm);
    painter.drawImage(0, 0, m_engine->brushPreview(edge, edge));
    return pm;
}

void BrushPresetPicker::refreshPreview()
{
    if (!m_engine) {
        return;
    }
    const int edge = m_preview->width();
    QPixmap pm(edge, edge);
    pm.fill(Qt::transparent);
    {
        QPainter painter(&pm);
        painter.drawImage(0, 0, m_engine->brushPreview(edge, edge));

        // The crosshair CS6 draws over the preview, marking the tip's centre.
        painter.setPen(QPen(QColor(0x80, 0x80, 0x80), 1));
        painter.drawLine(edge / 2, 2, edge / 2, edge - 2);
        painter.drawLine(2, edge / 2, edge - 2, edge / 2);
    }
    m_preview->setPixmap(pm);
}

void BrushPresetPicker::apply(const Preset &preset, bool announce)
{
    m_current = preset;
    m_current.size = qBound(1.0, m_current.size, 5000.0);
    m_current.hardness = qBound(0, m_current.hardness, 100);

    m_updating = true;
    m_sizeSlider->setValue(int(m_current.size));
    m_sizeValue->setValue(int(m_current.size));
    m_hardnessSlider->setValue(m_current.hardness);
    m_hardnessValue->setValue(m_current.hardness);
    m_updating = false;

    m_currentLabel->setText(QStringLiteral("  %1").arg(QString::fromUtf8(m_current.name)));
    // The preview reads the engine's brush, so point it at this tip first.
    pushToEngine(m_current);
    refreshPreview();

    if (announce) {
        emit tipChanged(m_current);
    }
}

void BrushPresetPicker::setValues(double size, int hardness)
{
    Preset next = m_current;
    next.size = size;
    next.hardness = hardness;
    apply(next, false);
}

void BrushPresetPicker::popUpUnder(QWidget *anchor)
{
    if (!anchor) {
        show();
        return;
    }
    adjustSize();
    QPoint where = anchor->mapToGlobal(QPoint(0, anchor->height()));

    // Keep it on screen: a button near the right edge would otherwise open the
    // panel half off it.
    if (const QScreen *screen = anchor->screen()) {
        const QRect available = screen->availableGeometry();
        where.setX(qBound(available.left(), where.x(), available.right() - width()));
        if (where.y() + height() > available.bottom()) {
            where.setY(anchor->mapToGlobal(QPoint(0, 0)).y() - height());
        }
    }
    move(where);
    show();
}

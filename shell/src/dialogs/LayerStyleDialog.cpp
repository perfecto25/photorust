#include "LayerStyleDialog.h"

#include "AngleDial.h"
#include "ColorPickerDialog.h"
#include "photorust_core/src/bridge.cxxqt.h"

#include <QCheckBox>
#include <QComboBox>
#include <QMouseEvent>
#include <QPainter>
#include <QPolygon>
#include <array>
#include <limits>
#include <QtMath>
#include <QDoubleSpinBox>
#include <QFormLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QListWidget>
#include <QPushButton>
#include <QRadioButton>
#include <QSlider>
#include <QStackedWidget>
#include <QVBoxLayout>

#include <algorithm>

/// CS6's **Blend If** ramp: a gradient bar with a handle at each end, either of
/// which splits in two.
///
/// The four values are the dark handle's start and end and the light handle's
/// start and end. Unsplit, a handle's halves sit together and the boundary is a
/// cliff; dragged apart with Alt — as in Photoshop — the gap between them is a
/// ramp, which is what stops a gated layer showing a hard edge.
class BlendIfSlider : public QWidget
{
    Q_OBJECT
public:
    explicit BlendIfSlider(QWidget *parent = nullptr)
        : QWidget(parent)
    {
        setFixedHeight(34);
        setMinimumWidth(220);
        setCursor(Qt::SizeHorCursor);
    }

    void setValues(int darkStart, int darkEnd, int lightStart, int lightEnd)
    {
        m_values = {darkStart, darkEnd, lightStart, lightEnd};
        update();
    }

signals:
    /// One handle half moved: its index in the four, and its new value.
    void handleMoved(int index, int value);

protected:
    void paintEvent(QPaintEvent *) override
    {
        QPainter painter(this);
        painter.setRenderHint(QPainter::Antialiasing, true);

        const QRect bar = barRect();
        QLinearGradient ramp(bar.topLeft(), bar.topRight());
        ramp.setColorAt(0.0, Qt::black);
        ramp.setColorAt(1.0, Qt::white);
        painter.setPen(QPen(QColor(0x88, 0x88, 0x88), 1.0));
        painter.setBrush(ramp);
        painter.drawRect(bar);

        for (int i = 0; i < 4; ++i) {
            paintHandle(&painter, m_values[i], i < 2);
        }
    }

    void mousePressEvent(QMouseEvent *event) override
    {
        m_grabbed = nearestHandle(event->position().x());
        // Photoshop splits a handle with Alt; without it the pair travels
        // together, which is what most adjustments want.
        m_splitting = event->modifiers().testFlag(Qt::AltModifier);
        drag(event->position().x());
    }

    void mouseMoveEvent(QMouseEvent *event) override
    {
        if (m_grabbed >= 0) {
            drag(event->position().x());
        }
    }

    void mouseReleaseEvent(QMouseEvent *) override { m_grabbed = -1; }

private:
    QRect barRect() const { return QRect(6, 6, width() - 12, 14); }

    int valueAt(double x) const
    {
        const QRect bar = barRect();
        const double t = (x - bar.left()) / std::max(1, bar.width());
        return std::clamp(int(t * 255.0 + 0.5), 0, 255);
    }

    int positionOf(int value) const
    {
        const QRect bar = barRect();
        return bar.left() + int(bar.width() * value / 255.0);
    }

    int nearestHandle(double x) const
    {
        int best = 0;
        int distance = std::numeric_limits<int>::max();
        for (int i = 0; i < 4; ++i) {
            const int d = std::abs(positionOf(m_values[i]) - int(x));
            if (d < distance) {
                distance = d;
                best = i;
            }
        }
        return best;
    }

    void drag(double x)
    {
        const int value = valueAt(x);
        emit handleMoved(m_grabbed, value);
        if (!m_splitting) {
            // The other half of the same handle comes along.
            emit handleMoved(m_grabbed < 2 ? 1 - m_grabbed : 5 - m_grabbed, value);
        }
    }

    void paintHandle(QPainter *painter, int value, bool dark)
    {
        const QRect bar = barRect();
        const int x = positionOf(value);
        // Dark handles hang below the bar, light ones above, as CS6 draws them.
        const int top = dark ? bar.bottom() + 1 : bar.top() - 9;
        QPolygon arrow;
        arrow << QPoint(x, dark ? top : top + 8) << QPoint(x - 4, dark ? top + 8 : top)
              << QPoint(x + 4, dark ? top + 8 : top);
        painter->setPen(QPen(QColor(0x20, 0x20, 0x20), 1.0));
        painter->setBrush(dark ? QColor(0x30, 0x30, 0x30) : QColor(0xe0, 0xe0, 0xe0));
        painter->drawPolygon(arrow);
    }

    std::array<int, 4> m_values{0, 0, 255, 255};
    int m_grabbed = -1;
    bool m_splitting = false;
};


namespace {

/// A colour packs into one number as 0xRRGGBB — the form the engine keeps it
/// in, so the two sides agree without a second encoding.
float packColor(const QColor &c)
{
    return float((c.red() << 16) | (c.green() << 8) | c.blue());
}

QColor unpackColor(float packed)
{
    const int v = std::clamp(int(packed + 0.5f), 0, 0xffffff);
    return QColor((v >> 16) & 0xff, (v >> 8) & 0xff, v & 0xff);
}

void paintSwatch(QPushButton *button, const QColor &color)
{
    button->setStyleSheet(
        QStringLiteral("background-color: %1; border: 1px solid #000;").arg(color.name()));
}

} // namespace

LayerStyleDialog::LayerStyleDialog(Engine *engine, int layerIndex, const QString &effect,
                                   QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
    , m_layerIndex(layerIndex)
{
    setWindowTitle(tr("Layer Style"));
    resize(720, 520);

    // Everything below writes into the engine as it moves, so the way back is
    // the state as it stands now — taken here, before a single control exists.
    if (m_engine) {
        m_engine->beginLayerStyleEdit(m_layerIndex);
    }

    auto *outer = new QHBoxLayout(this);

    m_list = new QListWidget;
    m_list->setFixedWidth(190);
    outer->addWidget(m_list);

    auto *right = new QVBoxLayout;
    m_pages = new QStackedWidget;
    right->addWidget(m_pages, 1);

    auto *buttons = new QHBoxLayout;
    buttons->addStretch();
    auto *ok = new QPushButton(tr("OK"));
    ok->setDefault(true);
    ok->setFixedWidth(90);
    auto *cancel = new QPushButton(tr("Cancel"));
    cancel->setFixedWidth(90);
    buttons->addWidget(ok);
    buttons->addWidget(cancel);
    right->addLayout(buttons);
    outer->addLayout(right, 1);

    // CS6's order down the list. Blending Options and the three effects the
    // engine cannot draw yet are shown greyed, so the list keeps its shape
    // rather than quietly omitting half of Photoshop's.
    addFixedPage(tr("Blending Options"), buildBlendingOptionsPage());
    addEffect(QStringLiteral("bevel"), tr("Bevel && Emboss"), buildBevelPage());
    addEffect(QStringLiteral("stroke"), tr("Stroke"), buildStrokePage());
    addEffect(QStringLiteral("innerShadow"), tr("Inner Shadow"),
              buildShadowPage(QStringLiteral("innerShadow"), true));
    addEffect(QStringLiteral("innerGlow"), tr("Inner Glow"),
              buildGlowPage(QStringLiteral("innerGlow")));
    addEffect(QStringLiteral("satin"), tr("Satin"), buildSatinPage());
    addEffect(QStringLiteral("colorOverlay"), tr("Color Overlay"), buildColorOverlayPage());
    addEffect(QStringLiteral("gradientOverlay"), tr("Gradient Overlay"),
              buildGradientOverlayPage());
    addEffect(QStringLiteral("patternOverlay"), tr("Pattern Overlay"),
              buildPatternOverlayPage());
    addEffect(QStringLiteral("outerGlow"), tr("Outer Glow"),
              buildGlowPage(QStringLiteral("outerGlow")));
    addEffect(QStringLiteral("dropShadow"), tr("Drop Shadow"),
              buildShadowPage(QStringLiteral("dropShadow"), false));

    connect(m_list, &QListWidget::currentRowChanged, this, [this](int row) {
        if (row >= 0 && row < m_pages->count()) {
            m_pages->setCurrentIndex(row);
        }
    });
    connect(m_list, &QListWidget::itemChanged, this, [this] { onListChanged(); });

    connect(ok, &QPushButton::clicked, this, [this] {
        // One history step for the whole visit, however many sliders moved.
        if (m_engine) {
            m_engine->commitLayerEffects();
        }
        accept();
    });
    connect(cancel, &QPushButton::clicked, this, &QDialog::reject);

    // Opening from a menu entry lands on that effect's page and switches it
    // on, which is what picking "Drop Shadow..." is asking for.
    // An empty key is the "Effects" heading, which lands on the first page —
    // Blending Options, as it does in CS6.
    const int row = m_effectKeys.indexOf(effect);
    if (row >= 0) {
        m_list->setCurrentRow(row);
        if (!effect.isEmpty() && value(effect + QStringLiteral(".on")) < 0.5f) {
            setValue(effect + QStringLiteral(".on"), 1.0f);
            m_updating = true;
            m_list->item(row)->setCheckState(Qt::Checked);
            m_updating = false;
            previewChanged();
        }
    } else {
        m_list->setCurrentRow(m_effectKeys.isEmpty() ? 0 : 0);
    }
}

void LayerStyleDialog::addEffect(const QString &key, const QString &title, QWidget *page)
{
    auto *item = new QListWidgetItem(title, m_list);
    item->setFlags(item->flags() | Qt::ItemIsUserCheckable);
    item->setCheckState(value(key + QStringLiteral(".on")) >= 0.5f ? Qt::Checked
                                                                  : Qt::Unchecked);
    m_pages->addWidget(page);
    m_effectKeys.append(key);
}

void LayerStyleDialog::addFixedPage(const QString &title, QWidget *page)
{
    // Blending Options is not an effect: it is always there and has nothing to
    // switch on, so its row carries no checkbox.
    auto *item = new QListWidgetItem(title, m_list);
    item->setFlags(item->flags() & ~Qt::ItemIsUserCheckable);
    m_pages->addWidget(page);
    m_effectKeys.append(QString());
}

QWidget *LayerStyleDialog::buildBlendingOptionsPage()
{
    auto *page = new QWidget;
    auto *layout = new QVBoxLayout(page);

    // --- General Blending -------------------------------------------------
    auto *general = new QGroupBox(tr("General Blending"));
    auto *generalForm = new QFormLayout(general);
    auto *mode = new QComboBox;
    bindBlendMode(mode, QStringLiteral("blending.mode"));
    generalForm->addRow(tr("Blend Mode:"), mode);
    generalForm->addRow(tr("Opacity:"),
                        sliderRow(QStringLiteral("blending.opacity"), 0, 100, 100.0,
                                  QStringLiteral("%")));
    layout->addWidget(general);

    // --- Advanced Blending ------------------------------------------------
    auto *advanced = new QGroupBox(tr("Advanced Blending"));
    auto *advancedForm = new QFormLayout(advanced);
    advancedForm->addRow(tr("Fill Opacity:"),
                         sliderRow(QStringLiteral("blending.fillOpacity"), 0, 100, 100.0,
                                   QStringLiteral("%")));

    auto *channels = new QHBoxLayout;
    for (const auto &[label, key] :
         {std::make_pair(tr("R"), QStringLiteral("blending.channelR")),
          std::make_pair(tr("G"), QStringLiteral("blending.channelG")),
          std::make_pair(tr("B"), QStringLiteral("blending.channelB"))}) {
        auto *box = new QCheckBox(label);
        box->setToolTip(tr("Switch a channel off and the backdrop's own value shows "
                           "through it."));
        bindCheck(box, key);
        channels->addWidget(box);
    }
    channels->addStretch();
    advancedForm->addRow(tr("Channels:"), channels);

    auto *knockout = new QComboBox;
    knockout->addItems({tr("None"), tr("Shallow"), tr("Deep")});
    knockout->setEnabled(false);
    knockout->setToolTip(tr("Knockout punches through a group, and there are no "
                            "layer groups yet."));
    advancedForm->addRow(tr("Knockout:"), knockout);

    auto *shapes = new QCheckBox(tr("Transparency Shapes Layer"));
    shapes->setToolTip(tr("Off, the layer's effects fill its whole rectangle rather "
                          "than following what is drawn on it."));
    bindCheck(shapes, QStringLiteral("blending.transparencyShapes"));
    advancedForm->addRow(QString(), shapes);

    auto *maskHides = new QCheckBox(tr("Layer Mask Hides Effects"));
    bindCheck(maskHides, QStringLiteral("blending.maskHidesEffects"));
    advancedForm->addRow(QString(), maskHides);

    for (const QString &pending : {tr("Blend Interior Effects as Group"),
                                   tr("Blend Clipped Layers as Group"),
                                   tr("Vector Mask Hides Effects")}) {
        auto *box = new QCheckBox(pending);
        box->setEnabled(false);
        box->setToolTip(tr("Not implemented yet: this needs layer groups and vector "
                           "masks, and there are neither."));
        advancedForm->addRow(QString(), box);
    }
    layout->addWidget(advanced);

    // --- Blend If ---------------------------------------------------------
    auto *blendIf = new QGroupBox(tr("Blend If"));
    auto *blendIfForm = new QFormLayout(blendIf);

    auto *channel = new QComboBox;
    channel->addItems({tr("Gray"), tr("Red"), tr("Green"), tr("Blue")});
    bindChoice(channel, QStringLiteral("blending.blendIfChannel"));
    blendIfForm->addRow(tr("Blend If:"), channel);

    // The four keys behind each ramp, in the order the slider hands back.
    const auto ramp = [this, blendIfForm](const QString &label, const QString &prefix) {
        auto *slider = new BlendIfSlider;
        const QStringList keys = {prefix + QStringLiteral("DarkStart"),
                                  prefix + QStringLiteral("DarkEnd"),
                                  prefix + QStringLiteral("LightStart"),
                                  prefix + QStringLiteral("LightEnd")};
        slider->setValues(int(value(keys[0])), int(value(keys[1])), int(value(keys[2])),
                          int(value(keys[3])));
        connect(slider, &BlendIfSlider::handleMoved, this,
                [this, keys, slider](int index, int level) {
                    setValue(keys.at(index), float(level));
                    slider->setValues(int(value(keys[0])), int(value(keys[1])),
                                      int(value(keys[2])), int(value(keys[3])));
                    previewChanged();
                });
        blendIfForm->addRow(label, slider);
    };
    ramp(tr("This Layer:"), QStringLiteral("blending.this"));
    ramp(tr("Underlying Layer:"), QStringLiteral("blending.under"));
    layout->addWidget(blendIf);

    layout->addStretch();
    return page;
}

void LayerStyleDialog::addPendingEffect(const QString &title)
{
    auto *item = new QListWidgetItem(title, m_list);
    item->setFlags(item->flags() & ~(Qt::ItemIsEnabled | Qt::ItemIsUserCheckable));
    item->setToolTip(tr("Not implemented yet"));

    auto *page = new QWidget;
    auto *layout = new QVBoxLayout(page);
    layout->addStretch();
    auto *note = new QLabel(tr("%1 is not implemented yet.").arg(title));
    note->setAlignment(Qt::AlignCenter);
    layout->addWidget(note);
    layout->addStretch();
    m_pages->addWidget(page);
    // Keeps row and page indices in step; there is no key to tick.
    m_effectKeys.append(QString());
}

// ------------------------------------------------------------------- pages --

QWidget *LayerStyleDialog::buildBevelPage()
{
    auto *page = new QWidget;
    auto *layout = new QVBoxLayout(page);

    // --- Structure --------------------------------------------------------
    auto *structure = new QGroupBox(tr("Structure"));
    auto *form = new QFormLayout(structure);

    auto *style = new QComboBox;
    style->addItems({tr("Outer Bevel"), tr("Inner Bevel"), tr("Emboss"),
                     tr("Pillow Emboss"), tr("Stroke Emboss")});
    bindChoice(style, QStringLiteral("bevel.style"));
    style->setToolTip(tr("Stroke Emboss rides the Stroke effect, so it needs one."));
    form->addRow(tr("Style:"), style);

    auto *technique = new QComboBox;
    technique->addItems({tr("Smooth"), tr("Chisel Hard"), tr("Chisel Soft")});
    bindChoice(technique, QStringLiteral("bevel.technique"));
    form->addRow(tr("Technique:"), technique);

    form->addRow(tr("Depth:"),
                 sliderRow(QStringLiteral("bevel.depth"), 1, 1000, 100.0,
                           QStringLiteral("%")));

    // Direction is two radio buttons in CS6 rather than a menu, because it is
    // the one control people flip back and forth.
    auto *direction = new QWidget;
    auto *directionRow = new QHBoxLayout(direction);
    directionRow->setContentsMargins(0, 0, 0, 0);
    auto *up = new QRadioButton(tr("Up"));
    auto *down = new QRadioButton(tr("Down"));
    (value(QStringLiteral("bevel.up")) >= 0.5f ? up : down)->setChecked(true);
    directionRow->addWidget(up);
    directionRow->addWidget(down);
    directionRow->addStretch();
    connect(up, &QRadioButton::toggled, this, [this](bool on) {
        setValue(QStringLiteral("bevel.up"), on ? 1.0f : 0.0f);
        previewChanged();
    });
    form->addRow(tr("Direction:"), direction);

    form->addRow(tr("Size:"),
                 sliderRow(QStringLiteral("bevel.size"), 0, 250, 1.0, tr(" px")));
    form->addRow(tr("Soften:"),
                 sliderRow(QStringLiteral("bevel.soften"), 0, 16, 1.0, tr(" px")));
    layout->addWidget(structure);

    // --- Shading ----------------------------------------------------------
    auto *shading = new QGroupBox(tr("Shading"));
    auto *shadingForm = new QFormLayout(shading);

    shadingForm->addRow(tr("Angle:"), angleRow(QStringLiteral("bevel.angle")));

    auto *altitude = new QDoubleSpinBox;
    altitude->setRange(0, 90);
    altitude->setDecimals(0);
    altitude->setSuffix(QStringLiteral("°"));
    altitude->setToolTip(tr("How high the light sits. Straight overhead lights every "
                            "slope alike and the bevel disappears."));
    bindSpin(altitude, QStringLiteral("bevel.altitude"));
    shadingForm->addRow(tr("Altitude:"), altitude);

    const auto shadingRow = [this, shadingForm](const QString &label, const QString &mode,
                                                const QString &color) {
        auto *combo = new QComboBox;
        bindBlendMode(combo, mode);
        auto *swatch = new QPushButton;
        swatch->setFixedSize(40, 20);
        bindColor(swatch, color);
        auto *row = new QHBoxLayout;
        row->addWidget(combo, 1);
        row->addWidget(swatch);
        shadingForm->addRow(label, row);
    };
    shadingRow(tr("Highlight Mode:"), QStringLiteral("bevel.highlightMode"),
               QStringLiteral("bevel.highlightColor"));
    shadingForm->addRow(tr("Opacity:"),
                        sliderRow(QStringLiteral("bevel.highlightOpacity"), 0, 100, 100.0,
                                  QStringLiteral("%")));
    shadingRow(tr("Shadow Mode:"), QStringLiteral("bevel.shadowMode"),
               QStringLiteral("bevel.shadowColor"));
    shadingForm->addRow(tr("Opacity:"),
                        sliderRow(QStringLiteral("bevel.shadowOpacity"), 0, 100, 100.0,
                                  QStringLiteral("%")));
    layout->addWidget(shading);

    layout->addStretch();
    return page;
}

QWidget *LayerStyleDialog::buildShadowPage(const QString &key, bool inner)
{
    auto *page = new QWidget;
    auto *layout = new QVBoxLayout(page);

    auto *structure = new QGroupBox(tr("Structure"));
    auto *form = new QFormLayout(structure);

    auto *mode = new QComboBox;
    bindBlendMode(mode, key + QStringLiteral(".mode"));
    auto *swatch = new QPushButton;
    swatch->setFixedSize(40, 20);
    bindColor(swatch, key + QStringLiteral(".color"));
    auto *modeRow = new QHBoxLayout;
    modeRow->addWidget(mode, 1);
    modeRow->addWidget(swatch);
    form->addRow(tr("Blend Mode:"), modeRow);

    form->addRow(tr("Opacity:"),
                 sliderRow(key + QStringLiteral(".opacity"), 0, 100, 100.0,
                           QStringLiteral("%")));

    form->addRow(tr("Angle:"), angleRow(key + QStringLiteral(".angle")));

    form->addRow(tr("Distance:"),
                 sliderRow(key + QStringLiteral(".distance"), 0, 250, 1.0, tr(" px")));
    // CS6 calls the same slider Choke on an inner shadow, because it eats into
    // the shape rather than growing out of it.
    form->addRow(inner ? tr("Choke:") : tr("Spread:"),
                 sliderRow(key + QStringLiteral(".spread"), 0, 100, 100.0,
                           QStringLiteral("%")));
    form->addRow(tr("Size:"),
                 sliderRow(key + QStringLiteral(".size"), 0, 250, 1.0, tr(" px")));

    layout->addWidget(structure);
    layout->addStretch();
    return page;
}

QWidget *LayerStyleDialog::buildGlowPage(const QString &key)
{
    auto *page = new QWidget;
    auto *layout = new QVBoxLayout(page);

    auto *structure = new QGroupBox(tr("Structure"));
    auto *form = new QFormLayout(structure);

    auto *mode = new QComboBox;
    bindBlendMode(mode, key + QStringLiteral(".mode"));
    form->addRow(tr("Blend Mode:"), mode);

    form->addRow(tr("Opacity:"),
                 sliderRow(key + QStringLiteral(".opacity"), 0, 100, 100.0,
                           QStringLiteral("%")));

    auto *swatch = new QPushButton;
    swatch->setFixedSize(40, 20);
    bindColor(swatch, key + QStringLiteral(".color"));
    form->addRow(tr("Color:"), swatch);

    form->addRow(tr("Spread:"),
                 sliderRow(key + QStringLiteral(".spread"), 0, 100, 100.0,
                           QStringLiteral("%")));
    form->addRow(tr("Size:"),
                 sliderRow(key + QStringLiteral(".size"), 0, 250, 1.0, tr(" px")));

    layout->addWidget(structure);
    layout->addStretch();
    return page;
}

QWidget *LayerStyleDialog::buildSatinPage()
{
    auto *page = new QWidget;
    auto *layout = new QVBoxLayout(page);
    auto *box = new QGroupBox(tr("Structure"));
    auto *form = new QFormLayout(box);

    auto *mode = new QComboBox;
    bindBlendMode(mode, QStringLiteral("satin.mode"));
    auto *swatch = new QPushButton;
    swatch->setFixedSize(40, 20);
    bindColor(swatch, QStringLiteral("satin.color"));
    auto *modeRow = new QHBoxLayout;
    modeRow->addWidget(mode, 1);
    modeRow->addWidget(swatch);
    form->addRow(tr("Blend Mode:"), modeRow);

    form->addRow(tr("Opacity:"),
                 sliderRow(QStringLiteral("satin.opacity"), 0, 100, 100.0,
                           QStringLiteral("%")));

    form->addRow(tr("Angle:"), angleRow(QStringLiteral("satin.angle")));

    form->addRow(tr("Distance:"),
                 sliderRow(QStringLiteral("satin.distance"), 0, 250, 1.0, tr(" px")));
    form->addRow(tr("Size:"),
                 sliderRow(QStringLiteral("satin.size"), 0, 250, 1.0, tr(" px")));

    auto *invert = new QCheckBox(tr("Invert"));
    invert->setToolTip(tr("Swap the bands for the gaps between them."));
    bindCheck(invert, QStringLiteral("satin.invert"));
    form->addRow(QString(), invert);

    layout->addWidget(box);
    layout->addStretch();
    return page;
}

QWidget *LayerStyleDialog::buildColorOverlayPage()
{
    auto *page = new QWidget;
    auto *layout = new QVBoxLayout(page);
    auto *box = new QGroupBox(tr("Color"));
    auto *form = new QFormLayout(box);

    auto *mode = new QComboBox;
    bindBlendMode(mode, QStringLiteral("colorOverlay.mode"));
    auto *swatch = new QPushButton;
    swatch->setFixedSize(40, 20);
    bindColor(swatch, QStringLiteral("colorOverlay.color"));
    auto *modeRow = new QHBoxLayout;
    modeRow->addWidget(mode, 1);
    modeRow->addWidget(swatch);
    form->addRow(tr("Blend Mode:"), modeRow);

    form->addRow(tr("Opacity:"),
                 sliderRow(QStringLiteral("colorOverlay.opacity"), 0, 100, 100.0,
                           QStringLiteral("%")));

    layout->addWidget(box);
    layout->addStretch();
    return page;
}

QWidget *LayerStyleDialog::buildGradientOverlayPage()
{
    auto *page = new QWidget;
    auto *layout = new QVBoxLayout(page);
    auto *box = new QGroupBox(tr("Gradient"));
    auto *form = new QFormLayout(box);

    auto *mode = new QComboBox;
    bindBlendMode(mode, QStringLiteral("gradientOverlay.mode"));
    form->addRow(tr("Blend Mode:"), mode);

    form->addRow(tr("Opacity:"),
                 sliderRow(QStringLiteral("gradientOverlay.opacity"), 0, 100, 100.0,
                           QStringLiteral("%")));

    // Two stops rather than the full gradient editor: enough for the ramps a
    // layer style is usually asked for, and honest about what it is.
    auto *stops = new QHBoxLayout;
    auto *from = new QPushButton;
    from->setFixedSize(40, 20);
    bindColor(from, QStringLiteral("gradientOverlay.from"));
    auto *to = new QPushButton;
    to->setFixedSize(40, 20);
    bindColor(to, QStringLiteral("gradientOverlay.to"));
    stops->addWidget(from);
    stops->addWidget(to);
    stops->addStretch();
    form->addRow(tr("Gradient:"), stops);

    auto *toggles = new QHBoxLayout;
    auto *reverse = new QCheckBox(tr("Reverse"));
    bindCheck(reverse, QStringLiteral("gradientOverlay.reverse"));
    auto *dither = new QCheckBox(tr("Dither"));
    dither->setToolTip(tr("Break up the banding a smooth ramp shows over a large area."));
    bindCheck(dither, QStringLiteral("gradientOverlay.dither"));
    toggles->addWidget(reverse);
    toggles->addWidget(dither);
    toggles->addStretch();
    form->addRow(QString(), toggles);

    auto *shape = new QComboBox;
    shape->addItems({tr("Linear"), tr("Radial"), tr("Angle"), tr("Reflected"),
                     tr("Diamond")});
    bindChoice(shape, QStringLiteral("gradientOverlay.shape"));
    form->addRow(tr("Style:"), shape);

    form->addRow(tr("Angle:"), angleRow(QStringLiteral("gradientOverlay.angle")));
    form->addRow(tr("Scale:"),
                 sliderRow(QStringLiteral("gradientOverlay.scale"), 10, 400, 100.0,
                           QStringLiteral("%")));

    auto *align = new QCheckBox(tr("Align with Layer"));
    align->setToolTip(tr("Span the layer's own content rather than the whole canvas."));
    bindCheck(align, QStringLiteral("gradientOverlay.align"));
    form->addRow(QString(), align);

    layout->addWidget(box);
    layout->addStretch();
    return page;
}

QWidget *LayerStyleDialog::buildPatternOverlayPage()
{
    auto *page = new QWidget;
    auto *layout = new QVBoxLayout(page);
    auto *box = new QGroupBox(tr("Pattern"));
    auto *form = new QFormLayout(box);

    auto *mode = new QComboBox;
    bindBlendMode(mode, QStringLiteral("patternOverlay.mode"));
    form->addRow(tr("Blend Mode:"), mode);

    form->addRow(tr("Opacity:"),
                 sliderRow(QStringLiteral("patternOverlay.opacity"), 0, 100, 100.0,
                           QStringLiteral("%")));

    auto *pattern = new QComboBox;
    if (m_engine) {
        // The engine owns the set — they are generated tiles, not Photoshop's
        // artwork — so the list cannot drift from what gets drawn.
        pattern->addItems(
            m_engine->patternNames().split(QLatin1Char('\n'), Qt::SkipEmptyParts));
    }
    bindChoice(pattern, QStringLiteral("patternOverlay.pattern"));
    form->addRow(tr("Pattern:"), pattern);

    form->addRow(tr("Scale:"),
                 sliderRow(QStringLiteral("patternOverlay.scale"), 10, 1000, 100.0,
                           QStringLiteral("%")));

    auto *link = new QCheckBox(tr("Link with Layer"));
    link->setToolTip(tr("Anchor the tiling to the layer, so moving it takes the "
                        "pattern along."));
    bindCheck(link, QStringLiteral("patternOverlay.link"));
    form->addRow(QString(), link);

    layout->addWidget(box);
    layout->addStretch();
    return page;
}

QWidget *LayerStyleDialog::buildStrokePage()
{
    auto *page = new QWidget;
    auto *layout = new QVBoxLayout(page);
    auto *box = new QGroupBox(tr("Structure"));
    auto *form = new QFormLayout(box);

    // CS6 leads the Stroke page with Size on a slider, which is the control
    // people reach for first.
    form->addRow(tr("Size:"),
                 sliderRow(QStringLiteral("stroke.size"), 1, 250, 1.0, tr(" px")));

    auto *position = new QComboBox;
    position->addItem(tr("Outside"));
    position->addItem(tr("Inside"));
    position->addItem(tr("Center"));
    position->setCurrentIndex(int(value(QStringLiteral("stroke.position")) + 0.5f));
    connect(position, &QComboBox::currentIndexChanged, this, [this](int index) {
        setValue(QStringLiteral("stroke.position"), float(index));
        previewChanged();
    });
    form->addRow(tr("Position:"), position);

    auto *mode = new QComboBox;
    bindBlendMode(mode, QStringLiteral("stroke.mode"));
    form->addRow(tr("Blend Mode:"), mode);

    form->addRow(tr("Opacity:"),
                 sliderRow(QStringLiteral("stroke.opacity"), 0, 100, 100.0,
                           QStringLiteral("%")));

    auto *swatch = new QPushButton;
    swatch->setFixedSize(40, 20);
    bindColor(swatch, QStringLiteral("stroke.color"));
    form->addRow(tr("Color:"), swatch);

    layout->addWidget(box);
    layout->addStretch();
    return page;
}

// ------------------------------------------------------------------ wiring --

void LayerStyleDialog::bindCheck(QCheckBox *box, const QString &key)
{
    box->setChecked(value(key) >= 0.5f);
    connect(box, &QCheckBox::toggled, this, [this, key](bool on) {
        setValue(key, on ? 1.0f : 0.0f);
        previewChanged();
    });
}

void LayerStyleDialog::bindSpin(QDoubleSpinBox *spin, const QString &key, double scale)
{
    spin->setValue(double(value(key)) * scale);
    connect(spin, &QDoubleSpinBox::valueChanged, this, [this, key, scale](double v) {
        setValue(key, float(v / scale));
        previewChanged();
    });
}

QWidget *LayerStyleDialog::sliderRow(const QString &key, double min, double max,
                                     double scale, const QString &suffix)
{
    auto *row = new QWidget;
    auto *layout = new QHBoxLayout(row);
    layout->setContentsMargins(0, 0, 0, 0);

    auto *slider = new QSlider(Qt::Horizontal);
    slider->setRange(int(min), int(max));
    auto *spin = new QDoubleSpinBox;
    spin->setRange(min, max);
    spin->setDecimals(0);
    spin->setSuffix(suffix);
    spin->setFixedWidth(70);

    const double start = double(value(key)) * scale;
    slider->setValue(int(start + 0.5));
    spin->setValue(start);

    // The two show one number, so each moves the other — guarded, or setting
    // one would set the other would set the first.
    connect(slider, &QSlider::valueChanged, this, [this, key, scale, spin](int v) {
        if (m_updating) {
            return;
        }
        m_updating = true;
        spin->setValue(double(v));
        m_updating = false;
        setValue(key, float(double(v) / scale));
        previewChanged();
    });
    connect(spin, &QDoubleSpinBox::valueChanged, this, [this, key, scale, slider](double v) {
        if (m_updating) {
            return;
        }
        m_updating = true;
        slider->setValue(int(v + 0.5));
        m_updating = false;
        setValue(key, float(v / scale));
        previewChanged();
    });

    layout->addWidget(slider, 1);
    layout->addWidget(spin);
    return row;
}

void LayerStyleDialog::bindBlendMode(QComboBox *combo, const QString &key)
{
    if (!m_engine) {
        return;
    }
    // The engine owns the list and its order, so the combo cannot drift out of
    // sync with the BlendMode discriminants — as in the Layers panel.
    const QStringList names =
        m_engine->blendModeNames().split(QLatin1Char('\n'), Qt::SkipEmptyParts);
    combo->addItems(names);

    const int current = int(value(key) + 0.5f);
    if (current >= 0 && current < combo->count()) {
        combo->setCurrentIndex(current);
    }
    connect(combo, &QComboBox::currentIndexChanged, this, [this, key](int index) {
        setValue(key, float(index));
        previewChanged();
    });
}

QWidget *LayerStyleDialog::angleRow(const QString &key)
{
    auto *row = new QWidget;
    auto *layout = new QHBoxLayout(row);
    layout->setContentsMargins(0, 0, 0, 0);

    auto *dial = new AngleDial;
    auto *spin = new QDoubleSpinBox;
    spin->setRange(-360, 360);
    spin->setDecimals(0);
    spin->setSuffix(QStringLiteral("°"));
    spin->setFixedWidth(70);

    const double start = double(value(key));
    dial->setAngle(start);
    spin->setValue(start);

    // Two views of one number, so each moves the other — guarded, or setting
    // either would set the other would set the first.
    connect(dial, &AngleDial::angleChanged, this, [this, key, spin](double degrees) {
        if (m_updating) {
            return;
        }
        m_updating = true;
        spin->setValue(degrees);
        m_updating = false;
        setValue(key, float(degrees));
        previewChanged();
    });
    connect(spin, &QDoubleSpinBox::valueChanged, this, [this, key, dial](double degrees) {
        if (m_updating) {
            return;
        }
        m_updating = true;
        dial->setAngle(degrees);
        m_updating = false;
        setValue(key, float(degrees));
        previewChanged();
    });

    layout->addWidget(dial);
    layout->addWidget(spin);
    layout->addStretch();
    return row;
}

void LayerStyleDialog::bindChoice(QComboBox *combo, const QString &key)
{
    const int current = int(value(key) + 0.5f);
    if (current >= 0 && current < combo->count()) {
        combo->setCurrentIndex(current);
    }
    connect(combo, &QComboBox::currentIndexChanged, this, [this, key](int index) {
        setValue(key, float(index));
        previewChanged();
    });
}

void LayerStyleDialog::bindColor(QPushButton *button, const QString &key)
{
    paintSwatch(button, unpackColor(value(key)));
    connect(button, &QPushButton::clicked, this, [this, key, button] {
        const QColor picked =
            ColorPickerDialog::getColor(unpackColor(value(key)), this, tr("Color"));
        if (!picked.isValid()) {
            return;
        }
        setValue(key, packColor(picked));
        paintSwatch(button, picked);
        previewChanged();
    });
}

void LayerStyleDialog::onListChanged()
{
    if (m_updating || !m_engine) {
        return;
    }
    for (int row = 0; row < m_list->count(); ++row) {
        const QString key = m_effectKeys.value(row);
        if (key.isEmpty()) {
            continue;
        }
        const bool on = m_list->item(row)->checkState() == Qt::Checked;
        setValue(key + QStringLiteral(".on"), on ? 1.0f : 0.0f);
        // Clearing the box here takes the effect *off* the layer, so its row
        // leaves the Layers panel with it. That is the difference between this
        // control and the eye on the panel's own row: the eye switches an
        // effect off and keeps it, this removes it.
        setValue(key + QStringLiteral(".present"), on ? 1.0f : 0.0f);
    }
    previewChanged();
}

void LayerStyleDialog::setValue(const QString &key, float value)
{
    if (m_engine) {
        m_engine->setLayerEffectValue(m_layerIndex, key, value);
    }
}

float LayerStyleDialog::value(const QString &key) const
{
    return m_engine ? m_engine->layerEffectValue(m_layerIndex, key) : 0.0f;
}

void LayerStyleDialog::previewChanged()
{
    // The engine has already raised its change signals; this is only here so
    // the dialog does not have to know how the canvas gets repainted.
    if (auto *window = parentWidget()) {
        window->update();
    }
}

void LayerStyleDialog::reject()
{
    // Nothing was committed, so the document has no history entry to step back
    // to: the state itself is what has to go back, all of it at once.
    if (m_engine) {
        m_engine->cancelLayerStyleEdit();
    }
    previewChanged();
    QDialog::reject();
}

#include "LayerStyleDialog.moc"

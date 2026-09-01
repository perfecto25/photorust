#include "PropertiesPanel.h"

#include "LayerIcons.h"

#include "dialogs/ColorPickerDialog.h"
#include "dialogs/CurvesDialog.h"
#include "dialogs/HueSaturationDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <cmath>

#include <QCheckBox>
#include <QComboBox>
#include <QDoubleSpinBox>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QPainter>
#include <QScrollArea>
#include <QSignalBlocker>
#include <QStackedWidget>
#include <QTimer>
#include <QToolButton>
#include <QVBoxLayout>

namespace {

constexpr int kFooterGlyph = 18;
constexpr int kHeaderThumb = 22;

const QColor kGlyph(0xcf, 0xcf, 0xcf);
const QColor kDimText(0x9a, 0x9a, 0x9a);

/// The pages, in the order they are added to the stack.
enum Page {
    kEmptyPage = 0,
    kParametersPage,
    kLayerPage,
};

/// A gesture that is still going gets one commit, not one per tick. This is how
/// long the panel waits after a change that had no end of its own — a wheel
/// notch, an arrow key — before deciding the gesture is over.
constexpr int kCommitDelayMs = 500;

/// Display units to slider steps, for a control showing `decimals` of them.
double stepsPerUnit(int decimals)
{
    return std::pow(10.0, decimals);
}

QList<QColor> rainbowRamp()
{
    QList<QColor> stops;
    for (int i = 0; i <= 6; ++i) {
        stops << QColor::fromHsv((i * 60) % 360, 255, 255);
    }
    return stops;
}

} // namespace

// ---------------------------------------------------------------------------
// RampSlider
// ---------------------------------------------------------------------------

RampSlider::RampSlider(QWidget *parent)
    : QSlider(Qt::Horizontal, parent)
{
}

void RampSlider::setRamp(const QList<QColor> &stops)
{
    // Rebuilding a stylesheet makes the widget recalculate its rules, and the
    // hue ramp is asked for again on every tick of a drag — so nothing happens
    // unless the colours actually moved.
    if (stops == m_stops) {
        return;
    }
    m_stops = stops;

    if (stops.size() < 2) {
        setStyleSheet(QString());
        return;
    }

    QStringList gradient;
    for (int i = 0; i < stops.size(); ++i) {
        gradient << QStringLiteral("stop:%1 %2")
                        .arg(qreal(i) / (stops.size() - 1))
                        .arg(stops.at(i).name());
    }

    // Taller than the theme's 3px line, which is too thin to read a rainbow
    // off, and with the filled sub-page turned off — on a ramp there is no
    // "how far along" to shade, the colour is the information.
    setStyleSheet(QStringLiteral(
                      "QSlider::groove:horizontal {"
                      "  height: 7px; border: 1px solid #2a2a2a; border-radius: 0px;"
                      "  background: qlineargradient(x1:0, y1:0, x2:1, y2:0, %1); }"
                      "QSlider::sub-page:horizontal { background: transparent; }")
                      .arg(gradient.join(QStringLiteral(", "))));
}

// ---------------------------------------------------------------------------
// PropertiesPanel
// ---------------------------------------------------------------------------

PropertiesPanel::PropertiesPanel(Engine *engine, QWidget *parent)
    : QWidget(parent)
    , m_engine(engine)
{
    m_commitTimer = new QTimer(this);
    m_commitTimer->setSingleShot(true);
    m_commitTimer->setInterval(kCommitDelayMs);
    connect(m_commitTimer, &QTimer::timeout, this, &PropertiesPanel::commitEdit);

    buildUi();
    refresh();
}

void PropertiesPanel::buildUi()
{
    auto *root = new QVBoxLayout(this);
    root->setContentsMargins(0, 0, 0, 0);
    root->setSpacing(0);

    // The header names what is being edited, and carries the same two
    // thumbnails CS6 shows: the layer, then its mask if it has one.
    auto *header = new QWidget(this);
    auto *headerLayout = new QHBoxLayout(header);
    headerLayout->setContentsMargins(6, 5, 6, 5);
    headerLayout->setSpacing(6);
    m_headerIcon = new QLabel(header);
    m_headerIcon->setFixedSize(kHeaderThumb, kHeaderThumb);
    m_headerMask = new QLabel(header);
    m_headerMask->setFixedSize(kHeaderThumb, kHeaderThumb);
    m_headerTitle = new QLabel(header);
    headerLayout->addWidget(m_headerIcon);
    headerLayout->addWidget(m_headerMask);
    headerLayout->addWidget(m_headerTitle, 1);
    root->addWidget(header);

    m_stack = new QStackedWidget(this);
    m_stack->addWidget(buildEmptyPage());
    m_stack->addWidget(buildParametersPage());
    m_stack->addWidget(buildLayerPage());
    root->addWidget(m_stack, 1);

    root->addWidget(buildFooter());
}

QWidget *PropertiesPanel::buildEmptyPage()
{
    auto *page = new QWidget(this);
    auto *layout = new QVBoxLayout(page);
    auto *label = new QLabel(tr("No properties."), page);
    label->setAlignment(Qt::AlignCenter);
    label->setStyleSheet(QStringLiteral("color: %1;").arg(kDimText.name()));
    layout->addWidget(label);
    return page;
}

QWidget *PropertiesPanel::buildParametersPage()
{
    m_parametersPage = new QWidget(this);
    m_parameterLayout = new QVBoxLayout(m_parametersPage);
    m_parameterLayout->setContentsMargins(8, 4, 8, 4);
    m_parameterLayout->setSpacing(3);

    // Levels and the Channel Mixer are taller than a narrow dock, so the
    // controls scroll rather than being squashed to nothing.
    auto *scroll = new QScrollArea(this);
    scroll->setWidget(m_parametersPage);
    scroll->setWidgetResizable(true);
    scroll->setFrameShape(QFrame::NoFrame);
    scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    return scroll;
}

QWidget *PropertiesPanel::buildLayerPage()
{
    auto *page = new QWidget(this);
    auto *layout = new QVBoxLayout(page);
    layout->setContentsMargins(8, 4, 8, 4);
    layout->setSpacing(4);

    auto *form = new QFormLayout;
    form->setContentsMargins(0, 0, 0, 0);
    form->setHorizontalSpacing(10);
    form->setVerticalSpacing(3);
    form->setLabelAlignment(Qt::AlignRight | Qt::AlignVCenter);

    auto addRow = [&](const QString &title, QLabel **value) {
        *value = new QLabel(page);
        auto *label = new QLabel(title, page);
        label->setStyleSheet(QStringLiteral("color: %1;").arg(kDimText.name()));
        form->addRow(label, *value);
    };

    addRow(tr("Kind:"), &m_kindValue);
    addRow(tr("Size:"), &m_sizeValue);
    addRow(tr("Position:"), &m_positionValue);
    addRow(tr("Blend:"), &m_blendValue);
    addRow(tr("Opacity:"), &m_opacityValue);
    addRow(tr("Fill:"), &m_fillValue);
    addRow(tr("Mask:"), &m_maskValue);
    addRow(tr("Locks:"), &m_lockValue);
    layout->addLayout(form);

    layout->addStretch(1);
    return page;
}

QWidget *PropertiesPanel::buildFooter()
{
    auto *footer = new QWidget(this);
    footer->setObjectName(QStringLiteral("panelFooter"));
    auto *layout = new QHBoxLayout(footer);
    layout->setContentsMargins(4, 2, 4, 2);
    layout->setSpacing(2);

    auto makeButton = [&](LayerIcons::Glyph glyph, const QString &tip) {
        auto *button = new QToolButton(footer);
        button->setIconSize(QSize(kFooterGlyph, kFooterGlyph));
        button->setIcon(LayerIcons::icon(glyph, kGlyph, kFooterGlyph));
        button->setToolTip(tip);
        button->setAutoRaise(true);
        layout->addWidget(button);
        return button;
    };

    m_clipButton = makeButton(LayerIcons::Glyph::ClipToLayer, tr("Clip to layer"));
    m_clipButton->setCheckable(true);
    m_resetButton = makeButton(LayerIcons::Glyph::Reset,
                               tr("Reset to adjustment defaults"));
    layout->addStretch(1);
    m_visibleButton = makeButton(LayerIcons::Glyph::Eye, tr("Toggle layer visibility"));
    m_visibleButton->setCheckable(true);
    m_deleteButton = makeButton(LayerIcons::Glyph::Delete, tr("Delete this layer"));

    connect(m_clipButton, &QToolButton::clicked, this, [this](bool on) {
        if (m_engine && m_layer >= 0) {
            m_engine->setLayerClipping(m_layer, on);
        }
    });
    connect(m_resetButton, &QToolButton::clicked, this, &PropertiesPanel::resetAdjustment);
    connect(m_visibleButton, &QToolButton::clicked, this, [this](bool on) {
        if (m_engine && m_layer >= 0) {
            m_engine->setLayerVisible(m_layer, on);
        }
    });
    connect(m_deleteButton, &QToolButton::clicked, this, [this] {
        if (!m_engine || m_layer < 0 || m_engine->getLayerCount() < 2) {
            return;
        }
        commitEdit();
        m_engine->deleteLayer(m_layer);
        emit documentChanged();
    });

    return footer;
}

// ------------------------------------------------------------------ state --

void PropertiesPanel::refresh()
{
    // Our own writes come straight back as change signals. Reloading on those
    // would drag the controls out from under the user mid-gesture.
    if (m_applying || !m_engine) {
        return;
    }

    // Getting here means something outside this panel changed the document,
    // which ends any gesture in progress rather than abandoning it: the change
    // is already on the canvas, so it belongs in the history too. It also stops
    // a session outliving the layer — or the document — it was opened on.
    commitEdit();
    m_layer = m_engine->getActiveLayerIndex();

    if (m_layer < 0 || m_layer >= m_engine->getLayerCount()) {
        m_layer = -1;
        m_headerIcon->clear();
        m_headerMask->hide();
        m_headerTitle->clear();
        m_stack->setCurrentIndex(kEmptyPage);
        m_clipButton->setEnabled(false);
        m_resetButton->setEnabled(false);
        m_visibleButton->setEnabled(false);
        m_deleteButton->setEnabled(false);
        return;
    }

    const QString adjustment = m_engine->layerAdjustmentName(m_layer);

    m_headerIcon->setPixmap(
        QPixmap::fromImage(m_engine->layerThumbnail(m_layer, kHeaderThumb)));
    if (m_engine->layerHasMask(m_layer)) {
        m_headerMask->setPixmap(
            QPixmap::fromImage(m_engine->layerMaskThumbnail(m_layer, kHeaderThumb)));
        m_headerMask->show();
    } else {
        m_headerMask->hide();
    }
    // Colorize is Hue/Saturation with its box ticked, not an adjustment of its
    // own — the header says what the user chose from the menu.
    m_headerTitle->setText(adjustment == QLatin1String("Colorize")
                               ? tr("Hue/Saturation")
                               : (adjustment.isEmpty() ? m_engine->layerName(m_layer)
                                                       : adjustment));

    m_clipButton->setEnabled(m_layer + 1 < m_engine->getLayerCount());
    m_clipButton->setChecked(m_engine->layerIsClipping(m_layer));
    m_resetButton->setEnabled(!adjustment.isEmpty());
    m_visibleButton->setEnabled(true);
    m_visibleButton->setChecked(m_engine->layerVisible(m_layer));
    m_deleteButton->setEnabled(m_engine->getLayerCount() > 1);

    if (adjustment.isEmpty()) {
        m_builtFor.clear();
        loadLayerProperties();
        m_stack->setCurrentIndex(kLayerPage);
        return;
    }

    if (adjustment != m_builtFor) {
        buildParameters(adjustment);
    }
    loadParameters();
    m_stack->setCurrentIndex(kParametersPage);
}

void PropertiesPanel::loadLayerProperties()
{
    const QString kind = m_engine->layerKindName(m_layer);
    m_kindValue->setText(tr("%1 layer").arg(kind));

    const QRect bounds = m_engine->layerContentBounds(m_layer);
    if (bounds.isEmpty()) {
        // A fill layer has no pixels of its own to measure, and an empty
        // raster layer has none yet.
        m_sizeValue->setText(tr("—"));
        m_positionValue->setText(tr("—"));
    } else {
        m_sizeValue->setText(tr("%1 × %2 px").arg(bounds.width()).arg(bounds.height()));
        m_positionValue->setText(QStringLiteral("%1, %2")
                                     .arg(m_engine->layerOffsetX(m_layer))
                                     .arg(m_engine->layerOffsetY(m_layer)));
    }

    const QStringList modes =
        m_engine->blendModeNames().split(QLatin1Char('\n'), Qt::SkipEmptyParts);
    const int mode = m_engine->layerBlendMode(m_layer);
    m_blendValue->setText(mode >= 0 && mode < modes.size() ? modes.at(mode) : tr("Normal"));
    m_opacityValue->setText(QStringLiteral("%1%").arg(m_engine->layerOpacity(m_layer)));
    m_fillValue->setText(QStringLiteral("%1%").arg(m_engine->layerFillOpacity(m_layer)));
    m_maskValue->setText(m_engine->layerHasMask(m_layer) ? tr("Yes") : tr("No"));

    QStringList locks;
    if (m_engine->layerLockTransparency(m_layer)) {
        locks << tr("Transparency");
    }
    if (m_engine->layerLockPixels(m_layer)) {
        locks << tr("Pixels");
    }
    if (m_engine->layerLockPosition(m_layer)) {
        locks << tr("Position");
    }
    m_lockValue->setText(locks.isEmpty() ? tr("None") : locks.join(QStringLiteral(", ")));
}

// ------------------------------------------------------------- the controls --

void PropertiesPanel::clearParameters()
{
    m_rows.clear();
    m_checks.clear();
    m_valueCombos.clear();
    m_groupCombo = nullptr;
    m_presetCombo = nullptr;
    m_spectrumBottom = nullptr;
    m_colorButton = nullptr;
    m_gradientStrip = nullptr;
    m_gradientPreset = nullptr;
    m_gradientReverse = nullptr;
    m_lookupPreset = nullptr;
    m_curve = nullptr;
    m_curveChannel = nullptr;

    while (QLayoutItem *item = m_parameterLayout->takeAt(0)) {
        if (QWidget *widget = item->widget()) {
            // Deleted later rather than here: this can run from inside one of
            // these widgets' own signal handlers, and deleting a widget that is
            // mid-signal takes the application with it.
            widget->hide();
            widget->setParent(nullptr);
            widget->deleteLater();
        }
        delete item;
    }
}

void PropertiesPanel::buildParameters(const QString &adjustment)
{
    clearParameters();
    m_builtFor = adjustment;

    if (adjustment == QLatin1String("Brightness/Contrast")) {
        addSlider(tr("Brightness:"), QStringLiteral("brightness"), -150, 150, 1.0 / 150.0);
        addSlider(tr("Contrast:"), QStringLiteral("contrast"), -50, 100, 1.0 / 100.0);
    } else if (adjustment == QLatin1String("Levels")) {
        addSlider(tr("Input Black:"), QStringLiteral("inBlack"), 0, 253, 1.0 / 255.0);
        addSlider(tr("Input White:"), QStringLiteral("inWhite"), 2, 255, 1.0 / 255.0);
        addSlider(tr("Gamma:"), QStringLiteral("gamma"), 0.10, 9.99, 1.0, 0.0, 2);
        addSlider(tr("Output Black:"), QStringLiteral("outBlack"), 0, 255, 1.0 / 255.0,
                  0.0, 0, {Qt::black, Qt::white});
        addSlider(tr("Output White:"), QStringLiteral("outWhite"), 0, 255, 1.0 / 255.0,
                  0.0, 0, {Qt::black, Qt::white});
    } else if (adjustment == QLatin1String("Curves")) {
        addCurveEditor();
    } else if (adjustment == QLatin1String("Exposure")) {
        addSlider(tr("Exposure:"), QStringLiteral("exposure"), -20, 20, 1.0, 0.0, 2);
        addSlider(tr("Offset:"), QStringLiteral("offset"), -0.5, 0.5, 1.0, 0.0, 3);
        addSlider(tr("Gamma Correction:"), QStringLiteral("gamma"), 0.10, 9.99, 1.0, 0.0, 2);
    } else if (adjustment == QLatin1String("Vibrance")) {
        addSlider(tr("Vibrance:"), QStringLiteral("vibrance"), -100, 100, 1.0 / 100.0);
        addSlider(tr("Saturation:"), QStringLiteral("saturation"), -100, 100, 1.0 / 100.0);
    } else if (adjustment == QLatin1String("Hue/Saturation")
               || adjustment == QLatin1String("Colorize")) {
        buildHueSaturation(adjustment == QLatin1String("Colorize"));
    } else if (adjustment == QLatin1String("Color Balance")) {
        // The engine keeps a multiplier per channel, so 0 on the control is a
        // multiplier of 1 — that is what the offset of 1.0 is doing.
        addSlider(tr("Cyan / Red:"), QStringLiteral("red"), -100, 100, 1.0 / 100.0, 1.0, 0,
                  {Qt::cyan, QColor(0x80, 0x80, 0x80), Qt::red});
        addSlider(tr("Magenta / Green:"), QStringLiteral("green"), -100, 100, 1.0 / 100.0,
                  1.0, 0, {Qt::magenta, QColor(0x80, 0x80, 0x80), Qt::green});
        addSlider(tr("Yellow / Blue:"), QStringLiteral("blue"), -100, 100, 1.0 / 100.0, 1.0,
                  0, {Qt::yellow, QColor(0x80, 0x80, 0x80), Qt::blue});
    } else if (adjustment == QLatin1String("Photo Filter")) {
        addColorButton(tr("Color:"));
        addSlider(tr("Density:"), QStringLiteral("density"), 1, 100, 1.0 / 100.0);
        addCheck(tr("Preserve Luminosity"), QStringLiteral("preserveLuminosity"));
    } else if (adjustment == QLatin1String("Channel Mixer")) {
        addGroupCombo({tr("Red"), tr("Green"), tr("Blue")});
        // Each output channel is a row of the matrix, so the combo above moves
        // these three keys three at a time.
        addSlider(tr("Red:"), QStringLiteral("matrix%1"), -200, 200, 1.0 / 100.0, 0.0, 0,
                  {}, 0, 3);
        addSlider(tr("Green:"), QStringLiteral("matrix%1"), -200, 200, 1.0 / 100.0, 0.0, 0,
                  {}, 1, 3);
        addSlider(tr("Blue:"), QStringLiteral("matrix%1"), -200, 200, 1.0 / 100.0, 0.0, 0,
                  {}, 2, 3);
        addSlider(tr("Constant:"), QStringLiteral("constant%1"), -200, 200, 1.0 / 100.0,
                  0.0, 0, {}, 0, 1);
        addCheck(tr("Monochrome"), QStringLiteral("monochrome"));
    } else if (adjustment == QLatin1String("Color Lookup")) {
        addColorLookupControls();
    } else if (adjustment == QLatin1String("Posterize")) {
        addSlider(tr("Levels:"), QStringLiteral("levels"), 2, 255, 1.0);
    } else if (adjustment == QLatin1String("Threshold")) {
        addSlider(tr("Threshold Level:"), QStringLiteral("level"), 1, 255, 1.0, 0.0, 0,
                  {Qt::black, Qt::white});
    } else if (adjustment == QLatin1String("Gradient Map")) {
        addGradientMapControls();
    } else if (adjustment == QLatin1String("Selective Color")) {
        addGroupCombo({tr("Reds"), tr("Yellows"), tr("Greens"), tr("Cyans"), tr("Blues"),
                       tr("Magentas"), tr("Whites"), tr("Neutrals"), tr("Blacks")});
        addSlider(tr("Cyan:"), QStringLiteral("range%1.cyan"), -100, 100, 1.0 / 100.0, 0.0,
                  0, {Qt::magenta, Qt::cyan}, 0, 1);
        addSlider(tr("Magenta:"), QStringLiteral("range%1.magenta"), -100, 100, 1.0 / 100.0,
                  0.0, 0, {Qt::green, Qt::magenta}, 0, 1);
        addSlider(tr("Yellow:"), QStringLiteral("range%1.yellow"), -100, 100, 1.0 / 100.0,
                  0.0, 0, {Qt::blue, Qt::yellow}, 0, 1);
        addSlider(tr("Black:"), QStringLiteral("range%1.black"), -100, 100, 1.0 / 100.0,
                  0.0, 0, {Qt::white, Qt::black}, 0, 1);
        addValueCombo(tr("Method:"), {tr("Absolute"), tr("Relative")},
                      QStringLiteral("relative"));
    } else if (adjustment == QLatin1String("Invert")) {
        addNote(tr("Invert has no settings — it flips every channel."));
    } else if (adjustment == QLatin1String("Black & White")) {
        addNote(tr("Converts to grey by Rec. 601 luma. Per-colour weights and "
                   "tinting are not built yet."));
    } else if (adjustment == QLatin1String("Desaturate")) {
        addNote(tr("Converts to grey by HSL lightness. No settings."));
    } else {
        addNote(tr("No controls for %1 yet.").arg(adjustment));
    }

    m_parameterLayout->addStretch(1);
}

void PropertiesPanel::buildHueSaturation(bool colorize)
{
    // Preset menu, which CS6 puts above everything else.
    auto *row = new QWidget(m_parametersPage);
    auto *rowLayout = new QHBoxLayout(row);
    rowLayout->setContentsMargins(0, 0, 0, 0);
    rowLayout->addWidget(new QLabel(tr("Preset:"), row));
    m_presetCombo = new QComboBox(row);
    m_presetCombo->addItem(tr("Default"));
    const QList<HueSatPreset> &presets = hueSaturationPresets();
    for (int i = 1; i < presets.size(); ++i) {
        m_presetCombo->addItem(QString::fromUtf8(presets.at(i).name));
    }
    m_presetCombo->addItem(tr("Custom"));
    rowLayout->addWidget(m_presetCombo, 1);
    m_parameterLayout->addWidget(row);
    connect(m_presetCombo, QOverload<int>::of(&QComboBox::currentIndexChanged), this,
            &PropertiesPanel::applyHueSaturationPreset);

    addValueCombo(QString(), {tr("Master"), tr("Reds"), tr("Yellows"), tr("Greens"),
                              tr("Cyans"), tr("Blues"), tr("Magentas")},
                  QStringLiteral("range"));

    // Colorize tints to an absolute hue, so its Hue runs the whole wheel and
    // its Saturation cannot go negative — there is nothing to take away from.
    addSlider(tr("Hue:"), QStringLiteral("hue"), colorize ? 0 : -180, colorize ? 360 : 180,
              1.0 / 360.0, 0.0, 0, rainbowRamp());
    addSlider(tr("Saturation:"), QStringLiteral("saturation"), colorize ? 0 : -100, 100,
              1.0 / 100.0, 0.0, 0, {QColor(0x80, 0x80, 0x80), Qt::red});
    addSlider(tr("Lightness:"), QStringLiteral("lightness"), -100, 100, 1.0 / 100.0, 0.0, 0,
              {Qt::black, QColor(0x80, 0x80, 0x80), Qt::white});
    addCheck(tr("Colorize"), QStringLiteral("colorize"));

    // The two spectrum bars along the bottom: the hues going in, and where the
    // current shift sends them.
    auto *top = new SpectrumBar(m_parametersPage);
    m_spectrumBottom = new SpectrumBar(m_parametersPage);
    m_parameterLayout->addWidget(top);
    m_parameterLayout->addWidget(m_spectrumBottom);

    for (ComboRow &combo : m_valueCombos) {
        if (combo.key == QLatin1String("range")) {
            combo.combo->setEnabled(!colorize);
        }
    }
}

// -- builders ----------------------------------------------------------------

void PropertiesPanel::addSlider(const QString &label, const QString &keyTemplate,
                                double minimum, double maximum, double scale, double offset,
                                int decimals, const QList<QColor> &ramp, int keyBase,
                                int keyStride)
{
    auto *row = new QWidget(m_parametersPage);
    auto *layout = new QVBoxLayout(row);
    layout->setContentsMargins(0, 0, 0, 2);
    layout->setSpacing(1);

    auto *top = new QHBoxLayout;
    top->setContentsMargins(0, 0, 0, 0);
    top->addWidget(new QLabel(label, row));
    top->addStretch(1);
    auto *spin = new QDoubleSpinBox(row);
    spin->setDecimals(decimals);
    spin->setRange(minimum, maximum);
    spin->setSingleStep(decimals > 0 ? 1.0 / stepsPerUnit(decimals) : 1.0);
    spin->setFixedWidth(58);
    spin->setAlignment(Qt::AlignRight);
    // Without this the engine gets a value for every keystroke, so typing
    // "-40" would push -4 on the way past.
    spin->setKeyboardTracking(false);
    top->addWidget(spin);
    layout->addLayout(top);

    auto *slider = new RampSlider(row);
    const double steps = stepsPerUnit(decimals);
    slider->setRange(qRound(minimum * steps), qRound(maximum * steps));
    if (!ramp.isEmpty()) {
        slider->setRamp(ramp);
    }
    layout->addWidget(slider);
    m_parameterLayout->addWidget(row);

    m_rows.append(SliderRow{slider, spin, keyTemplate, keyBase, keyStride, scale, offset});
    const int index = m_rows.size() - 1;

    connect(slider, &QSlider::valueChanged, this, [this, index, steps](int value) {
        if (m_loading || index >= m_rows.size()) {
            return;
        }
        const QSignalBlocker blocker(m_rows[index].spin);
        m_rows[index].spin->setValue(value / steps);
        pushRow(m_rows[index]);
    });
    connect(spin, &QDoubleSpinBox::valueChanged, this, [this, index, steps](double value) {
        if (m_loading || index >= m_rows.size()) {
            return;
        }
        const QSignalBlocker blocker(m_rows[index].slider);
        m_rows[index].slider->setValue(qRound(value * steps));
        pushRow(m_rows[index]);
    });
    // Releasing the handle ends the gesture there and then, rather than waiting
    // out the commit timer.
    connect(slider, &QSlider::sliderReleased, this, &PropertiesPanel::commitEdit);
    connect(spin, &QDoubleSpinBox::editingFinished, this, &PropertiesPanel::commitEdit);
}

void PropertiesPanel::addCheck(const QString &label, const QString &key)
{
    auto *box = new QCheckBox(label, m_parametersPage);
    m_parameterLayout->addWidget(box);
    m_checks.append(CheckRow{box, key});

    connect(box, &QCheckBox::toggled, this, [this, key](bool on) {
        if (m_loading) {
            return;
        }
        if (key == QLatin1String("colorize")) {
            onColorizeToggled(on);
            return;
        }
        pushValue(key, on ? 1.0f : 0.0f);
        commitEdit();
    });
}

void PropertiesPanel::addGroupCombo(const QStringList &options)
{
    m_groupCombo = new QComboBox(m_parametersPage);
    m_groupCombo->addItems(options);
    m_parameterLayout->addWidget(m_groupCombo);

    // This one sets nothing: it changes which parameters the sliders below it
    // are pointed at, so the values have to be read again.
    connect(m_groupCombo, QOverload<int>::of(&QComboBox::currentIndexChanged), this, [this] {
        if (!m_loading) {
            loadParameters();
        }
    });
}

void PropertiesPanel::addValueCombo(const QString &label, const QStringList &options,
                                    const QString &key)
{
    auto *row = new QWidget(m_parametersPage);
    auto *layout = new QHBoxLayout(row);
    layout->setContentsMargins(0, 0, 0, 0);
    if (!label.isEmpty()) {
        layout->addWidget(new QLabel(label, row));
    }
    auto *combo = new QComboBox(row);
    combo->addItems(options);
    layout->addWidget(combo, 1);
    m_parameterLayout->addWidget(row);
    m_valueCombos.append(ComboRow{combo, key});

    connect(combo, QOverload<int>::of(&QComboBox::currentIndexChanged), this,
            [this, key](int index) {
                if (m_loading) {
                    return;
                }
                pushValue(key, float(index));
                commitEdit();
                // A different colour range means different colours under the
                // sliders.
                if (key == QLatin1String("range")) {
                    updateHueRamps();
                }
            });
}

void PropertiesPanel::addNote(const QString &text)
{
    auto *note = new QLabel(text, m_parametersPage);
    note->setWordWrap(true);
    note->setStyleSheet(QStringLiteral("color: %1;").arg(kDimText.name()));
    m_parameterLayout->addWidget(note);
}

void PropertiesPanel::addColorButton(const QString &label)
{
    auto *row = new QWidget(m_parametersPage);
    auto *layout = new QHBoxLayout(row);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->addWidget(new QLabel(label, row));
    m_colorButton = new QToolButton(row);
    m_colorButton->setFixedSize(46, 18);
    layout->addWidget(m_colorButton);
    layout->addStretch(1);
    m_parameterLayout->addWidget(row);

    connect(m_colorButton, &QToolButton::clicked, this, [this] {
        if (!m_engine || m_layer < 0) {
            return;
        }
        const QColor current(
            qRound(m_engine->layerAdjustmentValue(m_layer, QStringLiteral("red"), 0.0f) * 255),
            qRound(m_engine->layerAdjustmentValue(m_layer, QStringLiteral("green"), 0.0f) * 255),
            qRound(m_engine->layerAdjustmentValue(m_layer, QStringLiteral("blue"), 0.0f) * 255));
        const QColor picked =
            ColorPickerDialog::getColor(current, this, tr("Photo Filter Color"));
        if (!picked.isValid()) {
            return;
        }
        pushValue(QStringLiteral("red"), float(picked.redF()));
        pushValue(QStringLiteral("green"), float(picked.greenF()));
        pushValue(QStringLiteral("blue"), float(picked.blueF()));
        commitEdit();
        loadParameters();
    });
}

void PropertiesPanel::addGradientMapControls()
{
    m_gradientStrip = new QLabel(m_parametersPage);
    m_gradientStrip->setFixedHeight(18);
    m_parameterLayout->addWidget(m_gradientStrip);

    m_gradientPreset = new QComboBox(m_parametersPage);
    if (m_engine) {
        m_gradientPreset->addItems(m_engine->gradientPresetNames().split(
            QLatin1Char('\n'), Qt::SkipEmptyParts));
    }
    m_parameterLayout->addWidget(m_gradientPreset);

    m_gradientReverse = new QCheckBox(tr("Reverse"), m_parametersPage);
    m_parameterLayout->addWidget(m_gradientReverse);

    auto apply = [this] {
        if (m_loading || !m_engine || !beginEdit()) {
            return;
        }
        m_applying = true;
        m_engine->setLayerGradientMap(m_gradientPreset->currentText(),
                                      m_gradientReverse->isChecked());
        m_applying = false;
        commitEdit();
        loadParameters();
    };
    connect(m_gradientPreset, QOverload<int>::of(&QComboBox::currentIndexChanged), this,
            apply);
    connect(m_gradientReverse, &QCheckBox::toggled, this, apply);
}

void PropertiesPanel::addColorLookupControls()
{
    auto *row = new QWidget(m_parametersPage);
    auto *layout = new QHBoxLayout(row);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->addWidget(new QLabel(tr("Look:"), row));
    m_lookupPreset = new QComboBox(row);
    if (m_engine) {
        m_lookupPreset->addItems(m_engine->colorLookupPresetNames().split(
            QLatin1Char('\n'), Qt::SkipEmptyParts));
    }
    layout->addWidget(m_lookupPreset, 1);
    m_parameterLayout->addWidget(row);

    addNote(tr("These are the engine's own looks. Photoshop's Color Lookup reads "
               "3D LUT files, which can remap any colour to any other; these are "
               "one table per channel, so they warm, cool, lift and crush."));

    connect(m_lookupPreset, QOverload<int>::of(&QComboBox::currentIndexChanged), this,
            [this] {
                if (m_loading || !m_engine || !beginEdit()) {
                    return;
                }
                m_applying = true;
                m_engine->setLayerColorLookup(m_lookupPreset->currentText());
                m_applying = false;
                commitEdit();
            });
}

void PropertiesPanel::addCurveEditor()
{
    auto *row = new QWidget(m_parametersPage);
    auto *layout = new QHBoxLayout(row);
    layout->setContentsMargins(0, 0, 0, 0);
    layout->addWidget(new QLabel(tr("Channel:"), row));
    m_curveChannel = new QComboBox(row);
    m_curveChannel->addItems({tr("RGB"), tr("Red"), tr("Green"), tr("Blue")});
    layout->addWidget(m_curveChannel, 1);
    m_parameterLayout->addWidget(row);

    m_curve = new CurveWidget(m_parametersPage);
    m_curve->setMinimumHeight(200);
    m_parameterLayout->addWidget(m_curve);

    if (m_engine) {
        m_curve->setHistogram(m_engine->compositeImage(), 0);
    }

    // The layer carries one curve and the channel it applies to, so switching
    // channel moves the curve rather than revealing a second one.
    addNote(tr("The editor opens on a straight line: a curve is stored as its "
               "table, which cannot be turned back into control points."));

    connect(m_curve, &CurveWidget::curveChanged, this, &PropertiesPanel::pushCurve);
    connect(m_curveChannel, QOverload<int>::of(&QComboBox::currentIndexChanged), this,
            [this](int channel) {
                if (m_loading || !m_engine) {
                    return;
                }
                m_curve->setHistogram(m_engine->compositeImage(), channel);
                pushCurve();
                commitEdit();
            });
}

// -- loading and pushing -----------------------------------------------------

QString PropertiesPanel::resolvedKey(const SliderRow &row) const
{
    if (!row.keyTemplate.contains(QLatin1String("%1"))) {
        return row.keyTemplate;
    }
    const int group = m_groupCombo ? m_groupCombo->currentIndex() : 0;
    return row.keyTemplate.arg(group * row.keyStride + row.keyBase);
}

PropertiesPanel::SliderRow *PropertiesPanel::rowForKey(const QString &key)
{
    for (SliderRow &row : m_rows) {
        if (resolvedKey(row) == key) {
            return &row;
        }
    }
    return nullptr;
}

void PropertiesPanel::loadParameters()
{
    if (!m_engine || m_layer < 0) {
        return;
    }
    m_loading = true;

    for (SliderRow &row : m_rows) {
        const float engineValue =
            m_engine->layerAdjustmentValue(m_layer, resolvedKey(row), 0.0f);
        const double shown = (engineValue - row.offset) / row.scale;
        row.spin->setValue(shown);
        row.slider->setValue(qRound(shown * stepsPerUnit(row.spin->decimals())));
    }
    for (CheckRow &check : m_checks) {
        check.box->setChecked(
            m_engine->layerAdjustmentValue(m_layer, check.key, 0.0f) != 0.0f);
    }
    for (ComboRow &combo : m_valueCombos) {
        const int index =
            int(m_engine->layerAdjustmentValue(m_layer, combo.key, 0.0f));
        combo.combo->setCurrentIndex(qBound(0, index, combo.combo->count() - 1));
    }
    if (m_colorButton) {
        const QColor color(
            qRound(m_engine->layerAdjustmentValue(m_layer, QStringLiteral("red"), 0.0f) * 255),
            qRound(m_engine->layerAdjustmentValue(m_layer, QStringLiteral("green"), 0.0f) * 255),
            qRound(m_engine->layerAdjustmentValue(m_layer, QStringLiteral("blue"), 0.0f) * 255));
        m_colorButton->setStyleSheet(
            QStringLiteral("background-color: %1; border: 1px solid #2a2a2a;")
                .arg(color.name()));
    }
    if (m_gradientStrip) {
        const int width = qMax(40, m_gradientStrip->width());
        m_gradientStrip->setPixmap(QPixmap::fromImage(
            m_engine->layerGradientMapPreview(m_layer, width, 18)));
    }
    if (m_lookupPreset) {
        const QString look = m_engine->layerColorLookupPreset(m_layer);
        const int index = m_lookupPreset->findText(look);
        if (index >= 0) {
            m_lookupPreset->setCurrentIndex(index);
        }
    }
    if (m_curveChannel) {
        m_curveChannel->setCurrentIndex(int(
            m_engine->layerAdjustmentValue(m_layer, QStringLiteral("channel"), 0.0f)));
    }

    m_loading = false;

    if (m_presetCombo) {
        updateHueRamps();
        markCustomPreset();
    }
}

bool PropertiesPanel::beginEdit()
{
    if (m_editing) {
        return true;
    }
    if (!m_engine || m_layer < 0) {
        return false;
    }
    m_editing = m_engine->beginAdjustmentEdit(m_layer);
    return m_editing;
}

void PropertiesPanel::commitEdit()
{
    m_commitTimer->stop();
    if (!m_editing || !m_engine) {
        return;
    }
    m_editing = false;
    m_applying = true;
    m_engine->endAdjustmentEdit(true);
    m_applying = false;
}

void PropertiesPanel::pushValue(const QString &key, float value)
{
    if (m_loading || !beginEdit()) {
        return;
    }
    m_applying = true;
    m_engine->setLayerAdjustmentValue(key, value);
    m_applying = false;
    // Nothing closed this gesture, so put a deadline on it. A slider release or
    // a finished edit beats the timer to it.
    m_commitTimer->start();
}

void PropertiesPanel::pushRow(const SliderRow &row)
{
    const QString key = resolvedKey(row);
    pushValue(key, float(row.spin->value() * row.scale + row.offset));

    if (m_presetCombo) {
        if (key == QLatin1String("hue")) {
            updateHueRamps();
        }
        markCustomPreset();
    }
}

void PropertiesPanel::pushCurve()
{
    if (m_loading || !m_curve || !beginEdit()) {
        return;
    }
    uint8_t lut[256];
    m_curve->buildLut(lut);
    m_applying = true;
    m_engine->setLayerCurves(rust::Slice<const uint8_t>(lut, 256),
                             m_curveChannel ? m_curveChannel->currentIndex() : 0);
    m_applying = false;
    m_commitTimer->start();
}

void PropertiesPanel::onColorizeToggled(bool on)
{
    if (!beginEdit()) {
        return;
    }

    m_applying = true;
    m_engine->setLayerAdjustmentValue(QStringLiteral("colorize"), on ? 1.0f : 0.0f);
    m_applying = false;

    // Both scales change with the checkbox, so the two sliders are re-ranged
    // and their values pushed again rather than left to be reinterpreted.
    SliderRow *hue = rowForKey(QStringLiteral("hue"));
    SliderRow *saturation = rowForKey(QStringLiteral("saturation"));
    m_loading = true;
    if (hue) {
        hue->spin->setRange(on ? 0 : -180, on ? 360 : 180);
        hue->slider->setRange(on ? 0 : -180, on ? 360 : 180);
    }
    if (saturation) {
        saturation->spin->setRange(on ? 0 : -100, 100);
        saturation->slider->setRange(on ? 0 : -100, 100);
        if (on && saturation->spin->value() == 0) {
            // CS6 opens Colorize with some colour in it; a tint of nothing
            // looks like the checkbox did not work.
            saturation->spin->setValue(25);
            saturation->slider->setValue(25);
        }
    }
    for (ComboRow &combo : m_valueCombos) {
        if (combo.key == QLatin1String("range")) {
            combo.combo->setEnabled(!on);
        }
    }
    m_loading = false;

    if (hue) {
        pushRow(*hue);
    }
    if (saturation) {
        pushRow(*saturation);
    }
    commitEdit();
    m_builtFor = on ? QStringLiteral("Colorize") : QStringLiteral("Hue/Saturation");
    updateHueRamps();
    markCustomPreset();
}

// -- Hue/Saturation extras ---------------------------------------------------

void PropertiesPanel::updateHueRamps()
{
    SliderRow *hue = rowForKey(QStringLiteral("hue"));
    SliderRow *saturation = rowForKey(QStringLiteral("saturation"));
    SliderRow *lightness = rowForKey(QStringLiteral("lightness"));
    if (!hue || !saturation || !lightness) {
        return;
    }

    const int shift = qRound(hue->spin->value());
    const bool colorize = m_builtFor == QLatin1String("Colorize");
    // Under Saturation and Lightness, the colour the slider is putting in or
    // taking out: the tint being applied when colorizing, otherwise the range
    // being worked on.
    int centre = 0;
    if (colorize) {
        centre = ((shift % 360) + 360) % 360;
    } else {
        for (ComboRow &combo : m_valueCombos) {
            if (combo.key == QLatin1String("range")) {
                // Master shows red, as CS6 does; the six ranges show their own
                // colour, 60° apart starting at red.
                centre = qMax(0, combo.combo->currentIndex() - 1) * 60;
            }
        }
    }

    saturation->slider->setRamp({QColor(0x80, 0x80, 0x80), QColor::fromHsv(centre, 255, 255)});
    lightness->slider->setRamp({Qt::black, QColor::fromHsv(centre, colorize ? 255 : 0, 255),
                                Qt::white});
    if (m_spectrumBottom) {
        m_spectrumBottom->setHueShift(shift);
    }
}

void PropertiesPanel::applyHueSaturationPreset(int index)
{
    if (m_loading || index < 0 || !m_presetCombo) {
        return;
    }
    const QString name = m_presetCombo->itemText(index);
    if (name == tr("Custom")) {
        return;
    }

    for (const HueSatPreset &preset : hueSaturationPresets()) {
        if (name != QString::fromUtf8(preset.name)) {
            continue;
        }
        // The checkbox rewrites the slider ranges and pushes what it finds, so
        // it goes first and the values follow it.
        for (CheckRow &check : m_checks) {
            if (check.key == QLatin1String("colorize")
                && check.box->isChecked() != preset.colorize) {
                check.box->setChecked(preset.colorize);
            }
        }

        struct Assignment {
            const char *key;
            int value;
        };
        const Assignment assignments[] = {
            {"hue", preset.hue},
            {"saturation", preset.saturation},
            {"lightness", preset.lightness},
        };
        for (const Assignment &assignment : assignments) {
            SliderRow *row = rowForKey(QLatin1String(assignment.key));
            if (!row) {
                continue;
            }
            m_loading = true;
            row->spin->setValue(assignment.value);
            row->slider->setValue(assignment.value);
            m_loading = false;
            pushRow(*row);
        }
        commitEdit();
        updateHueRamps();
        // Ticking Colorize on the way through may have parked the combo on
        // "Custom"; the values are the preset's, so say so.
        markCustomPreset();
        return;
    }
}

void PropertiesPanel::markCustomPreset()
{
    if (!m_presetCombo) {
        return;
    }
    SliderRow *hue = rowForKey(QStringLiteral("hue"));
    SliderRow *saturation = rowForKey(QStringLiteral("saturation"));
    SliderRow *lightness = rowForKey(QStringLiteral("lightness"));
    if (!hue || !saturation || !lightness) {
        return;
    }
    bool colorize = false;
    for (CheckRow &check : m_checks) {
        if (check.key == QLatin1String("colorize")) {
            colorize = check.box->isChecked();
        }
    }

    QString match = tr("Custom");
    for (const HueSatPreset &preset : hueSaturationPresets()) {
        if (qRound(hue->spin->value()) == preset.hue
            && qRound(saturation->spin->value()) == preset.saturation
            && qRound(lightness->spin->value()) == preset.lightness
            && colorize == preset.colorize) {
            match = QString::fromUtf8(preset.name);
            break;
        }
    }

    const QSignalBlocker blocker(m_presetCombo);
    const int index = m_presetCombo->findText(match);
    if (index >= 0) {
        m_presetCombo->setCurrentIndex(index);
    }
}

void PropertiesPanel::resetAdjustment()
{
    if (!m_engine || m_layer < 0 || !beginEdit()) {
        return;
    }
    m_applying = true;
    m_engine->resetLayerAdjustment();
    m_applying = false;
    commitEdit();
    // The adjustment may have changed shape — Colorize resets back to
    // Hue/Saturation — so this goes through the full rebuild.
    refresh();
}

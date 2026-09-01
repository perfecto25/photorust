#include "HueSaturationDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QGridLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QLinearGradient>
#include <QPainter>
#include <QPushButton>
#include <QVBoxLayout>

// ---------------------------------------------------------------------------
// SpectrumBar — rainbow strip showing hue mapping
// ---------------------------------------------------------------------------

SpectrumBar::SpectrumBar(QWidget *parent)
    : QWidget(parent)
{
    setFixedHeight(14);
    setMinimumWidth(200);
}

void SpectrumBar::setHueShift(int degrees)
{
    m_hueShift = degrees;
    update();
}

void SpectrumBar::paintEvent(QPaintEvent *)
{
    QPainter p(this);
    const int w = width();
    const int h = height();

    QLinearGradient grad(0, 0, w, 0);
    const int stops = 7;
    for (int i = 0; i <= stops; ++i) {
        const qreal pos = static_cast<qreal>(i) / stops;
        int hue = static_cast<int>(pos * 360 + m_hueShift) % 360;
        if (hue < 0) hue += 360;
        grad.setColorAt(pos, QColor::fromHsv(hue, 255, 255));
    }
    p.fillRect(0, 0, w, h, grad);

    p.setPen(QPen(QColor(80, 80, 80), 1));
    p.drawRect(0, 0, w - 1, h - 1);
}

// ---------------------------------------------------------------------------
// Presets
// ---------------------------------------------------------------------------

static const HueSatPreset kPresets[] = {
    {"Default",                    0,    0,   0, false},
    {"Cyanotype",                215,   25,   0, true},
    {"Increase Saturation More",   0,   60,   0, false},
    {"Increase Saturation",        0,   30,   0, false},
    {"Old Style",                  0,  -40,   5, false},
    {"Red Boost",                 -5,   20,   0, false},
    {"Sepia",                     35,   25,   0, true},
    {"Strong Saturation",          0,   50,   0, false},
    {"Yellow Boost",               5,   20,   0, false},
};

static constexpr int kPresetCount = static_cast<int>(std::size(kPresets));

const QList<HueSatPreset> &hueSaturationPresets()
{
    static const QList<HueSatPreset> list(std::begin(kPresets), std::end(kPresets));
    return list;
}

// ---------------------------------------------------------------------------
// HueSaturationDialog
// ---------------------------------------------------------------------------

HueSaturationDialog::HueSaturationDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("Hue/Saturation"));
    setFixedSize(500, 340);

    auto *outer = new QHBoxLayout(this);

    // -- left column -----------------------------------------------------------
    auto *left = new QVBoxLayout;

    // Preset row
    auto *presetRow = new QHBoxLayout;
    presetRow->addWidget(new QLabel(tr("Preset:")));
    m_presetCombo = new QComboBox;
    m_presetCombo->addItem(tr("Default"));
    m_presetCombo->insertSeparator(1);
    for (int i = 1; i < kPresetCount; ++i)
        m_presetCombo->addItem(QString::fromUtf8(kPresets[i].name));
    m_presetCombo->insertSeparator(m_presetCombo->count());
    m_presetCombo->addItem(tr("Custom"));
    m_presetCombo->setMinimumWidth(200);
    presetRow->addWidget(m_presetCombo, 1);
    left->addLayout(presetRow);

    // Channel selector
    auto *channelRow = new QHBoxLayout;
    m_channelCombo = new QComboBox;
    m_channelCombo->addItem(tr("Master"));
    m_channelCombo->addItem(tr("Reds"));
    m_channelCombo->addItem(tr("Yellows"));
    m_channelCombo->addItem(tr("Greens"));
    m_channelCombo->addItem(tr("Cyans"));
    m_channelCombo->addItem(tr("Blues"));
    m_channelCombo->addItem(tr("Magentas"));
    m_channelCombo->setMinimumWidth(120);
    channelRow->addStretch();
    channelRow->addWidget(m_channelCombo);
    channelRow->addStretch();
    left->addLayout(channelRow);

    left->addSpacing(4);

    // Sliders
    auto *grid = new QGridLayout;
    grid->setColumnStretch(1, 1);
    grid->setHorizontalSpacing(8);
    grid->setVerticalSpacing(2);

    // Hue: -180 to +180
    grid->addWidget(new QLabel(tr("Hue:")), 0, 0, Qt::AlignRight);
    m_hueSpin = new QSpinBox;
    m_hueSpin->setRange(-180, 180);
    m_hueSpin->setValue(0);
    m_hueSpin->setFixedWidth(60);
    grid->addWidget(m_hueSpin, 0, 2);

    m_hueSlider = new QSlider(Qt::Horizontal);
    m_hueSlider->setRange(-180, 180);
    m_hueSlider->setValue(0);
    grid->addWidget(m_hueSlider, 1, 0, 1, 3);

    // Saturation: -100 to +100
    grid->addWidget(new QLabel(tr("Saturation:")), 2, 0, Qt::AlignRight);
    m_saturationSpin = new QSpinBox;
    m_saturationSpin->setRange(-100, 100);
    m_saturationSpin->setValue(0);
    m_saturationSpin->setFixedWidth(60);
    grid->addWidget(m_saturationSpin, 2, 2);

    m_saturationSlider = new QSlider(Qt::Horizontal);
    m_saturationSlider->setRange(-100, 100);
    m_saturationSlider->setValue(0);
    grid->addWidget(m_saturationSlider, 3, 0, 1, 3);

    // Lightness: -100 to +100
    grid->addWidget(new QLabel(tr("Lightness:")), 4, 0, Qt::AlignRight);
    m_lightnessSpin = new QSpinBox;
    m_lightnessSpin->setRange(-100, 100);
    m_lightnessSpin->setValue(0);
    m_lightnessSpin->setFixedWidth(60);
    grid->addWidget(m_lightnessSpin, 4, 2);

    m_lightnessSlider = new QSlider(Qt::Horizontal);
    m_lightnessSlider->setRange(-100, 100);
    m_lightnessSlider->setValue(0);
    grid->addWidget(m_lightnessSlider, 5, 0, 1, 3);

    left->addLayout(grid);

    left->addSpacing(6);

    // Bottom area: colorize + spectrum bars
    auto *bottomRow = new QHBoxLayout;
    bottomRow->addStretch();
    m_colorize = new QCheckBox(tr("Colorize"));
    bottomRow->addWidget(m_colorize);
    m_preview = new QCheckBox(tr("Preview"));
    m_preview->setChecked(true);
    bottomRow->addWidget(m_preview);
    left->addLayout(bottomRow);

    // Spectrum bars
    m_spectrumTop = new SpectrumBar;
    left->addWidget(m_spectrumTop);
    m_spectrumBottom = new SpectrumBar;
    left->addWidget(m_spectrumBottom);

    outer->addLayout(left, 1);

    // -- right column: buttons -------------------------------------------------
    auto *btnCol = new QVBoxLayout;
    auto *okBtn = new QPushButton(tr("OK"));
    okBtn->setDefault(true);
    okBtn->setFixedWidth(70);
    auto *cancelBtn = new QPushButton(tr("Cancel"));
    cancelBtn->setFixedWidth(70);
    btnCol->addWidget(okBtn);
    btnCol->addWidget(cancelBtn);
    btnCol->addStretch();
    outer->addLayout(btnCol);

    // -- connections -----------------------------------------------------------
    connect(m_hueSlider, &QSlider::valueChanged, m_hueSpin, &QSpinBox::setValue);
    connect(m_hueSpin, QOverload<int>::of(&QSpinBox::valueChanged),
            m_hueSlider, &QSlider::setValue);
    connect(m_saturationSlider, &QSlider::valueChanged,
            m_saturationSpin, &QSpinBox::setValue);
    connect(m_saturationSpin, QOverload<int>::of(&QSpinBox::valueChanged),
            m_saturationSlider, &QSlider::setValue);
    connect(m_lightnessSlider, &QSlider::valueChanged,
            m_lightnessSpin, &QSpinBox::setValue);
    connect(m_lightnessSpin, QOverload<int>::of(&QSpinBox::valueChanged),
            m_lightnessSlider, &QSlider::setValue);

    connect(m_hueSlider, &QSlider::valueChanged, this, &HueSaturationDialog::onValueChanged);
    connect(m_saturationSlider, &QSlider::valueChanged, this, &HueSaturationDialog::onValueChanged);
    connect(m_lightnessSlider, &QSlider::valueChanged, this, &HueSaturationDialog::onValueChanged);
    connect(m_colorize, &QCheckBox::toggled, this, [this](bool checked) {
        if (checked) {
            m_hueSlider->setRange(0, 360);
            m_hueSpin->setRange(0, 360);
            m_saturationSlider->setRange(0, 100);
            m_saturationSpin->setRange(0, 100);
        } else {
            m_hueSlider->setRange(-180, 180);
            m_hueSpin->setRange(-180, 180);
            m_saturationSlider->setRange(-100, 100);
            m_saturationSpin->setRange(-100, 100);
        }
        onValueChanged();
    });

    // Update bottom spectrum bar on hue change
    connect(m_hueSlider, &QSlider::valueChanged, this, [this](int v) {
        m_spectrumBottom->setHueShift(v);
    });

    connect(m_preview, &QCheckBox::toggled, this, [this](bool checked) {
        if (checked)
            applyPreview();
        else
            revertPreview();
    });

    connect(m_presetCombo, QOverload<int>::of(&QComboBox::currentIndexChanged),
            this, &HueSaturationDialog::applyPreset);

    connect(okBtn, &QPushButton::clicked, this, [this] {
        m_previewApplied = false;
        accept();
    });
    connect(cancelBtn, &QPushButton::clicked, this, &QDialog::reject);
}

HueSaturationDialog::~HueSaturationDialog()
{
    revertPreview();
}

void HueSaturationDialog::onValueChanged()
{
    if (!m_applyingPreset) {
        for (int i = 0; i < m_presetCombo->count(); ++i) {
            if (m_presetCombo->itemText(i) == tr("Custom")) {
                m_presetCombo->blockSignals(true);
                m_presetCombo->setCurrentIndex(i);
                m_presetCombo->blockSignals(false);
                break;
            }
        }
    }
    applyPreview();
}

void HueSaturationDialog::applyPreview()
{
    if (!m_engine || !m_preview->isChecked())
        return;

    revertPreview();

    const float hue = m_hueSpin->value() / 360.0f;
    const float saturation = m_saturationSpin->value() / 100.0f;
    const float lightness = m_lightnessSpin->value() / 100.0f;

    if (m_colorize->isChecked()) {
        m_engine->applyAdjustment(QStringLiteral("Colorize"),
                                  hue, saturation, lightness);
    } else {
        const int channel = m_channelCombo->currentIndex();
        m_engine->applyHueSaturationRange(hue, saturation, lightness, channel);
    }
    m_previewApplied = true;
}

void HueSaturationDialog::revertPreview()
{
    if (!m_engine || !m_previewApplied)
        return;
    m_engine->undo();
    m_previewApplied = false;
}

void HueSaturationDialog::applyPreset(int index)
{
    const QString text = m_presetCombo->itemText(index);
    if (text.isEmpty() || text == tr("Custom"))
        return;

    for (int i = 0; i < kPresetCount; ++i) {
        if (text == QString::fromUtf8(kPresets[i].name)) {
            m_applyingPreset = true;
            m_colorize->setChecked(kPresets[i].colorize);
            m_hueSpin->setValue(kPresets[i].hue);
            m_saturationSpin->setValue(kPresets[i].saturation);
            m_lightnessSpin->setValue(kPresets[i].lightness);
            m_applyingPreset = false;
            applyPreview();
            return;
        }
    }
}

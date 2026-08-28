#include "HdrToningDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QGroupBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QVBoxLayout>

struct HdrPreset {
    const char *name;
    int radius;
    float strength;
    float gamma;
    float exposure;
    int detail;
    int shadow;
    int highlight;
    int vibrance;
    int saturation;
};

static const HdrPreset kPresets[] = {
    {"Default",                       187, 4.00f, 0.99f, 0.00f,  30,   0,   0,   0,  20},
    {"City Twilight",                 383, 1.14f, 4.43f, 0.68f,  98,  27,  21,  63,  -3},
    {"Flat",                          200, 1.00f, 1.00f, 0.00f,   0,   0,   0,   0,   0},
    {"Monochromatic Artistic",        101, 3.45f, 2.02f,-1.20f, 288, 100,-100, -100, -100},
    {"Monochromatic High Contrast",   240, 2.06f, 1.80f, 0.88f, 200, -60,  50, -100, -100},
    {"Monochromatic Low Contrast",      1, 0.10f, 0.80f, 0.00f,-100, -10,  40, -100, -100},
    {"Monochromatic",                 100, 2.00f, 1.00f,-0.50f,  80,   0,   0, -100, -100},
    {"More Saturated",                288, 1.75f, 0.27f,-0.15f, 110,   0, -90,  59, 100},
    {"Photorealistic High Contrast",   25, 1.67f, 1.00f, 0.00f,  60, -60, -60,  20,  10},
    {"Photorealistic Low Contrast",    50, 1.00f, 1.50f, 0.00f,  20,  20,  20,  10,  10},
    {"Photorealistic",                 25, 1.67f, 1.26f, 0.00f,  46, -50, -50,  30,  10},
    {"RCS",                            76, 2.21f, 4.32f, 0.75f,  54, -30, -77, 100, -22},
    {"Saturated",                     187, 4.00f, 0.99f, 0.00f,  30,   0,   0,  50,  60},
    {"ScottS",                        176, 0.46f, 0.75f, 0.30f, 300,-100,-100,  22,  26},
    {"Surrealistic High Contrast",    126, 4.00f, 1.62f,-0.15f, 270, -40, -40, 100,   0},
    {"Surrealistic Low Contrast",     300, 4.00f, 3.00f,-1.00f, -81,  40,  40,  35,  55},
    {"Surrealistic",                   50, 3.00f, 0.30f, 0.00f,  80,   0,   0,  20,  20},
};

static constexpr int kPresetCount = static_cast<int>(std::size(kPresets));

HdrToningDialog::HdrToningDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("HDR Toning"));
    setFixedWidth(480);

    auto *outer = new QHBoxLayout(this);
    auto *left = new QVBoxLayout;

    // Preset row
    auto *presetRow = new QHBoxLayout;
    auto *presetLabel = new QLabel(tr("Preset:"));
    presetLabel->setStyleSheet(QStringLiteral("color: #4488cc;"));
    presetRow->addWidget(presetLabel);
    m_preset = new QComboBox;
    for (int i = 0; i < kPresetCount; ++i)
        m_preset->addItem(QString::fromUtf8(kPresets[i].name));
    m_preset->addItem(tr("Custom"));
    m_preset->setCurrentIndex(kPresetCount);
    m_preset->setMinimumWidth(200);
    presetRow->addWidget(m_preset, 1);
    left->addLayout(presetRow);

    // Method (fixed to Local Adaptation for now)
    auto *methodRow = new QHBoxLayout;
    auto *methodLabel = new QLabel(tr("Method:"));
    methodLabel->setStyleSheet(QStringLiteral("color: #4488cc;"));
    methodRow->addWidget(methodLabel);
    auto *methodCombo = new QComboBox;
    methodCombo->addItem(tr("Local Adaptation"));
    methodCombo->setEnabled(false);
    methodRow->addWidget(methodCombo, 1);
    left->addLayout(methodRow);

    // Helper to create slider+spin rows
    auto makeIntRow = [&](const QString &label, int min, int max, int def,
                          const QString &suffix, QSlider *&slider, QSpinBox *&spin) {
        auto *row = new QHBoxLayout;
        auto *lbl = new QLabel(label);
        lbl->setFixedWidth(70);
        lbl->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
        row->addWidget(lbl);
        slider = new QSlider(Qt::Horizontal);
        slider->setRange(min, max);
        slider->setValue(def);
        row->addWidget(slider, 1);
        spin = new QSpinBox;
        spin->setRange(min, max);
        spin->setValue(def);
        if (!suffix.isEmpty()) spin->setSuffix(suffix);
        spin->setFixedWidth(65);
        row->addWidget(spin);
        connect(slider, &QSlider::valueChanged, spin, &QSpinBox::setValue);
        connect(spin, QOverload<int>::of(&QSpinBox::valueChanged), slider, &QSlider::setValue);
        connect(slider, &QSlider::valueChanged, this, &HdrToningDialog::onValueChanged);
        return row;
    };

    auto makeDoubleRow = [&](const QString &label, double min, double max, double def,
                             double step, int decimals, QSlider *&slider, QDoubleSpinBox *&spin) {
        auto *row = new QHBoxLayout;
        auto *lbl = new QLabel(label);
        lbl->setFixedWidth(70);
        lbl->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
        row->addWidget(lbl);
        slider = new QSlider(Qt::Horizontal);
        int scale = static_cast<int>(1.0 / step);
        slider->setRange(static_cast<int>(min * scale), static_cast<int>(max * scale));
        slider->setValue(static_cast<int>(def * scale));
        row->addWidget(slider, 1);
        spin = new QDoubleSpinBox;
        spin->setRange(min, max);
        spin->setValue(def);
        spin->setSingleStep(step);
        spin->setDecimals(decimals);
        spin->setFixedWidth(65);
        row->addWidget(spin);
        connect(slider, &QSlider::valueChanged, this, [spin, scale](int v) {
            spin->setValue(static_cast<double>(v) / scale);
        });
        connect(spin, QOverload<double>::of(&QDoubleSpinBox::valueChanged), this,
                [slider, scale](double v) {
            slider->setValue(static_cast<int>(v * scale));
        });
        connect(slider, &QSlider::valueChanged, this, &HdrToningDialog::onValueChanged);
        return row;
    };

    // Edge Glow group
    auto *edgeGroup = new QGroupBox(tr("Edge Glow"));
    auto *edgeLayout = new QVBoxLayout(edgeGroup);
    edgeLayout->addLayout(makeIntRow(tr("Radius:"), 1, 500, 187,
                                      QStringLiteral(" px"), m_radiusSlider, m_radiusSpin));
    edgeLayout->addLayout(makeDoubleRow(tr("Strength:"), 0.01, 4.0, 4.0,
                                         0.01, 2, m_strengthSlider, m_strengthSpin));
    m_smoothEdges = new QCheckBox(tr("Smooth Edges"));
    edgeLayout->addWidget(m_smoothEdges);
    connect(m_smoothEdges, &QCheckBox::toggled, this, &HdrToningDialog::onValueChanged);
    left->addWidget(edgeGroup);

    // Tone and Detail group
    auto *toneGroup = new QGroupBox(tr("Tone and Detail"));
    auto *toneLayout = new QVBoxLayout(toneGroup);
    toneLayout->addLayout(makeDoubleRow(tr("Gamma:"), 0.01, 9.99, 0.99,
                                         0.01, 2, m_gammaSlider, m_gammaSpin));
    toneLayout->addLayout(makeDoubleRow(tr("Exposure:"), -5.0, 5.0, 0.0,
                                         0.01, 2, m_exposureSlider, m_exposureSpin));
    toneLayout->addLayout(makeIntRow(tr("Detail:"), -100, 300, 30,
                                      QStringLiteral(" %"), m_detailSlider, m_detailSpin));
    left->addWidget(toneGroup);

    // Advanced group
    auto *advGroup = new QGroupBox(tr("Advanced"));
    auto *advLayout = new QVBoxLayout(advGroup);
    advLayout->addLayout(makeIntRow(tr("Shadow:"), -100, 100, 0,
                                     QStringLiteral(" %"), m_shadowSlider, m_shadowSpin));
    advLayout->addLayout(makeIntRow(tr("Highlight:"), -100, 100, 0,
                                     QStringLiteral(" %"), m_highlightSlider, m_highlightSpin));
    advLayout->addLayout(makeIntRow(tr("Vibrance:"), -100, 100, 0,
                                     QStringLiteral(" %"), m_vibranceSlider, m_vibranceSpin));
    advLayout->addLayout(makeIntRow(tr("Saturation:"), -100, 100, 20,
                                     QStringLiteral(" %"), m_saturationSlider, m_saturationSpin));
    left->addWidget(advGroup);

    left->addStretch();
    outer->addLayout(left, 1);

    // Right column: OK, Cancel, Preview
    auto *btnCol = new QVBoxLayout;
    auto *okBtn = new QPushButton(tr("OK"));
    okBtn->setDefault(true);
    okBtn->setFixedWidth(70);
    auto *cancelBtn = new QPushButton(tr("Cancel"));
    cancelBtn->setFixedWidth(70);
    btnCol->addWidget(okBtn);
    btnCol->addWidget(cancelBtn);
    btnCol->addSpacing(12);
    m_preview = new QCheckBox(tr("Preview"));
    m_preview->setChecked(true);
    btnCol->addWidget(m_preview);
    btnCol->addStretch();
    outer->addLayout(btnCol);

    connect(m_preset, QOverload<int>::of(&QComboBox::currentIndexChanged),
            this, &HdrToningDialog::loadPreset);
    connect(m_preview, &QCheckBox::toggled, this, [this](bool on) {
        if (on) applyPreview(); else revertPreview();
    });
    connect(okBtn, &QPushButton::clicked, this, [this]() {
        m_previewApplied = false;
        accept();
    });
    connect(cancelBtn, &QPushButton::clicked, this, &QDialog::reject);
}

HdrToningDialog::~HdrToningDialog()
{
    revertPreview();
}

void HdrToningDialog::loadPreset(int index)
{
    if (index < 0 || index >= kPresetCount)
        return;

    m_loading = true;
    const auto &p = kPresets[index];
    m_radiusSlider->setValue(p.radius);
    m_strengthSpin->setValue(static_cast<double>(p.strength));
    m_gammaSpin->setValue(static_cast<double>(p.gamma));
    m_exposureSpin->setValue(static_cast<double>(p.exposure));
    m_detailSlider->setValue(p.detail);
    m_shadowSlider->setValue(p.shadow);
    m_highlightSlider->setValue(p.highlight);
    m_vibranceSlider->setValue(p.vibrance);
    m_saturationSlider->setValue(p.saturation);
    m_loading = false;
    if (m_preview->isChecked())
        applyPreview();
}

void HdrToningDialog::onValueChanged()
{
    if (m_loading)
        return;
    if (m_preset->currentIndex() < kPresetCount) {
        m_preset->blockSignals(true);
        m_preset->setCurrentIndex(kPresetCount);
        m_preset->blockSignals(false);
    }
    if (m_preview->isChecked())
        applyPreview();
}

void HdrToningDialog::applyPreview()
{
    revertPreview();
    if (!m_engine)
        return;

    m_engine->applyHdrToning(
        static_cast<float>(m_radiusSpin->value()),
        static_cast<float>(m_strengthSpin->value()),
        static_cast<float>(m_gammaSpin->value()),
        static_cast<float>(m_exposureSpin->value()),
        static_cast<float>(m_detailSpin->value()),
        static_cast<float>(m_shadowSpin->value()),
        static_cast<float>(m_highlightSpin->value()),
        static_cast<float>(m_vibranceSpin->value()),
        static_cast<float>(m_saturationSpin->value()));
    m_previewApplied = true;
}

void HdrToningDialog::revertPreview()
{
    if (m_previewApplied && m_engine) {
        m_engine->undo();
        m_previewApplied = false;
    }
}

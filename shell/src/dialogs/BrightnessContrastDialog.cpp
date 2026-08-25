#include "BrightnessContrastDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QGridLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QVBoxLayout>

BrightnessContrastDialog::BrightnessContrastDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("Brightness/Contrast"));
    setFixedSize(380, 175);

    auto *outer = new QHBoxLayout(this);

    // -- left column: sliders --------------------------------------------------
    auto *left = new QVBoxLayout;

    auto *grid = new QGridLayout;
    grid->setColumnStretch(1, 1);

    // Brightness row
    grid->addWidget(new QLabel(tr("Brightness:")), 0, 0, Qt::AlignRight);
    m_brightnessSpin = new QSpinBox;
    m_brightnessSpin->setRange(-150, 150);
    m_brightnessSpin->setValue(0);
    m_brightnessSpin->setFixedWidth(60);
    grid->addWidget(m_brightnessSpin, 0, 2);

    m_brightnessSlider = new QSlider(Qt::Horizontal);
    m_brightnessSlider->setRange(-150, 150);
    m_brightnessSlider->setValue(0);
    grid->addWidget(m_brightnessSlider, 1, 0, 1, 3);

    // Contrast row
    grid->addWidget(new QLabel(tr("Contrast:")), 2, 0, Qt::AlignRight);
    m_contrastSpin = new QSpinBox;
    m_contrastSpin->setRange(-50, 100);
    m_contrastSpin->setValue(0);
    m_contrastSpin->setFixedWidth(60);
    grid->addWidget(m_contrastSpin, 2, 2);

    m_contrastSlider = new QSlider(Qt::Horizontal);
    m_contrastSlider->setRange(-50, 100);
    m_contrastSlider->setValue(0);
    grid->addWidget(m_contrastSlider, 3, 0, 1, 3);

    left->addLayout(grid);
    left->addStretch();

    // Preview checkbox
    m_preview = new QCheckBox(tr("Preview"));
    m_preview->setChecked(true);
    left->addWidget(m_preview, 0, Qt::AlignRight);

    outer->addLayout(left, 1);

    // -- right column: buttons -------------------------------------------------
    auto *btnCol = new QVBoxLayout;
    auto *okBtn = new QPushButton(tr("OK"));
    okBtn->setDefault(true);
    okBtn->setFixedWidth(70);
    auto *cancelBtn = new QPushButton(tr("Cancel"));
    cancelBtn->setFixedWidth(70);
    auto *autoBtn = new QPushButton(tr("Auto"));
    autoBtn->setFixedWidth(70);
    autoBtn->setEnabled(false);
    btnCol->addWidget(okBtn);
    btnCol->addWidget(cancelBtn);
    btnCol->addWidget(autoBtn);
    btnCol->addStretch();
    outer->addLayout(btnCol);

    // -- connections -----------------------------------------------------------
    connect(m_brightnessSlider, &QSlider::valueChanged,
            m_brightnessSpin, &QSpinBox::setValue);
    connect(m_brightnessSpin, QOverload<int>::of(&QSpinBox::valueChanged),
            m_brightnessSlider, &QSlider::setValue);
    connect(m_contrastSlider, &QSlider::valueChanged,
            m_contrastSpin, &QSpinBox::setValue);
    connect(m_contrastSpin, QOverload<int>::of(&QSpinBox::valueChanged),
            m_contrastSlider, &QSlider::setValue);

    connect(m_brightnessSlider, &QSlider::valueChanged,
            this, &BrightnessContrastDialog::onValueChanged);
    connect(m_contrastSlider, &QSlider::valueChanged,
            this, &BrightnessContrastDialog::onValueChanged);

    connect(m_preview, &QCheckBox::toggled, this, [this](bool checked) {
        if (checked)
            applyPreview();
        else
            revertPreview();
    });

    connect(okBtn, &QPushButton::clicked, this, &QDialog::accept);
    connect(cancelBtn, &QPushButton::clicked, this, &QDialog::reject);
}

BrightnessContrastDialog::~BrightnessContrastDialog()
{
    revertPreview();
}

void BrightnessContrastDialog::onValueChanged()
{
    applyPreview();
}

void BrightnessContrastDialog::applyPreview()
{
    if (!m_engine || !m_preview->isChecked())
        return;

    revertPreview();

    const float brightness = m_brightnessSpin->value() / 150.0f;
    const float contrast = m_contrastSpin->value() / 100.0f;
    m_engine->applyAdjustment(QStringLiteral("Brightness/Contrast"),
                              brightness, contrast, 0.0f);
    m_previewApplied = true;
}

void BrightnessContrastDialog::revertPreview()
{
    if (!m_engine || !m_previewApplied)
        return;
    m_engine->undo();
    m_previewApplied = false;
}

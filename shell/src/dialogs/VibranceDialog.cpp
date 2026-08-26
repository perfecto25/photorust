#include "VibranceDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QGridLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QVBoxLayout>

VibranceDialog::VibranceDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("Vibrance"));
    setFixedSize(380, 175);

    auto *outer = new QHBoxLayout(this);

    // -- left column: sliders --------------------------------------------------
    auto *left = new QVBoxLayout;

    auto *grid = new QGridLayout;
    grid->setColumnStretch(1, 1);

    // Vibrance row
    grid->addWidget(new QLabel(tr("Vibrance:")), 0, 0, Qt::AlignRight);
    m_vibranceSpin = new QSpinBox;
    m_vibranceSpin->setRange(-100, 100);
    m_vibranceSpin->setValue(0);
    m_vibranceSpin->setFixedWidth(60);
    grid->addWidget(m_vibranceSpin, 0, 2);

    m_vibranceSlider = new QSlider(Qt::Horizontal);
    m_vibranceSlider->setRange(-100, 100);
    m_vibranceSlider->setValue(0);
    grid->addWidget(m_vibranceSlider, 1, 0, 1, 3);

    // Saturation row
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

    left->addLayout(grid);
    left->addStretch();

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
    btnCol->addWidget(okBtn);
    btnCol->addWidget(cancelBtn);
    btnCol->addStretch();
    outer->addLayout(btnCol);

    // -- connections -----------------------------------------------------------
    connect(m_vibranceSlider, &QSlider::valueChanged,
            m_vibranceSpin, &QSpinBox::setValue);
    connect(m_vibranceSpin, QOverload<int>::of(&QSpinBox::valueChanged),
            m_vibranceSlider, &QSlider::setValue);
    connect(m_saturationSlider, &QSlider::valueChanged,
            m_saturationSpin, &QSpinBox::setValue);
    connect(m_saturationSpin, QOverload<int>::of(&QSpinBox::valueChanged),
            m_saturationSlider, &QSlider::setValue);

    connect(m_vibranceSlider, &QSlider::valueChanged,
            this, &VibranceDialog::onValueChanged);
    connect(m_saturationSlider, &QSlider::valueChanged,
            this, &VibranceDialog::onValueChanged);

    connect(m_preview, &QCheckBox::toggled, this, [this](bool checked) {
        if (checked)
            applyPreview();
        else
            revertPreview();
    });

    connect(okBtn, &QPushButton::clicked, this, [this] {
        m_previewApplied = false;
        accept();
    });
    connect(cancelBtn, &QPushButton::clicked, this, &QDialog::reject);
}

VibranceDialog::~VibranceDialog()
{
    revertPreview();
}

void VibranceDialog::onValueChanged()
{
    applyPreview();
}

void VibranceDialog::applyPreview()
{
    if (!m_engine || !m_preview->isChecked())
        return;

    revertPreview();

    const float vibrance = m_vibranceSpin->value() / 100.0f;
    const float saturation = m_saturationSpin->value() / 100.0f;
    m_engine->applyAdjustment(QStringLiteral("Vibrance"),
                              vibrance, saturation, 0.0f);
    m_previewApplied = true;
}

void VibranceDialog::revertPreview()
{
    if (!m_engine || !m_previewApplied)
        return;
    m_engine->undo();
    m_previewApplied = false;
}

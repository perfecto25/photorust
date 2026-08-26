#include "ThresholdDialog.h"
#include "LevelsDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QVBoxLayout>

ThresholdDialog::ThresholdDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("Threshold"));
    setFixedSize(400, 230);

    if (m_engine)
        m_originalImage = m_engine->compositeImage();

    auto *outer = new QHBoxLayout(this);

    // -- left column -----------------------------------------------------------
    auto *left = new QVBoxLayout;

    auto *levelRow = new QHBoxLayout;
    levelRow->addWidget(new QLabel(tr("Threshold Level:")));
    m_levelSpin = new QSpinBox;
    m_levelSpin->setRange(1, 255);
    m_levelSpin->setValue(128);
    m_levelSpin->setFixedWidth(55);
    levelRow->addWidget(m_levelSpin);
    levelRow->addStretch();
    left->addLayout(levelRow);

    // Histogram
    m_histogram = new HistogramWidget;
    if (!m_originalImage.isNull())
        m_histogram->setImage(m_originalImage, 0);
    left->addWidget(m_histogram);

    m_levelSlider = new QSlider(Qt::Horizontal);
    m_levelSlider->setRange(1, 255);
    m_levelSlider->setValue(128);
    left->addWidget(m_levelSlider);

    left->addStretch();

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
    btnCol->addSpacing(10);
    m_preview = new QCheckBox(tr("Preview"));
    m_preview->setChecked(true);
    btnCol->addWidget(m_preview);
    btnCol->addStretch();
    outer->addLayout(btnCol);

    // -- connections -----------------------------------------------------------
    connect(m_levelSlider, &QSlider::valueChanged, m_levelSpin, &QSpinBox::setValue);
    connect(m_levelSpin, QOverload<int>::of(&QSpinBox::valueChanged),
            m_levelSlider, &QSlider::setValue);
    connect(m_levelSlider, &QSlider::valueChanged, this, &ThresholdDialog::onValueChanged);

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

    applyPreview();
}

ThresholdDialog::~ThresholdDialog()
{
    revertPreview();
}

void ThresholdDialog::onValueChanged()
{
    applyPreview();
}

void ThresholdDialog::applyPreview()
{
    if (!m_engine || !m_preview->isChecked())
        return;

    revertPreview();

    m_engine->applyAdjustment(
        QStringLiteral("Threshold"),
        static_cast<float>(m_levelSpin->value()),
        0.0f, 1.0f);
    m_previewApplied = true;
}

void ThresholdDialog::revertPreview()
{
    if (!m_engine || !m_previewApplied)
        return;
    m_engine->undo();
    m_previewApplied = false;
}

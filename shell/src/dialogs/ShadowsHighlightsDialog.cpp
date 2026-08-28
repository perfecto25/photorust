#include "ShadowsHighlightsDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QGroupBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QVBoxLayout>

ShadowsHighlightsDialog::ShadowsHighlightsDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("Shadows/Highlights"));
    setFixedSize(420, 240);

    auto *outer = new QHBoxLayout(this);
    auto *left = new QVBoxLayout;

    // Shadows group
    auto *shadowGroup = new QGroupBox(tr("Shadows"));
    auto *shadowLayout = new QHBoxLayout(shadowGroup);
    auto *shadowLabel = new QLabel(tr("Amount:"));
    shadowLayout->addWidget(shadowLabel);
    m_shadowSlider = new QSlider(Qt::Horizontal);
    m_shadowSlider->setRange(0, 100);
    m_shadowSlider->setValue(0);
    shadowLayout->addWidget(m_shadowSlider, 1);
    m_shadowSpin = new QSpinBox;
    m_shadowSpin->setRange(0, 100);
    m_shadowSpin->setValue(0);
    m_shadowSpin->setSuffix(QStringLiteral(" %"));
    m_shadowSpin->setFixedWidth(65);
    shadowLayout->addWidget(m_shadowSpin);
    left->addWidget(shadowGroup);

    // Highlights group
    auto *highlightGroup = new QGroupBox(tr("Highlights"));
    auto *highlightLayout = new QHBoxLayout(highlightGroup);
    auto *highlightLabel = new QLabel(tr("Amount:"));
    highlightLayout->addWidget(highlightLabel);
    m_highlightSlider = new QSlider(Qt::Horizontal);
    m_highlightSlider->setRange(0, 100);
    m_highlightSlider->setValue(0);
    highlightLayout->addWidget(m_highlightSlider, 1);
    m_highlightSpin = new QSpinBox;
    m_highlightSpin->setRange(0, 100);
    m_highlightSpin->setValue(0);
    m_highlightSpin->setSuffix(QStringLiteral(" %"));
    m_highlightSpin->setFixedWidth(65);
    highlightLayout->addWidget(m_highlightSpin);
    left->addWidget(highlightGroup);

    // Show More Options checkbox (present for CS6 fidelity, not functional)
    auto *showMore = new QCheckBox(tr("Show More Options"));
    showMore->setEnabled(false);
    left->addWidget(showMore);

    left->addStretch();
    outer->addLayout(left, 1);

    // Right column: OK, Cancel, Load, Save, Preview
    auto *btnCol = new QVBoxLayout;
    auto *okBtn = new QPushButton(tr("OK"));
    okBtn->setDefault(true);
    okBtn->setFixedWidth(70);
    auto *cancelBtn = new QPushButton(tr("Cancel"));
    cancelBtn->setFixedWidth(70);
    auto *loadBtn = new QPushButton(tr("Load..."));
    loadBtn->setFixedWidth(70);
    loadBtn->setEnabled(false);
    auto *saveBtn = new QPushButton(tr("Save..."));
    saveBtn->setFixedWidth(70);
    saveBtn->setEnabled(false);
    btnCol->addWidget(okBtn);
    btnCol->addWidget(cancelBtn);
    btnCol->addWidget(loadBtn);
    btnCol->addWidget(saveBtn);
    btnCol->addSpacing(12);
    m_preview = new QCheckBox(tr("Preview"));
    m_preview->setChecked(true);
    btnCol->addWidget(m_preview);
    btnCol->addStretch();
    outer->addLayout(btnCol);

    // Wire slider ↔ spinbox
    connect(m_shadowSlider, &QSlider::valueChanged, m_shadowSpin, &QSpinBox::setValue);
    connect(m_shadowSpin, QOverload<int>::of(&QSpinBox::valueChanged), m_shadowSlider, &QSlider::setValue);
    connect(m_highlightSlider, &QSlider::valueChanged, m_highlightSpin, &QSpinBox::setValue);
    connect(m_highlightSpin, QOverload<int>::of(&QSpinBox::valueChanged), m_highlightSlider, &QSlider::setValue);

    // Value changes → preview
    connect(m_shadowSlider, &QSlider::valueChanged, this, &ShadowsHighlightsDialog::onValueChanged);
    connect(m_highlightSlider, &QSlider::valueChanged, this, &ShadowsHighlightsDialog::onValueChanged);

    connect(m_preview, &QCheckBox::toggled, this, [this](bool on) {
        if (on) applyPreview(); else revertPreview();
    });

    connect(okBtn, &QPushButton::clicked, this, [this]() {
        m_previewApplied = false;
        accept();
    });
    connect(cancelBtn, &QPushButton::clicked, this, &QDialog::reject);
}

ShadowsHighlightsDialog::~ShadowsHighlightsDialog()
{
    revertPreview();
}

void ShadowsHighlightsDialog::onValueChanged()
{
    if (m_preview->isChecked())
        applyPreview();
}

void ShadowsHighlightsDialog::applyPreview()
{
    revertPreview();
    if (!m_engine)
        return;

    float sa = static_cast<float>(m_shadowSlider->value());
    float ha = static_cast<float>(m_highlightSlider->value());
    m_engine->applyShadowsHighlights(sa, ha);
    m_previewApplied = true;
}

void ShadowsHighlightsDialog::revertPreview()
{
    if (m_previewApplied && m_engine) {
        m_engine->undo();
        m_previewApplied = false;
    }
}

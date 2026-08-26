#include "PosterizeDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QVBoxLayout>

PosterizeDialog::PosterizeDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("Posterize"));
    setFixedSize(380, 130);

    auto *outer = new QHBoxLayout(this);

    // -- left column -----------------------------------------------------------
    auto *left = new QVBoxLayout;

    auto *levelsRow = new QHBoxLayout;
    levelsRow->addWidget(new QLabel(tr("Levels:")));
    m_levelsSpin = new QSpinBox;
    m_levelsSpin->setRange(2, 255);
    m_levelsSpin->setValue(4);
    m_levelsSpin->setFixedWidth(55);
    levelsRow->addWidget(m_levelsSpin);
    levelsRow->addStretch();
    left->addLayout(levelsRow);

    m_levelsSlider = new QSlider(Qt::Horizontal);
    m_levelsSlider->setRange(2, 255);
    m_levelsSlider->setValue(4);
    left->addWidget(m_levelsSlider);

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
    connect(m_levelsSlider, &QSlider::valueChanged, m_levelsSpin, &QSpinBox::setValue);
    connect(m_levelsSpin, QOverload<int>::of(&QSpinBox::valueChanged),
            m_levelsSlider, &QSlider::setValue);
    connect(m_levelsSlider, &QSlider::valueChanged, this, &PosterizeDialog::onValueChanged);

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

PosterizeDialog::~PosterizeDialog()
{
    revertPreview();
}

void PosterizeDialog::onValueChanged()
{
    applyPreview();
}

void PosterizeDialog::applyPreview()
{
    if (!m_engine || !m_preview->isChecked())
        return;

    revertPreview();

    m_engine->applyAdjustment(
        QStringLiteral("Posterize"),
        static_cast<float>(m_levelsSpin->value()),
        0.0f, 1.0f);
    m_previewApplied = true;
}

void PosterizeDialog::revertPreview()
{
    if (!m_engine || !m_previewApplied)
        return;
    m_engine->undo();
    m_previewApplied = false;
}

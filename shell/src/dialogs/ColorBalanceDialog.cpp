#include "ColorBalanceDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QGridLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QVBoxLayout>

ColorBalanceDialog::ColorBalanceDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("Color Balance"));
    setFixedSize(480, 280);

    auto *outer = new QHBoxLayout(this);

    // -- left column -----------------------------------------------------------
    auto *left = new QVBoxLayout;

    // Color Balance group
    auto *cbGroup = new QGroupBox(tr("Color Balance"));
    auto *cbLayout = new QVBoxLayout(cbGroup);

    // Color Levels row
    auto *levelsRow = new QHBoxLayout;
    levelsRow->addWidget(new QLabel(tr("Color Levels:")));
    m_cyanRedSpin = new QSpinBox;
    m_cyanRedSpin->setRange(-100, 100);
    m_cyanRedSpin->setValue(0);
    m_cyanRedSpin->setFixedWidth(55);
    levelsRow->addWidget(m_cyanRedSpin);
    m_magentaGreenSpin = new QSpinBox;
    m_magentaGreenSpin->setRange(-100, 100);
    m_magentaGreenSpin->setValue(0);
    m_magentaGreenSpin->setFixedWidth(55);
    levelsRow->addWidget(m_magentaGreenSpin);
    m_yellowBlueSpin = new QSpinBox;
    m_yellowBlueSpin->setRange(-100, 100);
    m_yellowBlueSpin->setValue(0);
    m_yellowBlueSpin->setFixedWidth(55);
    levelsRow->addWidget(m_yellowBlueSpin);
    levelsRow->addStretch();
    cbLayout->addLayout(levelsRow);

    constexpr int kLabelWidth = 58;

    // Cyan — Red slider
    auto *crRow = new QHBoxLayout;
    auto *cyanLabel = new QLabel(tr("Cyan"));
    cyanLabel->setFixedWidth(kLabelWidth);
    cyanLabel->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
    crRow->addWidget(cyanLabel);
    m_cyanRedSlider = new QSlider(Qt::Horizontal);
    m_cyanRedSlider->setRange(-100, 100);
    m_cyanRedSlider->setValue(0);
    crRow->addWidget(m_cyanRedSlider, 1);
    auto *redLabel = new QLabel(tr("Red"));
    redLabel->setFixedWidth(kLabelWidth);
    crRow->addWidget(redLabel);
    cbLayout->addLayout(crRow);

    // Magenta — Green slider
    auto *mgRow = new QHBoxLayout;
    auto *magentaLabel = new QLabel(tr("Magenta"));
    magentaLabel->setFixedWidth(kLabelWidth);
    magentaLabel->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
    mgRow->addWidget(magentaLabel);
    m_magentaGreenSlider = new QSlider(Qt::Horizontal);
    m_magentaGreenSlider->setRange(-100, 100);
    m_magentaGreenSlider->setValue(0);
    mgRow->addWidget(m_magentaGreenSlider, 1);
    auto *greenLabel = new QLabel(tr("Green"));
    greenLabel->setFixedWidth(kLabelWidth);
    mgRow->addWidget(greenLabel);
    cbLayout->addLayout(mgRow);

    // Yellow — Blue slider
    auto *ybRow = new QHBoxLayout;
    auto *yellowLabel = new QLabel(tr("Yellow"));
    yellowLabel->setFixedWidth(kLabelWidth);
    yellowLabel->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
    ybRow->addWidget(yellowLabel);
    m_yellowBlueSlider = new QSlider(Qt::Horizontal);
    m_yellowBlueSlider->setRange(-100, 100);
    m_yellowBlueSlider->setValue(0);
    ybRow->addWidget(m_yellowBlueSlider, 1);
    auto *blueLabel = new QLabel(tr("Blue"));
    blueLabel->setFixedWidth(kLabelWidth);
    ybRow->addWidget(blueLabel);
    cbLayout->addLayout(ybRow);

    left->addWidget(cbGroup);

    // Tone Balance group
    auto *toneGroup = new QGroupBox(tr("Tone Balance"));
    auto *toneLayout = new QHBoxLayout(toneGroup);
    m_toneGroup = new QButtonGroup(this);
    auto *shadows = new QRadioButton(tr("Shadows"));
    auto *midtones = new QRadioButton(tr("Midtones"));
    auto *highlights = new QRadioButton(tr("Highlights"));
    midtones->setChecked(true);
    m_toneGroup->addButton(shadows, 0);
    m_toneGroup->addButton(midtones, 1);
    m_toneGroup->addButton(highlights, 2);
    toneLayout->addWidget(shadows);
    toneLayout->addWidget(midtones);
    toneLayout->addWidget(highlights);
    left->addWidget(toneGroup);

    // Preserve Luminosity
    m_preserveLuminosity = new QCheckBox(tr("Preserve Luminosity"));
    m_preserveLuminosity->setChecked(true);
    left->addWidget(m_preserveLuminosity);

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
    connect(m_cyanRedSlider, &QSlider::valueChanged,
            m_cyanRedSpin, &QSpinBox::setValue);
    connect(m_cyanRedSpin, QOverload<int>::of(&QSpinBox::valueChanged),
            m_cyanRedSlider, &QSlider::setValue);
    connect(m_magentaGreenSlider, &QSlider::valueChanged,
            m_magentaGreenSpin, &QSpinBox::setValue);
    connect(m_magentaGreenSpin, QOverload<int>::of(&QSpinBox::valueChanged),
            m_magentaGreenSlider, &QSlider::setValue);
    connect(m_yellowBlueSlider, &QSlider::valueChanged,
            m_yellowBlueSpin, &QSpinBox::setValue);
    connect(m_yellowBlueSpin, QOverload<int>::of(&QSpinBox::valueChanged),
            m_yellowBlueSlider, &QSlider::setValue);

    connect(m_cyanRedSlider, &QSlider::valueChanged,
            this, &ColorBalanceDialog::onValueChanged);
    connect(m_magentaGreenSlider, &QSlider::valueChanged,
            this, &ColorBalanceDialog::onValueChanged);
    connect(m_yellowBlueSlider, &QSlider::valueChanged,
            this, &ColorBalanceDialog::onValueChanged);
    connect(m_toneGroup, &QButtonGroup::idClicked,
            this, [this] { onValueChanged(); });
    connect(m_preserveLuminosity, &QCheckBox::toggled,
            this, [this] { onValueChanged(); });

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

ColorBalanceDialog::~ColorBalanceDialog()
{
    revertPreview();
}

void ColorBalanceDialog::onValueChanged()
{
    applyPreview();
}

void ColorBalanceDialog::applyPreview()
{
    if (!m_engine || !m_preview->isChecked())
        return;

    revertPreview();

    const float cr = static_cast<float>(m_cyanRedSpin->value());
    const float mg = static_cast<float>(m_magentaGreenSpin->value());
    const float yb = static_cast<float>(m_yellowBlueSpin->value());
    const int tone = m_toneGroup->checkedId();
    const bool preserve = m_preserveLuminosity->isChecked();

    m_engine->applyColorBalance(cr, mg, yb, tone, preserve);
    m_previewApplied = true;
}

void ColorBalanceDialog::revertPreview()
{
    if (!m_engine || !m_previewApplied)
        return;
    m_engine->undo();
    m_previewApplied = false;
}

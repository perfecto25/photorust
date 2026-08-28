#include "StrokeDialog.h"
#include "ColorPickerDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QButtonGroup>
#include <QEvent>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QVBoxLayout>

static const char *kBlendModes[] = {
    "Normal", "Dissolve",
    "Darken", "Multiply", "Color Burn", "Linear Burn", "Darker Color",
    "Lighten", "Screen", "Color Dodge", "Linear Dodge (Add)", "Lighter Color",
    "Overlay", "Soft Light", "Hard Light", "Vivid Light", "Linear Light",
    "Pin Light", "Hard Mix",
    "Difference", "Exclusion", "Subtract", "Divide",
    "Hue", "Saturation", "Color", "Luminosity",
};

static constexpr int kModeCount = static_cast<int>(std::size(kBlendModes));

StrokeDialog::StrokeDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
    , m_color(Qt::black)
{
    setWindowTitle(tr("Stroke"));
    setFixedSize(380, 310);

    auto *outer = new QHBoxLayout(this);

    auto *left = new QVBoxLayout;

    // Stroke group
    auto *strokeGroup = new QGroupBox(tr("Stroke"));
    auto *strokeLayout = new QVBoxLayout(strokeGroup);

    auto *widthRow = new QHBoxLayout;
    auto *widthLabel = new QLabel(tr("Width:"));
    widthLabel->setFixedWidth(50);
    widthLabel->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
    widthRow->addWidget(widthLabel);
    m_width = new QSpinBox;
    m_width->setRange(1, 250);
    m_width->setValue(1);
    m_width->setSuffix(QStringLiteral(" px"));
    m_width->setFixedWidth(70);
    widthRow->addWidget(m_width);
    widthRow->addStretch();
    strokeLayout->addLayout(widthRow);

    auto *colorRow = new QHBoxLayout;
    auto *colorLabel = new QLabel(tr("Color:"));
    colorLabel->setFixedWidth(50);
    colorLabel->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
    colorRow->addWidget(colorLabel);
    m_colorSwatch = new QLabel;
    m_colorSwatch->setFixedSize(40, 24);
    m_colorSwatch->setCursor(Qt::PointingHandCursor);
    m_colorSwatch->installEventFilter(this);
    colorRow->addWidget(m_colorSwatch);
    colorRow->addStretch();
    strokeLayout->addLayout(colorRow);

    left->addWidget(strokeGroup);

    // Location group
    auto *locGroup = new QGroupBox(tr("Location"));
    auto *locLayout = new QHBoxLayout(locGroup);
    m_inside = new QRadioButton(tr("Inside"));
    m_center = new QRadioButton(tr("Center"));
    m_center->setChecked(true);
    m_outside = new QRadioButton(tr("Outside"));
    auto *locBtnGroup = new QButtonGroup(this);
    locBtnGroup->addButton(m_inside, 0);
    locBtnGroup->addButton(m_center, 1);
    locBtnGroup->addButton(m_outside, 2);
    locLayout->addWidget(m_inside);
    locLayout->addWidget(m_center);
    locLayout->addWidget(m_outside);
    left->addWidget(locGroup);

    // Blending group
    auto *blendGroup = new QGroupBox(tr("Blending"));
    auto *blendLayout = new QVBoxLayout(blendGroup);

    auto *modeRow = new QHBoxLayout;
    auto *modeLabel = new QLabel(tr("Mode:"));
    modeLabel->setFixedWidth(55);
    modeLabel->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
    modeRow->addWidget(modeLabel);
    m_mode = new QComboBox;
    for (int i = 0; i < kModeCount; ++i)
        m_mode->addItem(QString::fromUtf8(kBlendModes[i]));
    m_mode->setMinimumWidth(140);
    modeRow->addWidget(m_mode, 1);
    blendLayout->addLayout(modeRow);

    auto *opacityRow = new QHBoxLayout;
    auto *opacityLabel = new QLabel(tr("Opacity:"));
    opacityLabel->setFixedWidth(55);
    opacityLabel->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
    opacityRow->addWidget(opacityLabel);
    m_opacity = new QSpinBox;
    m_opacity->setRange(1, 100);
    m_opacity->setValue(100);
    m_opacity->setSuffix(QStringLiteral(" %"));
    m_opacity->setFixedWidth(65);
    opacityRow->addWidget(m_opacity);
    opacityRow->addStretch();
    blendLayout->addLayout(opacityRow);

    left->addWidget(blendGroup);

    m_preserveTransp = new QCheckBox(tr("Preserve Transparency"));
    left->addWidget(m_preserveTransp);

    left->addStretch();

    outer->addLayout(left, 1);

    // Buttons
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

    connect(okBtn, &QPushButton::clicked, this, &QDialog::accept);
    connect(cancelBtn, &QPushButton::clicked, this, &QDialog::reject);

    updateColorSwatch();
}

int StrokeDialog::strokeWidth() const { return m_width->value(); }

QColor StrokeDialog::strokeColor() const { return m_color; }

int StrokeDialog::location() const
{
    if (m_inside->isChecked()) return 0;
    if (m_outside->isChecked()) return 2;
    return 1;
}

int StrokeDialog::blendModeIndex() const { return m_mode->currentIndex(); }

int StrokeDialog::opacity() const { return m_opacity->value(); }

bool StrokeDialog::preserveTransparency() const { return m_preserveTransp->isChecked(); }

void StrokeDialog::openColorPicker()
{
    QColor picked = ColorPickerDialog::getColor(m_color, this, tr("Stroke Color"));
    if (picked.isValid()) {
        m_color = picked;
        updateColorSwatch();
    }
}

void StrokeDialog::updateColorSwatch()
{
    m_colorSwatch->setStyleSheet(
        QStringLiteral("background-color: %1; border: 1px solid #555;")
            .arg(m_color.name()));
}

bool StrokeDialog::eventFilter(QObject *obj, QEvent *event)
{
    if (obj == m_colorSwatch && event->type() == QEvent::MouseButtonRelease) {
        openColorPicker();
        return true;
    }
    return QDialog::eventFilter(obj, event);
}

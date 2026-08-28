#include "SelectiveColorDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QButtonGroup>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QVBoxLayout>

static const char *kRangeNames[] = {
    "Reds", "Yellows", "Greens", "Cyans", "Blues", "Magentas",
    "Whites", "Neutrals", "Blacks",
};

static const QColor kRangeSwatches[] = {
    QColor(180,  0,  0),    // Reds
    QColor(180,180,  0),    // Yellows
    QColor(  0,128,  0),    // Greens
    QColor(  0,180,180),    // Cyans
    QColor(  0,  0,200),    // Blues
    QColor(180,  0,180),    // Magentas
    QColor(240,240,240),    // Whites
    QColor(128,128,128),    // Neutrals
    QColor( 30, 30, 30),    // Blacks
};

static constexpr int kRangeCount = 9;

SelectiveColorDialog::SelectiveColorDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("Selective Color"));
    setFixedSize(470, 340);

    auto *outer = new QHBoxLayout(this);
    auto *left = new QVBoxLayout;

    // Colors combo
    auto *colorRow = new QHBoxLayout;
    auto *colorLabel = new QLabel(tr("Colors:"));
    colorLabel->setFixedWidth(55);
    colorLabel->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
    colorRow->addWidget(colorLabel);
    m_colorCombo = new QComboBox;
    for (int i = 0; i < kRangeCount; ++i) {
        QPixmap swatch(16, 16);
        swatch.fill(kRangeSwatches[i]);
        m_colorCombo->addItem(QIcon(swatch), QString::fromUtf8(kRangeNames[i]));
    }
    m_colorCombo->setMinimumWidth(180);
    colorRow->addWidget(m_colorCombo, 1);
    left->addLayout(colorRow);

    left->addSpacing(6);

    // CMYK sliders
    auto makeSliderRow = [&](const QString &label, QSlider *&slider, QSpinBox *&spin) {
        auto *row = new QVBoxLayout;
        auto *top = new QHBoxLayout;
        auto *lbl = new QLabel(label);
        lbl->setFixedWidth(65);
        lbl->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
        top->addWidget(lbl);
        top->addStretch();
        spin = new QSpinBox;
        spin->setRange(-100, 100);
        spin->setValue(0);
        spin->setSuffix(QStringLiteral(" %"));
        spin->setFixedWidth(70);
        top->addWidget(spin);
        row->addLayout(top);

        slider = new QSlider(Qt::Horizontal);
        slider->setRange(-100, 100);
        slider->setValue(0);
        row->addWidget(slider);

        connect(slider, &QSlider::valueChanged, spin, &QSpinBox::setValue);
        connect(spin, QOverload<int>::of(&QSpinBox::valueChanged), slider, &QSlider::setValue);
        connect(slider, &QSlider::valueChanged, this, &SelectiveColorDialog::onValueChanged);

        left->addLayout(row);
    };

    makeSliderRow(tr("Cyan:"), m_cyanSlider, m_cyanSpin);
    makeSliderRow(tr("Magenta:"), m_magentaSlider, m_magentaSpin);
    makeSliderRow(tr("Yellow:"), m_yellowSlider, m_yellowSpin);
    makeSliderRow(tr("Black:"), m_blackSlider, m_blackSpin);

    left->addSpacing(6);

    // Method: Relative / Absolute
    auto *methodRow = new QHBoxLayout;
    auto *methodLabel = new QLabel(tr("Method:"));
    methodLabel->setFixedWidth(55);
    methodLabel->setAlignment(Qt::AlignRight | Qt::AlignVCenter);
    methodRow->addWidget(methodLabel);
    m_relative = new QRadioButton(tr("Relative"));
    m_relative->setChecked(true);
    m_absolute = new QRadioButton(tr("Absolute"));
    auto *methodGroup = new QButtonGroup(this);
    methodGroup->addButton(m_relative, 0);
    methodGroup->addButton(m_absolute, 1);
    methodRow->addWidget(m_relative);
    methodRow->addWidget(m_absolute);
    methodRow->addStretch();
    left->addLayout(methodRow);

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

    connect(m_colorCombo, QOverload<int>::of(&QComboBox::currentIndexChanged),
            this, &SelectiveColorDialog::onColorChanged);
    connect(m_preview, &QCheckBox::toggled, this, [this](bool on) {
        if (on) applyPreview(); else revertPreview();
    });
    connect(m_relative, &QRadioButton::toggled, this, [this]() { onValueChanged(); });

    connect(okBtn, &QPushButton::clicked, this, [this]() {
        m_previewApplied = false;
        accept();
    });
    connect(cancelBtn, &QPushButton::clicked, this, &QDialog::reject);
}

SelectiveColorDialog::~SelectiveColorDialog()
{
    revertPreview();
}

void SelectiveColorDialog::saveCurrentRange()
{
    m_adjustments[m_currentRange][0] = static_cast<float>(m_cyanSlider->value());
    m_adjustments[m_currentRange][1] = static_cast<float>(m_magentaSlider->value());
    m_adjustments[m_currentRange][2] = static_cast<float>(m_yellowSlider->value());
    m_adjustments[m_currentRange][3] = static_cast<float>(m_blackSlider->value());
}

void SelectiveColorDialog::loadCurrentRange()
{
    m_cyanSlider->blockSignals(true);
    m_cyanSpin->blockSignals(true);
    m_magentaSlider->blockSignals(true);
    m_magentaSpin->blockSignals(true);
    m_yellowSlider->blockSignals(true);
    m_yellowSpin->blockSignals(true);
    m_blackSlider->blockSignals(true);
    m_blackSpin->blockSignals(true);

    m_cyanSlider->setValue(static_cast<int>(m_adjustments[m_currentRange][0]));
    m_cyanSpin->setValue(static_cast<int>(m_adjustments[m_currentRange][0]));
    m_magentaSlider->setValue(static_cast<int>(m_adjustments[m_currentRange][1]));
    m_magentaSpin->setValue(static_cast<int>(m_adjustments[m_currentRange][1]));
    m_yellowSlider->setValue(static_cast<int>(m_adjustments[m_currentRange][2]));
    m_yellowSpin->setValue(static_cast<int>(m_adjustments[m_currentRange][2]));
    m_blackSlider->setValue(static_cast<int>(m_adjustments[m_currentRange][3]));
    m_blackSpin->setValue(static_cast<int>(m_adjustments[m_currentRange][3]));

    m_cyanSlider->blockSignals(false);
    m_cyanSpin->blockSignals(false);
    m_magentaSlider->blockSignals(false);
    m_magentaSpin->blockSignals(false);
    m_yellowSlider->blockSignals(false);
    m_yellowSpin->blockSignals(false);
    m_blackSlider->blockSignals(false);
    m_blackSpin->blockSignals(false);
}

void SelectiveColorDialog::onColorChanged(int index)
{
    saveCurrentRange();
    m_currentRange = index;
    loadCurrentRange();
}

void SelectiveColorDialog::onValueChanged()
{
    saveCurrentRange();
    if (m_preview->isChecked())
        applyPreview();
}

void SelectiveColorDialog::applyPreview()
{
    revertPreview();
    if (!m_engine)
        return;

    saveCurrentRange();

    QStringList parts;
    for (int i = 0; i < 9; ++i) {
        parts << QStringLiteral("%1,%2,%3,%4")
            .arg(m_adjustments[i][0])
            .arg(m_adjustments[i][1])
            .arg(m_adjustments[i][2])
            .arg(m_adjustments[i][3]);
    }
    QString data = parts.join(QLatin1Char(';'));
    m_engine->applySelectiveColor(data, m_relative->isChecked());
    m_previewApplied = true;
}

void SelectiveColorDialog::revertPreview()
{
    if (m_previewApplied && m_engine) {
        m_engine->undo();
        m_previewApplied = false;
    }
}

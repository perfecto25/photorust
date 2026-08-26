#include "ExposureDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QGridLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QToolButton>
#include <QVBoxLayout>

struct ExposurePreset {
    const char *name;
    double exposure;
    double offset;
    double gamma;
};

static const ExposurePreset kPresets[] = {
    {"Default", 0.0, 0.0, 1.0},
};

ExposureDialog::ExposureDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
{
    setWindowTitle(tr("Exposure"));
    setFixedSize(460, 220);

    auto *outer = new QHBoxLayout(this);

    // -- left column -----------------------------------------------------------
    auto *left = new QVBoxLayout;

    // Preset row
    auto *presetRow = new QHBoxLayout;
    presetRow->addWidget(new QLabel(tr("Preset:")));
    m_presetCombo = new QComboBox;
    m_presetCombo->addItem(tr("Default"));
    m_presetCombo->addItem(tr("Custom"));
    m_presetCombo->setMinimumWidth(160);
    presetRow->addWidget(m_presetCombo, 1);
    left->addLayout(presetRow);

    left->addSpacing(8);

    // Sliders grid
    auto *grid = new QGridLayout;
    grid->setColumnStretch(1, 1);
    grid->setHorizontalSpacing(8);
    grid->setVerticalSpacing(6);

    // Exposure: range -20..+20, step 0.01
    grid->addWidget(new QLabel(tr("Exposure:")), 0, 0, Qt::AlignRight);
    m_exposureSlider = new QSlider(Qt::Horizontal);
    m_exposureSlider->setRange(-2000, 2000);
    m_exposureSlider->setValue(0);
    grid->addWidget(m_exposureSlider, 0, 1);
    m_exposureSpin = new QDoubleSpinBox;
    m_exposureSpin->setRange(-20.0, 20.0);
    m_exposureSpin->setDecimals(2);
    m_exposureSpin->setSingleStep(0.01);
    m_exposureSpin->setValue(0.0);
    m_exposureSpin->setFixedWidth(72);
    grid->addWidget(m_exposureSpin, 0, 2);

    // Offset: range -0.5..+0.5, step 0.0001
    grid->addWidget(new QLabel(tr("Offset:")), 1, 0, Qt::AlignRight);
    m_offsetSlider = new QSlider(Qt::Horizontal);
    m_offsetSlider->setRange(-5000, 5000);
    m_offsetSlider->setValue(0);
    grid->addWidget(m_offsetSlider, 1, 1);
    m_offsetSpin = new QDoubleSpinBox;
    m_offsetSpin->setRange(-0.5, 0.5);
    m_offsetSpin->setDecimals(4);
    m_offsetSpin->setSingleStep(0.0001);
    m_offsetSpin->setValue(0.0);
    m_offsetSpin->setFixedWidth(72);
    grid->addWidget(m_offsetSpin, 1, 2);

    // Gamma Correction: range 0.01..9.99, step 0.01
    grid->addWidget(new QLabel(tr("Gamma Correction:")), 2, 0, Qt::AlignRight);
    m_gammaSlider = new QSlider(Qt::Horizontal);
    m_gammaSlider->setRange(1, 999);
    m_gammaSlider->setValue(100);
    grid->addWidget(m_gammaSlider, 2, 1);
    m_gammaSpin = new QDoubleSpinBox;
    m_gammaSpin->setRange(0.01, 9.99);
    m_gammaSpin->setDecimals(2);
    m_gammaSpin->setSingleStep(0.01);
    m_gammaSpin->setValue(1.0);
    m_gammaSpin->setFixedWidth(72);
    grid->addWidget(m_gammaSpin, 2, 2);

    // Eyedropper buttons column
    auto *blackEye = new QToolButton;
    blackEye->setText(QStringLiteral("✏"));
    blackEye->setToolTip(tr("Set Black Point"));
    blackEye->setFixedSize(24, 24);
    blackEye->setEnabled(false);
    grid->addWidget(blackEye, 0, 3);

    auto *greyEye = new QToolButton;
    greyEye->setText(QStringLiteral("✏"));
    greyEye->setToolTip(tr("Set Midtone Point"));
    greyEye->setFixedSize(24, 24);
    greyEye->setEnabled(false);
    grid->addWidget(greyEye, 1, 3);

    auto *whiteEye = new QToolButton;
    whiteEye->setText(QStringLiteral("✏"));
    whiteEye->setToolTip(tr("Set White Point"));
    whiteEye->setFixedSize(24, 24);
    whiteEye->setEnabled(false);
    grid->addWidget(whiteEye, 2, 3);

    left->addLayout(grid);
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

    // -- connections: slider <-> spin sync -------------------------------------
    connect(m_exposureSlider, &QSlider::valueChanged, this, [this](int v) {
        QSignalBlocker b(m_exposureSpin);
        m_exposureSpin->setValue(v / 100.0);
        onValueChanged();
    });
    connect(m_exposureSpin, QOverload<double>::of(&QDoubleSpinBox::valueChanged),
            this, [this](double v) {
        QSignalBlocker b(m_exposureSlider);
        m_exposureSlider->setValue(static_cast<int>(v * 100));
        onValueChanged();
    });

    connect(m_offsetSlider, &QSlider::valueChanged, this, [this](int v) {
        QSignalBlocker b(m_offsetSpin);
        m_offsetSpin->setValue(v / 10000.0);
        onValueChanged();
    });
    connect(m_offsetSpin, QOverload<double>::of(&QDoubleSpinBox::valueChanged),
            this, [this](double v) {
        QSignalBlocker b(m_offsetSlider);
        m_offsetSlider->setValue(static_cast<int>(v * 10000));
        onValueChanged();
    });

    connect(m_gammaSlider, &QSlider::valueChanged, this, [this](int v) {
        QSignalBlocker b(m_gammaSpin);
        m_gammaSpin->setValue(v / 100.0);
        onValueChanged();
    });
    connect(m_gammaSpin, QOverload<double>::of(&QDoubleSpinBox::valueChanged),
            this, [this](double v) {
        QSignalBlocker b(m_gammaSlider);
        m_gammaSlider->setValue(static_cast<int>(v * 100));
        onValueChanged();
    });

    connect(m_preview, &QCheckBox::toggled, this, [this](bool checked) {
        if (checked)
            applyPreview();
        else
            revertPreview();
    });

    connect(m_presetCombo, QOverload<int>::of(&QComboBox::currentIndexChanged),
            this, &ExposureDialog::applyPreset);

    connect(okBtn, &QPushButton::clicked, this, [this] {
        m_previewApplied = false;
        accept();
    });
    connect(cancelBtn, &QPushButton::clicked, this, &QDialog::reject);
}

ExposureDialog::~ExposureDialog()
{
    revertPreview();
}

void ExposureDialog::onValueChanged()
{
    if (!m_applyingPreset) {
        m_presetCombo->blockSignals(true);
        m_presetCombo->setCurrentIndex(m_presetCombo->count() - 1);
        m_presetCombo->blockSignals(false);
    }
    applyPreview();
}

void ExposureDialog::applyPreview()
{
    if (!m_engine || !m_preview->isChecked())
        return;

    revertPreview();

    const float exposure = static_cast<float>(m_exposureSpin->value());
    const float offset = static_cast<float>(m_offsetSpin->value());
    const float gamma = static_cast<float>(m_gammaSpin->value());
    m_engine->applyAdjustment(QStringLiteral("Exposure"), exposure, offset, gamma);
    m_previewApplied = true;
}

void ExposureDialog::revertPreview()
{
    if (!m_engine || !m_previewApplied)
        return;
    m_engine->undo();
    m_previewApplied = false;
}

void ExposureDialog::applyPreset(int index)
{
    const QString text = m_presetCombo->itemText(index);
    if (text == tr("Custom"))
        return;

    for (const auto &p : kPresets) {
        if (text == tr(p.name)) {
            m_applyingPreset = true;
            m_exposureSpin->setValue(p.exposure);
            m_offsetSpin->setValue(p.offset);
            m_gammaSpin->setValue(p.gamma);
            m_applyingPreset = false;
            applyPreview();
            return;
        }
    }
}

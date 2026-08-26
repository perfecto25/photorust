#include "PhotoFilterDialog.h"
#include "ColorPickerDialog.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QEvent>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QPushButton>
#include <QVBoxLayout>

struct PhotoFilterPreset {
    const char *name;
    int r, g, b;
};

static const PhotoFilterPreset kFilters[] = {
    {"Warming Filter (85)",  236, 138,   0},
    {"Warming Filter (LBA)", 250, 150,  45},
    {"Warming Filter (81)",  235, 177,  19},
    {"Cooling Filter (80)",    0, 109, 255},
    {"Cooling Filter (LBB)",   0,  93, 186},
    {"Cooling Filter (82)",    0, 136, 234},
    {"Red",                  234,  26,   0},
    {"Orange",               235, 117,   0},
    {"Yellow",               255, 230,   0},
    {"Green",                  0, 148,   0},
    {"Cyan",                   0, 183, 239},
    {"Blue",                   0,  51, 209},
    {"Violet",                75,   0, 130},
    {"Magenta",              255,   0, 144},
    {"Sepia",                172, 122,  51},
    {"Deep Red",             130,   5,   0},
    {"Deep Blue",              0,   0, 130},
    {"Deep Emerald",           0, 100,  18},
    {"Deep Yellow",          255, 204,   0},
    {"Underwater",             0, 194, 190},
};

static constexpr int kFilterCount = static_cast<int>(std::size(kFilters));

PhotoFilterDialog::PhotoFilterDialog(Engine *engine, QWidget *parent)
    : QDialog(parent)
    , m_engine(engine)
    , m_currentColor(236, 138, 0)
{
    setWindowTitle(tr("Photo Filter"));
    setFixedSize(420, 220);

    auto *outer = new QHBoxLayout(this);

    // -- left column -----------------------------------------------------------
    auto *left = new QVBoxLayout;

    // Use group
    auto *useGroup = new QGroupBox(tr("Use"));
    auto *useLayout = new QVBoxLayout(useGroup);

    // Filter radio + combo
    auto *filterRow = new QHBoxLayout;
    m_radioFilter = new QRadioButton(tr("Filter:"));
    m_radioFilter->setChecked(true);
    filterRow->addWidget(m_radioFilter);

    m_filterCombo = new QComboBox;
    for (int i = 0; i < kFilterCount; ++i)
        m_filterCombo->addItem(QString::fromUtf8(kFilters[i].name));
    m_filterCombo->setMinimumWidth(180);
    filterRow->addWidget(m_filterCombo, 1);
    useLayout->addLayout(filterRow);

    // Color radio + swatch
    auto *colorRow = new QHBoxLayout;
    m_radioColor = new QRadioButton(tr("Color:"));
    colorRow->addWidget(m_radioColor);

    m_colorSwatch = new QLabel;
    m_colorSwatch->setFixedSize(40, 24);
    m_colorSwatch->setCursor(Qt::PointingHandCursor);
    m_colorSwatch->installEventFilter(this);
    colorRow->addWidget(m_colorSwatch);
    colorRow->addStretch();
    useLayout->addLayout(colorRow);

    left->addWidget(useGroup);

    left->addSpacing(4);

    // Density row
    auto *densityRow = new QHBoxLayout;
    densityRow->addWidget(new QLabel(tr("Density:")));
    m_densitySpin = new QSpinBox;
    m_densitySpin->setRange(1, 100);
    m_densitySpin->setValue(25);
    m_densitySpin->setSuffix(QStringLiteral(" %"));
    m_densitySpin->setFixedWidth(60);
    densityRow->addWidget(m_densitySpin);
    densityRow->addStretch();
    left->addLayout(densityRow);

    m_densitySlider = new QSlider(Qt::Horizontal);
    m_densitySlider->setRange(1, 100);
    m_densitySlider->setValue(25);
    left->addWidget(m_densitySlider);

    left->addSpacing(4);

    // Preserve Luminosity
    m_preserveLum = new QCheckBox(tr("Preserve Luminosity"));
    m_preserveLum->setChecked(true);
    left->addWidget(m_preserveLum);

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
    connect(m_densitySlider, &QSlider::valueChanged, m_densitySpin, &QSpinBox::setValue);
    connect(m_densitySpin, QOverload<int>::of(&QSpinBox::valueChanged),
            m_densitySlider, &QSlider::setValue);

    connect(m_densitySlider, &QSlider::valueChanged, this, &PhotoFilterDialog::onValueChanged);
    connect(m_preserveLum, &QCheckBox::toggled, this, [this] { onValueChanged(); });

    connect(m_radioFilter, &QRadioButton::toggled, this, [this](bool checked) {
        m_filterCombo->setEnabled(checked);
        if (checked)
            onFilterChanged(m_filterCombo->currentIndex());
    });
    connect(m_radioColor, &QRadioButton::toggled, this, [this](bool checked) {
        m_filterCombo->setEnabled(!checked);
        if (checked)
            onValueChanged();
    });

    connect(m_filterCombo, QOverload<int>::of(&QComboBox::currentIndexChanged),
            this, &PhotoFilterDialog::onFilterChanged);

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

    // Initial state
    updateColorSwatch();
    applyPreview();
}

PhotoFilterDialog::~PhotoFilterDialog()
{
    revertPreview();
}

void PhotoFilterDialog::onFilterChanged(int index)
{
    if (!m_radioFilter->isChecked())
        return;
    if (index < 0 || index >= kFilterCount)
        return;

    m_currentColor = QColor(kFilters[index].r, kFilters[index].g, kFilters[index].b);
    updateColorSwatch();
    applyPreview();
}

void PhotoFilterDialog::onValueChanged()
{
    applyPreview();
}

void PhotoFilterDialog::applyPreview()
{
    if (!m_engine || !m_preview->isChecked())
        return;

    revertPreview();

    m_engine->applyPhotoFilter(
        static_cast<float>(m_currentColor.redF()),
        static_cast<float>(m_currentColor.greenF()),
        static_cast<float>(m_currentColor.blueF()),
        static_cast<float>(m_densitySpin->value()),
        m_preserveLum->isChecked());
    m_previewApplied = true;
}

void PhotoFilterDialog::revertPreview()
{
    if (!m_engine || !m_previewApplied)
        return;
    m_engine->undo();
    m_previewApplied = false;
}

void PhotoFilterDialog::openColorPicker()
{
    QColor picked = ColorPickerDialog::getColor(m_currentColor, this, tr("Photo Filter Color"));
    if (!picked.isValid())
        return;

    m_currentColor = picked;
    m_radioColor->setChecked(true);
    updateColorSwatch();
    applyPreview();
}

void PhotoFilterDialog::updateColorSwatch()
{
    m_colorSwatch->setStyleSheet(
        QStringLiteral("background-color: %1; border: 1px solid #555;")
            .arg(m_currentColor.name()));
}

bool PhotoFilterDialog::eventFilter(QObject *obj, QEvent *event)
{
    if (obj == m_colorSwatch && event->type() == QEvent::MouseButtonRelease) {
        openColorPicker();
        return true;
    }
    return QDialog::eventFilter(obj, event);
}

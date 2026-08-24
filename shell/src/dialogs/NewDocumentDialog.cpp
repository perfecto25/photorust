#include "NewDocumentDialog.h"

#include <QComboBox>
#include <QDoubleSpinBox>
#include <QGridLayout>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QMessageBox>
#include <QPushButton>
#include <QSpinBox>
#include <QVBoxLayout>

#include <cmath>

namespace {

struct SizePreset {
    const char *name;
    double width;
    double height;
    int unit;       // 0=pixels
    int resolution;
};

const SizePreset kPresets[] = {
    {"Default Photoshop Size", 1280, 800, 0, 72},
    {"U.S. Paper", 8.5, 11, 1, 300},
    {"A4", 210, 297, 3, 300},
    {"Photo 4x6", 4, 6, 1, 300},
    {"Photo 5x7", 5, 7, 1, 300},
    {"Web 1920x1080", 1920, 1080, 0, 72},
    {"Web 1280x720", 1280, 720, 0, 72},
    {"Web 800x600", 800, 600, 0, 72},
    {"1024x768", 1024, 768, 0, 72},
    {"640x480", 640, 480, 0, 72},
};

QString formatSize(qint64 bytes)
{
    if (bytes < 1024)
        return QString::number(bytes) + QStringLiteral(" B");
    if (bytes < 1024 * 1024)
        return QString::number(bytes / 1024.0, 'f', 2) + QStringLiteral("K");
    return QString::number(bytes / (1024.0 * 1024.0), 'f', 2) + QStringLiteral("M");
}

} // namespace

NewDocumentDialog::NewDocumentDialog(int documentNumber, QWidget *parent)
    : QDialog(parent)
    , m_documentNumber(documentNumber)
{
    setWindowTitle(tr("New"));
    buildUi();
    updateImageSize();
}

void NewDocumentDialog::buildUi()
{
    auto *outerLayout = new QHBoxLayout(this);

    // -- Left: form fields --
    auto *formLayout = new QVBoxLayout;

    // Name
    auto *nameRow = new QHBoxLayout;
    nameRow->addWidget(new QLabel(tr("Name:"), this));
    m_nameEdit = new QLineEdit(this);
    m_nameEdit->setText(QStringLiteral("Untitled-%1").arg(m_documentNumber));
    m_nameEdit->setMinimumWidth(220);
    nameRow->addWidget(m_nameEdit, 1);
    formLayout->addLayout(nameRow);

    formLayout->addSpacing(4);

    // Document Type
    auto *docTypeRow = new QHBoxLayout;
    docTypeRow->addWidget(new QLabel(tr("Document Type:"), this));
    m_docTypeCombo = new QComboBox(this);
    m_docTypeCombo->addItem(tr("Custom"));
    m_docTypeCombo->addItem(tr("Clipboard"));
    m_docTypeCombo->addItem(tr("Default Photoshop Size"));
    m_docTypeCombo->addItem(tr("U.S. Paper"));
    m_docTypeCombo->addItem(tr("International Paper"));
    m_docTypeCombo->addItem(tr("Photo"));
    m_docTypeCombo->addItem(tr("Web"));
    docTypeRow->addWidget(m_docTypeCombo, 1);
    formLayout->addLayout(docTypeRow);

    // Size preset
    auto *sizeRow = new QHBoxLayout;
    sizeRow->addWidget(new QLabel(tr("Size:"), this));
    m_sizePresetCombo = new QComboBox(this);
    for (const auto &preset : kPresets)
        m_sizePresetCombo->addItem(QString::fromUtf8(preset.name));
    sizeRow->addWidget(m_sizePresetCombo, 1);
    formLayout->addLayout(sizeRow);

    formLayout->addSpacing(8);

    // Width
    auto *grid = new QGridLayout;
    grid->setColumnStretch(1, 1);

    grid->addWidget(new QLabel(tr("Width:"), this), 0, 0);
    m_widthSpin = new QDoubleSpinBox(this);
    m_widthSpin->setRange(1, 300000);
    m_widthSpin->setDecimals(2);
    m_widthSpin->setValue(1280);
    grid->addWidget(m_widthSpin, 0, 1);

    m_unitCombo = new QComboBox(this);
    m_unitCombo->addItem(tr("Pixels"));
    m_unitCombo->addItem(tr("Inches"));
    m_unitCombo->addItem(tr("Centimeters"));
    m_unitCombo->addItem(tr("Millimeters"));
    m_unitCombo->addItem(tr("Points"));
    m_unitCombo->addItem(tr("Picas"));
    m_unitCombo->addItem(tr("Columns"));
    grid->addWidget(m_unitCombo, 0, 2);

    // Height
    grid->addWidget(new QLabel(tr("Height:"), this), 1, 0);
    m_heightSpin = new QDoubleSpinBox(this);
    m_heightSpin->setRange(1, 300000);
    m_heightSpin->setDecimals(2);
    m_heightSpin->setValue(800);
    grid->addWidget(m_heightSpin, 1, 1);

    // Resolution
    grid->addWidget(new QLabel(tr("Resolution:"), this), 2, 0);
    m_resolutionSpin = new QSpinBox(this);
    m_resolutionSpin->setRange(1, 9999);
    m_resolutionSpin->setValue(72);
    grid->addWidget(m_resolutionSpin, 2, 1);

    m_resUnitCombo = new QComboBox(this);
    m_resUnitCombo->addItem(tr("Pixels/Inch"));
    m_resUnitCombo->addItem(tr("Pixels/Centimeter"));
    grid->addWidget(m_resUnitCombo, 2, 2);

    // Color Mode
    grid->addWidget(new QLabel(tr("Color Mode:"), this), 3, 0);
    m_colorModeCombo = new QComboBox(this);
    m_colorModeCombo->addItem(tr("Bitmap"));
    m_colorModeCombo->addItem(tr("Grayscale"));
    m_colorModeCombo->addItem(tr("RGB Color"));
    m_colorModeCombo->addItem(tr("CMYK Color"));
    m_colorModeCombo->addItem(tr("Lab Color"));
    m_colorModeCombo->setCurrentIndex(2);
    grid->addWidget(m_colorModeCombo, 3, 1);

    m_bitDepthCombo = new QComboBox(this);
    m_bitDepthCombo->addItem(tr("1 bit"));
    m_bitDepthCombo->addItem(tr("8 bit"));
    m_bitDepthCombo->addItem(tr("16 bit"));
    m_bitDepthCombo->addItem(tr("32 bit"));
    m_bitDepthCombo->setCurrentIndex(1);
    grid->addWidget(m_bitDepthCombo, 3, 2);

    // Background Contents
    grid->addWidget(new QLabel(tr("Background Contents:"), this), 4, 0);
    m_backgroundCombo = new QComboBox(this);
    m_backgroundCombo->addItem(tr("White"));
    m_backgroundCombo->addItem(tr("Black"));
    m_backgroundCombo->addItem(tr("Background Color"));
    m_backgroundCombo->addItem(tr("Transparent"));
    m_backgroundCombo->addItem(tr("Custom..."));
    grid->addWidget(m_backgroundCombo, 4, 1);

    formLayout->addLayout(grid);

    formLayout->addSpacing(12);

    // Advanced
    auto *advancedGroup = new QGroupBox(tr("Advanced"), this);
    auto *advLayout = new QGridLayout(advancedGroup);

    advLayout->addWidget(new QLabel(tr("Color Profile:"), advancedGroup), 0, 0);
    m_colorProfileCombo = new QComboBox(advancedGroup);
    m_colorProfileCombo->addItem(tr("Don't Color Manage this Document"));
    m_colorProfileCombo->addItem(tr("Working RGB:  sRGB IEC61966-2.1"));
    m_colorProfileCombo->insertSeparator(m_colorProfileCombo->count());
    m_colorProfileCombo->addItem(tr("Adobe RGB (1998)"));
    m_colorProfileCombo->addItem(tr("Apple RGB"));
    m_colorProfileCombo->addItem(tr("ColorMatch RGB"));
    m_colorProfileCombo->addItem(tr("ProPhoto RGB"));
    m_colorProfileCombo->addItem(tr("sRGB IEC61966-2.1"));
    m_colorProfileCombo->insertSeparator(m_colorProfileCombo->count());
    m_colorProfileCombo->addItem(tr("CIE RGB"));
    m_colorProfileCombo->addItem(tr("e-sRGB"));
    m_colorProfileCombo->addItem(tr("HDTV (Rec. 709)"));
    m_colorProfileCombo->addItem(tr("PAL/SECAM"));
    m_colorProfileCombo->addItem(tr("ROMM-RGB"));
    m_colorProfileCombo->addItem(tr("SDTV NTSC"));
    m_colorProfileCombo->addItem(tr("SDTV PAL"));
    m_colorProfileCombo->addItem(tr("SMPTE-C"));
    m_colorProfileCombo->addItem(tr("Wide Gamut RGB"));
    m_colorProfileCombo->setCurrentIndex(1);
    advLayout->addWidget(m_colorProfileCombo, 0, 1);

    advLayout->addWidget(new QLabel(tr("Pixel Aspect Ratio:"), advancedGroup), 1, 0);
    m_pixelAspectCombo = new QComboBox(advancedGroup);
    m_pixelAspectCombo->addItem(tr("Square Pixels"));
    m_pixelAspectCombo->addItem(tr("D1/DV NTSC (0.91)"));
    m_pixelAspectCombo->addItem(tr("D1/DV PAL (1.09)"));
    m_pixelAspectCombo->addItem(tr("D1/DV NTSC Widescreen (1.21)"));
    m_pixelAspectCombo->addItem(tr("HDV 1080/DVCPRO HD 720 (1.33)"));
    m_pixelAspectCombo->addItem(tr("D1/DV PAL Widescreen (1.46)"));
    m_pixelAspectCombo->addItem(tr("Anamorphic 2:1 (2)"));
    m_pixelAspectCombo->addItem(tr("DVCPRO HD 1080 (1.5)"));
    advLayout->addWidget(m_pixelAspectCombo, 1, 1);

    formLayout->addWidget(advancedGroup);
    formLayout->addStretch();

    outerLayout->addLayout(formLayout, 1);

    // -- Right: buttons and image size --
    auto *rightPanel = new QVBoxLayout;

    auto *okButton = new QPushButton(tr("OK"), this);
    okButton->setDefault(true);
    okButton->setFixedWidth(100);
    rightPanel->addWidget(okButton);

    auto *cancelButton = new QPushButton(tr("Cancel"), this);
    cancelButton->setFixedWidth(100);
    rightPanel->addWidget(cancelButton);

    rightPanel->addSpacing(8);

    auto *savePresetButton = new QPushButton(tr("Save Preset..."), this);
    savePresetButton->setFixedWidth(100);
    savePresetButton->setEnabled(false);
    rightPanel->addWidget(savePresetButton);

    auto *deletePresetButton = new QPushButton(tr("Delete Preset..."), this);
    deletePresetButton->setFixedWidth(100);
    deletePresetButton->setEnabled(false);
    rightPanel->addWidget(deletePresetButton);

    rightPanel->addStretch();

    auto *sizeHeader = new QLabel(tr("Image Size:"), this);
    sizeHeader->setStyleSheet(QStringLiteral("QLabel { font-weight: bold; }"));
    rightPanel->addWidget(sizeHeader);

    m_imageSizeLabel = new QLabel(this);
    rightPanel->addWidget(m_imageSizeLabel);

    outerLayout->addLayout(rightPanel);

    // Connections
    connect(okButton, &QPushButton::clicked, this, &QDialog::accept);
    connect(cancelButton, &QPushButton::clicked, this, &QDialog::reject);
    connect(m_unitCombo, &QComboBox::currentIndexChanged, this,
            &NewDocumentDialog::onUnitChanged);
    connect(m_sizePresetCombo, &QComboBox::currentIndexChanged, this,
            &NewDocumentDialog::onPresetChanged);
    connect(m_widthSpin, &QDoubleSpinBox::valueChanged, this,
            &NewDocumentDialog::onDimensionChanged);
    connect(m_heightSpin, &QDoubleSpinBox::valueChanged, this,
            &NewDocumentDialog::onDimensionChanged);
    connect(m_resolutionSpin, &QSpinBox::valueChanged, this,
            &NewDocumentDialog::onDimensionChanged);
    connect(m_bitDepthCombo, &QComboBox::currentIndexChanged, this,
            &NewDocumentDialog::onDimensionChanged);
    connect(m_colorModeCombo, &QComboBox::currentIndexChanged, this, [this](int index) {
        // Bitmap mode only supports 1 bit.
        if (index == 0) {
            m_bitDepthCombo->setCurrentIndex(0);
        } else if (m_bitDepthCombo->currentIndex() == 0) {
            m_bitDepthCombo->setCurrentIndex(1);
        }
        updateImageSize();
    });
}

void NewDocumentDialog::onUnitChanged(int index)
{
    if (m_updatingDimensions)
        return;
    m_updatingDimensions = true;

    // Convert current pixel values to the new unit for display.
    const double widthPx = widthPixels();
    const double heightPx = heightPixels();

    if (index == 0) {
        // Pixels
        m_widthSpin->setDecimals(0);
        m_heightSpin->setDecimals(0);
    } else {
        m_widthSpin->setDecimals(2);
        m_heightSpin->setDecimals(2);
    }

    // Temporarily set the unit to new value so fromPixels uses it.
    m_widthSpin->setValue(fromPixels(widthPx));
    m_heightSpin->setValue(fromPixels(heightPx));

    m_updatingDimensions = false;
    updateImageSize();
}

void NewDocumentDialog::onPresetChanged(int index)
{
    if (index < 0 || index >= static_cast<int>(std::size(kPresets)))
        return;

    m_updatingDimensions = true;
    const SizePreset &preset = kPresets[index];

    m_unitCombo->setCurrentIndex(preset.unit);
    m_resolutionSpin->setValue(preset.resolution);

    if (preset.unit == 0) {
        m_widthSpin->setDecimals(0);
        m_heightSpin->setDecimals(0);
    } else {
        m_widthSpin->setDecimals(2);
        m_heightSpin->setDecimals(2);
    }
    m_widthSpin->setValue(preset.width);
    m_heightSpin->setValue(preset.height);

    m_updatingDimensions = false;
    updateImageSize();
}

void NewDocumentDialog::onDimensionChanged()
{
    if (m_updatingDimensions)
        return;
    updateImageSize();
}

double NewDocumentDialog::toPixels(double value) const
{
    const int unit = m_unitCombo->currentIndex();
    const double dpi = m_resolutionSpin->value();
    switch (unit) {
    case 0: return value;                       // Pixels
    case 1: return value * dpi;                 // Inches
    case 2: return value * dpi / 2.54;          // Centimeters
    case 3: return value * dpi / 25.4;          // Millimeters
    case 4: return value * dpi / 72.0;          // Points
    case 5: return value * dpi / 6.0;           // Picas
    case 6: return value * dpi * 2.0;           // Columns (approx 2 inch)
    }
    return value;
}

double NewDocumentDialog::fromPixels(double px) const
{
    const int unit = m_unitCombo->currentIndex();
    const double dpi = m_resolutionSpin->value();
    switch (unit) {
    case 0: return px;
    case 1: return px / dpi;
    case 2: return px * 2.54 / dpi;
    case 3: return px * 25.4 / dpi;
    case 4: return px * 72.0 / dpi;
    case 5: return px * 6.0 / dpi;
    case 6: return px / (dpi * 2.0);
    }
    return px;
}

QString NewDocumentDialog::documentName() const
{
    return m_nameEdit->text();
}

int NewDocumentDialog::widthPixels() const
{
    return qMax(1, static_cast<int>(std::round(toPixels(m_widthSpin->value()))));
}

int NewDocumentDialog::heightPixels() const
{
    return qMax(1, static_cast<int>(std::round(toPixels(m_heightSpin->value()))));
}

int NewDocumentDialog::backgroundFill() const
{
    // Map the combo entries to the engine's fill indices:
    // 0=White, 1=Transparent, 2=Background Color
    switch (m_backgroundCombo->currentIndex()) {
    case 0: return 0;   // White
    case 1: return 0;   // Black → white (engine only has white/transparent/bg)
    case 2: return 2;   // Background Color
    case 3: return 1;   // Transparent
    case 4: return 0;   // Custom → white fallback
    }
    return 0;
}

void NewDocumentDialog::accept()
{
    constexpr int kLargeThreshold = 30000;
    const int w = widthPixels();
    const int h = heightPixels();

    if (w > kLargeThreshold || h > kLargeThreshold) {
        QMessageBox box(QMessageBox::Warning, tr("New"),
                        tr("Documents greater than 30,000 pixels in either dimension may\n"
                           "not be compatible with older versions of Photoshop and/or\n"
                           "other applications."),
                        QMessageBox::NoButton, this);
        auto *cont = box.addButton(tr("Continue"), QMessageBox::AcceptRole);
        box.addButton(QMessageBox::Cancel);
        box.exec();
        if (box.clickedButton() != cont)
            return;
    }

    QDialog::accept();
}

void NewDocumentDialog::updateImageSize()
{
    const int w = widthPixels();
    const int h = heightPixels();

    int bitsPerPixel = 8;
    const int bitIndex = m_bitDepthCombo->currentIndex();
    const int bits[] = {1, 8, 16, 32};
    if (bitIndex >= 0 && bitIndex < 4)
        bitsPerPixel = bits[bitIndex];

    int channels = 3;
    switch (m_colorModeCombo->currentIndex()) {
    case 0: channels = 1; break;  // Bitmap
    case 1: channels = 1; break;  // Grayscale
    case 2: channels = 3; break;  // RGB
    case 3: channels = 4; break;  // CMYK
    case 4: channels = 3; break;  // Lab
    }

    const qint64 bytes = static_cast<qint64>(w) * h * channels * bitsPerPixel / 8;
    m_imageSizeLabel->setText(formatSize(bytes));
}

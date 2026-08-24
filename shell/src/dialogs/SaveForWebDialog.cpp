#include "SaveForWebDialog.h"
#include "GifWriter.h"

#include <QBuffer>
#include <QCheckBox>
#include <QComboBox>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QPainter>
#include <QPushButton>
#include <QSpinBox>
#include <QTabBar>
#include <QVBoxLayout>

namespace {

QString formatSizeString(qint64 bytes)
{
    if (bytes < 1024)
        return QString::number(bytes) + QStringLiteral(" B");
    if (bytes < 1024 * 1024)
        return QString::number(bytes / 1024.0, 'f', 1) + QStringLiteral(" KB");
    return QString::number(bytes / (1024.0 * 1024.0), 'f', 1) + QStringLiteral(" MB");
}

constexpr int kPreviewMaxSize = 500;

} // namespace

SaveForWebDialog::SaveForWebDialog(const QImage &image, QWidget *parent)
    : QDialog(parent)
    , m_original(image)
    , m_originalWidth(image.width())
    , m_originalHeight(image.height())
{
    setWindowTitle(tr("Save for Web"));
    setMinimumSize(900, 650);
    resize(1050, 750);
    buildUi();
    onFormatChanged(m_formatCombo->currentIndex());
    updatePreview();
}

void SaveForWebDialog::buildUi()
{
    auto *outerLayout = new QVBoxLayout(this);

    // Top hint
    auto *hint = new QLabel(
        tr("Tip: Use File > Export > Export As... or right click on a layer for a "
           "faster way to export assets"),
        this);
    hint->setWordWrap(true);
    hint->setStyleSheet(QStringLiteral("QLabel { color: #888; padding: 4px; }"));
    outerLayout->addWidget(hint);

    auto *mainLayout = new QHBoxLayout;

    // -- Left: preview area --
    auto *previewLayout = new QVBoxLayout;

    m_previewTabs = new QTabBar(this);
    m_previewTabs->addTab(tr("Original"));
    m_previewTabs->addTab(tr("Optimized"));
    m_previewTabs->addTab(tr("2-Up"));
    m_previewTabs->addTab(tr("4-Up"));
    m_previewTabs->setCurrentIndex(1);
    previewLayout->addWidget(m_previewTabs);

    m_previewLabel = new QLabel(this);
    m_previewLabel->setMinimumSize(400, 400);
    m_previewLabel->setAlignment(Qt::AlignCenter);
    m_previewLabel->setStyleSheet(
        QStringLiteral("QLabel { background: #f0f0f0; border: 1px solid #ccc; }"));
    previewLayout->addWidget(m_previewLabel, 1);

    m_previewInfoLabel = new QLabel(this);
    m_previewInfoLabel->setStyleSheet(
        QStringLiteral("QLabel { color: #666; font-size: 11px; padding: 2px; }"));
    previewLayout->addWidget(m_previewInfoLabel);

    mainLayout->addLayout(previewLayout, 1);

    // -- Right: settings --
    auto *rightPanel = new QVBoxLayout;

    // Preset
    auto *presetRow = new QHBoxLayout;
    presetRow->addWidget(new QLabel(tr("Preset:"), this));
    m_presetCombo = new QComboBox(this);
    m_presetCombo->addItem(QStringLiteral("[Unnamed]"));
    m_presetCombo->addItem(QStringLiteral("GIF 128 Dithered"));
    m_presetCombo->addItem(QStringLiteral("GIF 128 No Dither"));
    m_presetCombo->addItem(QStringLiteral("GIF 32 Dithered"));
    m_presetCombo->addItem(QStringLiteral("GIF 32 No Dither"));
    m_presetCombo->addItem(QStringLiteral("GIF 64 Dithered"));
    m_presetCombo->addItem(QStringLiteral("GIF 64 No Dither"));
    m_presetCombo->addItem(QStringLiteral("JPEG High"));
    m_presetCombo->addItem(QStringLiteral("JPEG Low"));
    m_presetCombo->addItem(QStringLiteral("JPEG Medium"));
    m_presetCombo->addItem(QStringLiteral("PNG-8 128 Dithered"));
    m_presetCombo->addItem(QStringLiteral("PNG-24"));
    presetRow->addWidget(m_presetCombo);
    rightPanel->addLayout(presetRow);

    // Format
    m_formatCombo = new QComboBox(this);
    m_formatCombo->addItem(QStringLiteral("GIF"), static_cast<int>(GIF));
    m_formatCombo->addItem(QStringLiteral("JPEG"), static_cast<int>(JPEG));
    m_formatCombo->addItem(QStringLiteral("PNG-8"), static_cast<int>(PNG8));
    m_formatCombo->addItem(QStringLiteral("PNG-24"), static_cast<int>(PNG24));
    m_formatCombo->addItem(QStringLiteral("WBMP"), static_cast<int>(WBMP));
    rightPanel->addWidget(m_formatCombo);

    // -- GIF options --
    auto *gifRow1 = new QHBoxLayout;
    m_gifColorReduction = new QComboBox(this);
    m_gifColorReduction->addItem(tr("Selective"));
    m_gifColorReduction->addItem(tr("Perceptual"));
    m_gifColorReduction->addItem(tr("Adaptive"));
    m_gifColorReduction->addItem(tr("Restrictive (Web)"));
    gifRow1->addWidget(m_gifColorReduction);
    m_gifColorsLabel = new QLabel(tr("Colors:"), this);
    gifRow1->addWidget(m_gifColorsLabel);
    m_gifColors = new QSpinBox(this);
    m_gifColors->setRange(2, 256);
    m_gifColors->setValue(256);
    gifRow1->addWidget(m_gifColors);
    rightPanel->addLayout(gifRow1);

    auto *gifRow2 = new QHBoxLayout;
    m_gifDitherLabel = new QLabel(tr("Dither:"), this);
    gifRow2->addWidget(m_gifDitherLabel);
    m_gifDither = new QComboBox(this);
    m_gifDither->addItem(tr("Diffusion"));
    m_gifDither->addItem(tr("Pattern"));
    m_gifDither->addItem(tr("Noise"));
    m_gifDither->addItem(tr("No Dither"));
    gifRow2->addWidget(m_gifDither);
    m_gifDitherAmountLabel = new QLabel(tr("Dither:"), this);
    gifRow2->addWidget(m_gifDitherAmountLabel);
    m_gifDitherAmount = new QSpinBox(this);
    m_gifDitherAmount->setRange(0, 100);
    m_gifDitherAmount->setValue(100);
    m_gifDitherAmount->setSuffix(QStringLiteral("%"));
    gifRow2->addWidget(m_gifDitherAmount);
    rightPanel->addLayout(gifRow2);

    m_gifTransparency = new QCheckBox(tr("Transparency"), this);
    m_gifTransparency->setChecked(true);
    rightPanel->addWidget(m_gifTransparency);

    m_gifInterlaced = new QCheckBox(tr("Interlaced"), this);
    rightPanel->addWidget(m_gifInterlaced);

    // -- JPEG options --
    auto *jpegRow1 = new QHBoxLayout;
    m_jpegQualityPresetLabel = new QLabel(tr("Compression Quality:"), this);
    jpegRow1->addWidget(m_jpegQualityPresetLabel);
    m_jpegQualityPreset = new QComboBox(this);
    m_jpegQualityPreset->addItem(tr("Low"));
    m_jpegQualityPreset->addItem(tr("Medium"));
    m_jpegQualityPreset->addItem(tr("High"));
    m_jpegQualityPreset->addItem(tr("Very High"));
    m_jpegQualityPreset->addItem(tr("Maximum"));
    m_jpegQualityPreset->setCurrentIndex(2);
    jpegRow1->addWidget(m_jpegQualityPreset);
    rightPanel->addLayout(jpegRow1);

    auto *jpegRow2 = new QHBoxLayout;
    m_jpegQualityLabel = new QLabel(tr("Quality:"), this);
    jpegRow2->addWidget(m_jpegQualityLabel);
    m_jpegQuality = new QSpinBox(this);
    m_jpegQuality->setRange(1, 100);
    m_jpegQuality->setValue(60);
    jpegRow2->addWidget(m_jpegQuality);
    rightPanel->addLayout(jpegRow2);

    m_jpegProgressive = new QCheckBox(tr("Progressive"), this);
    rightPanel->addWidget(m_jpegProgressive);

    m_jpegOptimized = new QCheckBox(tr("Optimized"), this);
    m_jpegOptimized->setChecked(true);
    rightPanel->addWidget(m_jpegOptimized);

    // -- PNG options --
    m_pngTransparency = new QCheckBox(tr("Transparency"), this);
    m_pngTransparency->setChecked(true);
    rightPanel->addWidget(m_pngTransparency);

    m_pngInterlaced = new QCheckBox(tr("Interlaced"), this);
    rightPanel->addWidget(m_pngInterlaced);

    // -- Common options --
    rightPanel->addSpacing(8);
    m_convertSrgb = new QCheckBox(tr("Convert to sRGB"), this);
    m_convertSrgb->setChecked(true);
    rightPanel->addWidget(m_convertSrgb);

    auto *metaRow = new QHBoxLayout;
    metaRow->addWidget(new QLabel(tr("Metadata:"), this));
    m_metadataCombo = new QComboBox(this);
    m_metadataCombo->addItem(tr("None"));
    m_metadataCombo->addItem(tr("Copyright and Contact Info"));
    m_metadataCombo->addItem(tr("All"));
    m_metadataCombo->setCurrentIndex(1);
    metaRow->addWidget(m_metadataCombo);
    rightPanel->addLayout(metaRow);

    // -- Image Size --
    rightPanel->addSpacing(12);
    auto *sizeGroup = new QGroupBox(tr("Image Size"), this);
    auto *sizeLayout = new QVBoxLayout(sizeGroup);

    auto *swRow = new QHBoxLayout;
    swRow->addWidget(new QLabel(tr("W:"), this));
    m_imageSizeWidth = new QSpinBox(this);
    m_imageSizeWidth->setRange(1, 30000);
    m_imageSizeWidth->setValue(m_originalWidth);
    m_imageSizeWidth->setSuffix(QStringLiteral(" px"));
    swRow->addWidget(m_imageSizeWidth);
    swRow->addWidget(new QLabel(tr("Percent:"), this));
    m_imageSizePercent = new QSpinBox(this);
    m_imageSizePercent->setRange(1, 1000);
    m_imageSizePercent->setValue(100);
    m_imageSizePercent->setSuffix(QStringLiteral(" %"));
    swRow->addWidget(m_imageSizePercent);
    sizeLayout->addLayout(swRow);

    auto *shRow = new QHBoxLayout;
    shRow->addWidget(new QLabel(tr("H:"), this));
    m_imageSizeHeight = new QSpinBox(this);
    m_imageSizeHeight->setRange(1, 30000);
    m_imageSizeHeight->setValue(m_originalHeight);
    m_imageSizeHeight->setSuffix(QStringLiteral(" px"));
    shRow->addWidget(m_imageSizeHeight);
    shRow->addWidget(new QLabel(tr("Quality:"), this));
    m_imageSizeResample = new QComboBox(this);
    m_imageSizeResample->addItem(tr("Bicubic"));
    m_imageSizeResample->addItem(tr("Bilinear"));
    m_imageSizeResample->addItem(tr("Nearest Neighbor"));
    shRow->addWidget(m_imageSizeResample);
    sizeLayout->addLayout(shRow);

    rightPanel->addWidget(sizeGroup);

    // File info
    m_fileInfoLabel = new QLabel(this);
    m_fileInfoLabel->setStyleSheet(
        QStringLiteral("QLabel { color: #666; font-size: 11px; }"));
    rightPanel->addWidget(m_fileInfoLabel);

    rightPanel->addStretch();
    mainLayout->addLayout(rightPanel);

    outerLayout->addLayout(mainLayout, 1);

    // -- Bottom buttons --
    auto *buttons = new QHBoxLayout;
    auto *previewButton = new QPushButton(tr("Preview..."), this);
    previewButton->setEnabled(false);
    buttons->addWidget(previewButton);
    buttons->addStretch();
    auto *saveButton = new QPushButton(tr("Save..."), this);
    saveButton->setDefault(true);
    auto *cancelButton = new QPushButton(tr("Cancel"), this);
    auto *doneButton = new QPushButton(tr("Done"), this);
    buttons->addWidget(saveButton);
    buttons->addWidget(cancelButton);
    buttons->addWidget(doneButton);
    outerLayout->addLayout(buttons);

    // Connections
    connect(m_formatCombo, &QComboBox::currentIndexChanged, this,
            &SaveForWebDialog::onFormatChanged);
    connect(m_previewTabs, &QTabBar::currentChanged, this,
            &SaveForWebDialog::onPreviewTabChanged);
    connect(m_gifColors, &QSpinBox::valueChanged, this, &SaveForWebDialog::onSettingsChanged);
    connect(m_gifDither, &QComboBox::currentIndexChanged, this,
            &SaveForWebDialog::onSettingsChanged);
    connect(m_gifDitherAmount, &QSpinBox::valueChanged, this,
            &SaveForWebDialog::onSettingsChanged);
    connect(m_gifTransparency, &QCheckBox::toggled, this, &SaveForWebDialog::onSettingsChanged);
    connect(m_jpegQuality, &QSpinBox::valueChanged, this, &SaveForWebDialog::onSettingsChanged);
    connect(m_jpegQualityPreset, &QComboBox::currentIndexChanged, this, [this](int index) {
        const int qualities[] = {10, 30, 60, 80, 100};
        if (index >= 0 && index < 5)
            m_jpegQuality->setValue(qualities[index]);
    });
    connect(m_pngTransparency, &QCheckBox::toggled, this, &SaveForWebDialog::onSettingsChanged);
    connect(m_imageSizeWidth, &QSpinBox::valueChanged, this,
            &SaveForWebDialog::onImageWidthChanged);
    connect(m_imageSizeHeight, &QSpinBox::valueChanged, this,
            &SaveForWebDialog::onImageHeightChanged);
    connect(m_imageSizePercent, &QSpinBox::valueChanged, this,
            &SaveForWebDialog::onPercentChanged);
    connect(saveButton, &QPushButton::clicked, this, &QDialog::accept);
    connect(cancelButton, &QPushButton::clicked, this, &QDialog::reject);
    connect(doneButton, &QPushButton::clicked, this, &QDialog::reject);

    connect(m_presetCombo, &QComboBox::currentIndexChanged, this, [this](int index) {
        if (index <= 0)
            return;
        const QString name = m_presetCombo->currentText();
        if (name.startsWith(QStringLiteral("GIF"))) {
            m_formatCombo->setCurrentIndex(0);
            if (name.contains(QStringLiteral("128")))
                m_gifColors->setValue(128);
            else if (name.contains(QStringLiteral("32")))
                m_gifColors->setValue(32);
            else if (name.contains(QStringLiteral("64")))
                m_gifColors->setValue(64);
            m_gifDither->setCurrentIndex(name.contains(QStringLiteral("No Dither")) ? 3 : 0);
        } else if (name.startsWith(QStringLiteral("JPEG"))) {
            m_formatCombo->setCurrentIndex(1);
            if (name.contains(QStringLiteral("High")))
                m_jpegQuality->setValue(60);
            else if (name.contains(QStringLiteral("Low")))
                m_jpegQuality->setValue(10);
            else if (name.contains(QStringLiteral("Medium")))
                m_jpegQuality->setValue(30);
        } else if (name.startsWith(QStringLiteral("PNG-8"))) {
            m_formatCombo->setCurrentIndex(2);
            m_gifColors->setValue(128);
        } else if (name.startsWith(QStringLiteral("PNG-24"))) {
            m_formatCombo->setCurrentIndex(3);
        }
    });
}

void SaveForWebDialog::onFormatChanged(int index)
{
    Q_UNUSED(index)
    updateFormatOptions();
    updatePreview();
    updateFileInfo();
}

void SaveForWebDialog::updateFormatOptions()
{
    const Format fmt = chosenFormat();
    const bool isGif = (fmt == GIF);
    const bool isJpeg = (fmt == JPEG);
    const bool isPng8 = (fmt == PNG8);
    const bool isPng24 = (fmt == PNG24);
    const bool isWbmp = (fmt == WBMP);

    // GIF options (also used by PNG-8)
    m_gifColorReduction->setVisible(isGif || isPng8);
    m_gifColorsLabel->setVisible(isGif || isPng8);
    m_gifColors->setVisible(isGif || isPng8);
    m_gifDitherLabel->setVisible(isGif || isPng8);
    m_gifDither->setVisible(isGif || isPng8);
    m_gifDitherAmountLabel->setVisible(isGif || isPng8);
    m_gifDitherAmount->setVisible(isGif || isPng8);
    m_gifTransparency->setVisible(isGif || isPng8);
    m_gifInterlaced->setVisible(isGif);

    // JPEG options
    m_jpegQualityPresetLabel->setVisible(isJpeg);
    m_jpegQualityPreset->setVisible(isJpeg);
    m_jpegQualityLabel->setVisible(isJpeg);
    m_jpegQuality->setVisible(isJpeg);
    m_jpegProgressive->setVisible(isJpeg);
    m_jpegOptimized->setVisible(isJpeg);

    // PNG-24 options
    m_pngTransparency->setVisible(isPng24);
    m_pngInterlaced->setVisible(isPng24 || isWbmp);
}

void SaveForWebDialog::onPreviewTabChanged(int /*index*/)
{
    updatePreview();
}

void SaveForWebDialog::onSettingsChanged()
{
    m_presetCombo->setCurrentIndex(0);
    updatePreview();
    updateFileInfo();
}

void SaveForWebDialog::onImageWidthChanged(int value)
{
    if (m_updatingSize)
        return;
    m_updatingSize = true;
    const double ratio = static_cast<double>(m_originalHeight) / m_originalWidth;
    m_imageSizeHeight->setValue(qRound(value * ratio));
    m_imageSizePercent->setValue(qRound(100.0 * value / m_originalWidth));
    m_updatingSize = false;
    updatePreview();
    updateFileInfo();
}

void SaveForWebDialog::onImageHeightChanged(int value)
{
    if (m_updatingSize)
        return;
    m_updatingSize = true;
    const double ratio = static_cast<double>(m_originalWidth) / m_originalHeight;
    m_imageSizeWidth->setValue(qRound(value * ratio));
    m_imageSizePercent->setValue(qRound(100.0 * value / m_originalHeight));
    m_updatingSize = false;
    updatePreview();
    updateFileInfo();
}

void SaveForWebDialog::onPercentChanged(int value)
{
    if (m_updatingSize)
        return;
    m_updatingSize = true;
    m_imageSizeWidth->setValue(qRound(m_originalWidth * value / 100.0));
    m_imageSizeHeight->setValue(qRound(m_originalHeight * value / 100.0));
    m_updatingSize = false;
    updatePreview();
    updateFileInfo();
}

SaveForWebDialog::Format SaveForWebDialog::chosenFormat() const
{
    return static_cast<Format>(m_formatCombo->currentData().toInt());
}

int SaveForWebDialog::jpegQuality() const
{
    return m_jpegQuality->value();
}

QString SaveForWebDialog::fileExtension() const
{
    switch (chosenFormat()) {
    case GIF:
        return QStringLiteral("gif");
    case JPEG:
        return QStringLiteral("jpg");
    case PNG8:
    case PNG24:
        return QStringLiteral("png");
    case WBMP:
        return QStringLiteral("wbmp");
    }
    return QStringLiteral("png");
}

QImage SaveForWebDialog::sizedImage() const
{
    const int w = m_imageSizeWidth->value();
    const int h = m_imageSizeHeight->value();
    if (w == m_originalWidth && h == m_originalHeight)
        return m_original;

    Qt::TransformationMode mode = Qt::SmoothTransformation;
    if (m_imageSizeResample->currentIndex() == 2)
        mode = Qt::FastTransformation;

    return m_original.scaled(w, h, Qt::IgnoreAspectRatio, mode);
}

QImage SaveForWebDialog::optimizedImage() const
{
    QImage img = sizedImage();
    const Format fmt = chosenFormat();

    switch (fmt) {
    case JPEG: {
        QImage flat(img.size(), QImage::Format_RGB32);
        flat.fill(Qt::white);
        QPainter p(&flat);
        p.drawImage(0, 0, img);
        return flat;
    }
    case GIF:
    case PNG8:
        return img.convertToFormat(QImage::Format_Indexed8);
    case WBMP:
        return img.convertToFormat(QImage::Format_Mono);
    case PNG24:
        if (!m_pngTransparency->isChecked()) {
            QImage flat(img.size(), QImage::Format_RGB32);
            flat.fill(Qt::white);
            QPainter p(&flat);
            p.drawImage(0, 0, img);
            return flat;
        }
        return img;
    }
    return img;
}

QImage SaveForWebDialog::exportImage() const
{
    return optimizedImage();
}

void SaveForWebDialog::updatePreview()
{
    const int tabIndex = m_previewTabs->currentIndex();
    QImage preview;

    if (tabIndex == 0) {
        preview = sizedImage();
    } else {
        preview = optimizedImage();
    }

    const int maxDim = qMin(m_previewLabel->width(), m_previewLabel->height());
    const int previewSize = qMax(qMin(maxDim, kPreviewMaxSize), 100);

    if (preview.width() > previewSize || preview.height() > previewSize) {
        preview = preview.scaled(previewSize, previewSize, Qt::KeepAspectRatio,
                                 Qt::SmoothTransformation);
    }

    m_previewLabel->setPixmap(QPixmap::fromImage(preview));
}

void SaveForWebDialog::updateFileInfo()
{
    QImage img = optimizedImage();
    QByteArray data;
    QBuffer buffer(&data);
    buffer.open(QIODevice::WriteOnly);

    const Format fmt = chosenFormat();
    switch (fmt) {
    case GIF: {
        QByteArray gifData = encodeGif(img);
        data = gifData;
        break;
    }
    case JPEG:
        img.save(&buffer, "JPEG", m_jpegQuality->value());
        break;
    case PNG8:
    case PNG24:
        img.save(&buffer, "PNG");
        break;
    case WBMP:
        img.save(&buffer, "BMP");
        break;
    }

    const double kbps = data.size() * 8.0 / 1000.0;
    const double seconds56k = kbps / 56.0;
    m_previewInfoLabel->setText(QStringLiteral("%1\n%2    %3 x %4\n%5 sec @ 56.6 Kbps")
                                    .arg(m_formatCombo->currentText(),
                                         formatSizeString(data.size()))
                                    .arg(m_imageSizeWidth->value())
                                    .arg(m_imageSizeHeight->value())
                                    .arg(seconds56k, 0, 'f', 1));
}

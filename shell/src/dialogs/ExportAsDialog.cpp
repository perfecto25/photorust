#include "ExportAsDialog.h"

#include <QBuffer>
#include <QCheckBox>
#include <QComboBox>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QPainter>
#include <QPushButton>
#include <QSpinBox>
#include <QVBoxLayout>

namespace {

constexpr int kPreviewMaxSize = 400;

QString formatSizeString(qint64 bytes)
{
    if (bytes < 1024)
        return QString::number(bytes) + QStringLiteral(" B");
    if (bytes < 1024 * 1024)
        return QString::number(bytes / 1024.0, 'f', 1) + QStringLiteral(" KB");
    return QString::number(bytes / (1024.0 * 1024.0), 'f', 1) + QStringLiteral(" MB");
}

QImage drawCheckerboard(int width, int height)
{
    QImage checker(width, height, QImage::Format_RGB32);
    checker.fill(Qt::white);
    QPainter p(&checker);
    p.setPen(Qt::NoPen);
    p.setBrush(QColor(0xCC, 0xCC, 0xCC));
    constexpr int kSquare = 8;
    for (int y = 0; y < height; y += kSquare) {
        for (int x = 0; x < width; x += kSquare) {
            if (((x / kSquare) + (y / kSquare)) % 2 == 1)
                p.drawRect(x, y, kSquare, kSquare);
        }
    }
    return checker;
}

} // namespace

ExportAsDialog::ExportAsDialog(const QImage &image, const QString &documentName,
                               QWidget *parent)
    : QDialog(parent)
    , m_original(image)
    , m_originalWidth(image.width())
    , m_originalHeight(image.height())
{
    setWindowTitle(tr("Export As"));
    setMinimumSize(800, 550);
    resize(900, 600);
    buildUi(documentName);
    updatePreview();
    updateFileSizeEstimate();
}

void ExportAsDialog::buildUi(const QString &documentName)
{
    // Outer layout: the three-column body above the button row.
    auto *outerLayout = new QVBoxLayout(this);

    auto *body = new QHBoxLayout;

    // -- Left panel: document info --
    auto *leftPanel = new QVBoxLayout;
    m_infoLabel = new QLabel(this);
    m_infoLabel->setFixedWidth(150);
    m_infoLabel->setWordWrap(true);
    m_infoLabel->setAlignment(Qt::AlignTop | Qt::AlignLeft);
    m_infoLabel->setStyleSheet(
        QStringLiteral("QLabel { background: #3c3c3c; border: 1px solid #555;"
                       " padding: 8px; }"));
    leftPanel->addWidget(m_infoLabel);
    leftPanel->addStretch();
    body->addLayout(leftPanel);

    // -- Centre: preview --
    auto *centreLayout = new QVBoxLayout;
    m_previewLabel = new QLabel(this);
    m_previewLabel->setMinimumSize(300, 300);
    m_previewLabel->setAlignment(Qt::AlignCenter);
    m_previewLabel->setStyleSheet(
        QStringLiteral("QLabel { background: #808080; border: 1px solid #555; }"));
    centreLayout->addWidget(m_previewLabel, 1);

    m_fileSizeLabel = new QLabel(this);
    m_fileSizeLabel->setAlignment(Qt::AlignRight);
    centreLayout->addWidget(m_fileSizeLabel);

    body->addLayout(centreLayout, 1);

    // -- Right panel: settings --
    auto *rightWidget = new QWidget(this);
    rightWidget->setFixedWidth(240);
    auto *rightInner = new QVBoxLayout(rightWidget);
    rightInner->setContentsMargins(0, 0, 0, 0);

    // File Settings
    auto *fileGroup = new QGroupBox(tr("File Settings"), rightWidget);
    auto *fileLayout = new QVBoxLayout(fileGroup);

    auto *formatRow = new QHBoxLayout;
    formatRow->addWidget(new QLabel(tr("Format:"), fileGroup));
    m_formatCombo = new QComboBox(fileGroup);
    m_formatCombo->addItem(QStringLiteral("PNG"), static_cast<int>(PNG));
    m_formatCombo->addItem(QStringLiteral("JPG"), static_cast<int>(JPG));
    m_formatCombo->addItem(QStringLiteral("PNG-8"), static_cast<int>(PNG8));
    m_formatCombo->addItem(QStringLiteral("GIF"), static_cast<int>(GIF));
    formatRow->addWidget(m_formatCombo);
    fileLayout->addLayout(formatRow);

    m_transparencyCheck = new QCheckBox(tr("Transparency"), fileGroup);
    m_transparencyCheck->setChecked(true);
    fileLayout->addWidget(m_transparencyCheck);

    auto *qualityRow = new QHBoxLayout;
    m_qualityLabel = new QLabel(tr("Quality:"), fileGroup);
    qualityRow->addWidget(m_qualityLabel);
    m_qualitySpin = new QSpinBox(fileGroup);
    m_qualitySpin->setRange(1, 100);
    m_qualitySpin->setValue(80);
    m_qualitySpin->setSuffix(QStringLiteral("%"));
    qualityRow->addWidget(m_qualitySpin);
    fileLayout->addLayout(qualityRow);
    m_qualityLabel->setVisible(false);
    m_qualitySpin->setVisible(false);

    rightInner->addWidget(fileGroup);

    // Image Size
    auto *sizeGroup = new QGroupBox(tr("Image Size"), rightWidget);
    auto *sizeLayout = new QVBoxLayout(sizeGroup);

    auto *widthRow = new QHBoxLayout;
    widthRow->addWidget(new QLabel(tr("Width:"), sizeGroup));
    m_widthSpin = new QSpinBox(sizeGroup);
    m_widthSpin->setRange(1, 30000);
    m_widthSpin->setValue(m_originalWidth);
    m_widthSpin->setSuffix(QStringLiteral(" px"));
    widthRow->addWidget(m_widthSpin);
    sizeLayout->addLayout(widthRow);

    auto *heightRow = new QHBoxLayout;
    heightRow->addWidget(new QLabel(tr("Height:"), sizeGroup));
    m_heightSpin = new QSpinBox(sizeGroup);
    m_heightSpin->setRange(1, 30000);
    m_heightSpin->setValue(m_originalHeight);
    m_heightSpin->setSuffix(QStringLiteral(" px"));
    heightRow->addWidget(m_heightSpin);
    sizeLayout->addLayout(heightRow);

    auto *scaleRow = new QHBoxLayout;
    scaleRow->addWidget(new QLabel(tr("Scale:"), sizeGroup));
    m_scaleSpin = new QSpinBox(sizeGroup);
    m_scaleSpin->setRange(1, 1000);
    m_scaleSpin->setValue(100);
    m_scaleSpin->setSuffix(QStringLiteral("%"));
    scaleRow->addWidget(m_scaleSpin);
    sizeLayout->addLayout(scaleRow);

    auto *resampleRow = new QHBoxLayout;
    resampleRow->addWidget(new QLabel(tr("Resample:"), sizeGroup));
    m_resampleCombo = new QComboBox(sizeGroup);
    m_resampleCombo->addItem(tr("Bicubic"));
    m_resampleCombo->addItem(tr("Bilinear"));
    m_resampleCombo->addItem(tr("Nearest Neighbor"));
    resampleRow->addWidget(m_resampleCombo);
    sizeLayout->addLayout(resampleRow);

    rightInner->addWidget(sizeGroup);

    // Canvas Size
    auto *canvasGroup = new QGroupBox(tr("Canvas Size"), rightWidget);
    auto *canvasLayout = new QVBoxLayout(canvasGroup);

    auto *cWidthRow = new QHBoxLayout;
    cWidthRow->addWidget(new QLabel(tr("Width:"), canvasGroup));
    m_canvasWidthSpin = new QSpinBox(canvasGroup);
    m_canvasWidthSpin->setRange(1, 30000);
    m_canvasWidthSpin->setValue(m_originalWidth);
    m_canvasWidthSpin->setSuffix(QStringLiteral(" px"));
    cWidthRow->addWidget(m_canvasWidthSpin);
    canvasLayout->addLayout(cWidthRow);

    auto *cHeightRow = new QHBoxLayout;
    cHeightRow->addWidget(new QLabel(tr("Height:"), canvasGroup));
    m_canvasHeightSpin = new QSpinBox(canvasGroup);
    m_canvasHeightSpin->setRange(1, 30000);
    m_canvasHeightSpin->setValue(m_originalHeight);
    m_canvasHeightSpin->setSuffix(QStringLiteral(" px"));
    cHeightRow->addWidget(m_canvasHeightSpin);
    canvasLayout->addLayout(cHeightRow);

    auto *resetButton = new QPushButton(tr("Reset"), canvasGroup);
    canvasLayout->addWidget(resetButton);

    rightInner->addWidget(canvasGroup);
    rightInner->addStretch();

    body->addWidget(rightWidget);

    outerLayout->addLayout(body, 1);

    // -- Bottom buttons --
    auto *buttons = new QHBoxLayout;
    buttons->addStretch();
    auto *cancelButton = new QPushButton(tr("Cancel"), this);
    auto *exportButton = new QPushButton(tr("Export..."), this);
    exportButton->setDefault(true);
    buttons->addWidget(cancelButton);
    buttons->addWidget(exportButton);
    outerLayout->addLayout(buttons);

    // Update the info label
    const QString formatName = m_formatCombo->currentText();
    m_infoLabel->setText(QStringLiteral("%1\n%2    %3 x %4")
                             .arg(documentName, formatName)
                             .arg(m_originalWidth)
                             .arg(m_originalHeight));

    // Connections
    connect(m_formatCombo, &QComboBox::currentIndexChanged, this,
            &ExportAsDialog::onFormatChanged);
    connect(m_transparencyCheck, &QCheckBox::toggled, this, &ExportAsDialog::updatePreview);
    connect(m_transparencyCheck, &QCheckBox::toggled, this,
            &ExportAsDialog::updateFileSizeEstimate);
    connect(m_qualitySpin, &QSpinBox::valueChanged, this,
            &ExportAsDialog::updateFileSizeEstimate);
    connect(m_widthSpin, &QSpinBox::valueChanged, this, &ExportAsDialog::onWidthChanged);
    connect(m_heightSpin, &QSpinBox::valueChanged, this, &ExportAsDialog::onHeightChanged);
    connect(m_scaleSpin, &QSpinBox::valueChanged, this, &ExportAsDialog::onScaleChanged);
    connect(m_canvasWidthSpin, &QSpinBox::valueChanged, this,
            &ExportAsDialog::onCanvasWidthChanged);
    connect(m_canvasHeightSpin, &QSpinBox::valueChanged, this,
            &ExportAsDialog::onCanvasHeightChanged);
    connect(resetButton, &QPushButton::clicked, this, &ExportAsDialog::resetCanvasSize);
    connect(cancelButton, &QPushButton::clicked, this, &QDialog::reject);
    connect(exportButton, &QPushButton::clicked, this, &QDialog::accept);
}

void ExportAsDialog::onFormatChanged(int index)
{
    const Format fmt = static_cast<Format>(m_formatCombo->itemData(index).toInt());
    const bool isJpeg = (fmt == JPG);
    m_qualityLabel->setVisible(isJpeg);
    m_qualitySpin->setVisible(isJpeg);

    m_transparencyCheck->setEnabled(fmt != JPG);
    if (fmt == JPG)
        m_transparencyCheck->setChecked(false);

    const QString formatName = m_formatCombo->currentText();
    m_infoLabel->setText(QStringLiteral("%1\n%2    %3 x %4")
                             .arg(windowTitle(), formatName)
                             .arg(m_widthSpin->value())
                             .arg(m_heightSpin->value()));

    updatePreview();
    updateFileSizeEstimate();
}

void ExportAsDialog::onWidthChanged(int value)
{
    if (m_updatingSize)
        return;
    m_updatingSize = true;
    const double ratio = static_cast<double>(m_originalHeight) / m_originalWidth;
    m_heightSpin->setValue(qRound(value * ratio));
    m_scaleSpin->setValue(qRound(100.0 * value / m_originalWidth));
    m_updatingSize = false;
    updatePreview();
    updateFileSizeEstimate();
}

void ExportAsDialog::onHeightChanged(int value)
{
    if (m_updatingSize)
        return;
    m_updatingSize = true;
    const double ratio = static_cast<double>(m_originalWidth) / m_originalHeight;
    m_widthSpin->setValue(qRound(value * ratio));
    m_scaleSpin->setValue(qRound(100.0 * value / m_originalHeight));
    m_updatingSize = false;
    updatePreview();
    updateFileSizeEstimate();
}

void ExportAsDialog::onScaleChanged(int value)
{
    if (m_updatingSize)
        return;
    m_updatingSize = true;
    m_widthSpin->setValue(qRound(m_originalWidth * value / 100.0));
    m_heightSpin->setValue(qRound(m_originalHeight * value / 100.0));
    m_updatingSize = false;
    updatePreview();
    updateFileSizeEstimate();
}

void ExportAsDialog::onCanvasWidthChanged(int /*value*/)
{
    updatePreview();
    updateFileSizeEstimate();
}

void ExportAsDialog::onCanvasHeightChanged(int /*value*/)
{
    updatePreview();
    updateFileSizeEstimate();
}

void ExportAsDialog::resetCanvasSize()
{
    m_canvasWidthSpin->setValue(m_widthSpin->value());
    m_canvasHeightSpin->setValue(m_heightSpin->value());
}

ExportAsDialog::Format ExportAsDialog::chosenFormat() const
{
    return static_cast<Format>(m_formatCombo->currentData().toInt());
}

int ExportAsDialog::jpegQuality() const
{
    return m_qualitySpin->value();
}

QImage ExportAsDialog::scaledImage() const
{
    const int w = m_widthSpin->value();
    const int h = m_heightSpin->value();
    if (w == m_originalWidth && h == m_originalHeight)
        return m_original;

    Qt::TransformationMode mode = Qt::SmoothTransformation;
    if (m_resampleCombo->currentIndex() == 2)
        mode = Qt::FastTransformation;

    return m_original.scaled(w, h, Qt::IgnoreAspectRatio, mode);
}

QImage ExportAsDialog::canvasAdjustedImage(const QImage &img) const
{
    const int canvasW = m_canvasWidthSpin->value();
    const int canvasH = m_canvasHeightSpin->value();
    if (canvasW == img.width() && canvasH == img.height())
        return img;

    QImage canvas(canvasW, canvasH, img.format());
    canvas.fill(Qt::transparent);
    QPainter p(&canvas);
    const int x = (canvasW - img.width()) / 2;
    const int y = (canvasH - img.height()) / 2;
    p.drawImage(x, y, img);
    return canvas;
}

QImage ExportAsDialog::exportImage() const
{
    QImage img = scaledImage();
    img = canvasAdjustedImage(img);

    const Format fmt = chosenFormat();
    if (fmt == JPG || !m_transparencyCheck->isChecked()) {
        QImage flat(img.size(), QImage::Format_RGB32);
        flat.fill(Qt::white);
        QPainter p(&flat);
        p.drawImage(0, 0, img);
        return flat;
    }

    if (fmt == PNG8 || fmt == GIF) {
        return img.convertToFormat(QImage::Format_Indexed8);
    }

    return img;
}

void ExportAsDialog::updatePreview()
{
    QImage img = scaledImage();
    img = canvasAdjustedImage(img);

    const int maxDim = qMin(m_previewLabel->width(), m_previewLabel->height());
    const int previewSize = qMax(qMin(maxDim, kPreviewMaxSize), 100);
    QImage preview;
    if (img.width() > previewSize || img.height() > previewSize) {
        preview = img.scaled(previewSize, previewSize, Qt::KeepAspectRatio,
                             Qt::SmoothTransformation);
    } else {
        preview = img;
    }

    if (m_transparencyCheck->isChecked() && preview.hasAlphaChannel()) {
        QImage checker = drawCheckerboard(preview.width(), preview.height());
        QPainter p(&checker);
        p.drawImage(0, 0, preview);
        preview = checker;
    }

    m_previewLabel->setPixmap(QPixmap::fromImage(preview));
}

void ExportAsDialog::updateFileSizeEstimate()
{
    QImage img = exportImage();
    QByteArray data;
    QBuffer buffer(&data);
    buffer.open(QIODevice::WriteOnly);

    const Format fmt = chosenFormat();
    switch (fmt) {
    case PNG:
    case PNG8:
    case GIF:
        img.save(&buffer, "PNG");
        break;
    case JPG:
        img.save(&buffer, "JPEG", m_qualitySpin->value());
        break;
    }

    m_fileSizeLabel->setText(formatSizeString(data.size()));
}

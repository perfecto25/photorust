#include "PrintDialog.h"

#include <QCheckBox>
#include <QComboBox>
#include <QGroupBox>
#include <QHBoxLayout>
#include <QLabel>
#include <QPainter>
#include <QPrintDialog>
#include <QPrinter>
#include <QPrinterInfo>
#include <QPushButton>
#include <QSpinBox>
#include <QStyle>
#include <QToolButton>
#include <QVBoxLayout>

namespace {

constexpr int kPreviewWidth = 280;
constexpr int kPreviewHeight = 360;

QIcon orientationIcon(bool landscape, const QColor &color)
{
    QPixmap pix(20, 20);
    pix.fill(Qt::transparent);
    QPainter p(&pix);
    p.setPen(color);
    p.setBrush(Qt::NoBrush);
    if (landscape) {
        p.drawRect(2, 4, 16, 12);
    } else {
        p.drawRect(4, 2, 12, 16);
    }
    return QIcon(pix);
}

} // namespace

PrintDialog::PrintDialog(const QImage &image, QWidget *parent)
    : QDialog(parent)
    , m_image(image)
{
    setWindowTitle(tr("PhotoRust Print Settings"));
    setMinimumSize(700, 500);
    resize(780, 550);
    buildUi();
    populatePrinters();
    updatePreview();
}

void PrintDialog::buildUi()
{
    auto *outerLayout = new QVBoxLayout(this);

    auto *body = new QHBoxLayout;

    // -- Left: preview --
    auto *leftPanel = new QVBoxLayout;

    m_pageSizeLabel = new QLabel(tr("8.5 in x 11 in"), this);
    m_pageSizeLabel->setStyleSheet(
        QStringLiteral("QLabel { color: #aaa; font-size: 11px; }"));
    leftPanel->addWidget(m_pageSizeLabel);

    m_previewLabel = new QLabel(this);
    m_previewLabel->setFixedSize(kPreviewWidth, kPreviewHeight);
    m_previewLabel->setAlignment(Qt::AlignCenter);
    m_previewLabel->setStyleSheet(
        QStringLiteral("QLabel { background: #808080; border: 1px solid #555; }"));
    leftPanel->addWidget(m_previewLabel);

    leftPanel->addSpacing(12);

    m_matchPrintColors = new QCheckBox(tr("Match Print Colors"), this);
    leftPanel->addWidget(m_matchPrintColors);

    m_gamutWarning = new QCheckBox(tr("Gamut Warning"), this);
    leftPanel->addWidget(m_gamutWarning);

    m_showPaperWhite = new QCheckBox(tr("Show Paper White"), this);
    leftPanel->addWidget(m_showPaperWhite);

    leftPanel->addStretch();
    body->addLayout(leftPanel);

    // -- Right: settings --
    auto *rightPanel = new QVBoxLayout;

    // Printer Setup
    auto *setupGroup = new QGroupBox(tr("Printer Setup"), this);
    auto *setupLayout = new QVBoxLayout(setupGroup);

    auto *printerRow = new QHBoxLayout;
    printerRow->addWidget(new QLabel(tr("Printer:"), setupGroup));
    m_printerCombo = new QComboBox(setupGroup);
    m_printerCombo->setMinimumWidth(200);
    printerRow->addWidget(m_printerCombo, 1);
    setupLayout->addLayout(printerRow);

    auto *copiesRow = new QHBoxLayout;
    copiesRow->addWidget(new QLabel(tr("Copies:"), setupGroup));
    m_copiesSpin = new QSpinBox(setupGroup);
    m_copiesSpin->setRange(1, 999);
    m_copiesSpin->setValue(1);
    m_copiesSpin->setFixedWidth(60);
    copiesRow->addWidget(m_copiesSpin);
    copiesRow->addSpacing(12);
    m_printSettingsButton = new QPushButton(tr("Print Settings..."), setupGroup);
    copiesRow->addWidget(m_printSettingsButton);
    copiesRow->addStretch();
    setupLayout->addLayout(copiesRow);

    auto *layoutRow = new QHBoxLayout;
    layoutRow->addWidget(new QLabel(tr("Layout:"), setupGroup));

    const QColor iconColor(0xd4, 0xd4, 0xd4);
    m_portraitButton = new QPushButton(setupGroup);
    m_portraitButton->setIcon(orientationIcon(false, iconColor));
    m_portraitButton->setCheckable(true);
    m_portraitButton->setChecked(true);
    m_portraitButton->setFixedSize(30, 26);
    m_portraitButton->setToolTip(tr("Portrait"));
    layoutRow->addWidget(m_portraitButton);

    m_landscapeButton = new QPushButton(setupGroup);
    m_landscapeButton->setIcon(orientationIcon(true, iconColor));
    m_landscapeButton->setCheckable(true);
    m_landscapeButton->setFixedSize(30, 26);
    m_landscapeButton->setToolTip(tr("Landscape"));
    layoutRow->addWidget(m_landscapeButton);

    layoutRow->addStretch();
    setupLayout->addLayout(layoutRow);

    rightPanel->addWidget(setupGroup);

    // Color Management
    auto *colorGroup = new QGroupBox(tr("Color Management"), this);
    auto *colorLayout = new QVBoxLayout(colorGroup);

    auto *hintRow = new QHBoxLayout;
    auto *warnIcon = new QLabel(colorGroup);
    warnIcon->setPixmap(style()->standardPixmap(QStyle::SP_MessageBoxWarning)
                            .scaled(20, 20, Qt::KeepAspectRatio, Qt::SmoothTransformation));
    warnIcon->setFixedSize(24, 24);
    hintRow->addWidget(warnIcon);
    auto *hintLabel = new QLabel(
        tr("Remember to enable the printer's color\nmanagement in the print settings dialog box."),
        colorGroup);
    hintLabel->setStyleSheet(QStringLiteral("QLabel { font-size: 11px; }"));
    hintRow->addWidget(hintLabel, 1);
    colorLayout->addLayout(hintRow);

    colorLayout->addSpacing(4);

    auto *profileLabel = new QLabel(tr("Document Profile: sRGB IEC61966-2.1"), colorGroup);
    profileLabel->setStyleSheet(QStringLiteral("QLabel { font-weight: bold; font-size: 11px; }"));
    colorLayout->addWidget(profileLabel);

    auto *handlingRow = new QHBoxLayout;
    handlingRow->addWidget(new QLabel(tr("Color Handling:"), colorGroup));
    m_colorHandlingCombo = new QComboBox(colorGroup);
    m_colorHandlingCombo->addItem(tr("Printer Manages Colors"));
    m_colorHandlingCombo->addItem(tr("Photoshop Manages Colors"));
    m_colorHandlingCombo->addItem(tr("Separations"));
    handlingRow->addWidget(m_colorHandlingCombo, 1);
    colorLayout->addLayout(handlingRow);

    auto *printerProfileRow = new QHBoxLayout;
    printerProfileRow->addWidget(new QLabel(tr("Printer Profile:"), colorGroup));
    m_printerProfileCombo = new QComboBox(colorGroup);
    m_printerProfileCombo->addItem(tr("CIE RGB"));
    m_printerProfileCombo->addItem(tr("sRGB IEC61966-2.1"));
    m_printerProfileCombo->addItem(tr("Adobe RGB (1998)"));
    m_printerProfileCombo->setEnabled(false);
    printerProfileRow->addWidget(m_printerProfileCombo, 1);
    colorLayout->addLayout(printerProfileRow);

    auto *intentRow = new QHBoxLayout;
    auto *normalLabel = new QLabel(tr("Normal Printing"), colorGroup);
    normalLabel->setStyleSheet(
        QStringLiteral("QLabel { border: 1px solid #666; padding: 2px 8px; }"));
    intentRow->addWidget(normalLabel);
    intentRow->addStretch();
    colorLayout->addLayout(intentRow);

    auto *renderRow = new QHBoxLayout;
    renderRow->addWidget(new QLabel(tr("Rendering Intent:"), colorGroup));
    m_renderingIntentCombo = new QComboBox(colorGroup);
    m_renderingIntentCombo->addItem(tr("Relative Colorimetric"));
    m_renderingIntentCombo->addItem(tr("Perceptual"));
    m_renderingIntentCombo->addItem(tr("Saturation"));
    m_renderingIntentCombo->addItem(tr("Absolute Colorimetric"));
    m_renderingIntentCombo->setEnabled(false);
    renderRow->addWidget(m_renderingIntentCombo, 1);
    colorLayout->addLayout(renderRow);

    m_blackPointCheck = new QCheckBox(tr("Black Point Compensation"), colorGroup);
    m_blackPointCheck->setEnabled(false);
    colorLayout->addWidget(m_blackPointCheck);

    rightPanel->addWidget(colorGroup);

    // Description
    auto *descGroup = new QGroupBox(tr("Description"), this);
    auto *descLayout = new QVBoxLayout(descGroup);
    auto *descLabel = new QLabel(
        tr("Prints the composite image to the selected printer."), descGroup);
    descLabel->setWordWrap(true);
    descLabel->setStyleSheet(QStringLiteral("QLabel { font-size: 11px; color: #aaa; }"));
    descLayout->addWidget(descLabel);
    rightPanel->addWidget(descGroup);

    rightPanel->addStretch();
    body->addLayout(rightPanel, 1);

    outerLayout->addLayout(body, 1);

    // -- Bottom buttons --
    auto *buttons = new QHBoxLayout;
    buttons->addStretch();
    auto *cancelButton = new QPushButton(tr("Cancel"), this);
    auto *doneButton = new QPushButton(tr("Done"), this);
    auto *printButton = new QPushButton(tr("Print"), this);
    printButton->setDefault(true);
    buttons->addWidget(cancelButton);
    buttons->addWidget(doneButton);
    buttons->addWidget(printButton);
    outerLayout->addLayout(buttons);

    // Connections
    connect(m_printerCombo, &QComboBox::currentIndexChanged, this,
            &PrintDialog::onPrinterChanged);
    connect(m_printSettingsButton, &QPushButton::clicked, this,
            &PrintDialog::onPrintSettingsClicked);
    connect(m_portraitButton, &QPushButton::clicked, this, [this] {
        m_landscape = false;
        m_portraitButton->setChecked(true);
        m_landscapeButton->setChecked(false);
        updatePreview();
    });
    connect(m_landscapeButton, &QPushButton::clicked, this, [this] {
        m_landscape = true;
        m_portraitButton->setChecked(false);
        m_landscapeButton->setChecked(true);
        updatePreview();
    });
    connect(m_colorHandlingCombo, &QComboBox::currentIndexChanged, this, [this](int index) {
        const bool appManaged = (index == 1);
        m_printerProfileCombo->setEnabled(appManaged);
        m_renderingIntentCombo->setEnabled(appManaged);
        m_blackPointCheck->setEnabled(appManaged);
    });
    connect(m_showPaperWhite, &QCheckBox::toggled, this, [this] { updatePreview(); });
    connect(cancelButton, &QPushButton::clicked, this, &QDialog::reject);
    connect(doneButton, &QPushButton::clicked, this, &QDialog::accept);
    connect(printButton, &QPushButton::clicked, this, &PrintDialog::onPrint);
}

void PrintDialog::populatePrinters()
{
    m_printerCombo->clear();
    const QList<QPrinterInfo> printers = QPrinterInfo::availablePrinters();
    if (printers.isEmpty()) {
        m_printerCombo->addItem(tr("(no printers found)"));
        m_printSettingsButton->setEnabled(false);
        return;
    }
    const QString defaultName = QPrinterInfo::defaultPrinter().printerName();
    int defaultIndex = 0;
    for (int i = 0; i < printers.size(); ++i) {
        m_printerCombo->addItem(printers[i].printerName());
        if (printers[i].printerName() == defaultName)
            defaultIndex = i;
    }
    m_printerCombo->setCurrentIndex(defaultIndex);
}

void PrintDialog::onPrinterChanged(int /*index*/)
{
    updatePreview();
}

void PrintDialog::onPrintSettingsClicked()
{
    const QString printerName = m_printerCombo->currentText();
    QPrinter printer;
    printer.setPrinterName(printerName);
    QPrintDialog dialog(&printer, this);
    dialog.exec();
}

void PrintDialog::onPrint()
{
    const QString printerName = m_printerCombo->currentText();
    if (printerName.isEmpty() || printerName == tr("(no printers found)")) {
        reject();
        return;
    }

    QPrinter printer;
    printer.setPrinterName(printerName);
    printer.setCopyCount(m_copiesSpin->value());
    printer.setPageOrientation(m_landscape ? QPageLayout::Landscape
                                           : QPageLayout::Portrait);

    QPainter painter(&printer);
    if (!painter.isActive()) {
        reject();
        return;
    }

    const QRect pageRect = painter.viewport();
    QImage img = m_image;
    if (img.isNull()) {
        reject();
        return;
    }

    QSize scaled = img.size();
    scaled.scale(pageRect.size(), Qt::KeepAspectRatio);
    const int x = (pageRect.width() - scaled.width()) / 2;
    const int y = (pageRect.height() - scaled.height()) / 2;
    painter.drawImage(QRect(x, y, scaled.width(), scaled.height()), img);
    painter.end();

    accept();
}

QImage PrintDialog::pagePreview() const
{
    int pageW = kPreviewWidth - 20;
    int pageH = kPreviewHeight - 20;

    if (m_landscape)
        std::swap(pageW, pageH);

    const QColor paper = m_showPaperWhite && m_showPaperWhite->isChecked()
                             ? QColor(0xF5, 0xF5, 0xF0)
                             : Qt::white;

    QImage page(pageW, pageH, QImage::Format_RGB32);
    page.fill(paper);

    if (!m_image.isNull()) {
        QSize imgSize = m_image.size();
        const int margin = 10;
        const int availW = pageW - 2 * margin;
        const int availH = pageH - 2 * margin;
        imgSize.scale(availW, availH, Qt::KeepAspectRatio);
        const int x = (pageW - imgSize.width()) / 2;
        const int y = (pageH - imgSize.height()) / 2;

        QPainter p(&page);
        p.setRenderHint(QPainter::SmoothPixmapTransform);
        p.drawImage(QRect(x, y, imgSize.width(), imgSize.height()), m_image);
    }

    // Draw page border
    {
        QPainter p(&page);
        p.setPen(QColor(0x99, 0x99, 0x99));
        p.drawRect(0, 0, pageW - 1, pageH - 1);
    }

    // Centre the page on a grey background
    QImage result(kPreviewWidth, kPreviewHeight, QImage::Format_RGB32);
    result.fill(QColor(0x80, 0x80, 0x80));
    QPainter rp(&result);
    const int px = (kPreviewWidth - pageW) / 2;
    const int py = (kPreviewHeight - pageH) / 2;
    // Drop shadow
    rp.fillRect(px + 2, py + 2, pageW, pageH, QColor(0x50, 0x50, 0x50));
    rp.drawImage(px, py, page);

    return result;
}

void PrintDialog::updatePreview()
{
    m_previewLabel->setPixmap(QPixmap::fromImage(pagePreview()));

    const QString sizeText = m_landscape ? tr("11 in x 8.5 in") : tr("8.5 in x 11 in");
    m_pageSizeLabel->setText(sizeText);
}

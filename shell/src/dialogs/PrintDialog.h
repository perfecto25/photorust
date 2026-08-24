#pragma once

#include <QDialog>
#include <QImage>

class QCheckBox;
class QComboBox;
class QLabel;
class QPrinter;
class QPushButton;
class QSpinBox;

/// File > Print: Photoshop-style print settings dialog.
///
/// Matches CS6's layout: a preview of the image on the page (left), Printer
/// Setup and Color Management sections (right), and Match Print Colors / Gamut
/// Warning / Show Paper White checkboxes below the preview.
class PrintDialog : public QDialog
{
    Q_OBJECT

public:
    explicit PrintDialog(const QImage &image, QWidget *parent = nullptr);

private slots:
    void onPrinterChanged(int index);
    void onPrintSettingsClicked();
    void onPrint();
    void updatePreview();

private:
    void buildUi();
    void populatePrinters();
    QImage pagePreview() const;

    QImage m_image;

    // Printer Setup
    QComboBox *m_printerCombo = nullptr;
    QSpinBox *m_copiesSpin = nullptr;
    QPushButton *m_printSettingsButton = nullptr;
    QPushButton *m_portraitButton = nullptr;
    QPushButton *m_landscapeButton = nullptr;

    // Color Management
    QComboBox *m_colorHandlingCombo = nullptr;
    QComboBox *m_printerProfileCombo = nullptr;
    QComboBox *m_renderingIntentCombo = nullptr;
    QCheckBox *m_blackPointCheck = nullptr;

    // Preview
    QLabel *m_previewLabel = nullptr;
    QLabel *m_pageSizeLabel = nullptr;
    QCheckBox *m_matchPrintColors = nullptr;
    QCheckBox *m_gamutWarning = nullptr;
    QCheckBox *m_showPaperWhite = nullptr;

    bool m_landscape = false;
};

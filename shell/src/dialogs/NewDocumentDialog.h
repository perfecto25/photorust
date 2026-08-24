#pragma once

#include <QDialog>

class QComboBox;
class QDoubleSpinBox;
class QGroupBox;
class QLabel;
class QLineEdit;
class QSpinBox;

/// File > New: CS6-style new document dialog.
///
/// Matches CS6's layout: Name, Document Type, Size preset, Width/Height with
/// unit selector, Resolution, Color Mode with bit depth, Background Contents,
/// and an Advanced section with Color Profile and Pixel Aspect Ratio.
/// The right column shows OK/Cancel/Save Preset/Delete Preset and an image
/// size estimate.
class NewDocumentDialog : public QDialog
{
    Q_OBJECT

public:
    explicit NewDocumentDialog(int documentNumber, QWidget *parent = nullptr);

    QString documentName() const;
    int widthPixels() const;
    int heightPixels() const;
    int backgroundFill() const;
    void accept() override;

private slots:
    void onUnitChanged(int index);
    void onPresetChanged(int index);
    void onDimensionChanged();
    void updateImageSize();

private:
    void buildUi();
    double toPixels(double value) const;
    double fromPixels(double px) const;

    QLineEdit *m_nameEdit = nullptr;
    QComboBox *m_docTypeCombo = nullptr;
    QComboBox *m_sizePresetCombo = nullptr;
    QDoubleSpinBox *m_widthSpin = nullptr;
    QDoubleSpinBox *m_heightSpin = nullptr;
    QComboBox *m_unitCombo = nullptr;
    QSpinBox *m_resolutionSpin = nullptr;
    QComboBox *m_resUnitCombo = nullptr;
    QComboBox *m_colorModeCombo = nullptr;
    QComboBox *m_bitDepthCombo = nullptr;
    QComboBox *m_backgroundCombo = nullptr;
    QComboBox *m_colorProfileCombo = nullptr;
    QComboBox *m_pixelAspectCombo = nullptr;
    QLabel *m_imageSizeLabel = nullptr;

    bool m_updatingDimensions = false;
    int m_documentNumber;
};

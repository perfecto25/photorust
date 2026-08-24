#pragma once

#include <QDialog>
#include <QImage>

class QCheckBox;
class QComboBox;
class QLabel;
class QSpinBox;
class QTabBar;

/// File > Export > Save for Web (Legacy): the classic web-export dialog.
///
/// Matches CS6's layout: preview tabs (Original / Optimized / 2-Up / 4-Up),
/// format-specific settings on the right, image size section, and the
/// Save / Cancel / Done buttons at the bottom.
class SaveForWebDialog : public QDialog
{
    Q_OBJECT

public:
    enum Format { GIF, JPEG, PNG8, PNG24, WBMP };

    explicit SaveForWebDialog(const QImage &image, QWidget *parent = nullptr);

    /// The format the user chose.
    Format chosenFormat() const;
    /// The final image, converted for the chosen format.
    QImage exportImage() const;
    /// The file extension for the chosen format.
    QString fileExtension() const;
    /// JPEG quality (1-100).
    int jpegQuality() const;

private slots:
    void onFormatChanged(int index);
    void onPreviewTabChanged(int index);
    void onSettingsChanged();
    void onImageWidthChanged(int value);
    void onImageHeightChanged(int value);
    void onPercentChanged(int value);

private:
    void buildUi();
    void updatePreview();
    void updateFormatOptions();
    void updateFileInfo();
    QImage optimizedImage() const;
    QImage sizedImage() const;

    QImage m_original;
    int m_originalWidth;
    int m_originalHeight;

    // Preview
    QTabBar *m_previewTabs = nullptr;
    QLabel *m_previewLabel = nullptr;
    QLabel *m_previewInfoLabel = nullptr;

    // Format settings
    QComboBox *m_presetCombo = nullptr;
    QComboBox *m_formatCombo = nullptr;

    // GIF settings
    QComboBox *m_gifColorReduction = nullptr;
    QSpinBox *m_gifColors = nullptr;
    QComboBox *m_gifDither = nullptr;
    QSpinBox *m_gifDitherAmount = nullptr;
    QCheckBox *m_gifTransparency = nullptr;
    QCheckBox *m_gifInterlaced = nullptr;
    QLabel *m_gifColorsLabel = nullptr;
    QLabel *m_gifDitherLabel = nullptr;
    QLabel *m_gifDitherAmountLabel = nullptr;

    // JPEG settings
    QComboBox *m_jpegQualityPreset = nullptr;
    QSpinBox *m_jpegQuality = nullptr;
    QCheckBox *m_jpegProgressive = nullptr;
    QCheckBox *m_jpegOptimized = nullptr;
    QLabel *m_jpegQualityPresetLabel = nullptr;
    QLabel *m_jpegQualityLabel = nullptr;

    // PNG settings
    QCheckBox *m_pngTransparency = nullptr;
    QCheckBox *m_pngInterlaced = nullptr;

    // Common
    QCheckBox *m_convertSrgb = nullptr;
    QComboBox *m_metadataCombo = nullptr;

    // Image Size
    QSpinBox *m_imageSizeWidth = nullptr;
    QSpinBox *m_imageSizeHeight = nullptr;
    QSpinBox *m_imageSizePercent = nullptr;
    QComboBox *m_imageSizeResample = nullptr;

    // File info
    QLabel *m_fileInfoLabel = nullptr;

    bool m_updatingSize = false;
};

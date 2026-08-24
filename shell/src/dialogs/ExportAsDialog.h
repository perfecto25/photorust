#pragma once

#include <QDialog>
#include <QImage>

class QComboBox;
class QCheckBox;
class QLabel;
class QSpinBox;

/// File > Export > Export As: a single-image export dialog matching CS6's layout.
///
/// Left panel: layer list (simplified to document name + format + dimensions).
/// Centre: live preview of the image.
/// Right panel: File Settings (format, transparency), Image Size (width, height,
/// scale, resample), Canvas Size (width, height, reset).
/// Bottom: file size estimate, Cancel and Export buttons.
class ExportAsDialog : public QDialog
{
    Q_OBJECT

public:
    enum Format { PNG, JPG, PNG8, GIF };

    explicit ExportAsDialog(const QImage &image, const QString &documentName,
                            QWidget *parent = nullptr);

    /// The format the user chose.
    Format chosenFormat() const;
    /// The JPEG quality (1-100), meaningful only when format is JPG.
    int jpegQuality() const;
    /// The final image, scaled and converted as requested.
    QImage exportImage() const;

private slots:
    void onFormatChanged(int index);
    void onWidthChanged(int value);
    void onHeightChanged(int value);
    void onScaleChanged(int value);
    void onCanvasWidthChanged(int value);
    void onCanvasHeightChanged(int value);
    void resetCanvasSize();
    void updatePreview();
    void updateFileSizeEstimate();

private:
    void buildUi(const QString &documentName);
    QImage scaledImage() const;
    QImage canvasAdjustedImage(const QImage &img) const;

    QImage m_original;
    int m_originalWidth;
    int m_originalHeight;

    QComboBox *m_formatCombo = nullptr;
    QCheckBox *m_transparencyCheck = nullptr;
    QSpinBox *m_qualitySpin = nullptr;
    QLabel *m_qualityLabel = nullptr;
    QSpinBox *m_widthSpin = nullptr;
    QSpinBox *m_heightSpin = nullptr;
    QSpinBox *m_scaleSpin = nullptr;
    QComboBox *m_resampleCombo = nullptr;
    QSpinBox *m_canvasWidthSpin = nullptr;
    QSpinBox *m_canvasHeightSpin = nullptr;
    QLabel *m_previewLabel = nullptr;
    QLabel *m_fileSizeLabel = nullptr;
    QLabel *m_infoLabel = nullptr;

    bool m_updatingSize = false;
};

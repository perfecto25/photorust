#pragma once

#include <QDialog>

class Engine;
class QCheckBox;
class QComboBox;
class QDoubleSpinBox;
class QLabel;
class QToolButton;

/// Photoshop's Image ▸ Image Size.
///
/// Width, Height and Resolution are three views of the same two numbers: a
/// pixel count and a pixels-per-inch figure. Which of them may change depends
/// on Resample:
///
///   * **Resample on** — the pixel count follows Width and Height. Changing
///     Resolution alone changes nothing about the pixels.
///   * **Resample off** — the pixel count is fixed, so Width, Height and
///     Resolution are locked to each other: raising the resolution shrinks the
///     printed size and vice versa. Nothing is resampled and the image is
///     untouched; only the print metadata moves.
class ImageSizeDialog : public QDialog
{
    Q_OBJECT
public:
    explicit ImageSizeDialog(Engine *engine, QWidget *parent = nullptr);

    /// Pixel dimensions the user settled on.
    int resultWidth() const { return m_pixelWidth; }
    int resultHeight() const { return m_pixelHeight; }
    double resultResolution() const;
    /// CS6 Resample menu index, or -1 when resampling is off.
    int resampleMode() const;

private:
    void buildUi();
    /// Recompute every field from the pixel size, without re-entering.
    void syncFields();
    void onWidthEdited();
    void onHeightEdited();
    void onResolutionEdited();
    void onUnitsChanged();
    void onFitToChanged(int index);
    /// Apply one of the Fit To presets, or restore the size on entry.
    void applyFitPreset(int presetIndex);
    /// Show "Custom" without re-entering `onFitToChanged`.
    void markCustom();
    void onResampleToggled(bool on);
    void updateSummary();

    /// Pixels per unit of the current Width/Height unit.
    double unitScale(int unitIndex) const;

    Engine *m_engine = nullptr;

    // The document's real state, in pixels. Every field derives from these.
    int m_pixelWidth = 0;
    int m_pixelHeight = 0;
    double m_resolution = 72.0;
    /// Size and resolution on entry, so "resample off" and the Original Size
    /// preset can restore them exactly.
    int m_originalWidth = 0;
    int m_originalHeight = 0;
    double m_originalResolution = 72.0;

    /// Guards the mutual updates between Width, Height and Resolution.
    bool m_updating = false;

    QLabel *m_summary = nullptr;
    QLabel *m_dimensions = nullptr;
    QLabel *m_preview = nullptr;
    QComboBox *m_fitTo = nullptr;
    QDoubleSpinBox *m_width = nullptr;
    QDoubleSpinBox *m_height = nullptr;
    QDoubleSpinBox *m_resolution_field = nullptr;
    QComboBox *m_widthUnit = nullptr;
    QComboBox *m_heightUnit = nullptr;
    QComboBox *m_resolutionUnit = nullptr;
    QCheckBox *m_resample = nullptr;
    QComboBox *m_resampleMode = nullptr;
    QToolButton *m_chain = nullptr;
};

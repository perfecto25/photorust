#pragma once

#include <QCheckBox>
#include <QColor>
#include <QDialog>
#include <QPoint>
#include <QVector>

#include <functional>

class Engine;
class QLabel;
class QRadioButton;
class QSlider;
class QSpinBox;
class QTimer;
class QToolButton;

/// Photoshop's Image > Adjustments > Replace Color.
///
/// The eyedropper builds a list of sampled colours; every pixel close enough
/// to one of them — "close enough" being the Fuzziness tolerance — is shifted
/// in HSL by the Hue/Saturation/Lightness sliders. The thumbnail shows that
/// selection as a mask, white where the shift applies at full strength.
class ReplaceColorDialog : public QDialog
{
    Q_OBJECT
public:
    explicit ReplaceColorDialog(Engine *engine, QWidget *parent = nullptr);
    ~ReplaceColorDialog() override;

    /// How the eyedropper reads the image: given a point on screen, report
    /// the colour there and which document pixel it came from, or return
    /// false if the point is not over the image.
    ///
    /// Application-wide for the same reason ColorPickerDialog's is: there is
    /// one canvas, and threading it through the Image menu would mean
    /// teaching the menu about the canvas.
    using Sampler = std::function<bool(const QPoint &globalPos, QColor *color, QPoint *docPos)>;
    static void setSampler(Sampler sampler);

protected:
    // While the pointer is off the dialog it holds the mouse, so these arrive
    // wherever it is — including over the canvas behind the dialog.
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;
    void showEvent(QShowEvent *event) override;
    void hideEvent(QHideEvent *event) override;

private:
    /// One picked colour, and the pixel it came from.
    struct Sample {
        QPoint pos;
        QColor color;
    };

    /// Which eyedropper button is down.
    enum class PickMode { Replace, Add, Subtract };

    void buildUi();
    void applyPreview();
    void revertPreview();
    void onValueChanged();
    void refreshMask();
    void refreshSwatches();

    /// Serialise the samples as "x,y,r,g,b;..." for the bridge.
    QString samplesString() const;

    // Eyedropper plumbing, mirroring ColorPickerDialog: the dialog is modal,
    // so it has to hold the mouse to see clicks land on the canvas.
    void updateHoverSampling();
    void showCursorFor(bool overImage);
    void clearCursorOverride();
    void takeSampleAt(const QPoint &globalPos);

    Engine *m_engine = nullptr;
    bool m_previewApplied = false;
    bool m_loading = false;

    QVector<Sample> m_samples;
    PickMode m_pickMode = PickMode::Replace;

    QToolButton *m_pickButton = nullptr;
    QToolButton *m_addButton = nullptr;
    QToolButton *m_subButton = nullptr;

    QCheckBox *m_localized = nullptr;
    QSlider *m_fuzzinessSlider = nullptr;
    QSpinBox *m_fuzzinessSpin = nullptr;

    QLabel *m_maskLabel = nullptr;
    QRadioButton *m_showSelection = nullptr;
    QRadioButton *m_showImage = nullptr;

    QSlider *m_hueSlider = nullptr;
    QSpinBox *m_hueSpin = nullptr;
    QSlider *m_satSlider = nullptr;
    QSpinBox *m_satSpin = nullptr;
    QSlider *m_lightSlider = nullptr;
    QSpinBox *m_lightSpin = nullptr;

    QLabel *m_colorSwatch = nullptr;
    QLabel *m_resultSwatch = nullptr;
    QCheckBox *m_preview = nullptr;

    QTimer *m_hoverTimer = nullptr;
    bool m_sampling = false;
    bool m_cursorOverridden = false;
};

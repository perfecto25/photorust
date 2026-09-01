#pragma once

#include <QCheckBox>
#include <QColor>
#include <QCursor>
#include <QDialog>
#include <QPoint>
#include <QVector>

#include <functional>

class Engine;
class QLabel;
class QRadioButton;
class QSlider;
class QSpinBox;
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

    /// Shows the eyedropper over the canvas, or restores the tool's cursor
    /// when passed nullptr.
    ///
    /// Set on the canvas widget rather than through an application override
    /// cursor: this dialog is modal, and the override route did not reliably
    /// take effect over the blocked main window.
    using CursorHook = std::function<void(const QCursor *)>;
    static void setCursorHook(CursorHook hook);

public slots:
    /// Take a colour the canvas reported while sampling was on.
    void addSample(const QPoint &documentPos, const QColor &color);

protected:
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
    /// One revert, optionally a mask rebuild, then one apply.
    /// `maskDirty` is false for the Hue/Saturation/Lightness sliders, which
    /// change the result but not which pixels are selected.
    void applyChange(bool maskDirty);
    void refreshMask();
    void refreshSwatches();

    /// Serialise the samples as "x,y,r,g,b;..." for the bridge.
    QString samplesString() const;

    // Eyedropper plumbing. Unlike ColorPickerDialog this never grabs the
    // mouse: grabbing froze other applications and stopped the window manager
    // from moving this dialog.
    /// Re-apply the dropper cursor for the current pick mode, so switching
    /// between Sample / Add / Subtract updates the badge without the pointer
    /// having to leave the image and come back.
    void refreshSamplingCursor();
    void clearCursorOverride();
    void applySample(const QPoint &documentPos, const QColor &color);

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

    bool m_cursorOverridden = false;
};

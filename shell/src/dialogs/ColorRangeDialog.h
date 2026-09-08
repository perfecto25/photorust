#pragma once

#include <QColor>
#include <QDialog>
#include <QPoint>

#include <functional>

class Engine;
class QCheckBox;
class QComboBox;
class QLabel;
class QSlider;
class QSpinBox;
class QToolButton;

/// CS6's Select ▸ Color Range: pick a range of colours and select everything
/// in the image that falls inside it.
///
/// The preview is the selection itself rather than an impression of it — the
/// same mask the engine will apply, drawn as the greyscale Photoshop shows,
/// so what you see is what you get when you press OK.
///
/// Sampled Colors takes its colour from the eyedropper, which picks straight
/// off the canvas — including from whatever is showing beneath this window.
///
/// That is why the dialog is **not modal**, following Replace Color for the
/// same reason: Qt delivers no mouse events at all to a window a modal dialog
/// has blocked, so a modal version could not sample the image however it
/// filtered events. The canvas is put into colour-sampling mode for the
/// dialog's lifetime, which also stops the active tool painting on the image
/// while the user is picking from it.
///
/// CS6's Localized Color Clusters, Detect Faces, Skin Tones and Out of Gamut
/// are not offered: the first two need clustering and face detection, and the
/// last two need a colour-managed profile to be out of. Its add and subtract
/// eyedroppers are not here either — this samples one colour at a time.
class ColorRangeDialog : public QDialog
{
    Q_OBJECT

public:
    explicit ColorRangeDialog(Engine *engine, QWidget *parent = nullptr);

    /// Shows the eyedropper over the canvas, or restores the tool's cursor
    /// when passed nullptr. Set on the canvas widget rather than as an
    /// application override, for the reason Replace Color gives.
    using CursorHook = std::function<void(const QCursor *)>;
    static void setCursorHook(CursorHook hook);

    /// Which of the Select list is chosen, in the engine's numbering.
    int range() const;
    QColor sampledColor() const { return m_sampled; }
    int fuzziness() const;
    bool inverted() const;

public slots:
    /// Take a colour the canvas reported while sampling was on.
    void takeSample(const QPoint &documentPos, const QColor &color);

protected:
    void hideEvent(QHideEvent *event) override;

private:
    /// Put the dropper cursor over the canvas, or take it away again.
    void refreshSamplingCursor();
    void refreshPreview();
    /// Fuzziness only means anything for a sampled colour and the tonal
    /// bands; a colour band is defined by its hue.
    void refreshEnabled();

    Engine *m_engine = nullptr;
    QComboBox *m_select = nullptr;
    QSpinBox *m_fuzziness = nullptr;
    QSlider *m_fuzzinessSlider = nullptr;
    QCheckBox *m_invert = nullptr;
    QLabel *m_preview = nullptr;
    QLabel *m_swatch = nullptr;
    QToolButton *m_eyedropper = nullptr;

    QColor m_sampled{Qt::black};
};

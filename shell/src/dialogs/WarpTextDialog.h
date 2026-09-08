#pragma once

#include <QDialog>

#include "../canvas/TypeWarp.h"

class QAbstractButton;
class QComboBox;
class QRadioButton;
class QSlider;
class QSpinBox;

/// CS6's Warp Text: a style, whether it runs along the text or across it, and
/// three amounts.
///
/// Photoshop previews the warp on the canvas as the sliders move, so this
/// reports every change rather than only its result — the caller applies it
/// and puts the old settings back if the dialog is cancelled.
class WarpTextDialog : public QDialog
{
    Q_OBJECT

public:
    WarpTextDialog(const TypeWarp &warp, QWidget *parent = nullptr);

    TypeWarp warp() const;

signals:
    /// Emitted whenever any control moves, for the live preview.
    void warpChanged(const TypeWarp &warp);

private:
    /// Grey the Horizontal/Vertical pair out for the radial styles, which
    /// have no along-or-across to choose.
    void refreshOrientationEnabled();
    void announce();

    QComboBox *m_style = nullptr;
    QRadioButton *m_horizontal = nullptr;
    QRadioButton *m_vertical = nullptr;
    QSpinBox *m_bend = nullptr;
    QSpinBox *m_hDistort = nullptr;
    QSpinBox *m_vDistort = nullptr;
    QSlider *m_bendSlider = nullptr;
    QSlider *m_hSlider = nullptr;
    QSlider *m_vSlider = nullptr;

    bool m_updating = false;
};

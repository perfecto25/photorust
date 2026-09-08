#pragma once

#include <QDialog>

class QCheckBox;

/// Photoshop's Lock Layers dialog — Layer ▸ Lock Layers… (`Ctrl+/`).
///
/// The same four locks the Layers panel's Lock row carries, as a dialog: CS6
/// offers both, and the dialog is the one that can set several at once. Lock
/// All is not a fifth lock but the other three together, which is why ticking
/// it ticks them and untying any of them unties it.
class LockLayersDialog : public QDialog
{
    Q_OBJECT

public:
    /// Opens with the locks the layer already carries.
    LockLayersDialog(bool transparency, bool pixels, bool position,
                     QWidget *parent = nullptr);

    bool lockTransparency() const;
    bool lockPixels() const;
    bool lockPosition() const;

private:
    /// Keep Lock All in step with the three it stands for.
    void syncLockAll();

    QCheckBox *m_transparency = nullptr;
    QCheckBox *m_pixels = nullptr;
    QCheckBox *m_position = nullptr;
    QCheckBox *m_all = nullptr;
    /// Guards the two-way sync between Lock All and the individual locks.
    bool m_syncing = false;
};

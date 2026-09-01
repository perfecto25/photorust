#pragma once

#include <QDialog>
#include <QString>

class Engine;
class QCheckBox;
class QComboBox;
class QLineEdit;
class QSpinBox;

/// Photoshop's New Layer dialog.
///
/// Serves both Layer ▸ New ▸ Layer… and Layer ▸ New ▸ Layer From Background…,
/// which in CS6 are the same dialog under different titles — the second one
/// names the layer the Background is about to become.
class NewLayerDialog : public QDialog
{
    Q_OBJECT
public:
    /// `fromBackground` switches to the Layer From Background wording and
    /// drops the clipping-mask option, which has nothing to clip to at the
    /// bottom of the stack.
    NewLayerDialog(Engine *engine, bool fromBackground, QWidget *parent = nullptr);

    /// Open with this name in the box, selected — what a fill layer wants,
    /// since CS6 offers "Color Fill 1" rather than an empty field.
    void presetName(const QString &name);

    QString layerName() const;
    /// Blend mode as a `BlendMode` discriminant, matching the engine's order.
    int blendMode() const;
    int opacityPercent() const;
    /// CS6's layer row colour: 0 for None, then Red through Gray.
    int labelColor() const;
    bool useClippingMask() const;

private:
    void populateBlendModes();

    Engine *m_engine = nullptr;
    QLineEdit *m_name = nullptr;
    QComboBox *m_mode = nullptr;
    QComboBox *m_label = nullptr;
    QSpinBox *m_opacity = nullptr;
    QCheckBox *m_clipping = nullptr;
};

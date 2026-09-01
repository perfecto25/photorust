#pragma once

#include <QColor>
#include <QDialog>

class AngleDial;
class Engine;
class QCheckBox;
class QComboBox;
class QDoubleSpinBox;
class QPushButton;
class QSpinBox;

/// Photoshop's **Gradient Fill** dialog, the second half of Layer ▸ New Fill
/// Layer ▸ Gradient.
///
/// Two stops rather than a full gradient editor, which is the part of CS6's
/// panel not reproduced here.
class GradientFillDialog : public QDialog
{
    Q_OBJECT
public:
    /// Edits the fill layer the caller has already created, so the canvas
    /// shows every change as it is made — which is the whole reason CS6 makes
    /// the layer first and asks afterwards.
    explicit GradientFillDialog(Engine *engine, QWidget *parent = nullptr);

    QString preset() const { return m_preset; }
    /// The Gradient tool's own order: linear, radial, angle, reflected, diamond.
    int shape() const;
    double angle() const;
    int scalePercent() const;
    bool reverse() const;
    bool dither() const;
    bool alignWithLayer() const;

private:
    /// Push the current settings into the previewed layer.
    void apply();
    /// CS6's preset grid, dropped from the gradient swatch.
    void showPresets();
    void paintPreset();

    Engine *m_engine = nullptr;
    QPushButton *m_gradient = nullptr;
    QComboBox *m_shape = nullptr;
    AngleDial *m_dial = nullptr;
    QDoubleSpinBox *m_angle = nullptr;
    QSpinBox *m_scale = nullptr;
    QCheckBox *m_reverse = nullptr;
    QCheckBox *m_dither = nullptr;
    QCheckBox *m_align = nullptr;
    QString m_preset;
    bool m_updating = false;
};

/// Photoshop's **Pattern Fill** dialog.
class PatternFillDialog : public QDialog
{
    Q_OBJECT
public:
    explicit PatternFillDialog(Engine *engine, QWidget *parent = nullptr);

    int pattern() const;
    int scalePercent() const;
    bool linkWithLayer() const;

private:
    /// Push the current settings into the previewed layer.
    void apply();

    Engine *m_engine = nullptr;
    QComboBox *m_pattern = nullptr;
    QSpinBox *m_scale = nullptr;
    QCheckBox *m_link = nullptr;
};

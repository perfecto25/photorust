#pragma once

#include <QCheckBox>
#include <QComboBox>
#include <QDialog>
#include <QSlider>
#include <QSpinBox>
#include <QWidget>

class Engine;

class SpectrumBar : public QWidget
{
    Q_OBJECT
public:
    explicit SpectrumBar(QWidget *parent = nullptr);
    void setHueShift(int degrees);

protected:
    void paintEvent(QPaintEvent *) override;

private:
    int m_hueShift = 0;
};

/// One entry of CS6's Preset menu. Hue is in degrees, the other two in
/// percent — the numbers as the controls show them, not as the engine takes
/// them.
struct HueSatPreset {
    const char *name;
    int hue;
    int saturation;
    int lightness;
    bool colorize;
};

/// The preset list, shared with the Properties panel so the two offer the same
/// menu. "Default" is first and "Custom" is not in here: it is what the combo
/// shows once the sliders no longer match any preset.
const QList<HueSatPreset> &hueSaturationPresets();

class HueSaturationDialog : public QDialog
{
    Q_OBJECT
public:
    explicit HueSaturationDialog(Engine *engine, QWidget *parent = nullptr);
    ~HueSaturationDialog() override;

private:
    void applyPreview();
    void revertPreview();
    void onValueChanged();
    void applyPreset(int index);

    Engine *m_engine = nullptr;
    bool m_previewApplied = false;
    bool m_applyingPreset = false;

    QComboBox *m_presetCombo = nullptr;
    QComboBox *m_channelCombo = nullptr;
    QSlider *m_hueSlider = nullptr;
    QSpinBox *m_hueSpin = nullptr;
    QSlider *m_saturationSlider = nullptr;
    QSpinBox *m_saturationSpin = nullptr;
    QSlider *m_lightnessSlider = nullptr;
    QSpinBox *m_lightnessSpin = nullptr;
    QCheckBox *m_colorize = nullptr;
    QCheckBox *m_preview = nullptr;
    SpectrumBar *m_spectrumTop = nullptr;
    SpectrumBar *m_spectrumBottom = nullptr;
};

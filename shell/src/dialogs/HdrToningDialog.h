#pragma once

#include <QCheckBox>
#include <QComboBox>
#include <QDialog>
#include <QDoubleSpinBox>
#include <QSlider>
#include <QSpinBox>

class Engine;

class HdrToningDialog : public QDialog
{
    Q_OBJECT
public:
    explicit HdrToningDialog(Engine *engine, QWidget *parent = nullptr);
    ~HdrToningDialog() override;

private:
    void applyPreview();
    void revertPreview();
    void onValueChanged();
    void loadPreset(int index);

    Engine *m_engine = nullptr;
    bool m_previewApplied = false;
    bool m_loading = false;

    QComboBox *m_preset = nullptr;

    // Edge Glow
    QSlider *m_radiusSlider = nullptr;
    QSpinBox *m_radiusSpin = nullptr;
    QSlider *m_strengthSlider = nullptr;
    QDoubleSpinBox *m_strengthSpin = nullptr;
    QCheckBox *m_smoothEdges = nullptr;

    // Tone and Detail
    QSlider *m_gammaSlider = nullptr;
    QDoubleSpinBox *m_gammaSpin = nullptr;
    QSlider *m_exposureSlider = nullptr;
    QDoubleSpinBox *m_exposureSpin = nullptr;
    QSlider *m_detailSlider = nullptr;
    QSpinBox *m_detailSpin = nullptr;

    // Advanced
    QSlider *m_shadowSlider = nullptr;
    QSpinBox *m_shadowSpin = nullptr;
    QSlider *m_highlightSlider = nullptr;
    QSpinBox *m_highlightSpin = nullptr;
    QSlider *m_vibranceSlider = nullptr;
    QSpinBox *m_vibranceSpin = nullptr;
    QSlider *m_saturationSlider = nullptr;
    QSpinBox *m_saturationSpin = nullptr;

    QCheckBox *m_preview = nullptr;
};

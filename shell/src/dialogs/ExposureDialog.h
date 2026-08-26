#pragma once

#include <QCheckBox>
#include <QComboBox>
#include <QDialog>
#include <QDoubleSpinBox>
#include <QSlider>

class Engine;

class ExposureDialog : public QDialog
{
    Q_OBJECT
public:
    explicit ExposureDialog(Engine *engine, QWidget *parent = nullptr);
    ~ExposureDialog() override;

private:
    void applyPreview();
    void revertPreview();
    void onValueChanged();
    void applyPreset(int index);

    Engine *m_engine = nullptr;
    bool m_previewApplied = false;
    bool m_applyingPreset = false;

    QComboBox *m_presetCombo = nullptr;
    QSlider *m_exposureSlider = nullptr;
    QDoubleSpinBox *m_exposureSpin = nullptr;
    QSlider *m_offsetSlider = nullptr;
    QDoubleSpinBox *m_offsetSpin = nullptr;
    QSlider *m_gammaSlider = nullptr;
    QDoubleSpinBox *m_gammaSpin = nullptr;
    QCheckBox *m_preview = nullptr;
};

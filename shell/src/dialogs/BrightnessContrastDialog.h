#pragma once

#include <QCheckBox>
#include <QDialog>
#include <QSlider>
#include <QSpinBox>

class Engine;

class BrightnessContrastDialog : public QDialog
{
    Q_OBJECT
public:
    explicit BrightnessContrastDialog(Engine *engine, QWidget *parent = nullptr);
    ~BrightnessContrastDialog() override;

private:
    void applyPreview();
    void revertPreview();
    void onValueChanged();

    Engine *m_engine = nullptr;
    bool m_previewApplied = false;

    QSlider *m_brightnessSlider = nullptr;
    QSpinBox *m_brightnessSpin = nullptr;
    QSlider *m_contrastSlider = nullptr;
    QSpinBox *m_contrastSpin = nullptr;
    QCheckBox *m_preview = nullptr;
};

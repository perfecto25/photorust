#pragma once

#include <QCheckBox>
#include <QDialog>
#include <QSlider>
#include <QSpinBox>

class Engine;

class VibranceDialog : public QDialog
{
    Q_OBJECT
public:
    explicit VibranceDialog(Engine *engine, QWidget *parent = nullptr);
    ~VibranceDialog() override;

private:
    void applyPreview();
    void revertPreview();
    void onValueChanged();

    Engine *m_engine = nullptr;
    bool m_previewApplied = false;

    QSlider *m_vibranceSlider = nullptr;
    QSpinBox *m_vibranceSpin = nullptr;
    QSlider *m_saturationSlider = nullptr;
    QSpinBox *m_saturationSpin = nullptr;
    QCheckBox *m_preview = nullptr;
};

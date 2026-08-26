#pragma once

#include <QCheckBox>
#include <QComboBox>
#include <QDialog>
#include <QLabel>
#include <QSlider>
#include <QSpinBox>

class Engine;

class BlackWhiteDialog : public QDialog
{
    Q_OBJECT
public:
    explicit BlackWhiteDialog(Engine *engine, QWidget *parent = nullptr);
    ~BlackWhiteDialog() override;

protected:
    bool eventFilter(QObject *obj, QEvent *event) override;

private:
    void applyPreview();
    void revertPreview();
    void onValueChanged();
    void applyPreset(int index);
    void openTintColorPicker();
    void updateTintSwatch();

    Engine *m_engine = nullptr;
    bool m_previewApplied = false;
    bool m_applyingPreset = false;

    QComboBox *m_presetCombo = nullptr;
    QSlider *m_sliders[6]{};
    QSpinBox *m_spins[6]{};
    QCheckBox *m_tint = nullptr;
    QLabel *m_tintSwatch = nullptr;
    QSlider *m_hueSlider = nullptr;
    QSpinBox *m_hueSpin = nullptr;
    QSlider *m_satSlider = nullptr;
    QSpinBox *m_satSpin = nullptr;
    QCheckBox *m_preview = nullptr;
};

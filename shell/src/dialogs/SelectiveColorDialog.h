#pragma once

#include <QCheckBox>
#include <QComboBox>
#include <QDialog>
#include <QRadioButton>
#include <QSlider>
#include <QSpinBox>

class Engine;

class SelectiveColorDialog : public QDialog
{
    Q_OBJECT
public:
    explicit SelectiveColorDialog(Engine *engine, QWidget *parent = nullptr);
    ~SelectiveColorDialog() override;

private:
    void applyPreview();
    void revertPreview();
    void onValueChanged();
    void onColorChanged(int index);
    void saveCurrentRange();
    void loadCurrentRange();

    Engine *m_engine = nullptr;
    bool m_previewApplied = false;

    QComboBox *m_colorCombo = nullptr;
    QSlider *m_cyanSlider = nullptr;
    QSpinBox *m_cyanSpin = nullptr;
    QSlider *m_magentaSlider = nullptr;
    QSpinBox *m_magentaSpin = nullptr;
    QSlider *m_yellowSlider = nullptr;
    QSpinBox *m_yellowSpin = nullptr;
    QSlider *m_blackSlider = nullptr;
    QSpinBox *m_blackSpin = nullptr;
    QRadioButton *m_relative = nullptr;
    QRadioButton *m_absolute = nullptr;
    QCheckBox *m_preview = nullptr;

    int m_currentRange = 0;
    float m_adjustments[9][4] = {};
};

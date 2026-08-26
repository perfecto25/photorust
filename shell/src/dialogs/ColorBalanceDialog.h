#pragma once

#include <QButtonGroup>
#include <QCheckBox>
#include <QDialog>
#include <QRadioButton>
#include <QSlider>
#include <QSpinBox>

class Engine;

class ColorBalanceDialog : public QDialog
{
    Q_OBJECT
public:
    explicit ColorBalanceDialog(Engine *engine, QWidget *parent = nullptr);
    ~ColorBalanceDialog() override;

private:
    void applyPreview();
    void revertPreview();
    void onValueChanged();

    Engine *m_engine = nullptr;
    bool m_previewApplied = false;

    QSlider *m_cyanRedSlider = nullptr;
    QSpinBox *m_cyanRedSpin = nullptr;
    QSlider *m_magentaGreenSlider = nullptr;
    QSpinBox *m_magentaGreenSpin = nullptr;
    QSlider *m_yellowBlueSlider = nullptr;
    QSpinBox *m_yellowBlueSpin = nullptr;
    QButtonGroup *m_toneGroup = nullptr;
    QCheckBox *m_preserveLuminosity = nullptr;
    QCheckBox *m_preview = nullptr;
};

#pragma once

#include <QComboBox>
#include <QCheckBox>
#include <QDialog>
#include <QDoubleSpinBox>
#include <QLabel>
#include <QSpinBox>

class ColorSettingsDialog : public QDialog
{
    Q_OBJECT
public:
    explicit ColorSettingsDialog(QWidget *parent = nullptr);

    static QString configPath();

private:
    void buildUi();
    void loadSettings();
    void saveSettings();
    void updateDescription(const QString &text);
    void onSettingsPresetChanged(int index);

    QComboBox *m_settingsPreset = nullptr;

    // Working Spaces
    QComboBox *m_rgbSpace = nullptr;
    QComboBox *m_cmykSpace = nullptr;
    QComboBox *m_graySpace = nullptr;
    QComboBox *m_spotSpace = nullptr;

    // Color Management Policies
    QComboBox *m_rgbPolicy = nullptr;
    QComboBox *m_cmykPolicy = nullptr;
    QComboBox *m_grayPolicy = nullptr;
    QCheckBox *m_mismatchAskOpen = nullptr;
    QCheckBox *m_mismatchAskPaste = nullptr;
    QCheckBox *m_missingAskOpen = nullptr;

    // Conversion Options
    QComboBox *m_engine = nullptr;
    QComboBox *m_intent = nullptr;
    QCheckBox *m_blackPoint = nullptr;
    QCheckBox *m_dither = nullptr;
    QCheckBox *m_sceneReferred = nullptr;

    // Advanced Controls
    QCheckBox *m_desaturateCheck = nullptr;
    QSpinBox *m_desaturateSpin = nullptr;
    QCheckBox *m_blendRgbCheck = nullptr;
    QDoubleSpinBox *m_blendRgbSpin = nullptr;
    QCheckBox *m_blendTextCheck = nullptr;
    QDoubleSpinBox *m_blendTextSpin = nullptr;

    QCheckBox *m_preview = nullptr;
    QLabel *m_descLabel = nullptr;
};

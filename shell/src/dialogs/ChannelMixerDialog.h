#pragma once

#include <QCheckBox>
#include <QComboBox>
#include <QDialog>
#include <QLabel>
#include <QSlider>
#include <QSpinBox>

class Engine;

class ChannelMixerDialog : public QDialog
{
    Q_OBJECT
public:
    explicit ChannelMixerDialog(Engine *engine, QWidget *parent = nullptr);
    ~ChannelMixerDialog() override;

private:
    void applyPreview();
    void revertPreview();
    void onValueChanged();
    void onOutputChannelChanged(int index);
    void onMonochromeToggled(bool checked);
    void applyPreset(int index);
    void updateTotal();
    void loadChannelToUi();
    void saveUiToChannel();

    Engine *m_engine = nullptr;
    bool m_previewApplied = false;
    bool m_applyingPreset = false;
    bool m_updatingUi = false;

    QComboBox *m_presetCombo = nullptr;
    QComboBox *m_outputCombo = nullptr;

    QSlider *m_sliders[3]{};
    QSpinBox *m_spins[3]{};
    QLabel *m_totalLabel = nullptr;

    QSlider *m_constSlider = nullptr;
    QSpinBox *m_constSpin = nullptr;

    QCheckBox *m_monochrome = nullptr;
    QCheckBox *m_preview = nullptr;

    // 3x3 matrix: rows = output channels (R,G,B), cols = source (R,G,B)
    // matrix[outCh][srcCh]
    int m_matrix[3][3]{};
    int m_constants[3]{};
    int m_currentOutput = 0;
};

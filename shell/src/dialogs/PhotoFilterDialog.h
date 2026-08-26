#pragma once

#include <QCheckBox>
#include <QComboBox>
#include <QDialog>
#include <QLabel>
#include <QRadioButton>
#include <QSlider>
#include <QSpinBox>

class Engine;

class PhotoFilterDialog : public QDialog
{
    Q_OBJECT
public:
    explicit PhotoFilterDialog(Engine *engine, QWidget *parent = nullptr);
    ~PhotoFilterDialog() override;

protected:
    bool eventFilter(QObject *obj, QEvent *event) override;

private:
    void applyPreview();
    void revertPreview();
    void onValueChanged();
    void onFilterChanged(int index);
    void openColorPicker();
    void updateColorSwatch();

    Engine *m_engine = nullptr;
    bool m_previewApplied = false;

    QRadioButton *m_radioFilter = nullptr;
    QRadioButton *m_radioColor = nullptr;
    QComboBox *m_filterCombo = nullptr;
    QLabel *m_colorSwatch = nullptr;
    QSlider *m_densitySlider = nullptr;
    QSpinBox *m_densitySpin = nullptr;
    QCheckBox *m_preserveLum = nullptr;
    QCheckBox *m_preview = nullptr;

    QColor m_currentColor;
};

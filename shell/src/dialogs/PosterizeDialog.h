#pragma once

#include <QCheckBox>
#include <QDialog>
#include <QSlider>
#include <QSpinBox>

class Engine;

class PosterizeDialog : public QDialog
{
    Q_OBJECT
public:
    explicit PosterizeDialog(Engine *engine, QWidget *parent = nullptr);
    ~PosterizeDialog() override;

private:
    void applyPreview();
    void revertPreview();
    void onValueChanged();

    Engine *m_engine = nullptr;
    bool m_previewApplied = false;

    QSpinBox *m_levelsSpin = nullptr;
    QSlider *m_levelsSlider = nullptr;
    QCheckBox *m_preview = nullptr;
};

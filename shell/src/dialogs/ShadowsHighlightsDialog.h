#pragma once

#include <QCheckBox>
#include <QDialog>
#include <QSlider>
#include <QSpinBox>

class Engine;

class ShadowsHighlightsDialog : public QDialog
{
    Q_OBJECT
public:
    explicit ShadowsHighlightsDialog(Engine *engine, QWidget *parent = nullptr);
    ~ShadowsHighlightsDialog() override;

private:
    void applyPreview();
    void revertPreview();
    void onValueChanged();

    Engine *m_engine = nullptr;
    bool m_previewApplied = false;

    QSlider *m_shadowSlider = nullptr;
    QSpinBox *m_shadowSpin = nullptr;
    QSlider *m_highlightSlider = nullptr;
    QSpinBox *m_highlightSpin = nullptr;
    QCheckBox *m_preview = nullptr;
};

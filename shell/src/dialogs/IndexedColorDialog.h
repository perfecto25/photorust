#pragma once

#include <QCheckBox>
#include <QComboBox>
#include <QDialog>
#include <QSpinBox>

class Engine;

class IndexedColorDialog : public QDialog
{
    Q_OBJECT
public:
    explicit IndexedColorDialog(Engine *engine, QWidget *parent = nullptr);
    ~IndexedColorDialog() override;

    int colors() const;
    int ditherIndex() const;
    int ditherAmount() const;

private:
    void onPaletteChanged(int index);
    void onDitherChanged(int index);
    void applyPreview();
    void revertPreview();

    Engine *m_engine = nullptr;
    bool m_previewApplied = false;

    QComboBox *m_palette = nullptr;
    QSpinBox *m_colors = nullptr;
    QComboBox *m_forced = nullptr;
    QCheckBox *m_transparency = nullptr;

    QComboBox *m_matte = nullptr;
    QComboBox *m_dither = nullptr;
    QSpinBox *m_amount = nullptr;
    QCheckBox *m_preserveExact = nullptr;
    QCheckBox *m_preview = nullptr;
};

#pragma once

#include "GradientEditorDialog.h"

#include <QCheckBox>
#include <QDialog>
#include <QLabel>
#include <QMenu>
#include <QToolButton>

class Engine;

class GradientMapDialog : public QDialog
{
    Q_OBJECT
public:
    explicit GradientMapDialog(Engine *engine, QWidget *parent = nullptr);
    ~GradientMapDialog() override;

protected:
    bool eventFilter(QObject *obj, QEvent *event) override;

private:
    void applyPreview();
    void revertPreview();
    void onValueChanged();
    void updateSwatchIcon();
    void openGradientEditor();

    Engine *m_engine = nullptr;
    bool m_previewApplied = false;
    bool m_useCustom = false;

    QToolButton *m_gradientBtn = nullptr;
    QCheckBox *m_dither = nullptr;
    QCheckBox *m_reverse = nullptr;
    QCheckBox *m_preview = nullptr;

    QString m_currentGradient;
    QVector<GradientColorStop> m_customStops;
};

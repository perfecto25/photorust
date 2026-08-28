#pragma once

#include <QCheckBox>
#include <QComboBox>
#include <QDialog>
#include <QLabel>
#include <QRadioButton>
#include <QSpinBox>

class Engine;

class StrokeDialog : public QDialog
{
    Q_OBJECT
public:
    explicit StrokeDialog(Engine *engine, QWidget *parent = nullptr);

    int strokeWidth() const;
    QColor strokeColor() const;
    int location() const; // 0=Inside, 1=Center, 2=Outside
    int blendModeIndex() const;
    int opacity() const;
    bool preserveTransparency() const;

protected:
    bool eventFilter(QObject *obj, QEvent *event) override;

private:
    void openColorPicker();
    void updateColorSwatch();

    Engine *m_engine = nullptr;

    QSpinBox *m_width = nullptr;
    QLabel *m_colorSwatch = nullptr;
    QColor m_color;

    QRadioButton *m_inside = nullptr;
    QRadioButton *m_center = nullptr;
    QRadioButton *m_outside = nullptr;

    QComboBox *m_mode = nullptr;
    QSpinBox *m_opacity = nullptr;
    QCheckBox *m_preserveTransp = nullptr;
};

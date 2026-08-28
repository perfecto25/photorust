#pragma once

#include <QCheckBox>
#include <QComboBox>
#include <QDialog>
#include <QSpinBox>

class QGridLayout;
class QGroupBox;
class QLabel;
class QToolButton;
class Engine;

class FillDialog : public QDialog
{
    Q_OBJECT
public:
    explicit FillDialog(Engine *engine, QWidget *parent = nullptr);

    QColor fillColor() const;
    int blendModeIndex() const;
    int opacity() const;
    bool preserveTransparency() const;

    bool isPatternFill() const;
    int selectedPatternIndex() const;

private:
    void onContentsChanged(int index);
    void buildPatternGrid();

    Engine *m_engine = nullptr;
    QComboBox *m_contents = nullptr;
    QComboBox *m_mode = nullptr;
    QSpinBox *m_opacity = nullptr;
    QCheckBox *m_preserveTransp = nullptr;

    QGroupBox *m_patternGroup = nullptr;
    QToolButton *m_patternSwatch = nullptr;
    QWidget *m_patternPopup = nullptr;
    int m_selectedPattern = 0;

    QColor m_customColor;
};

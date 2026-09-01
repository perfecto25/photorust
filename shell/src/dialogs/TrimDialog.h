#pragma once

#include <QDialog>

class Engine;
class QCheckBox;
class QRadioButton;

/// Photoshop's Image ▸ Trim.
///
/// Cuts uniform border off the edges of the flattened image — either the
/// transparent part or whatever colour sits in one of the two corners — and
/// only off the edges that are ticked.
class TrimDialog : public QDialog
{
    Q_OBJECT
public:
    explicit TrimDialog(Engine *engine, QWidget *parent = nullptr);

    /// 0 transparent pixels, 1 top-left colour, 2 bottom-right colour.
    int basis() const;
    bool trimTop() const;
    bool trimBottom() const;
    bool trimLeft() const;
    bool trimRight() const;

private:
    QRadioButton *m_transparent = nullptr;
    QRadioButton *m_topLeft = nullptr;
    QRadioButton *m_bottomRight = nullptr;
    QCheckBox *m_top = nullptr;
    QCheckBox *m_bottom = nullptr;
    QCheckBox *m_left = nullptr;
    QCheckBox *m_right = nullptr;
};

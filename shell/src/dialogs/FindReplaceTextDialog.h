#pragma once

#include <QDialog>

class QCheckBox;
class QLineEdit;
class QPushButton;

class CanvasView;
class Engine;

/// Edit > Find And Replace Text: CS6-style dialog for searching and replacing
/// text content across type layers.
class FindReplaceTextDialog : public QDialog
{
    Q_OBJECT

public:
    explicit FindReplaceTextDialog(Engine *engine, CanvasView *canvas,
                                   QWidget *parent = nullptr);
    ~FindReplaceTextDialog() override;

private slots:
    void findNext();
    void changeText();
    void changeAll();
    void changeAndFind();

private:
    struct Match {
        int layerIndex = -1;
        int charOffset = -1;
        int length = 0;
    };

    QString layerFullText(int layerIndex) const;
    Match findInLayer(int layerIndex, int startChar) const;
    bool replaceMatch(const Match &match);
    void updateButtons();

    Engine *m_engine = nullptr;
    CanvasView *m_canvas = nullptr;

    QLineEdit *m_findEdit = nullptr;
    QLineEdit *m_changeEdit = nullptr;
    QCheckBox *m_searchAllLayers = nullptr;
    QCheckBox *m_forward = nullptr;
    QCheckBox *m_caseSensitive = nullptr;
    QCheckBox *m_wholeWord = nullptr;
    QCheckBox *m_ignoreAccents = nullptr;
    QPushButton *m_findNextBtn = nullptr;
    QPushButton *m_changeBtn = nullptr;
    QPushButton *m_changeAllBtn = nullptr;
    QPushButton *m_changeFindBtn = nullptr;

    Match m_currentMatch;
};

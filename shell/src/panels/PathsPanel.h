#pragma once

#include <QListWidget>
#include <QToolButton>
#include <QWidget>

class Engine;

/// The Paths panel.
///
/// One row per saved path, plus the transient "Work Path" the Pen tool starts
/// automatically the first time it is used with nothing selected here —
/// exactly as Photoshop's panel behaves. Selecting a row makes it the path the
/// Pen, Path Selection and Direct Selection tools act on; double-click a name
/// to rename it, which is what "saves" a Work Path in Photoshop's own sense
/// (a Work Path is otherwise replaced the next time you start drawing with
/// nothing selected).
class PathsPanel : public QWidget
{
    Q_OBJECT

public:
    explicit PathsPanel(Engine *engine, QWidget *parent = nullptr);

public slots:
    /// Rebuild the list from the engine.
    void refresh();

signals:
    /// Something changed that requires the canvas to repaint — a new active
    /// path, or Fill/Stroke Path editing pixels.
    void documentChanged();

private slots:
    void onSelectionChanged();
    void onItemChanged(QListWidgetItem *item);
    void onRowContextMenu(const QPoint &pos);

    void addPath();
    void duplicatePath();
    void deletePath();
    void loadSelection();
    void fillPath();
    void strokePath();

private:
    void buildUi();
    /// Row index currently selected, or -1.
    int currentIndex() const;

    Engine *m_engine = nullptr;
    QListWidget *m_list = nullptr;

    QToolButton *m_fillButton = nullptr;
    QToolButton *m_strokeButton = nullptr;
    QToolButton *m_loadSelectionButton = nullptr;
    QToolButton *m_addButton = nullptr;
    QToolButton *m_deleteButton = nullptr;

    /// Guards against re-entrancy: refresh() writes to the list, whose change
    /// signals would otherwise write straight back to the engine.
    bool m_updating = false;
};

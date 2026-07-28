#pragma once

#include <QListWidget>
#include <QWidget>

class Engine;

/// The History panel.
///
/// A linear list of states with a cursor. Rows after the cursor are dimmed —
/// they are still reachable by clicking, but performing a new action discards
/// them, exactly as Photoshop behaves.
class HistoryPanel : public QWidget
{
    Q_OBJECT

public:
    explicit HistoryPanel(Engine *engine, QWidget *parent = nullptr);

public slots:
    void refresh();

signals:
    /// The user jumped to a different state; the canvas must repaint.
    void documentChanged();

private slots:
    void onRowActivated(int row);

private:
    Engine *m_engine = nullptr;
    QListWidget *m_list = nullptr;
    bool m_updating = false;
};

#pragma once

#include <QListWidget>
#include <QToolButton>
#include <QWidget>

class Engine;
class ChannelListWidget;

/// The Channels panel — CS6's Layers/Channels/Paths tab group.
///
/// Shows the composite channel and individual color channels for the current
/// color mode. Each row has a visibility eye and a grayscale thumbnail of that
/// channel's data extracted from the composite image.
class ChannelsPanel : public QWidget
{
    Q_OBJECT

public:
    explicit ChannelsPanel(Engine *engine, QWidget *parent = nullptr);

public slots:
    void refresh();

private:
    void buildUi();
    void toggleVisibility(int row);

    Engine *m_engine = nullptr;
    ChannelListWidget *m_list = nullptr;

    QToolButton *m_loadSelectionButton = nullptr;
    QToolButton *m_addButton = nullptr;
    QToolButton *m_deleteButton = nullptr;
};

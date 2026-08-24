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

signals:
    /// Emitted when the channel visibility mask changes.
    /// Bits: 0 = Red/Cyan, 1 = Green/Magenta, 2 = Blue/Yellow, 3 = Black(K).
    /// 0xFF = all visible.
    void channelMaskChanged(uint8_t mask);

private:
    void buildUi();
    void toggleVisibility(int row);
    void updateMask();
    void addChannel();
    void deleteChannel();

    Engine *m_engine = nullptr;
    ChannelListWidget *m_list = nullptr;

    QToolButton *m_loadSelectionButton = nullptr;
    QToolButton *m_addButton = nullptr;
    QToolButton *m_deleteButton = nullptr;

    /// Number of built-in channels for the current mode (composite + components).
    int m_builtinCount = 0;
};

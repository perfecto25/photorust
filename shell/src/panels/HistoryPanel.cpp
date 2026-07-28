#include "HistoryPanel.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QVBoxLayout>

HistoryPanel::HistoryPanel(Engine *engine, QWidget *parent)
    : QWidget(parent)
    , m_engine(engine)
{
    auto *root = new QVBoxLayout(this);
    root->setContentsMargins(0, 0, 0, 0);
    root->setSpacing(0);

    m_list = new QListWidget(this);
    m_list->setObjectName(QStringLiteral("historyList"));
    m_list->setSelectionMode(QAbstractItemView::SingleSelection);
    root->addWidget(m_list, 1);

    connect(m_list, &QListWidget::currentRowChanged,
            this, &HistoryPanel::onRowActivated);

    refresh();
}

void HistoryPanel::refresh()
{
    if (!m_engine) {
        return;
    }
    m_updating = true;

    const int count = m_engine->historyCount();
    const int cursor = m_engine->historyCursor();

    m_list->clear();
    for (int i = 0; i < count; ++i) {
        auto *item = new QListWidgetItem(m_engine->historyName(i));
        // States past the cursor are "undone": still clickable, but shown
        // greyed to signal they will be discarded by the next edit.
        if (i > cursor) {
            item->setForeground(QColor(0x7d, 0x7d, 0x7d));
        }
        m_list->addItem(item);
    }

    if (cursor >= 0 && cursor < count) {
        m_list->setCurrentRow(cursor);
        m_list->scrollToItem(m_list->item(cursor));
    }

    m_updating = false;
}

void HistoryPanel::onRowActivated(int row)
{
    if (m_updating || !m_engine || row < 0) {
        return;
    }
    if (row == m_engine->historyCursor()) {
        return;
    }
    m_engine->jumpToHistory(row);
    emit documentChanged();
    refresh();
}

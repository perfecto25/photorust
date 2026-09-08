#include "ParagraphStylesPanel.h"

#include "../tools/ToolIcons.h"

#include <QHBoxLayout>
#include <QListWidget>
#include <QToolButton>
#include <QVBoxLayout>

namespace {

/// The tint the rest of the panel chrome uses.
const QColor kFooterIconColor(0xd4, 0xd4, 0xd4);

} // namespace

ParagraphStylesPanel::ParagraphStylesPanel(QWidget *parent)
    : QWidget(parent)
{
    auto *root = new QVBoxLayout(this);
    root->setContentsMargins(0, 0, 0, 0);
    root->setSpacing(0);

    m_list = new QListWidget(this);
    m_list->setSelectionMode(QAbstractItemView::SingleSelection);
    root->addWidget(m_list, 1);

    // CS6 ships every document with one style, and it cannot be deleted.
    m_styles.append(ParagraphStyle{});
    refreshList(0);

    // A single click applies, a double-click opens the options — the way the
    // panel behaves in CS6.
    connect(m_list, &QListWidget::itemClicked, this, [this](QListWidgetItem *item) {
        const int row = m_list->row(item);
        if (row >= 0 && row < m_styles.size()) {
            emit styleApplied(m_styles.at(row));
        }
    });
    connect(m_list, &QListWidget::itemDoubleClicked, this, [this](QListWidgetItem *item) {
        editStyle(m_list->row(item));
    });

    auto *footer = new QWidget(this);
    footer->setObjectName(QStringLiteral("panelFooter"));
    auto *row = new QHBoxLayout(footer);
    row->setContentsMargins(4, 2, 4, 2);
    row->setSpacing(2);
    row->addStretch(1);

    auto *newStyle = new QToolButton(footer);
    newStyle->setAutoRaise(true);
    newStyle->setIcon(ToolIcons::fromSvgBody(
        QStringLiteral(R"SVG(<rect x="4" y="3" width="12" height="14" stroke-width="1.2"/>)SVG"),
        kFooterIconColor));
    newStyle->setToolTip(tr("Create a new paragraph style"));
    connect(newStyle, &QToolButton::clicked, this, &ParagraphStylesPanel::addStyle);
    row->addWidget(newStyle);

    auto *deleteStyle = new QToolButton(footer);
    deleteStyle->setAutoRaise(true);
    deleteStyle->setIcon(ToolIcons::fromSvgBody(
        QStringLiteral(R"SVG(<path d="M5 6h10M8 6V4h4v2M6.5 6l0.7 11h5.6l0.7-11"
                  stroke-width="1.2"/>)SVG"),
        kFooterIconColor));
    deleteStyle->setToolTip(tr("Delete the selected paragraph style"));
    connect(deleteStyle, &QToolButton::clicked, this,
            &ParagraphStylesPanel::deleteSelectedStyle);
    row->addWidget(deleteStyle);

    root->addWidget(footer);
}

void ParagraphStylesPanel::refreshList(int current)
{
    const QSignalBlocker blocker(m_list);
    m_list->clear();
    for (const ParagraphStyle &style : std::as_const(m_styles)) {
        m_list->addItem(style.name);
    }
    if (current >= 0 && current < m_list->count()) {
        m_list->setCurrentRow(current);
    }
}

void ParagraphStylesPanel::addStyle()
{
    ParagraphStyle style;
    style.name = tr("Paragraph Style %1").arg(m_styles.size());

    ParagraphStyleDialog dialog(style, this);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    m_styles.append(dialog.style());
    refreshList(m_styles.size() - 1);
}

void ParagraphStylesPanel::deleteSelectedStyle()
{
    const int row = m_list->currentRow();
    // Row 0 is the basic style every document has; CS6 will not delete it
    // either.
    if (row <= 0 || row >= m_styles.size()) {
        return;
    }
    m_styles.removeAt(row);
    refreshList(qMin(row, m_styles.size() - 1));
}

void ParagraphStylesPanel::editStyle(int row)
{
    if (row < 0 || row >= m_styles.size()) {
        return;
    }
    ParagraphStyleDialog dialog(m_styles.at(row), this);
    if (dialog.exec() != QDialog::Accepted) {
        return;
    }
    m_styles[row] = dialog.style();
    refreshList(row);
    // Editing a style is how CS6 restyles the text set in it, so the edit
    // takes effect rather than only being recorded.
    emit styleApplied(m_styles.at(row));
}

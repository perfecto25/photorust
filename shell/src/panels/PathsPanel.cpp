#include "PathsPanel.h"

#include "PathIcons.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QHBoxLayout>
#include <QInputDialog>
#include <QMenu>
#include <QMessageBox>
#include <QVBoxLayout>

namespace {
/// Row height and thumbnail size. Smaller than the Layers panel's — a path row
/// carries only a name and a generic glyph, no per-shape preview (see
/// `PathIcons::PathThumbnail`'s own comment for why).
constexpr int kRowHeight = 32;
constexpr int kThumbSize = 22;
constexpr int kFooterGlyph = 18;

const QColor kGlyph(0xcf, 0xcf, 0xcf);
const QColor kGlyphOff(0x8a, 0x8a, 0x8a);
} // namespace

PathsPanel::PathsPanel(Engine *engine, QWidget *parent)
    : QWidget(parent)
    , m_engine(engine)
{
    buildUi();
    refresh();
}

void PathsPanel::buildUi()
{
    auto *root = new QVBoxLayout(this);
    root->setContentsMargins(0, 0, 0, 0);
    root->setSpacing(0);

    m_list = new QListWidget(this);
    m_list->setObjectName(QStringLiteral("pathList"));
    m_list->setIconSize(QSize(kThumbSize, kThumbSize));
    m_list->setSelectionMode(QAbstractItemView::SingleSelection);
    m_list->setContextMenuPolicy(Qt::CustomContextMenu);
    root->addWidget(m_list, 1);

    auto *footer = new QWidget(this);
    footer->setObjectName(QStringLiteral("panelFooter"));
    auto *footerLayout = new QHBoxLayout(footer);
    footerLayout->setContentsMargins(4, 2, 4, 2);
    footerLayout->setSpacing(2);

    auto makeButton = [&](PathIcons::Glyph glyph, const QString &tip, bool implemented = true) {
        auto *b = new QToolButton(footer);
        b->setIconSize(QSize(kFooterGlyph, kFooterGlyph));
        b->setIcon(PathIcons::icon(glyph, implemented ? kGlyph : kGlyphOff, kFooterGlyph));
        b->setToolTip(implemented ? tip : tr("Not implemented yet"));
        b->setEnabled(implemented);
        b->setAutoRaise(true);
        footerLayout->addWidget(b);
        return b;
    };

    // CS6's order, left to right.
    m_fillButton = makeButton(PathIcons::Glyph::Fill, tr("Fill path with foreground colour"));
    m_strokeButton = makeButton(PathIcons::Glyph::Stroke, tr("Stroke path with the current brush"));
    m_loadSelectionButton =
        makeButton(PathIcons::Glyph::LoadSelection, tr("Load path as a selection"));
    // Tracing a selection's contour into a path is a real chunk of work of its
    // own (marching squares, then simplifying the trace into anchors) that
    // this pass does not include — shown for the panel's shape, disabled, the
    // same convention every other unimplemented control in this app follows.
    makeButton(PathIcons::Glyph::MakeWorkPath, tr("Make work path from selection"), false);
    footerLayout->addStretch(1);
    // Duplicate Path has no dedicated footer glyph in CS6 either — it lives on
    // the row's right-click menu, same as here.
    m_addButton = makeButton(PathIcons::Glyph::NewPath, tr("Create a new path"));
    m_deleteButton = makeButton(PathIcons::Glyph::Delete, tr("Delete path"));

    root->addWidget(footer);

    connect(m_list, &QListWidget::itemSelectionChanged, this, &PathsPanel::onSelectionChanged);
    connect(m_list, &QListWidget::itemChanged, this, &PathsPanel::onItemChanged);
    connect(m_list, &QListWidget::customContextMenuRequested,
            this, &PathsPanel::onRowContextMenu);

    connect(m_fillButton, &QToolButton::clicked, this, &PathsPanel::fillPath);
    connect(m_strokeButton, &QToolButton::clicked, this, &PathsPanel::strokePath);
    connect(m_loadSelectionButton, &QToolButton::clicked, this, &PathsPanel::loadSelection);
    connect(m_addButton, &QToolButton::clicked, this, &PathsPanel::addPath);
    connect(m_deleteButton, &QToolButton::clicked, this, &PathsPanel::deletePath);
}

int PathsPanel::currentIndex() const
{
    return m_list->currentRow();
}

void PathsPanel::refresh()
{
    if (!m_engine) {
        return;
    }
    m_updating = true;

    const int count = m_engine->pathCount();
    const int active = m_engine->activePathIndex();

    m_list->clear();
    for (int i = 0; i < count; ++i) {
        auto *item = new QListWidgetItem(m_engine->pathName(i));
        item->setSizeHint(QSize(0, kRowHeight));
        item->setFlags(item->flags() | Qt::ItemIsEditable);
        item->setIcon(PathIcons::icon(PathIcons::Glyph::PathThumbnail, kGlyph, kThumbSize));
        m_list->addItem(item);
    }

    if (active >= 0 && active < count) {
        m_list->setCurrentRow(active);
    }

    const bool hasActive = active >= 0;
    m_fillButton->setEnabled(hasActive);
    m_strokeButton->setEnabled(hasActive);
    m_loadSelectionButton->setEnabled(hasActive);
    m_deleteButton->setEnabled(hasActive);

    m_updating = false;
}

void PathsPanel::onSelectionChanged()
{
    if (m_updating || !m_engine) {
        return;
    }
    const int index = currentIndex();
    if (index < 0) {
        return;
    }
    m_engine->setActivePathIndex(index);
    refresh();
    emit documentChanged();
}

void PathsPanel::onItemChanged(QListWidgetItem *item)
{
    if (m_updating || !m_engine || !item) {
        return;
    }
    const int index = m_list->row(item);
    const QString name = item->text();
    if (index < 0 || name.isEmpty()) {
        return;
    }
    if (name != m_engine->pathName(index)) {
        m_engine->renamePath(index, name);
    }
}

void PathsPanel::onRowContextMenu(const QPoint &pos)
{
    const QModelIndex index = m_list->indexAt(pos);
    if (!index.isValid() || !m_engine) {
        return;
    }
    if (index.row() != currentIndex()) {
        m_list->setCurrentRow(index.row());
    }

    QMenu menu(this);
    QAction *duplicate = menu.addAction(tr("Duplicate Path"));
    QAction *remove = menu.addAction(tr("Delete Path"));
    QAction *chosen = menu.exec(m_list->viewport()->mapToGlobal(pos));
    if (chosen == duplicate) {
        duplicatePath();
    } else if (chosen == remove) {
        deletePath();
    }
}

void PathsPanel::addPath()
{
    if (!m_engine) {
        return;
    }
    m_engine->addPath();
    refresh();
}

void PathsPanel::duplicatePath()
{
    if (!m_engine) {
        return;
    }
    const int index = currentIndex();
    if (index >= 0) {
        m_engine->duplicatePath(index);
        refresh();
    }
}

void PathsPanel::deletePath()
{
    if (!m_engine) {
        return;
    }
    const int index = currentIndex();
    if (index >= 0) {
        m_engine->deletePath(index);
        refresh();
        emit documentChanged();
    }
}

void PathsPanel::loadSelection()
{
    if (!m_engine || currentIndex() < 0) {
        return;
    }
    bool ok = false;
    const int feather = QInputDialog::getInt(this, tr("Make Selection"),
                                             tr("Feather Radius (pixels):"), 0, 0, 250, 1, &ok);
    if (!ok) {
        return;
    }
    // 0 = Replace, matching SelectionOp's discriminant order.
    if (!m_engine->pathMakeSelection(0, feather)) {
        QMessageBox::information(this, tr("PhotoRust"),
                                 tr("The active path has no closed area to select."));
        return;
    }
    emit documentChanged();
}

void PathsPanel::fillPath()
{
    if (!m_engine || currentIndex() < 0) {
        return;
    }
    if (!m_engine->pathFill()) {
        QMessageBox::information(this, tr("PhotoRust"),
                                 tr("Could not fill the path: it has no closed area, or the "
                                    "layer is locked."));
        return;
    }
    emit documentChanged();
}

void PathsPanel::strokePath()
{
    if (!m_engine || currentIndex() < 0) {
        return;
    }
    if (!m_engine->pathStroke()) {
        QMessageBox::information(this, tr("PhotoRust"),
                                 tr("Could not stroke the path: it is empty, or the layer is "
                                    "locked."));
        return;
    }
    emit documentChanged();
}

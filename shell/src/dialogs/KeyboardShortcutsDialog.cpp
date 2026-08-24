#include "KeyboardShortcutsDialog.h"
#include "shortcuts/CommandRegistry.h"

#include <QBoxLayout>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QGroupBox>
#include <QHeaderView>
#include <QKeyEvent>
#include <QLabel>
#include <QMessageBox>
#include <QPainter>
#include <QPushButton>

// ---------------------------------------------------------------------------
// ShortcutEditWidget — inline key-capture widget shown in the tree
// ---------------------------------------------------------------------------

ShortcutEditWidget::ShortcutEditWidget(QWidget *parent)
    : QWidget(parent)
{
    setFocusPolicy(Qt::StrongFocus);
    setMinimumHeight(22);
}

void ShortcutEditWidget::startCapture()
{
    m_capturing = true;
    setFocus();
    update();
}

void ShortcutEditWidget::setSequence(const QKeySequence &seq)
{
    m_sequence = seq;
    m_capturing = false;
    update();
}

void ShortcutEditWidget::keyPressEvent(QKeyEvent *event)
{
    if (!m_capturing) {
        QWidget::keyPressEvent(event);
        return;
    }

    const int key = event->key();
    if (key == Qt::Key_Escape) {
        m_capturing = false;
        update();
        return;
    }
    if (key == Qt::Key_unknown || key == Qt::Key_Control || key == Qt::Key_Shift ||
        key == Qt::Key_Alt || key == Qt::Key_Meta) {
        return;
    }

    int mods = event->modifiers() & (Qt::ControlModifier | Qt::ShiftModifier |
                                     Qt::AltModifier | Qt::MetaModifier);
    m_sequence = QKeySequence(key | mods);
    m_capturing = false;
    update();
    emit sequenceCaptured(m_sequence);
}

void ShortcutEditWidget::focusOutEvent(QFocusEvent *event)
{
    m_capturing = false;
    update();
    QWidget::focusOutEvent(event);
}

void ShortcutEditWidget::paintEvent(QPaintEvent *)
{
    QPainter p(this);
    p.fillRect(rect(), m_capturing ? QColor(0x40, 0x60, 0x90) : QColor(0x3c, 0x3c, 0x3c));
    p.setPen(QColor(0xdd, 0xdd, 0xdd));
    if (m_capturing) {
        p.drawText(rect().adjusted(4, 0, -4, 0), Qt::AlignVCenter,
                   tr("Type shortcut…"));
    } else if (!m_sequence.isEmpty()) {
        p.drawText(rect().adjusted(4, 0, -4, 0), Qt::AlignVCenter,
                   m_sequence.toString(QKeySequence::NativeText));
    }
    p.setPen(QColor(0x66, 0x66, 0x66));
    p.drawRect(rect().adjusted(0, 0, -1, -1));
}

// ---------------------------------------------------------------------------
// KeyboardShortcutsDialog
// ---------------------------------------------------------------------------

static const struct {
    const char *prefix;
    const char *label;
} kMenuGroups[] = {
    {"file.",    "File"},
    {"edit.",    "Edit"},
    {"image.",   "Image"},
    {"layer.",   "Layer"},
    {"select.",  "Select"},
    {"filter.",  "Filter"},
    {"view.",    "View"},
    {"window.",  "Window"},
    {"help.",    "Help"},
    {"tool.",    "Tools"},
};

KeyboardShortcutsDialog::KeyboardShortcutsDialog(CommandRegistry *registry,
                                                   QWidget *parent)
    : QDialog(parent)
    , m_registry(registry)
{
    setWindowTitle(tr("Keyboard Shortcuts and Menus"));
    resize(780, 520);

    auto *root = new QVBoxLayout(this);

    // -- top row: "Shortcuts For:" combo ------------------------------------
    auto *topRow = new QHBoxLayout;
    topRow->addWidget(new QLabel(tr("Shortcuts For:")));
    auto *scopeCombo = new QComboBox;
    scopeCombo->addItem(tr("Application Menus"));
    scopeCombo->setEnabled(false);
    topRow->addWidget(scopeCombo);
    topRow->addStretch();
    topRow->addWidget(new QLabel(tr("Set:")));
    auto *setCombo = new QComboBox;
    setCombo->addItem(tr("Photoshop Defaults"));
    setCombo->setEnabled(false);
    topRow->addWidget(setCombo);
    root->addLayout(topRow);

    // -- main area: tree + buttons -----------------------------------------
    auto *mainRow = new QHBoxLayout;

    // tree
    m_tree = new QTreeWidget;
    m_tree->setHeaderLabels({tr("Application Menu Command"), tr("Shortcut")});
    m_tree->header()->setStretchLastSection(false);
    m_tree->header()->setSectionResizeMode(0, QHeaderView::Stretch);
    m_tree->header()->setSectionResizeMode(1, QHeaderView::Fixed);
    m_tree->header()->resizeSection(1, 180);
    m_tree->setRootIsDecorated(true);
    m_tree->setSelectionMode(QAbstractItemView::SingleSelection);
    m_tree->setAlternatingRowColors(false);
    m_tree->setIndentation(20);
    buildTree();
    mainRow->addWidget(m_tree, 1);

    // right-side buttons
    auto *btnCol = new QVBoxLayout;
    btnCol->setSpacing(6);

    m_acceptBtn = new QPushButton(tr("Accept"));
    m_undoBtn = new QPushButton(tr("Undo"));
    m_defaultBtn = new QPushButton(tr("Use Default"));
    m_addBtn = new QPushButton(tr("Add Shortcut"));
    m_deleteBtn = new QPushButton(tr("Delete Shortcut"));

    m_acceptBtn->setEnabled(false);
    m_undoBtn->setEnabled(false);
    m_defaultBtn->setEnabled(false);
    m_addBtn->setEnabled(false);
    m_deleteBtn->setEnabled(false);

    btnCol->addWidget(m_acceptBtn);
    btnCol->addWidget(m_undoBtn);
    btnCol->addWidget(m_defaultBtn);
    btnCol->addSpacing(12);
    btnCol->addWidget(m_addBtn);
    btnCol->addWidget(m_deleteBtn);
    btnCol->addStretch();
    mainRow->addLayout(btnCol);

    root->addLayout(mainRow, 1);

    // -- editor widget (hidden until a command row is clicked) ---------------
    m_editor = new ShortcutEditWidget(this);
    m_editor->hide();

    // -- bottom OK / Cancel -------------------------------------------------
    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok |
                                         QDialogButtonBox::Cancel);
    root->addWidget(buttons);

    // -- wiring -------------------------------------------------------------
    connect(m_tree, &QTreeWidget::itemClicked, this,
            &KeyboardShortcutsDialog::onItemClicked);
    connect(m_acceptBtn, &QPushButton::clicked, this,
            &KeyboardShortcutsDialog::acceptShortcut);
    connect(m_undoBtn, &QPushButton::clicked, this,
            &KeyboardShortcutsDialog::undoShortcut);
    connect(m_defaultBtn, &QPushButton::clicked, this,
            &KeyboardShortcutsDialog::useDefault);
    connect(m_addBtn, &QPushButton::clicked, this,
            &KeyboardShortcutsDialog::addShortcut);
    connect(m_deleteBtn, &QPushButton::clicked, this,
            &KeyboardShortcutsDialog::deleteShortcut);
    connect(m_editor, &ShortcutEditWidget::sequenceCaptured, this, [this](const QKeySequence &seq) {
        if (!m_editingItem)
            return;
        m_editingItem->setText(1, seq.toString(QKeySequence::NativeText));
        const QString id = m_editingItem->data(0, Qt::UserRole).toString();
        m_pending[id] = seq;
        m_acceptBtn->setEnabled(true);
        m_undoBtn->setEnabled(true);

        const QString conflict = m_registry->commandForShortcut(seq);
        if (!conflict.isEmpty() && conflict != id) {
            QAction *conflictAction = m_registry->action(conflict);
            const QString name = conflictAction ? conflictAction->text() : conflict;
            m_editingItem->setToolTip(1,
                tr("%1 is already assigned to \"%2\". Accepting will reassign it.")
                    .arg(seq.toString(QKeySequence::NativeText), name));
        } else {
            m_editingItem->setToolTip(1, QString());
        }
    });

    connect(buttons, &QDialogButtonBox::accepted, this, [this] {
        for (auto it = m_pending.constBegin(); it != m_pending.constEnd(); ++it)
            m_registry->setShortcut(it.key(), it.value());
        m_registry->saveUserKeymap();
        accept();
    });
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
}

void KeyboardShortcutsDialog::buildTree()
{
    const QStringList ids = m_registry->commandIds();

    QHash<QString, QTreeWidgetItem *> groups;

    for (const auto &g : kMenuGroups) {
        auto *item = new QTreeWidgetItem(m_tree, {QString::fromLatin1(g.label)});
        QFont f = item->font(0);
        f.setBold(true);
        item->setFont(0, f);
        item->setFlags(Qt::ItemIsEnabled);
        groups.insert(QString::fromLatin1(g.prefix), item);
    }

    for (const QString &id : ids) {
        QString prefix;
        for (const auto &g : kMenuGroups) {
            if (id.startsWith(QLatin1String(g.prefix))) {
                prefix = QString::fromLatin1(g.prefix);
                break;
            }
        }
        if (prefix.isEmpty())
            continue;

        QTreeWidgetItem *parent = groups.value(prefix);
        if (!parent)
            continue;

        QAction *action = m_registry->action(id);
        const QString name = action ? action->text().remove(QLatin1Char('&')) : id;
        const QKeySequence seq = m_registry->shortcut(id);

        auto *item = new QTreeWidgetItem(parent, {
            QStringLiteral("    ") + name,
            seq.isEmpty() ? QString() : seq.toString(QKeySequence::NativeText)
        });
        item->setData(0, Qt::UserRole, id);
        item->setFlags(Qt::ItemIsEnabled | Qt::ItemIsSelectable);
    }

    m_tree->expandAll();
}

void KeyboardShortcutsDialog::onItemClicked(QTreeWidgetItem *item, int column)
{
    Q_UNUSED(column)
    const QString id = item->data(0, Qt::UserRole).toString();
    if (id.isEmpty())
        return;

    m_editingItem = item;
    m_originalShortcut = m_registry->shortcut(id);

    if (m_pending.contains(id))
        m_originalShortcut = m_pending.value(id);

    m_defaultBtn->setEnabled(true);
    m_deleteBtn->setEnabled(!m_originalShortcut.isEmpty());
    m_addBtn->setEnabled(m_originalShortcut.isEmpty());
    m_acceptBtn->setEnabled(false);
    m_undoBtn->setEnabled(false);

    QRect rect = m_tree->visualItemRect(item);
    int col1X = m_tree->header()->sectionPosition(1);
    int col1W = m_tree->header()->sectionSize(1);

    m_editor->setParent(m_tree->viewport());
    m_editor->setGeometry(col1X, rect.y(), col1W, rect.height());
    m_editor->setSequence(m_originalShortcut);
    m_editor->show();
    m_editor->startCapture();
}

void KeyboardShortcutsDialog::acceptShortcut()
{
    if (!m_editingItem)
        return;
    const QString id = m_editingItem->data(0, Qt::UserRole).toString();
    if (id.isEmpty())
        return;

    const QKeySequence seq = m_editor->sequence();
    m_pending[id] = seq;

    const QString conflict = m_registry->commandForShortcut(seq);
    if (!conflict.isEmpty() && conflict != id) {
        m_pending[conflict] = QKeySequence();
        auto items = m_tree->findItems(QString(), Qt::MatchContains | Qt::MatchRecursive, 0);
        for (auto *it : items) {
            if (it->data(0, Qt::UserRole).toString() == conflict) {
                it->setText(1, QString());
                break;
            }
        }
    }

    m_editingItem->setText(1, seq.toString(QKeySequence::NativeText));
    m_editingItem->setToolTip(1, QString());
    m_acceptBtn->setEnabled(false);
    m_undoBtn->setEnabled(false);
    m_editor->hide();
}

void KeyboardShortcutsDialog::undoShortcut()
{
    if (!m_editingItem)
        return;
    const QString id = m_editingItem->data(0, Qt::UserRole).toString();
    m_pending.remove(id);
    m_editingItem->setText(1, m_originalShortcut.isEmpty()
                                ? QString()
                                : m_originalShortcut.toString(QKeySequence::NativeText));
    m_editor->setSequence(m_originalShortcut);
    m_acceptBtn->setEnabled(false);
    m_undoBtn->setEnabled(false);
    m_editor->hide();
}

void KeyboardShortcutsDialog::useDefault()
{
    if (!m_editingItem)
        return;
    const QString id = m_editingItem->data(0, Qt::UserRole).toString();
    const QKeySequence def = m_registry->defaultShortcut(id);
    m_editor->setSequence(def);
    m_editingItem->setText(1, def.isEmpty()
                                ? QString()
                                : def.toString(QKeySequence::NativeText));
    m_pending[id] = def;
    m_acceptBtn->setEnabled(false);
    m_undoBtn->setEnabled(false);
    m_editor->hide();
}

void KeyboardShortcutsDialog::addShortcut()
{
    if (!m_editingItem)
        return;
    m_editor->setSequence(QKeySequence());
    m_editor->startCapture();
}

void KeyboardShortcutsDialog::deleteShortcut()
{
    if (!m_editingItem)
        return;
    const QString id = m_editingItem->data(0, Qt::UserRole).toString();
    m_pending[id] = QKeySequence();
    m_editingItem->setText(1, QString());
    m_editor->setSequence(QKeySequence());
    m_acceptBtn->setEnabled(false);
    m_undoBtn->setEnabled(false);
    m_deleteBtn->setEnabled(false);
    m_addBtn->setEnabled(true);
    m_editor->hide();
}

#pragma once

#include <QDialog>
#include <QHash>
#include <QKeySequence>
#include <QTreeWidget>

class CommandRegistry;
class QPushButton;

class ShortcutEditWidget : public QWidget
{
    Q_OBJECT
public:
    explicit ShortcutEditWidget(QWidget *parent = nullptr);
    void startCapture();
    QKeySequence sequence() const { return m_sequence; }
    void setSequence(const QKeySequence &seq);

signals:
    void sequenceCaptured(const QKeySequence &seq);

protected:
    void keyPressEvent(QKeyEvent *event) override;
    void focusOutEvent(QFocusEvent *event) override;
    void paintEvent(QPaintEvent *event) override;

private:
    QKeySequence m_sequence;
    bool m_capturing = false;
};

class KeyboardShortcutsDialog : public QDialog
{
    Q_OBJECT
public:
    explicit KeyboardShortcutsDialog(CommandRegistry *registry,
                                     QWidget *parent = nullptr);

private:
    void buildTree();
    void onItemClicked(QTreeWidgetItem *item, int column);
    void acceptShortcut();
    void undoShortcut();
    void useDefault();
    void addShortcut();
    void deleteShortcut();

    CommandRegistry *m_registry;
    QTreeWidget *m_tree = nullptr;
    ShortcutEditWidget *m_editor = nullptr;
    QPushButton *m_acceptBtn = nullptr;
    QPushButton *m_undoBtn = nullptr;
    QPushButton *m_defaultBtn = nullptr;
    QPushButton *m_addBtn = nullptr;
    QPushButton *m_deleteBtn = nullptr;

    QTreeWidgetItem *m_editingItem = nullptr;
    QKeySequence m_originalShortcut;
    QHash<QString, QKeySequence> m_pending;
};

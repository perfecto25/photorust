#pragma once

#include <QAction>
#include <QHash>
#include <QKeySequence>
#include <QObject>
#include <QString>
#include <QStringList>

/// Central registry mapping command ids to QActions and key bindings.
///
/// Per CLAUDE.md §9 shortcuts are data, not code. Widgets ask the registry for
/// an action by id and never construct a QKeySequence themselves; the binding
/// comes from `shortcuts.json` and may be overridden per user.
///
/// Typical use:
/// \code
///   auto *undo = registry.action("edit.undo");
///   connect(undo, &QAction::triggered, this, &MainWindow::undo);
///   menu->addAction(undo);
/// \endcode
class CommandRegistry : public QObject
{
    Q_OBJECT

public:
    explicit CommandRegistry(QObject *parent = nullptr);

    /// Load default bindings, then apply any user overrides on top.
    ///
    /// Returns false if the default keymap could not be read, in which case
    /// the registry still works but has no bindings.
    bool load(const QString &defaultsPath);

    /// Register a command. Safe to call for an id already present — the
    /// existing action is returned so callers cannot create duplicates.
    QAction *registerCommand(const QString &id,
                             const QString &text,
                             const QKeySequence &shortcut = {});

    /// The action for `id`, or nullptr if it was never registered.
    QAction *action(const QString &id) const;

    /// Whether a command exists.
    bool contains(const QString &id) const;

    /// All registered ids, sorted. Used by the shortcut editor.
    QStringList commandIds() const;

    /// Current binding for `id`, or an empty sequence.
    QKeySequence shortcut(const QString &id) const;

    /// Rebind a command at runtime, as Edit ▸ Keyboard Shortcuts does.
    ///
    /// Returns false if `id` is unknown. Rebinding to a sequence already in
    /// use clears it from the previous owner, matching Photoshop's behaviour
    /// of warning then reassigning.
    bool setShortcut(const QString &id, const QKeySequence &shortcut);

    /// The id currently bound to `shortcut`, or an empty string.
    QString commandForShortcut(const QKeySequence &shortcut) const;

    /// Persist the current bindings as user overrides.
    bool saveUserKeymap() const;

    /// Drop all user overrides and return to the shipped defaults.
    void resetToDefaults();

    /// Where per-user overrides live (`~/.config/PhotoRust/shortcuts.json` on
    /// Linux, `~/Library/Preferences/...` on macOS).
    static QString userKeymapPath();

signals:
    /// A binding changed, so menus should refresh their displayed shortcut.
    void shortcutChanged(const QString &id, const QKeySequence &shortcut);

private:
    /// Parse one keymap file into the registry.
    bool loadKeymapFile(const QString &path, bool isDefaults);

    QHash<QString, QAction *> m_actions;
    /// Defaults as shipped, so resetToDefaults() does not need the file again.
    QHash<QString, QKeySequence> m_defaults;
};

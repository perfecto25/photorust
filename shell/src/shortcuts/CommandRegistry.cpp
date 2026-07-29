#include "CommandRegistry.h"

#include <QDir>
#include <QFile>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLoggingCategory>
#include <QStandardPaths>

Q_LOGGING_CATEGORY(lcShortcuts, "photorust.shortcuts")

namespace {

/// The two arrays in shortcuts.json that hold bindings. Everything else in the
/// file (comments, version) is ignored.
const char *const kBindingArrays[] = {"tools", "commands"};

} // namespace

CommandRegistry::CommandRegistry(QObject *parent)
    : QObject(parent)
{
}

bool CommandRegistry::load(const QString &defaultsPath)
{
    if (!loadKeymapFile(defaultsPath, /*isDefaults=*/true)) {
        qCWarning(lcShortcuts) << "could not load default keymap from" << defaultsPath;
        return false;
    }

    // User overrides are optional; a missing file is the normal case.
    const QString userPath = userKeymapPath();
    if (QFile::exists(userPath)) {
        loadKeymapFile(userPath, /*isDefaults=*/false);
    }
    return true;
}

bool CommandRegistry::loadKeymapFile(const QString &path, bool isDefaults)
{
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly)) {
        return false;
    }

    QJsonParseError error{};
    const QJsonDocument doc = QJsonDocument::fromJson(file.readAll(), &error);
    if (error.error != QJsonParseError::NoError) {
        qCWarning(lcShortcuts) << path << "is not valid JSON:" << error.errorString();
        return false;
    }
    if (!doc.isObject()) {
        qCWarning(lcShortcuts) << path << "should contain a JSON object";
        return false;
    }

    const QJsonObject root = doc.object();
    for (const char *arrayName : kBindingArrays) {
        const QJsonArray entries = root.value(QLatin1String(arrayName)).toArray();
        for (const QJsonValue &value : entries) {
            const QJsonObject entry = value.toObject();
            const QString id = entry.value(QStringLiteral("id")).toString();
            if (id.isEmpty()) {
                continue;
            }

            const QString name = entry.value(QStringLiteral("name")).toString();
            // An empty "key" is legitimate: the command exists and appears in
            // menus, it simply has no default binding.
            const QString keyText = entry.value(QStringLiteral("key")).toString();
            const QKeySequence sequence =
                keyText.isEmpty() ? QKeySequence() : QKeySequence(keyText);

            if (!keyText.isEmpty() && sequence.isEmpty()) {
                qCWarning(lcShortcuts) << "unparseable key" << keyText << "for" << id;
            }

            if (isDefaults) {
                // Two commands sharing a key is worse than it looks: Qt calls
                // that an ambiguous shortcut and fires neither, so a clash
                // silently disables both. Refuse the later binding and say so.
                const QString clash = commandForShortcut(sequence);
                if (!clash.isEmpty() && clash != id) {
                    qCWarning(lcShortcuts)
                        << "ignoring" << keyText << "for" << id << "— already bound to" << clash;
                    registerCommand(id, name, QKeySequence());
                    m_defaults.insert(id, QKeySequence());
                    continue;
                }
                registerCommand(id, name, sequence);
                m_defaults.insert(id, sequence);
            } else if (QAction *existing = action(id)) {
                // A user override rebinds a command the defaults declared.
                existing->setShortcut(sequence);
                emit shortcutChanged(id, sequence);
            } else {
                qCWarning(lcShortcuts) << "user keymap binds unknown command" << id;
            }
        }
    }
    return true;
}

QAction *CommandRegistry::registerCommand(const QString &id,
                                          const QString &text,
                                          const QKeySequence &shortcut)
{
    if (QAction *existing = m_actions.value(id)) {
        // Re-registering is how widgets attach a nicer label to a command the
        // keymap declared first. Keep the action identity stable.
        if (!text.isEmpty()) {
            existing->setText(text);
        }
        return existing;
    }

    auto *act = new QAction(text.isEmpty() ? id : text, this);
    act->setObjectName(id);
    if (!shortcut.isEmpty()) {
        act->setShortcut(shortcut);
    }
    // Shortcuts must fire regardless of which dock panel holds focus.
    act->setShortcutContext(Qt::WindowShortcut);
    m_actions.insert(id, act);
    return act;
}

QAction *CommandRegistry::action(const QString &id) const
{
    return m_actions.value(id, nullptr);
}

bool CommandRegistry::contains(const QString &id) const
{
    return m_actions.contains(id);
}

QStringList CommandRegistry::commandIds() const
{
    QStringList ids = m_actions.keys();
    ids.sort();
    return ids;
}

QKeySequence CommandRegistry::shortcut(const QString &id) const
{
    if (QAction *act = action(id)) {
        return act->shortcut();
    }
    return {};
}

bool CommandRegistry::setShortcut(const QString &id, const QKeySequence &sequence)
{
    QAction *act = action(id);
    if (!act) {
        return false;
    }

    // Photoshop reassigns a conflicting shortcut rather than refusing, so
    // clear it from whoever held it first.
    if (!sequence.isEmpty()) {
        const QString previousOwner = commandForShortcut(sequence);
        if (!previousOwner.isEmpty() && previousOwner != id) {
            m_actions.value(previousOwner)->setShortcut(QKeySequence());
            emit shortcutChanged(previousOwner, QKeySequence());
        }
    }

    act->setShortcut(sequence);
    emit shortcutChanged(id, sequence);
    return true;
}

QString CommandRegistry::commandForShortcut(const QKeySequence &sequence) const
{
    if (sequence.isEmpty()) {
        return {};
    }
    for (auto it = m_actions.constBegin(); it != m_actions.constEnd(); ++it) {
        if (it.value()->shortcut() == sequence) {
            return it.key();
        }
    }
    return {};
}

bool CommandRegistry::saveUserKeymap() const
{
    const QString path = userKeymapPath();
    if (!QDir().mkpath(QFileInfo(path).absolutePath())) {
        qCWarning(lcShortcuts) << "could not create config directory for" << path;
        return false;
    }

    // Only write bindings that actually differ from the defaults, so the file
    // stays small and future default changes still reach the user.
    QJsonArray overrides;
    for (auto it = m_actions.constBegin(); it != m_actions.constEnd(); ++it) {
        const QKeySequence current = it.value()->shortcut();
        const QKeySequence original = m_defaults.value(it.key());
        if (current == original) {
            continue;
        }
        QJsonObject entry;
        entry.insert(QStringLiteral("id"), it.key());
        entry.insert(QStringLiteral("key"), current.toString(QKeySequence::PortableText));
        entry.insert(QStringLiteral("name"), it.value()->text());
        overrides.append(entry);
    }

    QJsonObject root;
    root.insert(QStringLiteral("version"), 1);
    root.insert(QStringLiteral("commands"), overrides);

    QFile file(path);
    if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
        qCWarning(lcShortcuts) << "could not write" << path;
        return false;
    }
    file.write(QJsonDocument(root).toJson(QJsonDocument::Indented));
    return true;
}

void CommandRegistry::resetToDefaults()
{
    for (auto it = m_defaults.constBegin(); it != m_defaults.constEnd(); ++it) {
        if (QAction *act = m_actions.value(it.key())) {
            if (act->shortcut() != it.value()) {
                act->setShortcut(it.value());
                emit shortcutChanged(it.key(), it.value());
            }
        }
    }
    QFile::remove(userKeymapPath());
}

QString CommandRegistry::userKeymapPath()
{
    const QString dir =
        QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation);
    return dir + QStringLiteral("/shortcuts.json");
}

// Invariants of the shipped keymap.
//
// No widgets, no engine: this is the keymap file read the way the application
// reads it, and asked the questions that have gone wrong before. Two commands
// sharing a key is the dangerous one — Qt calls that an ambiguous shortcut and
// fires *neither*, so a clash silently disables both rather than picking one.

#include "shortcuts/CommandRegistry.h"

#include <QAction>
#include <QDir>
#include <QFileInfo>
#include <QSet>
#include <QTest>

class TestKeymap : public QObject
{
    Q_OBJECT

private slots:
    void initTestCase();
    void everyBindingParses();
    void noTwoCommandsShareAShortcut();
    void theBindingsPeopleReachForAreThere();
    void aCommandKeepsItsNameWhenItsMenuLabelChanges();

private:
    QString m_keymap;
};

void TestKeymap::initTestCase()
{
    // The in-tree copy, which is what the build stages next to the binary.
    m_keymap = QStringLiteral(PHOTORUST_SOURCE_RESOURCES "/shortcuts.json");
    QVERIFY2(QFileInfo::exists(m_keymap), qPrintable(m_keymap));
}

void TestKeymap::everyBindingParses()
{
    CommandRegistry registry;
    QVERIFY(registry.load(m_keymap));
    QVERIFY(!registry.commandIds().isEmpty());

    // A key Qt cannot parse is dropped with a warning nobody reads, leaving a
    // menu entry with no shortcut and no explanation.
    for (const QString &id : registry.commandIds()) {
        QAction *action = registry.action(id);
        QVERIFY2(action, qPrintable(id));
        for (const QKeySequence &sequence : action->shortcuts()) {
            QVERIFY2(!sequence.isEmpty(), qPrintable(id));
        }
    }
}

void TestKeymap::noTwoCommandsShareAShortcut()
{
    CommandRegistry registry;
    QVERIFY(registry.load(m_keymap));

    QHash<QString, QString> owner;
    QStringList clashes;
    for (const QString &id : registry.commandIds()) {
        const QAction *action = registry.action(id);
        for (const QKeySequence &sequence : action->shortcuts()) {
            const QString key = sequence.toString();
            if (owner.contains(key)) {
                clashes << QStringLiteral("%1 and %2 both want %3")
                               .arg(owner.value(key), id, key);
            } else {
                owner.insert(key, id);
            }
        }
    }
    QVERIFY2(clashes.isEmpty(), qPrintable(clashes.join(QStringLiteral("; "))));
}

void TestKeymap::theBindingsPeopleReachForAreThere()
{
    CommandRegistry registry;
    QVERIFY(registry.load(m_keymap));

    // Not an exhaustive list — a sample of the ones whose absence would be
    // noticed immediately, including the two that have gone missing before.
    const QList<QPair<QString, QString>> expected = {
        {QStringLiteral("edit.cut"), QStringLiteral("Ctrl+X")},
        {QStringLiteral("edit.copy"), QStringLiteral("Ctrl+C")},
        {QStringLiteral("edit.paste"), QStringLiteral("Ctrl+V")},
        {QStringLiteral("edit.undo"), QStringLiteral("Ctrl+Z")},
        {QStringLiteral("file.close"), QStringLiteral("Ctrl+W")},
        {QStringLiteral("layer.group"), QStringLiteral("Ctrl+G")},
        {QStringLiteral("layer.bringForward"), QStringLiteral("Ctrl+]")},
        {QStringLiteral("layer.sendBackward"), QStringLiteral("Ctrl+[")},
    };
    for (const auto &[id, key] : expected) {
        QVERIFY2(registry.contains(id), qPrintable(id));
        QCOMPARE(registry.shortcut(id), QKeySequence(key));
    }
}

void TestKeymap::aCommandKeepsItsNameWhenItsMenuLabelChanges()
{
    // Filter ▸ Last Filter renames itself in the menu to whichever filter was
    // last run, which is what CS6 does. The shortcut editor lists commands,
    // not menu labels, so it must go on calling it Last Filter — otherwise
    // Ctrl+F turns up in that list as a second "Blur More" next to the real
    // one, and there is no way to tell which row rebinds what.
    CommandRegistry registry;
    QVERIFY(registry.load(m_keymap));

    QCOMPARE(registry.commandName(QStringLiteral("filter.last")),
             QStringLiteral("Last Filter"));

    registry.action(QStringLiteral("filter.last"))->setText(QStringLiteral("Blur More"));
    QCOMPARE(registry.commandName(QStringLiteral("filter.last")),
             QStringLiteral("Last Filter"));
}

QTEST_MAIN(TestKeymap)
#include "tst_keymap.moc"

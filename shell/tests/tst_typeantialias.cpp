// The anti-aliasing method, shared by Type ▸ Anti-Alias, the Type options bar
// and the Character panel.
//
// Six of the seven methods render identically, so picking the wrong one is
// invisible. What is not invisible, but is easy to reach by accident, is
// selecting the separator between them: that would set the method to an empty
// string, which ticks nothing in the menu and — since only "None" means off —
// would leave the text antialiased under a method that does not exist.

#include "panels/CharacterPanel.h"
#include "tools/ToolId.h"

#include <QComboBox>
#include <QSignalSpy>
#include <QTest>

class TestTypeAntialias : public QObject
{
    Q_OBJECT

private slots:
    void onlyNoneTurnsSmoothingOff();
    void theListCarriesOneSeparator();
    void thePanelOffersEveryMethod();
    void thePanelNeverReportsTheSeparator();

private:
    /// The Character panel's anti-alias combo, found by the entry every
    /// method list has.
    static QComboBox *antialiasCombo(const CharacterPanel &panel);
};

QComboBox *TestTypeAntialias::antialiasCombo(const CharacterPanel &panel)
{
    for (QComboBox *combo : panel.findChildren<QComboBox *>()) {
        if (combo->findText(QStringLiteral("Crisp")) >= 0) {
            return combo;
        }
    }
    return nullptr;
}

void TestTypeAntialias::onlyNoneTurnsSmoothingOff()
{
    // This is the one thing the renderer reads, so it is the one thing that
    // has to stay true as methods are added or renamed.
    QVERIFY(!TypeDefaults::antialiasOn(QStringLiteral("None")));
    for (const QString &method : TypeDefaults::antialiasMethods()) {
        if (method.isEmpty() || method == QLatin1String("None")) {
            continue;
        }
        QVERIFY2(TypeDefaults::antialiasOn(method), qPrintable(method));
    }
    QVERIFY(TypeDefaults::antialiasOn(TypeDefaults::defaultAntialiasMethod()));
}

void TestTypeAntialias::theListCarriesOneSeparator()
{
    const QStringList methods = TypeDefaults::antialiasMethods();
    // The empty entry is the menu's separator; the combo boxes turn it into
    // one too, so exactly one has to be there and it must not be an end.
    QCOMPARE(methods.count(QString()), 1);
    QVERIFY(!methods.first().isEmpty());
    QVERIFY(!methods.last().isEmpty());
}

void TestTypeAntialias::thePanelOffersEveryMethod()
{
    CharacterPanel panel;
    QComboBox *combo = antialiasCombo(panel);
    QVERIFY(combo);
    for (const QString &method : TypeDefaults::antialiasMethods()) {
        if (!method.isEmpty()) {
            QVERIFY2(combo->findText(method) >= 0, qPrintable(method));
        }
    }
    QCOMPARE(combo->currentText(), TypeDefaults::defaultAntialiasMethod());
}

void TestTypeAntialias::thePanelNeverReportsTheSeparator()
{
    CharacterPanel panel;
    QComboBox *combo = antialiasCombo(panel);
    QVERIFY(combo);

    const int separator = combo->findText(QString());
    QVERIFY2(separator >= 0, "the combo should carry the list's separator");

    QSignalSpy spy(&panel, &CharacterPanel::antialiasChanged);
    combo->setCurrentIndex(separator);
    // Landing on it must not be reported as a method: an empty one would tick
    // nothing in the menu and still count as smoothing.
    for (const QList<QVariant> &reported : spy) {
        QVERIFY(!reported.at(0).toString().isEmpty());
    }
}

QTEST_MAIN(TestTypeAntialias)
#include "tst_typeantialias.moc"

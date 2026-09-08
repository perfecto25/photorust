// Paragraph styles: the dialog's read-back, and the panel's list.
//
// A style is only ever seen through what applying it does to some text, so a
// field that does not survive the dialog — a size that failed to parse, an
// alignment whose combo data was mapped wrong, a name that came back empty —
// shows up as text being restyled not quite right, a long way from the cause.

#include "dialogs/ParagraphStyleDialog.h"
#include "panels/ParagraphStylesPanel.h"

#include <QFontDatabase>
#include <QListWidget>
#include <QSignalSpy>
#include <QToolButton>
#include <QTest>

class TestParagraphStyles : public QObject
{
    Q_OBJECT

private slots:
    void theDialogGivesBackWhatItWasGiven();
    void thePanelStartsWithTheBasicStyle();
    void theBasicStyleCannotBeDeleted();
    void clickingAStyleAppliesIt();
};

void TestParagraphStyles::theDialogGivesBackWhatItWasGiven()
{
    ParagraphStyle before;
    before.name = QStringLiteral("Heading");
    // A real family, or the combo cannot select it and the round trip would
    // fail for a reason that has nothing to do with the dialog.
    before.family = QFontDatabase::families().value(0);
    before.style = QFontDatabase::styles(before.family).value(0);
    before.size = 48;
    before.color = QColor(0x20, 0x80, 0xc0);
    // Deliberately different from each other, and neither of them 100%.
    before.hScale = 1.5;
    before.vScale = 0.75;
    before.alignment = Qt::AlignRight;

    ParagraphStyleDialog dialog(before);
    const ParagraphStyle after = dialog.style();

    QCOMPARE(after.name, before.name);
    QCOMPARE(after.family, before.family);
    QCOMPARE(after.style, before.style);
    QCOMPARE(after.size, before.size);
    QCOMPARE(after.color, before.color);
    QCOMPARE(after.hScale, before.hScale);
    QCOMPARE(after.vScale, before.vScale);
    QCOMPARE(after.alignment, before.alignment);
}

void TestParagraphStyles::thePanelStartsWithTheBasicStyle()
{
    ParagraphStylesPanel panel;
    auto *list = panel.findChild<QListWidget *>();
    QVERIFY(list);
    QCOMPARE(list->count(), 1);
    QCOMPARE(list->item(0)->text(), QStringLiteral("Basic Paragraph"));
}

void TestParagraphStyles::theBasicStyleCannotBeDeleted()
{
    ParagraphStylesPanel panel;
    auto *list = panel.findChild<QListWidget *>();
    QVERIFY(list);
    list->setCurrentRow(0);

    // The delete button is the second in the footer; invoking it rather than
    // the private slot keeps this to what a user can actually reach.
    const QList<QToolButton *> buttons = panel.findChildren<QToolButton *>();
    QCOMPARE(buttons.size(), 2);
    buttons.at(1)->click();

    QCOMPARE(list->count(), 1);
}

void TestParagraphStyles::clickingAStyleAppliesIt()
{
    ParagraphStylesPanel panel;
    auto *list = panel.findChild<QListWidget *>();
    QVERIFY(list);

    QSignalSpy spy(&panel, &ParagraphStylesPanel::styleApplied);
    emit list->itemClicked(list->item(0));
    QCOMPARE(spy.count(), 1);
}

QTEST_MAIN(TestParagraphStyles)
#include "tst_paragraphstyles.moc"

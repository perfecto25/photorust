// The Glyphs panel's grid.
//
// Qt exposes no way to read a font's character map, so the grid is built by
// offering the font one code point at a time and keeping what it answers to.
// Both halves of that fail quietly: a scan that matches nothing looks like a
// sparse font, and a subset filter off by one block looks like the font
// simply having those glyphs. Neither would draw attention on its own.

#include "panels/GlyphsPanel.h"

#include <QComboBox>
#include <QListWidget>
#include <QApplication>
#include <QSignalSpy>
#include <QTest>

class TestGlyphsPanel : public QObject
{
    Q_OBJECT

private slots:
    void theGridFillsFromTheFont();
    void aSubsetShowsOnlyItsOwnBlock();
    void doubleClickingAGlyphReportsItWithItsFont();

private:
    /// The subset combo, found by the one entry it is guaranteed to have,
    /// rather than by its position among the panel's three combo boxes.
    static QComboBox *subsetCombo(const GlyphsPanel &panel);
};

QComboBox *TestGlyphsPanel::subsetCombo(const GlyphsPanel &panel)
{
    for (QComboBox *combo : panel.findChildren<QComboBox *>()) {
        if (combo->count() > 0 && combo->itemText(0) == QLatin1String("Entire Font")) {
            return combo;
        }
    }
    return nullptr;
}

void TestGlyphsPanel::theGridFillsFromTheFont()
{
    GlyphsPanel panel;
    auto *grid = panel.findChild<QListWidget *>();
    QVERIFY(grid);

    // Any font this test could run against has printable ASCII, so an empty
    // grid means the scan itself found nothing.
    QVERIFY2(grid->count() > 0, "the whole-font scan produced no glyphs at all");
}

void TestGlyphsPanel::aSubsetShowsOnlyItsOwnBlock()
{
    GlyphsPanel panel;
    auto *grid = panel.findChild<QListWidget *>();
    QVERIFY(grid);
    QComboBox *subset = subsetCombo(panel);
    QVERIFY(subset);

    const int basicLatin = subset->findText(QStringLiteral("Basic Latin"));
    QVERIFY(basicLatin > 0);
    subset->setCurrentIndex(basicLatin);

    QVERIFY(grid->count() > 0);
    // The block's real bounds, spelled out here so an off-by-one in the
    // panel's own indexing cannot agree with itself.
    for (int i = 0; i < grid->count(); ++i) {
        const QList<uint> code = grid->item(i)->text().toUcs4();
        QCOMPARE(code.size(), 1);
        QVERIFY2(code.first() >= 0x20 && code.first() <= 0x7E,
                 qPrintable(QStringLiteral("U+%1 is outside Basic Latin")
                                .arg(code.first(), 4, 16, QLatin1Char('0'))));
    }

    // And it is a narrowing: the whole font has more than one block's worth.
    const int narrowed = grid->count();
    subset->setCurrentIndex(0);
    QVERIFY(grid->count() > narrowed);
}

void TestGlyphsPanel::doubleClickingAGlyphReportsItWithItsFont()
{
    GlyphsPanel panel;
    panel.resize(400, 400);
    panel.show();
    // The grid lays its icons out on the resize that `show` posts, so the
    // click has nowhere to land until that has been delivered.
    qApp->processEvents();

    auto *grid = panel.findChild<QListWidget *>();
    QVERIFY(grid);
    QVERIFY(grid->count() > 0);

    QSignalSpy spy(&panel, &GlyphsPanel::glyphChosen);
    QListWidgetItem *item = grid->item(0);
    const QPoint centre = grid->visualItemRect(item).center();
    QVERIFY2(grid->itemAt(centre) == item, "the grid has not laid its glyphs out yet");
    // The click before the double-click is not padding: the view only reports
    // a double-click on an index it has already seen pressed, and
    // `mouseDClick` alone sends just the tail of the gesture a user makes.
    QTest::mouseClick(grid->viewport(), Qt::LeftButton, {}, centre);
    QTest::mouseDClick(grid->viewport(), Qt::LeftButton, {}, centre);

    QCOMPARE(spy.count(), 1);
    const QList<QVariant> chosen = spy.first();
    QCOMPARE(chosen.at(0).toString(), item->text());
    // The family and style ride along so the insert can set the text to a
    // font that actually has the glyph — see the handler in MainWindow.
    QVERIFY(!chosen.at(1).toString().isEmpty());
    QVERIFY(!chosen.at(2).toString().isEmpty());
}

QTEST_MAIN(TestGlyphsPanel)
#include "tst_glyphspanel.moc"

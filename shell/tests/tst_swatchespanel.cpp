// The Swatches panel.
//
// The grid is placed by arithmetic rather than by a layout — it reflows to
// whatever width the panel is given — so the hit testing and the drawing have
// to agree about where a swatch is. When they do not, clicking a colour picks
// up its neighbour, which is the kind of fault that looks like a misclick
// rather than like a bug.
//
// The modifiers are the other half: CS6 puts three different actions on one
// mouse button, and getting them the wrong way round means alt-clicking a
// swatch to delete it silently paints with it instead.

#include "panels/SwatchesPanel.h"

#include <QApplication>
#include <QMouseEvent>
#include <QSignalSpy>
#include <QTest>

#include <memory>

class TestSwatchesPanel : public QObject
{
    Q_OBJECT

private slots:
    void clickingASwatchPicksItUpForTheForeground();
    void ctrlClickTakesTheBackgroundHalfOfThePair();
    void altClickTakesASwatchOutOfTheSet();
    void theGridReflowsToTheWidthItIsGiven();
    void clickingPastTheLastSwatchAsksForANewOne();
    void deletingLeavesTheMarkOnSomethingThatIsStillThere();

private:
    /// Click at a point in the grid, with modifiers.
    static void click(SwatchGrid *grid, const QPoint &at,
                      Qt::KeyboardModifiers modifiers = Qt::NoModifier);
    /// The middle of swatch `index`, found the same way the grid finds it —
    /// by asking, rather than by repeating its arithmetic here.
    static QPoint centreOf(SwatchGrid *grid, int index);
};

void TestSwatchesPanel::click(SwatchGrid *grid, const QPoint &at,
                              Qt::KeyboardModifiers modifiers)
{
    QMouseEvent press(QEvent::MouseButtonPress, QPointF(at), grid->mapToGlobal(at),
                      Qt::LeftButton, Qt::LeftButton, modifiers);
    QApplication::sendEvent(grid, &press);
}

QPoint TestSwatchesPanel::centreOf(SwatchGrid *grid, int index)
{
    // Walk the grid until the point that reports this index is found: the
    // cell rectangles are private, and a test that recomputed them would
    // agree with a broken grid just as happily as with a working one. The
    // search runs over the height the grid *needs*, which is not the height
    // it has been given — inside the panel a scroll area sorts that out.
    for (int y = 0; y < grid->heightFor(grid->width()); ++y) {
        for (int x = 0; x < grid->width(); ++x) {
            if (grid->indexAt(QPoint(x, y)) == index) {
                return QPoint(x + 2, y + 2);
            }
        }
    }
    return QPoint(-1, -1);
}

void TestSwatchesPanel::clickingASwatchPicksItUpForTheForeground()
{
    auto grid = std::make_unique<SwatchGrid>();
    grid->resize(200, 400);
    QSignalSpy foreground(grid.get(), &SwatchGrid::foregroundPicked);
    QSignalSpy background(grid.get(), &SwatchGrid::backgroundPicked);

    const int index = 3;
    click(grid.get(), centreOf(grid.get(), index));

    QCOMPARE(foreground.count(), 1);
    QCOMPARE(background.count(), 0);
    QCOMPARE(foreground.first().first().value<QColor>(), grid->swatches().at(index).color);
    QCOMPARE(grid->current(), index);
}

void TestSwatchesPanel::ctrlClickTakesTheBackgroundHalfOfThePair()
{
    auto grid = std::make_unique<SwatchGrid>();
    grid->resize(200, 400);
    QSignalSpy foreground(grid.get(), &SwatchGrid::foregroundPicked);
    QSignalSpy background(grid.get(), &SwatchGrid::backgroundPicked);

    const int index = 5;
    click(grid.get(), centreOf(grid.get(), index), Qt::ControlModifier);

    QCOMPARE(background.count(), 1);
    QCOMPARE(foreground.count(), 0);
    QCOMPARE(background.first().first().value<QColor>(), grid->swatches().at(index).color);
}

void TestSwatchesPanel::altClickTakesASwatchOutOfTheSet()
{
    auto grid = std::make_unique<SwatchGrid>();
    grid->resize(200, 400);
    const int before = int(grid->swatches().size());
    const QColor doomed = grid->swatches().at(2).color;
    QSignalSpy foreground(grid.get(), &SwatchGrid::foregroundPicked);

    click(grid.get(), centreOf(grid.get(), 2), Qt::AltModifier);

    QCOMPARE(int(grid->swatches().size()), before - 1);
    QVERIFY2(grid->swatches().at(2).color != doomed, "the wrong swatch went");
    QCOMPARE(foreground.count(), 0);
}

void TestSwatchesPanel::theGridReflowsToTheWidthItIsGiven()
{
    auto grid = std::make_unique<SwatchGrid>();
    grid->resize(120, 400);
    const int narrow = grid->heightFor(120);

    grid->resize(400, 400);
    const int wide = grid->heightFor(400);

    QVERIFY2(wide < narrow, "a wider panel did not fit more swatches on a row");
    // Whichever way round, the grid asks for exactly the height its rows
    // need, and every swatch has a place to be clicked.
    QCOMPARE(grid->minimumSizeHint().height(), wide);
    QVERIFY(centreOf(grid.get(), int(grid->swatches().size()) - 1) != QPoint(-1, -1));
    grid->resize(120, 400);
    QCOMPARE(grid->minimumSizeHint().height(), narrow);
    QVERIFY(centreOf(grid.get(), int(grid->swatches().size()) - 1) != QPoint(-1, -1));
}

void TestSwatchesPanel::clickingPastTheLastSwatchAsksForANewOne()
{
    auto grid = std::make_unique<SwatchGrid>();
    grid->resize(200, 600);
    QSignalSpy asked(grid.get(), &SwatchGrid::newSwatchRequested);
    QSignalSpy foreground(grid.get(), &SwatchGrid::foregroundPicked);

    // Below every row there is: CS6 fills that space with a paint bucket.
    click(grid.get(), QPoint(grid->width() / 2, grid->heightFor(grid->width()) + 4));

    QCOMPARE(asked.count(), 1);
    QCOMPARE(foreground.count(), 0);
}

void TestSwatchesPanel::deletingLeavesTheMarkOnSomethingThatIsStillThere()
{
    auto grid = std::make_unique<SwatchGrid>();
    grid->resize(200, 400);

    // Delete the last swatch through the mark, the way the footer's bin does.
    click(grid.get(), centreOf(grid.get(), int(grid->swatches().size()) - 1));
    QVERIFY(grid->removeSwatch(grid->current()));
    QVERIFY2(grid->current() < int(grid->swatches().size()),
             "the mark is left pointing past the end of the set");
    QVERIFY(grid->current() >= 0);

    // And deleting the rest never leaves it pointing at nothing that exists.
    while (!grid->swatches().isEmpty()) {
        QVERIFY(grid->removeSwatch(grid->current()));
        QVERIFY(grid->current() < int(grid->swatches().size()));
    }
    QCOMPARE(grid->current(), -1);
}

QTEST_MAIN(TestSwatchesPanel)
#include "tst_swatchespanel.moc"

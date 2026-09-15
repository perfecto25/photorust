// The resize grip in the corner of a floating panel.
//
// Three things here are invisible when they break. The grip is placed by hand
// rather than by a layout, so a panel that is resized leaves it stranded
// wherever it was. It is shown only while the panel floats, because a docked
// panel is sized by its splitter and a grip there would try to resize the main
// window instead. And the drag is measured from where it started rather than
// from the last move, which is the difference between a panel that follows the
// pointer and one that slowly drifts away from it.

#include "panels/PanelResizeGrip.h"

#include <QApplication>
#include <QDockWidget>
#include <QLabel>
#include <QMainWindow>
#include <QMouseEvent>
#include <QTest>

#include <memory>

class TestPanelGrip : public QObject
{
    Q_OBJECT

private slots:
    void theGripOnlyShowsWhileThePanelFloats();
    void theGripStaysInTheCornerAsThePanelResizes();
    void draggingTheGripResizesThePanel();
    void thePanelWillNotBeDraggedSmallerThanItsContents();

private:
    /// A main window with one dock in it, and the grip attached the way
    /// `MainWindow::createDocks` attaches it.
    struct Panel {
        std::unique_ptr<QMainWindow> window;
        QDockWidget *dock = nullptr;
        PanelResizeGrip *grip = nullptr;
    };
    static Panel panel();
    /// Drive the grip the way a pointer would.
    static void drag(PanelResizeGrip *grip, const QPoint &by);
};

TestPanelGrip::Panel TestPanelGrip::panel()
{
    Panel out;
    out.window = std::make_unique<QMainWindow>();
    out.window->resize(800, 600);
    out.dock = new QDockWidget(QStringLiteral("History"), out.window.get());
    auto *content = new QLabel(QStringLiteral("content"), out.dock);
    content->setMinimumSize(120, 80);
    out.dock->setWidget(content);
    out.window->addDockWidget(Qt::RightDockWidgetArea, out.dock);
    out.grip = new PanelResizeGrip(out.dock);
    out.window->show();
    return out;
}

void TestPanelGrip::drag(PanelResizeGrip *grip, const QPoint &by)
{
    const QPoint from = grip->mapToGlobal(QPoint(grip->width() / 2, grip->height() / 2));
    QMouseEvent press(QEvent::MouseButtonPress, QPointF(grip->rect().center()), QPointF(from),
                      QPointF(from), Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
    QApplication::sendEvent(grip, &press);

    const QPointF to = QPointF(from + by);
    QMouseEvent move(QEvent::MouseMove, QPointF(grip->rect().center()), to, to, Qt::NoButton,
                     Qt::LeftButton, Qt::NoModifier);
    QApplication::sendEvent(grip, &move);

    QMouseEvent release(QEvent::MouseButtonRelease, QPointF(grip->rect().center()), to, to,
                        Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
    QApplication::sendEvent(grip, &release);
}

void TestPanelGrip::theGripOnlyShowsWhileThePanelFloats()
{
    Panel p = panel();
    QVERIFY2(!p.grip->isVisible(), "a docked panel is offering a resize grip");

    p.dock->setFloating(true);
    QVERIFY2(p.grip->isVisible(), "a floating panel has no resize grip");

    p.dock->setFloating(false);
    QVERIFY2(!p.grip->isVisible(), "the grip stayed behind when the panel docked again");
}

void TestPanelGrip::theGripStaysInTheCornerAsThePanelResizes()
{
    Panel p = panel();
    p.dock->setFloating(true);
    p.dock->resize(260, 340);
    QCoreApplication::processEvents();

    // Inside the panel, hard against the bottom-right.
    const QRect where = p.grip->geometry();
    QVERIFY2(p.dock->rect().contains(where), "the grip is hanging outside the panel");
    QVERIFY2(p.dock->width() - where.right() <= 3, "the grip is not against the right edge");
    QVERIFY2(p.dock->height() - where.bottom() <= 3, "the grip is not against the bottom edge");

    // ...and it follows, rather than staying where it was first put.
    p.dock->resize(420, 500);
    QCoreApplication::processEvents();
    QVERIFY(p.grip->geometry() != where);
    QVERIFY(p.dock->width() - p.grip->geometry().right() <= 3);
}

void TestPanelGrip::draggingTheGripResizesThePanel()
{
    Panel p = panel();
    p.dock->setFloating(true);
    p.dock->resize(300, 300);
    QCoreApplication::processEvents();
    const QSize before = p.dock->size();

    drag(p.grip, QPoint(60, 40));
    QCOMPARE(p.dock->size(), QSize(before.width() + 60, before.height() + 40));

    // And back the other way: the grip shrinks as well as grows.
    drag(p.grip, QPoint(-40, -30));
    QCOMPARE(p.dock->size(),
             QSize(before.width() + 60 - 40, before.height() + 40 - 30));
}

void TestPanelGrip::thePanelWillNotBeDraggedSmallerThanItsContents()
{
    Panel p = panel();
    p.dock->setFloating(true);
    p.dock->resize(300, 300);
    QCoreApplication::processEvents();

    // Dragged far past nothing. Without the floor the panel would end up
    // clipped with no grip left to drag it back out with.
    drag(p.grip, QPoint(-2000, -2000));
    const QSize least = p.dock->minimumSizeHint().expandedTo(p.dock->minimumSize());
    QVERIFY2(p.dock->width() >= least.width() && p.dock->height() >= least.height(),
             "the panel was dragged smaller than it can be drawn");
}

QTEST_MAIN(TestPanelGrip)
#include "tst_panelgrip.moc"

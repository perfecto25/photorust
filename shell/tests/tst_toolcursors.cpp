// Which tools get the brush-size circle for a cursor, and what the Pen shows
// instead.
//
// The cursor is chosen by a switch with a `default:` that builds a circle the
// width of the brush. Anything not named there silently inherits it, which is
// how the Pen tool ended up dragging a paintbrush-sized circle across a path
// it was meant to be editing point by point. A tool added later would inherit
// it the same way, and nothing about the code would look wrong.

#include "canvas/CanvasView.h"
#include "tools/ToolIcons.h"
#include "tools/ToolId.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QTest>

class TestToolCursors : public QObject
{
    Q_OBJECT

private slots:
    void geometryToolsDoNotGetTheBrushCircle();
    void thePenCursorDoesNotGrowWithTheBrush();
    void thePenCursorPointsAtWhatItWillClick();
    void paintingToolsStillGetTheCircle();

private:
    /// The cursor a tool shows, asked afresh so the size the brush was set to
    /// has been taken into account.
    static QCursor cursorFor(CanvasView &canvas, ToolId tool, double brush)
    {
        canvas.setBrushSize(brush);
        canvas.setActiveTool(tool);
        return canvas.cursor();
    }
};

void TestToolCursors::geometryToolsDoNotGetTheBrushCircle()
{
    Engine engine;
    CanvasView canvas(&engine);

    struct Expected {
        ToolId tool;
        Qt::CursorShape shape;
    };
    const Expected cases[] = {
        {ToolId::Shape, Qt::CrossCursor},
        {ToolId::Gradient, Qt::CrossCursor},
        {ToolId::PathSelect, Qt::ArrowCursor},
    };
    for (const Expected &c : cases) {
        // A brush wide enough that the circle would certainly have been built
        // had the tool fallen through to it.
        QCOMPARE(cursorFor(canvas, c.tool, 60).shape(), c.shape);
    }
}

void TestToolCursors::thePenCursorDoesNotGrowWithTheBrush()
{
    Engine engine;
    CanvasView canvas(&engine);

    // The Pen draws a nib rather than one of Qt's stock shapes, so "is it a
    // bitmap" says nothing. What separates the nib from the brush circle is
    // that the circle is *sized by the brush* and the nib is not.
    const QSize narrow = cursorFor(canvas, ToolId::Pen, 20).pixmap().size();
    const QSize wide = cursorFor(canvas, ToolId::Pen, 160).pixmap().size();
    QVERIFY(!narrow.isEmpty());
    QCOMPARE(narrow, wide);

    // And the brush, for contrast, does follow it.
    QVERIFY(cursorFor(canvas, ToolId::Brush, 20).pixmap().size()
            != cursorFor(canvas, ToolId::Brush, 160).pixmap().size());
}

void TestToolCursors::thePenCursorPointsAtWhatItWillClick()
{
    Engine engine;
    CanvasView canvas(&engine);
    const QCursor pen = cursorFor(canvas, ToolId::Pen, 40);

    // The hotspot has to land on the nib's drawn tip, or every click goes
    // somewhere other than where the cursor is pointing. Both sides derive
    // from the one constant, so this checks they were derived the same way.
    const QSize size = pen.pixmap().size();
    QVERIFY(!size.isEmpty());
    const int expected = int(std::lround(ToolIcons::kPenCursorTip / 20.0 * size.width()));
    QCOMPARE(pen.hotSpot(), QPoint(expected, expected));
    // Near the top-left corner, which is where a pen points.
    QVERIFY(pen.hotSpot().x() < size.width() / 3);
}

void TestToolCursors::paintingToolsStillGetTheCircle()
{
    Engine engine;
    CanvasView canvas(&engine);

    // The other half of the fix: naming those tools must not have taken the
    // circle away from the ones whose whole point is painting a brush width.
    for (ToolId tool : {ToolId::Brush, ToolId::Eraser, ToolId::CloneStamp, ToolId::Dodge}) {
        QCOMPARE(cursorFor(canvas, tool, 60).shape(), Qt::BitmapCursor);
    }
}

QTEST_MAIN(TestToolCursors)
#include "tst_toolcursors.moc"

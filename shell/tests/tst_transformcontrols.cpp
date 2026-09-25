// The Move tool's Show Transform Controls.
//
// With the box on, a press on one of its handles has to become a Free
// Transform *and* keep going as the drag that scales it — CS6 does not make
// you press twice. A press anywhere else is still the Move tool's, and with
// the box off none of the handles exist at all. These are all things that
// look right in a screenshot and are only wrong under the mouse.

#include "canvas/CanvasView.h"
#include "tools/ToolId.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QImage>
#include <QTest>

class TestTransformControls : public QObject
{
    Q_OBJECT

private slots:
    void aHandlePressStartsFreeTransform();
    void theDragThatFollowsScales();
    void aPressInsideTheBoxStillMoves();
    void withTheControlsOffThereAreNoHandles();
    void otherToolsHaveNoHandles();

private:
    /// A 40×40 black square on its own layer at (50, 50), with the Move tool
    /// up on a canvas at 100%.
    static constexpr int kLeft = 50;
    static constexpr int kSize = 40;
    static void setUp(Engine &engine, CanvasView &canvas)
    {
        QImage square(kSize, kSize, QImage::Format_ARGB32_Premultiplied);
        square.fill(Qt::black);
        QVERIFY(engine.addImageLayer(square, kLeft, kLeft, QStringLiteral("Square")));
        canvas.resize(800, 600);
        canvas.setZoom(1.0);
        canvas.setActiveTool(ToolId::Move);
        canvas.refresh();
    }
    static QPoint at(const CanvasView &canvas, double x, double y)
    {
        return canvas.documentToWidget(QPointF(x, y)).toPoint();
    }
};

void TestTransformControls::aHandlePressStartsFreeTransform()
{
    Engine engine;
    CanvasView canvas(&engine);
    setUp(engine, canvas);
    canvas.setShowTransformControls(true);

    QTest::mousePress(&canvas, Qt::LeftButton, {}, at(canvas, kLeft, kLeft));
    QVERIFY(canvas.isFreeTransforming());
    QCOMPARE(canvas.transformOrigBounds(), QRectF(kLeft, kLeft, kSize, kSize));
    canvas.cancelFreeTransform();
}

void TestTransformControls::theDragThatFollowsScales()
{
    // The press is handed on to Free Transform, so it has already caught the
    // bottom-right handle by the time the pointer moves: one drag, not two.
    Engine engine;
    CanvasView canvas(&engine);
    setUp(engine, canvas);
    canvas.setShowTransformControls(true);

    const double corner = kLeft + kSize;
    QTest::mousePress(&canvas, Qt::LeftButton, {}, at(canvas, corner, corner));
    QTest::mouseMove(&canvas, at(canvas, corner + 20, corner + 20));
    QTest::mouseRelease(&canvas, Qt::LeftButton, {}, at(canvas, corner + 20, corner + 20));

    QVERIFY(canvas.isFreeTransforming());
    QVERIFY2(canvas.transformBounds().width() > kSize + 10,
             "dragging the handle did not scale the box");
    canvas.cancelFreeTransform();
}

void TestTransformControls::aPressInsideTheBoxStillMoves()
{
    Engine engine;
    CanvasView canvas(&engine);
    setUp(engine, canvas);
    canvas.setShowTransformControls(true);

    const double middle = kLeft + kSize / 2.0;
    QTest::mousePress(&canvas, Qt::LeftButton, {}, at(canvas, middle, middle));
    QTest::mouseMove(&canvas, at(canvas, middle + 10, middle));
    QTest::mouseRelease(&canvas, Qt::LeftButton, {}, at(canvas, middle + 10, middle));

    QVERIFY(!canvas.isFreeTransforming());
    QCOMPARE(engine.layerContentBounds(engine.getActiveLayerIndex()).x(), kLeft + 10);
}

void TestTransformControls::withTheControlsOffThereAreNoHandles()
{
    Engine engine;
    CanvasView canvas(&engine);
    setUp(engine, canvas);
    canvas.setShowTransformControls(false);

    QTest::mousePress(&canvas, Qt::LeftButton, {}, at(canvas, kLeft, kLeft));
    QVERIFY(!canvas.isFreeTransforming());
    QTest::mouseRelease(&canvas, Qt::LeftButton, {}, at(canvas, kLeft, kLeft));
}

void TestTransformControls::otherToolsHaveNoHandles()
{
    // The setting is the Move tool's. It is remembered across a switch, but
    // the box is not a thing another tool can grab.
    Engine engine;
    CanvasView canvas(&engine);
    setUp(engine, canvas);
    canvas.setShowTransformControls(true);
    canvas.setActiveTool(ToolId::Marquee);

    QTest::mousePress(&canvas, Qt::LeftButton, {}, at(canvas, kLeft, kLeft));
    QVERIFY(!canvas.isFreeTransforming());
    QTest::mouseRelease(&canvas, Qt::LeftButton, {}, at(canvas, kLeft, kLeft));
}

QTEST_MAIN(TestTransformControls)
#include "tst_transformcontrols.moc"

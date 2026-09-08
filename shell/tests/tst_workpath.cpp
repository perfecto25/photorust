// Type ▸ Create Work Path: glyph outlines becoming an editable path.
//
// Qt states a cubic as "curve to c1, c2, p3" from wherever the pen is, while
// the engine stores an out-handle on the point being left and an in-handle on
// the one being reached. Getting that translation slightly wrong still
// produces a path — one whose curves bulge the wrong way, or that has a
// stray zero-length segment where the contour closed. None of that is
// obvious from looking at a panel that says "Work Path".

#include "MainWindow.h"
#include "canvas/CanvasView.h"
#include "canvas/TypeWarp.h"
#include "shortcuts/CommandRegistry.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QFontDatabase>
#include <QImage>
#include <QPainterPath>
#include <QTest>

class TestWorkPath : public QObject
{
    Q_OBJECT

private slots:
    void aTypeLayerGivesGlyphOutlines();
    void theOutlineFollowsTheLayersStretch();
    void aNonTypeLayerGivesNothing();
    void convertToShapeReplacesTheTypeLayer();
    void convertToShapeLeavesTheCountersHollow();
    void rasterizingKeepsThePixelsAndDropsTheText();
    void warpingKeepsTheLettersOnScreen();
    void indentsAndSpacingMoveTheText();
    void aWarpBendsTheInsideOfALetterNotJustItsCorners();
    void colorRangeSelectsOnlyTheMatchingColour();

private:
    /// A type layer reading "OO" — two letters, each an outer and an inner
    /// contour, so the outline has holes to get wrong.
    static int addTypeLayer(Engine &engine, float size, float hScale);
    /// Send a layer's glyph outlines over as the active path, the way the
    /// menu entry does.
    static int buildOutlinePath(Engine &engine, CanvasView &canvas, int layer);
};

int TestWorkPath::addTypeLayer(Engine &engine, float size, float hScale)
{
    engine.beginTextRuns();
    engine.addTextRun(QStringLiteral("OO"), QFontDatabase::families().value(0),
                      QStringLiteral("Regular"), size, QColor(Qt::black), hScale, 1.0f);

    QImage pixels(200, 100, QImage::Format_ARGB32_Premultiplied);
    pixels.fill(Qt::transparent);
    if (!engine.addTextLayer(pixels, 10, 10, QStringLiteral("OO"), 0, true, false, 10.0f,
                             80.0f)) {
        return -1;
    }
    const int index = engine.getActiveLayerIndex();
    return engine.layerTextRunCount(index) > 0 ? index : -1;
}

int TestWorkPath::buildOutlinePath(Engine &engine, CanvasView &canvas, int layer)
{
    const QPainterPath outline = canvas.typeLayerOutline(layer);
    if (outline.isEmpty()) {
        return -1;
    }
    engine.beginPathBuild();
    QList<QPolygonF> contours = outline.toSubpathPolygons();
    for (const QPolygonF &contour : std::as_const(contours)) {
        if (contour.size() < 3) {
            continue;
        }
        engine.pathBuildSubpath(true);
        for (const QPointF &p : contour) {
            engine.pathBuildPoint(float(p.x()), float(p.y()), false, 0, 0, false, 0, 0);
        }
    }
    return engine.commitBuiltPath(QStringLiteral("Shape Path"));
}

void TestWorkPath::aTypeLayerGivesGlyphOutlines()
{
    Engine engine;
    CommandRegistry registry;
    QVERIFY(registry.load(QStringLiteral(PHOTORUST_SOURCE_RESOURCES "/shortcuts.json")));
    MainWindow window(&engine, &registry);

    const int layer = addTypeLayer(engine, 72.0f, 1.0f);
    QVERIFY2(layer >= 0, "could not add a type layer to test with");

    auto *canvas = window.findChild<CanvasView *>();
    QVERIFY(canvas);
    const QPainterPath outline = canvas->typeLayerOutline(layer);
    QVERIFY2(!outline.isEmpty(), "the glyphs produced no outline at all");

    // It has to sit on the text, not near it. The origin is the top-left the
    // text is laid out from — the first baseline falls an ascent below it —
    // so the letters occupy the band starting there and running down by
    // roughly one line.
    const QRectF bounds = outline.boundingRect();
    QVERIFY(bounds.width() > 0 && bounds.height() > 0);
    QVERIFY2(bounds.left() >= 10.0 && bounds.top() >= 80.0 && bounds.bottom() < 80.0 + 3 * 72.0,
             qPrintable(QStringLiteral("outline sits at x %1, y %2..%3, away from the text")
                            .arg(bounds.left())
                            .arg(bounds.top())
                            .arg(bounds.bottom())));
}

void TestWorkPath::theOutlineFollowsTheLayersStretch()
{
    Engine engine;
    CommandRegistry registry;
    QVERIFY(registry.load(QStringLiteral(PHOTORUST_SOURCE_RESOURCES "/shortcuts.json")));
    MainWindow window(&engine, &registry);
    auto *canvas = window.findChild<CanvasView *>();
    QVERIFY(canvas);

    const int plain = addTypeLayer(engine, 72.0f, 1.0f);
    QVERIFY(plain >= 0);
    const qreal plainWidth = canvas->typeLayerOutline(plain).boundingRect().width();
    QVERIFY(plainWidth > 0);

    // The same text at 200% horizontal scale traces twice as wide, and no
    // taller — tracing the unstretched glyphs would put the path beside the
    // letters it is supposed to be on.
    const int stretched = addTypeLayer(engine, 72.0f, 2.0f);
    QVERIFY(stretched >= 0);
    const QRectF stretchedBounds = canvas->typeLayerOutline(stretched).boundingRect();
    QVERIFY2(stretchedBounds.width() > plainWidth * 1.8,
             qPrintable(QStringLiteral("stretched outline is %1 wide against %2 unstretched")
                            .arg(stretchedBounds.width())
                            .arg(plainWidth)));
}

void TestWorkPath::aNonTypeLayerGivesNothing()
{
    Engine engine;
    CommandRegistry registry;
    QVERIFY(registry.load(QStringLiteral(PHOTORUST_SOURCE_RESOURCES "/shortcuts.json")));
    MainWindow window(&engine, &registry);
    auto *canvas = window.findChild<CanvasView *>();
    QVERIFY(canvas);

    engine.addLayer();
    QVERIFY(canvas->typeLayerOutline(engine.getActiveLayerIndex()).isEmpty());
}

void TestWorkPath::convertToShapeReplacesTheTypeLayer()
{
    Engine engine;
    CommandRegistry registry;
    QVERIFY(registry.load(QStringLiteral(PHOTORUST_SOURCE_RESOURCES "/shortcuts.json")));
    MainWindow window(&engine, &registry);
    auto *canvas = window.findChild<CanvasView *>();
    QVERIFY(canvas);

    const int layer = addTypeLayer(engine, 72.0f, 1.0f);
    QVERIFY(layer >= 0);
    const int before = engine.getLayerCount();
    const QString name = engine.layerName(layer);

    QVERIFY(buildOutlinePath(engine, *canvas, layer) >= 0);
    QVERIFY(engine.convertTypeLayerToShape());

    // Replaced, not added alongside: the letters stop being text.
    QCOMPARE(engine.getLayerCount(), before);
    const int now = engine.getActiveLayerIndex();
    QCOMPARE(engine.layerName(now), name);
    QCOMPARE(engine.layerTextRunCount(now), 0);
}

void TestWorkPath::convertToShapeLeavesTheCountersHollow()
{
    Engine engine;
    CommandRegistry registry;
    QVERIFY(registry.load(QStringLiteral(PHOTORUST_SOURCE_RESOURCES "/shortcuts.json")));
    MainWindow window(&engine, &registry);
    auto *canvas = window.findChild<CanvasView *>();
    QVERIFY(canvas);

    const int layer = addTypeLayer(engine, 72.0f, 1.0f);
    QVERIFY(layer >= 0);

    // The first contour is the outer ring of the first "O", so its middle is
    // the counter and a point just inside its left edge is on the stroke.
    // Located from the outline itself rather than guessed at as a fraction of
    // the text, which would drift with whatever font the machine has.
    const QPainterPath outline = canvas->typeLayerOutline(layer);
    const QList<QPolygonF> contours = outline.toSubpathPolygons();
    QVERIFY(contours.size() >= 2);
    const QRectF ring = contours.first().boundingRect();

    QVERIFY(buildOutlinePath(engine, *canvas, layer) >= 0);
    QVERIFY(engine.convertTypeLayerToShape());

    // The document has an opaque white Background under the shape, so the
    // question is the colour that shows through, not the alpha: black where
    // the letter is drawn, white where its counter was cut out. Getting the
    // fill rule wrong fills the whole letter, and nothing about the layer
    // looks wrong until you see it.
    const QImage composite = engine.compositeImage();
    QVERIFY(!composite.isNull());

    const QPoint counter(int(ring.center().x()), int(ring.center().y()));
    const QPoint stroke(int(ring.left() + 2), int(ring.center().y()));
    QVERIFY(composite.rect().contains(counter));
    QVERIFY(composite.rect().contains(stroke));
    QVERIFY2(qGray(composite.pixel(counter)) > 200,
             "the middle of the O is filled — the counter was not cut out");
    QVERIFY2(qGray(composite.pixel(stroke)) < 100,
             "the stroke of the O is not filled — the shape is missing");
}

void TestWorkPath::rasterizingKeepsThePixelsAndDropsTheText()
{
    Engine engine;
    CommandRegistry registry;
    QVERIFY(registry.load(QStringLiteral(PHOTORUST_SOURCE_RESOURCES "/shortcuts.json")));
    MainWindow window(&engine, &registry);

    const int layer = addTypeLayer(engine, 72.0f, 1.0f);
    QVERIFY(layer >= 0);
    const int before = engine.getLayerCount();
    const QString name = engine.layerName(layer);
    const QImage pixelsBefore = engine.layerImage(layer);
    QVERIFY(!pixelsBefore.isNull());

    QVERIFY(engine.rasterizeTypeLayer());

    // The whole point is that nothing is redrawn: the same layer, the same
    // name, the same pixels — only the record saying what they were set in
    // has gone, so the letters can no longer be retyped.
    QCOMPARE(engine.getLayerCount(), before);
    const int now = engine.getActiveLayerIndex();
    QCOMPARE(engine.layerName(now), name);
    QCOMPARE(engine.layerTextRunCount(now), 0);
    QCOMPARE(engine.layerImage(now), pixelsBefore);

    // And pressing it again is a refusal rather than a second history step.
    QVERIFY(!engine.rasterizeTypeLayer());
}

void TestWorkPath::warpingKeepsTheLettersOnScreen()
{
    Engine engine;
    CommandRegistry registry;
    QVERIFY(registry.load(QStringLiteral(PHOTORUST_SOURCE_RESOURCES "/shortcuts.json")));
    MainWindow window(&engine, &registry);
    auto *canvas = window.findChild<CanvasView *>();
    QVERIFY(canvas);

    const int layer = addTypeLayer(engine, 72.0f, 1.0f);
    QVERIFY(layer >= 0);
    QVERIFY(canvas->reflowTypeLayer(layer));
    const QSize unbent = engine.layerImage(layer).size();
    QVERIFY(unbent.width() > 20 && unbent.height() > 20);

    // Bending pushes letters outside the box they were laid out in, and that
    // box is what the committed image is sized from. Get the measurement
    // wrong and the image collapses to a few pixels — which reads as the text
    // vanishing off the canvas entirely, not as a bounds bug.
    for (int style : {int(TypeWarp::Arc), int(TypeWarp::Wave), int(TypeWarp::Twist)}) {
        QVERIFY(engine.setLayerTextWarp(layer, style, true, 0.5f, 0.0f, 0.0f));
        QVERIFY(canvas->reflowTypeLayer(layer));

        // Not "no smaller than unwarped": a warped layer is sized to the
        // letters themselves, which is legitimately tighter than the line box
        // an unwarped one is sized to — that box carries the font's ascender
        // space and descent. What must not happen is the collapse this test
        // was written for, where the layer came back a few pixels across.
        const QSize bent = engine.layerImage(layer).size();
        QVERIFY2(bent.width() > unbent.width() / 2 && bent.height() > unbent.height() / 2,
                 qPrintable(QStringLiteral("style %1 collapsed the layer to %2x%3 from %4x%5")
                                .arg(style)
                                .arg(bent.width())
                                .arg(bent.height())
                                .arg(unbent.width())
                                .arg(unbent.height())));

        // And something was actually drawn into it.
        const QImage image = engine.layerImage(layer);
        bool anyInk = false;
        for (int y = 0; y < image.height() && !anyInk; ++y) {
            for (int x = 0; x < image.width(); ++x) {
                if (qAlpha(image.pixel(x, y)) > 0) {
                    anyInk = true;
                    break;
                }
            }
        }
        QVERIFY2(anyInk, qPrintable(QStringLiteral("style %1 drew nothing at all").arg(style)));
    }
}

void TestWorkPath::indentsAndSpacingMoveTheText()
{
    Engine engine;
    CommandRegistry registry;
    QVERIFY(registry.load(QStringLiteral(PHOTORUST_SOURCE_RESOURCES "/shortcuts.json")));
    MainWindow window(&engine, &registry);
    auto *canvas = window.findChild<CanvasView *>();
    QVERIFY(canvas);

    const int layer = addTypeLayer(engine, 48.0f, 1.0f);
    QVERIFY(layer >= 0);
    QVERIFY(canvas->reflowTypeLayer(layer));
    const QPainterPath plain = canvas->typeLayerOutline(layer);
    QVERIFY(!plain.isEmpty());
    const QRectF before = plain.boundingRect();

    // A left indent moves the letters right by exactly that much: the text
    // is left-aligned, so the indent is measured from the edge it sits on.
    QVERIFY(engine.setLayerTextParagraph(layer, 30.0f, 0.0f, 0.0f, 0.0f, 0.0f));
    QVERIFY(canvas->reflowTypeLayer(layer));
    const QRectF indented = canvas->typeLayerOutline(layer).boundingRect();
    QVERIFY2(std::abs((indented.left() - before.left()) - 30.0) < 1.0,
             qPrintable(QStringLiteral("indent moved the text by %1, not 30")
                            .arg(indented.left() - before.left())));
    QVERIFY(std::abs(indented.top() - before.top()) < 1.0);

    // Space before pushes it down instead, leaving the left edge alone.
    QVERIFY(engine.setLayerTextParagraph(layer, 0.0f, 0.0f, 0.0f, 25.0f, 0.0f));
    QVERIFY(canvas->reflowTypeLayer(layer));
    const QRectF spaced = canvas->typeLayerOutline(layer).boundingRect();
    QVERIFY2(std::abs((spaced.top() - before.top()) - 25.0) < 1.0,
             qPrintable(QStringLiteral("spacing moved the text by %1, not 25")
                            .arg(spaced.top() - before.top())));
    QVERIFY(std::abs(spaced.left() - before.left()) < 1.0);

    // And they survive the record being rewritten by a restyle, which is
    // where settings that live outside the runs tend to get lost.
    const rust::Vec<float> kept = engine.layerTextParagraph(layer);
    QCOMPARE(kept.size(), size_t(5));
    QCOMPARE(kept[3], 25.0f);
}

void TestWorkPath::aWarpBendsTheInsideOfALetterNotJustItsCorners()
{
    Engine engine;
    CommandRegistry registry;
    QVERIFY(registry.load(QStringLiteral(PHOTORUST_SOURCE_RESOURCES "/shortcuts.json")));
    MainWindow window(&engine, &registry);
    auto *canvas = window.findChild<CanvasView *>();
    QVERIFY(canvas);

    // Capitals with long straight stems, which is where this goes wrong: an
    // outline carries points only where it changes direction, so a stem is
    // two points tens of pixels apart. Warping only those leaves the line
    // between them straight, and a warp that varies along the stem does
    // nothing at all — the letter comes out displaced rather than bent.
    engine.beginTextRuns();
    engine.addTextRun(QStringLiteral("HHHHHHHHHH"), QFontDatabase::families().value(0),
                      QStringLiteral("Regular"), 48.0f, QColor(Qt::black), 1.0f, 1.0f);
    QImage pixels(400, 200, QImage::Format_ARGB32_Premultiplied);
    pixels.fill(Qt::transparent);
    QVERIFY(engine.addTextLayer(pixels, 20, 20, QStringLiteral("H"), 0, true, false, 60.0f,
                                120.0f));
    const int layer = engine.getActiveLayerIndex();

    // A horizontal squeeze presses the ends inward at mid-height and lets the
    // top and bottom spread, so the ink is much narrower halfway down than it
    // is at the top.
    QVERIFY(engine.setLayerTextWarp(layer, int(TypeWarp::Squeeze), true, 1.0f, 0.0f, 0.0f));
    QVERIFY(canvas->reflowTypeLayer(layer));

    const QImage image = engine.layerImage(layer);
    QVERIFY(!image.isNull());
    int top = -1;
    int bottom = -1;
    for (int y = 0; y < image.height(); ++y) {
        for (int x = 0; x < image.width(); ++x) {
            if (qAlpha(image.pixel(x, y)) > 8) {
                if (top < 0) {
                    top = y;
                }
                bottom = y;
                break;
            }
        }
    }
    QVERIFY(top >= 0 && bottom > top);

    auto widthAtRow = [&](int y) {
        int left = -1;
        int right = -1;
        for (int x = 0; x < image.width(); ++x) {
            if (qAlpha(image.pixel(x, y)) > 8) {
                if (left < 0) {
                    left = x;
                }
                right = x;
            }
        }
        return right < 0 ? 0 : right - left + 1;
    };

    const int atTop = widthAtRow(top + 1);
    const int atMiddle = widthAtRow((top + bottom) / 2);
    QVERIFY2(atMiddle * 3 < atTop,
             qPrintable(QStringLiteral("the letters are %1 wide at the middle against %2 at "
                                       "the top — the warp is only moving their corners")
                            .arg(atMiddle)
                            .arg(atTop)));
}

void TestWorkPath::colorRangeSelectsOnlyTheMatchingColour()
{
    Engine engine;

    // Half the canvas red, half blue, so there is a right answer to check
    // against rather than "something got selected".
    const int width = engine.getCanvasWidth();
    const int height = engine.getCanvasHeight();
    QVERIFY(width > 8 && height > 8);
    QImage art(width, height, QImage::Format_ARGB32_Premultiplied);
    art.fill(QColor(255, 0, 0));
    for (int y = 0; y < height; ++y) {
        for (int x = width / 2; x < width; ++x) {
            art.setPixelColor(x, y, QColor(0, 0, 255));
        }
    }
    QVERIFY(engine.addImageLayer(art, 0, 0, QStringLiteral("art")));

    // `[x, y, width, height]`, all zeroes when nothing is selected.
    auto selectionBox = [&engine] {
        const rust::Vec<int32_t> b = engine.selectionBounds();
        return b.size() == 4 ? QRect(b[0], b[1], b[2], b[3]) : QRect();
    };

    // Sampled Colors on the red half.
    engine.selectColorRange(0, QColor(255, 0, 0), 40, false, 0);
    const QRect onRed = selectionBox();
    QVERIFY2(!onRed.isEmpty(), "matching red selected nothing at all");
    QVERIFY2(onRed.right() < width * 3 / 4,
             qPrintable(QStringLiteral("the selection reached x=%1, into the blue half")
                            .arg(onRed.right())));

    // The blue half is the other answer, not the same one.
    engine.selectColorRange(0, QColor(0, 0, 255), 40, false, 0);
    const QRect onBlue = selectionBox();
    QVERIFY(!onBlue.isEmpty());
    QVERIFY2(onBlue.left() > width / 4,
             qPrintable(QStringLiteral("the selection started at x=%1, in the red half")
                            .arg(onBlue.left())));

    // Inverting swaps which half is taken, rather than selecting nothing.
    engine.selectColorRange(0, QColor(0, 0, 255), 40, true, 0);
    const QRect inverted = selectionBox();
    QVERIFY(!inverted.isEmpty());
    QVERIFY(inverted.left() < onBlue.left());
}

QTEST_MAIN(TestWorkPath)
#include "tst_workpath.moc"

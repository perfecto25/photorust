// The canvas redraws only what changed.
//
// It keeps the composite it last drew and asks the engine what has changed
// since. The answer has one hard requirement — it must never be smaller than
// what really changed, or the canvas shows a stale picture — and one soft
// one, that it be small when it can be, which is the whole point. These pin
// down both, through the same calls the canvas makes.

#include "MainWindow.h"
#include "canvas/CanvasView.h"
#include "shortcuts/CommandRegistry.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QColor>
#include <QImage>
#include <QTest>

class TestCanvasDamage : public QObject
{
    Q_OBJECT

private slots:
    void aStrokeRedrawsOnlyWhereItWent();
    void theRedrawnPatchIsTheSameAsTheWholePicture();
    void aChangeNobodyDescribedRedrawsEverything();
    void aSmallChangeCannotHideAnEarlierUndescribedOne();
    void nothingChangedMeansNothingToRedraw();
    void theDisplayIsRebuiltOnceAndThenUpdatedByTheStroke();
    void aStrokeReachesTheSmallerLevels();
    void aCropOffTheEdgeOfALevelIsStillItsFullSize();
    void theCanvasDrawsThePictureWhereItBelongsAtEveryZoom();
    void undoingAStrokeRedrawsOnlyItsTiles();
    void everydayUseLeavesTheCanvasNothingToRedraw();
    void typeLayerEditsRedrawOnlyTheText();
    void typingLeavesTheCanvasNothingToRedraw();
    void movingALayerRedrawsOnlyWhereItWasAndIs();
    void draggingALayerLeavesTheCanvasNothingToRedraw();

private:
    static constexpr int kWidth = 400;
    static constexpr int kHeight = 300;

    /// A fresh document, with the damage from making it already taken.
    static void setUp(Engine &engine)
    {
        engine.newDocument(kWidth, kHeight, 0);
        engine.setBrush(10.0f, 100, 100, 100, 25);
        engine.setForegroundColor(QColor(200, 30, 30));
        engine.takeCanvasDamage();
    }

    static void stroke(Engine &engine)
    {
        QVERIFY(engine.beginStroke(100.0f, 100.0f, 1.0f));
        engine.extendStroke(150.0f, 120.0f, 1.0f);
        engine.endStroke();
    }

    static QRect canvas() { return QRect(0, 0, kWidth, kHeight); }
};

void TestCanvasDamage::aStrokeRedrawsOnlyWhereItWent()
{
    Engine engine;
    setUp(engine);

    QVERIFY(engine.beginStroke(100.0f, 100.0f, 1.0f));
    engine.extendStroke(150.0f, 120.0f, 1.0f);
    // Mid-stroke the layer is untouched — the canvas shows the dabs by patch
    // — so there is nothing for it to redraw from the document yet.
    QVERIFY(engine.takeCanvasDamage().isEmpty());

    engine.endStroke();
    const QRect damage = engine.takeCanvasDamage();
    QVERIFY(damage.contains(QPoint(100, 100)));
    QVERIFY(damage.contains(QPoint(150, 120)));
    QVERIFY2(damage.width() < kWidth / 2 && damage.height() < kHeight / 2,
             qPrintable(QStringLiteral("a short stroke redraws %1x%2 of a %3x%4 canvas")
                            .arg(damage.width())
                            .arg(damage.height())
                            .arg(kWidth)
                            .arg(kHeight)));
}

void TestCanvasDamage::theRedrawnPatchIsTheSameAsTheWholePicture()
{
    // A patch that differs from the full composite by even a level would
    // leave a visible rectangle behind every stroke.
    Engine engine;
    setUp(engine);
    stroke(engine);
    const QRect damage = engine.takeCanvasDamage();
    QVERIFY(!damage.isEmpty());

    const QImage whole = engine.compositeImage();
    const QImage patch = engine.compositeRegionImage(damage);
    QCOMPARE(patch.size(), damage.size());
    QCOMPARE(patch.convertToFormat(QImage::Format_RGBA8888_Premultiplied),
             whole.copy(damage).convertToFormat(QImage::Format_RGBA8888_Premultiplied));
}

void TestCanvasDamage::aChangeNobodyDescribedRedrawsEverything()
{
    // A filter does not say where it changed the picture, so the canvas must
    // assume everywhere — slower, never stale.
    Engine engine;
    setUp(engine);
    const float radius = 2.0f;
    engine.applyFilter(QStringLiteral("Gaussian Blur"), rust::Slice<const float>(&radius, 1));
    QCOMPARE(engine.takeCanvasDamage(), canvas());
}

void TestCanvasDamage::aSmallChangeCannotHideAnEarlierUndescribedOne()
{
    // The trap: an undescribed change the canvas has not caught up with,
    // then a stroke that describes itself precisely. Redrawing only the
    // stroke would leave the first change off the screen.
    Engine engine;
    setUp(engine);
    const float radius = 2.0f;
    engine.applyFilter(QStringLiteral("Gaussian Blur"), rust::Slice<const float>(&radius, 1));
    stroke(engine);
    QCOMPARE(engine.takeCanvasDamage(), canvas());
}

void TestCanvasDamage::nothingChangedMeansNothingToRedraw()
{
    // What makes the shell's habit of refreshing after the engine already
    // has cost nothing, where it used to cost a second full composite.
    Engine engine;
    setUp(engine);
    stroke(engine);
    engine.takeCanvasDamage();
    engine.setForegroundColor(QColor(0, 0, 255));
    QVERIFY(engine.takeCanvasDamage().isEmpty());
}

void TestCanvasDamage::theDisplayIsRebuiltOnceAndThenUpdatedByTheStroke()
{
    // The canvas's picture now lives in the engine as a pyramid; after a
    // stroke only the stroke's rectangle of it is redone, and what the canvas
    // is handed there is exactly the composite.
    Engine engine;
    engine.newDocument(2000, 1500, 0);
    engine.setBrush(24.0f, 100, 100, 100, 25);
    engine.setForegroundColor(QColor(200, 30, 30));

    QCOMPARE(engine.updateDisplay(), QRect(0, 0, 2000, 1500));
    // 2000 → 1000 → 500; one more would be under 256 on its long side.
    QCOMPARE(engine.displayLevelCount(), 3);
    QCOMPARE(engine.displayLevelSize(1), QSize(1000, 750));
    QVERIFY(engine.updateDisplay().isEmpty());

    QVERIFY(engine.beginStroke(900.0f, 700.0f, 1.0f));
    engine.extendStroke(1000.0f, 760.0f, 1.0f);
    engine.endStroke();
    const QRect damage = engine.updateDisplay();
    QVERIFY(damage.contains(QPoint(900, 700)));
    QVERIFY(damage.width() < 400);

    const QImage shown = engine.displayImage(0, damage);
    QCOMPARE(shown.convertToFormat(QImage::Format_RGBA8888_Premultiplied),
             engine.compositeImage().copy(damage).convertToFormat(
                 QImage::Format_RGBA8888_Premultiplied));
}

void TestCanvasDamage::aStrokeReachesTheSmallerLevels()
{
    // Zoomed out, the canvas draws from a smaller level; a stroke that only
    // reached level 0 would not show until the next full rebuild.
    Engine engine;
    engine.newDocument(2000, 1500, 0);
    engine.setBrush(40.0f, 100, 100, 100, 25);
    engine.setForegroundColor(QColor(0, 0, 0));
    engine.updateDisplay();

    const QRect around(420, 320, 60, 60); // level 2 pixels over the stroke
    const QImage before = engine.displayImage(2, around);
    QVERIFY(engine.beginStroke(1800.0f, 1300.0f, 1.0f));
    engine.extendStroke(1900.0f, 1400.0f, 1.0f);
    engine.endStroke();
    engine.updateDisplay();
    QVERIFY(engine.displayImage(2, around) != before);
}

void TestCanvasDamage::aCropOffTheEdgeOfALevelIsStillItsFullSize()
{
    // The canvas asks for what is on screen plus a margin, clipped; but a
    // request hanging off the edge must still come back the size asked for,
    // or it would be drawn stretched.
    Engine engine;
    engine.newDocument(2000, 1500, 0);
    engine.updateDisplay();
    QCOMPARE(engine.displayImage(1, QRect(900, 700, 200, 100)).size(), QSize(200, 100));
}

void TestCanvasDamage::theCanvasDrawsThePictureWhereItBelongsAtEveryZoom()
{
    // The canvas draws only the part on screen, from whichever level the
    // zoom calls for. An off-by-one in the level arithmetic shows as the
    // picture drawn a few pixels out of place, or at the wrong scale — so a
    // crisp black square is looked for exactly where the document says it is,
    // with white beside it, zoomed out (a smaller level) and at 100%.
    Engine engine;
    engine.newDocument(2000, 1500, 0);
    QImage square(200, 200, QImage::Format_ARGB32_Premultiplied);
    square.fill(Qt::black);
    QVERIFY(engine.addImageLayer(square, 900, 700, QStringLiteral("Square")));

    CanvasView canvas(&engine);
    canvas.resize(800, 600);
    canvas.refresh();

    auto shade = [&canvas](const QImage &shot, double x, double y) {
        return qGray(shot.pixel(canvas.documentToWidget(QPointF(x, y)).toPoint()));
    };

    canvas.fitToWindow();
    QVERIFY(canvas.zoom() < 0.5); // drawn from level 1 or below
    QImage shot = canvas.grab().toImage();
    QVERIFY2(shade(shot, 1000, 800) < 30, "the square is not where it belongs zoomed out");
    QVERIFY2(shade(shot, 850, 800) > 225, "the square spills left when zoomed out");
    QVERIFY2(shade(shot, 1150, 800) > 225, "the square spills right when zoomed out");
    QVERIFY2(shade(shot, 1000, 650) > 225, "the square spills up when zoomed out");

    canvas.setZoom(1.0);
    shot = canvas.grab().toImage();
    QVERIFY2(shade(shot, 902, 800) < 30, "the square's left edge moved at 100%");
    QVERIFY2(shade(shot, 897, 800) > 225, "the square's left edge moved at 100%");
    QVERIFY2(shade(shot, 1097, 898) < 30, "the square's corner moved at 100%");
    QVERIFY2(shade(shot, 1102, 903) > 225, "the square's corner moved at 100%");
}

void TestCanvasDamage::undoingAStrokeRedrawsOnlyItsTiles()
{
    // History records a stroke as the tiles it changed, so undoing it can
    // tell the canvas exactly where to redraw — a second's composite on a
    // large picture otherwise. And what the canvas is handed there has to be
    // the picture from before the stroke.
    Engine engine;
    engine.newDocument(2000, 1500, 0);
    engine.setBrush(24.0f, 100, 100, 100, 25);
    engine.setForegroundColor(QColor(200, 30, 30));
    engine.updateDisplay();
    const QImage before = engine.compositeImage();

    QVERIFY(engine.beginStroke(900.0f, 700.0f, 1.0f));
    engine.extendStroke(1000.0f, 760.0f, 1.0f);
    engine.endStroke();
    engine.updateDisplay();

    QVERIFY(engine.undo());
    const QRect damage = engine.updateDisplay();
    QVERIFY(damage.contains(QPoint(900, 700)));
    QVERIFY2(damage.width() <= 768 && damage.height() <= 768,
             qPrintable(QStringLiteral("undoing a stroke redraws %1x%2")
                            .arg(damage.width())
                            .arg(damage.height())));
    QCOMPARE(engine.displayImage(0, damage).convertToFormat(QImage::Format_RGBA8888_Premultiplied),
             before.copy(damage).convertToFormat(QImage::Format_RGBA8888_Premultiplied));

    QVERIFY(engine.redo());
    QVERIFY(engine.updateDisplay().width() <= 768);

    // Whatever undo reports, the one property that matters: once the canvas
    // has caught up, what it shows is the document — everywhere, not just
    // where it was told to look. Through a stroke, a filter over it, and
    // undoing and redoing both.
    auto showsTheDocument = [&engine] {
        engine.updateDisplay();
        const QRect all(0, 0, 2000, 1500);
        return engine.displayImage(0, all).convertToFormat(QImage::Format_RGBA8888_Premultiplied)
               == engine.compositeImage().convertToFormat(QImage::Format_RGBA8888_Premultiplied);
    };
    const float radius = 6.0f;
    engine.applyFilter(QStringLiteral("Gaussian Blur"), rust::Slice<const float>(&radius, 1));
    QVERIFY(showsTheDocument());
    QVERIFY(engine.undo());
    QVERIFY(showsTheDocument());
    QVERIFY(engine.undo());
    QVERIFY(showsTheDocument());
    QVERIFY(engine.redo());
    QVERIFY(showsTheDocument());
    QVERIFY(engine.redo());
    QVERIFY(showsTheDocument());
}

void TestCanvasDamage::everydayUseLeavesTheCanvasNothingToRedraw()
{
    // The whole application, not just the engine: the window, its panels and
    // everything listening to the engine. After a stroke the canvas redraws
    // the stroke; if anything listening then makes a change it does not
    // describe — a setter binding the whole engine, a query filling a cache
    // through the mutable path — the canvas redraws the whole document,
    // usually straight away in the same round of refreshes: a second with
    // every core busy on a large picture, felt as the next stroke starting
    // late and straight. Two such were found by hand; this makes the next
    // one fail here.
    Engine engine;
    CommandRegistry registry;
    QVERIFY(registry.load(QStringLiteral(PHOTORUST_SOURCE_RESOURCES "/shortcuts.json")));
    MainWindow window(&engine, &registry);
    window.resize(1400, 900);
    window.show();
    QVERIFY(QTest::qWaitForWindowExposed(&window));
    engine.newDocument(2000, 1500, 0);
    QTest::qWait(200);
    auto *canvas = window.findChild<CanvasView *>();
    QVERIFY(canvas);
    canvas->setActiveTool(ToolId::Brush);
    QTest::qWait(100);
    canvas->refresh();
    const int wholeRedraws = canvas->wholeRedraws();

    const QPoint c = canvas->rect().center();
    for (int stroke = 0; stroke < 3; ++stroke) {
        QTest::mousePress(canvas, Qt::LeftButton, {}, c + QPoint(stroke * 40, 0));
        for (int i = 1; i <= 15; ++i) {
            QTest::mouseMove(canvas, c + QPoint(stroke * 40 + i * 4, i * 2));
        }
        QTest::mouseRelease(canvas, Qt::LeftButton, {}, c + QPoint(stroke * 40 + 60, 30));
        QTest::qWait(50);
        canvas->refresh();
        QVERIFY2(canvas->wholeRedraws() == wholeRedraws,
                 qPrintable(QStringLiteral("stroke %1 made the canvas redraw the whole document; "
                                           "run with PHOTORUST_TRACE_EDITS=1 to see what")
                                .arg(stroke + 1)));
    }

    // Changing the brush, hovering, and switching tools change nothing on
    // the picture either.
    engine.setBrush(40.0f, 50, 100, 100, 25);
    for (int i = 0; i < 20; ++i) {
        QTest::mouseMove(canvas, c + QPoint(i * 7, i * 3));
    }
    for (auto tool : {ToolId::Move, ToolId::Marquee, ToolId::Eraser, ToolId::Brush}) {
        canvas->setActiveTool(tool);
        QTest::qWait(20);
    }
    canvas->refresh();
    QVERIFY2(canvas->wholeRedraws() == wholeRedraws,
             "brush settings, hovering or tool switches made the canvas redraw everything");
}

void TestCanvasDamage::typeLayerEditsRedrawOnlyTheText()
{
    // The Type tool adds a layer at every click, hides it while it is being
    // edited and puts it back with new pixels when it is committed — each of
    // them a second's composite on a large picture if it redrew everything.
    // They redraw only the text; but the text is styled here, and has a
    // layer clipped to it, so that "only the text" has to reach the shadow
    // and the clipped layer, including while the layer is hidden and its
    // style no longer counts.
    Engine engine;
    engine.newDocument(2000, 1500, 0);
    engine.updateDisplay();

    auto showsTheDocument = [&engine] {
        const QRect all(0, 0, 2000, 1500);
        return engine.displayImage(0, all).convertToFormat(QImage::Format_RGBA8888_Premultiplied)
               == engine.compositeImage().convertToFormat(QImage::Format_RGBA8888_Premultiplied);
    };
    auto addText = [&engine](const QImage &pixels, int x, int y) {
        engine.beginTextRuns();
        engine.addTextRun(QStringLiteral("HELLO"), QStringLiteral("Adwaita Mono"),
                          QStringLiteral("Regular"), 40.0f, QColor(Qt::black), 1.0f, 1.0f);
        return engine.addTextLayer(pixels, x, y, QStringLiteral("HELLO"), 0, true, false,
                                   float(x), float(y));
    };
    auto updateText = [&engine](int index, const QImage &pixels, int x, int y) {
        engine.beginTextRuns();
        engine.addTextRun(QStringLiteral("HELLO"), QStringLiteral("Adwaita Mono"),
                          QStringLiteral("Regular"), 40.0f, QColor(Qt::black), 1.0f, 1.0f);
        return engine.updateTextLayer(index, pixels, x, y, QStringLiteral("HELLO"), 0, true,
                                      false, float(x), float(y));
    };

    QImage ink(200, 60, QImage::Format_ARGB32_Premultiplied);
    ink.fill(Qt::black);
    QVERIFY(addText(ink, 900, 700));
    QRect damage = engine.updateDisplay();
    QVERIFY2(damage.width() < 400 && damage.height() < 400,
             qPrintable(QStringLiteral("adding a type layer redraws %1x%2")
                            .arg(damage.width())
                            .arg(damage.height())));
    QVERIFY(showsTheDocument());
    const int text = engine.getActiveLayerIndex();

    // A long drop shadow, and a red layer over the whole canvas clipped to
    // the text — hiding the text takes the red with it.
    QVERIFY(engine.setLayerEffectValue(text, QStringLiteral("dropShadow.on"), 1.0f));
    QVERIFY(engine.setLayerEffectValue(text, QStringLiteral("dropShadow.distance"), 60.0f));
    QVERIFY(engine.setLayerEffectValue(text, QStringLiteral("dropShadow.size"), 20.0f));
    engine.commitLayerEffects();
    QImage red(2000, 1500, QImage::Format_ARGB32_Premultiplied);
    red.fill(Qt::red);
    QVERIFY(engine.addImageLayer(red, 0, 0, QStringLiteral("Red")));
    engine.setLayerClipping(engine.getActiveLayerIndex(), true);
    engine.updateDisplay();
    QVERIFY(showsTheDocument());
    const int textNow = text + 1; // the red layer went in above it

    QVERIFY(engine.beginTextEdit(textNow));
    damage = engine.updateDisplay();
    QVERIFY(damage.width() < 600 && damage.height() < 600);
    QVERIFY2(showsTheDocument(), "hiding a styled type layer left its shadow or clip behind");

    QImage moved(300, 80, QImage::Format_ARGB32_Premultiplied);
    moved.fill(Qt::black);
    QVERIFY(updateText(textNow, moved, 400, 300));
    engine.updateDisplay();
    QVERIFY2(showsTheDocument(), "committing moved text left the old rendering behind");

    QVERIFY(engine.beginTextEdit(textNow));
    engine.updateDisplay();
    QVERIFY(showsTheDocument());
    engine.endTextEdit();
    engine.updateDisplay();
    QVERIFY2(showsTheDocument(), "ending an edit did not bring the shadow back");
}

void TestCanvasDamage::typingLeavesTheCanvasNothingToRedraw()
{
    // As `everydayUseLeavesTheCanvasNothingToRedraw`, for the Type tool:
    // clicking to start text, typing it, committing it and clicking to start
    // more must not recomposite the whole document at any step.
    Engine engine;
    CommandRegistry registry;
    QVERIFY(registry.load(QStringLiteral(PHOTORUST_SOURCE_RESOURCES "/shortcuts.json")));
    MainWindow window(&engine, &registry);
    window.resize(1400, 900);
    window.show();
    QVERIFY(QTest::qWaitForWindowExposed(&window));
    engine.newDocument(2000, 1500, 0);
    QTest::qWait(200);
    auto *canvas = window.findChild<CanvasView *>();
    QVERIFY(canvas);
    canvas->setActiveTool(ToolId::Type);
    QTest::qWait(100);
    canvas->refresh();
    const int wholeRedraws = canvas->wholeRedraws();

    const QPoint c = canvas->rect().center();
    for (int piece = 0; piece < 2; ++piece) {
        QTest::mouseClick(canvas, Qt::LeftButton, {}, c + QPoint(0, piece * 80));
        QTest::keyClicks(canvas, QStringLiteral("Hello"));
        canvas->refresh();
        QVERIFY2(canvas->wholeRedraws() == wholeRedraws,
                 qPrintable(QStringLiteral("starting text %1 made the canvas redraw the whole "
                                           "document; run with PHOTORUST_TRACE_EDITS=1 to see what")
                                .arg(piece + 1)));
    }
    canvas->commitTypeEdit();
    canvas->refresh();
    QVERIFY2(canvas->wholeRedraws() == wholeRedraws,
             "committing text made the canvas redraw the whole document");
    QCOMPARE(engine.layerTextRunText(engine.getActiveLayerIndex(), 0), QStringLiteral("Hello"));
}

void TestCanvasDamage::movingALayerRedrawsOnlyWhereItWasAndIs()
{
    // A Move-tool drag moves the layer on every mouse move, and redrawing
    // the whole document for each took over a second a step on a large
    // picture. Only where the layer was and where it is change — together
    // with its drop shadow, the layer clipped to it and a layer linked to
    // it, all of which have to come along.
    Engine engine;
    engine.newDocument(2000, 1500, 0);

    QImage square(200, 200, QImage::Format_ARGB32_Premultiplied);
    square.fill(Qt::black);
    QVERIFY(engine.addImageLayer(square, 300, 300, QStringLiteral("Linked")));
    QVERIFY(engine.addImageLayer(square, 900, 700, QStringLiteral("Square")));
    int moved = engine.getActiveLayerIndex();
    QVERIFY(engine.setLayerEffectValue(moved, QStringLiteral("dropShadow.on"), 1.0f));
    QVERIFY(engine.setLayerEffectValue(moved, QStringLiteral("dropShadow.distance"), 60.0f));
    engine.commitLayerEffects();
    QImage red(2000, 1500, QImage::Format_ARGB32_Premultiplied);
    red.fill(Qt::red);
    QVERIFY(engine.addImageLayer(red, 0, 0, QStringLiteral("Red")));
    engine.setLayerClipping(engine.getActiveLayerIndex(), true);
    moved += 1; // the red layer went in above it
    QVERIFY(engine.linkLayers(QList<int>{moved, moved + 1})); // "Linked" is just below
    engine.updateDisplay();

    auto showsTheDocument = [&engine] {
        const QRect all(0, 0, 2000, 1500);
        return engine.displayImage(0, all).convertToFormat(QImage::Format_RGBA8888_Premultiplied)
               == engine.compositeImage().convertToFormat(QImage::Format_RGBA8888_Premultiplied);
    };

    for (int step = 0; step < 3; ++step) {
        engine.offsetLayer(moved, 37, 21);
        const QRect damage = engine.updateDisplay();
        QVERIFY2(!damage.contains(QRect(0, 0, 2000, 1500)),
                 "moving a layer redrew the whole document");
        QVERIFY2(showsTheDocument(),
                 qPrintable(QStringLiteral("step %1 left the old position, the shadow, the "
                                           "clipped layer or the linked layer behind")
                                .arg(step + 1)));
    }
    engine.sealHistory();
    QVERIFY(engine.undo());
    engine.updateDisplay();
    QVERIFY2(showsTheDocument(), "undoing the drag left it half undone on screen");
}

void TestCanvasDamage::draggingALayerLeavesTheCanvasNothingToRedraw()
{
    // As `everydayUseLeavesTheCanvasNothingToRedraw`, for the Move tool.
    Engine engine;
    CommandRegistry registry;
    QVERIFY(registry.load(QStringLiteral(PHOTORUST_SOURCE_RESOURCES "/shortcuts.json")));
    MainWindow window(&engine, &registry);
    window.resize(1400, 900);
    window.show();
    QVERIFY(QTest::qWaitForWindowExposed(&window));
    engine.newDocument(2000, 1500, 0);
    QImage square(200, 200, QImage::Format_ARGB32_Premultiplied);
    square.fill(Qt::black);
    QVERIFY(engine.addImageLayer(square, 900, 700, QStringLiteral("Square")));
    QTest::qWait(200);
    auto *canvas = window.findChild<CanvasView *>();
    QVERIFY(canvas);
    canvas->setActiveTool(ToolId::Move);
    QTest::qWait(100);
    canvas->refresh();
    const int wholeRedraws = canvas->wholeRedraws();

    const QPoint start = canvas->documentToWidget(QPointF(1000, 800)).toPoint();
    QTest::mousePress(canvas, Qt::LeftButton, {}, start);
    for (int i = 1; i <= 15; ++i) {
        QTest::mouseMove(canvas, start + QPoint(i * 6, i * 3));
    }
    QTest::mouseRelease(canvas, Qt::LeftButton, {}, start + QPoint(90, 45));
    QTest::qWait(50);
    canvas->refresh();
    QVERIFY2(canvas->wholeRedraws() == wholeRedraws,
             "dragging a layer made the canvas redraw the whole document; "
             "run with PHOTORUST_TRACE_EDITS=1 to see what");
    QVERIFY(engine.layerOffsetX(engine.getActiveLayerIndex()) > 900);
}

QTEST_MAIN(TestCanvasDamage)
#include "tst_canvasdamage.moc"

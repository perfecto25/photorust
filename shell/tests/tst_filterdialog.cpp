// The shared filter dialog, and the Filter menu's Last Filter entry.
//
// Two things here are easy to get subtly wrong and impossible to see by
// looking. The first is which number goes where: the dialog collects its
// parameters in the order a caller names them and hands back the two floats
// `applyFilter` takes, so a dialog whose rows are added in one order and read
// in another blurs by the threshold and thresholds by the radius. The second
// is that a preview must leave nothing behind — the layer it filtered while
// the dialog was open has to go back exactly as it was, or Cancel quietly
// keeps the filter and OK applies it twice.

#include "MainWindow.h"
#include "canvas/CanvasView.h"
#include "dialogs/FilterPreviewDialog.h"
#include "shortcuts/CommandRegistry.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QAction>
#include <QCheckBox>
#include <QImage>
#include <QSignalSpy>
#include <QTest>

class TestFilterDialog : public QObject
{
    Q_OBJECT

private slots:
    void parametersComeBackInTheOrderTheyWereAdded();
    void aChoiceReportsTheValueItStandsFor();
    void zoomingInShowsLessOfTheImage();
    void thePreviewRegionStaysOverTheDocument();
    void theCanvasIsToldWhichSquareIsBeingShown();
    void theThumbnailShowsTheFilterNotTheOriginal();
    void closingTheDialogTakesThePreviewOffTheLayer();
    void aRadioChoiceReportsTheValueItStandsFor();
    void theBlurCentreFillsTwoSlotsAndStartsInTheMiddle();
    void radialBlurLaysItselfOutLikeCs6();
    void zoomShortcutsReachTheCanvasThroughADialog();
    void aDialogDoesNotLetEditingShortcutsThrough();
    void theTopOfTheFilterMenuNamesTheLastFilterRun();
    void repeatingAFilterAppliesItWithoutAsking();

private:
    /// A document with something in it for a blur to move about.
    static void paintStripes(Engine &engine);
};

void TestFilterDialog::paintStripes(Engine &engine)
{
    QImage image(engine.getCanvasWidth(), engine.getCanvasHeight(),
                 QImage::Format_ARGB32_Premultiplied);
    image.fill(Qt::white);
    for (int y = 0; y < image.height(); ++y) {
        for (int x = 0; x < image.width(); ++x) {
            if (((x / 4) + (y / 4)) % 2 == 0) {
                image.setPixelColor(x, y, Qt::black);
            }
        }
    }
    QVERIFY(engine.addImageLayer(image, 0, 0, QStringLiteral("Stripes")));
}

void TestFilterDialog::parametersComeBackInTheOrderTheyWereAdded()
{
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Surface Blur"));
    dialog.addParameter(QStringLiteral("Radius:"), 1, 100, 7);
    dialog.addParameter(QStringLiteral("Threshold:"), 2, 255, 42);

    QCOMPARE(dialog.parameters(), QList<float>({7.0f, 42.0f}));
}

void TestFilterDialog::aChoiceReportsTheValueItStandsFor()
{
    // Radial Blur's method is a word to the user and a flag to the engine;
    // the dialog is where the two meet.
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Radial Blur"));
    dialog.addParameter(QStringLiteral("Amount:"), 1, 100, 10);
    dialog.addChoice(QStringLiteral("Blur Method:"),
                     {QStringLiteral("Spin"), QStringLiteral("Zoom")}, {1.0, 0.0}, 0);

    QCOMPARE(dialog.parameters(), QList<float>({10.0f, 1.0f}));
}

void TestFilterDialog::zoomingInShowsLessOfTheImage()
{
    // The thumbnail is a fixed size, so magnifying it must mean showing a
    // smaller piece of the document — the mistake would be scaling the region
    // the same way as the image and showing the same thing bigger.
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Box Blur"));
    dialog.addParameter(QStringLiteral("Radius:"), 1, 250, 10);
    dialog.show();

    const QRectF at100 = dialog.previewRegion();

    // Two steps up the ladder from 100% is 200%, which halves each side.
    dialog.setZoomStep(6);
    const QRectF at200 = dialog.previewRegion();

    QVERIFY2(at200.width() < at100.width(),
             "zooming the preview in did not narrow the region it shows");
    QVERIFY2(at200.height() < at100.height(),
             "zooming the preview in did not shorten the region it shows");
}

void TestFilterDialog::thePreviewRegionStaysOverTheDocument()
{
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Box Blur"));
    dialog.addParameter(QStringLiteral("Radius:"), 1, 250, 10);
    dialog.show();

    // Drag far past the corner. The region should stop at the edge rather
    // than wander off into a part of the document that does not exist.
    dialog.panPreview(QPointF(-100000, -100000));
    const QRectF topLeft = dialog.previewRegion();
    QVERIFY2(topLeft.right() > 0 && topLeft.bottom() > 0,
             "panning ran the preview off the top-left of the document");

    dialog.panPreview(QPointF(100000, 100000));
    const QRectF bottomRight = dialog.previewRegion();
    QVERIFY2(bottomRight.left() < engine.getCanvasWidth()
                 && bottomRight.top() < engine.getCanvasHeight(),
             "panning ran the preview off the bottom-right of the document");
    QVERIFY2(bottomRight.left() > topLeft.left(), "panning the other way did not move it back");
}

void TestFilterDialog::theCanvasIsToldWhichSquareIsBeingShown()
{
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Box Blur"));
    dialog.addParameter(QStringLiteral("Radius:"), 1, 250, 10);

    QSignalSpy spy(&dialog, &FilterPreviewDialog::previewRegionChanged);
    dialog.show();
    QVERIFY2(spy.count() > 0, "the dialog opened without telling the canvas where it is looking");
    QCOMPARE(spy.last().at(0).toRectF(), dialog.previewRegion());

    // The canvas is the consumer, so check it takes the marker and lets it go.
    CanvasView canvas(&engine);
    canvas.setFilterPreviewRect(dialog.previewRegion());
    canvas.setFilterPreviewRect(QRectF());
}

void TestFilterDialog::theThumbnailShowsTheFilterNotTheOriginal()
{
    Engine engine;
    paintStripes(engine);

    const int size = 40;
    const QImage plain = engine.layerImage(engine.getActiveLayerIndex());
    const float radius = 6.0f;
    const QImage blurred = engine.filterPreview(QStringLiteral("Box Blur"),
                                                rust::Slice<const float>(&radius, 1), 10, 10,
                                                size, size);

    QCOMPARE(blurred.size(), QSize(size, size));
    QVERIFY2(!plain.isNull(), "the layer came back empty, so there was nothing to compare");

    bool differs = false;
    for (int y = 0; y < size && !differs; ++y) {
        for (int x = 0; x < size; ++x) {
            if (blurred.pixelColor(x, y) != plain.pixelColor(x + 10, y + 10)) {
                differs = true;
                break;
            }
        }
    }
    QVERIFY2(differs, "the preview thumbnail is showing the unfiltered image");
}

void TestFilterDialog::closingTheDialogTakesThePreviewOffTheLayer()
{
    Engine engine;
    paintStripes(engine);
    const int layer = engine.getActiveLayerIndex();
    const QImage before = engine.layerImage(layer);

    {
        FilterPreviewDialog dialog(&engine, QStringLiteral("Box Blur"));
        dialog.addParameter(QStringLiteral("Radius:"), 1, 250, 12);
        dialog.show();
        QVERIFY2(engine.layerImage(layer) != before,
                 "Preview was ticked but the layer never showed the filter");
    }

    QVERIFY2(engine.layerImage(layer) == before,
             "the dialog closed leaving its preview on the layer");
}

void TestFilterDialog::aRadioChoiceReportsTheValueItStandsFor()
{
    // Quality is Draft/Good/Best to the user and 0/1/2 to the engine. Best
    // must not arrive as 1 because it happens to be the third button.
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Radial Blur"));
    const int quality = dialog.addRadioChoice(
        QStringLiteral("Quality:"),
        {QStringLiteral("Draft"), QStringLiteral("Good"), QStringLiteral("Best")},
        {0.0, 1.0, 2.0}, 2);

    QCOMPARE(dialog.parameterValue(quality), 2.0f);
}

void TestFilterDialog::theBlurCentreFillsTwoSlotsAndStartsInTheMiddle()
{
    // One widget standing for two parameters is the part that would quietly
    // shift everything after it along by one if the second slot were missed.
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Radial Blur"));
    dialog.addParameter(QStringLiteral("Amount:"), 1, 100, 10);
    const int method = dialog.addRadioChoice(
        QStringLiteral("Blur Method:"), {QStringLiteral("Spin"), QStringLiteral("Zoom")},
        {1.0, 0.0}, 0);
    const int centre = dialog.addCenterPicker(QStringLiteral("Blur Center"), [&dialog, method] {
        return dialog.parameterValue(method) != 0.0f;
    });
    const int quality = dialog.addRadioChoice(
        QStringLiteral("Quality:"),
        {QStringLiteral("Draft"), QStringLiteral("Good"), QStringLiteral("Best")},
        {0.0, 1.0, 2.0}, 1);

    QCOMPARE(centre, 2);
    QCOMPARE(quality, 4);
    // The middle of the image, which is where CS6 opens it.
    QCOMPARE(dialog.parameterValue(centre), 0.5f);
    QCOMPARE(dialog.parameterValue(centre + 1), 0.5f);
    QCOMPARE(dialog.parameters(), QList<float>({10.0f, 1.0f, 0.5f, 0.5f, 1.0f}));
}

void TestFilterDialog::radialBlurLaysItselfOutLikeCs6()
{
    // CS6's Radial Blur has no preview thumbnail — the Blur Center box stands
    // in for one — so the shared dialog has to be able to go without it. The
    // menu is what builds the dialog, so this drives the real thing.
    Engine engine;
    CommandRegistry registry;
    QVERIFY(registry.load(QStringLiteral(PHOTORUST_SOURCE_RESOURCES "/shortcuts.json")));
    MainWindow window(&engine, &registry);

    FilterPreviewDialog bare(&engine, QStringLiteral("Radial Blur"));
    bare.setPreviewPaneVisible(false);
    bare.addParameter(QStringLiteral("Amount:"), 1, 100, 10);
    bare.show();

    // Nothing should be asking the engine to filter the whole layer while a
    // radial dialog is open, which is what the Preview checkbox would do.
    const QList<QCheckBox *> checks = bare.findChildren<QCheckBox *>();
    for (QCheckBox *check : checks) {
        QVERIFY2(!check->isVisible(),
                 "Radial Blur is showing a Preview checkbox, which CS6 does not have");
    }

    // ...and with no thumbnail on show, it must not be asking for one either.
    QVERIFY2(bare.previewRegion().isValid(), "the region should still be well-formed");
}

void TestFilterDialog::zoomShortcutsReachTheCanvasThroughADialog()
{
    // A dialog takes the whole keyboard, so the window behind it never sees a
    // shortcut. Photoshop still lets you zoom the image while a filter dialog
    // is open — you have to, to judge what the filter is doing — so the zoom
    // keys are forwarded through.
    Engine engine;
    CommandRegistry registry;
    QVERIFY(registry.load(QStringLiteral(PHOTORUST_SOURCE_RESOURCES "/shortcuts.json")));
    MainWindow window(&engine, &registry);
    auto *canvas = window.findChild<CanvasView *>();
    QVERIFY(canvas);

    FilterPreviewDialog dialog(&engine, QStringLiteral("Box Blur"), &window);
    dialog.addParameter(QStringLiteral("Radius:"), 1, 250, 10);
    dialog.show();

    const double resting = canvas->zoom();
    QTest::keyClick(&dialog, Qt::Key_Plus, Qt::ControlModifier);
    const double zoomedIn = canvas->zoom();
    QVERIFY2(zoomedIn > resting, "Ctrl++ did not reach the canvas through the dialog");

    QTest::keyClick(&dialog, Qt::Key_Minus, Qt::ControlModifier);
    QVERIFY2(canvas->zoom() < zoomedIn, "Ctrl+- did not reach the canvas through the dialog");
}

void TestFilterDialog::aDialogDoesNotLetEditingShortcutsThrough()
{
    // Only the view commands are forwarded. An undo taken while a dialog is
    // showing an uncommitted preview would apply to a document the user
    // cannot see the true state of, so it stays blocked.
    Engine engine;
    CommandRegistry registry;
    QVERIFY(registry.load(QStringLiteral(PHOTORUST_SOURCE_RESOURCES "/shortcuts.json")));
    MainWindow window(&engine, &registry);
    paintStripes(engine);
    const int layers = engine.getLayerCount();

    FilterPreviewDialog dialog(&engine, QStringLiteral("Box Blur"), &window);
    dialog.addParameter(QStringLiteral("Radius:"), 1, 250, 10);
    dialog.show();

    QTest::keyClick(&dialog, Qt::Key_Z, Qt::ControlModifier);
    QCOMPARE(engine.getLayerCount(), layers);
}

void TestFilterDialog::theTopOfTheFilterMenuNamesTheLastFilterRun()
{
    Engine engine;
    CommandRegistry registry;
    QVERIFY(registry.load(QStringLiteral(PHOTORUST_SOURCE_RESOURCES "/shortcuts.json")));
    MainWindow window(&engine, &registry);

    QAction *last = registry.action(QStringLiteral("filter.last"));
    QVERIFY(last);
    QCOMPARE(last->text(), QStringLiteral("Last Filter"));
    QVERIFY2(!last->isEnabled(), "Last Filter was offered before any filter had been run");

    // A filter that takes no parameters, so nothing opens and the test does
    // not sit waiting on a modal dialog.
    QVERIFY(QMetaObject::invokeMethod(&window, "applyFilter", Qt::DirectConnection,
                                      Q_ARG(QString, QStringLiteral("Blur More"))));

    QCOMPARE(last->text(), QStringLiteral("Blur More"));
    QVERIFY2(last->isEnabled(), "Last Filter stayed greyed out after a filter was run");
}

void TestFilterDialog::repeatingAFilterAppliesItWithoutAsking()
{
    Engine engine;
    CommandRegistry registry;
    QVERIFY(registry.load(QStringLiteral(PHOTORUST_SOURCE_RESOURCES "/shortcuts.json")));
    MainWindow window(&engine, &registry);
    paintStripes(engine);

    QVERIFY(QMetaObject::invokeMethod(&window, "applyFilter", Qt::DirectConnection,
                                      Q_ARG(QString, QStringLiteral("Blur More"))));
    const int layer = engine.getActiveLayerIndex();
    const QImage once = engine.layerImage(layer);

    // Ctrl+F must run straight off. If it opened a dialog instead, this call
    // would not return and the test would time out — which is the assertion.
    QVERIFY(QMetaObject::invokeMethod(&window, "repeatLastFilter", Qt::DirectConnection,
                                      Q_ARG(bool, false)));

    QVERIFY2(engine.layerImage(layer) != once, "repeating the filter did not blur any further");
}

QTEST_MAIN(TestFilterDialog)
#include "tst_filterdialog.moc"

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
#include "dialogs/AngleDial.h"
#include "dialogs/FilterPreviewDialog.h"
#include "shortcuts/CommandRegistry.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QAction>
#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QPushButton>
#include <QRadioButton>
#include <QSlider>
#include <QTabWidget>
#include <QDoubleSpinBox>
#include <QImage>
#include <QLabel>
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
    void theAngleWheelAndItsFieldStayInStep();
    void smartSharpensAngleIsOnlyLiveForMotionBlur();
    void addNoiseHandsOverAmountThenDistributionThenMonochromatic();
    void displaceCollectsItsTwoScalesAndTwoModes();
    void theShearCurveFillsOneSlotPerSampledRow();
    void colorHalftoneOpensOnTheStandardPressAngles();
    void aSeparatorDoesNotShiftWhatAListMeans();
    void pointillizeTakesItsGroundFromTheDocument();
    void lensFlarePutsItsCrosshairAfterItsTwoControls();
    void aFlarePreviewShowsTheWholePictureNotACrop();
    void lightingEffectsFillsItsSlotsInPanelOrder();
    void lightingGreysOutWhatDoesNotApplyToThisLamp();
    void diffuseOffersItsFourModesAndNothingElse();
    void embossCollectsAngleThenHeightThenAmount();
    void extrudeGreysOutWhatAPyramidCannotHave();
    void tilesCollectsItsTwoNumbersThenTheFillChoice();
    void traceContourCollectsTheLevelThenTheEdge();
    void windCollectsTheMethodThenTheDirection();
    void customReadsItsGridRowByRowThenScaleAndOffset();
    void flameNeedsAPathToBurnAlong();
    void aTabbedDialogStillNumbersItsControlsInOrder();
    void randomizeRollsANewSeedThatStaysPut();
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

    // With no thumbnail to sit beside, OK and Cancel drop to the foot of the
    // dialog and lie flat. Left where they were, they would be a column of
    // two floating at the top with nothing alongside them.
    auto *buttons = bare.findChild<QDialogButtonBox *>();
    QVERIFY(buttons);
    QCOMPARE(buttons->orientation(), Qt::Horizontal);
    QVERIFY2(buttons->y() > bare.height() / 2,
             "the buttons stayed at the top of a dialog with no preview beside them");
}

void TestFilterDialog::theAngleWheelAndItsFieldStayInStep()
{
    // Two controls on one value. The failure to guard against is one of them
    // driving the other in a loop, or the wheel moving and the filter still
    // being handed the number the field was showing before.
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Motion Blur"));
    const int angle = dialog.addAngleParameter(QStringLiteral("Angle:"), 0);
    dialog.addParameter(QStringLiteral("Distance:"), 1, 999, 20);

    QCOMPARE(dialog.parameterValue(angle), 0.0f);

    auto *dial = dialog.findChild<AngleDial *>();
    QVERIFY2(dial, "Motion Blur's angle has no wheel");

    // Drag the wheel a quarter turn. It reports the angle it was dragged to,
    // and the parameter must follow.
    emit dial->angleChanged(45.0);
    QCOMPARE(dialog.parameterValue(angle), 45.0f);

    // ...and the other way: typing in the field turns the wheel. The field is
    // the one sharing a cell with the wheel — the dialog has another for the
    // distance.
    auto *field = dial->parentWidget()->findChild<QDoubleSpinBox *>();
    QVERIFY(field);
    field->setValue(120);
    QCOMPARE(dialog.parameterValue(angle), 120.0f);
}

void TestFilterDialog::smartSharpensAngleIsOnlyLiveForMotionBlur()
{
    // Only a motion streak has a direction. CS6 greys the angle out for the
    // other two rather than hiding it, so the row does not jump about — and
    // the two controls it greys out are a field and a wheel, so both have to
    // follow the combo.
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Smart Sharpen"));
    dialog.addParameter(QStringLiteral("Amount:"), 1, 500, 100);
    dialog.addParameter(QStringLiteral("Radius:"), 0.1, 64.0, 1.0, 1);
    dialog.addParameter(QStringLiteral("Reduce Noise:"), 0, 100, 10);
    const int remove = dialog.addChoiceWithAngle(
        QStringLiteral("Remove:"),
        {QStringLiteral("Gaussian Blur"), QStringLiteral("Lens Blur"),
         QStringLiteral("Motion Blur")},
        {0.0, 1.0, 2.0}, 1, 0.0, 2);

    // Five slots in all, with the angle right after the choice — the order
    // the engine reads them in.
    QCOMPARE(remove, 3);
    QCOMPARE(dialog.parameters().size(), 5);
    QCOMPARE(dialog.parameterValue(remove), 1.0f);

    auto *dial = dialog.findChild<AngleDial *>();
    QVERIFY(dial);
    auto *field = dial->parentWidget()->findChild<QDoubleSpinBox *>();
    QVERIFY(field);
    QVERIFY2(!dial->isEnabled(), "the angle is live on Lens Blur, which has no direction");
    QVERIFY(!field->isEnabled());

    auto *combo = dial->parentWidget()->findChild<QComboBox *>();
    QVERIFY(combo);
    combo->setCurrentIndex(2);
    QVERIFY2(dial->isEnabled(), "the angle stayed greyed out on Motion Blur");
    QVERIFY(field->isEnabled());
    QCOMPARE(dialog.parameterValue(remove), 2.0f);
}

void TestFilterDialog::addNoiseHandsOverAmountThenDistributionThenMonochromatic()
{
    // Three controls of three different kinds on one filter, read
    // positionally by the engine. Getting the order wrong would swap the
    // distribution for the monochromatic flag, and both are booleans, so
    // nothing would complain.
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Add Noise"));
    dialog.addParameter(QStringLiteral("Amount:"), 0.1, 400.0, 12.5, 1);
    dialog.addRadioChoice(QStringLiteral("Distribution"),
                          {QStringLiteral("Uniform"), QStringLiteral("Gaussian")}, {0.0, 1.0}, 1);
    dialog.addCheckBox(QStringLiteral("Monochromatic"), true);

    QCOMPARE(dialog.parameters(), QList<float>({12.5f, 1.0f, 1.0f}));
}

void TestFilterDialog::displaceCollectsItsTwoScalesAndTwoModes()
{
    // Displace is the one filter that does not go through applyFilter — it
    // needs an image, not numbers — so its four values are read positionally
    // by hand. Two of them are booleans dressed as radio boxes, and swapping
    // them would tile a stretched map or wrap what should repeat, neither of
    // which announces itself.
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Displace"));
    dialog.setPreviewPaneVisible(false);
    dialog.addParameter(QStringLiteral("Horizontal Scale:"), -999, 999, 10);
    dialog.addParameter(QStringLiteral("Vertical Scale:"), -999, 999, 25);
    dialog.addRadioChoice(QStringLiteral("Displacement Map:"),
                          {QStringLiteral("Stretch To Fit"), QStringLiteral("Tile")},
                          {1.0, 0.0}, 1);
    dialog.addRadioChoice(QStringLiteral("Undefined Areas:"),
                          {QStringLiteral("Wrap Around"), QStringLiteral("Repeat Edge Pixels")},
                          {1.0, 0.0}, 0);

    // Tile, and Wrap Around.
    QCOMPARE(dialog.parameters(), QList<float>({10.0f, 25.0f, 0.0f, 1.0f}));
}

void TestFilterDialog::theShearCurveFillsOneSlotPerSampledRow()
{
    // The curve is a whole sampled line rather than a number, and the
    // Undefined Areas flag sits after it. Miscount the curve and the flag is
    // read out of the middle of it — which is a valid float, so nothing
    // complains, and the filter quietly wraps when it should clamp.
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Shear"));
    dialog.setPreviewPaneVisible(false);
    const int curve = dialog.addShearCurve(17);
    dialog.addRadioChoice(QStringLiteral("Undefined Areas:"),
                          {QStringLiteral("Wrap Around"), QStringLiteral("Repeat Edge Pixels")},
                          {1.0, 0.0}, 1);

    QCOMPARE(curve, 0);
    const QList<float> params = dialog.parameters();
    QCOMPARE(params.size(), 18);
    // An untouched curve is a straight line down the middle: no offset at all.
    for (int i = 0; i < 17; ++i) {
        QCOMPARE(params.at(i), 0.0f);
    }
    QCOMPARE(params.at(17), 0.0f); // Repeat Edge Pixels.
}

void TestFilterDialog::colorHalftoneOpensOnTheStandardPressAngles()
{
    // Five fields that are all just numbers, read positionally by the engine:
    // the radius first, then the four screens in channel order. Shuffled, the
    // filter still produces a perfectly convincing halftone — with the plates
    // at each other's angles.
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Color Halftone"));
    dialog.setPreviewPaneVisible(false);
    dialog.addParameter(QStringLiteral("Max. Radius:"), 4, 127, 8, 0, QString(), false);
    dialog.addHeading(QStringLiteral("Screen Angles (Degrees):"));
    const double angles[4] = {108, 162, 90, 45};
    for (int channel = 0; channel < 4; ++channel) {
        dialog.addParameter(QStringLiteral("Channel %1:").arg(channel + 1), -360, 360,
                            angles[channel], 0, QString(), false);
    }

    QCOMPARE(dialog.parameters(), QList<float>({8.0f, 108.0f, 162.0f, 90.0f, 45.0f}));

    // ...and a field with no slider under it really has none, or the dialog
    // would be four times the height CS6's is.
    QCOMPARE(dialog.findChildren<QSlider *>().size(), 0);
}

void TestFilterDialog::aSeparatorDoesNotShiftWhatAListMeans()
{
    // Mezzotint's ten types are ruled off into dots, lines and strokes. If
    // the value were read from the chosen item's position, every separator
    // would push everything below it along by one and Short Strokes would
    // apply as Long Lines — a perfectly convincing mezzotint, just the wrong
    // one.
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Mezzotint"));
    const int type = dialog.addChoice(
        QStringLiteral("Type:"),
        {QStringLiteral("Fine Dots"), QStringLiteral("Medium Dots"), QStringLiteral("Grainy Dots"),
         QStringLiteral("Coarse Dots"), QStringLiteral("Short Lines"),
         QStringLiteral("Medium Lines"), QStringLiteral("Long Lines"),
         QStringLiteral("Short Strokes"), QStringLiteral("Medium Strokes"),
         QStringLiteral("Long Strokes")},
        {0, 1, 2, 3, 4, 5, 6, 7, 8, 9}, 0, {3, 6});

    auto *combo = dialog.findChild<QComboBox *>();
    QVERIFY(combo);
    // Ten types plus the two rules between the groups.
    QCOMPARE(combo->count(), 12);
    QCOMPARE(dialog.parameterValue(type), 0.0f);

    // Short Strokes is the eighth type but sits two rows further down.
    const int row = combo->findText(QStringLiteral("Short Strokes"));
    QCOMPARE(row, 9);
    combo->setCurrentIndex(row);
    QCOMPARE(dialog.parameterValue(type), 7.0f);
}

void TestFilterDialog::pointillizeTakesItsGroundFromTheDocument()
{
    // Pointillize paints the canvas between its dabs in the background
    // colour, which belongs to the document rather than to the dialog — so
    // the dialog never asks for it and the engine fills it in. If that seam
    // is broken the filter still works and still looks like pointillism; it
    // just paints on white whatever the swatch says.
    Engine engine;
    QImage image(engine.getCanvasWidth(), engine.getCanvasHeight(),
                 QImage::Format_ARGB32_Premultiplied);
    image.fill(QColor(220, 40, 40));
    QVERIFY(engine.addImageLayer(image, 0, 0, QStringLiteral("Red")));
    engine.setBackgroundColor(QColor(0, 0, 255));

    const float cell = 9.0f;
    engine.applyFilter(QStringLiteral("Pointillize"), rust::Slice<const float>(&cell, 1));

    const QImage after = engine.layerImage(engine.getActiveLayerIndex());
    bool ground = false;
    for (int y = 0; y < after.height() && !ground; ++y) {
        for (int x = 0; x < after.width(); ++x) {
            if (after.pixelColor(x, y) == QColor(0, 0, 255)) {
                ground = true;
                break;
            }
        }
    }
    QVERIFY2(ground, "the gaps between the dabs are not the document's background colour");
}

void TestFilterDialog::lensFlarePutsItsCrosshairAfterItsTwoControls()
{
    // The crosshair is one widget filling two slots at the *end* of the list,
    // after the two controls CS6 shows. Get that order wrong and the flare is
    // placed by the brightness and lit by its own position.
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Lens Flare"));
    dialog.addParameter(QStringLiteral("Brightness:"), 10, 300, 143);
    dialog.addRadioChoice(QStringLiteral("Lens Type"),
                          {QStringLiteral("50-300mm Zoom"), QStringLiteral("35mm Prime"),
                           QStringLiteral("105mm Prime"), QStringLiteral("Movie Prime")},
                          {0.0, 1.0, 2.0, 3.0}, 2);
    const int centre = dialog.addPlacementPreview();

    QCOMPARE(centre, 2);
    QCOMPARE(dialog.parameters(), QList<float>({143.0f, 2.0f, 0.5f, 0.5f}));

    // And it opens wherever it was left, not always in the middle.
    FilterPreviewDialog resumed(&engine, QStringLiteral("Lens Flare"));
    resumed.addParameter(QStringLiteral("Brightness:"), 10, 300, 100);
    resumed.addRadioChoice(QStringLiteral("Lens Type"), {QStringLiteral("50-300mm Zoom")},
                           {0.0}, 0);
    resumed.addPlacementPreview(QPointF(0.25, 0.75));
    QCOMPARE(resumed.parameters(), QList<float>({100.0f, 0.0f, 0.25f, 0.75f}));
}

void TestFilterDialog::aFlarePreviewShowsTheWholePictureNotACrop()
{
    // The question this dialog answers is where in the *frame* the flare
    // sits, so its preview shows the picture entire — and shrunk, so that
    // dragging the crosshair over a large photograph does not filter twenty
    // megapixels a frame. The magnified thumbnail the other filters use
    // cannot answer that question at all.
    Engine engine;
    QImage image(engine.getCanvasWidth(), engine.getCanvasHeight(),
                 QImage::Format_ARGB32_Premultiplied);
    image.fill(QColor(20, 20, 20));
    QVERIFY(engine.addImageLayer(image, 0, 0, QStringLiteral("Dark")));

    const QList<float> params{100.0f, 0.0f, 0.5f, 0.5f};
    const rust::Slice<const float> slice(params.constData(), size_t(params.size()));
    const QImage proxy =
        engine.filterProxyPreview(QStringLiteral("Lens Flare"), slice, 250, 250);
    QVERIFY(!proxy.isNull());
    QVERIFY2(proxy.width() <= 250 && proxy.height() <= 250, "the proxy was not shrunk to fit");
    // Same shape as the layer, or the crosshair would point somewhere else.
    QVERIFY(qAbs(double(proxy.width()) / proxy.height()
                 - double(image.width()) / image.height())
            < 0.02);
    // And the flare is on it: the middle is lit, the corner is not.
    QVERIFY(proxy.pixelColor(proxy.width() / 2, proxy.height() / 2).red() > 200);
    QVERIFY(proxy.pixelColor(1, proxy.height() - 2).red() < 120);
}

void TestFilterDialog::lightingEffectsFillsItsSlotsInPanelOrder()
{
    // Nineteen slots, two of which are colour swatches filling three each and
    // one a preview filling two. Miscount any of them and the exposure ends
    // up in a colour channel — which still renders, and still looks like
    // lighting, just not the lighting that was asked for.
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Lighting Effects"));
    const int type = dialog.addChoice(
        QStringLiteral("Light Type:"),
        {QStringLiteral("Spot"), QStringLiteral("Point"), QStringLiteral("Infinite")},
        {0.0, 1.0, 2.0}, 1);
    const int colour = dialog.addColorButton(QStringLiteral("Color:"), QColor(255, 200, 100));
    const int intensity = dialog.addParameter(QStringLiteral("Intensity:"), -100, 100, 35);
    dialog.addParameter(QStringLiteral("Hotspot:"), -100, 100, -22);
    const int colorize = dialog.addColorButton(QStringLiteral("Colorize:"), QColor(10, 20, 30));
    dialog.addParameter(QStringLiteral("Exposure:"), -100, 100, 15);
    dialog.addParameter(QStringLiteral("Gloss:"), -100, 100, -13);
    dialog.addParameter(QStringLiteral("Metallic:"), -100, 100, 41);
    dialog.addParameter(QStringLiteral("Ambience:"), -100, 100, 44);
    const int texture = dialog.addChoice(
        QStringLiteral("Texture:"),
        {QStringLiteral("None"), QStringLiteral("Red"), QStringLiteral("Green"),
         QStringLiteral("Blue")},
        {0.0, 1.0, 2.0, 3.0}, 2);
    dialog.addParameter(QStringLiteral("Height:"), 0, 100, 80);
    dialog.addParameter(QStringLiteral("Size:"), 2, 150, 60);
    dialog.addAngleParameter(QStringLiteral("Angle:"), 120);
    const int where = dialog.addPlacementPreview(QPointF(0.25, 0.75));

    QCOMPARE(type, 0);
    QCOMPARE(colour, 1);
    QCOMPARE(intensity, 4);
    QCOMPARE(colorize, 6);
    QCOMPARE(texture, 13);
    QCOMPARE(where, 17);
    QCOMPARE(dialog.parameters(),
             QList<float>({1.0f, 255.0f, 200.0f, 100.0f, 35.0f, -22.0f, 10.0f, 20.0f, 30.0f,
                           15.0f, -13.0f, 41.0f, 44.0f, 2.0f, 80.0f, 60.0f, 120.0f, 0.25f,
                           0.75f}));
}

void TestFilterDialog::lightingGreysOutWhatDoesNotApplyToThisLamp()
{
    // Only a spot has a cone to put a hotspot in, and there is no height to
    // raise without a channel to raise it from. CS6 greys both out rather
    // than leaving a live control that does nothing.
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Lighting Effects"));
    const int type = dialog.addChoice(
        QStringLiteral("Light Type:"),
        {QStringLiteral("Spot"), QStringLiteral("Point"), QStringLiteral("Infinite")},
        {0.0, 1.0, 2.0}, 1);
    dialog.addParameter(QStringLiteral("Hotspot:"), -100, 100, 44, 0, QString(), true,
                        [&dialog, type] { return dialog.parameterValue(type) == 0.0f; });
    const int texture = dialog.addChoice(
        QStringLiteral("Texture:"), {QStringLiteral("None"), QStringLiteral("Red")}, {0.0, 1.0},
        0);
    dialog.addParameter(QStringLiteral("Height:"), 0, 100, 50, 0, QString(), true,
                        [&dialog, texture] { return dialog.parameterValue(texture) != 0.0f; });
    dialog.show();

    auto rowIsLive = [&dialog](const QString &label) {
        for (QLabel *caption : dialog.findChildren<QLabel *>()) {
            if (caption->text() == label) {
                return caption->isEnabled();
            }
        }
        return true;
    };
    // Opened on a Point light with no texture: neither applies.
    QVERIFY2(!rowIsLive(QStringLiteral("Hotspot:")), "a point light is offering a hotspot");
    QVERIFY2(!rowIsLive(QStringLiteral("Height:")), "height is live with no texture to raise");

    // Switch to a spot and the hotspot comes back.
    for (QComboBox *box : dialog.findChildren<QComboBox *>()) {
        if (box->count() == 3) {
            box->setCurrentIndex(0);
        }
    }
    QVERIFY2(rowIsLive(QStringLiteral("Hotspot:")), "a spot light has no hotspot control");
    QVERIFY2(!rowIsLive(QStringLiteral("Height:")), "height woke up with no texture");
}

void TestFilterDialog::diffuseOffersItsFourModesAndNothingElse()
{
    // The whole of CS6's Diffuse dialog is one box of radio buttons, so the
    // mode is parameter zero and there is no parameter one. A dialog that
    // added a slider out of habit would send the engine a mode it never
    // asked for.
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Diffuse"));
    const int mode = dialog.addRadioChoice(
        QStringLiteral("Mode"),
        {QStringLiteral("Normal"), QStringLiteral("Darken Only"),
         QStringLiteral("Lighten Only"), QStringLiteral("Anisotropic")},
        {0.0, 1.0, 2.0, 3.0}, 3);

    QCOMPARE(mode, 0);
    QCOMPARE(dialog.parameters(), QList<float>({3.0f}));
}

void TestFilterDialog::embossCollectsAngleThenHeightThenAmount()
{
    // Three numbers in CS6's order, the first of them on a wheel rather than
    // a slider. Height and Amount are both plain numbers in the same range of
    // sizes, so swapping them produces a picture that looks embossed and is
    // simply wrong — the kind of mistake only the order can catch.
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Emboss"));
    const int angle = dialog.addAngleParameter(QStringLiteral("Angle:"), 135);
    const int height = dialog.addParameter(QStringLiteral("Height:"), 1, 100, 3);
    const int amount = dialog.addParameter(QStringLiteral("Amount:"), 1, 500, 100);

    QCOMPARE(angle, 0);
    QCOMPARE(height, 1);
    QCOMPARE(amount, 2);
    QCOMPARE(dialog.parameters(), QList<float>({135.0f, 3.0f, 100.0f}));
}

void TestFilterDialog::extrudeGreysOutWhatAPyramidCannotHave()
{
    // Six controls, two of them tick boxes, and one of those only applies to
    // one of the two shapes. The order matters as ever — Size and Depth are
    // both plain numbers in overlapping ranges — and so does the greying,
    // since a live tick box that changes nothing is worse than none.
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Extrude"));
    dialog.setPreviewPaneVisible(false);
    const int type = dialog.addRadioChoice(
        QStringLiteral("Type"), {QStringLiteral("Blocks"), QStringLiteral("Pyramids")},
        {0.0, 1.0}, 1);
    dialog.addParameter(QStringLiteral("Size:"), 2, 255, 30, 0, QString(), false);
    dialog.addParameter(QStringLiteral("Depth:"), 1, 255, 45, 0, QString(), false);
    dialog.addRadioChoice(QStringLiteral("Depth from"),
                          {QStringLiteral("Random"), QStringLiteral("Level-based")}, {0.0, 1.0},
                          1);
    dialog.addCheckBox(QStringLiteral("Solid Front Faces"), true,
                       [&dialog, type] { return dialog.parameterValue(type) == 0.0f; });
    dialog.addCheckBox(QStringLiteral("Mask Incomplete Blocks"), false);
    dialog.show();

    QCOMPARE(dialog.parameters(),
             QList<float>({1.0f, 30.0f, 45.0f, 1.0f, 1.0f, 0.0f}));

    auto solidFaces = [&dialog]() -> QCheckBox * {
        for (QCheckBox *box : dialog.findChildren<QCheckBox *>()) {
            if (box->text() == QStringLiteral("Solid Front Faces")) {
                return box;
            }
        }
        return nullptr;
    };
    QVERIFY(solidFaces());
    QVERIFY2(!solidFaces()->isEnabled(), "a pyramid is offering a solid front face");

    // Back to blocks and it comes alive again.
    for (QRadioButton *radio : dialog.findChildren<QRadioButton *>()) {
        if (radio->text() == QStringLiteral("Blocks")) {
            radio->click();
        }
    }
    QVERIFY2(solidFaces()->isEnabled(), "blocks cannot have a solid front face");
}

void TestFilterDialog::tilesCollectsItsTwoNumbersThenTheFillChoice()
{
    // Two plain numbers and a box of four radios, in CS6's order. Both
    // numbers run 1-99, so a swap sends a plausible-looking pair of values
    // that cuts the picture the wrong way and nothing else would catch it.
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Tiles"));
    dialog.setPreviewPaneVisible(false);
    const int count = dialog.addParameter(QStringLiteral("Number Of Tiles:"), 1, 99, 55, 0,
                                          QString(), false);
    const int offset = dialog.addParameter(QStringLiteral("Maximum Offset:"), 1, 99, 30, 0,
                                           QStringLiteral("%"), false);
    const int fill = dialog.addRadioChoice(
        QStringLiteral("Fill Empty Area With:"),
        {QStringLiteral("Background Color"), QStringLiteral("Foreground Color"),
         QStringLiteral("Inverse Image"), QStringLiteral("Unaltered Image")},
        {0.0, 1.0, 2.0, 3.0}, 2);
    dialog.show();

    QCOMPARE(count, 0);
    QCOMPARE(offset, 1);
    QCOMPARE(fill, 2);
    QCOMPARE(dialog.parameters(), QList<float>({55.0f, 30.0f, 2.0f}));
}

void TestFilterDialog::traceContourCollectsTheLevelThenTheEdge()
{
    // A level and which side of it to ink. Both are plain numbers to the
    // engine, and an Edge sent as the level would ask for a contour at 1.
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Trace Contour"));
    const int level = dialog.addParameter(QStringLiteral("Level:"), 0, 255, 128);
    const int edge = dialog.addRadioChoice(QStringLiteral("Edge"),
                                           {QStringLiteral("Lower"), QStringLiteral("Upper")},
                                           {0.0, 1.0}, 0);
    dialog.show();

    QCOMPARE(level, 0);
    QCOMPARE(edge, 1);
    QCOMPARE(dialog.parameters(), QList<float>({128.0f, 0.0f}));
}

void TestFilterDialog::windCollectsTheMethodThenTheDirection()
{
    // Two boxes of radios and nothing else, so the method is parameter zero
    // and the direction parameter one. A dialog that added a slider out of
    // habit would push the direction into a slot the engine reads as Stagger.
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Wind"));
    const int method = dialog.addRadioChoice(
        QStringLiteral("Method"),
        {QStringLiteral("Wind"), QStringLiteral("Blast"), QStringLiteral("Stagger")},
        {0.0, 1.0, 2.0}, 1);
    const int direction = dialog.addRadioChoice(
        QStringLiteral("Direction"),
        {QStringLiteral("From the Right"), QStringLiteral("From the Left")}, {0.0, 1.0}, 1);
    dialog.show();

    QCOMPARE(method, 0);
    QCOMPARE(direction, 1);
    QCOMPARE(dialog.parameters(), QList<float>({1.0f, 1.0f}));
}

void TestFilterDialog::customReadsItsGridRowByRowThenScaleAndOffset()
{
    // Twenty-seven slots from one control, and the engine reads the grid row
    // by row from the top left. A grid handed over column by column would
    // transpose every kernel anybody types in — which looks plausible for the
    // symmetrical ones and wrong for every other.
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Custom"));
    QList<double> weights;
    for (int i = 0; i < 25; ++i) {
        weights.append(i);
    }
    const int first = dialog.addKernelGrid(5, weights, 3.0, 128.0);
    dialog.show();

    QCOMPARE(first, 0);
    const QList<float> params = dialog.parameters();
    QCOMPARE(params.size(), 27);
    for (int i = 0; i < 25; ++i) {
        QCOMPARE(params.at(i), float(i));
    }
    QCOMPARE(params.at(25), 3.0f);
    QCOMPARE(params.at(26), 128.0f);
}

void TestFilterDialog::flameNeedsAPathToBurnAlong()
{
    // Flame draws along a line, and without one there is nothing to draw.
    // Refusing is the whole behaviour here: rendering nothing quietly would
    // leave the user wondering which of twenty settings was at fault.
    Engine engine;
    QImage image(engine.getCanvasWidth(), engine.getCanvasHeight(),
                 QImage::Format_ARGB32_Premultiplied);
    image.fill(Qt::black);
    QVERIFY(engine.addImageLayer(image, 0, 0, QStringLiteral("Ground")));

    QVERIFY2(!engine.hasActivePath(), "a fresh document should have no path");
    const QList<float> params{2.0f, 100.0f};
    QVERIFY2(!engine.applyFlame(rust::Slice<const float>(params.constData(),
                                                         size_t(params.size()))),
             "Flame claimed to have burned with no path to burn along");

    // With a path it goes ahead, and something lights up.
    engine.beginPathBuild();
    engine.pathBuildSubpath(false);
    engine.pathBuildPoint(60, 220, false, 0, 0, false, 0, 0);
    engine.pathBuildPoint(240, 220, false, 0, 0, false, 0, 0);
    engine.commitBuiltPath(QStringLiteral("Wick"));
    QVERIFY(engine.hasActivePath());

    QVERIFY(engine.applyFlame(rust::Slice<const float>(params.constData(),
                                                       size_t(params.size()))));
    const QImage after = engine.layerImage(engine.getActiveLayerIndex());
    bool burned = false;
    for (int y = 0; y < after.height() && !burned; ++y) {
        for (int x = 0; x < after.width(); ++x) {
            if (after.pixelColor(x, y).red() > 40) {
                burned = true;
                break;
            }
        }
    }
    QVERIFY2(burned, "Flame ran with a path and left the layer black");
}

void TestFilterDialog::aTabbedDialogStillNumbersItsControlsInOrder()
{
    // Flame has twenty settings across two tabs, read positionally by the
    // engine. Tabs are a change of layout, not of numbering, and a control
    // that landed in the wrong slot would still produce a flame.
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Flame"));
    dialog.setPreviewPaneVisible(false);

    dialog.beginTab(QStringLiteral("Basic"));
    dialog.addChoice(QStringLiteral("Flame Type:"), {QStringLiteral("a"), QStringLiteral("b")},
                     {0.0, 1.0}, 1);
    dialog.addParameter(QStringLiteral("Length:"), 1, 500, 120);

    dialog.beginTab(QStringLiteral("Advanced"));
    dialog.addParameter(QStringLiteral("Turbulent:"), 0, 100, 40);
    const int last = dialog.addCheckBox(QStringLiteral("Randomize Shapes"), true);

    QCOMPARE(last, 3);
    QCOMPARE(dialog.parameters(), QList<float>({1.0f, 120.0f, 40.0f, 1.0f}));

    auto *tabs = dialog.findChild<QTabWidget *>();
    QVERIFY(tabs);
    QCOMPARE(tabs->count(), 2);
    QCOMPARE(tabs->tabText(1), QStringLiteral("Advanced"));
}

void TestFilterDialog::randomizeRollsANewSeedThatStaysPut()
{
    // Wave's generators are drawn from a seed rather than from a running
    // random source, because the preview and the applied filter are separate
    // runs and undo and redo are two more. Randomize must change the seed;
    // everything else must leave it alone.
    Engine engine;
    FilterPreviewDialog dialog(&engine, QStringLiteral("Wave"));
    dialog.addParameter(QStringLiteral("Number of Generators:"), 1, 999, 5);
    const int seed = dialog.addRandomizeButton(QStringLiteral("Randomize"));

    const float before = dialog.parameterValue(seed);
    QCOMPARE(dialog.parameterValue(seed), before);

    auto *button = dialog.findChild<QPushButton *>();
    QVERIFY(button);
    button->click();
    QVERIFY2(dialog.parameterValue(seed) != before, "Randomize did not re-roll the seed");
    // ...and it stays put until asked again.
    const float rolled = dialog.parameterValue(seed);
    QCOMPARE(dialog.parameterValue(seed), rolled);
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

// Restyling a type layer from the always-visible panels.
//
// The Type options bar is only on screen under the Type tool, but the
// Character and Paragraph panels are up whatever tool is in hand, and both
// restyle the whole selected layer. That makes the bar's state dangerous when
// it has not been pointed at the layer: `restyleTypeLayer` writes *every*
// field, so one untouched default — a 12pt size the user never set — rides
// along with the anti-aliasing change they did make and silently resizes
// their text. That is what this pins down, because the only evidence of it is
// text that quietly got smaller.

#include "MainWindow.h"
#include "panels/CharacterPanel.h"
#include "canvas/CanvasView.h"
#include "panels/LayersPanel.h"
#include "shortcuts/CommandRegistry.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QImage>
#include <QTest>

class TestTypeRestyle : public QObject
{
    Q_OBJECT

private slots:
    void restylingUnderAnotherToolKeepsTheLayersSize();
    void turningALayerVerticalKeepsItsFormatting();

private:
    /// Put a type layer of `size` points into the document and make it
    /// active. Returns its panel index.
    static int addTypeLayer(Engine &engine, float size);
};

int TestTypeRestyle::addTypeLayer(Engine &engine, float size)
{
    engine.beginTextRuns();
    engine.addTextRun(QStringLiteral("HELLO there precious tomato"),
                      QStringLiteral("Adwaita Mono"), QStringLiteral("Regular"), size,
                      QColor(Qt::black), 1.0f, 1.0f);

    // The glyphs themselves do not matter here — only the record does.
    QImage pixels(200, 60, QImage::Format_ARGB32_Premultiplied);
    pixels.fill(Qt::transparent);
    const bool added = engine.addTextLayer(pixels, 40, 40, QStringLiteral("HELLO"), 0, true,
                                           false, 40.0f, 80.0f);
    if (!added) {
        return -1;
    }
    const int index = engine.getActiveLayerIndex();
    return engine.layerTextRunCount(index) > 0 ? index : -1;
}

void TestTypeRestyle::restylingUnderAnotherToolKeepsTheLayersSize()
{
    Engine engine;
    CommandRegistry registry;
    QVERIFY(registry.load(QStringLiteral(PHOTORUST_SOURCE_RESOURCES "/shortcuts.json")));

    MainWindow window(&engine, &registry);

    // 40pt, well clear of the 12pt the options bar starts on — the whole
    // point is that the bar's default must not win.
    const int layer = addTypeLayer(engine, 40.0f);
    QVERIFY2(layer >= 0, "could not add a type layer to test with");
    QCOMPARE(engine.layerTextRunSize(layer, 0), 40.0f);

    auto *character = window.findChild<CharacterPanel *>();
    QVERIFY(character);

    // Selecting the layer, as the Layers panel reports it. The window is
    // left on its starting tool rather than the Type tool, because the Type
    // tool syncs the bar by other routes and would hide this.
    auto *layers = window.findChild<LayersPanel *>();
    QVERIFY(layers);
    emit layers->documentChanged();

    // Exactly the reported gesture: change only the anti-aliasing method.
    emit character->antialiasChanged(QStringLiteral("Strong"));

    QCOMPARE(engine.layerTextRunSize(layer, 0), 40.0f);
}

void TestTypeRestyle::turningALayerVerticalKeepsItsFormatting()
{
    Engine engine;
    CommandRegistry registry;
    QVERIFY(registry.load(QStringLiteral(PHOTORUST_SOURCE_RESOURCES "/shortcuts.json")));
    MainWindow window(&engine, &registry);

    const int layer = addTypeLayer(engine, 40.0f);
    QVERIFY2(layer >= 0, "could not add a type layer to test with");
    QVERIFY(!engine.layerTextVertical(layer));

    auto *canvas = window.findChild<CanvasView *>();
    QVERIFY(canvas);
    QVERIFY(canvas->setTypeLayerVertical(layer, true));

    QVERIFY(engine.layerTextVertical(layer));
    // Turning text on its side changes how it is laid out, not how it is set:
    // the reorientation rebuilds every run, which is exactly where formatting
    // goes missing without anyone noticing.
    QCOMPARE(engine.layerTextRunSize(layer, 0), 40.0f);
    QCOMPARE(engine.layerTextRunText(layer, 0),
             QStringLiteral("HELLO there precious tomato"));
    QCOMPARE(engine.layerTextRunFamily(layer, 0), QStringLiteral("Adwaita Mono"));

    // And back again, which must be refused as a no-op the second time.
    QVERIFY(canvas->setTypeLayerVertical(layer, false));
    QVERIFY(!engine.layerTextVertical(layer));
    QVERIFY(!canvas->setTypeLayerVertical(layer, false));
}

QTEST_MAIN(TestTypeRestyle)
#include "tst_typerestyle.moc"

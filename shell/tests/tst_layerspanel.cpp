// The Layers panel's state rules.
//
// Every case here is a bug that reached the user once: the selection growing
// as a layer was arranged, a collapsed group's members reappearing, and the
// drop-target bands that decide "into this group" from "past it".
//
// The panel is driven through a real `Engine`, so these test the two halves
// together — which is where the mistakes have been, rather than in either
// alone.

#include "panels/LayersPanel.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QTest>
#include <QLineEdit>
#include <QTreeWidget>

class TestLayersPanel : public QObject
{
    Q_OBJECT

private slots:
    void selectionFollowsTheLayerNotTheRow();
    void aNewLayerIsTheOnlyOneSelected();
    void aMultiSelectionSurvivesARefresh();
    void findLayersMatchesNamesAndTheWordsInThem();
    void aClosedGroupHidesItsMembers();
    void dropBandsSplitAFolderRowIntoThree();
    void arrangeAgreesWithWhatItWillDo();

private:
    /// The tree inside the panel. Found rather than exposed: the panel has one
    /// and the test has no business being handed it.
    static QTreeWidget *treeOf(LayersPanel &panel)
    {
        return panel.findChild<QTreeWidget *>();
    }

    /// A document with `count` layers above the Background, topmost last.
    static void addLayers(Engine &engine, int count)
    {
        for (int i = 0; i < count; ++i) {
            engine.addLayer();
        }
    }
};

void TestLayersPanel::selectionFollowsTheLayerNotTheRow()
{
    Engine engine;
    addLayers(engine, 3);
    LayersPanel panel(&engine);
    QTreeWidget *tree = treeOf(panel);
    QVERIFY(tree);

    // Select one layer in the middle of the stack.
    const int row = 1;
    tree->setCurrentItem(tree->topLevelItem(row));
    const QString name = engine.layerName(row);
    QCOMPARE(tree->selectedItems().size(), 1);

    // Move it, three times. The panel rebuilds after each, and the selection
    // must arrive with the layer rather than be reapplied to the row it left —
    // which is how one selected layer used to become four.
    for (int i = 0; i < 3; ++i) {
        const int active = engine.getActiveLayerIndex();
        if (!engine.canArrangeLayer(active, /*Send Backward=*/2)) {
            break;
        }
        QVERIFY(engine.arrangeLayer(active, 2));
        panel.refresh();

        QCOMPARE(tree->selectedItems().size(), 1);
        QCOMPARE(tree->selectedItems().first()->text(0), name);
    }
}

void TestLayersPanel::aNewLayerIsTheOnlyOneSelected()
{
    Engine engine;
    addLayers(engine, 2);
    LayersPanel panel(&engine);
    QTreeWidget *tree = treeOf(panel);
    QVERIFY(tree);
    QCOMPARE(tree->selectedItems().size(), 1);
    const QString wasSelected = tree->selectedItems().first()->text(0);

    // The engine makes the new layer active, and the panel rebuilds. The
    // layer that was selected before must not still be ticked alongside it.
    engine.addLayer();
    panel.refresh();

    QCOMPARE(tree->selectedItems().size(), 1);
    const QString nowSelected = tree->selectedItems().first()->text(0);
    QVERIFY2(nowSelected != wasSelected,
             "the new layer should be selected, not the one that was before");
    QCOMPARE(nowSelected, engine.layerName(engine.getActiveLayerIndex()));
}

void TestLayersPanel::aMultiSelectionSurvivesARefresh()
{
    Engine engine;
    addLayers(engine, 3);
    LayersPanel panel(&engine);
    QTreeWidget *tree = treeOf(panel);
    QVERIFY(tree);

    // Two rows selected by hand, with the active layer among them — the case
    // the fix above must leave alone. The engine is told which one is active
    // because that is what clicking a row does, and it is precisely what
    // separates a selection the user built from a layer the engine activated
    // on its own.
    tree->clearSelection();
    tree->setCurrentItem(tree->topLevelItem(2));
    tree->topLevelItem(1)->setSelected(true);
    tree->topLevelItem(2)->setSelected(true);
    engine.setActiveLayer(2);
    QCOMPARE(tree->selectedItems().size(), 2);

    panel.refresh();
    QCOMPARE(tree->selectedItems().size(), 2);
}

void TestLayersPanel::findLayersMatchesNamesAndTheWordsInThem()
{
    Engine engine;
    addLayers(engine, 2);
    engine.setLayerName(engine.getActiveLayerIndex(), QStringLiteral("banana"));

    // A type layer left with its default name, so the only way to find it is
    // by what it says.
    engine.beginTextRuns();
    engine.addTextRun(QStringLiteral("apple pie"), QStringLiteral("Adwaita Mono"),
                      QStringLiteral("Regular"), 24.0f, QColor(Qt::black), 1.0f, 1.0f);
    QImage pixels(40, 20, QImage::Format_ARGB32_Premultiplied);
    pixels.fill(Qt::transparent);
    QVERIFY(engine.addTextLayer(pixels, 0, 0, QStringLiteral("Layer 9"), 0, true, false, 0.0f,
                                10.0f));

    LayersPanel panel(&engine);
    QTreeWidget *tree = treeOf(panel);
    QVERIFY(tree);
    panel.beginFindLayers();

    auto *field = panel.findChild<QLineEdit *>();
    QVERIFY2(field, "the Name filter has no search field");

    auto visibleRows = [&] {
        QStringList names;
        for (int row = 0; row < tree->topLevelItemCount(); ++row) {
            if (!tree->topLevelItem(row)->isHidden()) {
                names << tree->topLevelItem(row)->text(0);
            }
        }
        return names;
    };

    const int total = tree->topLevelItemCount();
    QVERIFY(total >= 4);
    // An empty search hides nothing, so opening the field is not destructive.
    QCOMPARE(visibleRows().size(), total);

    // By name.
    field->setText(QStringLiteral("bana"));
    QCOMPARE(visibleRows(), QStringList{QStringLiteral("banana")});

    // By the words in a type layer, which is named nothing like them.
    field->setText(QStringLiteral("apple"));
    QCOMPARE(visibleRows(), QStringList{QStringLiteral("Layer 9")});

    // And a search matching neither leaves the list empty rather than full.
    field->setText(QStringLiteral("zzzz"));
    QVERIFY(visibleRows().isEmpty());
}

void TestLayersPanel::aClosedGroupHidesItsMembers()
{
    Engine engine;
    addLayers(engine, 2);
    LayersPanel panel(&engine);
    QTreeWidget *tree = treeOf(panel);
    QVERIFY(tree);

    // Group the two layers above the Background. Panel indices run top-first,
    // so those are rows 0 and 1.
    QVector<int> selection{0, 1};
    QVERIFY(engine.groupLayers(selection));
    panel.refresh();

    // Folder on top, its two members beneath it, then the Background.
    QCOMPARE(tree->topLevelItemCount(), 4);
    QVERIFY(engine.layerIsGroup(0));

    engine.setLayerGroupExpanded(0, false);
    panel.refresh();
    QVERIFY2(!tree->topLevelItem(0)->isHidden(), "the folder itself stays visible");
    QVERIFY2(tree->topLevelItem(1)->isHidden(), "a member of a closed group is hidden");
    QVERIFY2(tree->topLevelItem(2)->isHidden(), "and so is the other one");
    QVERIFY2(!tree->topLevelItem(3)->isHidden(), "the Background is not in the group");

    // Opening it brings them back. This is the half that broke: two places
    // were setting `hidden`, and the filter ran last.
    engine.setLayerGroupExpanded(0, true);
    panel.refresh();
    for (int row = 0; row < tree->topLevelItemCount(); ++row) {
        QVERIFY2(!tree->topLevelItem(row)->isHidden(), qPrintable(QString::number(row)));
    }
}

void TestLayersPanel::dropBandsSplitAFolderRowIntoThree()
{
    // A 40px row: the middle means "into this group", the edges mean above and
    // below it, which is how a layer gets past a folder instead of inside it.
    const QRect row(0, 100, 200, 40);

    QVERIFY(dropsIntoGroupRow(row, QPoint(50, 120)));
    QVERIFY(!dropsIntoGroupRow(row, QPoint(50, 101)));
    QVERIFY(!dropsIntoGroupRow(row, QPoint(50, 138)));
    // Exactly on the band boundaries, the middle wins — the bands are the
    // outer 9px, not 9px plus a pixel of slop.
    QVERIFY(dropsIntoGroupRow(row, QPoint(50, 109)));
    QVERIFY(dropsIntoGroupRow(row, QPoint(50, 130)));

    // A row too short to have a middle is all edge.
    QVERIFY(!dropsIntoGroupRow(QRect(0, 0, 200, 12), QPoint(50, 6)));
}

void TestLayersPanel::arrangeAgreesWithWhatItWillDo()
{
    Engine engine;
    addLayers(engine, 2);

    // Whatever the menu is told about an entry has to be what the command
    // does, or an enabled entry does nothing when clicked.
    for (int index = 0; index < engine.getLayerCount(); ++index) {
        engine.setActiveLayer(index);
        for (int op = 0; op < 4; ++op) {
            const bool promised = engine.canArrangeLayer(index, op);
            const bool happened = engine.arrangeLayer(index, op);
            QCOMPARE(happened, promised);
            if (happened) {
                // Put it back, so the next question is asked of the same stack.
                engine.undo();
            }
        }
    }
}

QTEST_MAIN(TestLayersPanel)
#include "tst_layerspanel.moc"

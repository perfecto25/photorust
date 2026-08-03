#include "MainWindow.h"

#include "canvas/CanvasView.h"
#include "panels/ColorPanel.h"
#include "panels/HistoryPanel.h"
#include "panels/InfoPanel.h"
#include "panels/LayersPanel.h"
#include "panels/PanelHeader.h"
#include "shortcuts/CommandRegistry.h"
#include "tools/ToolIcons.h"
#include "tools/ToolStrip.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QApplication>
#include <QButtonGroup>
#include <QCheckBox>
#include <QCloseEvent>
#include <QComboBox>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDockWidget>
#include <QDoubleSpinBox>
#include <QFileDialog>
#include <QFileInfo>
#include <QFormLayout>
#include <QInputDialog>
#include <QLabel>
#include <QMenu>
#include <QMenuBar>
#include <QMessageBox>
#include <QSpinBox>
#include <QStatusBar>
#include <QToolBar>
#include <QToolButton>
#include <QVBoxLayout>

namespace {

/// Formats accepted by File ▸ Open. PSD is ours; the rest go through Qt's
/// image plugins.
const char *const kOpenFilter =
    "All Supported Formats (*.psd *.png *.jpg *.jpeg *.tif *.tiff *.bmp *.webp);;"
    "Photoshop (*.psd);;"
    "PNG (*.png);;"
    "JPEG (*.jpg *.jpeg);;"
    "TIFF (*.tif *.tiff);;"
    "All Files (*)";

const char *const kSaveFilter =
    "Photoshop (*.psd);;"
    "PNG (*.png);;"
    "JPEG (*.jpg *.jpeg);;"
    "TIFF (*.tif *.tiff);;"
    "All Files (*)";

/// Line-art tint for options-bar icons, matching the tool strip.
const QColor kOptionsIconColor(0xd4, 0xd4, 0xd4);

} // namespace

MainWindow::MainWindow(Engine *engine, CommandRegistry *registry, QWidget *parent)
    : QMainWindow(parent)
    , m_engine(engine)
    , m_registry(registry)
{
    setWindowTitle(tr("PhotoRust"));
    resize(1400, 900);
    // Photoshop lets panels stack into tabbed groups and nest side by side.
    setDockOptions(QMainWindow::AnimatedDocks | QMainWindow::AllowNestedDocks
                   | QMainWindow::AllowTabbedDocks);
    setTabPosition(Qt::AllDockWidgetAreas, QTabWidget::North);

    m_canvas = new CanvasView(m_engine, this);
    setCentralWidget(m_canvas);

    createToolPanel();

    createOptionsBar();
    createMenus();
    createDocks();
    createStatusBar();
    connectEngine();
    // Must run after createMenus(), so the tool commands the strip registered
    // and the menu commands are all present in the registry.
    installShortcuts();

    connect(m_toolStrip, &ToolStrip::toolChanged, this, &MainWindow::onToolChanged);
    connect(m_canvas, &CanvasView::cursorMoved, this, &MainWindow::onCursorMoved);
    connect(m_canvas, &CanvasView::cursorMoved, m_infoPanel, &InfoPanel::setCursorPosition);
    connect(m_canvas, &CanvasView::cursorLeft, m_infoPanel, &InfoPanel::clearCursorPosition);
    connect(m_canvas, &CanvasView::zoomChanged, this, &MainWindow::onZoomChanged);
    connect(m_canvas, &CanvasView::contextMenuRequested, this,
            &MainWindow::showSelectionContextMenu);
    connect(m_canvas, &CanvasView::noteEditRequested, this, &MainWindow::editNote);
    connect(m_canvas, &CanvasView::statusMessage, this, [this](const QString &text) {
        statusBar()->showMessage(text, 4000);
    });
    connect(m_canvas, &CanvasView::healingSourceRequired, this,
            &MainWindow::warnHealingSourceRequired);
    connect(m_canvas, &CanvasView::colorPicked, this, [this](const QColor &c) {
        m_colorPanel->setForegroundColor(c);
        m_toolStrip->swatches()->setForeground(c);
    });

    // Keep the tool strip's swatch and the Color panel showing the same pair.
    connect(m_toolStrip->swatches(), &ColorSwatchWidget::foregroundChanged,
            m_colorPanel, &ColorPanel::setForegroundColor);
    connect(m_colorPanel, &ColorPanel::foregroundChanged, this, [this](const QColor &c) {
        m_toolStrip->swatches()->setForeground(c);
    });

    onToolChanged(ToolId::Brush, 0);
    updateWindowTitle();
    // Wait for the first layout pass so the canvas knows its real size.
    QMetaObject::invokeMethod(this, &MainWindow::fitOnScreen, Qt::QueuedConnection);
}

// ------------------------------------------------------------------- menus --

template <typename Slot>
QAction *MainWindow::command(const QString &id, const QString &text, Slot slot)
{
    // The registry already knows this command from shortcuts.json; registering
    // again just attaches the display text and returns the same QAction.
    QAction *action = m_registry->registerCommand(id, text);
    connect(action, &QAction::triggered, this, slot);
    // Actions must be children of the window to fire as window shortcuts.
    addAction(action);
    return action;
}

void MainWindow::createMenus()
{
    // -- File ---------------------------------------------------------------
    QMenu *file = menuBar()->addMenu(tr("&File"));
    file->addAction(command(QStringLiteral("file.new"), tr("&New..."),
                            &MainWindow::newDocument));
    file->addAction(command(QStringLiteral("file.open"), tr("&Open..."),
                            &MainWindow::openDocument));
    file->addSeparator();
    file->addAction(command(QStringLiteral("file.save"), tr("&Save"),
                            [this] { saveDocument(); }));
    file->addAction(command(QStringLiteral("file.saveAs"), tr("Save &As..."),
                            [this] { saveDocumentAs(); }));
    file->addAction(command(QStringLiteral("file.saveSlices"), tr("Save S&lices..."),
                            &MainWindow::exportSlices));
    file->addSeparator();
    file->addAction(command(QStringLiteral("file.exit"), tr("E&xit"),
                            [this] { close(); }));

    // -- Edit ---------------------------------------------------------------
    QMenu *edit = menuBar()->addMenu(tr("&Edit"));
    edit->addAction(command(QStringLiteral("edit.undo"), tr("&Undo"), &MainWindow::undo));
    edit->addAction(command(QStringLiteral("edit.stepForward"), tr("Step &Forward"),
                            &MainWindow::redo));
    edit->addAction(command(QStringLiteral("edit.stepBackward"), tr("Step &Backward"),
                            &MainWindow::undo));
    edit->addSeparator();
    edit->addAction(command(QStringLiteral("edit.fillForeground"),
                            tr("Fill with Foreground Color"),
                            &MainWindow::fillWithForeground));
    edit->addAction(command(QStringLiteral("edit.fillBackground"),
                            tr("Fill with Background Color"),
                            &MainWindow::fillWithBackground));

    // -- Image --------------------------------------------------------------
    QMenu *image = menuBar()->addMenu(tr("&Image"));
    QMenu *adjustments = image->addMenu(tr("&Adjustments"));

    // Each entry names an adjustment the engine already knows; the string is
    // the contract between the two sides.
    struct AdjustmentEntry {
        const char *commandId;
        const char *engineName;
    };
    const AdjustmentEntry adjustmentEntries[] = {
        {"image.levels", "Levels"},
        {"image.hueSaturation", "Hue/Saturation"},
        {"image.colorBalance", "Color Balance"},
        {"image.blackAndWhite", "Black & White"},
        {"image.invert", "Invert"},
        {"image.desaturate", "Black & White"},
    };
    for (const auto &entry : adjustmentEntries) {
        const QString engineName = QString::fromUtf8(entry.engineName);
        adjustments->addAction(
            command(QLatin1String(entry.commandId), engineName,
                    [this, engineName] { applyAdjustment(engineName); }));
    }
    adjustments->addSeparator();
    for (const char *name : {"Posterize", "Threshold", "Brightness/Contrast", "Exposure"}) {
        const QString engineName = QString::fromUtf8(name);
        auto *action = new QAction(engineName, this);
        connect(action, &QAction::triggered, this,
                [this, engineName] { applyAdjustment(engineName); });
        adjustments->addAction(action);
    }

    image->addSeparator();
    image->addAction(command(QStringLiteral("image.canvasSize"), tr("&Canvas Size..."),
                             &MainWindow::showCanvasSize));

    // -- Layer --------------------------------------------------------------
    QMenu *layer = menuBar()->addMenu(tr("&Layer"));
    layer->addAction(command(QStringLiteral("layer.new"), tr("&New Layer"), [this] {
        m_engine->addLayer();
        refreshAll();
    }));
    layer->addAction(command(QStringLiteral("layer.newViaCopy"), tr("Layer via &Copy"),
                             [this] {
                                 m_engine->duplicateLayer(m_engine->getActiveLayerIndex());
                                 refreshAll();
                             }));
    layer->addSeparator();
    layer->addAction(command(QStringLiteral("layer.createClippingMask"),
                             tr("Create &Clipping Mask"), [this] {
                                 const int index = m_engine->getActiveLayerIndex();
                                 m_engine->setLayerClipping(index,
                                                            !m_engine->layerIsClipping(index));
                                 refreshAll();
                             }));
    layer->addSeparator();
    layer->addAction(command(QStringLiteral("layer.mergeDown"), tr("&Merge Down"), [this] {
        m_engine->mergeLayerDown(m_engine->getActiveLayerIndex());
        refreshAll();
    }));
    layer->addAction(command(QStringLiteral("layer.mergeVisible"), tr("&Flatten Image"),
                             [this] {
                                 m_engine->flattenImage();
                                 refreshAll();
                             }));
    layer->addSeparator();
    layer->addAction(command(QStringLiteral("layer.delete"), tr("&Delete Layer"), [this] {
        m_engine->deleteLayer(m_engine->getActiveLayerIndex());
        refreshAll();
    }));

    // -- Select -------------------------------------------------------------
    QMenu *select = menuBar()->addMenu(tr("&Select"));
    select->addAction(command(QStringLiteral("select.all"), tr("&All"), [this] {
        m_engine->selectAll();
        m_canvas->update();
    }));
    select->addAction(command(QStringLiteral("select.deselect"), tr("&Deselect"), [this] {
        m_engine->deselect();
        m_canvas->update();
    }));
    select->addAction(command(QStringLiteral("select.inverse"), tr("&Inverse"), [this] {
        m_engine->invertSelection();
        m_canvas->update();
    }));
    select->addSeparator();
    select->addAction(command(QStringLiteral("select.feather"), tr("&Feather..."), [this] {
        bool ok = false;
        const int radius = QInputDialog::getInt(this, tr("Feather Selection"),
                                                tr("Feather Radius (pixels):"), 5, 0, 250,
                                                1, &ok);
        if (ok) {
            m_engine->featherSelection(radius);
            m_canvas->update();
        }
    }));

    // -- Filter -------------------------------------------------------------
    QMenu *filter = menuBar()->addMenu(tr("Fi&lter"));
    QMenu *blur = filter->addMenu(tr("&Blur"));
    blur->addAction(command(QStringLiteral("filter.gaussianBlur"), tr("&Gaussian Blur..."),
                            [this] { applyFilter(QStringLiteral("Gaussian Blur")); }));
    QMenu *sharpen = filter->addMenu(tr("&Sharpen"));
    sharpen->addAction(command(QStringLiteral("filter.sharpen"), tr("&Sharpen"),
                               [this] { applyFilter(QStringLiteral("Sharpen")); }));
    sharpen->addAction(command(QStringLiteral("filter.unsharpMask"), tr("&Unsharp Mask..."),
                               [this] { applyFilter(QStringLiteral("Unsharp Mask")); }));
    QMenu *noise = filter->addMenu(tr("&Noise"));
    noise->addAction(command(QStringLiteral("filter.addNoise"), tr("&Add Noise..."),
                             [this] { applyFilter(QStringLiteral("Add Noise")); }));

    // -- View ---------------------------------------------------------------
    QMenu *view = menuBar()->addMenu(tr("&View"));
    view->addAction(command(QStringLiteral("view.zoomIn"), tr("Zoom &In"),
                            &MainWindow::zoomIn));
    view->addAction(command(QStringLiteral("view.zoomOut"), tr("Zoom &Out"),
                            &MainWindow::zoomOut));
    view->addAction(command(QStringLiteral("view.fitOnScreen"), tr("&Fit on Screen"),
                            &MainWindow::fitOnScreen));
    view->addAction(command(QStringLiteral("view.actualPixels"), tr("&Actual Pixels"),
                            &MainWindow::actualPixels));

    // -- Window (populated with the dock toggles in createDocks) ------------
    menuBar()->addMenu(tr("&Window"))->setObjectName(QStringLiteral("windowMenu"));

    // -- Help ---------------------------------------------------------------
    QMenu *help = menuBar()->addMenu(tr("&Help"));
    auto *about = new QAction(tr("&About PhotoRust"), this);
    connect(about, &QAction::triggered, this, [this] {
        QMessageBox::about(this, tr("About PhotoRust"),
                           tr("<h3>PhotoRust</h3>"
                              "<p>A Photoshop CS6 clone.</p>"
                              "<p>Qt %1 shell over a Rust image engine.</p>")
                               .arg(QLatin1String(qVersion())));
    });
    help->addAction(about);

    // Colour commands live in the keymap but have no menu home in CS6 either;
    // register them so D and X work.
    addAction(command(QStringLiteral("tool.defaultColors"), tr("Default Colors"), [this] {
        m_engine->resetColors();
        m_toolStrip->swatches()->reset();
    }));
    addAction(command(QStringLiteral("tool.swapColors"), tr("Swap Colors"), [this] {
        m_engine->swapColors();
        m_toolStrip->swatches()->swap();
    }));
}

// ------------------------------------------------------------ options bar ---

void MainWindow::installShortcuts()
{
    if (!m_registry) {
        return;
    }
    // Adopt every registered command, not just the ones that reached a menu.
    // Tool commands are registered by the ToolStrip and would otherwise never
    // belong to a widget, leaving their shortcuts dead.
    for (const QString &id : m_registry->commandIds()) {
        QAction *action = m_registry->action(id);
        if (action && !actions().contains(action)) {
            addAction(action);
        }
    }

    // Shift+letter cycles within a tool group, as CS6 does with its "Use Shift
    // Key for Tool Switch" preference on by default. Registered here rather
    // than in the strip so they land on the window with the rest.
    struct Cycle {
        const char *id;
        QString text;
        QString key;
        ToolId tool;
    };
    const Cycle cycles[] = {
        {"tool.marquee.cycle", tr("Cycle Marquee Tool"), QStringLiteral("Shift+M"),
         ToolId::Marquee},
        {"tool.lasso.cycle", tr("Cycle Lasso Tool"), QStringLiteral("Shift+L"),
         ToolId::Lasso},
        {"tool.quickselect.cycle", tr("Cycle Quick Selection Tool"),
         QStringLiteral("Shift+W"), ToolId::QuickSelect},
        {"tool.crop.cycle", tr("Cycle Crop Tool"), QStringLiteral("Shift+C"),
         ToolId::Crop},
        {"tool.eyedropper.cycle", tr("Cycle Eyedropper Tool"), QStringLiteral("Shift+I"),
         ToolId::Eyedropper},
        {"tool.healing.cycle", tr("Cycle Healing Tool"), QStringLiteral("Shift+J"),
         ToolId::Healing},
    };

    for (const Cycle &entry : cycles) {
        QAction *cycle = m_registry->registerCommand(QString::fromUtf8(entry.id), entry.text,
                                                     QKeySequence(entry.key));
        const ToolId tool = entry.tool;
        connect(cycle, &QAction::triggered, this,
                [this, tool] { m_toolStrip->cycleVariant(tool); });
        if (!actions().contains(cycle)) {
            addAction(cycle);
        }
    }
}

void MainWindow::showSelectionContextMenu(const QPoint &globalPos)
{
    // CS6's marquee right-click menu, in its order and grouping. Entries the
    // engine cannot do yet are listed and disabled rather than omitted, the
    // same way the tool flyouts list their unimplemented variants — the menu
    // keeps CS6's shape and nothing silently does nothing.
    struct Entry {
        const char *commandId; ///< Registry id, or nullptr for a separator.
        const char *text;      ///< Label, for entries with no command yet.
        bool implemented;
    };
    const Entry entries[] = {
        {"select.deselect", "Deselect", true},
        {"select.inverse", "Select Inverse", true},
        {"select.feather", "Feather...", true},
        {"select.refineEdge", "Refine Edge...", false},
        {nullptr, nullptr, false},
        {nullptr, "Save Selection...", false},
        {nullptr, "Make Work Path...", false},
        {nullptr, nullptr, false},
        {"layer.newViaCopy", "Layer Via Copy", true},
        {"layer.newViaCut", "Layer Via Cut", false},
        {"layer.new", "New Layer...", true},
        {nullptr, nullptr, false},
        {"edit.freeTransform", "Free Transform", false},
        {nullptr, "Transform Selection", false},
        {nullptr, nullptr, false},
        {"edit.fill", "Fill...", false},
        {nullptr, "Stroke...", false},
        {nullptr, nullptr, false},
        {"filter.last", "Last Filter", false},
        {"edit.fade", "Fade...", false},
    };

    QMenu menu(this);
    menu.setObjectName(QStringLiteral("canvasContextMenu"));
    // So the disabled entries can say why they are disabled.
    menu.setToolTipsVisible(true);

    for (const Entry &entry : entries) {
        if (!entry.commandId && !entry.text) {
            menu.addSeparator();
            continue;
        }

        // An implemented entry reuses the registry's action, so the menu shows
        // the same shortcut and runs the same handler as the menu bar.
        if (entry.implemented && entry.commandId) {
            if (QAction *action = m_registry->action(QLatin1String(entry.commandId))) {
                menu.addAction(action);
                continue;
            }
        }

        // Not tr(): the label comes from the table above, not a literal, so
        // there is nothing for lupdate to collect here.
        QAction *placeholder = menu.addAction(QString::fromUtf8(entry.text));
        placeholder->setEnabled(false);
        placeholder->setToolTip(tr("Not implemented yet"));
    }

    menu.exec(globalPos);
}

void MainWindow::createToolPanel()
{
    m_toolStrip = new ToolStrip(m_registry, this);

    // CS6's Tools panel: dragged by its header, dockable on either side,
    // floatable and closable. A QDockWidget with a PanelHeader for its title
    // bar gives all four; a QToolBar could not, because Qt stops painting a
    // toolbar's drag grip once it floats, stranding the panel.
    m_toolsDock = new QDockWidget(tr("Tools"), this);
    m_toolsDock->setObjectName(QStringLiteral("toolsDock"));
    m_toolsDock->setAllowedAreas(Qt::LeftDockWidgetArea | Qt::RightDockWidgetArea);
    m_toolsDock->setFeatures(QDockWidget::DockWidgetMovable
                             | QDockWidget::DockWidgetFloatable
                             | QDockWidget::DockWidgetClosable);

    auto *header = new PanelHeader(m_toolsDock);
    m_toolsDock->setTitleBarWidget(header);
    m_toolsDock->setWidget(m_toolStrip);
    addDockWidget(Qt::LeftDockWidgetArea, m_toolsDock);

    connect(header, &PanelHeader::closeClicked, m_toolsDock, &QWidget::close);
    connect(header, &PanelHeader::collapseClicked, this, [this] {
        m_toolStrip->setColumnCount(m_toolStrip->columnCount() == 1 ? 2 : 1);
    });
    connect(m_toolStrip, &ToolStrip::columnCountChanged, this,
            [this, header](int columns) {
                header->setCollapsePointsLeft(columns == 2);
                // The dock area caches its extent, so ask for the new one.
                resizeDocks({m_toolsDock}, {m_toolStrip->sizeHint().width()},
                            Qt::Horizontal);
            });
}

void MainWindow::createOptionsBar()
{
    m_optionsBar = new QToolBar(tr("Options"), this);
    m_optionsBar->setObjectName(QStringLiteral("optionsBar"));
    m_optionsBar->setMovable(false);
    m_optionsBar->setFloatable(false);
    addToolBar(Qt::TopToolBarArea, m_optionsBar);
}

void MainWindow::populateOptionsBar(ToolId tool, int variant)
{
    m_optionsBar->clear();
    // These point into the widgets we just deleted.
    m_brushSize = nullptr;
    m_brushHardness = nullptr;
    m_brushOpacity = nullptr;
    m_brushFlow = nullptr;

    // Name the active variant, so switching to Elliptical says so.
    auto *label = new QLabel(QStringLiteral("  %1  ").arg(toolVariantName(tool, variant)),
                             m_optionsBar);
    QFont bold = label->font();
    bold.setBold(true);
    label->setFont(bold);
    m_optionsBar->addWidget(label);
    m_optionsBar->addSeparator();

    // The healing group is mostly brush-driven, but Patch, Content-Aware Move
    // and Red Eye are not — they have no brush at all, so they must not take the
    // Size/Hardness/Opacity/Flow branch below.
    const bool paintsWithBrush = toolPaints(tool)
        && (!toolHeals(tool) || healingIsBrush(static_cast<HealingType>(variant)));

    if (paintsWithBrush) {
        m_optionsBar->addWidget(new QLabel(tr("Size:"), m_optionsBar));
        m_brushSize = new QDoubleSpinBox(m_optionsBar);
        m_brushSize->setRange(1.0, 5000.0);
        m_brushSize->setValue(20.0);
        m_brushSize->setSuffix(tr(" px"));
        m_brushSize->setFixedWidth(80);
        m_optionsBar->addWidget(m_brushSize);

        m_optionsBar->addWidget(new QLabel(tr("Hardness:"), m_optionsBar));
        m_brushHardness = new QSpinBox(m_optionsBar);
        m_brushHardness->setRange(0, 100);
        m_brushHardness->setValue(100);
        m_brushHardness->setSuffix(QStringLiteral("%"));
        m_brushHardness->setFixedWidth(64);
        m_optionsBar->addWidget(m_brushHardness);

        m_optionsBar->addSeparator();

        m_optionsBar->addWidget(new QLabel(tr("Opacity:"), m_optionsBar));
        m_brushOpacity = new QSpinBox(m_optionsBar);
        m_brushOpacity->setRange(0, 100);
        m_brushOpacity->setValue(100);
        m_brushOpacity->setSuffix(QStringLiteral("%"));
        m_brushOpacity->setFixedWidth(64);
        m_optionsBar->addWidget(m_brushOpacity);

        m_optionsBar->addWidget(new QLabel(tr("Flow:"), m_optionsBar));
        m_brushFlow = new QSpinBox(m_optionsBar);
        m_brushFlow->setRange(1, 100);
        m_brushFlow->setValue(100);
        m_brushFlow->setSuffix(QStringLiteral("%"));
        m_brushFlow->setFixedWidth(64);
        m_optionsBar->addWidget(m_brushFlow);

        connect(m_brushSize, &QDoubleSpinBox::valueChanged, this,
                &MainWindow::pushBrushSettings);
        connect(m_brushHardness, &QSpinBox::valueChanged, this,
                &MainWindow::pushBrushSettings);
        connect(m_brushOpacity, &QSpinBox::valueChanged, this,
                &MainWindow::pushBrushSettings);
        connect(m_brushFlow, &QSpinBox::valueChanged, this,
                &MainWindow::pushBrushSettings);

        pushBrushSettings();

        // The Spot Healing Brush adds CS6's Type buttons after the brush
        // controls. Opacity and Flow have no meaning for it — the region is
        // rebuilt, not painted — so they are hidden rather than left there
        // doing nothing.
        if (toolHeals(tool)) {
            m_brushOpacity->setEnabled(false);
            m_brushOpacity->setToolTip(tr("Not used when healing"));
            m_brushFlow->setEnabled(false);
            m_brushFlow->setToolTip(tr("Not used when healing"));

            m_optionsBar->addSeparator();
            const auto healing = static_cast<HealingType>(variant);
            if (healing == HealingType::SpotHealing) {
                // Only the Spot Healing Brush chooses how to reconstruct; the
                // Healing Brush is told where to sample from instead.
                addHealTypeButtons();
            } else {
                m_optionsBar->addWidget(new QLabel(
                    tr("Alt+click to define a source point, then drag to repair"),
                    m_optionsBar));
            }
        }
    } else if (toolSelects(tool)) {
        addSelectionModeButtons();

        // Feather, as CS6 places it: straight after the combine buttons. The
        // value applies to selections made from now on, not to the current
        // one — Select ▸ Feather is what softens an existing selection.
        //
        // CS6 gives it to the marquee and lasso families only; the Quick
        // Selection button's two tools have no Feather field, so neither do
        // we. The stored radius still applies to them, which is why the canvas
        // is told either way.
        if (tool != ToolId::QuickSelect) {
            m_optionsBar->addWidget(new QLabel(tr("Feather:"), m_optionsBar));
            auto *feather = new QSpinBox(m_optionsBar);
            feather->setRange(0, 1000);
            feather->setValue(m_featherRadius);
            feather->setSuffix(tr(" px"));
            feather->setFixedWidth(72);
            feather->setToolTip(tr("Soften the edge of new selections"));
            m_optionsBar->addWidget(feather);
            connect(feather, &QSpinBox::valueChanged, this, [this](int value) {
                m_featherRadius = value;
                m_canvas->setFeatherRadius(value);
            });
            m_optionsBar->addSeparator();
        }
        m_canvas->setFeatherRadius(m_featherRadius);

        const bool lineSelect = tool == ToolId::Marquee
            && (static_cast<MarqueeType>(variant) == MarqueeType::SingleRow
                || static_cast<MarqueeType>(variant) == MarqueeType::SingleColumn);
        const LassoType lasso = static_cast<LassoType>(variant);
        const QuickSelectType quick = static_cast<QuickSelectType>(variant);

        // The Magnetic Lasso's own three controls, in CS6's order. They tune
        // the edge search, so they only appear for that variant.
        if (tool == ToolId::Lasso && lasso == LassoType::Magnetic) {
            addMagneticOptions();
            m_optionsBar->addSeparator();
        } else if (tool == ToolId::QuickSelect) {
            addQuickSelectOptions(quick);
            m_optionsBar->addSeparator();
        }

        QString hint;
        if (lineSelect) {
            hint = tr("Click to select a line    Ctrl+Shift = add    Ctrl+Alt = subtract");
        } else if (tool == ToolId::Lasso && lasso != LassoType::Freehand) {
            hint = tr("Click to place points    Double-click or Enter to close    "
                      "Backspace undoes one    Esc cancels");
        } else if (tool == ToolId::QuickSelect && quick == QuickSelectType::MagicWand) {
            hint = tr("Click to select a matching area    Ctrl+Shift = add    "
                      "Ctrl+Alt = subtract");
        } else if (tool == ToolId::QuickSelect) {
            hint = tr("Drag to grow the selection    Ctrl+Shift = add    "
                      "Ctrl+Alt = subtract");
        } else {
            hint = tr("Ctrl+Shift = add    Ctrl+Alt = subtract    Click = deselect");
        }
        m_optionsBar->addWidget(new QLabel(hint, m_optionsBar));
    } else if (tool == ToolId::Healing) {
        addHealingRegionOptions(static_cast<HealingType>(variant));
    } else if (tool == ToolId::Crop) {
        addCropOptions(static_cast<CropType>(variant));
    } else if (tool == ToolId::Eyedropper
               && static_cast<EyedropperType>(variant) != EyedropperType::Eyedropper) {
        addAnnotationOptions(static_cast<EyedropperType>(variant));
    } else if (tool == ToolId::Zoom) {
        m_optionsBar->addWidget(
            new QLabel(tr("Click to zoom in    Alt+click to zoom out"), m_optionsBar));
    } else if (tool == ToolId::Move) {
        m_optionsBar->addWidget(
            new QLabel(tr("Drag to move the active layer    Arrow keys nudge"),
                       m_optionsBar));
    }

    m_optionsBar->addSeparator();
    auto *spacer = new QWidget(m_optionsBar);
    spacer->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Preferred);
    m_optionsBar->addWidget(spacer);
}

void MainWindow::addSelectionModeButtons()
{
    // CS6 opens the selection tools' options bar with these four buttons,
    // before Feather and the rest. They are a radio set: the chosen mode
    // persists across tool switches, and holding a modifier overrides it for
    // one drag without moving the checked button (see
    // CanvasView::effectiveSelectionMode).
    struct Entry {
        SelectionMode mode;
        QString hint;
    };
    const Entry entries[] = {
        {SelectionMode::New, QString()},
        {SelectionMode::Add, tr("Ctrl+Shift-drag")},
        {SelectionMode::Subtract, tr("Ctrl+Alt-drag")},
        {SelectionMode::Intersect, tr("Ctrl+Shift+Alt-drag")},
    };

    auto *group = new QButtonGroup(m_optionsBar);
    group->setExclusive(true);

    for (const Entry &entry : entries) {
        auto *button = new QToolButton(m_optionsBar);
        button->setCheckable(true);
        button->setAutoRaise(true);
        button->setIcon(ToolIcons::fromSvgBody(ToolIcons::selectionModeSvg(entry.mode),
                                               kOptionsIconColor));
        button->setIconSize(QSize(20, 20));
        button->setChecked(entry.mode == m_selectionMode);

        const QString name = selectionModeName(entry.mode);
        button->setToolTip(entry.hint.isEmpty()
                               ? name
                               : QStringLiteral("%1 (%2)").arg(name, entry.hint));
        button->setStatusTip(button->toolTip());

        group->addButton(button, static_cast<int>(entry.mode));
        m_optionsBar->addWidget(button);
    }

    connect(group, &QButtonGroup::idClicked, this, [this](int id) {
        m_selectionMode = static_cast<SelectionMode>(id);
        m_canvas->setSelectionMode(m_selectionMode);
    });

    // Keep the canvas in step even if the bar was rebuilt for another tool.
    m_canvas->setSelectionMode(m_selectionMode);
    m_optionsBar->addSeparator();
}

void MainWindow::addMagneticOptions()
{
    // Width, Contrast and Frequency, as CS6 labels and orders them. The values
    // live on MainWindow rather than in the widgets so they survive the
    // options bar being rebuilt on every tool change.
    struct Entry {
        QString label;
        int min;
        int max;
        int *value;
        QString suffix;
        QString tip;
    };
    const Entry entries[] = {
        {tr("Width:"), 1, 256, &m_magneticWidth, tr(" px"),
         tr("How far either side of the cursor an edge is looked for")},
        {tr("Contrast:"), 1, 100, &m_magneticContrast, QStringLiteral("%"),
         tr("How strong a gradient has to be to count as an edge")},
        {tr("Frequency:"), 0, 100, &m_magneticFrequency, QString(),
         tr("How often a fastening point is dropped automatically")},
    };

    for (const Entry &entry : entries) {
        m_optionsBar->addWidget(new QLabel(entry.label, m_optionsBar));
        auto *spin = new QSpinBox(m_optionsBar);
        spin->setRange(entry.min, entry.max);
        spin->setValue(*entry.value);
        spin->setSuffix(entry.suffix);
        spin->setFixedWidth(72);
        spin->setToolTip(entry.tip);
        spin->setStatusTip(entry.tip);
        m_optionsBar->addWidget(spin);

        int *slot = entry.value;
        connect(spin, &QSpinBox::valueChanged, this, [this, slot](int value) {
            *slot = value;
            pushMagneticOptions();
        });
    }

    pushMagneticOptions();
}

QString MainWindow::infoHintFor(EyedropperType type) const
{
    // CS6 changes the Info panel's footer per tool, and it is the only place
    // the modifier hints for these tools appear.
    switch (type) {
    case EyedropperType::ColorSampler:
        return tr("Click image to place new color sampler.\n"
                  "Drag to move it. Use Alt to remove.");
    case EyedropperType::Ruler:
        return tr("Click and drag to create ruler.\nDrag either end to adjust.");
    case EyedropperType::Note:
        return tr("Click image to add a note.\nClick a note to edit it.");
    case EyedropperType::Count:
        return tr("Click image to count. Use Alt to remove a mark.");
    case EyedropperType::Eyedropper:
        break;
    }
    return tr("Click image to sample a color.");
}

void MainWindow::addPatchOptions()
{
    // CS6's Patch bar, left to right: the selection combine buttons, the Patch
    // mode, the Source/Destination pair, Transparent, and Use Pattern.
    addSelectionModeButtons();

    m_optionsBar->addWidget(new QLabel(tr("Patch:"), m_optionsBar));
    auto *mode = new QComboBox(m_optionsBar);
    mode->addItem(tr("Normal"), false);
    mode->addItem(tr("Content-Aware"), true);
    mode->setCurrentIndex(m_patchContentAware ? 1 : 0);
    mode->setToolTip(tr("Normal samples from where you drag; Content-Aware rebuilds the "
                        "selection from its surroundings and ignores the drag"));
    m_optionsBar->addWidget(mode);

    m_optionsBar->addSeparator();

    // Source and Destination are a radio pair, drawn as two buttons.
    auto *direction = new QButtonGroup(m_optionsBar);
    direction->setExclusive(true);
    QToolButton *sourceButton = nullptr;
    QToolButton *destinationButton = nullptr;
    struct Entry {
        QString label;
        bool destination;
        QString tip;
    };
    const Entry entries[] = {
        {tr("Source"), false,
         tr("The selection is the flaw; drag it onto the pixels to repair it with")},
        {tr("Destination"), true,
         tr("The selection is good material; drag it onto the area to repair")},
    };
    for (const Entry &entry : entries) {
        auto *button = new QToolButton(m_optionsBar);
        button->setCheckable(true);
        button->setAutoRaise(true);
        button->setText(entry.label);
        button->setToolTip(entry.tip);
        button->setStatusTip(entry.tip);
        button->setChecked(entry.destination == m_patchDestination);
        direction->addButton(button, entry.destination ? 1 : 0);
        m_optionsBar->addWidget(button);
        if (entry.destination) {
            destinationButton = button;
        } else {
            sourceButton = button;
        }
    }
    connect(direction, &QButtonGroup::idClicked, this, [this](int id) {
        m_patchDestination = id == 1;
        pushPatchOptions();
    });

    m_optionsBar->addSeparator();

    auto *transparent = new QCheckBox(tr("Transparent"), m_optionsBar);
    transparent->setChecked(m_patchTransparent);
    transparent->setToolTip(tr("Transfer only the source's texture, keeping the patched "
                               "area's own colour"));
    m_optionsBar->addWidget(transparent);
    connect(transparent, &QCheckBox::toggled, this, [this](bool on) {
        m_patchTransparent = on;
        pushPatchOptions();
    });

    // No pattern support yet, so this is present for CS6's shape but disabled —
    // the same treatment the unimplemented flyout entries get.
    auto *usePattern = new QToolButton(m_optionsBar);
    usePattern->setText(tr("Use Pattern"));
    usePattern->setEnabled(false);
    usePattern->setToolTip(tr("Not implemented yet: there are no patterns to fill with"));
    m_optionsBar->addWidget(usePattern);

    // Source, Destination and Transparent all describe how to sample from the
    // drag, and Content-Aware does not sample at all — so they switch off
    // together rather than sitting there having no effect.
    const auto syncEnabled = [sourceButton, destinationButton, transparent](bool contentAware) {
        sourceButton->setEnabled(!contentAware);
        destinationButton->setEnabled(!contentAware);
        transparent->setEnabled(!contentAware);
    };
    syncEnabled(m_patchContentAware);
    connect(mode, &QComboBox::currentIndexChanged, this,
            [this, mode, syncEnabled](int index) {
                m_patchContentAware = mode->itemData(index).toBool();
                syncEnabled(m_patchContentAware);
                pushPatchOptions();
            });

    m_optionsBar->addSeparator();
    m_optionsBar->addWidget(new QLabel(
        tr("Drag to outline a region, then drag the outline to patch"), m_optionsBar));

    pushPatchOptions();
}

void MainWindow::pushPatchOptions()
{
    m_canvas->setPatchOptions(m_patchContentAware, m_patchDestination, m_patchTransparent);
}

void MainWindow::warnHealingSourceRequired()
{
    // Photoshop's own wording and its error icon. The Healing Brush cannot
    // guess where to repair from, so this is a hard stop rather than a hint in
    // the status bar.
#ifdef Q_OS_MACOS
    const QString message = tr("Option-click to define a source point to be used to "
                               "repair the image.");
#else
    const QString message = tr("Alt-click to define a source point to be used to "
                               "repair the image.");
#endif
    QMessageBox box(QMessageBox::Critical, tr("PhotoRust"), message, QMessageBox::Ok, this);
    box.exec();
}

void MainWindow::addHealingRegionOptions(HealingType type)
{
    switch (type) {
    case HealingType::Patch:
        addPatchOptions();
        break;

    case HealingType::ContentAwareMove: {
        // CS6's Mode: Move relocates the region and heals the gap; Extend
        // leaves the original, so the subject is lengthened.
        m_optionsBar->addWidget(new QLabel(tr("Mode:"), m_optionsBar));
        auto *mode = new QComboBox(m_optionsBar);
        mode->addItem(tr("Move"), false);
        mode->addItem(tr("Extend"), true);
        mode->setCurrentIndex(m_camExtend ? 1 : 0);
        m_optionsBar->addWidget(mode);
        connect(mode, &QComboBox::currentIndexChanged, this, [this, mode](int index) {
            m_camExtend = mode->itemData(index).toBool();
            m_canvas->setContentAwareExtend(m_camExtend);
        });
        m_canvas->setContentAwareExtend(m_camExtend);

        m_optionsBar->addSeparator();
        m_optionsBar->addWidget(new QLabel(
            tr("Drag to outline a subject, then drag it where it should go"),
            m_optionsBar));
        break;
    }

    case HealingType::RedEye: {
        struct Field {
            QString label;
            int *value;
        };
        const Field fields[] = {
            {tr("Pupil Size:"), &m_pupilSize},
            {tr("Darken Amount:"), &m_darkenAmount},
        };
        for (const Field &field : fields) {
            m_optionsBar->addWidget(new QLabel(field.label, m_optionsBar));
            auto *spin = new QSpinBox(m_optionsBar);
            spin->setRange(0, 100);
            spin->setValue(*field.value);
            spin->setSuffix(QStringLiteral("%"));
            spin->setFixedWidth(64);
            m_optionsBar->addWidget(spin);

            int *slot = field.value;
            connect(spin, &QSpinBox::valueChanged, this, [this, slot](int v) {
                *slot = v;
                m_canvas->setRedEyeOptions(m_pupilSize, m_darkenAmount);
            });
        }
        m_canvas->setRedEyeOptions(m_pupilSize, m_darkenAmount);

        m_optionsBar->addSeparator();
        m_optionsBar->addWidget(
            new QLabel(tr("Drag over an eye, or click it"), m_optionsBar));
        break;
    }

    case HealingType::SpotHealing:
    case HealingType::Healing:
        // Handled with the brush controls.
        break;
    }
}

void MainWindow::addHealTypeButtons()
{
    // CS6's Type: a radio set of three. The choice persists across tool
    // switches, so it lives on MainWindow rather than in the widgets.
    m_optionsBar->addWidget(new QLabel(tr("Type:"), m_optionsBar));

    auto *group = new QButtonGroup(m_optionsBar);
    group->setExclusive(true);

    const HealType types[] = {HealType::ProximityMatch, HealType::CreateTexture,
                              HealType::ContentAware};
    const QString tips[] = {
        tr("Fill smoothly from the pixels around the brush — best for a blemish "
           "on skin, sky or any other gradient"),
        tr("Fill smoothly, then add grain matched to the surroundings"),
        tr("Rebuild from nearby patches, so edges and texture carry across"),
    };

    for (int i = 0; i < 3; ++i) {
        auto *button = new QToolButton(m_optionsBar);
        button->setCheckable(true);
        button->setAutoRaise(true);
        button->setText(healTypeName(types[i]));
        button->setChecked(types[i] == m_healType);
        button->setToolTip(tips[i]);
        button->setStatusTip(tips[i]);
        group->addButton(button, static_cast<int>(types[i]));
        m_optionsBar->addWidget(button);
    }

    connect(group, &QButtonGroup::idClicked, this, [this](int id) {
        m_healType = static_cast<HealType>(id);
        m_canvas->setHealType(m_healType);
    });

    m_canvas->setHealType(m_healType);
}

void MainWindow::addAnnotationOptions(EyedropperType type)
{
    // The readout labels are recreated with the bar, so the list of them is
    // rebuilt here and `updateAnnotationReadouts` fills them in.
    m_annotationReadouts.clear();

    auto addReadout = [this](const QString &prefix, int width) {
        auto *label = new QLabel(prefix, m_optionsBar);
        label->setMinimumWidth(width);
        label->setProperty("readoutPrefix", prefix);
        m_optionsBar->addWidget(label);
        m_annotationReadouts.append(label);
    };

    switch (type) {
    case EyedropperType::ColorSampler:
        // No readouts here: the sampler values live in the Info panel, which
        // is where CS6 puts them too.
        break;

    case EyedropperType::Ruler:
        // Photoshop's own labels, in its order.
        for (const QString &field : {QStringLiteral("X:"), QStringLiteral("Y:"),
                                     QStringLiteral("W:"), QStringLiteral("H:"),
                                     QStringLiteral("A:"), QStringLiteral("D1:")}) {
            addReadout(field, 62);
        }
        break;

    case EyedropperType::Count:
        addReadout(tr("Count:"), 80);
        break;

    case EyedropperType::Note:
        addReadout(tr("Notes:"), 80);
        break;

    case EyedropperType::Eyedropper:
        return;
    }

    m_optionsBar->addSeparator();

    auto *clear = new QToolButton(m_optionsBar);
    clear->setText(tr("Clear"));
    m_optionsBar->addWidget(clear);
    connect(clear, &QToolButton::clicked, this, [this, type] {
        if (!m_engine) {
            return;
        }
        switch (type) {
        case EyedropperType::ColorSampler:
            m_engine->clearMarkers(static_cast<int>(MarkerKind::ColorSampler));
            break;
        case EyedropperType::Note:
            m_engine->clearMarkers(static_cast<int>(MarkerKind::Note));
            break;
        case EyedropperType::Count:
            m_engine->clearMarkers(static_cast<int>(MarkerKind::Count));
            break;
        case EyedropperType::Ruler:
            m_engine->clearRuler();
            break;
        case EyedropperType::Eyedropper:
            break;
        }
    });

    m_optionsBar->addSeparator();

    QString hint;
    switch (type) {
    case EyedropperType::ColorSampler:
        hint = tr("Click to place a sampler    Drag to move    Alt+click to remove    "
                  "Values read out in the Info panel (F8)");
        break;
    case EyedropperType::Ruler:
        hint = tr("Drag to measure    Drag either end to adjust");
        break;
    case EyedropperType::Note:
        hint = tr("Click to add a note    Click a note to edit it    Alt+click to remove");
        break;
    case EyedropperType::Count:
        hint = tr("Click to count    Drag to move a mark    Alt+click to remove");
        break;
    case EyedropperType::Eyedropper:
        break;
    }
    m_optionsBar->addWidget(new QLabel(hint, m_optionsBar));

    updateAnnotationReadouts();
}

void MainWindow::updateAnnotationReadouts()
{
    if (m_annotationReadouts.isEmpty() || !m_engine) {
        return;
    }

    const auto set = [this](int i, const QString &text) {
        if (i < m_annotationReadouts.size()) {
            QLabel *label = m_annotationReadouts.at(i);
            label->setText(label->property("readoutPrefix").toString() + QLatin1Char(' ') + text);
        }
    };

    switch (m_activeTool == ToolId::Eyedropper ? static_cast<EyedropperType>(m_activeVariant)
                                               : EyedropperType::Eyedropper) {
    case EyedropperType::ColorSampler:
        // Handled entirely by the Info panel.
        break;

    case EyedropperType::Ruler: {
        const rust::Vec<float> m = m_engine->rulerMeasurement();
        if (m.size() < 6) {
            for (int i = 0; i < m_annotationReadouts.size(); ++i) {
                set(i, QStringLiteral("—"));
            }
            break;
        }
        for (int i = 0; i < 6; ++i) {
            // The angle gets a degree sign; the rest are plain pixels.
            set(i, i == 4 ? QStringLiteral("%1°").arg(m[i], 0, 'f', 1)
                          : QStringLiteral("%1").arg(m[i], 0, 'f', 1));
        }
        break;
    }

    case EyedropperType::Count:
        set(0, QString::number(m_engine->markerCount(static_cast<int>(MarkerKind::Count))));
        break;

    case EyedropperType::Note:
        set(0, QString::number(m_engine->markerCount(static_cast<int>(MarkerKind::Note))));
        break;

    case EyedropperType::Eyedropper:
        break;
    }
}

void MainWindow::editNote(int index)
{
    if (!m_engine) {
        return;
    }
    const int kind = static_cast<int>(MarkerKind::Note);
    const QString current = m_engine->markerText(kind, index);

    bool ok = false;
    const QString text = QInputDialog::getMultiLineText(this, tr("Note"), tr("Note text:"),
                                                        current, &ok);
    if (!ok) {
        // Cancelling a brand-new note removes it again, rather than leaving an
        // empty marker on the image.
        if (current.isEmpty()) {
            m_engine->removeMarker(kind, index);
        }
        return;
    }
    m_engine->setMarkerText(kind, index, text);
}

void MainWindow::addCropOptions(CropType type)
{
    if (type == CropType::Slice || type == CropType::SliceSelect) {
        auto *clear = new QToolButton(m_optionsBar);
        clear->setText(tr("Clear Slices"));
        clear->setToolTip(tr("Remove every user slice, leaving the whole canvas as one "
                             "auto slice"));
        m_optionsBar->addWidget(clear);
        connect(clear, &QToolButton::clicked, this, [this] {
            if (m_engine) {
                m_engine->clearSlices();
            }
        });

        if (type == CropType::SliceSelect) {
            auto *remove = new QToolButton(m_optionsBar);
            remove->setText(tr("Delete Slice"));
            remove->setToolTip(tr("Delete the selected slice (Del)"));
            m_optionsBar->addWidget(remove);
            connect(remove, &QToolButton::clicked, m_canvas, &CanvasView::deleteSelectedSlice);
        }

        m_optionsBar->addSeparator();

        auto *save = new QToolButton(m_optionsBar);
        save->setText(tr("Save Slices..."));
        save->setToolTip(tr("Write every slice out as its own image file"));
        m_optionsBar->addWidget(save);
        connect(save, &QToolButton::clicked, this, &MainWindow::exportSlices);

        m_optionsBar->addSeparator();
        m_optionsBar->addWidget(new QLabel(
            type == CropType::Slice
                ? tr("Drag to cut a slice    The rest of the canvas is sliced automatically")
                : tr("Click a slice to select it    Drag it or its handles to adjust    "
                     "Del to remove it"),
            m_optionsBar));
        return;
    }

    if (type == CropType::Perspective) {
        // Perspective Crop has neither a ratio preset nor Delete Cropped
        // Pixels in CS6: the output size comes from the quad the user marked,
        // and everything outside it is resampled away by definition.
        auto *cancel = new QToolButton(m_optionsBar);
        cancel->setText(QStringLiteral("✘"));
        cancel->setToolTip(tr("Cancel the crop (Esc)"));
        m_optionsBar->addWidget(cancel);
        connect(cancel, &QToolButton::clicked, m_canvas, &CanvasView::resetCrop);

        auto *apply = new QToolButton(m_optionsBar);
        apply->setText(QStringLiteral("✓"));
        apply->setToolTip(tr("Straighten and crop (Enter)"));
        m_optionsBar->addWidget(apply);
        connect(apply, &QToolButton::clicked, m_canvas, &CanvasView::commitCrop);

        m_optionsBar->addSeparator();
        m_optionsBar->addWidget(new QLabel(
            tr("Drag out a box, then pull its corners onto the subject    "
               "Enter or double-click to straighten    Esc to reset"),
            m_optionsBar));
        return;
    }

    // CS6 opens the Crop bar with a ratio preset. The named ratios are the
    // ones its own list offers; "Unconstrained" leaves the box free.
    struct Preset {
        QString label;
        double ratio;
    };
    const Preset presets[] = {
        {tr("Unconstrained"), 0.0},
        {tr("1 : 1 (Square)"), 1.0},
        {tr("4 : 5 (8:10)"), 4.0 / 5.0},
        {tr("5 : 7"), 5.0 / 7.0},
        {tr("2 : 3 (4:6)"), 2.0 / 3.0},
        {tr("16 : 9"), 16.0 / 9.0},
    };

    auto *ratio = new QComboBox(m_optionsBar);
    for (const Preset &preset : presets) {
        ratio->addItem(preset.label, preset.ratio);
    }
    ratio->setCurrentIndex(0);
    for (int i = 0; i < ratio->count(); ++i) {
        if (qFuzzyCompare(ratio->itemData(i).toDouble() + 1.0, m_cropRatio + 1.0)) {
            ratio->setCurrentIndex(i);
            break;
        }
    }
    ratio->setToolTip(tr("Lock the crop box to an aspect ratio"));
    m_optionsBar->addWidget(ratio);
    connect(ratio, &QComboBox::currentIndexChanged, this, [this, ratio](int index) {
        m_cropRatio = ratio->itemData(index).toDouble();
        pushCropOptions();
    });

    m_optionsBar->addSeparator();

    auto *deletePixels = new QCheckBox(tr("Delete Cropped Pixels"), m_optionsBar);
    deletePixels->setChecked(m_cropDeletePixels);
    deletePixels->setToolTip(tr("Discard the pixels outside the crop, rather than keeping "
                                "them hidden beyond the canvas edge"));
    m_optionsBar->addWidget(deletePixels);
    connect(deletePixels, &QCheckBox::toggled, this, [this](bool on) {
        m_cropDeletePixels = on;
        pushCropOptions();
    });

    m_optionsBar->addSeparator();

    // CS6 puts a cancel/commit pair at the right of the crop bar. The keyboard
    // route (Esc / Enter) is handled by the canvas.
    auto *cancel = new QToolButton(m_optionsBar);
    cancel->setText(QStringLiteral("✘"));
    cancel->setToolTip(tr("Cancel the crop (Esc)"));
    m_optionsBar->addWidget(cancel);
    connect(cancel, &QToolButton::clicked, m_canvas, &CanvasView::resetCrop);

    auto *commit = new QToolButton(m_optionsBar);
    commit->setText(QStringLiteral("✓"));
    commit->setToolTip(tr("Apply the crop (Enter)"));
    m_optionsBar->addWidget(commit);
    connect(commit, &QToolButton::clicked, m_canvas, &CanvasView::commitCrop);

    m_optionsBar->addSeparator();
    m_optionsBar->addWidget(new QLabel(
        tr("Drag to set the crop    Enter or double-click to apply    Esc to reset"),
        m_optionsBar));

    pushCropOptions();
}

void MainWindow::pushCropOptions()
{
    m_canvas->setCropOptions(m_cropRatio, m_cropDeletePixels);
}

void MainWindow::addQuickSelectOptions(QuickSelectType type)
{
    if (type == QuickSelectType::Brush) {
        // CS6 gives Quick Selection a full brush picker; the diameter is the
        // part that changes the result, so that is what is here. Notably it
        // has no Tolerance — the tool works that out from the pixels under the
        // brush (see core/src/wand.rs).
        m_optionsBar->addWidget(new QLabel(tr("Size:"), m_optionsBar));
        auto *size = new QSpinBox(m_optionsBar);
        size->setRange(1, 5000);
        size->setValue(m_quickBrushSize);
        size->setSuffix(tr(" px"));
        size->setFixedWidth(80);
        size->setToolTip(tr("Diameter of the brush the selection grows from"));
        m_optionsBar->addWidget(size);
        connect(size, &QSpinBox::valueChanged, this, [this](int value) {
            m_quickBrushSize = value;
            pushQuickSelectOptions();
        });
        pushQuickSelectOptions();
        return;
    }

    // Magic Wand: Tolerance, then the two checkboxes, in CS6's order.
    m_optionsBar->addWidget(new QLabel(tr("Tolerance:"), m_optionsBar));
    auto *tolerance = new QSpinBox(m_optionsBar);
    tolerance->setRange(0, 255);
    tolerance->setValue(m_wandTolerance);
    tolerance->setFixedWidth(64);
    tolerance->setToolTip(tr("How far a pixel may differ, per channel, and still match"));
    m_optionsBar->addWidget(tolerance);
    connect(tolerance, &QSpinBox::valueChanged, this, [this](int value) {
        m_wandTolerance = value;
        pushQuickSelectOptions();
    });

    struct Toggle {
        QString label;
        bool *value;
        QString tip;
    };
    const Toggle toggles[] = {
        {tr("Anti-alias"), &m_wandAntialias, tr("Soften the edge of the selection")},
        {tr("Contiguous"), &m_wandContiguous,
         tr("Select only the connected area, rather than every matching pixel")},
    };

    for (const Toggle &toggle : toggles) {
        auto *box = new QCheckBox(toggle.label, m_optionsBar);
        box->setChecked(*toggle.value);
        box->setToolTip(toggle.tip);
        m_optionsBar->addWidget(box);

        bool *slot = toggle.value;
        connect(box, &QCheckBox::toggled, this, [this, slot](bool on) {
            *slot = on;
            pushQuickSelectOptions();
        });
    }

    pushQuickSelectOptions();
}

void MainWindow::pushQuickSelectOptions()
{
    m_canvas->setQuickSelectOptions(m_quickBrushSize, m_wandTolerance, m_wandAntialias,
                                    m_wandContiguous);
}

void MainWindow::pushMagneticOptions()
{
    m_canvas->setMagneticOptions(m_magneticWidth, m_magneticContrast, m_magneticFrequency);
}

void MainWindow::pushBrushSettings()
{
    if (!m_engine || !m_brushSize) {
        return;
    }
    m_engine->setBrush(float(m_brushSize->value()), m_brushHardness->value(),
                       m_brushOpacity->value(), m_brushFlow->value(),
                       /*spacing=*/25);
}

// ------------------------------------------------------------------- docks --

void MainWindow::createDocks()
{
    QMenu *windowMenu = menuBar()->findChild<QMenu *>(QStringLiteral("windowMenu"));

    // The tool panel closes from the × on its own header, so it needs an entry
    // here to come back — same as CS6's Window ▸ Tools.
    if (windowMenu && m_toolsDock) {
        windowMenu->addAction(m_toolsDock->toggleViewAction());
        windowMenu->addSeparator();
    }

    auto addPanel = [&](const QString &title, QWidget *content, Qt::DockWidgetArea area,
                        const QString &commandId = {}) {
        auto *dock = new QDockWidget(title, this);
        dock->setObjectName(title + QStringLiteral("Dock"));
        dock->setWidget(content);
        dock->setAllowedAreas(Qt::LeftDockWidgetArea | Qt::RightDockWidgetArea);
        addDockWidget(area, dock);

        if (windowMenu) {
            QAction *toggle = dock->toggleViewAction();
            // Bind the panel toggle to its keymap entry (F7 for Layers, …).
            if (!commandId.isEmpty()) {
                if (QAction *bound = m_registry->action(commandId)) {
                    toggle->setShortcut(bound->shortcut());
                }
            }
            windowMenu->addAction(toggle);
        }
        return dock;
    };

    m_colorPanel = new ColorPanel(m_engine, this);
    addPanel(tr("Color"), m_colorPanel, Qt::RightDockWidgetArea,
             QStringLiteral("window.color"));

    // A stand-in so the Color/Swatches pair reads like CS6; real swatch
    // management is not implemented yet.
    auto *swatchesPlaceholder = new QLabel(tr("  Swatches"), this);
    swatchesPlaceholder->setAlignment(Qt::AlignTop | Qt::AlignLeft);
    QDockWidget *swatchesDock =
        addPanel(tr("Swatches"), swatchesPlaceholder, Qt::RightDockWidgetArea);

    // CS6 tabs Info in with Properties; we have no Properties panel, so it
    // joins the Color/Swatches group in the same corner of the dock area.
    m_infoPanel = new InfoPanel(m_engine, this);
    m_infoDock = addPanel(tr("Info"), m_infoPanel, Qt::RightDockWidgetArea,
                          QStringLiteral("window.info"));

    m_historyPanel = new HistoryPanel(m_engine, this);
    QDockWidget *historyDock = addPanel(tr("History"), m_historyPanel,
                                        Qt::RightDockWidgetArea);

    m_layersPanel = new LayersPanel(m_engine, this);
    QDockWidget *layersDock = addPanel(tr("Layers"), m_layersPanel,
                                       Qt::RightDockWidgetArea,
                                       QStringLiteral("window.layers"));

    // Stack Color/Swatches into one tabbed group, as CS6 ships them.
    if (QDockWidget *colorDock =
            findChild<QDockWidget *>(tr("Color") + QStringLiteral("Dock"))) {
        tabifyDockWidget(colorDock, swatchesDock);
        if (m_infoDock) {
            tabifyDockWidget(swatchesDock, m_infoDock);
        }
        colorDock->raise();
    }

    resizeDocks({historyDock, layersDock}, {220, 380}, Qt::Vertical);

    connect(m_layersPanel, &LayersPanel::documentChanged,
            this, &MainWindow::onDocumentChanged);
    connect(m_historyPanel, &HistoryPanel::documentChanged,
            this, &MainWindow::onDocumentChanged);
}

void MainWindow::createStatusBar()
{
    m_statusZoom = new QLabel(QStringLiteral("100%"), this);
    m_statusZoom->setMinimumWidth(56);
    statusBar()->addWidget(m_statusZoom);

    m_statusDocSize = new QLabel(this);
    m_statusDocSize->setMinimumWidth(140);
    statusBar()->addWidget(m_statusDocSize);

    m_statusPosition = new QLabel(this);
    m_statusPosition->setMinimumWidth(120);
    statusBar()->addPermanentWidget(m_statusPosition);
}

void MainWindow::connectEngine()
{
    if (!m_engine) {
        return;
    }
    // The engine is the source of truth: it announces changes and the UI
    // re-reads. No panel pushes state at another panel.
    connect(m_engine, &Engine::canvasChanged, m_canvas, &CanvasView::refresh);
    connect(m_engine, &Engine::layersChanged, m_layersPanel, &LayersPanel::refresh);
    connect(m_engine, &Engine::historyChanged, m_historyPanel, &HistoryPanel::refresh);
    connect(m_engine, &Engine::selectionChanged, m_canvas, &CanvasView::refreshSelection);
    connect(m_engine, &Engine::slicesChanged, m_canvas, &CanvasView::refreshSlices);
    connect(m_engine, &Engine::annotationsChanged, m_canvas, &CanvasView::refreshAnnotations);
    connect(m_engine, &Engine::annotationsChanged, this,
            &MainWindow::updateAnnotationReadouts);
    connect(m_engine, &Engine::annotationsChanged, m_infoPanel, &InfoPanel::refreshSamplers);
    connect(m_engine, &Engine::annotationsChanged, m_infoPanel, &InfoPanel::refreshRuler);
    connect(m_engine, &Engine::selectionChanged, m_infoPanel, &InfoPanel::refreshSelection);
    // The samplers read the pixels beneath them, so an edit changes their
    // values even when the samplers themselves have not moved.
    connect(m_engine, &Engine::canvasChanged, m_infoPanel, &InfoPanel::refreshSamplers);
    connect(m_engine, &Engine::layersChanged, m_infoPanel, &InfoPanel::refreshDocumentSize);
    connect(m_engine, &Engine::documentTitleChanged, this, &MainWindow::updateWindowTitle);
}

// ---------------------------------------------------------------- commands --

void MainWindow::newDocument()
{
    if (!confirmDiscardChanges()) {
        return;
    }

    QDialog dialog(this);
    dialog.setWindowTitle(tr("New"));
    auto *form = new QFormLayout(&dialog);

    auto *width = new QSpinBox(&dialog);
    width->setRange(1, 30000);
    width->setValue(1280);
    width->setSuffix(tr(" px"));
    form->addRow(tr("Width:"), width);

    auto *height = new QSpinBox(&dialog);
    height->setRange(1, 30000);
    height->setValue(800);
    height->setSuffix(tr(" px"));
    form->addRow(tr("Height:"), height);

    auto *fill = new QComboBox(&dialog);
    fill->addItems({tr("White"), tr("Transparent"), tr("Background Color")});
    form->addRow(tr("Background Contents:"), fill);

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel,
                                         &dialog);
    connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
    form->addRow(buttons);

    if (dialog.exec() != QDialog::Accepted) {
        return;
    }

    m_engine->newDocument(width->value(), height->value(), fill->currentIndex());
    refreshAll();
    fitOnScreen();
}

void MainWindow::openDocument()
{
    if (!confirmDiscardChanges()) {
        return;
    }
    const QString path = QFileDialog::getOpenFileName(this, tr("Open"), QString(),
                                                      QLatin1String(kOpenFilter));
    if (path.isEmpty()) {
        return;
    }

    if (!m_engine->openFile(path)) {
        // The engine reads PSD itself and delegates the rest to Qt's plugins;
        // if it declined, try decoding here and handing the pixels over.
        QImage image(path);
        if (image.isNull() || !m_engine->loadImage(image, path)) {
            QMessageBox::warning(this, tr("Open"),
                                 tr("Could not open \"%1\".\n\n"
                                    "The file may be corrupt, or in a format that is "
                                    "not supported yet.")
                                     .arg(QFileInfo(path).fileName()));
            return;
        }
    }
    refreshAll();
    fitOnScreen();
}

bool MainWindow::saveDocument()
{
    // Without a known path this is really Save As.
    return saveDocumentAs();
}

bool MainWindow::saveDocumentAs()
{
    const QString path = QFileDialog::getSaveFileName(this, tr("Save As"), QString(),
                                                      QLatin1String(kSaveFilter));
    if (path.isEmpty()) {
        return false;
    }

    // PSD is written by the engine; everything else by Qt's writers.
    if (path.endsWith(QStringLiteral(".psd"), Qt::CaseInsensitive)) {
        if (!m_engine->saveFile(path)) {
            QMessageBox::warning(this, tr("Save"), tr("Could not write \"%1\".").arg(path));
            return false;
        }
    } else {
        const QImage composite = m_engine->compositeImage();
        if (!composite.save(path)) {
            QMessageBox::warning(this, tr("Save"), tr("Could not write \"%1\".").arg(path));
            return false;
        }
        m_engine->markSavedAs(path);
    }

    updateWindowTitle();
    return true;
}

void MainWindow::exportSlices()
{
    if (!m_engine) {
        return;
    }
    const int count = m_engine->sliceCount();
    if (count <= 0) {
        return;
    }

    const QString dir = QFileDialog::getExistingDirectory(this, tr("Save Slices To"));
    if (dir.isEmpty()) {
        return;
    }

    // Photoshop names slice files after the document with the slice number
    // appended, which is what makes a sliced layout reassemblable.
    QString base = QFileInfo(m_engine->getDocumentTitle()).completeBaseName();
    base.remove(QLatin1Char('*'));
    if (base.isEmpty()) {
        base = QStringLiteral("slice");
    }

    int written = 0;
    QStringList failures;
    for (int i = 0; i < count; ++i) {
        const rust::Vec<::std::int32_t> info = m_engine->sliceAt(i);
        if (info.size() < 6) {
            continue;
        }
        const QImage image = m_engine->sliceImage(i);
        if (image.isNull()) {
            continue;
        }

        const QString path =
            QStringLiteral("%1/%2_%3.png")
                .arg(dir, base, QString::number(info[4]).rightJustified(2, QLatin1Char('0')));
        if (image.save(path)) {
            ++written;
        } else {
            failures.append(QFileInfo(path).fileName());
        }
    }

    if (failures.isEmpty()) {
        statusBar()->showMessage(tr("Wrote %n slice(s) to %1", nullptr, written).arg(dir), 4000);
    } else {
        QMessageBox::warning(this, tr("Save Slices"),
                             tr("Wrote %1 of %2 slices. Could not write: %3")
                                 .arg(written)
                                 .arg(count)
                                 .arg(failures.join(QStringLiteral(", "))));
    }
}

void MainWindow::undo()
{
    m_engine->undo();
    refreshAll();
}

void MainWindow::redo()
{
    m_engine->redo();
    refreshAll();
}

void MainWindow::fillWithForeground()
{
    m_engine->fillForeground();
    refreshAll();
}

void MainWindow::fillWithBackground()
{
    m_engine->fillBackground();
    refreshAll();
}

void MainWindow::clearSelection()
{
    m_engine->clearSelection();
    refreshAll();
}

void MainWindow::showCanvasSize()
{
    QDialog dialog(this);
    dialog.setWindowTitle(tr("Canvas Size"));
    auto *form = new QFormLayout(&dialog);

    auto *width = new QSpinBox(&dialog);
    width->setRange(1, 30000);
    width->setValue(m_engine->getCanvasWidth());
    width->setSuffix(tr(" px"));
    form->addRow(tr("Width:"), width);

    auto *height = new QSpinBox(&dialog);
    height->setRange(1, 30000);
    height->setValue(m_engine->getCanvasHeight());
    height->setSuffix(tr(" px"));
    form->addRow(tr("Height:"), height);

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel,
                                         &dialog);
    connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
    form->addRow(buttons);

    if (dialog.exec() == QDialog::Accepted) {
        m_engine->resizeCanvas(width->value(), height->value());
        refreshAll();
    }
}

void MainWindow::applyAdjustment(const QString &name)
{
    // Adjustments that take parameters get a small prompt; the rest apply
    // straight away. Full CS6-style dialogs with live preview come later.
    float p1 = 0.0f;
    float p2 = 0.0f;
    float p3 = 1.0f;
    bool ok = true;

    if (name == QLatin1String("Posterize")) {
        p1 = float(QInputDialog::getInt(this, name, tr("Levels:"), 4, 2, 255, 1, &ok));
    } else if (name == QLatin1String("Threshold")) {
        p1 = float(QInputDialog::getInt(this, name, tr("Threshold Level:"), 128, 0, 255,
                                        1, &ok));
    } else if (name == QLatin1String("Brightness/Contrast")) {
        p1 = float(QInputDialog::getDouble(this, name, tr("Brightness (-1 to 1):"), 0.0,
                                           -1.0, 1.0, 2, &ok));
        if (ok) {
            p2 = float(QInputDialog::getDouble(this, name, tr("Contrast (-1 to 1):"), 0.0,
                                               -1.0, 1.0, 2, &ok));
        }
    } else if (name == QLatin1String("Hue/Saturation")) {
        p1 = float(QInputDialog::getDouble(this, name, tr("Hue (-1 to 1):"), 0.0, -1.0,
                                           1.0, 2, &ok));
        if (ok) {
            p2 = float(QInputDialog::getDouble(this, name, tr("Saturation (-1 to 1):"),
                                               0.0, -1.0, 1.0, 2, &ok));
        }
    }

    if (!ok) {
        return;
    }
    m_engine->applyAdjustment(name, p1, p2, p3);
    refreshAll();
}

void MainWindow::applyFilter(const QString &name)
{
    float p1 = 0.0f;
    float p2 = 0.0f;
    bool ok = true;

    if (name == QLatin1String("Gaussian Blur")) {
        p1 = float(QInputDialog::getDouble(this, name, tr("Radius (pixels):"), 2.0, 0.1,
                                           250.0, 1, &ok));
    } else if (name == QLatin1String("Unsharp Mask")) {
        p1 = float(QInputDialog::getDouble(this, name, tr("Amount:"), 1.0, 0.0, 5.0, 2,
                                           &ok));
        if (ok) {
            p2 = float(QInputDialog::getDouble(this, name, tr("Radius (pixels):"), 1.0,
                                               0.1, 250.0, 1, &ok));
        }
    } else if (name == QLatin1String("Add Noise")) {
        p1 = float(QInputDialog::getDouble(this, name, tr("Amount (0-1):"), 0.1, 0.0, 1.0,
                                           2, &ok));
    }

    if (!ok) {
        return;
    }
    m_engine->applyFilter(name, p1, p2);
    refreshAll();
}

void MainWindow::zoomIn()
{
    m_canvas->zoomIn();
}

void MainWindow::zoomOut()
{
    m_canvas->zoomOut();
}

void MainWindow::fitOnScreen()
{
    m_canvas->fitToWindow();
}

void MainWindow::actualPixels()
{
    m_canvas->actualPixels();
}

// --------------------------------------------------------------- reactions --

void MainWindow::onToolChanged(ToolId tool, int variant)
{
    m_activeTool = tool;
    m_activeVariant = variant;

    m_canvas->setActiveTool(tool);
    if (tool == ToolId::Healing) {
        m_canvas->setHealingType(static_cast<HealingType>(variant));
    }
    if (tool == ToolId::Marquee) {
        m_canvas->setMarqueeType(static_cast<MarqueeType>(variant));
    } else if (tool == ToolId::Lasso) {
        m_canvas->setLassoType(static_cast<LassoType>(variant));
    } else if (tool == ToolId::QuickSelect) {
        m_canvas->setQuickSelectType(static_cast<QuickSelectType>(variant));
    } else if (tool == ToolId::Crop) {
        m_canvas->setCropType(static_cast<CropType>(variant));
    } else if (tool == ToolId::Eyedropper) {
        const auto kind = static_cast<EyedropperType>(variant);
        m_canvas->setEyedropperType(kind);
        // Both the Color Sampler and the Ruler put their numbers in the Info
        // panel rather than the options bar, so bring it forward for either.
        const bool wantsInfo = kind == EyedropperType::ColorSampler
            || kind == EyedropperType::Ruler;
        if (wantsInfo && m_infoDock) {
            m_infoDock->show();
            m_infoDock->raise();
        }
        if (m_infoPanel) {
            m_infoPanel->setRulerMode(kind == EyedropperType::Ruler);
            m_infoPanel->setHint(infoHintFor(kind));
        }
    } else if (m_infoPanel) {
        // Leaving the eyedropper group puts the panel back to CMYK.
        m_infoPanel->setRulerMode(false);
        m_infoPanel->setHint(infoHintFor(EyedropperType::Eyedropper));
    }
    populateOptionsBar(tool, variant);
    statusBar()->showMessage(toolVariantName(tool, variant), 2000);
}

void MainWindow::onDocumentChanged()
{
    m_canvas->refresh();
    updateWindowTitle();
}

void MainWindow::onCursorMoved(const QPointF &pos)
{
    m_statusPosition->setText(QStringLiteral("X: %1   Y: %2")
                                  .arg(int(pos.x()))
                                  .arg(int(pos.y())));
}

void MainWindow::onZoomChanged(double zoom)
{
    m_statusZoom->setText(QStringLiteral("%1%").arg(zoom * 100.0, 0, 'f', 1));
}

void MainWindow::refreshAll()
{
    m_canvas->refresh();
    m_layersPanel->refresh();
    m_historyPanel->refresh();
    updateWindowTitle();

    m_statusDocSize->setText(tr("%1 × %2 px")
                                 .arg(m_engine->getCanvasWidth())
                                 .arg(m_engine->getCanvasHeight()));
}

void MainWindow::updateWindowTitle()
{
    const QString title = m_engine ? m_engine->getDocumentTitle() : QString();
    setWindowTitle(title.isEmpty() ? tr("PhotoRust")
                                   : tr("%1 — PhotoRust").arg(title));
    if (m_statusDocSize && m_engine) {
        m_statusDocSize->setText(tr("%1 × %2 px")
                                     .arg(m_engine->getCanvasWidth())
                                     .arg(m_engine->getCanvasHeight()));
    }
}

bool MainWindow::confirmDiscardChanges()
{
    if (!m_engine || !m_engine->getModified()) {
        return true;
    }
    const auto choice = QMessageBox::question(
        this, tr("PhotoRust"),
        tr("Save changes to \"%1\" before closing?").arg(m_engine->getDocumentTitle()),
        QMessageBox::Save | QMessageBox::Discard | QMessageBox::Cancel);

    switch (choice) {
    case QMessageBox::Save:
        return saveDocument();
    case QMessageBox::Discard:
        return true;
    default:
        return false;
    }
}

void MainWindow::closeEvent(QCloseEvent *event)
{
    if (confirmDiscardChanges()) {
        event->accept();
    } else {
        event->ignore();
    }
}

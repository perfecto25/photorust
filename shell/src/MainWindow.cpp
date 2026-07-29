#include "MainWindow.h"

#include "canvas/CanvasView.h"
#include "panels/ColorPanel.h"
#include "panels/HistoryPanel.h"
#include "panels/LayersPanel.h"
#include "shortcuts/CommandRegistry.h"
#include "tools/ToolStrip.h"

#include "photorust_core/src/bridge.cxxqt.h"

#include <QApplication>
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

    m_toolStrip = new ToolStrip(m_registry, this);
    addToolBar(Qt::LeftToolBarArea, m_toolStrip);

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
    connect(m_canvas, &CanvasView::zoomChanged, this, &MainWindow::onZoomChanged);
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

    // Shift+M cycles Rectangular ↔ Elliptical, as CS6 does with its
    // "Use Shift Key for Tool Switch" preference on by default. Registered
    // here rather than in the strip so it lands on the window with the rest.
    QAction *cycle = m_registry->registerCommand(QStringLiteral("tool.marquee.cycle"),
                                                 tr("Cycle Marquee Tool"),
                                                 QKeySequence(QStringLiteral("Shift+M")));
    connect(cycle, &QAction::triggered, this,
            [this] { m_toolStrip->cycleVariant(ToolId::Marquee); });
    if (!actions().contains(cycle)) {
        addAction(cycle);
    }
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

    if (toolPaints(tool)) {
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
    } else if (toolSelects(tool)) {
        const bool lineSelect = tool == ToolId::Marquee
            && (static_cast<MarqueeType>(variant) == MarqueeType::SingleRow
                || static_cast<MarqueeType>(variant) == MarqueeType::SingleColumn);
        m_optionsBar->addWidget(new QLabel(
            lineSelect
                ? tr("Click to select a line    Shift = add to selection    Alt = subtract")
                : tr("Shift = add to selection    Alt = subtract    Click = deselect"),
            m_optionsBar));
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
    connect(m_engine, &Engine::selectionChanged, m_canvas,
            qOverload<>(&QWidget::update));
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
    if (tool == ToolId::Marquee) {
        m_canvas->setMarqueeType(static_cast<MarqueeType>(variant));
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

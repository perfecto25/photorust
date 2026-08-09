#include "MainWindow.h"

#include "canvas/CanvasView.h"
#include "panels/BrushPresetPicker.h"
#include "panels/ColorPanel.h"
#include "panels/HistoryPanel.h"
#include "panels/InfoPanel.h"
#include "panels/LayersPanel.h"
#include "panels/PathsPanel.h"
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
#include <QLayout>
#include <QLineEdit>
#include <QPushButton>
#include <QMenu>
#include <QMenuBar>
#include <QPainter>
#include <QMessageBox>
#include <QSpinBox>
#include <QSignalBlocker>
#include <QStatusBar>
#include <QTabBar>
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

/// Stop a dialog's buttons being squeezed narrower than their text.
///
/// A message box sizes itself from its message, and its button row is allowed to
/// shrink below what the buttons asked for — so a long label ends up clipped
/// mid-word. Pinning each button's minimum to its own size hint forces the
/// dialog to widen instead. This bites hardest with the platform-substituted
/// labels: Qt's "Discard" role becomes "Close without Saving" under GTK.
void unsqueezeButtons(QDialog *dialog)
{
    for (QAbstractButton *button : dialog->findChildren<QAbstractButton *>()) {
        button->setMinimumWidth(button->sizeHint().width());
    }
    if (QLayout *layout = dialog->layout()) {
        layout->activate();
    }
}

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

    // The canvas sits under a tab bar, one tab per open document, as CS6 does.
    m_canvas = new CanvasView(m_engine, this);

    m_documentTabs = new QTabBar(this);
    m_documentTabs->setObjectName(QStringLiteral("documentTabs"));
    m_documentTabs->setExpanding(false);
    m_documentTabs->setTabsClosable(true);
    m_documentTabs->setMovable(true);
    m_documentTabs->setDrawBase(false);
    m_documentTabs->setElideMode(Qt::ElideRight);
    m_documentTabs->setUsesScrollButtons(true);

    auto *centre = new QWidget(this);
    auto *centreLayout = new QVBoxLayout(centre);
    centreLayout->setContentsMargins(0, 0, 0, 0);
    centreLayout->setSpacing(0);
    centreLayout->addWidget(m_documentTabs);
    centreLayout->addWidget(m_canvas, 1);
    setCentralWidget(centre);

    connect(m_documentTabs, &QTabBar::currentChanged, this, &MainWindow::onTabSelected);
    connect(m_documentTabs, &QTabBar::tabCloseRequested, this, &MainWindow::onTabCloseRequested);

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
    connect(m_canvas, &CanvasView::mixerLoadChanged, this,
            &MainWindow::refreshMixerLoadSwatch);
    connect(m_canvas, &CanvasView::lockedLayerRefused, this,
            &MainWindow::warnLayerLocked);
    connect(m_canvas, &CanvasView::cloneSourceRequired, this,
            &MainWindow::warnCloneSourceRequired);
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
    // "Foreground to Background" means the colours as they are now, so the
    // options-bar swatch has to follow them.
    connect(m_colorPanel, &ColorPanel::foregroundChanged, this,
            [this] { refreshGradientSwatch(); });
    // The Color panel only reports the foreground; the tool strip's swatch is
    // where the background pair is edited.
    connect(m_toolStrip->swatches(), &ColorSwatchWidget::backgroundChanged, this,
            [this] { refreshGradientSwatch(); });
    connect(m_toolStrip->swatches(), &ColorSwatchWidget::foregroundChanged, this,
            [this] { refreshGradientSwatch(); });

    onToolChanged(ToolId::Brush, 0);
    refreshDocumentTabs();
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
    // Closing one document is the deliberate act that *does* prompt.
    file->addAction(command(QStringLiteral("file.close"), tr("&Close"),
                            &MainWindow::closeDocument));
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
        {"tool.brush.cycle", tr("Cycle Brush Tool"), QStringLiteral("Shift+B"),
         ToolId::Brush},
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
    m_brushOpacity = nullptr;
    m_brushFlow = nullptr;
    m_brushTipButton = nullptr;
    m_mixerLoadButton = nullptr;
    m_mixerPresetCombo = nullptr;
    m_gradientSwatch = nullptr;

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
        // CS6 puts Size and Hardness inside the brush preset picker rather than
        // on the bar; the bar carries the tip button that opens it, showing the
        // current tip and its diameter.
        m_brushTipButton = new QToolButton(m_optionsBar);
        m_brushTipButton->setObjectName(QStringLiteral("brushTipButton"));
        m_brushTipButton->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
        m_brushTipButton->setPopupMode(QToolButton::InstantPopup);
        m_brushTipButton->setIconSize(QSize(20, 20));
        m_brushTipButton->setToolTip(tr("Brush preset picker: size, hardness and presets"));
        m_optionsBar->addWidget(m_brushTipButton);
        refreshBrushTipButton();
        connect(m_brushTipButton, &QToolButton::clicked, this, [this] {
            brushPicker()->setValues(m_brushSizeValue, m_brushHardnessValue);
            brushPicker()->popUpUnder(m_brushTipButton);
        });

        // The Pencil has no Flow in CS6 — it lays whole pixels, so there is
        // nothing to build up gradually — and gains Auto Erase instead.
        const bool pencil = tool == ToolId::Brush
            && brushIsPencil(static_cast<BrushType>(variant));

        const bool replacing = tool == ToolId::Brush
            && brushReplacesColor(static_cast<BrushType>(variant));

        // The Mixer Brush has no Opacity in CS6: how much paint reaches the
        // canvas is Wet, Load and Flow's business, and a master opacity on top
        // of them would mean two controls for one thing.
        const bool mixing = tool == ToolId::Brush
            && brushMixesColor(static_cast<BrushType>(variant));

        if (!mixing) {
            m_optionsBar->addWidget(new QLabel(tr("Opacity:"), m_optionsBar));
            m_brushOpacity = new QSpinBox(m_optionsBar);
            m_brushOpacity->setRange(0, 100);
            m_brushOpacity->setValue(100);
            m_brushOpacity->setSuffix(QStringLiteral("%"));
            m_brushOpacity->setFixedWidth(64);
            m_optionsBar->addWidget(m_brushOpacity);
        }

        if (replacing) {
            addColorReplaceOptions();
        } else if (mixing) {
            addMixerOptions();
        } else if (!pencil) {
            m_optionsBar->addWidget(new QLabel(tr("Flow:"), m_optionsBar));
            m_brushFlow = new QSpinBox(m_optionsBar);
            m_brushFlow->setRange(1, 100);
            m_brushFlow->setValue(100);
            m_brushFlow->setSuffix(QStringLiteral("%"));
            m_brushFlow->setFixedWidth(64);
            m_optionsBar->addWidget(m_brushFlow);
        } else {
            m_optionsBar->addSeparator();
            auto *autoErase = new QCheckBox(tr("Auto Erase"), m_optionsBar);
            autoErase->setChecked(m_autoErase);
            autoErase->setToolTip(tr("Begin a stroke on a pixel that is already the "
                                     "foreground colour and it paints the background "
                                     "colour instead"));
            m_optionsBar->addWidget(autoErase);
            connect(autoErase, &QCheckBox::toggled, this, [this](bool on) {
                m_autoErase = on;
                if (m_engine) {
                    m_engine->setAutoErase(on);
                }
            });
        }

        if (m_brushOpacity) {
            connect(m_brushOpacity, &QSpinBox::valueChanged, this,
                    &MainWindow::pushBrushSettings);
        }
        if (m_brushFlow) {
            connect(m_brushFlow, &QSpinBox::valueChanged, this,
                    &MainWindow::pushBrushSettings);
        }

        pushBrushSettings();

        // Neither do the toning tools: Exposure (or the Sponge's Flow) is the
        // only strength they have.
        if (tool == ToolId::Dodge) {
            const QString unused = tr("Not used by this tool; see Exposure");
            if (m_brushOpacity) {
                m_brushOpacity->setEnabled(false);
                m_brushOpacity->setToolTip(unused);
            }
            if (m_brushFlow) {
                m_brushFlow->setEnabled(false);
                m_brushFlow->setToolTip(unused);
            }
            addToneOptions(static_cast<ToneTool>(variant));
        }

        // None of the Blur button's three has Opacity or Flow in CS6 — how much
        // they do is Strength's business — so they are disabled rather than left
        // there doing nothing, the same treatment the healing brushes get.
        if (tool == ToolId::Blur) {
            const QString unused = tr("Not used by this tool; see Strength");
            if (m_brushOpacity) {
                m_brushOpacity->setEnabled(false);
                m_brushOpacity->setToolTip(unused);
            }
            if (m_brushFlow) {
                m_brushFlow->setEnabled(false);
                m_brushFlow->setToolTip(unused);
            }
            addBlurOptions(static_cast<BlurTool>(variant));
        }

        // The Clone Stamp adds CS6's Aligned and Sample after the brush
        // controls — the stroke is an ordinary one, so everything above applies
        // to it unchanged.
        if (tool == ToolId::CloneStamp) {
            addCloneOptions();
        }

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
    } else if (tool == ToolId::Gradient) {
        if (static_cast<GradientTool>(variant) == GradientTool::PaintBucket) {
            addBucketOptions();
        } else {
            addGradientOptions();
        }
    } else if (tool == ToolId::Pen) {
        addPenOptions(static_cast<PenTool>(variant));
    } else if (tool == ToolId::PathSelect) {
        m_optionsBar->addWidget(new QLabel(
            static_cast<PathSelectTool>(variant) == PathSelectTool::PathSelection
                ? tr("Drag a subpath to move it")
                : tr("Drag an anchor or handle to reshape the path. Alt+drag a handle "
                     "breaks it free of its smooth point"),
            m_optionsBar));
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

void MainWindow::addContentAwareMoveOptions()
{
    // CS6's bar, left to right: combine buttons, Mode, Structure, Color,
    // Sample All Layers, and the transform-on-drop toggle.
    addSelectionModeButtons();

    // Mode: Move relocates the subject and heals the gap; Extend leaves the
    // original, so the subject is lengthened instead.
    m_optionsBar->addWidget(new QLabel(tr("Mode:"), m_optionsBar));
    auto *mode = new QComboBox(m_optionsBar);
    mode->addItem(tr("Move"), false);
    mode->addItem(tr("Extend"), true);
    mode->setCurrentIndex(m_camExtend ? 1 : 0);
    m_optionsBar->addWidget(mode);
    connect(mode, &QComboBox::currentIndexChanged, this, [this, mode](int index) {
        m_camExtend = mode->itemData(index).toBool();
        pushContentAwareMoveOptions();
    });

    m_optionsBar->addSeparator();

    struct Slider {
        QString label;
        int min;
        int max;
        int *value;
        QString tip;
    };
    const Slider sliders[] = {
        {tr("Structure:"), 1, 7, &m_camStructure,
         tr("How strictly the filled area follows the edges around it. Higher matches "
            "larger patches more finely")},
        {tr("Color:"), 0, 10, &m_camColor,
         tr("How far the moved pixels shift toward the colour of their new "
            "surroundings. 0 moves them untouched")},
    };
    for (const Slider &slider : sliders) {
        m_optionsBar->addWidget(new QLabel(slider.label, m_optionsBar));
        auto *spin = new QSpinBox(m_optionsBar);
        spin->setRange(slider.min, slider.max);
        spin->setValue(*slider.value);
        spin->setFixedWidth(52);
        spin->setToolTip(slider.tip);
        spin->setStatusTip(slider.tip);
        m_optionsBar->addWidget(spin);

        int *slot = slider.value;
        connect(spin, &QSpinBox::valueChanged, this, [this, slot](int v) {
            *slot = v;
            pushContentAwareMoveOptions();
        });
    }

    m_optionsBar->addSeparator();

    auto *sampleAll = new QCheckBox(tr("Sample All Layers"), m_optionsBar);
    sampleAll->setChecked(m_camSampleAllLayers);
    sampleAll->setToolTip(tr("Read the pixels to move from the composite rather than the "
                             "active layer alone. The result is still written to the "
                             "active layer"));
    m_optionsBar->addWidget(sampleAll);
    connect(sampleAll, &QCheckBox::toggled, this, [this](bool on) {
        m_camSampleAllLayers = on;
        pushContentAwareMoveOptions();
    });

    // CS6's T: show transform handles on the dropped region. Transforms are not
    // implemented, so this is present for the bar's shape but disabled.
    auto *transform = new QCheckBox(tr("T"), m_optionsBar);
    transform->setEnabled(false);
    transform->setToolTip(tr("Not implemented yet: transform on drop needs the transform "
                             "tools"));
    m_optionsBar->addWidget(transform);

    m_optionsBar->addSeparator();
    m_optionsBar->addWidget(new QLabel(
        tr("Drag to outline a subject, then drag it where it should go"), m_optionsBar));

    pushContentAwareMoveOptions();
}

void MainWindow::pushContentAwareMoveOptions()
{
    m_canvas->setContentAwareMoveOptions(m_camExtend, m_camStructure, m_camColor,
                                        m_camSampleAllLayers);
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

void MainWindow::warnLayerLocked()
{
    // Photoshop's own wording, naming the tool that was refused, with its error
    // icon. A modal alert rather than a status-bar line, because the click the
    // user just made did nothing at all.
    const QString tool = toolVariantName(m_activeTool, m_activeVariant).toLower();
    QMessageBox box(QMessageBox::Critical, tr("PhotoRust"),
                    tr("Could not use the %1 because the layer is locked.").arg(tool),
                    QMessageBox::Ok, this);
    unsqueezeButtons(&box);
    box.exec();
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
    unsqueezeButtons(&box);
    box.exec();
}

void MainWindow::addHealingRegionOptions(HealingType type)
{
    switch (type) {
    case HealingType::Patch:
        addPatchOptions();
        break;

    case HealingType::ContentAwareMove:
        addContentAwareMoveOptions();
        break;

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

void MainWindow::addColorReplaceOptions()
{
    // CS6's bar: Mode, the three Sampling buttons, Limits, Tolerance and
    // Anti-alias. Opacity and Flow have already been added above.
    struct Choice {
        QString label;
        int value;
        QString tip;
    };

    const auto addChoices = [this](const QString &label, const QList<Choice> &choices,
                                   int *slot) {
        m_optionsBar->addWidget(new QLabel(label, m_optionsBar));
        auto *combo = new QComboBox(m_optionsBar);
        for (const Choice &choice : choices) {
            combo->addItem(choice.label, choice.value);
            combo->setItemData(combo->count() - 1, choice.tip, Qt::ToolTipRole);
            if (choice.value == *slot) {
                combo->setCurrentIndex(combo->count() - 1);
            }
        }
        m_optionsBar->addWidget(combo);
        connect(combo, &QComboBox::currentIndexChanged, this, [this, combo, slot](int index) {
            *slot = combo->itemData(index).toInt();
            pushColorReplaceOptions();
        });
    };

    addChoices(tr("Mode:"),
               {{tr("Hue"), int(ReplaceMode::Hue), tr("Replace the hue alone")},
                {tr("Saturation"), int(ReplaceMode::Saturation),
                 tr("Replace how saturated the pixel is")},
                {tr("Color"), int(ReplaceMode::Color),
                 tr("Replace hue and saturation, keeping the pixel's brightness — so "
                    "shading survives")},
                {tr("Luminosity"), int(ReplaceMode::Luminosity),
                 tr("Replace the brightness, keeping the pixel's colour")}},
               &m_replaceMode);

    m_optionsBar->addSeparator();

    addChoices(tr("Sampling:"),
               {{tr("Continuous"), int(ReplaceSampling::Continuous),
                 tr("Re-read the colour under the brush as it moves")},
                {tr("Once"), int(ReplaceSampling::Once),
                 tr("Read the colour where the stroke begins, and keep it")},
                {tr("Background Swatch"), int(ReplaceSampling::BackgroundSwatch),
                 tr("Replace whatever matches the background colour")}},
               &m_replaceSampling);

    addChoices(tr("Limits:"),
               {{tr("Discontiguous"), int(ReplaceLimits::Discontiguous),
                 tr("Every matching pixel under the brush")},
                {tr("Contiguous"), int(ReplaceLimits::Contiguous),
                 tr("Only pixels joined to the one under the cursor")},
                {tr("Find Edges"), int(ReplaceLimits::FindEdges),
                 tr("As contiguous, but stopping at edges so colour does not leak "
                    "across a boundary")}},
               &m_replaceLimits);

    m_optionsBar->addWidget(new QLabel(tr("Tolerance:"), m_optionsBar));
    auto *tolerance = new QSpinBox(m_optionsBar);
    tolerance->setRange(0, 100);
    tolerance->setValue(m_replaceTolerance);
    tolerance->setSuffix(QStringLiteral("%"));
    tolerance->setFixedWidth(64);
    tolerance->setToolTip(tr("How far a pixel may differ from the sampled colour and "
                             "still be replaced"));
    m_optionsBar->addWidget(tolerance);
    connect(tolerance, &QSpinBox::valueChanged, this, [this](int v) {
        m_replaceTolerance = v;
        pushColorReplaceOptions();
    });

    auto *antialias = new QCheckBox(tr("Anti-alias"), m_optionsBar);
    antialias->setChecked(m_replaceAntialias);
    antialias->setToolTip(tr("Soften the edge of the replaced area"));
    m_optionsBar->addWidget(antialias);
    connect(antialias, &QCheckBox::toggled, this, [this](bool on) {
        m_replaceAntialias = on;
        pushColorReplaceOptions();
    });

    pushColorReplaceOptions();
}

void MainWindow::pushColorReplaceOptions()
{
    if (!m_engine) {
        return;
    }
    // CS6 shows Tolerance as a percentage; the engine matches per channel in
    // 0-255.
    const int tolerance = qRound(m_replaceTolerance * 255.0 / 100.0);
    m_engine->setReplaceOptions(m_replaceMode, m_replaceSampling, m_replaceLimits, tolerance,
                                m_replaceAntialias);
}

void MainWindow::addMixerOptions()
{
    // CS6's bar, left to right after the tip: the load swatch with its Load and
    // Clean menu, the two after-each-stroke toggles, the Wet/Load/Mix preset
    // menu, the four sliders themselves, and Sample All Layers.
    m_mixerLoadButton = new QToolButton(m_optionsBar);
    m_mixerLoadButton->setPopupMode(QToolButton::InstantPopup);
    m_mixerLoadButton->setIconSize(QSize(20, 20));
    m_mixerLoadButton->setToolTip(tr("The paint currently on the brush"));

    auto *loadMenu = new QMenu(m_mixerLoadButton);
    QAction *loadPaint = loadMenu->addAction(tr("Load Brush"));
    loadPaint->setToolTip(tr("Fill the brush with the foreground colour"));
    QAction *cleanBrush = loadMenu->addAction(tr("Clean Brush"));
    cleanBrush->setToolTip(tr("Empty the brush, so a wet one smears without adding "
                              "colour of its own"));
    m_mixerLoadButton->setMenu(loadMenu);
    m_optionsBar->addWidget(m_mixerLoadButton);
    connect(loadPaint, &QAction::triggered, this, [this] {
        if (m_engine) {
            m_engine->loadMixerBrush();
            refreshMixerLoadSwatch();
        }
    });
    connect(cleanBrush, &QAction::triggered, this, [this] {
        if (m_engine) {
            m_engine->cleanMixerBrush();
            refreshMixerLoadSwatch();
        }
    });
    refreshMixerLoadSwatch();

    struct Toggle {
        QString label;
        QString tip;
        bool *value;
    };
    const Toggle toggles[] = {
        {tr("Load After Stroke"), tr("Refill the brush from the foreground colour when "
                                     "each stroke ends"),
         &m_mixerLoadAfterStroke},
        {tr("Clean After Stroke"), tr("Empty the brush when each stroke ends, so the "
                                      "next one starts with no paint of its own"),
         &m_mixerCleanAfterStroke},
    };
    for (const Toggle &toggle : toggles) {
        auto *button = new QToolButton(m_optionsBar);
        button->setCheckable(true);
        button->setAutoRaise(true);
        button->setText(toggle.label);
        button->setToolTip(toggle.tip);
        button->setStatusTip(toggle.tip);
        button->setChecked(*toggle.value);
        m_optionsBar->addWidget(button);

        bool *slot = toggle.value;
        connect(button, &QToolButton::toggled, this, [this, slot](bool on) {
            *slot = on;
            pushMixerOptions();
        });
    }

    m_optionsBar->addSeparator();

    m_mixerPresetCombo = new QComboBox(m_optionsBar);
    for (const MixerPreset &preset : mixerPresets()) {
        m_mixerPresetCombo->addItem(tr(preset.name));
    }
    m_mixerPresetCombo->setToolTip(tr("Ready-made Wet, Load and Mix combinations"));
    m_optionsBar->addWidget(m_mixerPresetCombo);
    syncMixerPresetCombo();
    connect(m_mixerPresetCombo, &QComboBox::activated, this, [this](int index) {
        const QList<MixerPreset> &presets = mixerPresets();
        if (index < 0 || index >= presets.size() || presets[index].wet < 0) {
            // Custom carries no values: choosing it changes nothing, it is only
            // what the menu falls back to once a slider is moved by hand.
            return;
        }
        m_mixerWet = presets[index].wet;
        m_mixerLoad = presets[index].load;
        m_mixerMix = presets[index].mix;
        // The sliders are rebuilt from the new values rather than updated one by
        // one, which keeps this from having to hold three more pointers.
        populateOptionsBar(m_activeTool, m_activeVariant);
    });

    struct Field {
        QString label;
        QString tip;
        int *value;
    };
    const Field fields[] = {
        {tr("Wet:"), tr("How wet the canvas is: at 0 the paint sits on top like an "
                        "ordinary brush, higher and the colour already there joins in"),
         &m_mixerWet},
        {tr("Load:"), tr("How much paint the brush holds. It runs down as the stroke "
                         "goes, and a wet brush that has run out only smears"),
         &m_mixerLoad},
        {tr("Mix:"), tr("The balance between the canvas colour and the brush's own: at "
                        "100% the stroke is a pure smear"),
         &m_mixerMix},
        {tr("Flow:"), tr("How fast each dab deposits"), &m_mixerFlow},
    };
    QSpinBox *spins[4] = {};
    int index = 0;
    for (const Field &field : fields) {
        m_optionsBar->addWidget(new QLabel(field.label, m_optionsBar));
        auto *spin = new QSpinBox(m_optionsBar);
        spin->setRange(0, 100);
        spin->setValue(*field.value);
        spin->setSuffix(QStringLiteral("%"));
        spin->setFixedWidth(64);
        spin->setToolTip(field.tip);
        m_optionsBar->addWidget(spin);
        spins[index++] = spin;

        int *slot = field.value;
        connect(spin, &QSpinBox::valueChanged, this, [this, slot](int v) {
            *slot = v;
            pushMixerOptions();
            syncMixerPresetCombo();
        });
    }

    // Load and Mix have no say while the canvas is dry: a dry brush paints its
    // own paint, never runs out and has nothing to mix with. CS6 greys them out
    // rather than letting them look effective.
    const auto followWet = [spins](int wet) {
        spins[1]->setEnabled(wet > 0);
        spins[2]->setEnabled(wet > 0);
    };
    followWet(m_mixerWet);
    connect(spins[0], &QSpinBox::valueChanged, this,
            [followWet](int wet) { followWet(wet); });

    m_optionsBar->addSeparator();

    auto *sampleAll = new QCheckBox(tr("Sample All Layers"), m_optionsBar);
    sampleAll->setChecked(m_mixerSampleAllLayers);
    sampleAll->setToolTip(tr("Pick colour up from every visible layer. The paint still "
                             "lands on the active one"));
    m_optionsBar->addWidget(sampleAll);
    connect(sampleAll, &QCheckBox::toggled, this, [this](bool on) {
        m_mixerSampleAllLayers = on;
        pushMixerOptions();
    });

    pushMixerOptions();
}

void MainWindow::pushMixerOptions()
{
    if (!m_engine) {
        return;
    }
    m_engine->setMixerOptions(m_mixerWet, m_mixerLoad, m_mixerMix, m_mixerFlow,
                              m_mixerSampleAllLayers, m_mixerLoadAfterStroke,
                              m_mixerCleanAfterStroke);
}

void MainWindow::refreshMixerLoadSwatch()
{
    if (!m_mixerLoadButton || !m_engine) {
        return;
    }
    const QColor paint = m_engine->mixerLoadColor();

    // A clean brush is drawn as the checkerboard alone, so "no paint" reads
    // differently from "loaded with white".
    QPixmap swatch(20, 20);
    QPainter painter(&swatch);
    const QColor checks[2] = {QColor(0xcc, 0xcc, 0xcc), QColor(0xff, 0xff, 0xff)};
    for (int y = 0; y < 20; y += 5) {
        for (int x = 0; x < 20; x += 5) {
            painter.fillRect(x, y, 5, 5, checks[((x / 5) + (y / 5)) % 2]);
        }
    }
    painter.fillRect(swatch.rect(), paint);
    painter.setPen(QColor(0x1a, 0x1a, 0x1a));
    painter.drawRect(0, 0, 19, 19);
    painter.end();

    m_mixerLoadButton->setIcon(QIcon(swatch));
}

void MainWindow::syncMixerPresetCombo()
{
    if (!m_mixerPresetCombo) {
        return;
    }
    const QList<MixerPreset> &presets = mixerPresets();
    int match = 0; // Custom, unless one of the presets matches exactly.
    for (int i = 0; i < presets.size(); ++i) {
        if (presets[i].wet == m_mixerWet && presets[i].load == m_mixerLoad
            && presets[i].mix == m_mixerMix) {
            match = i;
            break;
        }
    }
    const QSignalBlocker blocker(m_mixerPresetCombo);
    m_mixerPresetCombo->setCurrentIndex(match);
}

void MainWindow::addToneOptions(ToneTool tool)
{
    m_optionsBar->addSeparator();

    const bool sponge = tool == ToneTool::Sponge;

    // Dodge and Burn choose a tonal band; the Sponge chooses a direction. CS6
    // puts each in the same place on the bar.
    m_optionsBar->addWidget(new QLabel(sponge ? tr("Mode:") : tr("Range:"), m_optionsBar));
    auto *choice = new QComboBox(m_optionsBar);
    struct Entry {
        QString label;
        int value;
        QString tip;
    };
    const QList<Entry> entries = sponge
        ? QList<Entry>{{tr("Desaturate"), int(SpongeMode::Desaturate),
                        tr("Drain colour toward grey")},
                       {tr("Saturate"), int(SpongeMode::Saturate),
                        tr("Lift colour away from grey")}}
        : QList<Entry>{{tr("Shadows"), int(ToneRange::Shadows),
                        tr("Work hardest on the dark end of the scale")},
                       {tr("Midtones"), int(ToneRange::Midtones),
                        tr("Work hardest on the middle of the scale")},
                       {tr("Highlights"), int(ToneRange::Highlights),
                        tr("Work hardest on the bright end of the scale")}};
    for (const Entry &entry : entries) {
        choice->addItem(entry.label, entry.value);
        choice->setItemData(choice->count() - 1, entry.tip, Qt::ToolTipRole);
        if (entry.value == (sponge ? m_spongeMode : m_toneRange)) {
            choice->setCurrentIndex(choice->count() - 1);
        }
    }
    m_optionsBar->addWidget(choice);
    connect(choice, &QComboBox::currentIndexChanged, this, [this, choice, sponge](int index) {
        (sponge ? m_spongeMode : m_toneRange) = choice->itemData(index).toInt();
        pushToneOptions();
    });

    // One number under two names, exactly as CS6 labels it.
    m_optionsBar->addWidget(new QLabel(sponge ? tr("Flow:") : tr("Exposure:"), m_optionsBar));
    auto *amount = new QSpinBox(m_optionsBar);
    amount->setRange(0, 100);
    amount->setValue(m_toneAmount);
    amount->setSuffix(QStringLiteral("%"));
    amount->setFixedWidth(64);
    amount->setToolTip(sponge
                           ? tr("How much colour each dab moves. Dwelling on one spot goes "
                                "on moving it")
                           : tr("How much each dab lightens or darkens. Dwelling on one "
                                "spot goes on working it"));
    m_optionsBar->addWidget(amount);
    connect(amount, &QSpinBox::valueChanged, this, [this](int v) {
        m_toneAmount = v;
        pushToneOptions();
    });

    m_optionsBar->addSeparator();

    if (sponge) {
        auto *vibrance = new QCheckBox(tr("Vibrance"), m_optionsBar);
        vibrance->setChecked(m_toneVibrance);
        vibrance->setToolTip(tr("Ease off on colour that is already vivid, so the flat "
                                "parts of an image lift without the vivid parts clipping"));
        m_optionsBar->addWidget(vibrance);
        connect(vibrance, &QCheckBox::toggled, this, [this](bool on) {
            m_toneVibrance = on;
            pushToneOptions();
        });
    } else {
        auto *protect = new QCheckBox(tr("Protect Tones"), m_optionsBar);
        protect->setChecked(m_toneProtectTones);
        protect->setToolTip(tr("Change brightness and keep the pixel's own colour, rather "
                               "than scaling its channels — which is what bleaches a "
                               "dodge and muddies a burn"));
        m_optionsBar->addWidget(protect);
        connect(protect, &QCheckBox::toggled, this, [this](bool on) {
            m_toneProtectTones = on;
            pushToneOptions();
        });
    }

    pushToneOptions();
}

void MainWindow::pushToneOptions()
{
    if (!m_engine) {
        return;
    }
    m_engine->setToneOptions(m_toneAmount, m_toneRange, m_spongeMode, m_toneProtectTones,
                             m_toneVibrance);
}

void MainWindow::addBlurOptions(BlurTool tool)
{
    m_optionsBar->addSeparator();

    m_optionsBar->addWidget(new QLabel(tr("Mode:"), m_optionsBar));
    auto *mode = new QComboBox(m_optionsBar);
    // CS6 offers a cut-down list here, not all 27: see `blurModes()`.
    for (const auto &entry : blurModes()) {
        mode->addItem(entry.first, entry.second);
        if (entry.second == m_blurMode) {
            mode->setCurrentIndex(mode->count() - 1);
        }
    }
    m_optionsBar->addWidget(mode);
    connect(mode, &QComboBox::currentIndexChanged, this, [this, mode](int index) {
        m_blurMode = mode->itemData(index).toInt();
        pushBlurOptions();
    });

    m_optionsBar->addWidget(new QLabel(tr("Strength:"), m_optionsBar));
    auto *strength = new QSpinBox(m_optionsBar);
    strength->setRange(0, 100);
    strength->setValue(m_blurStrength);
    strength->setSuffix(QStringLiteral("%"));
    strength->setFixedWidth(64);
    switch (tool) {
    case BlurTool::Blur:
        strength->setToolTip(tr("How much each dab softens. Dwelling on one spot goes on "
                                "softening it"));
        break;
    case BlurTool::Sharpen:
        strength->setToolTip(tr("How much each dab sharpens. Dwelling on one spot goes on "
                                "sharpening it"));
        break;
    case BlurTool::Smudge:
        strength->setToolTip(tr("How much of what the finger is carrying each dab lays "
                                "down. At 100% it drags the pixels it started on the "
                                "whole way"));
        break;
    }
    m_optionsBar->addWidget(strength);
    connect(strength, &QSpinBox::valueChanged, this, [this](int v) {
        m_blurStrength = v;
        pushBlurOptions();
    });

    m_optionsBar->addSeparator();

    auto *sampleAll = new QCheckBox(tr("Sample All Layers"), m_optionsBar);
    sampleAll->setChecked(m_blurSampleAllLayers);
    sampleAll->setToolTip(tr("Work on what the whole visible image shows. The result "
                             "still lands on the active layer"));
    m_optionsBar->addWidget(sampleAll);
    connect(sampleAll, &QCheckBox::toggled, this, [this](bool on) {
        m_blurSampleAllLayers = on;
        pushBlurOptions();
    });

    // Each of the two has one checkbox of its own, which CS6 shows only for it.
    if (tool == BlurTool::Sharpen) {
        auto *protect = new QCheckBox(tr("Protect Detail"), m_optionsBar);
        protect->setChecked(m_blurProtectDetail);
        protect->setToolTip(tr("Hold each pixel inside the range its own neighbours span, "
                               "so repeated passes cannot throw haloes or blown speckle"));
        m_optionsBar->addWidget(protect);
        connect(protect, &QCheckBox::toggled, this, [this](bool on) {
            m_blurProtectDetail = on;
            pushBlurOptions();
        });
    } else if (tool == BlurTool::Smudge) {
        auto *finger = new QCheckBox(tr("Finger Painting"), m_optionsBar);
        finger->setChecked(m_blurFingerPainting);
        finger->setToolTip(tr("Start each stroke with the foreground colour on the finger, "
                              "so it drags paint in rather than what was already there"));
        m_optionsBar->addWidget(finger);
        connect(finger, &QCheckBox::toggled, this, [this](bool on) {
            m_blurFingerPainting = on;
            pushBlurOptions();
        });
    }

    pushBlurOptions();
}

void MainWindow::pushBlurOptions()
{
    if (!m_engine) {
        return;
    }
    m_engine->setFocusOptions(m_blurStrength, m_blurMode, m_blurSampleAllLayers,
                              m_blurProtectDetail, m_blurFingerPainting);
}

void MainWindow::addBucketOptions()
{
    // CS6's bar: Fill, Mode, Opacity, Tolerance, then Anti-alias, Contiguous and
    // All Layers.
    m_optionsBar->addWidget(new QLabel(tr("Fill:"), m_optionsBar));
    auto *fill = new QComboBox(m_optionsBar);
    fill->addItem(tr("Foreground"), int(BucketFill::Foreground));
    fill->addItem(tr("Pattern"), int(BucketFill::Pattern));
    // Patterns are a sub-system of their own and there are none to fill with, so
    // the entry is listed for the bar's shape and disabled — the same treatment
    // the Patch tool's Use Pattern button gets.
    fill->setItemData(1, false, Qt::UserRole - 1);
    fill->setItemData(1, tr("Not implemented yet: there are no patterns to fill with"),
                      Qt::ToolTipRole);
    m_optionsBar->addWidget(fill);

    m_optionsBar->addWidget(new QLabel(tr("Mode:"), m_optionsBar));
    auto *mode = new QComboBox(m_optionsBar);
    mode->addItems(m_engine->blendModeNames().split(QLatin1Char('\n'), Qt::SkipEmptyParts));
    mode->setCurrentIndex(m_bucketMode);
    m_optionsBar->addWidget(mode);
    connect(mode, &QComboBox::currentIndexChanged, this, [this](int index) {
        m_bucketMode = index;
        pushBucketOptions();
    });

    struct Field {
        QString label;
        QString suffix;
        int max;
        QString tip;
        int *value;
    };
    const Field fields[] = {
        {tr("Opacity:"), QStringLiteral("%"), 100, tr("How strongly the fill is applied"),
         &m_bucketOpacity},
        {tr("Tolerance:"), QString(), 255, tr("How far a pixel may differ from the one "
                                              "clicked and still be filled — the same "
                                              "scale as the Magic Wand's"),
         &m_bucketTolerance},
    };
    for (const Field &field : fields) {
        m_optionsBar->addWidget(new QLabel(field.label, m_optionsBar));
        auto *spin = new QSpinBox(m_optionsBar);
        spin->setRange(0, field.max);
        spin->setValue(*field.value);
        spin->setSuffix(field.suffix);
        spin->setFixedWidth(64);
        spin->setToolTip(field.tip);
        m_optionsBar->addWidget(spin);

        int *slot = field.value;
        connect(spin, &QSpinBox::valueChanged, this, [this, slot](int v) {
            *slot = v;
            pushBucketOptions();
        });
    }

    m_optionsBar->addSeparator();

    struct Toggle {
        QString label;
        QString tip;
        bool *value;
    };
    const Toggle toggles[] = {
        {tr("Anti-alias"), tr("Soften the edge of the filled area"), &m_bucketAntialias},
        {tr("Contiguous"), tr("Fill only the area joined to the pixel clicked. Off, every "
                              "matching pixel in the layer is filled"),
         &m_bucketContiguous},
        {tr("All Layers"), tr("Decide what matches from the whole visible image. The fill "
                              "still lands on the active layer"),
         &m_bucketAllLayers},
    };
    for (const Toggle &toggle : toggles) {
        auto *box = new QCheckBox(toggle.label, m_optionsBar);
        box->setChecked(*toggle.value);
        box->setToolTip(toggle.tip);
        m_optionsBar->addWidget(box);

        bool *slot = toggle.value;
        connect(box, &QCheckBox::toggled, this, [this, slot](bool on) {
            *slot = on;
            pushBucketOptions();
        });
    }

    m_optionsBar->addSeparator();
    m_optionsBar->addWidget(
        new QLabel(tr("Click to fill an area with the foreground colour"), m_optionsBar));

    pushBucketOptions();
}

void MainWindow::pushBucketOptions()
{
    if (!m_engine) {
        return;
    }
    m_engine->setBucketOptions(m_bucketMode, m_bucketOpacity, m_bucketTolerance,
                               m_bucketAntialias, m_bucketContiguous, m_bucketAllLayers);
}

void MainWindow::addGradientOptions()
{
    // CS6's bar: the gradient swatch and its preset menu, the five type buttons,
    // Mode, Opacity, then Reverse, Dither and Transparency.
    m_gradientSwatch = new QToolButton(m_optionsBar);
    m_gradientSwatch->setObjectName(QStringLiteral("gradientSwatch"));
    m_gradientSwatch->setPopupMode(QToolButton::InstantPopup);
    m_gradientSwatch->setIconSize(QSize(64, 16));
    m_gradientSwatch->setToolTip(tr("Click to choose a gradient"));

    auto *presets = new QMenu(m_gradientSwatch);
    const QStringList names =
        m_engine->gradientPresetNames().split(QLatin1Char('\n'), Qt::SkipEmptyParts);
    if (m_gradientPreset.isEmpty() && !names.isEmpty()) {
        m_gradientPreset = names.first();
    }
    for (const QString &name : names) {
        // The preview comes from the engine, so what the menu shows and what the
        // tool paints cannot drift apart.
        QAction *action = presets->addAction(QIcon(QPixmap::fromImage(
                                                m_engine->gradientPreview(name, 64, 16))),
                                            name);
        action->setCheckable(true);
        action->setChecked(name == m_gradientPreset);
        connect(action, &QAction::triggered, this, [this, name] {
            m_gradientPreset = name;
            refreshGradientSwatch();
            pushGradientOptions();
        });
    }
    m_gradientSwatch->setMenu(presets);
    m_optionsBar->addWidget(m_gradientSwatch);
    refreshGradientSwatch();

    m_optionsBar->addSeparator();

    auto *types = new QButtonGroup(m_optionsBar);
    types->setExclusive(true);
    for (GradientType type : {GradientType::Linear, GradientType::Radial,
                              GradientType::Angle, GradientType::Reflected,
                              GradientType::Diamond}) {
        auto *button = new QToolButton(m_optionsBar);
        button->setCheckable(true);
        button->setAutoRaise(true);
        button->setIconSize(QSize(20, 20));
        button->setIcon(ToolIcons::fromSvgBody(ToolIcons::gradientTypeSvg(type),
                                               QColor(0x30, 0x30, 0x30)));
        button->setToolTip(gradientTypeName(type));
        button->setStatusTip(gradientTypeName(type));
        button->setChecked(int(type) == m_gradientType);
        types->addButton(button, int(type));
        m_optionsBar->addWidget(button);
    }
    connect(types, &QButtonGroup::idClicked, this, [this](int id) {
        m_gradientType = id;
        pushGradientOptions();
    });

    m_optionsBar->addSeparator();

    m_optionsBar->addWidget(new QLabel(tr("Mode:"), m_optionsBar));
    auto *mode = new QComboBox(m_optionsBar);
    // The engine owns the blend-mode list and its order, exactly as it does for
    // the Layers panel.
    mode->addItems(m_engine->blendModeNames().split(QLatin1Char('\n'), Qt::SkipEmptyParts));
    mode->setCurrentIndex(m_gradientMode);
    m_optionsBar->addWidget(mode);
    connect(mode, &QComboBox::currentIndexChanged, this, [this](int index) {
        m_gradientMode = index;
        pushGradientOptions();
    });

    m_optionsBar->addWidget(new QLabel(tr("Opacity:"), m_optionsBar));
    auto *opacity = new QSpinBox(m_optionsBar);
    opacity->setRange(0, 100);
    opacity->setValue(m_gradientOpacity);
    opacity->setSuffix(QStringLiteral("%"));
    opacity->setFixedWidth(64);
    m_optionsBar->addWidget(opacity);
    connect(opacity, &QSpinBox::valueChanged, this, [this](int v) {
        m_gradientOpacity = v;
        pushGradientOptions();
    });

    m_optionsBar->addSeparator();

    struct Toggle {
        QString label;
        QString tip;
        bool *value;
    };
    const Toggle toggles[] = {
        {tr("Reverse"), tr("Draw the ramp end to start"), &m_gradientReverse},
        {tr("Dither"), tr("Break up banding with a little noise"), &m_gradientDither},
        {tr("Transparency"), tr("Honour the gradient's own opacity. Off, it is drawn "
                                "solid"),
         &m_gradientTransparency},
    };
    for (const Toggle &toggle : toggles) {
        auto *box = new QCheckBox(toggle.label, m_optionsBar);
        box->setChecked(*toggle.value);
        box->setToolTip(toggle.tip);
        m_optionsBar->addWidget(box);

        bool *slot = toggle.value;
        connect(box, &QCheckBox::toggled, this, [this, slot](bool on) {
            *slot = on;
            pushGradientOptions();
        });
    }

    m_optionsBar->addSeparator();
    m_optionsBar->addWidget(new QLabel(
        tr("Drag to set the gradient's direction and length    Shift constrains the angle"),
        m_optionsBar));

    pushGradientOptions();
}

void MainWindow::pushGradientOptions()
{
    if (!m_engine) {
        return;
    }
    m_engine->setGradientOptions(m_gradientPreset, m_gradientType, m_gradientMode,
                                 m_gradientOpacity, m_gradientReverse, m_gradientDither,
                                 m_gradientTransparency);
}

void MainWindow::refreshGradientSwatch()
{
    if (!m_gradientSwatch || !m_engine) {
        return;
    }
    m_gradientSwatch->setIcon(
        QIcon(QPixmap::fromImage(m_engine->gradientPreview(m_gradientPreset, 64, 16))));
    m_gradientSwatch->setToolTip(tr("%1 — click to choose a gradient").arg(m_gradientPreset));
}

void MainWindow::addPenOptions(PenTool tool)
{
    m_optionsBar->addSeparator();

    if (tool == PenTool::Pen) {
        auto *autoAddDelete = new QCheckBox(tr("Auto Add/Delete"), m_optionsBar);
        autoAddDelete->setChecked(m_penAutoAddDelete);
        autoAddDelete->setToolTip(tr("Hovering the finished part of the path adds an anchor "
                                     "over a segment, or removes one under the cursor, "
                                     "without switching tools"));
        m_optionsBar->addWidget(autoAddDelete);
        connect(autoAddDelete, &QCheckBox::toggled, this, [this](bool on) {
            m_penAutoAddDelete = on;
            pushPenOptions();
        });

        auto *rubberBand = new QCheckBox(tr("Rubber Band"), m_optionsBar);
        rubberBand->setChecked(m_penRubberBand);
        rubberBand->setToolTip(tr("Preview the next segment from the last anchor to the "
                                  "cursor, before it is placed"));
        m_optionsBar->addWidget(rubberBand);
        connect(rubberBand, &QCheckBox::toggled, this, [this](bool on) {
            m_penRubberBand = on;
            pushPenOptions();
        });

        m_optionsBar->addSeparator();
        m_optionsBar->addWidget(new QLabel(
            tr("Click for a corner, drag for a curve    Click the start to close    "
               "Enter or double-click to finish"),
            m_optionsBar));
    } else if (tool == PenTool::FreeformPen) {
        m_optionsBar->addWidget(new QLabel(tr("Curve Fit:"), m_optionsBar));
        auto *fit = new QDoubleSpinBox(m_optionsBar);
        fit->setRange(0.5, 10.0);
        fit->setSingleStep(0.5);
        fit->setSuffix(tr(" px"));
        fit->setValue(m_freeformTolerance);
        fit->setFixedWidth(72);
        fit->setToolTip(tr("How closely the fitted path follows the drag. Lower values keep "
                           "more anchors"));
        m_optionsBar->addWidget(fit);
        connect(fit, &QDoubleSpinBox::valueChanged, this, [this](double v) {
            m_freeformTolerance = v;
            m_canvas->setFreeformPenTolerance(v);
        });

        // CS6's Magnetic checkbox reuses the Magnetic Lasso's edge-snapping.
        // Wiring it up for a *path* — anchors that snap to edges rather than a
        // selection mask — is a real chunk of its own and is not included
        // here.
        auto *magnetic = new QCheckBox(tr("Magnetic"), m_optionsBar);
        magnetic->setEnabled(false);
        magnetic->setToolTip(tr("Not implemented yet"));
        m_optionsBar->addWidget(magnetic);

        m_optionsBar->addSeparator();
        m_optionsBar->addWidget(new QLabel(
            tr("Drag to draw freehand    Drag back to the start to close"), m_optionsBar));
    } else {
        const QString hint = tool == PenTool::AddAnchor
            ? tr("Click a segment of the active path to add an anchor there")
            : tool == PenTool::DeleteAnchor
                ? tr("Click an anchor on the active path to remove it")
                : tr("Click a smooth anchor to make it a corner    Drag a corner to pull out "
                     "new handles    Drag a handle to break it free");
        m_optionsBar->addWidget(new QLabel(hint, m_optionsBar));
    }

    pushPenOptions();
}

void MainWindow::pushPenOptions()
{
    m_canvas->setPenOptions(m_penAutoAddDelete, m_penRubberBand);
    m_canvas->setFreeformPenTolerance(m_freeformTolerance);
}

void MainWindow::addCloneOptions()
{
    m_optionsBar->addSeparator();

    auto *aligned = new QCheckBox(tr("Aligned"), m_optionsBar);
    aligned->setChecked(m_cloneAligned);
    aligned->setToolTip(tr("Keep the sample point travelling with the cursor across "
                           "strokes. Off, every stroke starts copying from the source "
                           "point again"));
    m_optionsBar->addWidget(aligned);
    connect(aligned, &QCheckBox::toggled, this, [this](bool on) {
        m_cloneAligned = on;
        pushCloneOptions();
    });

    m_optionsBar->addWidget(new QLabel(tr("Sample:"), m_optionsBar));
    auto *sample = new QComboBox(m_optionsBar);
    struct Choice {
        QString label;
        CloneSampling value;
        QString tip;
    };
    const Choice choices[] = {
        {tr("Current Layer"), CloneSampling::CurrentLayer,
         tr("Copy from the active layer alone")},
        {tr("Current & Below"), CloneSampling::CurrentAndBelow,
         tr("Copy from the active layer composited with everything beneath it")},
        {tr("All Layers"), CloneSampling::AllLayers,
         tr("Copy from the whole visible image")},
    };
    for (const Choice &choice : choices) {
        sample->addItem(choice.label, int(choice.value));
        sample->setItemData(sample->count() - 1, choice.tip, Qt::ToolTipRole);
        if (int(choice.value) == m_cloneSampling) {
            sample->setCurrentIndex(sample->count() - 1);
        }
    }
    m_optionsBar->addWidget(sample);
    connect(sample, &QComboBox::currentIndexChanged, this, [this, sample](int index) {
        m_cloneSampling = sample->itemData(index).toInt();
        pushCloneOptions();
    });

    m_optionsBar->addSeparator();
#ifdef Q_OS_MACOS
    m_optionsBar->addWidget(new QLabel(
        tr("Option+click to set the source point, then drag to clone"), m_optionsBar));
#else
    m_optionsBar->addWidget(new QLabel(
        tr("Alt+click to set the source point, then drag to clone"), m_optionsBar));
#endif

    pushCloneOptions();
}

void MainWindow::pushCloneOptions()
{
    m_canvas->setCloneOptions(m_cloneAligned, static_cast<CloneSampling>(m_cloneSampling));
}

void MainWindow::warnCloneSourceRequired()
{
    // Photoshop's own wording, down to the parenthesis.
#ifdef Q_OS_MACOS
    const QString message = tr("Could not use the clone stamp because the area to clone "
                               "has not been defined (Option-click to define a source "
                               "point).");
#else
    const QString message = tr("Could not use the clone stamp because the area to clone "
                               "has not been defined (Alt-click to define a source "
                               "point).");
#endif
    QMessageBox box(QMessageBox::Critical, tr("PhotoRust"), message, QMessageBox::Ok, this);
    unsqueezeButtons(&box);
    box.exec();
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
    if (!m_engine) {
        return;
    }
    // Opacity and Flow live on the bar and only exist while a painting tool is
    // active; size and hardness are kept here, since the picker that edits them
    // is created once and outlives any one options bar.
    const int opacity = m_brushOpacity ? m_brushOpacity->value() : 100;
    const int flow = m_brushFlow ? m_brushFlow->value() : 100;
    // Spacing belongs to the tip, so it comes from the picker's preset rather
    // than a fixed value — a spatter brush needs a much wider step than a round
    // one to read as spatter instead of a solid line.
    const int spacing = m_brushPicker ? m_brushPicker->current().spacing : 25;
    m_engine->setBrush(float(m_brushSizeValue), m_brushHardnessValue, opacity, flow, spacing);
    // The Pencil paints aliased; every other tool in the family antialiases.
    m_engine->setBrushAntialias(!m_pencilMode);
    m_engine->setAutoErase(m_pencilMode && m_autoErase);
}

BrushPresetPicker *MainWindow::brushPicker()
{
    // Built on first use and kept: it holds the current tip, so it must not be
    // recreated with the options bar.
    if (!m_brushPicker) {
        m_brushPicker = new BrushPresetPicker(m_engine, this);
        connect(m_brushPicker, &BrushPresetPicker::tipChanged, this,
                [this](const BrushPresetPicker::Preset &preset) {
                    m_brushSizeValue = preset.size;
                    m_brushHardnessValue = preset.hardness;
                    refreshBrushTipButton();
                    // The picker has already pushed the tip shape; this adds the
                    // options bar's Opacity and Flow on top.
                    pushBrushSettings();
                });
    }
    return m_brushPicker;
}

void MainWindow::refreshBrushTipButton()
{
    if (!m_brushTipButton) {
        return;
    }
    m_brushTipButton->setIcon(QIcon(brushPicker()->tipPreview(20)));
    m_brushTipButton->setText(QString::number(int(m_brushSizeValue)));
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

    // CS6 tabs Layers, Channels and Paths together at the bottom right. There
    // is no Channels panel yet, so Paths joins Layers alone.
    m_pathsPanel = new PathsPanel(m_engine, this);
    QDockWidget *pathsDock = addPanel(tr("Paths"), m_pathsPanel, Qt::RightDockWidgetArea);

    // Stack Color/Swatches into one tabbed group, as CS6 ships them.
    if (QDockWidget *colorDock =
            findChild<QDockWidget *>(tr("Color") + QStringLiteral("Dock"))) {
        tabifyDockWidget(colorDock, swatchesDock);
        if (m_infoDock) {
            tabifyDockWidget(swatchesDock, m_infoDock);
        }
        colorDock->raise();
    }
    tabifyDockWidget(layersDock, pathsDock);
    layersDock->raise();

    resizeDocks({historyDock, layersDock}, {220, 380}, Qt::Vertical);

    connect(m_layersPanel, &LayersPanel::documentChanged,
            this, &MainWindow::onDocumentChanged);
    connect(m_historyPanel, &HistoryPanel::documentChanged,
            this, &MainWindow::onDocumentChanged);
    connect(m_pathsPanel, &PathsPanel::documentChanged,
            this, &MainWindow::onDocumentChanged);
}

void MainWindow::createStatusBar()
{
    // Editable, as Photoshop's is: type a percentage and press Enter.
    m_statusZoom = new QLineEdit(QStringLiteral("100%"), this);
    m_statusZoom->setObjectName(QStringLiteral("statusZoom"));
    m_statusZoom->setFixedWidth(58);
    m_statusZoom->setAlignment(Qt::AlignRight);
    m_statusZoom->setToolTip(tr("Zoom level. Type a percentage and press Enter."));
    statusBar()->addWidget(m_statusZoom);

    connect(m_statusZoom, &QLineEdit::returnPressed, this, &MainWindow::applyTypedZoom);
    // Clicking away without pressing Enter puts the real value back, rather than
    // leaving a half-typed number sitting there looking authoritative.
    connect(m_statusZoom, &QLineEdit::editingFinished, this, [this] {
        if (!m_statusZoom->hasFocus()) {
            onZoomChanged(m_canvas->zoom());
        }
    });

    m_statusDocSize = new QLabel(this);
    m_statusDocSize->setMinimumWidth(140);
    statusBar()->addWidget(m_statusDocSize);

    m_statusPosition = new QLabel(this);
    m_statusPosition->setMinimumWidth(120);
    statusBar()->addPermanentWidget(m_statusPosition);
}

void MainWindow::refreshDocumentTabs()
{
    if (!m_engine || !m_documentTabs) {
        return;
    }
    // Rebuilding emits currentChanged, which would bounce straight back into
    // the engine and fight with what it just told us.
    const QSignalBlocker blocker(m_documentTabs);

    const int count = m_engine->documentCount();
    while (m_documentTabs->count() > count) {
        m_documentTabs->removeTab(m_documentTabs->count() - 1);
    }
    while (m_documentTabs->count() < count) {
        m_documentTabs->addTab(QString());
    }
    for (int i = 0; i < count; ++i) {
        m_documentTabs->setTabText(i, m_engine->documentTitleAt(i));
        m_documentTabs->setTabToolTip(i, m_engine->documentTitleAt(i));
    }
    m_documentTabs->setCurrentIndex(m_engine->activeDocument());
}

void MainWindow::onTabSelected(int index)
{
    if (m_engine && index >= 0) {
        m_engine->setActiveDocument(index);
    }
}

void MainWindow::onTabCloseRequested(int index)
{
    if (!m_engine || index < 0) {
        return;
    }
    // Switch to the tab first, so an unsaved-changes prompt names that document
    // and Save writes the right one.
    if (m_engine->documentModifiedAt(index)) {
        m_engine->setActiveDocument(index);
        refreshDocumentTabs();
        if (!confirmDiscardChanges()) {
            return;
        }
        index = m_engine->activeDocument();
    }

    if (!m_engine->closeDocument(index)) {
        statusBar()->showMessage(tr("The last document cannot be closed."), 4000);
    }
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
    connect(m_engine, &Engine::pathsChanged, m_pathsPanel, &PathsPanel::refresh);
    // The Pen and Path Selection tools edit the active path directly, with no
    // C++-side echo of their own — this is what repaints the overlay.
    connect(m_engine, &Engine::pathsChanged, m_canvas, [this] { m_canvas->update(); });
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
    connect(m_engine, &Engine::documentTitleChanged, this, &MainWindow::refreshDocumentTabs);
    connect(m_engine, &Engine::documentsChanged, this, &MainWindow::refreshDocumentTabs);
    // A document swap changes everything downstream, so treat it like a reload.
    connect(m_engine, &Engine::documentsChanged, this, &MainWindow::onDocumentChanged);
    // Only a *swap* invalidates the Alt-clicked sample points, which is why this
    // hangs off the engine's signal and not `onDocumentChanged` — the panels
    // raise that one for every edit, and adding a layer must not silently
    // forget where the Clone Stamp was told to sample from.
    connect(m_engine, &Engine::documentsChanged, m_canvas,
            &CanvasView::forgetSampleSources);
}

// ---------------------------------------------------------------- commands --

void MainWindow::newDocument()
{
    // No prompt: this opens a tab alongside whatever is already open rather
    // than replacing it, so there is nothing to lose.
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
    // Opens in its own tab, so nothing already open is at risk.
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
    // Aliasing follows the brush variant, and has to be set before the options
    // bar pushes the rest of the brush settings.
    m_pencilMode = tool == ToolId::Brush && brushIsPencil(static_cast<BrushType>(variant));
    m_canvas->setReplaceMode(tool == ToolId::Brush
                             && brushReplacesColor(static_cast<BrushType>(variant)));
    m_canvas->setMixerMode(tool == ToolId::Brush
                           && brushMixesColor(static_cast<BrushType>(variant)));
    m_canvas->setRetouchMode(tool == ToolId::Blur || tool == ToolId::Dodge);
    if (m_engine && tool == ToolId::Blur) {
        // Which of the six strokes is decided in the engine, so the canvas has
        // one path for both buttons rather than six.
        m_engine->setFocusTool(variant);
        pushBlurOptions();
    } else if (m_engine && tool == ToolId::Dodge) {
        m_engine->setToneTool(variant);
        pushToneOptions();
    }
    if (tool == ToolId::Marquee) {
        m_canvas->setMarqueeType(static_cast<MarqueeType>(variant));
    } else if (tool == ToolId::Lasso) {
        m_canvas->setLassoType(static_cast<LassoType>(variant));
    } else if (tool == ToolId::QuickSelect) {
        m_canvas->setQuickSelectType(static_cast<QuickSelectType>(variant));
    } else if (tool == ToolId::Gradient) {
        m_canvas->setGradientTool(static_cast<GradientTool>(variant));
    } else if (tool == ToolId::Pen) {
        m_canvas->setPenTool(static_cast<PenTool>(variant));
    } else if (tool == ToolId::PathSelect) {
        m_canvas->setPathSelectTool(static_cast<PathSelectTool>(variant));
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
    if (!m_statusZoom) {
        return;
    }
    // Leave the field alone while it is being typed into: overwriting it
    // mid-edit would fight the user, and zoom changes arrive from the wheel and
    // the View menu too.
    if (m_statusZoom->hasFocus()) {
        return;
    }
    // Whole numbers read as "400%", not "400.0%"; fractional stops such as
    // 66.7% keep their decimal.
    const double percent = zoom * 100.0;
    const int decimals = qFuzzyCompare(percent, qRound(percent)) ? 0 : 1;
    m_statusZoom->setText(QStringLiteral("%1%").arg(percent, 0, 'f', decimals));
}

void MainWindow::applyTypedZoom()
{
    if (!m_statusZoom || !m_canvas) {
        return;
    }
    // Accept "400", "400%", "400 %" and a comma decimal separator, since the
    // field is small and people type what is quickest.
    QString text = m_statusZoom->text().trimmed();
    text.remove(QLatin1Char('%'));
    text.replace(QLatin1Char(','), QLatin1Char('.'));

    bool ok = false;
    const double percent = text.trimmed().toDouble(&ok);
    if (!ok || percent <= 0.0) {
        // Unparseable: put the real value back rather than guessing.
        onZoomChanged(m_canvas->zoom());
        return;
    }

    // The canvas clamps to the range CS6 allows, so out-of-range input lands on
    // the nearest limit rather than being rejected.
    m_canvas->setZoom(percent / 100.0);
    // setZoom only signals when the value actually changed, so refresh the text
    // directly — typing 400% when already at 400% should still tidy up "400".
    m_statusZoom->clearFocus();
    onZoomChanged(m_canvas->zoom());
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
    // Built by hand rather than through QMessageBox::question so the buttons
    // carry Photoshop's own wording instead of whatever the platform theme
    // substitutes for the standard roles.
    QMessageBox box(QMessageBox::Question, tr("PhotoRust"),
                    tr("Save changes to \"%1\" before closing?")
                        .arg(m_engine->getDocumentTitle()),
                    QMessageBox::NoButton, this);
    QPushButton *save = box.addButton(tr("&Yes"), QMessageBox::AcceptRole);
    QPushButton *discard = box.addButton(tr("&No"), QMessageBox::DestructiveRole);
    box.addButton(tr("Cancel"), QMessageBox::RejectRole);
    box.setDefaultButton(save);
    unsqueezeButtons(&box);
    box.exec();

    if (box.clickedButton() == save) {
        return saveDocument();
    }
    return box.clickedButton() == discard;
}

bool MainWindow::confirmDiscardAll()
{
    if (!m_engine) {
        return true;
    }
    // Every open document gets its own prompt, and is brought into view first so
    // the decision is made while looking at the right image. Any Cancel stops
    // the whole thing.
    for (int i = 0; i < m_engine->documentCount(); ++i) {
        if (!m_engine->documentModifiedAt(i)) {
            continue;
        }
        m_engine->setActiveDocument(i);
        refreshDocumentTabs();
        if (!confirmDiscardChanges()) {
            return false;
        }
    }
    return true;
}

void MainWindow::closeDocument()
{
    if (m_documentTabs) {
        onTabCloseRequested(m_documentTabs->currentIndex());
    }
}

void MainWindow::closeEvent(QCloseEvent *event)
{
    if (confirmDiscardAll()) {
        event->accept();
    } else {
        event->ignore();
    }
}

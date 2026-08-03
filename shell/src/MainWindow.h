#pragma once

#include <QLabel>
#include <QMainWindow>

#include "tools/ToolId.h"

class CanvasView;
class ColorPanel;
class CommandRegistry;
class Engine;
class BrushPresetPicker;
class HistoryPanel;
class InfoPanel;
class LayersPanel;
class ToolStrip;
class QLineEdit;
class QComboBox;
class QDockWidget;
class QTabBar;
class QToolButton;
class QDoubleSpinBox;
class QSpinBox;
class QToolBar;

/// The application window: menus, options bar, tool strip, docked panels and
/// the canvas.
///
/// This class owns the UI and nothing else. Every operation it offers is a
/// call into the engine (CLAUDE.md §2) — there is no image maths here.
class MainWindow : public QMainWindow
{
    Q_OBJECT

public:
    explicit MainWindow(Engine *engine, CommandRegistry *registry,
                        QWidget *parent = nullptr);

protected:
    void closeEvent(QCloseEvent *event) override;

private slots:
    // -- File --
    void newDocument();
    void openDocument();
    bool saveDocument();
    bool saveDocumentAs();
    /// Write every slice out as its own image file (File ▸ Save Slices).
    void exportSlices();

    // -- Edit --
    void undo();
    void redo();
    void fillWithForeground();
    void fillWithBackground();
    void clearSelection();

    // -- Image --
    void showCanvasSize();
    void applyAdjustment(const QString &name);

    // -- Filter --
    void applyFilter(const QString &name);

    // -- View --
    void zoomIn();
    void zoomOut();
    void fitOnScreen();
    void actualPixels();

    // -- reactions --
    void onToolChanged(ToolId tool, int variant);
    void onDocumentChanged();
    void onCursorMoved(const QPointF &pos);
    void onZoomChanged(double zoom);
    /// Parse the zoom field and apply it to the canvas.
    void applyTypedZoom();
    void refreshAll();
    void updateWindowTitle();

private:
    void createMenus();
    /// Build the Tools panel: the strip inside its dock with a CS6 header.
    void createToolPanel();
    /// CS6's marquee right-click menu, opened by the canvas.
    void showSelectionContextMenu(const QPoint &globalPos);
    void createOptionsBar();
    void createDocks();
    /// Rebuild the document tab bar from the engine's open documents.
    void refreshDocumentTabs();
    /// The user picked a tab.
    void onTabSelected(int index);
    /// The user clicked a tab's close button.
    void onTabCloseRequested(int index);
    void createStatusBar();
    void connectEngine();

    /// Fetch an action from the registry, give it a handler and return it.
    /// Keeps every menu entry going through the keymap.
    template <typename Slot>
    QAction *command(const QString &id, const QString &text, Slot slot);

    /// Prompt to save the active document if it has unsaved changes.
    /// Returns false if the user cancelled.
    bool confirmDiscardChanges();
    /// Prompt about every open document's unsaved changes, for quitting.
    bool confirmDiscardAll();
    /// Close the active document — File ▸ Close.
    void closeDocument();

    /// Make every registered command's shortcut live.
    ///
    /// A QAction only fires its shortcut once it belongs to a widget. Menu
    /// commands get that from being added to a QMenu, but tool commands live
    /// only in the registry and a QActionGroup, so without this the whole
    /// single-letter keymap (V, M, L, B, …) is inert.
    void installShortcuts();

    /// Rebuild the options bar for the active tool.
    void populateOptionsBar(ToolId tool, int variant);
    /// Add the new/add/subtract/intersect buttons the selection tools open
    /// their options bar with.
    void addSelectionModeButtons();
    /// Add the Magnetic Lasso's Width / Contrast / Frequency controls.
    void addMagneticOptions();
    /// Push the magnetic lasso settings into the canvas.
    void pushMagneticOptions();
    /// Add the Quick Selection brush size, or the Magic Wand's tolerance and
    /// checkboxes, depending on which of the two is active.
    void addQuickSelectOptions(QuickSelectType type);
    /// Push those settings into the canvas.
    void pushQuickSelectOptions();
    /// Add the Crop tool's ratio preset, Delete Cropped Pixels checkbox and
    /// the commit/cancel pair.
    void addCropOptions(CropType type);
    /// The Info panel's footer text for an eyedropper-group tool.
    QString infoHintFor(EyedropperType type) const;
    /// Add the Color Replacement Brush's Mode, Sampling, Limits, Tolerance and
    /// Anti-alias controls.
    void addColorReplaceOptions();
    /// Push those into the engine.
    void pushColorReplaceOptions();
    /// Add the Spot Healing Brush's Type radio set.
    void addHealTypeButtons();
    /// Add the Content-Aware Move tool's options: Mode, Structure, Color,
    /// Sample All Layers and the transform toggle.
    void addContentAwareMoveOptions();
    /// Push those into the canvas.
    void pushContentAwareMoveOptions();
    /// Add the Patch tool's own options: combine modes, Patch mode, the
    /// Source/Destination pair, Transparent and Use Pattern.
    void addPatchOptions();
    /// Push those into the canvas.
    void pushPatchOptions();
    /// Tell the user the Healing Brush needs a source point first.
    void warnHealingSourceRequired();
    /// Add the options for the healing group's region-based variants.
    void addHealingRegionOptions(HealingType type);
    /// Add the annotation tools' readouts and Clear button.
    void addAnnotationOptions(EyedropperType type);
    /// Refill those readouts from the engine.
    void updateAnnotationReadouts();
    /// Put up the note editor for a note marker.
    void editNote(int index);
    /// Push the crop settings into the canvas.
    void pushCropOptions();
    /// Push the current brush settings into the engine.
    void pushBrushSettings();
    /// The brush preset picker, created on first use. It outlives the options
    /// bar because it holds the current tip.
    BrushPresetPicker *brushPicker();
    /// Redraw the options-bar tip button from the current size and hardness.
    void refreshBrushTipButton();

    Engine *m_engine = nullptr;
    CommandRegistry *m_registry = nullptr;

    CanvasView *m_canvas = nullptr;
    ToolStrip *m_toolStrip = nullptr;
    QDockWidget *m_toolsDock = nullptr;
    QToolBar *m_optionsBar = nullptr;
    /// One tab per open document, above the canvas.
    QTabBar *m_documentTabs = nullptr;

    LayersPanel *m_layersPanel = nullptr;
    ColorPanel *m_colorPanel = nullptr;
    HistoryPanel *m_historyPanel = nullptr;
    InfoPanel *m_infoPanel = nullptr;
    QDockWidget *m_infoDock = nullptr;

    // Options-bar widgets for the brush family. Recreated per tool, so these
    // are only valid while a painting tool is active.
    QSpinBox *m_brushOpacity = nullptr;
    QSpinBox *m_brushFlow = nullptr;
    QToolButton *m_brushTipButton = nullptr;

    /// The brush tip. Held here rather than in a widget because the picker that
    /// edits it is a popup and the bar is rebuilt on every tool change.
    /// True while the Brush button is holding the Pencil, which paints aliased.
    bool m_pencilMode = false;
    /// The Pencil's Auto Erase option.
    bool m_autoErase = false;
    /// Color Replacement Brush options, which persist across tool switches.
    int m_replaceMode = int(ReplaceDefaults::kMode);
    int m_replaceSampling = int(ReplaceDefaults::kSampling);
    int m_replaceLimits = int(ReplaceDefaults::kLimits);
    int m_replaceTolerance = ReplaceDefaults::kTolerancePercent;
    bool m_replaceAntialias = ReplaceDefaults::kAntialias;
    double m_brushSizeValue = 20.0;
    int m_brushHardnessValue = 100;
    BrushPresetPicker *m_brushPicker = nullptr;

    QLabel *m_statusPosition = nullptr;
    QLineEdit *m_statusZoom = nullptr;
    QLabel *m_statusDocSize = nullptr;

    ToolId m_activeTool = ToolId::Brush;
    int m_activeVariant = 0;
    /// Options-bar combine mode and feather radius, kept here because the bar
    /// is rebuilt on every tool change and CS6 remembers both.
    SelectionMode m_selectionMode = SelectionMode::New;
    int m_featherRadius = 0;
    /// Magnetic Lasso settings, kept here for the same reason.
    int m_magneticWidth = MagneticDefaults::kWidth;
    int m_magneticContrast = MagneticDefaults::kContrast;
    int m_magneticFrequency = MagneticDefaults::kFrequency;
    /// Quick Selection and Magic Wand settings, likewise.
    int m_quickBrushSize = WandDefaults::kBrushSize;
    int m_wandTolerance = WandDefaults::kTolerance;
    bool m_wandAntialias = WandDefaults::kAntialias;
    bool m_wandContiguous = WandDefaults::kContiguous;
    /// Crop settings. 0 leaves the box unconstrained.
    double m_cropRatio = 0.0;
    bool m_cropDeletePixels = true;
    /// Spot Healing Brush type. CS6 defaults to Content-Aware.
    HealType m_healType = HealType::ContentAware;
    /// Content-Aware Move mode, and the Red Eye tool's two settings.
    bool m_camExtend = false;
    int m_camStructure = CamDefaults::kStructure;
    int m_camColor = CamDefaults::kColor;
    bool m_camSampleAllLayers = false;
    /// Patch tool options, which persist across tool switches.
    bool m_patchContentAware = false;
    bool m_patchDestination = false;
    bool m_patchTransparent = false;
    int m_pupilSize = RedEyeDefaults::kPupilSize;
    int m_darkenAmount = RedEyeDefaults::kDarkenAmount;
    /// Options-bar readout labels for the annotation tools. Recreated with the
    /// bar, so this list is only valid while one of those tools is active.
    QList<QLabel *> m_annotationReadouts;
};

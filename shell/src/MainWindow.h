#pragma once

#include <QColor>
#include <QFont>
#include <QLabel>
#include <QMainWindow>
#include <QStringList>

#include "tools/ToolId.h"

class CanvasView;
class ChannelsPanel;
class ColorPanel;
class CommandRegistry;
class Engine;
class BrushPresetPicker;
class HistoryPanel;
class InfoPanel;
class LayersPanel;
class PathsPanel;
class ToolStrip;
class QLineEdit;
class QComboBox;
class QDockWidget;
class QMenu;
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
    /// Close every open document, prompting for each unsaved one.
    void closeAllDocuments();
    /// Show what the active document's file says about itself.
    void showFileInfo();
    bool saveDocument();
    bool saveDocumentAs();
    /// Write every slice out as its own image file (File ▸ Save Slices).
    void exportSlices();
    /// File ▸ Export ▸ Export As.
    void exportAs();
    /// File ▸ Export ▸ Save for Web (Legacy).
    void saveForWeb();
    /// File ▸ Print.
    void printDocument();
    /// File ▸ Print One Copy.
    void printOneCopy();

    // -- Edit --
    void undo();
    void redo();
    /// Copy what is selected, then clear it.
    void cut();
    /// Copy what is selected from the active layer, or — merged — from the
    /// whole visible image. False when there was nothing to copy.
    bool copy();
    bool copyMerged();
    /// Paste the clipboard image as a new layer, centred on the view.
    void paste();
    /// Paste it back where it was copied from.
    void pasteInPlace();
    /// Paste it inside the selection, or outside it, as a masked layer.
    void pasteInto();
    void pasteOutside();
    void showFillDialog();
    void showStrokeDialog();
    void clearSelection();
    void findReplaceText();
    void freeTransform();
    void transformRotate180();
    void transformRotate90CW();
    void transformRotate90CCW();
    void transformFlipHorizontal();
    void transformFlipVertical();

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
    /// CS6's Zoom tool right-click menu, likewise.
    void showZoomContextMenu(const QPoint &globalPos);
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

    /// The work behind Copy and Copy Merged.
    bool copyToClipboard(bool merged);
    /// The work behind Paste Into and Paste Outside — `mode` 1 or 2.
    void pasteConfined(int mode);
    /// Put a clipboard image into the document at `at`.
    void pasteAt(const QImage &image, const QPoint &at, int mode);

    /// Open a file by path, telling the user if it could not be read — what
    /// File ▸ Open Recent needs.
    void openPath(const QString &path);
    /// The work behind it: open the file into its own tab and return whether
    /// that worked, saying nothing either way. Opening several files at once
    /// reports on them together rather than one dialog at a time.
    bool loadPath(const QString &path);
    /// The remembered list, most recent first.
    QStringList recentFiles() const;
    /// Put a path at the top of it.
    void rememberRecentFile(const QString &path);
    /// Rebuild the Open Recent submenu from that list.
    void refreshRecentMenu();

    /// Open a multi-frame image — an animated GIF — with each frame as a
    /// layer. False when the file is an ordinary single image, which the
    /// caller then opens the usual way.
    bool openAnimatedFrames(const QString &path);

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
    /// Add the Mixer Brush's load swatch, preset menu and Wet/Load/Mix/Flow
    /// controls.
    void addMixerOptions();
    /// Push those into the engine.
    void pushMixerOptions();
    /// Redraw the load swatch from the paint currently on the brush.
    void refreshMixerLoadSwatch();
    /// Move the preset menu to whichever entry matches the current Wet/Load/Mix,
    /// or to Custom when none does.
    void syncMixerPresetCombo();
    /// Add the shared Mode / Strength / Sample All Layers row for the Blur
    /// button's three tools, plus whichever checkbox that variant owns.
    void addBlurOptions(BlurTool tool);
    /// Add the toning tools' Range or Mode, Exposure or Flow, and their
    /// Protect Tones / Vibrance checkbox.
    void addToneOptions(ToneTool tool);
    /// Push those into the engine.
    void pushToneOptions();
    /// Push those into the engine.
    void pushBlurOptions();
    /// Add the Paint Bucket's Fill menu, Mode, Opacity, Tolerance and its three
    /// checkboxes.
    void addBucketOptions();
    /// Push those into the engine.
    void pushBucketOptions();
    /// Add the Gradient tool's preset swatch, five type buttons, Mode, Opacity,
    /// Reverse, Dither and Transparency.
    void addGradientOptions();
    /// Push those into the engine.
    void pushGradientOptions();
    /// Redraw the options-bar swatch from the chosen preset.
    void refreshGradientSwatch();
    /// Add the Pen button's options: Auto Add/Delete and Rubber Band for the
    /// Pen tool itself, a Curve Fit slider for Freeform Pen, and a plain hint
    /// for the three tools with nothing to configure.
    void addPenOptions(PenTool tool);
    /// Push Auto Add/Delete and Rubber Band into the canvas.
    void pushPenOptions();
    /// Add the Clone Stamp's Aligned checkbox and Sample menu.
    void addCloneOptions();
    /// Push those into the canvas, which owns the source point.
    void pushCloneOptions();
    /// Add the Rotate View tool's angle field and Reset View button.
    void addRotateViewOptions();
    /// Add the shape tools' Mode menu and Fill swatch, plus whichever setting
    /// this particular shape owns — Radius, Sides, Weight or the shape picker.
    void addShapeOptions(ShapeTool tool);
    /// The hint the bar ends with, which differs per tool because the
    /// modifiers do.
    QString shapeHintFor(ShapeTool tool) const;
    /// Redraw the options-bar swatch from the chosen custom shape.
    void refreshCustomShapeButton();
    /// Push the shape settings into the engine, which owns the geometry.
    void pushShapeOptions();
    /// Add the Background Eraser's Sampling, Limits, Tolerance and Protect
    /// Foreground Color.
    void addBackgroundEraseOptions();
    /// Push those into the canvas.
    void pushBackgroundEraseOptions();
    /// Add the Magic Eraser's Tolerance, Anti-alias, Contiguous, Sample All
    /// Layers and Opacity.
    void addMagicEraseOptions();
    /// Push those into the canvas, which passes them with the click.
    void pushMagicEraseOptions();
    /// Add the Pattern Stamp's pattern picker, Aligned and Impressionist.
    void addPatternStampOptions();
    /// Redraw the options-bar swatch from the chosen pattern.
    void refreshPatternSwatch();
    /// Push the pattern choice into the engine.
    void pushPatternOptions();
    /// Tell the user the Clone Stamp needs a source point first.
    void warnCloneSourceRequired();
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
    /// Tell the user an edit was refused because the layer is locked.
    void warnLayerLocked();
    /// Show the Auto-Align Layers dialog.
    void autoAlignLayers();
    void editKeyboardShortcuts();
    void editColorSettings();
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
    /// Add the Type tool's options: font family and style, size, the
    /// anti-aliasing method, paragraph alignment, text colour, and the
    /// (unimplemented) warp and panel-toggle buttons CS6 has after them.
    void addTypeOptions();
    /// Push those into the canvas.
    void pushTypeOptions();
    /// Take on the type of text the Type tool just reopened, and rebuild the
    /// options bar so it describes that text.
    void adoptTypeStyle(const QString &family, const QString &style, qreal pointSize,
                        const QColor &color, Qt::Alignment alignment, bool antialias,
                        bool vertical);
    /// The brush preset picker, created on first use. It outlives the options
    /// bar because it holds the current tip.
    BrushPresetPicker *brushPicker();
    /// Redraw the options-bar tip button from the current size and hardness.
    void refreshBrushTipButton();

    QList<QAction *> m_editNonTypingActions;
    QAction *m_transformAgainAction = nullptr;
    QAction *m_autoAlignAction = nullptr;
    bool m_hasTransformed = false;

    void showTransformOptionsBar();
    void hideTransformOptionsBar();
    void updateTransformReadouts();
    bool m_transformBarActive = false;
    ToolId m_preTransformTool = ToolId::Move;
    int m_preTransformVariant = 0;

    Engine *m_engine = nullptr;
    CommandRegistry *m_registry = nullptr;

    CanvasView *m_canvas = nullptr;
    ToolStrip *m_toolStrip = nullptr;
    QDockWidget *m_toolsDock = nullptr;
    QToolBar *m_optionsBar = nullptr;
    /// One tab per open document, above the canvas.
    QTabBar *m_documentTabs = nullptr;

    LayersPanel *m_layersPanel = nullptr;
    ChannelsPanel *m_channelsPanel = nullptr;
    PathsPanel *m_pathsPanel = nullptr;
    ColorPanel *m_colorPanel = nullptr;
    HistoryPanel *m_historyPanel = nullptr;
    InfoPanel *m_infoPanel = nullptr;
    QDockWidget *m_infoDock = nullptr;

    // Options-bar widgets for the brush family. Recreated per tool, so these
    // are only valid while a painting tool is active.
    QSpinBox *m_brushOpacity = nullptr;
    QSpinBox *m_brushFlow = nullptr;
    QToolButton *m_brushTipButton = nullptr;

    // The Mixer Brush's own widgets, valid only while it is the active tool.
    QToolButton *m_mixerLoadButton = nullptr;
    QComboBox *m_mixerPresetCombo = nullptr;

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
    /// Mixer Brush options, which persist across tool switches. The paint on the
    /// brush is not among them — that lives in the engine, which is what mixes
    /// with it.
    int m_mixerWet = MixerDefaults::kWet;
    int m_mixerLoad = MixerDefaults::kLoad;
    int m_mixerMix = MixerDefaults::kMix;
    int m_mixerFlow = MixerDefaults::kFlow;
    bool m_mixerSampleAllLayers = MixerDefaults::kSampleAllLayers;
    bool m_mixerLoadAfterStroke = MixerDefaults::kLoadAfterStroke;
    bool m_mixerCleanAfterStroke = MixerDefaults::kCleanAfterStroke;
    /// The options-bar swatch showing the chosen gradient, valid only while the
    /// Gradient tool is active.
    QToolButton *m_gradientSwatch = nullptr;
    /// Gradient options, which persist across tool switches. The preset is held
    /// by name — the engine owns the ramps.
    QString m_gradientPreset;
    int m_gradientType = int(GradientDefaults::kType);
    int m_gradientMode = 0;
    int m_gradientOpacity = GradientDefaults::kOpacity;
    bool m_gradientReverse = GradientDefaults::kReverse;
    bool m_gradientDither = GradientDefaults::kDither;
    bool m_gradientTransparency = GradientDefaults::kTransparency;
    /// Options for the Blur button's three tools, which persist across tool
    /// switches. Strength, Mode and Sample All Layers are shared, as CS6 shares
    /// them; the last two belong to Sharpen and Smudge respectively.
    int m_blurMode = 0;
    int m_blurStrength = BlurDefaults::kStrength;
    bool m_blurSampleAllLayers = BlurDefaults::kSampleAllLayers;
    bool m_blurProtectDetail = BlurDefaults::kProtectDetail;
    bool m_blurFingerPainting = BlurDefaults::kFingerPainting;
    /// Toning tool options, which persist across tool switches. Exposure and
    /// Flow are the same number, as they are in the engine.
    int m_toneRange = int(ToneDefaults::kRange);
    int m_spongeMode = int(ToneDefaults::kSpongeMode);
    int m_toneAmount = ToneDefaults::kAmount;
    bool m_toneProtectTones = ToneDefaults::kProtectTones;
    bool m_toneVibrance = ToneDefaults::kVibrance;
    /// Pen tool options, which persist across tool switches.
    bool m_penAutoAddDelete = PenDefaults::kAutoAddDelete;
    bool m_penRubberBand = PenDefaults::kRubberBand;
    double m_freeformTolerance = PenDefaults::kFreeformTolerance;
    /// Type tool options, which persist across tool switches. The style name
    /// (e.g. "Bold Italic") is kept separately from the font because it comes
    /// from the family's own style list, not from QFont's bold/italic bits.
    QFont m_typeFont{QStringLiteral("Myriad Pro"), int(TypeDefaults::kSize)};
    QString m_typeStyle = QStringLiteral("Regular");
    QColor m_typeColor = Qt::black;
    Qt::Alignment m_typeAlignment = Qt::AlignLeft;
    bool m_typeAntialias = TypeDefaults::kAntialias;
    /// True while the Vertical Type tool is the one in hand, or while text that
    /// is itself vertical is open for editing.
    bool m_typeVertical = false;

    /// Where the last Copy made here came from, and whether the clipboard is
    /// still holding it. The system clipboard carries pixels alone, so this is
    /// what lets Paste in Place put them back.
    QPoint m_copyOrigin;
    bool m_copyIsOurs = false;

    /// File ▸ Open Recent, rebuilt from settings each time it opens.
    QMenu *m_recentMenu = nullptr;

    /// The shape tools' options, which persist across tool switches like every
    /// other tool's.
    ShapeMode m_shapeMode = ShapeDefaults::kMode;
    ShapeTool m_shapeTool = ShapeTool::Rectangle;
    int m_shapeCornerRadius = ShapeDefaults::kCornerRadius;
    int m_shapeSides = ShapeDefaults::kSides;
    int m_shapeLineWeight = ShapeDefaults::kLineWeight;
    int m_customShape = 0;
    QToolButton *m_customShapeButton = nullptr;

    /// The two colour erasers' options, which persist across tool switches like
    /// every other tool's.
    int m_bgEraseSampling = BackgroundEraseDefaults::kSampling;
    int m_bgEraseLimits = BackgroundEraseDefaults::kLimits;
    int m_bgEraseTolerance = BackgroundEraseDefaults::kTolerance;
    bool m_bgEraseProtectForeground = BackgroundEraseDefaults::kProtectForeground;
    int m_magicEraseTolerance = MagicEraseDefaults::kTolerance;
    bool m_magicEraseAntialias = MagicEraseDefaults::kAntialias;
    bool m_magicEraseContiguous = MagicEraseDefaults::kContiguous;
    bool m_magicEraseSampleAll = MagicEraseDefaults::kSampleAllLayers;
    int m_magicEraseOpacity = MagicEraseDefaults::kOpacity;

    /// The Pattern Stamp's options, which persist across tool switches like
    /// every other tool's. The index is into the engine's pattern list.
    int m_patternIndex = 0;
    bool m_patternAligned = true;
    QToolButton *m_patternSwatch = nullptr;
    /// True once `m_typeColor` has been seeded from the engine's foreground
    /// colour, which happens the first time the Type tool's bar is built.
    /// After that the user's own choice persists, the same as every other
    /// tool option here.
    bool m_typeColorInitialized = false;
    /// Paint Bucket options, which persist across tool switches.
    int m_bucketMode = 0;
    int m_bucketOpacity = BucketDefaults::kOpacity;
    int m_bucketTolerance = BucketDefaults::kTolerance;
    bool m_bucketAntialias = BucketDefaults::kAntialias;
    bool m_bucketContiguous = BucketDefaults::kContiguous;
    bool m_bucketAllLayers = BucketDefaults::kAllLayers;
    /// Clone Stamp options, which persist across tool switches.
    bool m_cloneAligned = CloneDefaults::kAligned;
    int m_cloneSampling = int(CloneDefaults::kSampling);
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

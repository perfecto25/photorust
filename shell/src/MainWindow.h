#pragma once

#include <QLabel>
#include <QMainWindow>

#include "tools/ToolId.h"

class CanvasView;
class ColorPanel;
class CommandRegistry;
class Engine;
class HistoryPanel;
class LayersPanel;
class ToolStrip;
class QComboBox;
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
    void refreshAll();
    void updateWindowTitle();

private:
    void createMenus();
    void createOptionsBar();
    void createDocks();
    void createStatusBar();
    void connectEngine();

    /// Fetch an action from the registry, give it a handler and return it.
    /// Keeps every menu entry going through the keymap.
    template <typename Slot>
    QAction *command(const QString &id, const QString &text, Slot slot);

    /// Prompt to save when the document has unsaved changes.
    /// Returns false if the user cancelled.
    bool confirmDiscardChanges();

    /// Make every registered command's shortcut live.
    ///
    /// A QAction only fires its shortcut once it belongs to a widget. Menu
    /// commands get that from being added to a QMenu, but tool commands live
    /// only in the registry and a QActionGroup, so without this the whole
    /// single-letter keymap (V, M, L, B, …) is inert.
    void installShortcuts();

    /// Rebuild the options bar for the active tool.
    void populateOptionsBar(ToolId tool, int variant);
    /// Push the current brush settings into the engine.
    void pushBrushSettings();

    Engine *m_engine = nullptr;
    CommandRegistry *m_registry = nullptr;

    CanvasView *m_canvas = nullptr;
    ToolStrip *m_toolStrip = nullptr;
    QToolBar *m_optionsBar = nullptr;

    LayersPanel *m_layersPanel = nullptr;
    ColorPanel *m_colorPanel = nullptr;
    HistoryPanel *m_historyPanel = nullptr;

    // Options-bar widgets for the brush family. Recreated per tool, so these
    // are only valid while a painting tool is active.
    QDoubleSpinBox *m_brushSize = nullptr;
    QSpinBox *m_brushHardness = nullptr;
    QSpinBox *m_brushOpacity = nullptr;
    QSpinBox *m_brushFlow = nullptr;

    QLabel *m_statusPosition = nullptr;
    QLabel *m_statusZoom = nullptr;
    QLabel *m_statusDocSize = nullptr;

    ToolId m_activeTool = ToolId::Brush;
    int m_activeVariant = 0;
};

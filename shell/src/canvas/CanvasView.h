#pragma once

#include <QImage>
#include <QPainterPath>
#include <QPoint>
#include <QPointF>
#include <QWidget>

#include "../tools/ToolId.h"

class Engine;

/// The document viewport.
///
/// Owns zoom/pan and translates mouse input into document-space coordinates
/// before handing it to the engine. All pixel work happens in Rust — this
/// widget only presents the `QImage` the engine hands back, so the painting
/// path stays a blit plus the checkerboard beneath it.
///
/// Rendering today is `QPainter` onto a widget. The GPU backend described in
/// CLAUDE.md §7 will replace `paintEvent` without changing this interface.
class CanvasView : public QWidget
{
    Q_OBJECT

public:
    explicit CanvasView(Engine *engine, QWidget *parent = nullptr);

    /// Current zoom factor; 1.0 is 100%.
    double zoom() const { return m_zoom; }

    /// Set zoom, keeping the view centre fixed. Clamped to the range CS6
    /// allows (0.1% to 3200%).
    void setZoom(double zoom);

    /// Set zoom, keeping `focusWidgetPos` pinned to the same document pixel.
    /// This is what makes scroll-wheel zoom feel anchored to the cursor.
    void setZoomAt(double zoom, const QPointF &focusWidgetPos);

    /// Scale so the whole document fits, then centre it (Ctrl+0).
    void fitToWindow();

    /// Jump to 100% (Ctrl+1).
    void actualPixels();

    void zoomIn();
    void zoomOut();

    /// The tool that receives mouse input.
    void setActiveTool(ToolId tool);
    ToolId activeTool() const { return m_tool; }

    /// Which marquee variant the Marquee tool draws.
    void setMarqueeType(MarqueeType type);
    MarqueeType marqueeType() const { return m_marqueeType; }

    /// How the next selection combines with the current one — the options
    /// bar's new/add/subtract/intersect buttons. Modifiers held during a drag
    /// override this for that drag only, as they do in CS6.
    void setSelectionMode(SelectionMode mode) { m_selectionMode = mode; }
    SelectionMode selectionMode() const { return m_selectionMode; }

    /// Feather radius in pixels for new selections — the options bar's
    /// Feather field. Softens the incoming region only, so it does not
    /// re-soften what the selection already holds.
    void setFeatherRadius(int pixels) { m_featherRadius = qMax(0, pixels); }
    int featherRadius() const { return m_featherRadius; }

    /// Re-fetch the composited image from the engine and repaint.
    void refresh();

    /// Re-trace the selection outline from the engine and repaint.
    ///
    /// Deliberately *not* called by `refresh()`. Tracing the contour walks the
    /// whole mask, and `refresh()` runs on every brush dab; only a real change
    /// to the selection should pay for it. The engine's `selectionChanged`
    /// signal is the trigger.
    void refreshSelection();

    /// Convert a widget point to document space.
    QPointF widgetToDocument(const QPointF &pos) const;
    /// Convert a document point to widget space.
    QPointF documentToWidget(const QPointF &pos) const;

signals:
    /// Emitted as the cursor moves, for the status bar readout.
    void cursorMoved(const QPointF &documentPos);
    /// Emitted whenever the zoom factor changes.
    void zoomChanged(double zoom);
    /// The user picked a colour with the eyedropper.
    void colorPicked(const QColor &color);
    /// Right-click with a selection tool active. `globalPos` is where the
    /// menu should open. The canvas does not build the menu itself: the
    /// commands on it belong to the registry, which `MainWindow` owns.
    void contextMenuRequested(const QPoint &globalPos);

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;
    void wheelEvent(QWheelEvent *event) override;
    void keyPressEvent(QKeyEvent *event) override;
    void keyReleaseEvent(QKeyEvent *event) override;
    void contextMenuEvent(QContextMenuEvent *event) override;
    void resizeEvent(QResizeEvent *event) override;
    void enterEvent(QEnterEvent *event) override;
    void leaveEvent(QEvent *event) override;

private:
    /// Where the document's top-left sits in widget coordinates.
    QPointF documentOrigin() const;
    /// The document's on-screen rectangle at the current zoom.
    QRectF documentRect() const;
    /// Keep the document from being panned entirely out of view.
    void clampPan();
    /// Cursor appropriate to the active tool and modifier state.
    void updateCursor();
    /// Marching-ants outline of the current selection.
    void paintSelection(QPainter &painter);
    /// The combine operation for a gesture made with `modifiers` held: the
    /// options-bar mode unless a modifier overrides it.
    SelectionMode effectiveSelectionMode(Qt::KeyboardModifiers modifiers) const;
    /// Send the in-progress marquee to the engine.
    void commitMarquee(const QRectF &documentRect, Qt::KeyboardModifiers modifiers);
    /// True when the active marquee variant is a click rather than a drag.
    bool marqueeIsLineSelect() const;

    Engine *m_engine = nullptr;

    /// Cached composite. Refreshed from the engine, never edited here.
    QImage m_image;

    double m_zoom = 1.0;
    /// Pan offset in widget pixels, from the centred position.
    QPointF m_pan{0.0, 0.0};

    ToolId m_tool = ToolId::Brush;
    MarqueeType m_marqueeType = MarqueeType::Rectangular;
    SelectionMode m_selectionMode = SelectionMode::New;
    int m_featherRadius = 0;

    // -- interaction state --
    bool m_dragging = false;
    bool m_panning = false;
    /// True while space is held, which temporarily activates the Hand tool.
    bool m_spacePanOverride = false;
    QPointF m_lastMousePos;
    QPointF m_dragStartDoc;

    /// Live marquee rectangle while dragging a selection tool.
    QRectF m_marquee;
    bool m_marqueeActive = false;
    /// Modifiers held when the selection drag began. Photoshop samples these
    /// at press, so letting go of Shift mid-drag does not change the mode.
    Qt::KeyboardModifiers m_gestureModifiers = Qt::NoModifier;

    /// Animation phase for the marching ants.
    int m_antsOffset = 0;
    /// The selection contour in document coordinates, one subpath per loop.
    /// Cached; rebuilt only by `refreshSelection()`.
    QPainterPath m_selectionPath;
};

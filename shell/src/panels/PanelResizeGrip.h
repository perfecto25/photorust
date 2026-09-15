#pragma once

#include <QPoint>
#include <QSize>
#include <QWidget>

class QDockWidget;

/// CS6's resize grip: the little diagonal hatch in the bottom-right corner of
/// a floating panel, dragged to make the panel bigger or smaller.
///
/// A floating panel *can* be resized by its window edges, but those are a
/// pixel or two wide and invisible — there is nothing to tell you they are
/// there. CS6 draws this corner instead, and it is what people reach for.
///
/// Not `QSizeGrip`: that draws whatever the platform style says a grip looks
/// like, which under Fusion is a pale triangle of lines that reads as
/// scratched-on damage over a dark panel. The drag itself is three lines of
/// arithmetic, so drawing CS6's hatch and doing the resize here costs less
/// than fighting the style for the other one.
///
/// It attaches itself to the dock: it watches for the panel being floated or
/// docked and shows or hides accordingly — a docked panel is resized by its
/// splitter, and a grip there would try to resize the main window — and it
/// keeps itself in the corner as the panel is resized.
class PanelResizeGrip : public QWidget
{
    Q_OBJECT

public:
    /// Parents itself to `dock` and takes over its own placement; the caller
    /// has nothing to keep hold of.
    explicit PanelResizeGrip(QDockWidget *dock);

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    /// Sit in the bottom-right corner of the panel, over whatever is there.
    void moveToCorner();

    QDockWidget *m_dock = nullptr;
    /// Where the drag started, and how big the window was then. Measured from
    /// the start rather than from the last move, so the window cannot drift
    /// away from the pointer over a long drag.
    QPoint m_from;
    QSize m_was;
    bool m_dragging = false;
};

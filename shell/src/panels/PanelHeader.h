#pragma once

#include <QWidget>

class QToolButton;

/// CS6 panel chrome: a dotted drag grip with the collapse chevron and the
/// close cross at its right edge.
///
/// Used as a `QDockWidget` title bar. Everything the buttons do not consume
/// falls through to the dock widget, which is what makes the whole header a
/// drag surface — the panel can be moved by it whether it is docked or
/// floating. Qt's own `QToolBar` grip cannot do that: it is not painted at all
/// while the toolbar floats, which leaves a floating panel with nothing to
/// grab.
class PanelHeader : public QWidget
{
    Q_OBJECT

public:
    explicit PanelHeader(QWidget *parent = nullptr);

    /// Point the chevron left (collapse) or right (expand).
    void setCollapsePointsLeft(bool pointsLeft);
    /// Hide the chevron for panels with nothing to collapse.
    void setCollapseVisible(bool visible);

signals:
    void collapseClicked();
    void closeClicked();

protected:
    void paintEvent(QPaintEvent *event) override;
    QSize sizeHint() const override;

private:
    QToolButton *m_collapse = nullptr;
    QToolButton *m_close = nullptr;
};

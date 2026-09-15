#pragma once

#include <QColor>
#include <QList>
#include <QPoint>
#include <QString>
#include <QWidget>

class QScrollArea;
class QToolButton;

/// One colour in the Swatches panel.
///
/// CS6 names every colour in its libraries, and the name is what the tooltip
/// shows — a grid of unlabelled squares is much harder to work with than it
/// looks, since two swatches a step apart in tone are indistinguishable at
/// this size.
struct Swatch
{
    QColor color;
    QString name;
};

/// The grid of colour squares itself.
///
/// Split from the panel around it so the panel can put CS6's footer under it
/// and a scroll area around it, and so the grid can be driven directly by a
/// test without a scroll viewport in the way.
///
/// The grid reflows: how many swatches fit on a row is decided by the width it
/// is given, and its height follows from that. That is what lets a panel
/// dragged wider show more per row, as CS6's does.
class SwatchGrid : public QWidget
{
    Q_OBJECT

public:
    explicit SwatchGrid(QWidget *parent = nullptr);

    const QList<Swatch> &swatches() const { return m_swatches; }
    void setSwatches(QList<Swatch> swatches);
    void addSwatch(const Swatch &swatch);
    /// Remove one, and report whether there was one there to remove.
    bool removeSwatch(int index);

    /// CS6's shipped library is a licensed file we do not have; this is a set
    /// in its shape — the primaries, a sweep through the hues light, pure and
    /// dark, and a grey ramp.
    static QList<Swatch> defaultSwatches();

    /// Which swatch is under a point, or -1 for the empty area past the end.
    int indexAt(const QPoint &pos) const;

    /// How tall the grid needs to be for a given width — the scroll area
    /// asks, rather than the grid setting its own height. A grid that sized
    /// itself would only be right once its resize event had been delivered,
    /// which makes its layout depend on an event loop having run.
    int heightFor(int width) const;
    QSize sizeHint() const override;
    QSize minimumSizeHint() const override;

    /// The one last clicked, which is what the footer's bin deletes. CS6 has
    /// no persistent selection here — you drag a swatch onto the bin — so the
    /// marked swatch is this shell's way of giving that button something to
    /// act on when it is pressed rather than dropped onto.
    int current() const { return m_current; }

signals:
    /// A swatch was clicked, or ctrl-clicked for the background pair.
    void foregroundPicked(const QColor &color);
    void backgroundPicked(const QColor &color);
    /// The empty area past the last swatch was clicked, which in CS6 makes a
    /// new swatch from the foreground colour.
    void newSwatchRequested();
    void currentChanged(int index);
    void countChanged();

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void resizeEvent(QResizeEvent *event) override;
    bool event(QEvent *event) override;

private:
    /// Where a swatch sits, in the grid's coordinates.
    QRect cellRect(int index) const;
    int columns() const;
    int columnsFor(int width) const;

    QList<Swatch> m_swatches;
    int m_current = -1;
};

/// CS6's Swatches panel: the grid, and the two footer buttons under it.
///
/// Click a swatch for the foreground colour, ctrl-click for the background,
/// alt-click to delete it — CS6's three, in its modifiers. Clicking the empty
/// space past the last swatch makes a new one from the foreground colour, as
/// does the left footer button.
///
/// Not built: loading and saving `.aco` swatch libraries, and the preset
/// libraries in the panel menu. Both are file-format work rather than panel
/// work, and neither changes what the panel does with the colours it holds.
class SwatchesPanel : public QWidget
{
    Q_OBJECT

public:
    explicit SwatchesPanel(QWidget *parent = nullptr);

    SwatchGrid *grid() const { return m_grid; }

signals:
    void foregroundPicked(const QColor &color);
    void backgroundPicked(const QColor &color);

public slots:
    /// The colour a new swatch is made from — the document's foreground,
    /// which the panel is told about rather than reaching for.
    void setForegroundColor(const QColor &color);

private:
    /// CS6 asks for a name; an empty answer is a cancel.
    void addSwatchFromForeground();
    void deleteCurrentSwatch();
    void showContextMenu(const QPoint &at);

    SwatchGrid *m_grid = nullptr;
    QScrollArea *m_scroll = nullptr;
    QToolButton *m_newButton = nullptr;
    QToolButton *m_deleteButton = nullptr;
    QColor m_foreground{Qt::black};
};

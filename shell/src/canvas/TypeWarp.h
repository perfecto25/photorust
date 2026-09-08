#pragma once

#include <QPointF>
#include <QRectF>
#include <QString>
#include <QStringList>

/// Warp Text: CS6's fifteen ways of bending a block of type.
///
/// Each style is a displacement of the text's own bounding box, written in
/// normalised coordinates — `u` running 0 to 1 along the text and `v` 0 to 1
/// down it — so the same formula works whatever size the letters are. The
/// shell owns this rather than the engine because it is the glyph outlines
/// Qt produces that get bent; the engine only records what was asked for.
///
/// The curves are reconstructions. Adobe does not publish the functions
/// behind these styles, so what is here is fitted to how each one looks at
/// CS6's default +50% bend — the right family of curve and the right
/// direction, not a byte-identical match.
class TypeWarp
{
public:
    /// CS6's menu order, which is also what the record stores and what a
    /// `.psd` would carry.
    enum Style {
        None = 0,
        Arc,
        ArcLower,
        ArcUpper,
        Arch,
        Bulge,
        ShellLower,
        ShellUpper,
        Flag,
        Wave,
        Fish,
        Rise,
        Fisheye,
        Inflate,
        Squeeze,
        Twist,
        Count,
    };

    /// The names down the Style menu, one per style and indexed by it — so
    /// `styleNames().at(Arc)` is "Arc". No separators in here: they are a
    /// presentation detail, and putting them in the list once made the index
    /// stop being the style, which stored Arc as Arc Lower.
    static QStringList styleNames();

    /// Whether CS6 draws a separator above this style, grouping the menu.
    static bool startsGroup(int style);

    /// Whether a style lets the Horizontal/Vertical pair be chosen. CS6 greys
    /// it out for Fisheye, Inflate and Twist — the first and last because
    /// they work about a centre rather than along an axis, and Inflate
    /// because it swells the block the same way whichever way you read it.
    static bool hasOrientation(int style);

    TypeWarp() = default;
    TypeWarp(int style, bool horizontal, qreal bend, qreal hDistort, qreal vDistort)
        : m_style(style)
        , m_horizontal(horizontal)
        , m_bend(bend)
        , m_hDistort(hDistort)
        , m_vDistort(vDistort)
    {
    }

    bool isActive() const { return m_style != None && m_style < Count; }
    int style() const { return m_style; }
    bool horizontal() const { return m_horizontal; }
    qreal bend() const { return m_bend; }
    qreal horizontalDistortion() const { return m_hDistort; }
    qreal verticalDistortion() const { return m_vDistort; }

    /// Where `point` ends up, given the text's unbent bounding box. Points
    /// outside the box are carried along by the same formula, which is what
    /// lets a glyph's overshoot bend with the rest of the line.
    QPointF map(const QPointF &point, const QRectF &bounds) const;

private:
    /// The displacement in normalised space, before the box is put back.
    QPointF displace(qreal u, qreal v) const;

    /// Arc and Arch, which bend the block around a circle rather than push
    /// its points about.
    ///
    /// The difference matters: under a displacement field the two ends of a
    /// letter move by different amounts and it comes out sheared, while
    /// wrapping the block onto an arc carries each letter around bodily and
    /// turns it to face along the curve — which is what Photoshop's arcs do,
    /// and why its letters stay letter-shaped however far they are bent.
    QPointF bendAroundArc(const QPointF &point, const QRectF &bounds, qreal theta) const;

    /// The shells, which hold one edge straight and wrap the other onto an
    /// arc, blending between the two across the text.
    ///
    /// The far edge fans the letters rather than merely lowering them: an arc
    /// spans less width than the straight line it replaces, so each letter's
    /// far edge is drawn in towards the middle and the letter leans. Moving
    /// points straight down instead leaves them standing upright, which is
    /// the difference between CS6's shell and a sagging line of text.
    QPointF bendBetweenEdges(const QPointF &point, const QRectF &bounds) const;

    /// Rise, which carries the text up an S-curve and turns each letter to
    /// stand on it. A plain sloping displacement leaves them upright on a
    /// tilted line, which reads as a sheared block rather than text climbing
    /// a hill.
    QPointF riseAlongCurve(const QPointF &point, const QRectF &bounds) const;

    int m_style = None;
    bool m_horizontal = true;
    qreal m_bend = 0.5;
    qreal m_hDistort = 0.0;
    qreal m_vDistort = 0.0;
};

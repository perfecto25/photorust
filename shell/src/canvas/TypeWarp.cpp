#include "TypeWarp.h"

#include <QObject>

#include <algorithm>
#include <cmath>

namespace {

/// A parabola peaking at the middle: 0 at the ends, 1 at u = 0.5.
qreal arch(qreal u)
{
    const qreal r = 2.0 * u - 1.0;
    return 1.0 - r * r;
}

/// An S-curve from 0 to 1: flat at both ends, steepest in the middle. CS6's
/// Rise starts and finishes level rather than climbing at a constant angle.
qreal ease(qreal t)
{
    const qreal c = std::clamp(t, 0.0, 1.0);
    return c * c * (3.0 - 2.0 * c);
}

/// Its slope, for turning the letters to stand on the curve.
qreal easeSlope(qreal t)
{
    const qreal c = std::clamp(t, 0.0, 1.0);
    return 6.0 * c * (1.0 - c);
}

/// A half sine over the span — the same rise and fall as `arch`, but flatter
/// across the middle, which is what separates CS6's Arch and Shell from its
/// Arc.
qreal hump(qreal u)
{
    return std::sin(M_PI * u);
}

} // namespace

QStringList TypeWarp::styleNames()
{
    // Indexed by Style, so the position in this list *is* the number the
    // record stores. Anything inserted here without a matching enum value
    // would renumber every style after it, and text bent one way would
    // reopen bent another.
    return {
        QObject::tr("None"),        QObject::tr("Arc"),        QObject::tr("Arc Lower"),
        QObject::tr("Arc Upper"),   QObject::tr("Arch"),       QObject::tr("Bulge"),
        QObject::tr("Shell Lower"), QObject::tr("Shell Upper"), QObject::tr("Flag"),
        QObject::tr("Wave"),        QObject::tr("Fish"),       QObject::tr("Rise"),
        QObject::tr("Fisheye"),     QObject::tr("Inflate"),    QObject::tr("Squeeze"),
        QObject::tr("Twist"),
    };
}

bool TypeWarp::startsGroup(int style)
{
    // CS6 groups them: None, then the arcs, then the arches and shells, then
    // the waves, then the radial four.
    return style == Arc || style == Arch || style == Flag || style == Fisheye;
}

bool TypeWarp::hasOrientation(int style)
{
    // The three radial styles bend about the centre rather than along an
    // axis, so there is no along-or-across to choose. CS6 greys the pair out
    // for exactly these.
    return style != Fisheye && style != Inflate && style != Twist;
}

QPointF TypeWarp::displace(qreal u, qreal v) const
{
    const qreal b = m_bend;
    // Everything below is in fractions of the text's own box, so the same
    // numbers bend 12pt and 120pt by the same proportion.
    switch (m_style) {
    case Arc:
        // The whole block rides the curve, ends anchored.
        return {0.0, -b * 0.5 * arch(u)};
    case ArcLower:
        // Only the foot of the text bends; the top stays a straight line.
        return {0.0, -b * 0.5 * arch(u) * v};
    case ArcUpper:
        return {0.0, -b * 0.5 * arch(u) * (1.0 - v)};
    case Arch:
        // Flatter over the middle than Arc, which is what makes it read as an
        // arch rather than a bow.
        return {0.0, -b * 0.5 * hump(u)};
    case Bulge:
        // Top and bottom part company in the middle: the letters grow taller
        // there rather than the line moving.
        return {0.0, b * 0.5 * arch(u) * (2.0 * v - 1.0)};
    case Flag:
        // One full wave along the text, the whole block travelling with it.
        return {0.0, b * 0.35 * std::sin(2.0 * M_PI * u)};
    case Wave:
        // Two frequencies, so the crests are uneven the way CS6's are.
        return {0.0,
                b * 0.3 * (std::sin(2.0 * M_PI * u) + 0.5 * std::sin(4.0 * M_PI * u))};
    default:
        break;
    }
    return {0.0, 0.0};
}

QPointF TypeWarp::bendAroundArc(const QPointF &point, const QRectF &bounds,
                                qreal theta) const
{
    // Worked in a frame where the bend runs "along" and the block's thickness
    // is "across", so Vertical is the same construction with the two axes
    // exchanged rather than a second formula to keep in step.
    const bool swap = !m_horizontal;
    const qreal alongLength = swap ? bounds.height() : bounds.width();
    const qreal alongCentre = swap ? bounds.center().y() : bounds.center().x();
    const qreal acrossCentre = swap ? bounds.center().x() : bounds.center().y();
    const qreal along = swap ? point.y() : point.x();
    const qreal across = swap ? point.x() : point.y();

    if (std::abs(theta) < 1e-4 || alongLength <= 0.0) {
        return point;
    }

    // Radius chosen so the arc is as long as the block was wide: bending
    // stretches nothing, it only curves.
    const qreal radius = alongLength / theta;
    const qreal angle = ((along - alongCentre) / alongLength) * theta;
    const qreal r = radius - (across - acrossCentre);

    const qreal newAlong = alongCentre + r * std::sin(angle);
    const qreal newAcross = acrossCentre + radius - r * std::cos(angle);
    return swap ? QPointF(newAcross, newAlong) : QPointF(newAlong, newAcross);
}

QPointF TypeWarp::bendBetweenEdges(const QPointF &point, const QRectF &bounds) const
{
    const qreal u = (point.x() - bounds.left()) / bounds.width();
    const qreal v = (point.y() - bounds.top()) / bounds.height();
    const qreal along = m_horizontal ? u : v;
    const qreal across = m_horizontal ? v : u;

    // Shell Lower wraps the foot of the text and holds its top; Shell Upper
    // is the same the other way up. At weight zero the point is left exactly
    // where it was, which is what keeps the near edge dead straight.
    const qreal weight = m_style == ShellLower ? across : 1.0 - across;
    if (weight <= 0.0) {
        return point;
    }

    // The bow is anchored at both ends and deepest in the middle. A circular
    // bend will not do here even though the far edge ends up curved: wrapping
    // the block round a circle lifts its ends as it sags the middle, and a
    // shell has to leave the ends of the anchored edge where they are.
    const qreal direction = m_style == ShellLower ? 1.0 : -1.0;
    const qreal bow = direction * m_bend * 0.9 * hump(along) * weight;

    // ...and the far edge is drawn in towards the middle, which is what fans
    // the letters. Without it they sag while standing bolt upright, where
    // CS6's lean to follow the curve.
    const qreal converge = 1.0 - std::abs(m_bend) * 0.22 * weight;

    if (m_horizontal) {
        return {bounds.left() + (0.5 + (u - 0.5) * converge) * bounds.width(),
                bounds.top() + (v + bow) * bounds.height()};
    }
    return {bounds.left() + (u + bow) * bounds.width(),
            bounds.top() + (0.5 + (v - 0.5) * converge) * bounds.height()};
}

QPointF TypeWarp::riseAlongCurve(const QPointF &point, const QRectF &bounds) const
{
    const bool swap = !m_horizontal;
    const qreal alongLength = swap ? bounds.height() : bounds.width();
    const qreal acrossLength = swap ? bounds.width() : bounds.height();
    const qreal alongStart = swap ? bounds.top() : bounds.left();
    const qreal acrossCentre = swap ? bounds.center().x() : bounds.center().y();
    const qreal along = swap ? point.y() : point.x();
    const qreal across = swap ? point.x() : point.y();
    if (alongLength <= 0.0 || acrossLength <= 0.0) {
        return point;
    }

    const qreal t = (along - alongStart) / alongLength;
    // How far the line has climbed by here, and how steeply it is climbing.
    const qreal climb = -m_bend * 0.6 * ease(t) * acrossLength;
    const qreal slope = -m_bend * 0.6 * easeSlope(t) * acrossLength / alongLength;

    // Each letter is turned to stand on the curve rather than left upright on
    // a tilted line, so it leans with the climb the way CS6's do.
    const qreal angle = std::atan(slope);
    const qreal offset = across - acrossCentre;
    const qreal newAlong = along - offset * std::sin(angle);
    const qreal newAcross = acrossCentre + climb + offset * std::cos(angle);
    return swap ? QPointF(newAcross, newAlong) : QPointF(newAlong, newAcross);
}

QPointF TypeWarp::map(const QPointF &point, const QRectF &bounds) const
{
    if (!isActive() || bounds.width() <= 0.0 || bounds.height() <= 0.0) {
        return point;
    }

    const qreal w = bounds.width();
    const qreal h = bounds.height();

    // Arc and Arch wrap the block onto a circle, which turns the letters as
    // it carries them; everything below only pushes points about.
    QPointF placed = point;
    if (m_style == Arc || m_style == Arch) {
        // Negative so a positive bend lifts the middle, matching CS6's slider.
        placed = bendAroundArc(point, bounds, -m_bend * M_PI * 0.6);
    } else if (m_style == ShellLower || m_style == ShellUpper) {
        placed = bendBetweenEdges(point, bounds);
    } else if (m_style == Rise) {
        placed = riseAlongCurve(point, bounds);
    }
    qreal u = (placed.x() - bounds.left()) / w;
    qreal v = (placed.y() - bounds.top()) / h;

    // Inflate swells the block rather than turning it about a centre: the
    // letters in the middle grow taller, and the ones at either end splay
    // outward at top and bottom into the parentheses CS6 makes of them.
    // Written as two scalings about the middle, because that is what puts the
    // movement at the edges — a radial falloff does the reverse, leaving the
    // rim untouched, which is why this used to barely move the ends at all.
    if (m_style == Squeeze) {
        // An hourglass: the waist is pinched and the ends keep their full
        // height. That is what the name describes, and it is why CS6's outer
        // letters are *tall* crescents rather than stubs — squeezing the ends
        // instead, as this did at first, pulls in all four corners and gives
        // the word a lens shape that Photoshop never makes.
        //
        // The curl comes free: this profile changes fastest at the ends, so
        // the letters there are sheared hardest and bow, while the ones at
        // the waist are scaled evenly and stay upright.
        // The waist closes and the ends flare out past where they began, the
        // way squeezing the middle of something has to send the rest of it
        // somewhere. Holding the ends at their original height instead — the
        // previous version — gives a much tamer shape than CS6's, whose
        // outline balloons at both ends.
        // Horizontal squeezes the text from its *ends*, pressing left and
        // right inward — not from above and below. The profile runs across
        // the text and the pinch is applied along it, which is the transpose
        // of what this did at first: same hourglass, laid the other way, and
        // the axis is the whole difference between CS6's shape and one turned
        // ninety degrees from it.
        const qreal across = m_horizontal ? v : u;
        const qreal waist = arch(across);
        const qreal pinch = std::max(0.05, 1.0 + m_bend * (0.8 * (1.0 - waist) - waist));
        if (m_horizontal) {
            u = 0.5 + (u - 0.5) * pinch;
        } else {
            v = 0.5 + (v - 0.5) * pinch;
        }
    } else if (m_style == Inflate) {
        // All four edges bow outward, each most at its own middle: the top
        // and bottom puff away from the centre line, and the left and right
        // ends bow out hardest at mid-height, which is what curls the first
        // and last letters into CS6's parentheses.
        //
        // The corners are the fixed points — both terms vanish there. That
        // matters: pushing hardest at the top and bottom instead moves a
        // letter's head further than its foot, and the word comes out
        // leaning rather than inflated.
        const qreal outward = 1.0 + m_bend * 0.25 * arch(v);
        const qreal upward = 1.0 + m_bend * 0.8 * arch(u);
        u = 0.5 + (u - 0.5) * outward;
        v = 0.5 + (v - 0.5) * upward;
    } else if (m_style == Fisheye) {
        // A lens over the middle of the word: the letters there are spread
        // apart and magnified, and the ones at the ends are crowded in.
        //
        // Worked as a remap along the text and a scale across it, rather than
        // pushing points radially. On a line of text — a box far wider than
        // it is tall — a radial field moves points a long way sideways and
        // barely at all vertically, and it changes so sharply inside a single
        // letter that the glyph is torn to pieces instead of magnified.
        const qreal centred = 2.0 * u - 1.0;
        const qreal spread = 1.0 / (1.0 + m_bend * 0.8);
        const qreal remapped =
            std::copysign(std::pow(std::abs(centred), spread), centred);
        v = 0.5 + (v - 0.5) * (1.0 + m_bend * 0.8 * arch(u));
        u = 0.5 + 0.5 * remapped;
    } else if (m_style == Twist) {
        // The text wrung about its own centre line: it turns edge-on in the
        // middle, where it flattens to a sliver, and lies flat again at the
        // ends. Turning the *whole box* about its centre instead — the last
        // attempt — swings the far ends through a huge arc and shreds them.
        const qreal offset = v - 0.5;
        const qreal angle = m_bend * (M_PI / 2.0) * arch(u);
        v = 0.5 + offset * std::cos(angle);
        u = u + offset * std::sin(angle) * (h / w);
    } else if (m_style == Fish) {
        // A body, a waist and a tail — not a taper. CS6's fish keeps both
        // ends of the text near full height and pinches it at one place
        // about two thirds along, which is what gives the outline its two
        // lobes. Narrowing steadily to a point instead makes a wedge, which
        // is a perfectly good shape and the wrong one.
        const qreal along = m_horizontal ? u : v;
        const qreal fromWaist = (along - 0.68) / 0.22;
        const qreal waist = std::exp(-fromWaist * fromWaist);
        const qreal keep = std::max(0.05, 1.0 - m_bend * 0.85 * waist);
        if (m_horizontal) {
            v = 0.5 + (v - 0.5) * keep;
        } else {
            u = 0.5 + (u - 0.5) * keep;
        }
    } else if (m_style == Arc || m_style == Arch || m_style == ShellLower
               || m_style == ShellUpper || m_style == Rise) {
        // Already placed on the arc above; only the distortions are left.
    } else if (m_horizontal) {
        const QPointF d = displace(u, v);
        u += d.x();
        v += d.y();
    } else {
        // Across the text instead of along it: the same curves with the two
        // axes exchanged, which is what the Vertical button means.
        //
        // Scaled by the box's aspect so the displacement stays the same
        // *physical* size as it would be horizontally. Without that, bending
        // a wide line sideways moves points by a fraction of its width while
        // the curve runs over the height of a single line of text — a slope
        // so steep that the top of each letter lands far from its foot, and
        // the word arrives shredded rather than bent.
        const QPointF d = displace(v, u);
        u += d.y() * (h / w);
        v += d.x() * (w / h);
    }

    // CS6's two Distortion sliders, which taper the block rather than bend
    // it: each pulls one axis apart at one end and together at the other.
    if (!qFuzzyIsNull(m_hDistort)) {
        u = 0.5 + (u - 0.5) * (1.0 + m_hDistort * (2.0 * v - 1.0));
    }
    if (!qFuzzyIsNull(m_vDistort)) {
        v = 0.5 + (v - 0.5) * (1.0 + m_vDistort * (2.0 * u - 1.0));
    }

    return {bounds.left() + u * w, bounds.top() + v * h};
}

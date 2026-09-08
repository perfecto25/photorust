// Warp Text's geometry.
//
// The curves themselves are reconstructions — Adobe does not publish them —
// so what is pinned here is not their exact shape but the properties that
// make a warp a warp: it does nothing when off, it is anchored where CS6
// anchors it, bending one way is the mirror of bending the other, and the
// text's bounding box actually grows to hold the bent letters. Those are the
// ways a warp goes wrong without looking obviously broken.

#include "canvas/TypeWarp.h"

#include <QTest>

#include <limits>

class TestTypeWarp : public QObject
{
    Q_OBJECT

private slots:
    void noneLeavesEveryPointWhereItWas();
    void zeroBendIsAlsoNoMovement();
    void everyStyleActuallyMovesSomething();
    void bendingBackIsTheMirrorOfBendingForward();
    void theArcsAnchorTheEdgeTheyAreNamedFor();
    void theShellsHoldTheEdgeTheyAreNamedAgainst();
    void theArcsAnchorTheirCentreAndSwingTheirEnds();
    void inflateBowsEveryEdgeOutwardAndPinsTheCorners();
    void squeezePressesTheEndsInwardNotTheEdgesDown();
    void riseClimbsAnSCurveAndLeansTheLetters();
    void fishPinchesAWaistBetweenTwoLobes();
    void fisheyeMagnifiesTheMiddleWithoutFolding();
    void twistGoesEdgeOnInTheMiddleWithoutFolding();
    void theStyleListLinesUpWithTheRecord();

private:
    static constexpr QRectF box() { return QRectF(100, 50, 200, 40); }
};

void TestTypeWarp::noneLeavesEveryPointWhereItWas()
{
    const TypeWarp off(TypeWarp::None, true, 0.5, 0.3, -0.2);
    QVERIFY(!off.isActive());
    // Style None means no warp whatever the sliders say — the dialog leaves
    // them where they were so switching back and forth keeps your settings.
    const QPointF p(150, 60);
    QCOMPARE(off.map(p, box()), p);
}

void TestTypeWarp::zeroBendIsAlsoNoMovement()
{
    for (int style = TypeWarp::Arc; style < TypeWarp::Count; ++style) {
        const TypeWarp flat(style, true, 0.0, 0.0, 0.0);
        const QPointF p(180, 70);
        const QPointF moved = flat.map(p, box());
        QVERIFY2(QLineF(p, moved).length() < 0.001,
                 qPrintable(QStringLiteral("style %1 moves at zero bend").arg(style)));
    }
}

void TestTypeWarp::everyStyleActuallyMovesSomething()
{
    // A style that quietly does nothing would look like the menu entry not
    // working, which is easy to miss among fifteen of them.
    for (int style = TypeWarp::Arc; style < TypeWarp::Count; ++style) {
        const TypeWarp warp(style, true, 0.5, 0.0, 0.0);
        QVERIFY(warp.isActive());

        qreal furthest = 0.0;
        for (int i = 0; i <= 10; ++i) {
            for (int j = 0; j <= 10; ++j) {
                const QPointF p(box().left() + box().width() * i / 10.0,
                                box().top() + box().height() * j / 10.0);
                furthest = std::max(furthest, QLineF(p, warp.map(p, box())).length());
            }
        }
        QVERIFY2(furthest > 1.0,
                 qPrintable(QStringLiteral("style %1 bends nothing at +50%%").arg(style)));
    }
}

void TestTypeWarp::bendingBackIsTheMirrorOfBendingForward()
{
    // CS6's bend runs -100 to +100 through a flat zero, so the two halves of
    // the slider have to be opposites rather than one side doing nothing.
    for (int style = TypeWarp::Arc; style < TypeWarp::Count; ++style) {
        if (style == TypeWarp::Twist || style == TypeWarp::Fisheye) {
            // One wrings the text and the other magnifies it, and neither is
            // linear in the bend — a rotation and a power curve respectively —
            // so reversing it is not a reflection. Inflate is *not* among
            // them: it scales the block about its middle, which is linear,
            // and so does mirror.
            continue;
        }
        if (style == TypeWarp::Rise) {
            // Rise turns each letter to stand on its curve, and a rotation is
            // not a reflection — see `riseClimbsAnSCurveAndLeansTheLetters`.
            continue;
        }
        if (style == TypeWarp::Arc || style == TypeWarp::Arch) {
            // These wrap the block onto a circle rather than push its points
            // around, and a bend is not a reflection: the letters turn to
            // face along the curve, so the two directions are not mirror
            // images. `theArcsAnchorTheirCentreAndSwingTheirEnds` is what
            // holds for them instead.
            continue;
        }
        const TypeWarp forward(style, true, 0.5, 0.0, 0.0);
        const TypeWarp back(style, true, -0.5, 0.0, 0.0);
        const QPointF p(box().left() + box().width() * 0.37,
                        box().top() + box().height() * 0.6);
        const QPointF a = forward.map(p, box());
        const QPointF b = back.map(p, box());
        QVERIFY2(std::abs((a.y() - p.y()) + (b.y() - p.y())) < 0.001,
                 qPrintable(QStringLiteral("style %1 is not symmetric about zero").arg(style)));
    }
}

void TestTypeWarp::theArcsAnchorTheEdgeTheyAreNamedFor()
{
    const QRectF b = box();
    const QPointF onTop(b.left() + b.width() * 0.5, b.top());
    const QPointF onBottom(b.left() + b.width() * 0.5, b.bottom());

    // Arc Lower bends the foot of the text and leaves the top alone; Arc
    // Upper does the reverse. Getting the two the wrong way round is a
    // one-character mistake that still produces a plausible curve.
    const TypeWarp lower(TypeWarp::ArcLower, true, 0.5, 0.0, 0.0);
    QVERIFY(std::abs(lower.map(onTop, b).y() - onTop.y()) < 0.001);
    QVERIFY(std::abs(lower.map(onBottom, b).y() - onBottom.y()) > 1.0);

    const TypeWarp upper(TypeWarp::ArcUpper, true, 0.5, 0.0, 0.0);
    QVERIFY(std::abs(upper.map(onBottom, b).y() - onBottom.y()) < 0.001);
    QVERIFY(std::abs(upper.map(onTop, b).y() - onTop.y()) > 1.0);
}

void TestTypeWarp::theShellsHoldTheEdgeTheyAreNamedAgainst()
{
    const QRectF b = box();
    const QPointF onTop(b.left() + b.width() * 0.5, b.top());
    const QPointF onBottom(b.left() + b.width() * 0.5, b.bottom());

    // A shell bows one edge and leaves the other dead straight. Which edge
    // stays put is the whole difference between the two styles, and it only
    // holds if the warp is measured against the box the letters fill — bend a
    // line box instead, with its ascender space above the capitals, and the
    // "straight" edge drifts by whatever that gap comes to.
    const TypeWarp lower(TypeWarp::ShellLower, true, 1.0, 0.0, 0.0);
    QVERIFY2(std::abs(lower.map(onTop, b).y() - onTop.y()) < 0.001,
             "Shell Lower moved the top edge, which it is supposed to hold");
    QVERIFY2(lower.map(onBottom, b).y() > onBottom.y() + 1.0,
             "Shell Lower did not bow the bottom edge down");

    const TypeWarp upper(TypeWarp::ShellUpper, true, 1.0, 0.0, 0.0);
    QVERIFY2(std::abs(upper.map(onBottom, b).y() - onBottom.y()) < 0.001,
             "Shell Upper moved the bottom edge, which it is supposed to hold");
    QVERIFY2(upper.map(onTop, b).y() < onTop.y() - 1.0,
             "Shell Upper did not arch the top edge up");

    // The bowed edge is also drawn in towards the middle, which is what leans
    // the letters into CS6's fan. Bowing without it leaves them sagging bolt
    // upright, which was the last thing wrong with these two.
    const QPointF footLeft(b.left(), b.bottom());
    QVERIFY2(lower.map(footLeft, b).x() > footLeft.x() + 1.0,
             "Shell Lower did not draw the foot of the text inward");
    const QPointF headLeft(b.left(), b.top());
    QVERIFY2(upper.map(headLeft, b).x() > headLeft.x() + 1.0,
             "Shell Upper did not draw the head of the text inward");
    // And the ends of the *anchored* edge stay put, so the text is bowed
    // rather than picked up at the corners.
    QVERIFY(QLineF(headLeft, lower.map(headLeft, b)).length() < 0.001);
    QVERIFY(QLineF(footLeft, upper.map(footLeft, b)).length() < 0.001);
}

void TestTypeWarp::theArcsAnchorTheirCentreAndSwingTheirEnds()
{
    const QRectF b = box();
    for (int style : {int(TypeWarp::Arc), int(TypeWarp::Arch)}) {
        for (bool horizontal : {true, false}) {
            const TypeWarp forward(style, horizontal, 0.5, 0.0, 0.0);
            const TypeWarp back(style, horizontal, -0.5, 0.0, 0.0);

            // Wrapping a block onto an arc pins the middle of it: that is the
            // point the curve is hung from, and it is what stops a bend from
            // sliding the whole word sideways.
            const QPointF centre = b.center();
            QVERIFY2(QLineF(centre, forward.map(centre, b)).length() < 0.001,
                     qPrintable(QStringLiteral("style %1 does not hold its centre").arg(style)));

            // And the far end swings the other way when the bend is reversed.
            const QPointF end = horizontal ? QPointF(b.left(), centre.y())
                                           : QPointF(centre.x(), b.top());
            const qreal a = horizontal ? forward.map(end, b).y() - end.y()
                                       : forward.map(end, b).x() - end.x();
            const qreal c = horizontal ? back.map(end, b).y() - end.y()
                                       : back.map(end, b).x() - end.x();
            QVERIFY2(std::abs(a) > 1.0 && std::abs(c) > 1.0 && (a > 0) != (c > 0),
                     qPrintable(QStringLiteral("style %1 end moved %2 and %3, not opposite ways")
                                    .arg(style)
                                    .arg(a)
                                    .arg(c)));
        }
    }
}

void TestTypeWarp::inflateBowsEveryEdgeOutwardAndPinsTheCorners()
{
    const QRectF b = box();
    const TypeWarp inflate(TypeWarp::Inflate, true, 0.5, 0.0, 0.0);

    // Inflating pushes each edge out at its own middle and holds the corners.
    // Two earlier attempts at this style failed exactly here: a radial
    // falloff left the edge midpoints almost where they were, and pushing
    // hardest at the top and bottom moved the corners — which tilts the
    // letters instead of swelling them.
    for (const QPointF &corner :
         {b.topLeft(), b.topRight(), b.bottomLeft(), b.bottomRight()}) {
        QVERIFY2(QLineF(corner, inflate.map(corner, b)).length() < 0.001,
                 "inflate moved a corner, which leans the letters");
    }

    const QPointF topMid(b.center().x(), b.top());
    const QPointF bottomMid(b.center().x(), b.bottom());
    const QPointF leftMid(b.left(), b.center().y());
    const QPointF rightMid(b.right(), b.center().y());

    QVERIFY2(inflate.map(topMid, b).y() < topMid.y() - 1.0, "the top edge did not bow up");
    QVERIFY2(inflate.map(bottomMid, b).y() > bottomMid.y() + 1.0,
             "the bottom edge did not bow down");
    QVERIFY2(inflate.map(leftMid, b).x() < leftMid.x() - 1.0,
             "the left end did not bow outward");
    QVERIFY2(inflate.map(rightMid, b).x() > rightMid.x() + 1.0,
             "the right end did not bow outward");
}

void TestTypeWarp::squeezePressesTheEndsInwardNotTheEdgesDown()
{
    const QRectF b = box();
    const TypeWarp squeeze(TypeWarp::Squeeze, true, 1.0, 0.0, 0.0);

    // Horizontal squeeze presses the *ends* of the text inward — left and
    // right — rather than pressing its top and bottom together. Getting that
    // axis the wrong way round gives CS6's shape turned ninety degrees, which
    // still looks like a squeeze and is still wrong.
    const QPointF leftMid(b.left(), b.center().y());
    const QPointF rightMid(b.right(), b.center().y());
    QVERIFY2(squeeze.map(leftMid, b).x() > leftMid.x() + 1.0,
             "the left end should be pressed inward");
    QVERIFY2(squeeze.map(rightMid, b).x() < rightMid.x() - 1.0,
             "the right end should be pressed inward");

    // Nothing is pressed vertically: the squeeze is along one axis only.
    QVERIFY(std::abs(squeeze.map(leftMid, b).y() - leftMid.y()) < 0.001);

    // What leaves the waist has to go somewhere, so the top and bottom edges
    // spread outward instead.
    QVERIFY2(squeeze.map(b.topLeft(), b).x() < b.topLeft().x() - 1.0,
             "the top edge should spread outward, not stay put");
    QVERIFY2(squeeze.map(b.bottomRight(), b).x() > b.bottomRight().x() + 1.0,
             "the bottom edge should spread outward, not stay put");

    // The centre is what everything closes onto.
    QVERIFY(QLineF(b.center(), squeeze.map(b.center(), b)).length() < 0.001);

    // Vertical presses top and bottom together instead — the transpose.
    const TypeWarp across(TypeWarp::Squeeze, false, 1.0, 0.0, 0.0);
    const QPointF topMid(b.center().x(), b.top());
    QVERIFY(across.map(topMid, b).y() > topMid.y() + 1.0);
    QVERIFY(std::abs(across.map(topMid, b).x() - topMid.x()) < 0.001);
}

void TestTypeWarp::riseClimbsAnSCurveAndLeansTheLetters()
{
    const QRectF b = box();
    const TypeWarp rise(TypeWarp::Rise, true, 1.0, 0.0, 0.0);

    // The climb eases in and out rather than running at one angle the whole
    // way: CS6's Rise leaves both ends level and does its climbing in the
    // middle, which a straight ramp does not.
    auto heightAt = [&](qreal t) {
        const QPointF p(b.left() + b.width() * t, b.center().y());
        return rise.map(p, b).y();
    };
    const qreal start = heightAt(0.0);
    const qreal quarter = heightAt(0.25);
    const qreal middle = heightAt(0.5);
    const qreal threeQuarters = heightAt(0.75);
    const qreal finish = heightAt(1.0);

    QVERIFY2(finish < start - 1.0, "Rise did not climb");
    // Steeper across the middle than at either end — the S.
    const qreal firstQuarter = start - quarter;
    const qreal middleHalf = quarter - threeQuarters;
    const qreal lastQuarter = threeQuarters - finish;
    QVERIFY2(middleHalf / 2.0 > firstQuarter && middleHalf / 2.0 > lastQuarter,
             "Rise climbs at a constant angle — it is a ramp, not an S-curve");

    // And the letters lean into it: two points on the same vertical run apart
    // horizontally once the curve turns them.
    const qreal steepest = b.left() + b.width() * 0.5;
    const QPointF head(steepest, b.top());
    const QPointF foot(steepest, b.bottom());
    QVERIFY2(std::abs(rise.map(head, b).x() - rise.map(foot, b).x()) > 1.0,
             "Rise left the letters upright on a tilted line");

    // Reversing the bend sends it downhill instead.
    const TypeWarp down(TypeWarp::Rise, true, -1.0, 0.0, 0.0);
    const QPointF end(b.right(), b.center().y());
    QVERIFY((rise.map(end, b).y() - end.y()) * (down.map(end, b).y() - end.y()) < 0.0);
}

void TestTypeWarp::fishPinchesAWaistBetweenTwoLobes()
{
    const QRectF b = box();
    const TypeWarp fish(TypeWarp::Fish, true, 1.0, 0.0, 0.0);

    auto heightAt = [&](qreal t) {
        const qreal x = b.left() + b.width() * t;
        return fish.map(QPointF(x, b.bottom()), b).y()
            - fish.map(QPointF(x, b.top()), b).y();
    };

    // A body, a waist and a tail. Narrowing steadily to a point instead gives
    // a wedge — a perfectly plausible shape, and the wrong one, which is what
    // this replaced.
    QVERIFY2(heightAt(0.0) > b.height() * 0.9, "the head of the fish is not full height");
    QVERIFY2(heightAt(1.0) > b.height() * 0.6, "the tail did not flare back out");

    // The waist is somewhere past the middle and closes to well under half.
    qreal narrowest = b.height();
    qreal waistAt = 0.0;
    for (int i = 0; i <= 40; ++i) {
        const qreal t = i / 40.0;
        if (heightAt(t) < narrowest) {
            narrowest = heightAt(t);
            waistAt = t;
        }
    }
    QVERIFY2(narrowest < b.height() * 0.4,
             qPrintable(QStringLiteral("the waist only closed to %1").arg(narrowest)));
    QVERIFY2(waistAt > 0.5 && waistAt < 0.9,
             qPrintable(QStringLiteral("the waist sits at %1, not past the middle").arg(waistAt)));

    // It closes onto the centre line, so the text pinches rather than sinking.
    const QPointF centre = b.center();
    QVERIFY(QLineF(centre, fish.map(centre, b)).length() < 0.001);
}

/// Whether a style keeps the text in order along its length.
///
/// A warp that runs backwards on itself — mapping a later part of the word to
/// an earlier position — folds the letters through each other, and a glyph
/// caught in the fold comes out as scattered fragments rather than a letter.
/// Both of these styles arrived that way, so this is the property worth
/// holding them to: sample across the text at several heights, and check the
/// mapping never doubles back.
static bool staysInOrderAlong(const TypeWarp &warp, const QRectF &b)
{
    for (int row = 0; row <= 4; ++row) {
        const qreal y = b.top() + b.height() * row / 4.0;
        qreal previous = -std::numeric_limits<qreal>::max();
        for (int i = 0; i <= 60; ++i) {
            const qreal x = b.left() + b.width() * i / 60.0;
            const qreal mapped = warp.map(QPointF(x, y), b).x();
            if (mapped < previous - 0.001) {
                return false;
            }
            previous = mapped;
        }
    }
    return true;
}

void TestTypeWarp::fisheyeMagnifiesTheMiddleWithoutFolding()
{
    const QRectF b = box();
    const TypeWarp fisheye(TypeWarp::Fisheye, true, 1.0, 0.0, 0.0);

    auto heightAt = [&](qreal t) {
        const qreal x = b.left() + b.width() * t;
        return fisheye.map(QPointF(x, b.bottom()), b).y()
            - fisheye.map(QPointF(x, b.top()), b).y();
    };

    // A lens: bigger in the middle than at the ends.
    QVERIFY2(heightAt(0.5) > b.height() * 1.4, "the middle is not magnified");
    QVERIFY2(heightAt(0.0) < heightAt(0.5), "the ends are not smaller than the middle");
    QVERIFY2(staysInOrderAlong(fisheye, b),
             "fisheye folds the text back on itself, which shreds the letters");
}

void TestTypeWarp::twistGoesEdgeOnInTheMiddleWithoutFolding()
{
    const QRectF b = box();
    const TypeWarp twist(TypeWarp::Twist, true, 1.0, 0.0, 0.0);

    auto heightAt = [&](qreal t) {
        const qreal x = b.left() + b.width() * t;
        return twist.map(QPointF(x, b.bottom()), b).y()
            - twist.map(QPointF(x, b.top()), b).y();
    };

    // Wrung about its own centre line: edge-on in the middle, flat at the ends.
    QVERIFY2(heightAt(0.5) < b.height() * 0.1, "the middle did not turn edge-on");
    QVERIFY2(heightAt(0.0) > b.height() * 0.9, "the ends should lie flat and full height");
    QVERIFY2(heightAt(1.0) > b.height() * 0.9, "the ends should lie flat and full height");
    QVERIFY2(staysInOrderAlong(twist, b),
             "twist folds the text back on itself, which shreds the letters");
}

void TestTypeWarp::theStyleListLinesUpWithTheRecord()
{
    // The style is stored as its position in this list, so a name inserted
    // without a separator to match would silently renumber every style after
    // it — text bent one way would reopen bent another.
    const QStringList names = TypeWarp::styleNames();
    QCOMPARE(names.size(), int(TypeWarp::Count));
    QCOMPARE(names.at(TypeWarp::None), QStringLiteral("None"));
    QCOMPARE(names.at(TypeWarp::ArcLower), QStringLiteral("Arc Lower"));
    QCOMPARE(names.at(TypeWarp::ShellUpper), QStringLiteral("Shell Upper"));
    QCOMPARE(names.at(TypeWarp::Arc), QStringLiteral("Arc"));
    QCOMPARE(names.at(TypeWarp::Twist), QStringLiteral("Twist"));

    // The radial three are the ones CS6 greys the orientation pair out for.
    QVERIFY(!TypeWarp::hasOrientation(TypeWarp::Fisheye));
    QVERIFY(!TypeWarp::hasOrientation(TypeWarp::Inflate));
    QVERIFY(!TypeWarp::hasOrientation(TypeWarp::Twist));
    QVERIFY(TypeWarp::hasOrientation(TypeWarp::Arc));
    QVERIFY(TypeWarp::hasOrientation(TypeWarp::Flag));
}

QTEST_MAIN(TestTypeWarp)
#include "tst_typewarp.moc"

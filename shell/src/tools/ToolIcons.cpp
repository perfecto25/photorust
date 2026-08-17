#include "ToolIcons.h"

#include <QGuiApplication>
#include <QHash>
#include <QPainter>
#include <QPixmap>
#include <QSvgRenderer>

namespace {

/// All artwork is authored on this grid.
constexpr int kViewBox = 20;
/// Logical icon size in the strip. CS6's tool glyphs sit in a 26px button.
constexpr int kIconSize = 20;

/// Default stroke attributes, matching CS6's single-weight line art. Icons
/// override these per-element where they need a solid fill.
const char *const kStrokeAttrs =
    R"SVG(fill="none" stroke="COLOR" stroke-width="1.25" )SVG"
    R"SVG(stroke-linecap="round" stroke-linejoin="round")SVG";

/// The cursor arrow both path selection tools are drawn from — filled for one,
/// hollow for the other (see `pathSelectVariantBody`). Kept in one place so the
/// pair cannot drift apart.
const char *const kPathArrow = "M5 1.8 5 16.2 8.5 12.7 10.9 17.8 13.4 16.6 11 11.6 15.8 11.6z";

/// SVG body for each tool, on a 20×20 viewBox.
///
/// `COLOR` is substituted at render time. Elements that want a solid
/// silhouette say so explicitly; everything else inherits the group stroke.
QString bodyFor(ToolId id)
{
    switch (id) {
    // A four-way arrow. CS6 pairs a pointer with this cross, but at 20px the
    // pointer muddies the shape, and the cross alone still reads as "move".
    case ToolId::Move:
        return R"SVG(<path fill="COLOR" stroke="none" d="M10 1.4 12.7 4.6 11 4.6 11 9 15.4 9
                  15.4 7.3 18.6 10 15.4 12.7 15.4 11 11 11 11 15.4 12.7 15.4 10 18.6
                  7.3 15.4 9 15.4 9 11 4.6 11 4.6 12.7 1.4 10 4.6 7.3 4.6 9 9 9 9 4.6 7.3 4.6z"/>)SVG";

    case ToolId::Marquee:
        return R"SVG(<rect x="2.6" y="4.7" width="14.8" height="10.6" fill="none" stroke="COLOR"
                  stroke-width="1.2" stroke-dasharray="2.6 1.9"/>)SVG";

    case ToolId::Lasso:
        return R"SVG(<path d="M12.9 13.4c2.6-1.2 3.8-3.9 2.5-6.2C13.9 4.4 9.7 3.2 6.4 4.7
                  3.2 6.2 2.2 9.4 4 11.6c1.3 1.6 3.6 2.2 5.5 1.4"/>
                  <path d="M12.9 13.4c-.5 1.7-2 2.5-2.3 3.8-.2 1.1.8 1.9 1.8 1.5"/>)SVG";

    // A dashed brush footprint with the "quick" sparkle CS6 puts on it.
    case ToolId::QuickSelect:
        return R"SVG(<circle cx="8" cy="12.4" r="4.4" fill="none" stroke="COLOR"
                  stroke-width="1.2" stroke-dasharray="2.1 1.7"/>
                  <path d="M12.6 8 16.6 4"/>
                  <path fill="COLOR" stroke="none" d="M15.2 1.6 16 3.8 18.2 4.6 16 5.4
                  15.2 7.6 14.4 5.4 12.2 4.6 14.4 3.8z"/>)SVG";

    case ToolId::Crop:
        return R"SVG(<path d="M5.8 1.4V14.2H18.6"/><path d="M1.4 5.8H14.2V18.6"/>)SVG";

    case ToolId::Eyedropper:
        return R"SVG(<path fill="COLOR" stroke="none" d="M14.2 2.4a2.6 2.6 0 0 1 3.7 3.7l-1.9 1.9
                  -3.7-3.7z"/>
                  <path d="M12.3 4.3 3.5 13.1 2.4 17.6 6.9 16.5 15.7 7.7"/>
                  <path d="M11 6.4 13.6 9"/>)SVG";

    // Spot Healing Brush: the bandage, angled as CS6 draws it, with the small
    // spot that tells it apart from the plain Healing Brush.
    case ToolId::Healing:
        return R"SVG(<g transform="rotate(-45 10 10)">
                  <rect x="2.4" y="7.3" width="15.2" height="5.4" rx="2.7" fill="none"
                  stroke="COLOR" stroke-width="1.25"/>
                  <path d="M7.6 7.3V12.7M12.4 7.3V12.7"/></g>
                  <circle cx="15.9" cy="4.1" r="1.9" fill="COLOR" stroke="none"/>)SVG";

    case ToolId::Brush:
        return R"SVG(<path fill="COLOR" stroke="none" d="M17.1 2.9c.9.9.9 2 0 2.9l-6.3 6.3-2.9-2.9
                  6.3-6.3c.9-.9 2-.9 2.9 0z"/>
                  <path d="M7.9 9.2 10.8 12.1c.3 2.4-1.2 4.2-3.2 4.9-1.6.5-4 .5-4 .5
                  1.1-1 1.7-1.7 2-3 .3-1.6.6-3.9 2.3-5.3z"/>)SVG";

    case ToolId::CloneStamp:
        return R"SVG(<path d="M3.4 17.6H16.6"/>
                  <path d="M4.6 14.4H15.4L13.3 10.4H6.7z"/>
                  <path d="M8.3 10.4V5.6a1.7 1.7 0 0 1 3.4 0v4.8"/>)SVG";

    case ToolId::HistoryBrush:
        return R"SVG(<path fill="COLOR" stroke="none" d="M16.4 8.4c.7.7.7 1.6 0 2.3l-4.6 4.6-2.3-2.3
                  4.6-4.6c.7-.7 1.6-.7 2.3 0z"/>
                  <path d="M9.5 13 11.8 15.3c.2 1.8-.9 3.1-2.4 3.6-1.2.4-3 .4-3 .4
                  .8-.7 1.3-1.3 1.5-2.2.2-1.2.4-2.9 1.6-4.1z"/>
                  <path d="M3.6 8.6a5 5 0 1 1 1.5 3.5"/>
                  <path fill="COLOR" stroke="none" d="M2 5.6 6.2 6.4 3.4 9.6z"/>)SVG";

    case ToolId::Eraser:
        return R"SVG(<path d="M7.4 17.4 2.6 12.6 10.8 4.4a2 2 0 0 1 2.8 0l3.4 3.4a2 2 0 0 1 0 2.8
                  L12.6 17.4z"/>
                  <path d="M8.6 6.6 13.4 11.4"/>
                  <path d="M7.4 17.4H17.4"/>)SVG";

    case ToolId::Gradient:
        return R"SVG(<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="0">
                  <stop offset="0" stop-color="COLOR" stop-opacity="1"/>
                  <stop offset="1" stop-color="COLOR" stop-opacity="0.05"/>
                  </linearGradient></defs>
                  <rect x="2.6" y="5.6" width="14.8" height="8.8" fill="url(#g)"
                  stroke="COLOR" stroke-width="1.2"/>)SVG";

    case ToolId::Blur:
        return R"SVG(<path d="M10 2.2c0 0 5.7 6.4 5.7 9.6a5.7 5.7 0 0 1-11.4 0C4.3 8.6 10 2.2 10 2.2z"/>)SVG";

    // The darkroom dodging paddle: a small disc on a thin wire, held between the
    // enlarger and the paper. Deliberately slimmer than the Zoom tool's
    // magnifier, which is otherwise the same silhouette.
    case ToolId::Dodge:
        return R"SVG(<circle cx="7.4" cy="7.4" r="3.9" fill="none" stroke="COLOR" stroke-width="1.25"/>
                  <path d="M10.2 10.2 17.4 17.4" stroke-width="1.4"/>)SVG";

    // The fountain-pen nib CS6 uses for the whole Pen group: an angular kite
    // tapering to the tip that draws, with the slit and breather hole real
    // nibs have. Angled tip-down-left like Brush and Eyedropper.
    case ToolId::Pen:
        return R"SVG(<g transform="rotate(-45 10 10)">
                  <path d="M10 18.4 6.6 8.6 8.5 3.2 12.6 4.1 13.2 9.4Z"/>
                  <path d="M10.1 10V17"/>
                  <circle cx="9.9" cy="8.2" r="1.05" fill="COLOR" stroke="none"/></g>)SVG";

    case ToolId::Type:
        return R"SVG(<path d="M3.8 3.6H16.2M10 3.6V16.4M7.2 16.4H12.8"/>)SVG";

    case ToolId::PathSelect:
        return QStringLiteral(R"SVG(<path fill="COLOR" stroke="none" d="%1"/>)SVG")
            .arg(QLatin1String(kPathArrow));

    case ToolId::Shape:
        return R"SVG(<rect x="2.8" y="5.4" width="14.4" height="9.2" fill="COLOR" stroke="none"/>)SVG";

    case ToolId::Hand:
        return R"SVG(<path d="M6.6 18c-1.9-1.5-3.3-3.8-3.5-5.9l-.2-2.3c-.1-1.3 1.8-1.5 2-.2l.3 2V5.1
                  c0-1.4 2.1-1.4 2.1 0v4.6V3.4c0-1.4 2.1-1.4 2.1 0v6.2V4.3c0-1.4 2.1-1.4 2.1 0v5.3
                  V6.5c0-1.3 2-1.3 2 0v5.9c0 2.8-1.5 5.6-3.2 5.6z"/>)SVG";

    case ToolId::Zoom:
        return R"SVG(<circle cx="8.4" cy="8.4" r="5.3" fill="none" stroke="COLOR" stroke-width="1.25"/>
                  <path d="M12.5 12.5 17.8 17.8" stroke-width="2.3"/>
                  <path d="M5.8 8.4H11M8.4 5.8V11"/>)SVG";

    case ToolId::Count:
        break;
    }
    return {};
}

/// Per-variant artwork for the crop group.
///
/// CS6 gives all four entries their own icon: the plain crop brackets, the same
/// shape with a perspective mesh inside, a blade cutting a framed rectangle,
/// and that rectangle with a pointer over it.
QString cropVariantBody(int variant)
{
    switch (static_cast<CropType>(variant)) {
    // The crop brackets drawn as a slight trapezoid — narrower at the top, the
    // way a plane recedes — with a mesh inside showing the straightening.
    // A single cross of guides keeps the mesh legible at 20px; the four-line
    // grid CS6 draws turns into a solid blob at this size.
    case CropType::Perspective:
        return R"SVG(<path stroke-width="1.15" d="M6.6 4.4 13.4 4.4 16.6 15.6 3.4 15.6z"/>
                  <path stroke-width="0.75" d="M10 4.4V15.6M4.9 10H15.1"/>)SVG";

    // A framed rectangle with a craft-knife blade cutting across its corner.
    case CropType::Slice:
        return R"SVG(<path stroke-width="1.1" d="M2.4 4.4H12.6V16.4H2.4z"/>
                  <path fill="COLOR" stroke="none" d="M17.6 2.2 18.6 3.2 11.4 10.4 10.3 11.5
                  10.6 9.3z"/>
                  <path stroke-width="1" d="M10.3 11.5 8.6 13.2"/>)SVG";

    // The same rectangle with CS6's solid arrow pointer over it. The pointer is
    // the Path Selection arrow, scaled down and set into the lower right so it
    // reads as a separate shape rather than merging with the frame.
    case CropType::SliceSelect:
        return R"SVG(<path stroke-width="1.1" d="M2.4 4.4H12.6V16.4H2.4z"/>
                  <g transform="translate(7.3 6.6) scale(0.62)">
                  <path fill="COLOR" stroke="COLOR" stroke-width="1.6" stroke-linejoin="round"
                  d="M5 1.8 5 16.2 8.5 12.7 10.9 17.8 13.4 16.6 11 11.6 15.8 11.6z"/></g>)SVG";

    case CropType::Rectangular:
        break;
    }
    return bodyFor(ToolId::Crop);
}

/// Per-variant artwork for the lasso group.
///
/// All three share the loop-with-a-tail silhouette; what distinguishes them is
/// how the loop is drawn — freehand curve, straight segments, or a curve with
/// the fastening points the magnetic wire drops along it.
QString lassoVariantBody(int variant)
{
    // The curly tail is common to all three, as it is in CS6.
    const QString tail =
        R"SVG(<path d="M12.9 13.4c-.5 1.7-2 2.5-2.3 3.8-.2 1.1.8 1.9 1.8 1.5"/>)SVG";

    switch (static_cast<LassoType>(variant)) {
    // Straight segments rather than a curve: the polygon the tool builds.
    case LassoType::Polygonal:
        return QStringLiteral(R"SVG(<path d="M12.9 13.4 15.6 8.4 11.6 4.3 5.6 5.1 3.6 9.8
                  9.4 12.9"/>)SVG") + tail;

    // The same loop, beaded: a dashed curve with solid fastening points on it,
    // the anchors the magnetic wire drops as it clings to an edge. Solid dots
    // on a solid curve just thicken it — at 20px the dashes are what make this
    // read as a different tool.
    case LassoType::Magnetic:
        return QStringLiteral(R"SVG(<path stroke-dasharray="2.6 1.9" d="M12.9 13.4c2.6-1.2
                  3.8-3.9 2.5-6.2C13.9 4.4 9.7 3.2 6.4 4.7 3.2 6.2 2.2 9.4 4 11.6c1.3 1.6
                  3.6 2.2 5.5 1.4"/>
                  <g fill="COLOR" stroke="none">
                  <rect x="14.2" y="6.1" width="2.6" height="2.6"/>
                  <rect x="1.9" y="9.6" width="2.6" height="2.6"/>
                  <rect x="8.3" y="2.5" width="2.6" height="2.6"/></g>)SVG")
            + tail;

    case LassoType::Freehand:
        break;
    }
    return bodyFor(ToolId::Lasso);
}

/// Per-variant artwork for the Quick Selection group.
QString quickSelectVariantBody(int variant)
{
    // A wand on the diagonal with sparkles at its tip, as CS6 draws it.
    if (static_cast<QuickSelectType>(variant) == QuickSelectType::MagicWand) {
        return R"SVG(<path stroke-width="2" d="M2.8 17.2 10.6 9.4"/>
                  <path stroke-width="1.1" d="M9.4 8.2 11.8 10.6"/>
                  <g fill="COLOR" stroke="none">
                  <path d="M15 2.2 15.8 4.5 18.1 5.3 15.8 6.1 15 8.4 14.2 6.1 11.9 5.3 14.2 4.5z"/>
                  <path d="M17.2 9.6 17.7 11 19.1 11.5 17.7 12 17.2 13.4 16.7 12 15.3 11.5
                  16.7 11z"/></g>)SVG";
    }
    return bodyFor(ToolId::QuickSelect);
}

/// Per-variant artwork for the eyedropper group.
///
/// CS6 gives each a plain object: the pipette with a target crosshair, an
/// angled ruler, a lined note page, and the digits 1-2-3.
QString eyedropperVariantBody(int variant)
{
    switch (static_cast<EyedropperType>(variant)) {
    // The pipette shrunk to make room for the crosshair target CS6 sets beside
    // it — the point whose value the sampler pins.
    case EyedropperType::ColorSampler:
        return R"SVG(<g transform="translate(-1.4 -1.4) scale(0.86)">
                  <path fill="COLOR" stroke="none" d="M14.2 2.4a2.6 2.6 0 0 1 3.7 3.7l-1.9 1.9
                  -3.7-3.7z"/>
                  <path d="M12.3 4.3 3.5 13.1 2.4 17.6 6.9 16.5 15.7 7.7"/></g>
                  <circle cx="14.6" cy="14.6" r="3.1" fill="none" stroke="COLOR"
                  stroke-width="1.1"/>
                  <path stroke-width="1" d="M14.6 10.7V18.5M10.7 14.6H18.5"/>)SVG";

    // A straightedge on the diagonal with graduations along one side.
    case EyedropperType::Ruler:
        return R"SVG(<g transform="rotate(-32 10 10)">
                  <rect x="1.6" y="7.4" width="16.8" height="5.2" fill="none" stroke="COLOR"
                  stroke-width="1.2"/>
                  <path stroke-width="0.95" d="M5 7.4V10.4M8.4 7.4V9.4M11.8 7.4V10.4
                  M15.2 7.4V9.4"/></g>)SVG";

    // A page with ruled lines and a folded corner.
    case EyedropperType::Note:
        return R"SVG(<path stroke-width="1.2" d="M3.4 2.6H13L16.6 6.2V17.4H3.4z"/>
                  <path stroke-width="1.1" d="M13 2.6V6.2H16.6"/>
                  <path stroke-width="1" d="M6.2 9.4H13.8M6.2 12.2H13.8M6.2 15H11"/>)SVG";

    // "123", as CS6 draws the Count tool.
    case EyedropperType::Count:
        return R"SVG(<g fill="none" stroke="COLOR" stroke-width="1.35"
                  stroke-linecap="round" stroke-linejoin="round">
                  <path d="M2.2 8.4 3.9 6.9V13.6"/>
                  <path d="M7.3 8.2a1.9 1.9 0 1 1 3.4 1.2L7.2 13.6h3.7"/>
                  <path d="M13.9 7.2h3.6l-2.1 2.6a2.1 2.1 0 1 1-1.7 3.3"/></g>)SVG";

    case EyedropperType::Eyedropper:
        break;
    }
    return bodyFor(ToolId::Eyedropper);
}

/// Per-variant artwork for the healing group.
///
/// CS6 gives the two brushes the same bandage, distinguished by the spot on the
/// Spot Healing one, then a stitched patch, crossing arrows and an eye.
QString healingVariantBody(int variant)
{
    switch (static_cast<HealingType>(variant)) {
    // The plain bandage: this brush is told where to sample from, so it has no
    // spot on it.
    case HealingType::Healing:
        return R"SVG(<g transform="rotate(-45 10 10)">
                  <rect x="2.4" y="7.3" width="15.2" height="5.4" rx="2.7" fill="none"
                  stroke="COLOR" stroke-width="1.25"/>
                  <path d="M7.6 7.3V12.7M12.4 7.3V12.7"/></g>)SVG";

    // A patch with a stitched seam across it, as CS6 draws it.
    case HealingType::Patch:
        return R"SVG(<path stroke-width="1.2" d="M3.4 6.2 9.4 2.8 16.6 6.6 14.2 15.4 6.2 17.2z"/>
                  <path stroke-width="1" stroke-dasharray="1.9 1.6"
                  d="M4.6 11.2 15.4 10.6"/>)SVG";

    // Two crossing arrows: the region goes one way, what fills its place comes
    // from the other.
    case HealingType::ContentAwareMove:
        return R"SVG(<path stroke-width="1.25" d="M4.4 15.6 15.2 4.8M15.6 15.2 4.8 4.4"/>
                  <path fill="COLOR" stroke="none" d="M17.6 2.4 16.4 7.6 11.6 3.6z"/>
                  <path fill="COLOR" stroke="none" d="M17.6 17.6 12.4 16.4 16.4 11.6z"/>)SVG";

    // An eye with a crosshair through the pupil.
    case HealingType::RedEye:
        return R"SVG(<circle cx="10" cy="10" r="3.4" fill="none" stroke="COLOR"
                  stroke-width="1.3"/>
                  <path stroke-width="1.15" d="M10 2.4V6M10 14V17.6M2.4 10H6M14 10H17.6"/>)SVG";

    case HealingType::SpotHealing:
        break;
    }
    return bodyFor(ToolId::Healing);
}

/// Per-variant artwork for the brush group.
QString brushVariantBody(int variant)
{
    // A pencil: hexagonal barrel, sharpened tip, on the same diagonal CS6 draws
    // its brush on so the two read as a pair.
    if (static_cast<BrushType>(variant) == BrushType::Pencil) {
        return R"SVG(<g transform="rotate(45 10 10)">
                  <path stroke-width="1.2" d="M7.6 2.6H12.4V13.4L10 17.4 7.6 13.4z"/>
                  <path stroke-width="1" d="M7.6 13.4H12.4"/>
                  <path fill="COLOR" stroke="none" d="M8.7 15.3H11.3L10 17.4z"/>
                  <path stroke-width="1" d="M10 2.6V13.4"/></g>)SVG";
    }
    // Colour Replacement: the brush with the swap arrows CS6 puts beside it.
    if (static_cast<BrushType>(variant) == BrushType::ColorReplacement) {
        return R"SVG(<g transform="translate(-2.6 2.6) scale(0.82)">
                  <path fill="COLOR" stroke="none" d="M17.1 2.9c.9.9.9 2 0 2.9l-6.3 6.3-2.9-2.9
                  6.3-6.3c.9-.9 2-.9 2.9 0z"/>
                  <path d="M7.9 9.2 10.8 12.1c.3 2.4-1.2 4.2-3.2 4.9-1.6.5-4 .5-4 .5
                  1.1-1 1.7-1.7 2-3 .3-1.6.6-3.9 2.3-5.3z"/></g>
                  <path stroke-width="1.1" d="M12.6 3.4H17.4M17.4 3.4 15.8 1.8M17.4 3.4 15.8 5"/>
                  <path stroke-width="1.1" d="M17.4 7.6H12.6M12.6 7.6 14.2 6M12.6 7.6 14.2 9.2"/>)SVG";
    }
    return bodyFor(ToolId::Brush);
}

QString blurVariantBody(int variant)
{
    switch (static_cast<BlurTool>(variant)) {
    // Sharpen: CS6's cone, apex up, with the ruled face that reads as a ground
    // edge rather than a plain triangle.
    case BlurTool::Sharpen:
        return R"SVG(<path stroke-width="1.2" d="M10 2 15.4 17.2 4.6 17.2z"/>
                  <path stroke-width="1" d="M10 2V17.2"/>)SVG";

    // Smudge: the pointing hand CS6 draws — a fist with the index finger out,
    // aimed up-left at what it is about to drag.
    case BlurTool::Smudge:
        // Tilted, as CS6 tilts it: the finger points up and to the left, at what
        // the drag is about to pull.
        return R"SVG(<g transform="rotate(-16 10 10)">
                  <path stroke-width="1.2" d="M8.1 9.9 8.1 4.3a1.3 1.3 0 0 1 2.6 0v5.1"/>
                  <path stroke-width="1.2" d="M10.7 9.4V7.6a1.2 1.2 0 0 1 2.4 0v2.2"/>
                  <path stroke-width="1.2" d="M13.1 9.8V8.6a1.2 1.2 0 0 1 2.4 0v4.2
                  c0 3-2 5.2-5 5.2-2.6 0-4.1-1.3-5.2-3.3L3.6 12.4a1.3 1.3 0 0 1 2.1-1.5L8.1 13.4"/>
                  </g>)SVG";

    case BlurTool::Blur:
        break;
    }
    return bodyFor(ToolId::Blur);
}

QString dodgeVariantBody(int variant)
{
    switch (static_cast<ToneTool>(variant)) {
    // Burn: the cupped hand a printer holds over the paper to keep light off
    // everything but the gap between finger and thumb. CS6 draws the "O" that
    // gap makes, with the hand closed around it.
    case ToneTool::Burn:
        return R"SVG(<path stroke-width="1.2" d="M12.6 4.4c-1.6-1.1-3.8-1.1-5.4.2
                  C5.4 6.1 5.1 8.5 6.2 10.2"/>
                  <path stroke-width="1.2" d="M6.2 10.2 4.1 9.1a1.2 1.2 0 0 0-1.2 2.1l3.1 2
                  c.6 2.7 3 4.7 5.9 4.7a6 6 0 0 0 6-6c0-2.4-1.4-4.5-3.4-5.5"/>
                  <circle cx="11.9" cy="11.9" r="2.7" fill="none" stroke="COLOR"
                  stroke-width="1.2"/>)SVG";

    // Sponge: the rounded block with its pores, as CS6 draws it.
    case ToneTool::Sponge:
        return R"SVG(<path stroke-width="1.2" d="M4 8.2c0-2.4 2.7-4.2 6-4.2s6 1.8 6 4.2v4.6
                  c0 2.4-2.7 4.2-6 4.2s-6-1.8-6-4.2z"/>
                  <circle cx="7.6" cy="8.8" r="1.1" fill="COLOR" stroke="none"/>
                  <circle cx="12.4" cy="7.9" r="0.9" fill="COLOR" stroke="none"/>
                  <circle cx="10.2" cy="12.2" r="1.2" fill="COLOR" stroke="none"/>
                  <circle cx="13.6" cy="12.6" r="0.8" fill="COLOR" stroke="none"/>
                  <circle cx="6.8" cy="12.9" r="0.8" fill="COLOR" stroke="none"/>)SVG";

    case ToneTool::Dodge:
        break;
    }
    return bodyFor(ToolId::Dodge);
}

/// Per-variant artwork for the hand group.
///
/// CS6 draws Rotate View as a hand with a curved arrow turning about it. At
/// 20px the hand plus an arrow is mud, so this is the arrow alone — a circle
/// open at one end with a head on it, which is what the eye picks out anyway.
QString handVariantBody(int variant)
{
    if (static_cast<HandTool>(variant) != HandTool::RotateView) {
        return bodyFor(ToolId::Hand);
    }
    return R"SVG(<path d="M15.6 6.4a7 7 0 1 0 1.7 5" stroke-width="1.4"/>
                  <path fill="COLOR" stroke="none" d="M10.4 2.2 16.8 4.2 15 10.6z"/>)SVG";
}

/// Per-variant artwork for the shape group.
///
/// Each is the shape itself as a solid silhouette, which is how CS6 draws them
/// — the tool and its icon are the same thing here, so there is nothing to
/// badge.
QString shapeVariantBody(int variant)
{
    switch (static_cast<ShapeTool>(variant)) {
    case ShapeTool::RoundedRectangle:
        return R"SVG(<rect x="2.8" y="5.4" width="14.4" height="9.2" rx="2.6" fill="COLOR"
                  stroke="none"/>)SVG";

    case ShapeTool::Ellipse:
        return R"SVG(<ellipse cx="10" cy="10" rx="7.6" ry="5.6" fill="COLOR" stroke="none"/>)SVG";

    case ShapeTool::Polygon:
        // A pentagon, point up, matching the five sides the tool defaults to.
        return R"SVG(<path fill="COLOR" stroke="none" d="M10 2.2 17.4 7.6 14.6 16.4 5.4 16.4
                  2.6 7.6z"/>)SVG";

    case ShapeTool::Line:
        return R"SVG(<path d="M2.6 15.6 17.4 4.4" stroke-width="2.2"/>)SVG";

    case ShapeTool::CustomShape:
        // CS6 uses a shape from the library itself; the star reads at 20px and
        // is the first entry of ours.
        return R"SVG(<path fill="COLOR" stroke="none" d="M10 1.8 12.3 7.4 18.3 7.8 13.7 11.7
                  15.2 17.6 10 14.3 4.8 17.6 6.3 11.7 1.7 7.8 7.7 7.4z"/>)SVG";

    case ShapeTool::Rectangle:
        break;
    }
    return bodyFor(ToolId::Shape);
}

/// Per-variant artwork for the eraser group.
///
/// CS6 keeps the eraser block for all three and marks what each one erases *by*:
/// scissors-like shears on the Background Eraser, which cuts a subject away from
/// its background, and the wand's sparkle on the Magic Eraser, the tool being
/// the Magic Wand's flood erased rather than selected.
QString eraserVariantBody(int variant)
{
    const QString block = bodyFor(ToolId::Eraser);

    switch (static_cast<EraserType>(variant)) {
    case EraserType::BackgroundEraser:
        // The crosshair the tool aims with, on the corner it erases from — the
        // one thing a user has to understand about this tool.
        return block
            + R"SVG(<g stroke-width="1.1"><path d="M2 5.2H8.4M5.2 2V8.4"/>
                  <circle cx="5.2" cy="5.2" r="2.4" fill="none" stroke="COLOR"/></g>)SVG";

    case EraserType::MagicEraser:
        return block
            + R"SVG(<path fill="COLOR" stroke="none" d="M4.6 1.2 5.5 3.5 7.8 4.4 5.5 5.3
                  4.6 7.6 3.7 5.3 1.4 4.4 3.7 3.5z"/>)SVG";

    case EraserType::Eraser:
        break;
    }
    return block;
}

/// Per-variant artwork for the path selection group.
///
/// Photoshop's two arrows, and the pair is how everyone tells the tools apart:
/// the Path Selection tool is the **black arrow**, which takes a whole subpath,
/// and Direct Selection is the **white arrow**, which takes one anchor or
/// handle. Here "black" is a filled silhouette and "white" a hollow outline,
/// both in the strip's own colour — the strip is dark, so a literally black
/// arrow would be invisible and a literally white one would not match the rest.
///
/// Both are the same outline at the same size, so the two read as one tool with
/// two modes rather than two unrelated glyphs.
QString pathSelectVariantBody(int variant)
{
    if (static_cast<PathSelectTool>(variant) == PathSelectTool::DirectSelection) {
        // Mitred rather than the group's round join, so the point of the arrow
        // stays a point instead of being rounded off at this size.
        return QStringLiteral(R"SVG(<path fill="none" stroke="COLOR" stroke-width="1.2")SVG"
                              R"SVG( stroke-linejoin="miter" d="%1"/>)SVG")
            .arg(QLatin1String(kPathArrow));
    }
    return bodyFor(ToolId::PathSelect);
}

/// Per-variant artwork for the clone group.
///
/// CS6 keeps the rubber stamp for both and marks the Pattern Stamp's with a
/// checkered swatch on its base, saying it prints a pattern rather than
/// whatever was sampled. The stamp itself is reused so the pair still reads as
/// one button's worth of tools.
QString cloneVariantBody(int variant)
{
    if (static_cast<CloneType>(variant) != CloneType::PatternStamp) {
        return bodyFor(ToolId::CloneStamp);
    }

    // The stamp's own base line is dropped: the swatch takes that row, and two
    // horizontals a pixel apart turn to mud at this size.
    return R"SVG(<path d="M4.6 12.4H15.4L13.3 8.4H6.7z"/>
                  <path d="M8.3 8.4V4.6a1.7 1.7 0 0 1 3.4 0v3.8"/>
                  <g stroke="none" fill="COLOR">
                  <rect x="3.4" y="14.4" width="3.3" height="3.3"/>
                  <rect x="10" y="14.4" width="3.3" height="3.3"/></g>
                  <rect x="3.4" y="14.4" width="13.2" height="3.3" fill="none"
                  stroke="COLOR" stroke-width="1.1"/>)SVG";
}

/// Per-variant artwork for the gradient group.
///
/// The button's default is the gradient swatch; the Paint Bucket gets CS6's own
/// glyph, which the flyout was showing the swatch for.
///
/// That glyph is a bucket tipped past horizontal with its mouth to the lower
/// left, its handle hanging away from the body, and a drop of paint falling
/// from the low lip. It is drawn tilted outright rather than as an upright
/// bucket inside a `rotate()`, so every point can be placed against the 20×20
/// grid — at this size a few tenths decide whether the mouth reads as open.
///
/// The bucket's axis runs up-right at 45°: the mouth is centred at (8.3, 11.2)
/// and the base at (14.7, 4.8), with the four corners of the body falling
/// either side of those two points along the perpendicular.
QString gradientVariantBody(int variant)
{
    if (static_cast<GradientTool>(variant) != GradientTool::PaintBucket) {
        return bodyFor(ToolId::Gradient);
    }

    return R"SVG(<path d="M5.3 8.2 12.6 2.7Q15.6 4 16.8 6.9L11.3 14.2"/>
                  <ellipse cx="8.3" cy="11.2" rx="4.2" ry="1.5"
                  transform="rotate(45 8.3 11.2)"/>
                  <path d="M5.3 8.2Q4.8 14.7 11.3 14.2" stroke-width="1.1"/>
                  <path fill="COLOR" stroke="none" d="M10.6 13.6c1.05 1.6 1.55 2.5 1.55 3.2
                  a1.55 1.55 0 0 1-3.1 0c0-.7.5-1.6 1.55-3.2z"/>)SVG";
}

/// The Type button's four variants.
///
/// CS6 turns the same "T" a quarter turn for the vertical tool, and outlines it
/// for the two mask tools — the mask tools cut their letterforms out of a
/// selection rather than laying down ink, and the hollow glyph says so.
QString typeVariantBody(int variant)
{
    const QString upright = bodyFor(ToolId::Type);

    switch (static_cast<TypeTool>(variant)) {
    case TypeTool::Vertical:
        return QStringLiteral(R"SVG(<g transform="rotate(90 10 10)">%1</g>)SVG").arg(upright);

    case TypeTool::HorizontalMask:
    case TypeTool::VerticalMask: {
        const QString dashed =
            QStringLiteral(R"SVG(<rect x="1.6" y="1.6" width="16.8" height="16.8" fill="none")SVG"
                           R"SVG( stroke="COLOR" stroke-width="1" stroke-dasharray="2.2 1.6"/>)SVG");
        if (static_cast<TypeTool>(variant) == TypeTool::VerticalMask) {
            return dashed
                + QStringLiteral(R"SVG(<g transform="rotate(90 10 10)">%1</g>)SVG").arg(upright);
        }
        return dashed + upright;
    }

    case TypeTool::Horizontal:
        break;
    }
    return upright;
}

/// Per-variant artwork for the pen group.
///
/// The first four entries are the nib with a small mark added: none for Pen
/// itself, a scribble tail for Freeform, a +/- badge for the two anchor-point
/// tools. Convert Point drops the nib and draws CS6's corner-to-curve glyph
/// instead — a straight arm meeting a curved one at a shared anchor.
QString penVariantBody(int variant)
{
    const QString nib = bodyFor(ToolId::Pen);

    switch (static_cast<PenTool>(variant)) {
    case PenTool::FreeformPen:
        return nib + R"SVG(<path d="M1.8 15.4c1.6 0.4 2.1-1.2 1-1.9-1-0.7-0.6-2.1 1-1.7"
                  stroke-width="1.05"/>)SVG";

    case PenTool::AddAnchor:
        return nib + R"SVG(<g stroke-width="1.2"><path d="M15.4 1.8V5.8"/>
                  <path d="M13.4 3.8H17.4"/></g>)SVG";

    case PenTool::DeleteAnchor:
        return nib + R"SVG(<path d="M13.4 3.8H17.4" stroke-width="1.2"/>)SVG";

    case PenTool::ConvertPoint:
        return R"SVG(<path d="M4 16.4 9.6 10.8"/>
                  <path d="M9.6 10.8C12.2 8.2 12.8 5 16.6 4.2"/>
                  <circle cx="4" cy="16.4" r="1.2" fill="COLOR" stroke="none"/>
                  <circle cx="9.6" cy="10.8" r="1.3" fill="none" stroke="COLOR" stroke-width="1.15"/>
                  <circle cx="16.6" cy="4.2" r="1.2" fill="COLOR" stroke="none"/>)SVG";

    case PenTool::Pen:
        break;
    }
    return nib;
}

/// Per-variant artwork, where a tool's flyout entries need distinct icons.
///
/// Every group whose variants are implemented has these; the rest reuse their
/// default icon, since an unimplemented entry is drawn disabled anyway.
QString variantBodyFor(ToolId id, int variant)
{
    if (id == ToolId::Crop) {
        return cropVariantBody(variant);
    }
    if (id == ToolId::Lasso) {
        return lassoVariantBody(variant);
    }
    if (id == ToolId::QuickSelect) {
        return quickSelectVariantBody(variant);
    }
    if (id == ToolId::Eyedropper) {
        return eyedropperVariantBody(variant);
    }
    if (id == ToolId::Healing) {
        return healingVariantBody(variant);
    }
    if (id == ToolId::Brush) {
        return brushVariantBody(variant);
    }
    if (id == ToolId::Blur) {
        return blurVariantBody(variant);
    }
    if (id == ToolId::Dodge) {
        return dodgeVariantBody(variant);
    }
    if (id == ToolId::Hand) {
        return handVariantBody(variant);
    }
    if (id == ToolId::Shape) {
        return shapeVariantBody(variant);
    }
    if (id == ToolId::Eraser) {
        return eraserVariantBody(variant);
    }
    if (id == ToolId::PathSelect) {
        return pathSelectVariantBody(variant);
    }
    if (id == ToolId::CloneStamp) {
        return cloneVariantBody(variant);
    }
    if (id == ToolId::Gradient) {
        return gradientVariantBody(variant);
    }
    if (id == ToolId::Pen) {
        return penVariantBody(variant);
    }
    if (id == ToolId::Type) {
        return typeVariantBody(variant);
    }
    if (id != ToolId::Marquee) {
        return bodyFor(id);
    }
    switch (static_cast<MarqueeType>(variant)) {
    case MarqueeType::Elliptical:
        return R"SVG(<ellipse cx="10" cy="10" rx="7.6" ry="5.4" fill="none" stroke="COLOR"
                  stroke-width="1.2" stroke-dasharray="2.4 1.8"/>)SVG";
    case MarqueeType::SingleRow:
        return R"SVG(<path d="M1.8 8.4H18.2M1.8 11.6H18.2" stroke-width="1.15"
                  stroke-dasharray="2.4 1.8"/>)SVG";
    case MarqueeType::SingleColumn:
        return R"SVG(<path d="M8.4 1.8V18.2M11.6 1.8V18.2" stroke-width="1.15"
                  stroke-dasharray="2.4 1.8"/>)SVG";
    case MarqueeType::Rectangular:
        break;
    }
    return bodyFor(id);
}

/// Wrap body markup in an SVG document with the colour substituted.
QString document(const QString &body, const QColor &color)
{
    QString svg = QStringLiteral(
                      R"SVG(<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 %1 %1">)SVG"
                      R"SVG(<g %2>%3</g></svg>)SVG")
                      .arg(kViewBox)
                      .arg(QLatin1String(kStrokeAttrs), body);
    // Do the substitution last so it also reaches attributes inside the body.
    return svg.replace(QLatin1String("COLOR"), color.name());
}

QPixmap renderPixmap(const QString &svg, int size)
{
    QSvgRenderer renderer(svg.toUtf8());
    if (!renderer.isValid()) {
        return {};
    }

    const qreal ratio = qGuiApp ? qGuiApp->devicePixelRatio() : 1.0;
    QPixmap pixmap(QSize(size, size) * ratio);
    pixmap.fill(Qt::transparent);
    pixmap.setDevicePixelRatio(ratio);

    QPainter painter(&pixmap);
    painter.setRenderHint(QPainter::Antialiasing, true);
    renderer.render(&painter, QRectF(0, 0, size, size));
    painter.end();

    return pixmap;
}

QIcon renderDocument(const QString &svg)
{
    QSvgRenderer renderer(svg.toUtf8());
    if (!renderer.isValid()) {
        return {};
    }

    // Render at the device ratio so the strokes stay sharp on retina displays,
    // then tag the pixmap so Qt lays it out at the logical size.
    const qreal ratio = qGuiApp ? qGuiApp->devicePixelRatio() : 1.0;
    QPixmap pixmap(QSize(kIconSize, kIconSize) * ratio);
    pixmap.fill(Qt::transparent);
    pixmap.setDevicePixelRatio(ratio);

    QPainter painter(&pixmap);
    painter.setRenderHint(QPainter::Antialiasing, true);
    renderer.render(&painter, QRectF(0, 0, kIconSize, kIconSize));
    painter.end();

    return QIcon(pixmap);
}

} // namespace

QIcon ToolIcons::icon(ToolId id, int variant, const QColor &color)
{
    // Keyed on colour and ratio too: the strip re-requests icons whenever it
    // rebuilds, and rasterising twenty SVGs on every pass is wasteful.
    static QHash<QString, QIcon> cache;
    const qreal ratio = qGuiApp ? qGuiApp->devicePixelRatio() : 1.0;
    const QString key = QStringLiteral("%1|%2|%3|%4")
                            .arg(int(id))
                            .arg(variant)
                            .arg(color.name(), QString::number(ratio));

    const auto it = cache.constFind(key);
    if (it != cache.constEnd()) {
        return it.value();
    }

    const QIcon result = renderDocument(document(variantBodyFor(id, variant), color));
    cache.insert(key, result);
    return result;
}

QIcon ToolIcons::icon(ToolId id, const QColor &color)
{
    return icon(id, 0, color);
}

QIcon ToolIcons::fromSvgBody(const QString &svgBody, const QColor &color)
{
    return renderDocument(document(svgBody, color));
}

QPixmap ToolIcons::pixmapFromSvgBody(const QString &svgBody, const QColor &color, int size)
{
    return renderPixmap(document(svgBody, color), qMax(1, size));
}

QString ToolIcons::quickMaskSvg(bool active)
{
    // Standard mode shows a hollow marquee; Quick Mask fills the surround, the
    // same way CS6 flips the two states of this button.
    if (active) {
        return QStringLiteral(
            R"SVG(<rect x="1.6" y="4.4" width="16.8" height="11.2" fill="COLOR" stroke="none"/>)SVG"
            R"SVG(<circle cx="10" cy="10" r="3.4" fill="#3c3c3c" stroke="none"/>)SVG");
    }
    return QStringLiteral(
        R"SVG(<rect x="1.6" y="4.4" width="16.8" height="11.2" fill="none" stroke="COLOR"
           stroke-width="1.25"/>)SVG"
        R"SVG(<circle cx="10" cy="10" r="3.4" fill="COLOR" stroke="none"/>)SVG");
}

QString ToolIcons::gradientTypeSvg(GradientType type)
{
    // Each is a 16px swatch showing the shape's own ramp. SVG gradients are
    // used rather than hand-stepped bands so they stay smooth at any size; the
    // stops are greyscale, and `COLOR` tints only the frame.
    const QString frame =
        QStringLiteral(R"SVG(<rect x="2" y="2" width="16" height="16" fill="none")SVG"
                       R"SVG( stroke="COLOR" stroke-width="1"/>)SVG");
    QString body;
    switch (type) {
    case GradientType::Linear:
        body = QStringLiteral(
            R"SVG(<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="0">)SVG"
            R"SVG(<stop offset="0" stop-color="#1a1a1a"/><stop offset="1" stop-color="#f0f0f0"/>)SVG"
            R"SVG(</linearGradient></defs>)SVG"
            R"SVG(<rect x="2" y="2" width="16" height="16" fill="url(#g)" stroke="none"/>)SVG");
        break;
    case GradientType::Radial:
        body = QStringLiteral(
            R"SVG(<defs><radialGradient id="g" cx="0.5" cy="0.5" r="0.5">)SVG"
            R"SVG(<stop offset="0" stop-color="#1a1a1a"/><stop offset="1" stop-color="#f0f0f0"/>)SVG"
            R"SVG(</radialGradient></defs>)SVG"
            R"SVG(<rect x="2" y="2" width="16" height="16" fill="url(#g)" stroke="none"/>)SVG");
        break;
    // A sweep has no SVG primitive, so it is four quadrant wedges stepping
    // through the ramp — enough to read as "rotates around a point" at 20px.
    case GradientType::Angle:
        body = QStringLiteral(
            R"SVG(<path d="M10 10 18 10 18 2z" fill="#f0f0f0" stroke="none"/>)SVG"
            R"SVG(<path d="M10 10 18 2 10 2z" fill="#b4b4b4" stroke="none"/>)SVG"
            R"SVG(<path d="M10 10 10 2 2 2z" fill="#787878" stroke="none"/>)SVG"
            R"SVG(<path d="M10 10 2 2 2 10z" fill="#3c3c3c" stroke="none"/>)SVG"
            R"SVG(<path d="M10 10 2 10 2 18z" fill="#1a1a1a" stroke="none"/>)SVG"
            R"SVG(<path d="M10 10 2 18 10 18z" fill="#3c3c3c" stroke="none"/>)SVG"
            R"SVG(<path d="M10 10 10 18 18 18z" fill="#787878" stroke="none"/>)SVG"
            R"SVG(<path d="M10 10 18 18 18 10z" fill="#b4b4b4" stroke="none"/>)SVG");
        break;
    case GradientType::Reflected:
        body = QStringLiteral(
            R"SVG(<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="0">)SVG"
            R"SVG(<stop offset="0" stop-color="#f0f0f0"/><stop offset="0.5" stop-color="#1a1a1a"/>)SVG"
            R"SVG(<stop offset="1" stop-color="#f0f0f0"/></linearGradient></defs>)SVG"
            R"SVG(<rect x="2" y="2" width="16" height="16" fill="url(#g)" stroke="none"/>)SVG");
        break;
    // Concentric diamonds, drawn largest first so the darker centre lands on top.
    case GradientType::Diamond:
        body = QStringLiteral(
            R"SVG(<rect x="2" y="2" width="16" height="16" fill="#f0f0f0" stroke="none"/>)SVG"
            R"SVG(<path d="M10 2 18 10 10 18 2 10z" fill="#b4b4b4" stroke="none"/>)SVG"
            R"SVG(<path d="M10 5 15 10 10 15 5 10z" fill="#646464" stroke="none"/>)SVG"
            R"SVG(<path d="M10 8 12 10 10 12 8 10z" fill="#1a1a1a" stroke="none"/>)SVG");
        break;
    }
    return body + frame;
}

QString ToolIcons::closeSvg()
{
    return QStringLiteral(
        R"SVG(<g fill="none" stroke="COLOR" stroke-width="1.6" stroke-linecap="round">
           <path d="M5 5 15 15"/><path d="M15 5 5 15"/></g>)SVG");
}

QString ToolIcons::columnToggleSvg(bool pointsLeft)
{
    // Two chevrons, thinner than the tool glyphs — this is chrome, not a tool.
    if (pointsLeft) {
        return QStringLiteral(
            R"SVG(<g fill="none" stroke="COLOR" stroke-width="1.6" stroke-linecap="round"
               stroke-linejoin="round"><path d="M10 5 5 10 10 15"/>
               <path d="M16 5 11 10 16 15"/></g>)SVG");
    }
    return QStringLiteral(
        R"SVG(<g fill="none" stroke="COLOR" stroke-width="1.6" stroke-linecap="round"
           stroke-linejoin="round"><path d="M10 5 15 10 10 15"/>
           <path d="M4 5 9 10 4 15"/></g>)SVG");
}

QString ToolIcons::crosshairSvg()
{
    // A plus with a short axis pair at the top left, the way CS6 marks the
    // cursor-position readout.
    return QStringLiteral(R"SVG(<path stroke-width="1.3" d="M10 3.6V16.4M3.6 10H16.4"/>
                  <path stroke-width="1.3" d="M2.2 6.4V2.2H6.4"/>)SVG");
}

QString ToolIcons::boundsSvg()
{
    // A dashed marquee with an arrow out of its lower right, for the
    // selection-size readout.
    return QStringLiteral(R"SVG(<rect x="2.6" y="2.6" width="10.4" height="10.4" fill="none"
                  stroke="COLOR" stroke-width="1.2" stroke-dasharray="2.2 1.7"/>
                  <path stroke-width="1.2" d="M11.6 11.6 17.4 17.4"/>
                  <path fill="COLOR" stroke="none" d="M17.8 17.8 13.4 17.2 17.2 13.4z"/>)SVG");
}

QString ToolIcons::protractorSvg()
{
    // A wedge with an arc across it — the angle mark CS6 puts on the ruler's
    // readout block.
    return QStringLiteral(R"SVG(<path stroke-width="1.3" d="M3 16.4 16.4 16.4 3 5.4z"/>
                  <path stroke-width="1" fill="none" d="M3 12.4a4 4 0 0 0 3.6 4"/>)SVG");
}

QString ToolIcons::textAlignSvg(Qt::Alignment align, bool vertical)
{
    // The vertical set is the horizontal one turned a quarter turn: ruled lines
    // running down the icon, flush to the top, centred, or flush to the bottom
    // — which is what the same three buttons mean for vertical type.
    if (vertical) {
        if (align & Qt::AlignHCenter) {
            return QStringLiteral(
                R"SVG(<path d="M5 3V17M10 5.5V14.5M15 4.2V15.8" stroke-width="1.6"/>)SVG");
        }
        if (align & Qt::AlignRight) {
            return QStringLiteral(
                R"SVG(<path d="M5 3V17M10 8V17M15 5V17" stroke-width="1.6"/>)SVG");
        }
        return QStringLiteral(R"SVG(<path d="M5 3V17M10 3V12M15 3V15" stroke-width="1.6"/>)SVG");
    }

    if (align & Qt::AlignHCenter) {
        return QStringLiteral(
            R"SVG(<path d="M3 5H17M5.5 10H14.5M4.2 15H15.8" stroke-width="1.6"/>)SVG");
    }
    if (align & Qt::AlignRight) {
        return QStringLiteral(R"SVG(<path d="M3 5H17M8 10H17M5 15H17" stroke-width="1.6"/>)SVG");
    }
    return QStringLiteral(R"SVG(<path d="M3 5H17M3 10H12M3 15H15" stroke-width="1.6"/>)SVG");
}

QString ToolIcons::commitSvg()
{
    return QStringLiteral(R"SVG(<path d="M4 10.5 8.2 15 16.5 5.5" stroke-width="1.8"/>)SVG");
}

QString ToolIcons::cancelSvg()
{
    return QStringLiteral(R"SVG(<circle cx="10" cy="10" r="7" stroke-width="1.5"/>
                  <path d="M5.2 14.8 14.8 5.2" stroke-width="1.5"/>)SVG");
}

QString ToolIcons::selectionModeSvg(SelectionMode mode)
{
    // Two squares on the 20×20 grid, overlapping in their corners:
    //   A = 3.5…12.5, B = 7.5…16.5, so the overlap is 7.5…12.5 on both axes.
    // Each icon outlines both squares and shades the region that mode keeps.
    // The regions are axis-aligned, so they are written out as explicit
    // polygons rather than leaning on SVG clipping, which Qt's renderer only
    // partly supports.
    const QString outlines = QStringLiteral(
        R"SVG(<rect x="3.5" y="3.5" width="9" height="9"/>)SVG"
        R"SVG(<rect x="7.5" y="7.5" width="9" height="9"/>)SVG");

    QString shaded;
    switch (mode) {
    case SelectionMode::New:
        // A single square, centred — nothing to combine with.
        return QStringLiteral(
            R"SVG(<rect x="5" y="5" width="10" height="10" fill="COLOR" fill-opacity="0.55"
               stroke="COLOR" stroke-width="1.2"/>)SVG");

    case SelectionMode::Add:
        // The union, as one L-shaped hexagon.
        shaded = QStringLiteral(
            R"SVG(<path d="M3.5 3.5H12.5V7.5H16.5V16.5H7.5V12.5H3.5Z"/>)SVG");
        break;

    case SelectionMode::Subtract:
        // A with the overlap bitten out.
        shaded = QStringLiteral(
            R"SVG(<path d="M3.5 3.5H12.5V7.5H7.5V12.5H3.5Z"/>)SVG");
        break;

    case SelectionMode::Intersect:
        // The overlap alone.
        shaded = QStringLiteral(
            R"SVG(<rect x="7.5" y="7.5" width="5" height="5"/>)SVG");
        break;
    }

    return QStringLiteral(R"SVG(<g fill="COLOR" fill-opacity="0.55" stroke="none">%1</g>)SVG")
               .arg(shaded)
        + QStringLiteral(R"SVG(<g fill="none" stroke="COLOR" stroke-width="1.2">%1</g>)SVG")
              .arg(outlines);
}

QString ToolIcons::screenModeSvg()
{
    return QStringLiteral(
        R"SVG(<rect x="1.8" y="3.6" width="16.4" height="12.8" fill="none" stroke="COLOR"
           stroke-width="1.25"/>)SVG"
        R"SVG(<rect x="1.8" y="3.6" width="16.4" height="3.2" fill="COLOR" stroke="none"/>)SVG");
}

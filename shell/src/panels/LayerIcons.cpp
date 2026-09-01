#include "LayerIcons.h"

#include <QObject>

#include "../tools/ToolIcons.h"

#include <QGuiApplication>
#include <QHash>

namespace {

/// SVG body for a glyph, on the 20×20 grid `ToolIcons` renders. `COLOR` is
/// substituted at render time; the enclosing group supplies a 1.25px stroke, so
/// anything that wants a solid silhouette says so itself.
QString bodyFor(LayerIcons::Glyph glyph)
{
    using Glyph = LayerIcons::Glyph;
    switch (glyph) {
    // The almond and pupil CS6 draws. Two mirrored arcs rather than an ellipse:
    // the eye is wider than it is tall and pointed at both ends.
    case Glyph::Eye:
        return R"SVG(<path d="M1.6 10c2.3-3.4 5-5.1 8.4-5.1s6.1 1.7 8.4 5.1c-2.3 3.4-5 5.1-8.4 5.1
                  S3.9 13.4 1.6 10z"/>
                  <circle cx="10" cy="10" r="2.5" fill="COLOR" stroke="none"/>)SVG";

    // The shackle is an arc, the body a rounded box. Solid means Lock All.
    case Glyph::LockSolid:
        return R"SVG(<path d="M6.6 8.6V6.4a3.4 3.4 0 0 1 6.8 0v2.2"/>
                  <rect x="4.4" y="8.6" width="11.2" height="8.4" rx="1.3" fill="COLOR"
                  stroke="none"/>)SVG";

    case Glyph::LockOutline:
        return R"SVG(<path d="M6.6 8.6V6.4a3.4 3.4 0 0 1 6.8 0v2.2"/>
                  <rect x="4.4" y="8.6" width="11.2" height="8.4" rx="1.3" fill="none"
                  stroke="COLOR" stroke-width="1.25"/>)SVG";

    // Photoshop's transparency checkerboard: a 2×2 of squares, two filled.
    case Glyph::LockTransparency:
        return R"SVG(<rect x="3.4" y="3.4" width="13.2" height="13.2" fill="none" stroke="COLOR"
                  stroke-width="1.1"/>
                  <rect x="3.4" y="3.4" width="6.6" height="6.6" fill="COLOR" stroke="none"/>
                  <rect x="10" y="10" width="6.6" height="6.6" fill="COLOR" stroke="none"/>)SVG";

    // A brush, angled as the tool strip's is but simplified — at 16px the
    // ferrule and bristle detail turns to mud.
    case Glyph::LockImage:
        return R"SVG(<path fill="COLOR" stroke="none" d="M16.6 3.4c.8.8.8 1.9 0 2.7l-5.9 5.9-2.7-2.7
                  5.9-5.9c.8-.8 1.9-.8 2.7 0z"/>
                  <path d="M7.4 9.1 10.1 11.8c.3 2.2-1.1 3.9-3 4.5-1.5.5-3.7.5-3.7.5
                  1-.9 1.6-1.6 1.9-2.8.3-1.5.6-3.6 2.1-4.9z"/>)SVG";

    case Glyph::LockPosition:
        return R"SVG(<path fill="COLOR" stroke="none" d="M10 2.2 12.4 5 11 5 11 9 15 9 15 7.6
                  17.8 10 15 12.4 15 11 11 11 11 15 12.4 15 10 17.8 7.6 15 9 15 9 11 5 11 5 12.4
                  2.2 10 5 7.6 5 9 9 9 9 5 7.6 5z"/>)SVG";

    case Glyph::LockAll:
        return bodyFor(Glyph::LockSolid);

    case Glyph::Search:
        return R"SVG(<circle cx="8.6" cy="8.6" r="5" fill="none" stroke="COLOR"
                  stroke-width="1.4"/>
                  <path d="M12.4 12.4 17.2 17.2" stroke-width="1.6"/>)SVG";

    // A photograph: the frame with a hill and a sun, which is how CS6 says
    // "pixel layer" in this row.
    case Glyph::KindPixel:
        return R"SVG(<rect x="2.6" y="4.4" width="14.8" height="11.2" fill="none" stroke="COLOR"
                  stroke-width="1.2"/>
                  <path d="M2.6 13.2 7 8.8l3.4 3.4 2.6-2.6 4.4 4.4"/>
                  <circle cx="13.4" cy="7.6" r="1.5" fill="COLOR" stroke="none"/>)SVG";

    // The half-filled circle CS6 uses for every adjustment layer.
    case Glyph::KindAdjustment:
    case Glyph::Adjustment:
        return R"SVG(<circle cx="10" cy="10" r="7" fill="none" stroke="COLOR"
                  stroke-width="1.3"/>
                  <path fill="COLOR" stroke="none" d="M10 3a7 7 0 0 1 0 14z"/>)SVG";

    case Glyph::KindType:
        return R"SVG(<path d="M4.4 4.6H15.6M10 4.6V15.4M7.4 15.4H12.6" stroke-width="1.5"/>)SVG";

    // A shape with its path anchors showing, as CS6 marks shape layers.
    case Glyph::KindShape:
        return R"SVG(<rect x="5" y="5" width="10" height="10" rx="1.4" fill="none" stroke="COLOR"
                  stroke-width="1.2"/>
                  <rect x="3.2" y="3.2" width="3.6" height="3.6" fill="COLOR" stroke="none"/>
                  <rect x="13.2" y="13.2" width="3.6" height="3.6" fill="COLOR"
                  stroke="none"/>)SVG";

    // A framed layer with the corner badge Photoshop puts on smart objects.
    case Glyph::KindSmartObject:
        return R"SVG(<rect x="2.6" y="4.4" width="14.8" height="11.2" fill="none" stroke="COLOR"
                  stroke-width="1.2"/>
                  <path d="M2.6 12.6 6.8 8.4l4.2 4.2"/>
                  <path fill="COLOR" stroke="none" d="M11.6 10.4H17.4V16.2H11.6z"/>
                  <path d="M12.6 13.3h3.8M14.5 11.4v3.8" stroke="#000000"/>)SVG";

    // The switch at the right of the filter row: a rounded track with the knob
    // over to one side. The panel tints it red when filtering is on, which is
    // how CS6 shows the state.
    case Glyph::FilterSwitch:
        return R"SVG(<rect x="1.6" y="6.2" width="16.8" height="7.6" rx="3.8" fill="none"
                  stroke="COLOR" stroke-width="1.3"/>
                  <circle cx="13.6" cy="10" r="2.6" fill="COLOR" stroke="none"/>)SVG";

    // Two interlocking links, drawn as CS6's chain: rounded, at a slight angle.
    case Glyph::Link:
        return R"SVG(<path d="M8.4 6.6H6.2a3.4 3.4 0 0 0 0 6.8h2.2"/>
                  <path d="M11.6 6.6h2.2a3.4 3.4 0 0 1 0 6.8h-2.2"/>
                  <path d="M6.8 10H13.2" stroke-width="1.4"/>)SVG";

    // CS6's "fx", drawn as strokes so it needs no font on the box.
    case Glyph::Effects:
        return R"SVG(<path d="M9.4 6.2c0-1.8.9-2.8 2.4-2.8"/>
                  <path d="M7.6 8.4h3.6"/>
                  <path d="M9.4 6.2V16.4"/>
                  <path d="M12.6 10.6 17 16.4M17 10.6 12.6 16.4"/>)SVG";

    // A layer mask: the frame with the ellipse that stands for the masked area.
    case Glyph::Mask:
        return R"SVG(<rect x="2.6" y="4.6" width="14.8" height="10.8" fill="none" stroke="COLOR"
                  stroke-width="1.3"/>
                  <ellipse cx="10" cy="10" rx="3.6" ry="3.2" fill="COLOR" stroke="none"/>)SVG";

    // A folder with its tab, the way CS6 draws a layer group.
    case Glyph::Group:
        return R"SVG(<path d="M2.6 15.4V5.4h5l1.6 2h8.2v8z" fill="none" stroke="COLOR"
                  stroke-width="1.3"/>)SVG";

    // A sheet with the corner turned up — CS6's new-layer glyph.
    case Glyph::NewLayer:
        return R"SVG(<path d="M4.4 3.4H12.4L15.6 6.6V16.6H4.4z" fill="none" stroke="COLOR"
                  stroke-width="1.3"/>
                  <path d="M12.4 3.4V6.6H15.6"/>)SVG";

    // The bin: lid, handle, and the three ribs CS6 rules down its front.
    case Glyph::Delete:
        return R"SVG(<path d="M3.6 5.8H16.4"/>
                  <path d="M8 5.8V4.2h4v1.6"/>
                  <path d="M5.4 5.8 6.4 16.8h7.2L14.6 5.8"/>
                  <path d="M8.2 8.4V14.2M10 8.4V14.2M11.8 8.4V14.2"/>)SVG";

    // The clipping badge: a square for the layer below, and the elbow arrow
    // CS6 points down into it.
    case Glyph::ClipToLayer:
        return R"SVG(<rect x="7.2" y="9.6" width="9.2" height="7.2" fill="none" stroke="COLOR"
                  stroke-width="1.3"/>
                  <path d="M11.8 3.6V7.2"/>
                  <path fill="COLOR" stroke="none" d="M11.8 9.8 9.4 6.2h4.8z"/>)SVG";

    // Three quarters of a circle with an arrowhead on the open end.
    case Glyph::Reset:
        return R"SVG(<path d="M15.4 7.6a6.4 6.4 0 1 0 1 3.4" fill="none" stroke="COLOR"
                  stroke-width="1.4"/>
                  <path fill="COLOR" stroke="none" d="M16.8 3.6 16.2 8.8 11.4 7.4z"/>)SVG";
    }
    return {};
}

} // namespace

QPixmap LayerIcons::pixmap(Glyph glyph, const QColor &color, int size)
{
    // The panel repaints every row on every refresh, so rasterising the same
    // handful of glyphs each time would be pure waste.
    static QHash<QString, QPixmap> cache;
    const qreal ratio = qGuiApp ? qGuiApp->devicePixelRatio() : 1.0;
    const QString key = QStringLiteral("%1|%2|%3|%4")
                            .arg(int(glyph))
                            .arg(size)
                            .arg(color.name(), QString::number(ratio));

    const auto it = cache.constFind(key);
    if (it != cache.constEnd()) {
        return it.value();
    }

    const QPixmap result = ToolIcons::pixmapFromSvgBody(bodyFor(glyph), color, size);
    cache.insert(key, result);
    return result;
}

QIcon LayerIcons::icon(Glyph glyph, const QColor &color, int size)
{
    return QIcon(pixmap(glyph, color, size));
}

QStringList LayerIcons::labelNames()
{
    return {QObject::tr("None"),   QObject::tr("Red"),    QObject::tr("Orange"),
            QObject::tr("Yellow"), QObject::tr("Green"),  QObject::tr("Blue"),
            QObject::tr("Violet"), QObject::tr("Gray")};
}

QColor LayerIcons::labelColor(int label)
{
    // Photoshop's own seven, sampled from its panel.
    static const QColor colors[] = {
        QColor(),                  // None
        QColor(0xb5, 0x53, 0x4f), // Red
        QColor(0xc0, 0x7d, 0x3e), // Orange
        QColor(0xb5, 0xa8, 0x3e), // Yellow
        QColor(0x5b, 0x8f, 0x54), // Green
        QColor(0x4c, 0x6b, 0xa8), // Blue
        QColor(0x7a, 0x5c, 0xa8), // Violet
        QColor(0x77, 0x77, 0x77), // Gray
    };
    const int count = int(sizeof(colors) / sizeof(colors[0]));
    return label > 0 && label < count ? colors[label] : QColor();
}

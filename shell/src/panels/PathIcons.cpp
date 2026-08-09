#include "PathIcons.h"

#include "../tools/ToolIcons.h"

#include <QGuiApplication>
#include <QHash>

namespace {

/// SVG body for a glyph, on the 20×20 grid `ToolIcons` renders.
QString bodyFor(PathIcons::Glyph glyph)
{
    using Glyph = PathIcons::Glyph;
    switch (glyph) {
    // A curve through three anchors — two hollow squares at the ends, a solid
    // one at the smooth point in the middle, with its direction line — reading
    // at a glance as "a path" rather than any one particular shape.
    case Glyph::PathThumbnail:
        return R"SVG(<path d="M3 15 Q6 5 10 10 T17 5"/>
                  <rect x="1.6" y="13.6" width="2.8" height="2.8" fill="none" stroke="COLOR"
                  stroke-width="1"/>
                  <rect x="15.6" y="3.6" width="2.8" height="2.8" fill="none" stroke="COLOR"
                  stroke-width="1"/>
                  <path d="M6.5 6.5 13.5 13.5" stroke-width="0.9"/>
                  <rect x="8.6" y="8.6" width="2.8" height="2.8" fill="COLOR" stroke="none"/>)SVG";

    // A paint bucket tipped over the path's own curve, the way CS6 pairs the
    // fill glyph with a curved swatch beneath it.
    case Glyph::Fill:
        return R"SVG(<path d="M9 2.6 15.4 9 9.9 14.5 3.5 8.1z" fill="none" stroke="COLOR"
                  stroke-width="1.2"/>
                  <path d="M3.5 8.1 9.9 14.5" stroke-width="1.2"/>
                  <path d="M2.4 17.2c0-4 15.2-4 15.2 0z" fill="COLOR" stroke="none"/>)SVG";

    // A brush laid along a curved dashed line — stroking a path rather than
    // painting freehand.
    case Glyph::Stroke:
        return R"SVG(<path d="M2.6 15.4Q7 6 17.4 8.6" stroke-dasharray="2.2 1.8"/>
                  <path fill="COLOR" stroke="none" d="M16.4 2.9c.7.7.7 1.7 0 2.4l-4.8 4.8-2.4-2.4
                  4.8-4.8c.7-.7 1.7-.7 2.4 0z"/>
                  <path d="M9.2 7.3 11.3 9.4c.2 1.8-.9 3.2-2.4 3.7-1.2.4-3 .4-3 .4
                  .8-.7 1.3-1.3 1.5-2.2.2-1.2.5-2.9 1.8-4z"/>)SVG";

    // The marching-ants dashed rectangle CS6 uses for "load as selection".
    case Glyph::LoadSelection:
        return R"SVG(<rect x="3" y="5" width="14" height="10" fill="none" stroke="COLOR"
                  stroke-width="1.2" stroke-dasharray="2 1.6"/>)SVG";

    // The reverse of Load Selection: a dashed rectangle turning into a solid
    // curve, for "make a path from the selection".
    case Glyph::MakeWorkPath:
        return R"SVG(<rect x="2.6" y="6" width="8.4" height="8.4" fill="none" stroke="COLOR"
                  stroke-width="1.1" stroke-dasharray="1.8 1.5"/>
                  <path d="M11 14 16 4" stroke-width="1.2"/>
                  <circle cx="16" cy="4" r="1.4" fill="COLOR" stroke="none"/>)SVG";

    // A sheet with the corner turned up — the same glyph the Layers panel
    // uses for a new layer, on the same idea of "a new blank entry".
    case Glyph::NewPath:
        return R"SVG(<path d="M4.4 3.4H12.4L15.6 6.6V16.6H4.4z" fill="none" stroke="COLOR"
                  stroke-width="1.3"/>
                  <path d="M12.4 3.4V6.6H15.6"/>)SVG";

    // The bin, matching the Layers panel's exactly — the same action on a
    // different kind of row.
    case Glyph::Delete:
        return R"SVG(<path d="M3.6 5.8H16.4"/>
                  <path d="M8 5.8V4.2h4v1.6"/>
                  <path d="M5.4 5.8 6.4 16.8h7.2L14.6 5.8"/>
                  <path d="M8.2 8.4V14.2M10 8.4V14.2M11.8 8.4V14.2"/>)SVG";
    }
    return {};
}

} // namespace

QPixmap PathIcons::pixmap(Glyph glyph, const QColor &color, int size)
{
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

QIcon PathIcons::icon(Glyph glyph, const QColor &color, int size)
{
    return QIcon(pixmap(glyph, color, size));
}

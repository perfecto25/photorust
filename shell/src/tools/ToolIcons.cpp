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

    // Spot Healing Brush: the bandage, angled as CS6 draws it.
    case ToolId::Healing:
        return R"SVG(<g transform="rotate(-45 10 10)">
                  <rect x="2.4" y="7.3" width="15.2" height="5.4" rx="2.7" fill="none"
                  stroke="COLOR" stroke-width="1.25"/>
                  <path d="M7.6 7.3V12.7M12.4 7.3V12.7"/></g>)SVG";

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

    case ToolId::Dodge:
        return R"SVG(<circle cx="7.9" cy="7.9" r="4.5" fill="none" stroke="COLOR" stroke-width="1.25"/>
                  <path d="M11.3 11.3 16.8 16.8" stroke-width="2.4"/>)SVG";

    case ToolId::Pen:
        return R"SVG(<path d="M10 1.8 15.1 9.1 10 18.2 4.9 9.1z"/>
                  <path d="M10 11.4V18.2"/>
                  <circle cx="10" cy="9.1" r="1.5" fill="none" stroke="COLOR" stroke-width="1.25"/>)SVG";

    case ToolId::Type:
        return R"SVG(<path d="M3.8 3.6H16.2M10 3.6V16.4M7.2 16.4H12.8"/>)SVG";

    case ToolId::PathSelect:
        return R"SVG(<path fill="COLOR" stroke="none" d="M5 1.8 5 16.2 8.5 12.7 10.9 17.8 13.4 16.6
                  11 11.6 15.8 11.6z"/>)SVG";

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

/// Per-variant artwork, where a tool's flyout entries need distinct icons.
///
/// Only the marquee group has these: CS6 draws a dashed rectangle, ellipse,
/// horizontal band and vertical band. Everything else reuses its default icon,
/// since the other groups' variants are not implemented anyway.
QString variantBodyFor(ToolId id, int variant)
{
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

QString ToolIcons::screenModeSvg()
{
    return QStringLiteral(
        R"SVG(<rect x="1.8" y="3.6" width="16.4" height="12.8" fill="none" stroke="COLOR"
           stroke-width="1.25"/>)SVG"
        R"SVG(<rect x="1.8" y="3.6" width="16.4" height="3.2" fill="COLOR" stroke="none"/>)SVG");
}

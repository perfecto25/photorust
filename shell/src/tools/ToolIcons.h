#pragma once

#include <QColor>
#include <QIcon>
#include <QString>

#include "ToolId.h"

/// Tool strip artwork.
///
/// CS6's icons are proprietary bitmaps we cannot ship, so these are line-art
/// reconstructions in the same visual language: a 20×20 grid, single-weight
/// strokes, monochrome, no fills except where CS6 uses a solid silhouette
/// (Move, Path Selection, Rectangle).
///
/// They are authored as SVG rather than `QPainterPath` calls so they stay
/// legible as shapes and render crisply at any device pixel ratio — the strip
/// is drawn at 20px on a 1x display and 40px on a retina one.
namespace ToolIcons {

/// The icon for a tool variant, tinted `color`.
///
/// `variant` indexes the tool's `subTools()` list. Only the Marquee group has
/// per-variant artwork; every other tool falls back to its default icon.
///
/// Results are cached per (tool, variant, colour, ratio), since the strip
/// rebuilds its tooltips and repaints far more often than artwork changes.
QIcon icon(ToolId id, int variant, const QColor &color);

/// The default (variant 0) icon for a tool.
QIcon icon(ToolId id, const QColor &color);

/// Render arbitrary SVG body markup, for the strip's footer controls.
/// Occurrences of the literal `COLOR` in `svgBody` are replaced with `color`.
QIcon fromSvgBody(const QString &svgBody, const QColor &color);

/// SVG body for the Quick Mask toggle at the foot of the strip.
QString quickMaskSvg(bool active);

/// SVG body for the screen-mode cycle button.
QString screenModeSvg();

} // namespace ToolIcons

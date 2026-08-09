#pragma once

#include <QColor>
#include <QIcon>
#include <QPixmap>
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

/// The same, at an explicit logical size and as a pixmap — the Layers panel
/// draws its own rows, so it needs the artwork rather than an icon, and it
/// needs it at several sizes (an eye is smaller than a footer button).
QPixmap pixmapFromSvgBody(const QString &svgBody, const QColor &color, int size);

/// SVG body for the Quick Mask toggle at the foot of the strip.
QString quickMaskSvg(bool active);

/// SVG body for the screen-mode cycle button.
QString screenModeSvg();

/// SVG body for the close cross on a panel header.
QString closeSvg();

/// SVG body for the double-chevron at the top of the tool strip, which
/// switches it between one and two columns. Points right to expand, left to
/// collapse — the same way CS6's does.
QString columnToggleSvg(bool pointsLeft);

/// SVG body for one of the options-bar selection combine buttons.
///
/// CS6 draws these as two overlapping squares with the resulting region
/// shaded, so the four read as a set at a glance.
QString selectionModeSvg(SelectionMode mode);

/// SVG body for one of the Gradient tool's five type buttons.
///
/// CS6 draws each as a small square filled with that shape's own ramp, so the
/// five read as a set — a horizontal fade, concentric rings, a sweep, a mirrored
/// fade and a diamond.
QString gradientTypeSvg(GradientType type);

/// SVG body for the Info panel's cursor-position glyph: CS6's small crosshair
/// with an origin corner.
QString crosshairSvg();

/// SVG body for the Info panel's selection-size glyph: a dashed rectangle with
/// a corner arrow.
QString boundsSvg();

/// SVG body for the Info panel's angle glyph: the protractor CS6 shows beside
/// the ruler's A and L readouts.
QString protractorSvg();

} // namespace ToolIcons

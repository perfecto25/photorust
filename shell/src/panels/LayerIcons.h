#pragma once

#include <QColor>
#include <QIcon>
#include <QPixmap>

/// Layers panel artwork.
///
/// The same line-art reconstruction as the tool strip (see `ToolIcons`, which
/// owns the renderer): a 20×20 grid, single-weight strokes, monochrome. CS6's
/// own bitmaps are proprietary, so these are drawn to match their silhouettes —
/// the eye, the padlock, the four Lock buttons, and the seven glyphs along the
/// panel's foot.
///
/// Sizes vary here in a way they do not in the strip: the eye in a layer row is
/// 15px, a lock badge 12px, a footer button 20px. So the API hands back pixmaps
/// at a requested size, and the panel — which paints its own rows — blits them.
namespace LayerIcons {

enum class Glyph {
    /// Visibility, in the row's left column. CS6 draws the eye only when the
    /// layer is visible and leaves the column empty when it is not.
    Eye,
    /// The padlock on a row. Solid for Lock All, outlined for a partial lock,
    /// which is the distinction CS6 draws.
    LockSolid,
    LockOutline,

    // -- the Lock row, in CS6's order --
    /// Lock Transparent Pixels: the checkerboard.
    LockTransparency,
    /// Lock Image Pixels: a brush. This is the lock that makes a layer
    /// untouchable by the tools.
    LockImage,
    /// Lock Position: the four-way move arrow.
    LockPosition,
    /// Lock All: the padlock again, as CS6 ends the row.
    LockAll,

    // -- the filter row at the top --
    /// The magnifier beside the Kind menu.
    Search,
    /// The filter kinds, in CS6's order after Kind.
    KindPixel,
    KindAdjustment,
    KindType,
    KindShape,
    KindSmartObject,
    /// The switch at the far right that turns filtering on and off.
    FilterSwitch,

    // -- the footer, in CS6's order --
    Link,
    Effects,
    Mask,
    Adjustment,
    Group,
    NewLayer,
    Delete,
};

/// Artwork for `glyph`, tinted `color`, at `size` logical pixels square.
/// Cached per (glyph, colour, size, device ratio).
QPixmap pixmap(Glyph glyph, const QColor &color, int size);

/// The same as an icon, for the buttons that take one.
QIcon icon(Glyph glyph, const QColor &color, int size);

} // namespace LayerIcons

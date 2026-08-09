#pragma once

#include <QColor>
#include <QIcon>
#include <QPixmap>

/// Paths panel artwork.
///
/// The same line-art reconstruction as the tool strip and the Layers panel
/// (see `ToolIcons`, which owns the renderer, and `LayerIcons` for the sibling
/// panel this one is modelled on): a 20×20 grid, single-weight strokes,
/// monochrome.
namespace PathIcons {

enum class Glyph {
    /// A generic path thumbnail: a curve through a couple of anchor points.
    /// Every row uses the same glyph rather than a live preview of its own
    /// shape — the same simplification the Layers panel makes for an
    /// adjustment layer's thumbnail.
    PathThumbnail,

    // -- the footer, in CS6's order --
    /// Fill the path with the foreground colour.
    Fill,
    /// Stroke the path with the current brush.
    Stroke,
    /// Load the path as a selection.
    LoadSelection,
    /// Make a work path from the current selection. Not implemented — see
    /// PathsPanel.cpp.
    MakeWorkPath,
    /// A new, empty path.
    NewPath,
    Delete,
};

/// Artwork for `glyph`, tinted `color`, at `size` logical pixels square.
/// Cached per (glyph, colour, size, device ratio).
QPixmap pixmap(Glyph glyph, const QColor &color, int size);

/// The same as an icon, for the buttons that take one.
QIcon icon(Glyph glyph, const QColor &color, int size);

} // namespace PathIcons

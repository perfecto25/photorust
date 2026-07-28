#pragma once

#include <QString>

/// The tools in the left-hand strip.
///
/// The order matches CS6's tool strip top to bottom, including the separators
/// between functional groups. Each entry maps to a `tool.*` command id in
/// `shortcuts.json` — the single-letter shortcuts (V, M, L, …) live there, not
/// here (CLAUDE.md §9).
enum class ToolId {
    Move,
    Marquee,
    Lasso,
    QuickSelect,
    Crop,
    Eyedropper,
    Healing,
    Brush,
    CloneStamp,
    HistoryBrush,
    Eraser,
    Gradient,
    Blur,
    Dodge,
    Pen,
    Type,
    PathSelect,
    Shape,
    Hand,
    Zoom,

    Count
};

/// Metadata for one tool strip entry.
struct ToolInfo {
    ToolId id;
    /// Command id in the registry, e.g. "tool.brush".
    const char *commandId;
    /// Display name shown in the tooltip and options bar.
    const char *name;
    /// Single character drawn as a stand-in icon until real artwork lands.
    const char *glyph;
    /// True when CS6 draws a separator line *above* this tool.
    bool groupBreak;
};

/// The tool strip, in order.
///
/// `glyph` is a placeholder: CS6's tool icons are bitmaps we do not have, so
/// each button currently renders a letter. Swapping in real icons means
/// changing only this table and `ToolStrip::createButton`.
inline const ToolInfo *toolTable()
{
    static const ToolInfo table[] = {
        {ToolId::Move,         "tool.move",         "Move Tool",                  "✥", false},
        {ToolId::Marquee,      "tool.marquee",      "Rectangular Marquee Tool",   "⬚", true},
        {ToolId::Lasso,        "tool.lasso",        "Lasso Tool",                 "⌒", false},
        {ToolId::QuickSelect,  "tool.quickselect",  "Quick Selection Tool",       "✦", false},
        {ToolId::Crop,         "tool.crop",         "Crop Tool",                  "⌗", false},
        {ToolId::Eyedropper,   "tool.eyedropper",   "Eyedropper Tool",            "⚗", false},
        {ToolId::Healing,      "tool.healing",      "Spot Healing Brush Tool",    "⊕", true},
        {ToolId::Brush,        "tool.brush",        "Brush Tool",                 "✎", false},
        {ToolId::CloneStamp,   "tool.clonestamp",   "Clone Stamp Tool",           "⛃", false},
        {ToolId::HistoryBrush, "tool.historybrush", "History Brush Tool",         "↺", false},
        {ToolId::Eraser,       "tool.eraser",       "Eraser Tool",                "▧", false},
        {ToolId::Gradient,     "tool.gradient",     "Gradient Tool",              "◨", false},
        {ToolId::Blur,         "tool.blur",         "Blur Tool",                  "◌", false},
        {ToolId::Dodge,        "tool.dodge",        "Dodge Tool",                 "◑", false},
        {ToolId::Pen,          "tool.pen",          "Pen Tool",                   "✒", true},
        {ToolId::Type,         "tool.type",         "Horizontal Type Tool",       "T",      false},
        {ToolId::PathSelect,   "tool.pathselect",   "Path Selection Tool",        "➤", false},
        {ToolId::Shape,        "tool.shape",        "Rectangle Tool",             "▭", false},
        {ToolId::Hand,         "tool.hand",         "Hand Tool",                  "✋", true},
        {ToolId::Zoom,         "tool.zoom",         "Zoom Tool",                  "⌕", false},
    };
    static_assert(sizeof(table) / sizeof(table[0]) == static_cast<int>(ToolId::Count),
                  "toolTable() must have one entry per ToolId");
    return table;
}

/// Metadata for a tool, or nullptr if `id` is out of range.
inline const ToolInfo *toolInfo(ToolId id)
{
    const int index = static_cast<int>(id);
    if (index < 0 || index >= static_cast<int>(ToolId::Count)) {
        return nullptr;
    }
    return &toolTable()[index];
}

/// Display name for a tool, for the options bar and status tips.
inline QString toolName(ToolId id)
{
    const ToolInfo *info = toolInfo(id);
    return info ? QString::fromUtf8(info->name) : QString();
}

/// True when the tool draws onto the canvas and needs a stroke.
inline bool toolPaints(ToolId id)
{
    return id == ToolId::Brush || id == ToolId::Eraser || id == ToolId::Healing
        || id == ToolId::CloneStamp || id == ToolId::HistoryBrush;
}

/// True when the tool defines a selection by dragging.
inline bool toolSelects(ToolId id)
{
    return id == ToolId::Marquee || id == ToolId::Lasso || id == ToolId::QuickSelect;
}

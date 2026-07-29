#pragma once

#include <QString>
#include <QStringList>

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
    /// True when CS6 draws a separator line *above* this tool.
    bool groupBreak;
};

/// The tool strip, in order. Artwork lives in ToolIcons.
inline const ToolInfo *toolTable()
{
    static const ToolInfo table[] = {
        {ToolId::Move,         "tool.move",         "Move Tool",                false},
        {ToolId::Marquee,      "tool.marquee",      "Rectangular Marquee Tool", true},
        {ToolId::Lasso,        "tool.lasso",        "Lasso Tool",               false},
        {ToolId::QuickSelect,  "tool.quickselect",  "Quick Selection Tool",     false},
        {ToolId::Crop,         "tool.crop",         "Crop Tool",                false},
        {ToolId::Eyedropper,   "tool.eyedropper",   "Eyedropper Tool",          false},
        {ToolId::Healing,      "tool.healing",      "Spot Healing Brush Tool",  true},
        {ToolId::Brush,        "tool.brush",        "Brush Tool",               false},
        {ToolId::CloneStamp,   "tool.clonestamp",   "Clone Stamp Tool",         false},
        {ToolId::HistoryBrush, "tool.historybrush", "History Brush Tool",       false},
        {ToolId::Eraser,       "tool.eraser",       "Eraser Tool",              false},
        {ToolId::Gradient,     "tool.gradient",     "Gradient Tool",            false},
        {ToolId::Blur,         "tool.blur",         "Blur Tool",                false},
        {ToolId::Dodge,        "tool.dodge",        "Dodge Tool",               false},
        {ToolId::Pen,          "tool.pen",          "Pen Tool",                 true},
        {ToolId::Type,         "tool.type",         "Horizontal Type Tool",     false},
        {ToolId::PathSelect,   "tool.pathselect",   "Path Selection Tool",      false},
        {ToolId::Shape,        "tool.shape",        "Rectangle Tool",           false},
        {ToolId::Hand,         "tool.hand",         "Hand Tool",                true},
        {ToolId::Zoom,         "tool.zoom",         "Zoom Tool",                false},
    };
    static_assert(sizeof(table) / sizeof(table[0]) == static_cast<int>(ToolId::Count),
                  "toolTable() must have one entry per ToolId");
    return table;
}

/// The marquee variants, in flyout order. These index the Marquee tool's
/// sub-tool list, and the canvas dispatches on them directly.
enum class MarqueeType {
    Rectangular = 0,
    Elliptical = 1,
    SingleRow = 2,
    SingleColumn = 3,
};

/// One entry in a tool's flyout.
struct SubTool {
    /// Display name, e.g. "Elliptical Marquee Tool".
    const char *name;
    /// The letter CS6 shows at the right of the row, or nullptr for none.
    /// Entries sharing a letter are what Shift+letter cycles between.
    const char *shortcut;
    /// Whether the engine actually implements this variant. Unimplemented
    /// entries are listed so the flyout keeps CS6's shape, but shown disabled
    /// rather than quietly falling back to the parent tool.
    bool implemented;
};

/// The hidden tools behind a strip button, as CS6 groups them.
///
/// The first entry is the tool's default variant. A group of one means the
/// button has no flyout and no corner triangle.
inline QList<SubTool> subTools(ToolId id)
{
    switch (id) {
    // The only fully implemented group. CS6 shows M on the rectangular and
    // elliptical rows only; the single-row and single-column variants are not
    // part of the Shift+M cycle.
    case ToolId::Marquee:
        return {{"Rectangular Marquee Tool", "M", true},
                {"Elliptical Marquee Tool", "M", true},
                {"Single Row Marquee Tool", nullptr, true},
                {"Single Column Marquee Tool", nullptr, true}};
    case ToolId::Lasso:
        return {{"Lasso Tool", "L", true},
                {"Polygonal Lasso Tool", nullptr, false},
                {"Magnetic Lasso Tool", nullptr, false}};
    case ToolId::QuickSelect:
        return {{"Quick Selection Tool", "W", true},
                {"Magic Wand Tool", nullptr, false}};
    case ToolId::Crop:
        return {{"Crop Tool", "C", true},
                {"Perspective Crop Tool", nullptr, false},
                {"Slice Tool", nullptr, false},
                {"Slice Select Tool", nullptr, false}};
    case ToolId::Eyedropper:
        return {{"Eyedropper Tool", "I", true},
                {"Color Sampler Tool", nullptr, false},
                {"Ruler Tool", nullptr, false},
                {"Note Tool", nullptr, false},
                {"Count Tool", nullptr, false}};
    case ToolId::Healing:
        return {{"Spot Healing Brush Tool", "J", true},
                {"Healing Brush Tool", nullptr, false},
                {"Patch Tool", nullptr, false},
                {"Content-Aware Move Tool", nullptr, false},
                {"Red Eye Tool", nullptr, false}};
    case ToolId::Brush:
        return {{"Brush Tool", "B", true},
                {"Pencil Tool", nullptr, false},
                {"Color Replacement Tool", nullptr, false},
                {"Mixer Brush Tool", nullptr, false}};
    case ToolId::CloneStamp:
        return {{"Clone Stamp Tool", "S", true},
                {"Pattern Stamp Tool", nullptr, false}};
    case ToolId::HistoryBrush:
        return {{"History Brush Tool", "Y", true},
                {"Art History Brush Tool", nullptr, false}};
    case ToolId::Eraser:
        return {{"Eraser Tool", "E", true},
                {"Background Eraser Tool", nullptr, false},
                {"Magic Eraser Tool", nullptr, false}};
    case ToolId::Gradient:
        return {{"Gradient Tool", "G", true},
                {"Paint Bucket Tool", nullptr, false}};
    case ToolId::Blur:
        return {{"Blur Tool", nullptr, true},
                {"Sharpen Tool", nullptr, false},
                {"Smudge Tool", nullptr, false}};
    case ToolId::Dodge:
        return {{"Dodge Tool", "O", true},
                {"Burn Tool", nullptr, false},
                {"Sponge Tool", nullptr, false}};
    case ToolId::Pen:
        return {{"Pen Tool", "P", true},
                {"Freeform Pen Tool", nullptr, false},
                {"Add Anchor Point Tool", nullptr, false},
                {"Delete Anchor Point Tool", nullptr, false},
                {"Convert Point Tool", nullptr, false}};
    case ToolId::Type:
        return {{"Horizontal Type Tool", "T", true},
                {"Vertical Type Tool", nullptr, false},
                {"Horizontal Type Mask Tool", nullptr, false},
                {"Vertical Type Mask Tool", nullptr, false}};
    case ToolId::PathSelect:
        return {{"Path Selection Tool", "A", true},
                {"Direct Selection Tool", nullptr, false}};
    case ToolId::Shape:
        return {{"Rectangle Tool", "U", true},
                {"Rounded Rectangle Tool", nullptr, false},
                {"Ellipse Tool", nullptr, false},
                {"Polygon Tool", nullptr, false},
                {"Line Tool", nullptr, false},
                {"Custom Shape Tool", nullptr, false}};
    case ToolId::Hand:
        return {{"Hand Tool", "H", true},
                {"Rotate View Tool", nullptr, false}};
    // Move and Zoom stand alone in CS6 — no corner triangle.
    case ToolId::Move:
    case ToolId::Zoom:
    case ToolId::Count:
        break;
    }
    return {};
}

/// Whether the button draws the small corner triangle.
inline bool hasFlyout(ToolId id)
{
    return subTools(id).size() > 1;
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

/// Display name for a tool, taking the active variant into account.
inline QString toolVariantName(ToolId id, int variant)
{
    const QList<SubTool> subs = subTools(id);
    if (variant >= 0 && variant < subs.size()) {
        return QString::fromUtf8(subs.at(variant).name);
    }
    const ToolInfo *info = toolInfo(id);
    return info ? QString::fromUtf8(info->name) : QString();
}

/// The variant Shift+letter should move to, cycling within the entries that
/// share the current variant's shortcut letter. Returns `variant` unchanged
/// when the tool has no cycle.
inline int nextVariantInCycle(ToolId id, int variant)
{
    const QList<SubTool> subs = subTools(id);
    if (variant < 0 || variant >= subs.size() || !subs.at(variant).shortcut) {
        return variant;
    }
    const QString letter = QString::fromUtf8(subs.at(variant).shortcut);

    // Walk forward, wrapping, to the next entry with the same letter.
    for (int step = 1; step <= subs.size(); ++step) {
        const int candidate = (variant + step) % subs.size();
        const SubTool &sub = subs.at(candidate);
        if (sub.shortcut && QString::fromUtf8(sub.shortcut) == letter && sub.implemented) {
            return candidate;
        }
    }
    return variant;
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

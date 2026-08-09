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

/// The lasso variants, in flyout order.
///
/// The three differ only in how the outline is *entered*: freehand drags it,
/// polygonal clicks corner to corner, magnetic clicks and then follows the
/// nearest edge. All three end up as the same closed polygon in the engine.
enum class LassoType {
    Freehand = 0,
    Polygonal = 1,
    Magnetic = 2,
};

/// CS6's defaults for the Magnetic Lasso's options bar.
namespace MagneticDefaults {
/// How far either side of the cursor an edge is looked for, in pixels.
constexpr int kWidth = 10;
/// How strong a gradient has to be to count as an edge, 1–100.
constexpr int kContrast = 10;
/// How often a fastening point is dropped automatically, 0–100. Higher
/// anchors the path down faster.
constexpr int kFrequency = 57;
} // namespace MagneticDefaults

/// The variants behind the Eyedropper button.
///
/// After the eyedropper itself these are all *annotation* tools: they read or
/// mark up the image without editing a pixel. The marks are document data,
/// held by the engine alongside slices (see core/src/annotation.rs).
enum class EyedropperType {
    Eyedropper = 0,
    ColorSampler = 1,
    Ruler = 2,
    Note = 3,
    Count = 4,
};

/// Marker kinds, mirroring `annotation::MarkerKind` across the bridge.
enum class MarkerKind {
    ColorSampler = 0,
    Note = 1,
    Count = 2,
};

/// The variants behind the Crop button.
///
/// The first two differ in what the user marks out: an axis-aligned rectangle,
/// or a free quadrilateral that gets straightened into one. The slice tools
/// are for web export and are not implemented.
enum class CropType {
    Rectangular = 0,
    Perspective = 1,
    Slice = 2,
    SliceSelect = 3,
};

/// The two tools behind the Quick Selection button.
///
/// Both select on colour; the difference is how much the user has to say.
/// The brush is dragged and grows a region that stops at edges; the wand is
/// clicked once and floods on colour similarity alone.
enum class QuickSelectType {
    Brush = 0,
    MagicWand = 1,
};

/// CS6's defaults for the Quick Selection and Magic Wand options bars.
namespace WandDefaults {
/// Quick Selection brush diameter, in pixels.
constexpr int kBrushSize = 30;
/// Magic Wand tolerance, 0–255 per channel.
constexpr int kTolerance = 32;
/// Both checkboxes are on by default in CS6.
constexpr bool kAntialias = true;
constexpr bool kContiguous = true;
} // namespace WandDefaults

/// How a new selection combines with the existing one.
///
/// The values are the `op` codes `Engine::selectRect`/`selectEllipse` take, so
/// this enum crosses the bridge as-is (see core/src/selection.rs SelectionOp).
enum class SelectionMode {
    New = 0,
    Add = 1,
    Subtract = 2,
    Intersect = 3,
};

/// Label for the options-bar button and its tooltip.
inline QString selectionModeName(SelectionMode mode)
{
    switch (mode) {
    case SelectionMode::New:       return QStringLiteral("New selection");
    case SelectionMode::Add:       return QStringLiteral("Add to selection");
    case SelectionMode::Subtract:  return QStringLiteral("Subtract from selection");
    case SelectionMode::Intersect: return QStringLiteral("Intersect with selection");
    }
    return {};
}

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
    // CS6 puts L on all three, so Shift+L cycles the whole group.
    case ToolId::Lasso:
        return {{"Lasso Tool", "L", true},
                {"Polygonal Lasso Tool", "L", true},
                {"Magnetic Lasso Tool", "L", true}};
    case ToolId::QuickSelect:
        return {{"Quick Selection Tool", "W", true},
                {"Magic Wand Tool", "W", true}};
    case ToolId::Crop:
        return {{"Crop Tool", "C", true},
                {"Perspective Crop Tool", "C", true},
                {"Slice Tool", "C", true},
                {"Slice Select Tool", "C", true}};
    case ToolId::Eyedropper:
        return {{"Eyedropper Tool", "I", true},
                {"Color Sampler Tool", "I", true},
                {"Ruler Tool", "I", true},
                {"Note Tool", "I", true},
                {"Count Tool", "I", true}};
    case ToolId::Healing:
        return {{"Spot Healing Brush Tool", "J", true},
                {"Healing Brush Tool", "J", true},
                {"Patch Tool", "J", true},
                {"Content-Aware Move Tool", "J", true},
                {"Red Eye Tool", "J", true}};
    case ToolId::Brush:
        return {{"Brush Tool", "B", true},
                {"Pencil Tool", "B", true},
                {"Color Replacement Tool", "B", true},
                {"Mixer Brush Tool", "B", true}};
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
                {"Paint Bucket Tool", "G", true}};
    case ToolId::Blur:
        return {{"Blur Tool", nullptr, true},
                {"Sharpen Tool", nullptr, true},
                {"Smudge Tool", nullptr, true}};
    case ToolId::Dodge:
        return {{"Dodge Tool", "O", true},
                {"Burn Tool", "O", true},
                {"Sponge Tool", "O", true}};
    case ToolId::Pen:
        return {{"Pen Tool", "P", true},
                {"Freeform Pen Tool", "P", true},
                {"Add Anchor Point Tool", nullptr, true},
                {"Delete Anchor Point Tool", nullptr, true},
                {"Convert Point Tool", nullptr, true}};
    case ToolId::Type:
        return {{"Horizontal Type Tool", "T", true},
                {"Vertical Type Tool", nullptr, false},
                {"Horizontal Type Mask Tool", nullptr, false},
                {"Vertical Type Mask Tool", nullptr, false}};
    case ToolId::PathSelect:
        return {{"Path Selection Tool", "A", true},
                {"Direct Selection Tool", "A", true}};
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
    // "Paints" here means "is stroked with a brush tip", not "lays down the
    // foreground colour": the Blur tool softens what it passes over and the
    // healing brushes rebuild it, but all of them want the tip picker and the
    // same dab machinery.
    return id == ToolId::Brush || id == ToolId::Eraser || id == ToolId::Healing
        || id == ToolId::CloneStamp || id == ToolId::HistoryBrush
        || id == ToolId::Blur || id == ToolId::Dodge;
}

/// True when the tool strokes a region and then *rebuilds* it from the
/// surroundings instead of applying a colour.
///
/// It still goes through the same stroke machinery as a brush — the difference
/// is what happens when the stroke ends (see core/src/healing.rs).
inline bool toolHeals(ToolId id)
{
    return id == ToolId::Healing;
}

/// The variants behind the healing button.
///
/// They divide into three kinds of gesture: the two brushes are stroked, the
/// Patch and Content-Aware Move tools work on a region the user drags, and the
/// Red Eye tool is dragged over an eye. What they share is that none of them
/// paints a colour — every one reconstructs pixels from other pixels.
enum class HealingType {
    SpotHealing = 0,
    Healing = 1,
    Patch = 2,
    ContentAwareMove = 3,
    RedEye = 4,
};

/// True when the variant is stroked with the brush rather than dragged as a
/// region.
inline bool healingIsBrush(HealingType type)
{
    return type == HealingType::SpotHealing || type == HealingType::Healing;
}

/// CS6's defaults for the Content-Aware Move tool's two adaptation sliders.
namespace CamDefaults {
/// How strictly the fill follows the edges it finds, 1-7.
constexpr int kStructure = 4;
/// How far the moved pixels adapt to their new surroundings, 0-10.
constexpr int kColor = 0;
} // namespace CamDefaults

/// CS6's defaults for the Red Eye tool's options bar.
namespace RedEyeDefaults {
constexpr int kPupilSize = 50;
constexpr int kDarkenAmount = 50;
} // namespace RedEyeDefaults

/// The variants behind the Brush button.
///
/// The Pencil differs from the Brush in exactly one way that matters: it paints
/// **aliased**, whole pixels only. That is what makes it the tool for touching up
/// single-pixel lines, and why hardness has no effect on it.
enum class BrushType {
    Brush = 0,
    Pencil = 1,
    ColorReplacement = 2,
    MixerBrush = 3,
};

/// True when the variant paints aliased, whole pixels only.
inline bool brushIsPencil(BrushType type)
{
    return type == BrushType::Pencil;
}

/// True when the variant recolours what is already there rather than painting
/// over it, and so needs its own stroke path.
inline bool brushReplacesColor(BrushType type)
{
    return type == BrushType::ColorReplacement;
}

/// The variants behind the Gradient button.
enum class GradientTool {
    Gradient = 0,
    PaintBucket = 1,
};

/// The five gradient shapes, mirroring `gradient::GradientType`. The order is
/// CS6's options-bar order, and the buttons are drawn in it.
enum class GradientType {
    Linear = 0,
    Radial = 1,
    Angle = 2,
    Reflected = 3,
    Diamond = 4,
};

/// Name for a gradient type's tooltip.
inline QString gradientTypeName(GradientType type)
{
    switch (type) {
    case GradientType::Linear:    return QStringLiteral("Linear Gradient");
    case GradientType::Radial:    return QStringLiteral("Radial Gradient");
    case GradientType::Angle:     return QStringLiteral("Angle Gradient");
    case GradientType::Reflected: return QStringLiteral("Reflected Gradient");
    case GradientType::Diamond:   return QStringLiteral("Diamond Gradient");
    }
    return {};
}

/// The variants behind the Dodge button — Photoshop's *toning* tools.
enum class ToneTool {
    Dodge = 0,
    Burn = 1,
    Sponge = 2,
};

/// Label for one of them.
inline QString toneToolName(ToneTool tool)
{
    switch (tool) {
    case ToneTool::Dodge:  return QStringLiteral("Dodge Tool");
    case ToneTool::Burn:   return QStringLiteral("Burn Tool");
    case ToneTool::Sponge: return QStringLiteral("Sponge Tool");
    }
    return {};
}

/// Dodge and Burn's tonal Range, mirroring `tone::ToneRange`.
enum class ToneRange { Shadows = 0, Midtones = 1, Highlights = 2 };
/// The Sponge's Mode, mirroring `tone::SpongeMode`.
enum class SpongeMode { Desaturate = 0, Saturate = 1 };

/// CS6's defaults for the toning tools' options bar.
namespace ToneDefaults {
constexpr ToneRange kRange = ToneRange::Midtones;
constexpr SpongeMode kSpongeMode = SpongeMode::Desaturate;
/// Exposure on Dodge and Burn, Flow on the Sponge — the same number.
constexpr int kAmount = 50;
constexpr bool kProtectTones = true;
constexpr bool kVibrance = true;
} // namespace ToneDefaults

/// The variants behind the Blur button.
enum class BlurTool {
    Blur = 0,
    Sharpen = 1,
    Smudge = 2,
};

/// CS6's defaults for the options bar the three share.
namespace BlurDefaults {
constexpr int kStrength = 50;
constexpr bool kSampleAllLayers = false;
/// Sharpen's own, ticked in CS6.
constexpr bool kProtectDetail = true;
/// Smudge's own.
constexpr bool kFingerPainting = false;
} // namespace BlurDefaults

/// Label for a variant behind the Blur button.
inline QString blurToolName(BlurTool tool)
{
    switch (tool) {
    case BlurTool::Blur:    return QStringLiteral("Blur Tool");
    case BlurTool::Sharpen: return QStringLiteral("Sharpen Tool");
    case BlurTool::Smudge:  return QStringLiteral("Smudge Tool");
    }
    return {};
}

/// The blend modes CS6 offers all three of the Blur button's tools. The full list makes no sense for a
/// tool whose source *is* its destination, softened — Multiply against a blurred
/// copy of yourself is not a thing anyone wants — so CS6 cuts it to these, and
/// the values are `BlendMode` discriminants.
inline QList<QPair<QString, int>> blurModes()
{
    return {{QStringLiteral("Normal"), 0},
            {QStringLiteral("Darken"), 2},
            {QStringLiteral("Lighten"), 7},
            {QStringLiteral("Hue"), 23},
            {QStringLiteral("Saturation"), 24},
            {QStringLiteral("Color"), 25},
            {QStringLiteral("Luminosity"), 26}};
}

/// What the Paint Bucket fills with, mirroring `bucket::BucketFill`.
enum class BucketFill { Foreground = 0, Pattern = 1 };

/// CS6's defaults for the Paint Bucket's options bar. Tolerance and the two
/// checkboxes match the Magic Wand's, because the two tools share the flood.
namespace BucketDefaults {
constexpr BucketFill kFill = BucketFill::Foreground;
constexpr int kOpacity = 100;
constexpr int kTolerance = 32;
constexpr bool kAntialias = true;
constexpr bool kContiguous = true;
constexpr bool kAllLayers = false;
} // namespace BucketDefaults

/// CS6's defaults for the Gradient tool's options bar. Dither is on, as it is in
/// Photoshop — an 8-bit ramp across a wide canvas bands visibly without it.
namespace GradientDefaults {
constexpr GradientType kType = GradientType::Linear;
constexpr int kOpacity = 100;
constexpr bool kReverse = false;
constexpr bool kDither = true;
constexpr bool kTransparency = true;
} // namespace GradientDefaults

/// The variants behind the Clone Stamp button.
enum class CloneType {
    CloneStamp = 0,
    PatternStamp = 1,
};

/// Where the Clone Stamp reads from, mirroring `stamp::CloneSampling`.
enum class CloneSampling { CurrentLayer = 0, CurrentAndBelow = 1, AllLayers = 2 };

/// CS6's defaults for the Clone Stamp's options bar.
namespace CloneDefaults {
constexpr bool kAligned = true;
constexpr CloneSampling kSampling = CloneSampling::CurrentLayer;
} // namespace CloneDefaults

/// True when the variant blends the brush's own paint with what is already on
/// the layer — the Mixer Brush, which also needs its own stroke path.
inline bool brushMixesColor(BrushType type)
{
    return type == BrushType::MixerBrush;
}

/// CS6's Mixer Brush presets: the Wet/Load/Mix combinations its first menu
/// offers. Flow is not part of a preset — it stays where the user left it.
///
/// The values are UI data, so they live here rather than in the engine, which
/// only ever sees the four numbers the bar sends it.
struct MixerPreset {
    /// Menu label. "Custom" is what the menu shows once a slider is moved by
    /// hand, and carries no values of its own.
    const char *name;
    /// Percentages, or -1 each for Custom.
    int wet;
    int load;
    int mix;
};

inline const QList<MixerPreset> &mixerPresets()
{
    static const QList<MixerPreset> presets = {
        {"Custom", -1, -1, -1},
        {"Dry", 0, 50, 0},
        {"Dry, Light Load", 0, 1, 0},
        {"Moist, Light Mix", 20, 50, 5},
        {"Moist, Heavy Mix", 20, 50, 60},
        {"Wet, Light Mix", 50, 50, 5},
        {"Wet, Heavy Mix", 50, 50, 60},
        {"Very Wet, Light Mix", 80, 80, 5},
        {"Very Wet, Heavy Mix", 80, 80, 60},
    };
    return presets;
}

/// CS6's defaults for the Mixer Brush's options bar — the "Dry" preset, with
/// both between-strokes toggles off and sampling limited to the active layer.
namespace MixerDefaults {
constexpr int kWet = 0;
constexpr int kLoad = 50;
constexpr int kMix = 0;
constexpr int kFlow = 100;
constexpr bool kSampleAllLayers = false;
constexpr bool kLoadAfterStroke = false;
constexpr bool kCleanAfterStroke = false;
} // namespace MixerDefaults

/// The Color Replacement Brush's Mode, mirroring `replace::ReplaceMode`.
enum class ReplaceMode { Hue = 0, Saturation = 1, Color = 2, Luminosity = 3 };
/// Its Sampling, mirroring `replace::ReplaceSampling`.
enum class ReplaceSampling { Continuous = 0, Once = 1, BackgroundSwatch = 2 };
/// Its Limits, mirroring `replace::ReplaceLimits`.
enum class ReplaceLimits { Discontiguous = 0, Contiguous = 1, FindEdges = 2 };

/// CS6's defaults for the Color Replacement Brush.
namespace ReplaceDefaults {
constexpr ReplaceMode kMode = ReplaceMode::Color;
constexpr ReplaceSampling kSampling = ReplaceSampling::Continuous;
constexpr ReplaceLimits kLimits = ReplaceLimits::Contiguous;
/// CS6 shows Tolerance as a percentage; the engine wants 0-255.
constexpr int kTolerancePercent = 30;
constexpr bool kAntialias = true;
} // namespace ReplaceDefaults

/// The Spot Healing Brush's Type, mirroring `healing::HealMode` across the
/// bridge. CS6 defaults to Content-Aware.
enum class HealType {
    ProximityMatch = 0,
    CreateTexture = 1,
    ContentAware = 2,
};

/// Label for a Type button.
inline QString healTypeName(HealType type)
{
    switch (type) {
    case HealType::ProximityMatch: return QStringLiteral("Proximity Match");
    case HealType::CreateTexture:  return QStringLiteral("Create Texture");
    case HealType::ContentAware:   return QStringLiteral("Content-Aware");
    }
    return {};
}

/// True when the tool defines a selection by dragging.
inline bool toolSelects(ToolId id)
{
    return id == ToolId::Marquee || id == ToolId::Lasso || id == ToolId::QuickSelect;
}

/// The variants behind the Pen button.
enum class PenTool {
    Pen = 0,
    FreeformPen = 1,
    AddAnchor = 2,
    DeleteAnchor = 3,
    ConvertPoint = 4,
};

/// The variants behind the Path Selection button.
enum class PathSelectTool {
    PathSelection = 0,
    DirectSelection = 1,
};

/// CS6's defaults for the Pen tool's options bar.
namespace PenDefaults {
/// Hovering the finished part of the active path adds an anchor over a
/// segment or removes one under the cursor, without switching tools.
constexpr bool kAutoAddDelete = true;
/// Preview the segment about to be drawn from the last anchor to the cursor,
/// before it is placed.
constexpr bool kRubberBand = true;
/// How coarsely a Freeform Pen drag is simplified into corner anchors, in
/// document pixels — CS6's "Curve Fit", 0.5-10px; this sits near its low end,
/// close enough that the traced shape reads as deliberate.
constexpr double kFreeformTolerance = 2.5;
} // namespace PenDefaults

/// True for the tools that edit a vector path rather than pixels: the whole
/// Pen group and both Path Selection variants. They share one gesture path on
/// the canvas, dispatching on the variant the way the retouch tools do.
inline bool toolEditsPaths(ToolId id)
{
    return id == ToolId::Pen || id == ToolId::PathSelect;
}

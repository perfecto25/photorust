# PhotoRust reference

Everything the interface currently offers: the window layout, the tool strip
and its flyouts, the panels, and the complete keymap.

This describes **what is implemented**, not what CS6 does. Where the two differ
the entry says so, so this doubles as the gap list. See [../README.md](../README.md)
for build instructions and [../CLAUDE.md](../CLAUDE.md) for architecture.

**Contents**

1. [Modifier keys](#1-modifier-keys)
2. [Window layout](#2-window-layout)
3. [The tool strip](#3-the-tool-strip)
4. [Tool options bar](#4-tool-options-bar)
5. [Panels](#5-panels)
6. [Menus](#6-menus)
7. [Keyboard shortcuts](#7-keyboard-shortcuts)
8. [Mouse and canvas gestures](#8-mouse-and-canvas-gestures)
9. [Remapping shortcuts](#9-remapping-shortcuts)

---

## 1. Modifier keys

Shortcut tables below give both platforms. The mapping is Photoshop's own:

| Linux | macOS | Notes |
|---|---|---|
| `Ctrl` | `Cmd` (⌘) | The primary modifier. |
| `Alt` | `Opt` (⌥) | The secondary modifier. |
| `Shift` | `Shift` (⇧) | Same on both. |

Nothing platform-specific is written into the keymap. Bindings are stored once,
in Qt's portable notation, and `Ctrl` resolves to Command on macOS
automatically — which is exactly the mapping Photoshop uses, so one table
serves both targets.

> The CS6 shortcut sheet this keymap was checked against is written in Mac
> notation throughout (`Cmd`, `Opt`). Read its `Cmd` as `Ctrl` on Linux and its
> `Opt` as `Alt`. A few of its entries are macOS window-management shortcuts
> with no Linux equivalent (Hide Photoshop, Minimize); those are marked below.

---

## 2. Window layout

```
┌──────────────────────────────────────────────────────────────────────┐
│ File  Edit  Image  Layer  Select  Filter  View  Window  Help         │  menu bar
├──────────────────────────────────────────────────────────────────────┤
│ Brush Tool │ Size: 20 px │ Hardness: 100% │ Opacity: 100% │ Flow: … │  options bar
├────┬────────────────────────────────────────────────┬────────────────┤
│ ✛  │                                                │  Color │Swatch│
│ ⬚  │                                                │ ┌────────────┐ │
│ ⌒  │                                                │ │            │ │
│ ✎  │                 canvas viewport                │ └────────────┘ │
│ ⌗  │            (checkerboard behind                ├────────────────┤
│ ✋ │             transparent pixels)                 │  History       │
│ 🔍 │                                                │                │
│    │                                                ├────────────────┤
│ ■  │                                                │  Layers        │
│ ⬚  │                                                │                │
│ ⛶  │                                                │                │
├────┴────────────────────────────────────────────────┴────────────────┤
│ 100%  │  1000 × 800  │                        X: 412   Y: 260        │  status bar
└──────────────────────────────────────────────────────────────────────┘
```

| Region | Contents |
|---|---|
| **Menu bar** | Eight menus; see [§6](#6-menus). |
| **Options bar** | The active tool's name in bold, then its settings. Rebuilt on every tool change. |
| **Document tabs** | One tab per open document, above the canvas. Click to switch, × to close. Untitled documents are numbered as CS6's are. The last document cannot be closed — the rest of the interface assumes there is one to act on. |
| **Tool strip** | Single column of 20 tools in four groups, then the colour swatch, Quick Mask and screen-mode buttons. |
| **Canvas viewport** | The document, centred, on the CS6 grey surround. Transparent pixels show a fixed-size checkerboard that does not scale with zoom, as in Photoshop. |
| **Dock area** | Right-hand side. Color, Swatches and Info share a tab group; History stacks below it, then Layers and Paths share a second tab group — CS6 tabs Layers, Channels and Paths together, and there is no Channels panel yet. Panels are dockable left or right, and float when dragged out. |
| **Status bar** | An **editable zoom field**, the document size, and the live cursor position in document coordinates. |

The colour palette is the CS6 "Kona" dark theme, in
[shell/resources/theme.qss](../shell/resources/theme.qss):

| Role | Colour |
|---|---|
| Panel background | `#535353` |
| Control background | `#454545` |
| Sunken / input background | `#3c3c3c` |
| Canvas surround | `#1e1e1e` |
| Text | `#d4d4d4` |
| Selection accent | `#4b6eaf` |

---

## 3. The tool strip

Icons are line-art reconstructions in CS6's visual language, authored as SVG on
a 20×20 grid and rendered at the display's device pixel ratio — not Adobe's
artwork. They are drawn in the same light grey as panel text (`#d4d4d4`).

A tool with hidden tools behind it shows a **small filled triangle in the
bottom-right corner** of its button. Opening the flyout:

- **press and hold** the button for 400 ms, or
- **right-click** it.

The flyout lists the whole CS6 group with each entry's own icon, the shortcut
letter right-aligned, and a check against the variant in use. Entries the
engine does not implement are listed but **disabled**, so the menu keeps CS6's
shape without pretending to work.

Flyout entries get their own artwork where CS6 gives them distinct icons: the
marquee group's four shapes; the lasso group's freehand loop, straight-segment
polygon and beaded magnetic curve; the healing group's spotted and plain
bandages, stitched patch, crossing arrows and eye; the Quick Selection brush and Magic Wand;
the eyedropper group's targeted pipette, ruler, note page and 1-2-3; and the
crop group's brackets, perspective mesh, slice blade and slice pointer. Other
groups' entries reuse the parent icon.

Where several entries share a letter, **Shift+letter cycles between them**,
matching CS6's "Use Shift Key for Tool Switch" default. Each tool remembers the
variant you last used and its strip button shows that variant's icon.

### Tools, top to bottom

Separators mark CS6's four functional groups.

| Icon | Tool | Key | Flyout | Works |
|---|---|---|---|---|
| Four-way arrow | Move | `V` | — | ✅ |
| | | | | |
| Dashed rectangle | **Marquee** | `M` | Rectangular ✅ · Elliptical ✅ · Single Row ✅ · Single Column ✅ | ✅ all four |
| Rope loop with tail | **Lasso** | `L` | Lasso ✅ · Polygonal ✅ · Magnetic ✅ | ✅ all three |
| Dashed circle + sparkle | **Quick Selection** | `W` | Quick Selection ✅ · Magic Wand ✅ | ✅ both |
| Two overlapping corners | **Crop** | `C` | Crop ✅ · Perspective Crop ✅ · Slice ✅ · Slice Select ✅ | ✅ all four |
| Pipette | **Eyedropper** | `I` | Eyedropper ✅ · Color Sampler ✅ · Ruler ✅ · Note ✅ · Count ✅ | ✅ all five |
| | | | | |
| Angled bandage + spot | **Spot Healing Brush** | `J` | Spot Healing ✅ · Healing ✅ · Patch ✅ · Content-Aware Move ✅ · Red Eye ✅ | ✅ all five |
| Brush with bristles | **Brush** | `B` | Brush ✅ · Pencil ✅ · Color Replacement ✅ · Mixer Brush ✅ | ✅ all four |
| Rubber stamp | **Clone Stamp** | `S` | Clone Stamp ✅ · Pattern Stamp ⛔ | first only |
| Brush + circular arrow | History Brush | `Y` | History Brush ✅ · Art History Brush ⛔ | first only |
| Angled eraser block | Eraser | `E` | Eraser ✅ · Background Eraser ⛔ · Magic Eraser ⛔ | first only |
| Rectangle fading out | **Gradient** | `G` | Gradient ✅ · Paint Bucket ✅ | ✅ both |
| Waterdrop | **Blur** | — | Blur ✅ · Sharpen ✅ · Smudge ✅ | ✅ all three |
| Dodging paddle | **Dodge** | `O` | Dodge ✅ · Burn ✅ · Sponge ✅ | ✅ all three |
| | | | | |
| Nib with anchor point | **Pen** | `P` | Pen ✅ · Freeform Pen ✅ · Add Anchor ✅ · Delete Anchor ✅ · Convert Point ✅ | ✅ all five |
| Serif **T** | Horizontal Type | `T` | Horizontal ✅ · Vertical ⛔ · Horizontal Mask ⛔ · Vertical Mask ⛔ | ⛔ |
| Solid arrow pointer | **Path Selection** | `A` | Path Selection ✅ · Direct Selection ✅ | ✅ both |
| Filled rectangle | Rectangle | `U` | Rectangle ✅ · Rounded Rectangle ⛔ · Ellipse ⛔ · Polygon ⛔ · Line ⛔ · Custom Shape ⛔ | ⛔ |
| | | | | |
| Open hand | Hand | `H` | Hand ✅ · Rotate View ⛔ | ✅ |
| Magnifier with **+** | Zoom | `Z` | — | ✅ |

**Works** describes the engine behind the tool, which is not the same as the
flyout being populated. Type and Shape are present in the strip with correct
icons and shortcuts, but selecting them does nothing on the canvas yet.

Marquee, Lasso, Eyedropper, Healing and Crop are fully implemented groups, and
Quick Selection has both of its entries. `Shift+M`, `Shift+L`, `Shift+W`,
`Shift+I`, `Shift+J` and `Shift+C` cycle within them, as CS6 does.

### Strip footer

| Control | Key | State |
|---|---|---|
| Foreground / background swatch | click either half to open the Color Picker | ✅ |
| Reset to black/white | `D` | ✅ |
| Swap foreground and background | `X` | ✅ |
| Edit in Quick Mask Mode | `Q` | button toggles and the icon changes; **editing behaviour is not implemented** |
| Change Screen Mode | `F` | ⛔ disabled; holds its place in the strip |

---

## 4. Tool options bar

The bar is rebuilt whenever the tool or variant changes. It always opens with
the active variant's name in bold — switching to Elliptical says
"Elliptical Marquee Tool", not "Marquee".

| Active tool | Options shown |
|---|---|
| Brush, Eraser, Spot Healing, Clone Stamp, History Brush | A **brush tip button** showing the current tip and its diameter — click for the preset picker — then **Opacity** (0–100%) and **Flow** (1–100%). All live, pushed to the engine on change |
| Clone Stamp | Additionally **Aligned** (on by default) and **Sample** (Current Layer · Current & Below · All Layers), then a reminder to `Alt`+click a source point |
| Gradient | The **gradient swatch** (click for the preset menu, every entry previewed by the engine), the five **type** buttons (Linear · Radial · Angle · Reflected · Diamond), **Mode**, **Opacity**, then **Reverse**, **Dither** (on by default) and **Transparency** |
| Dodge, Burn | The brush tip button, then **Range** (Shadows · Midtones · Highlights), **Exposure** (0–100%) and **Protect Tones** (ticked). Opacity and Flow are disabled — Exposure is the only strength they have |
| Sponge | The brush tip button, then **Mode** (Desaturate · Saturate), **Flow** (0–100%) and **Vibrance** (ticked) |
| Blur, Sharpen, Smudge | The brush tip button, then **Mode** (CS6's cut-down list: Normal · Darken · Lighten · Hue · Saturation · Color · Luminosity), **Strength** (0–100%) and **Sample All Layers**, shared by all three. Sharpen adds **Protect Detail** (ticked), Smudge adds **Finger Painting**. Opacity and Flow are disabled — how much they do is Strength's business |
| Paint Bucket | **Fill** (Foreground · Pattern — the latter disabled, there are no patterns), **Mode**, **Opacity**, **Tolerance** (0–255), then **Anti-alias**, **Contiguous** and **All Layers** |
| Color Replacement | The tip button and **Opacity**, then **Mode** (Hue · Saturation · Color · Luminosity), **Sampling** (Continuous · Once · Background Swatch), **Limits** (Discontiguous · Contiguous · Find Edges), **Tolerance** and **Anti-alias** |
| Mixer Brush | The tip button, then the **load swatch** (its menu holds Load Brush and Clean Brush), the **Load After Stroke** / **Clean After Stroke** toggles, the **preset menu** (Dry · Dry, Light Load · Moist / Wet / Very Wet × Light / Heavy Mix), **Wet**, **Load**, **Mix**, **Flow** and **Sample All Layers**. `Alt`+click on the canvas loads the brush from the image. No Opacity — how much paint reaches the canvas is Wet, Load and Flow's business. Load and Mix grey out while Wet is 0, where they have no effect |
| Pencil | The tip button and **Opacity**, then **Auto Erase**. No Flow — the Pencil lays whole pixels, so there is nothing to build up gradually |
| Spot Healing Brush | Additionally **Type**: Proximity Match · Create Texture · Content-Aware (CS6's default). Opacity and Flow are disabled — the region is rebuilt, not painted, so they have nothing to act on |
| Healing Brush | Brush controls, and a reminder to `Alt`+click a source point first |
| Patch | **Combine mode** buttons, **Patch** mode (Normal · Content-Aware), the **Source** / **Destination** pair, **Transparent**, and **Use Pattern** (disabled — no patterns yet). Source, Destination and Transparent grey out under Content-Aware, which does not sample from the drag at all |
| Content-Aware Move | **Combine mode** buttons, **Mode** (Move · Extend), **Structure** (1–7, how strictly the fill follows edges), **Color** (0–10, how far the moved pixels adapt to their new surroundings), **Sample All Layers**, and CS6's transform-on-drop **T** (disabled — transforms are not implemented) |
| Red Eye | **Pupil Size** and **Darken Amount**, both 0–100% |
| Pen | **Auto Add/Delete** (ticked) and **Rubber Band** (ticked), then a gesture hint |
| Freeform Pen | **Curve Fit** (0.5–10px, how closely the fitted path follows the drag) and a disabled **Magnetic** checkbox — see below |
| Add Anchor Point, Delete Anchor Point, Convert Point | A one-line hint for the click (and, for Convert Point, drag) each performs |
| Path Selection, Direct Selection | A one-line hint: drag a subpath, or drag an anchor/handle |
| Marquee, Lasso, Quick Selection | **Combine mode** buttons (new / add / subtract / intersect), **Feather** (0–1000 px), then a modifier hint: `Ctrl+Shift` = add · `Ctrl+Alt` = subtract · click = deselect |
| Lasso (Polygonal / Magnetic) | Same controls; the hint becomes: click to place points · double-click or `Enter` to close · `Backspace` undoes one · `Esc` cancels |
| Lasso (Magnetic) | Additionally **Width** (1–256 px), **Contrast** (1–100%) and **Frequency** (0–100), CS6's three edge-detection settings |
| Quick Selection | Combine mode buttons, then **Size** (the brush diameter the region grows from). No Feather and no Tolerance — CS6 gives it neither |
| Magic Wand | Combine mode buttons, **Tolerance** (0–255), **Anti-alias** and **Contiguous** checkboxes. No Feather, as in CS6 |
| Marquee (Single Row / Single Column) | Same controls; the hint changes to: click to select a line · `Ctrl+Shift` = add · `Ctrl+Alt` = subtract |
| Crop | **Ratio preset** (Unconstrained, 1:1, 4:5, 5:7, 2:3, 16:9), **Delete Cropped Pixels**, and a ✘ / ✓ pair to cancel or apply |
| Crop (Perspective) | Just the ✘ / ✓ pair — CS6 gives it neither a ratio nor Delete Cropped Pixels, since the output size comes from the marked quad and everything outside it is resampled away regardless |
| Color Sampler | **Clear** only — the sampler values read out in the Info panel, as they do in CS6 |
| Ruler | **X, Y, W, H, A, D1** readouts and **Clear** — Photoshop's own fields, in its order |
| Note | A **note count** and **Clear** |
| Count | The **running count** and **Clear** |
| Crop (Slice) | **Clear Slices** and **Save Slices...** |
| Crop (Slice Select) | **Clear Slices**, **Delete Slice** and **Save Slices...** |
| Zoom | Hint: click to zoom in · `Alt`+click to zoom out |
| Move | Hint: drag to move the active layer · arrow keys nudge |
| Anything else | Name only |

The **combine mode** is a radio set and persists across tool switches, as CS6
does. Holding a modifier overrides it for that one drag without moving the
checked button, and the modifiers are sampled when the drag *starts* — letting
go of Shift mid-drag does not change the mode. In New mode a click without a
drag deselects; in the other three it leaves the selection alone.

**Feather** applies to selections made from then on, and softens only the
incoming region — the part of the selection that was already there keeps its
edge. Select ▸ Feather is the command that softens an existing selection.

Still absent from the selection bar: Anti-alias, Style (Normal / Fixed Ratio /
Fixed Size) with its Width and Height fields, and Refine Edge. Brush presets,
blend mode and airbrush are likewise absent from the brush bar.

### The Pen tool

A path is built from **anchor points** joined by straight or curved segments.
Plain clicking places a **corner** point and a straight segment; dragging
places a **smooth** point, and the drag sets a pair of direction handles that
stay collinear through the anchor, so the curve flows through it without a
kink. `Alt`-dragging places a smooth point whose *incoming* segment is left
alone — only the one about to be drawn curves, which is how a shape with one
sharp corner and otherwise rounded sides gets drawn by hand.

**Auto Add/Delete** (on by default) is what lets you keep adding to or editing
an existing path without switching tools: hover the finished part of the
active path and a segment offers to take a new anchor, or an anchor offers to
be removed. It only applies between drawing sessions — mid-subpath, a click
always extends what you are drawing, or closes it if it lands back on the
first anchor. **Rubber Band** previews the segment about to be placed, live,
from the last anchor to the cursor.

`Enter`, double-click, or `Esc` all stop extending the open subpath without
closing it — none of them throw its points away, matching Photoshop. Switching
to a different tool does the same, silently.

The **Freeform Pen** draws as if with a pencil: the drag is fitted to a
polyline afterward, simplified by distance (Douglas-Peucker, tuned by **Curve
Fit**) rather than by fitting actual curves to the stroke. That is a real
simplification against Photoshop, whose Freeform Pen produces smooth Bezier
anchors — a freehand circle here comes out as a many-sided corner-only
polygon, fully editable afterward with Direct Selection and Convert Point but
not smooth to start with. Dragging back near the start closes the loop. Its
**Magnetic** option, which would snap the traced anchors to edges the way the
Magnetic Lasso snaps a selection, is not implemented.

**Add Anchor Point**, **Delete Anchor Point** and **Convert Point** are the
same operations Auto Add/Delete and Alt-dragging already offer from the plain
Pen tool, exposed as their own tools for when you want the click to always mean
one specific thing. Convert Point's click strips a smooth point's handles back
to a corner; its drag on a corner pulls out a fresh symmetric pair, and its
drag on an existing handle breaks that handle free of the opposite one.

**Path Selection** grabs and drags a whole subpath. **Direct Selection** grabs
one anchor or handle: dragging an anchor carries its handles with it, and
dragging a handle reshapes the curve, keeping a smooth point's opposite handle
collinear (its own length is untouched) unless `Alt` breaks it free
permanently — the same distinction Convert Point's handle-drag makes, reached a
second way.

A path's geometry is **not** part of the undo history — the same choice already
made for slices and annotations, since paths are vector overlay data, not
pixels. What a finished path *does* to the image is undoable as normal: Fill
Path and Stroke Path each commit one step, the way a Brush stroke does.

Several subpaths in one path combine under **nonzero winding** when turned into
pixels — Fill Path or Make Selection — so a subpath wound the opposite way from
the one around it cuts a hole rather than adding a second region. This is how a
compound shape like a letter "O" is built from two circles in Photoshop's own
path model, and it works here the same way. An open subpath is closed for this
purpose: a selection has to enclose an area, so where the pen was lifted is
implicitly joined back to where it started.

Stroke Path always uses the Brush tool's current tip and foreground colour.
Photoshop lets Stroke Path use any tool's settings from a menu; this
simplification covers the overwhelmingly common case.

---

## 5. Panels

All four dock right by default, can be moved to the left dock area, and float
when dragged out. Every panel has an entry in the **Window** menu that toggles
its visibility.

### Color · `F6`

Tabbed with Swatches, as CS6 ships them. Shows the foreground and background
swatches; clicking one opens the **Color Picker**:

- Square field plus vertical ramp, driven by the **H / S / B / R / G / B** radio
  buttons. Each choice re-maps *both* field axes and the ramp the way CS6 does.
- New / current comparison swatches.
- **HSB**, **RGB**, **Lab** and **CMYK** readouts, all editable, plus a hex field.
- **Only Web Colors** snaps to the 216-colour web-safe palette.
- Lab uses the **D50 white point with Bradford adaptation from D65**, which is
  what Photoshop uses — values match the real picker digit for digit.

Not implemented: the **L / a / b radio buttons** (Lab is editable but cannot
drive the field), ICC-profile CMYK (the conversion has no press profile, so it
will not match a colour-managed Photoshop), **Add to Swatches**, and **Color
Libraries**.

### Swatches

A placeholder so the Color/Swatches tab pair reads like CS6. No swatch
management yet.

### History

The linear undo stack, newest at the bottom. Click any state to jump to it.
The stack is bounded by both state count and total memory. A whole brush stroke
is one entry, not one per dab.

### Brush Preset Picker

The popup behind the options bar's tip button, in CS6's layout: a preview with a
centre crosshair, **Size** (1–5000 px) and **Hardness** (0–100%) as a slider and
a number each, the current diameter, and a grid of preset thumbnails with their
diameters printed under them.

Size and Hardness live here rather than on the bar, as they do in CS6. Editing
either keeps the rest of the tip, so nudging Size does not turn a spatter brush
back into a plain circle.

The set covers CS6's families: **soft** and **hard round**, **flat** and
**chisel** (a squashed, rotated tip), **charcoal** and **chalk** (broad tips with
the edge broken by jitter), **spatter**, **star**, and **grass** and **dune
grass**. Each preset carries a full tip — size, hardness, roundness, angle,
scatter, dab count, the three jitter amounts and spacing — which is what the
engine's brush is (see `core/src/brush.rs`).

The **Color Replacement Brush** paints the foreground colour onto pixels that
already resemble a sampled one. In its default **Color** mode it replaces hue and
saturation while keeping each pixel's own brightness, so recolouring shaded
material keeps the shading rather than flattening it to a sticker. It edits the
layer per dab rather than accumulating a stroke, because what it replaces depends
on what is already there — and with Continuous sampling the reference colour
changes as the brush moves. A stroke is still one undo step, and `Esc` abandons it.

The **toning tools** — Dodge, Burn and Sponge — come from the darkroom, and two of
the names still describe what they did there. Dodging was holding something back
from the enlarger's light so that part of the print came out lighter; burning was
giving one part extra exposure so it came out darker.

Two ideas do most of the work:

**Range** is the band of the tonal scale the tool works hardest in. Set to
Highlights, a burn bites into a bright sky and barely touches the shadows under
it, which is what makes these usable on a photograph rather than a blunt
brightness brush. The bands are Gaussian and overlap deliberately: three that each
owned a third of the scale would leave a seam where one handed over to the next.

**Protect Tones** changes the pixel's *luminance* and puts its own colour back,
rather than scaling its channels. Scaling channels moves each one by its own
headroom, so the darkest gains proportionally the most and the colour washes out —
that is how dodging ends up bleaching skin and burning ends up muddy. Dodged
gently with headroom to spare, Protect Tones holds the ratios between channels
exactly. It also means the tool cannot clip: the step is always a fraction of what
is left, so white and black are approached and never reached.

The **Sponge** moves colour toward or away from grey along the one axis that
matters, so it can neither shift a hue nor find colour in a grey pixel that has
none. **Vibrance** eases the effect off where the colour is already vivid, which
lifts the flat parts of an image without driving the vivid parts into clipping.

All three change colour and never coverage, and — like the focus tools — each dab
works on what the last one left, so dwelling goes on lightening, darkening or
draining. They read one pixel at a time, which is why CS6 gives them no Sample All
Layers: there is nothing a lower layer could add to a pixel's own tone.

**Blur** and **Sharpen** are one tool with its sign flipped. Both read the same
3×3 neighbourhood and both move a pixel along the line between itself and that
neighbourhood's average — Blur *toward* it, Sharpen *away* from it. That is why
they share a module as they share a button.

Neither is the corresponding *filter*. Filter ▸ Blur ▸ Gaussian Blur and
Filter ▸ Sharpen are one pass over a whole layer at a radius you choose; these are
brushes, working only where the tip passes and **getting stronger the more it
passes**. That accumulation is why each dab applies straight to the layer rather
than accumulating into a mask the way a paint stroke does — every dab has to work
on what the last one left. A mask would give one blur however long you dwelt.

The kernel is a fixed 3×3, deliberately: Photoshop's focus tools do not scale
their radius with the brush either. A big tip works a *wider area* by the same
amount per dab, and depth comes from working the same spot. A radius that grew
with the brush would turn a single click of a large tip into a smeared hole.

Sharpening exaggerates a pixel's departure from the average of its neighbours —
its *curvature*. A straight ramp has none, so an even gradient survives any amount
of sharpening untouched, which is worth knowing and is pinned by a test. **Protect
Detail** holds each pixel inside the range its own neighbours span; without it,
pass after pass overshoots into haloes and blown speckle.

The **Smudge tool** carries the image with it. The finger holds a patch of pixels
picked up where it last was, lays that patch down where it is now, and picks the
result up again for the next dab — so structure gets dragged along the stroke and a
smudged edge streaks in the direction of travel instead of merely going soft.
Carrying a *patch* rather than one colour is what makes that work: a single
averaged colour could only spread a flat smear. **Strength** is how much of the
patch each dab lays down, and **Finger Painting** loads the finger with the
foreground colour first, so the stroke starts by dragging paint in.

All three share **Strength**, **Mode** and **Sample All Layers**. Mode narrows what
may change — Luminosity works the shading and leaves the colour, Color the reverse
— and CS6 offers only the seven modes that mean anything for tools whose source
*is* their destination, worked on. Sample All Layers reads from the composite while
still writing to the active layer. The blur's neighbourhood is averaged in
premultiplied colour, so softening the edge of a layer feathers it outward instead
of drawing a dark rim inward from the transparent pixels beyond.

The **Paint Bucket** is the Magic Wand with a colour instead of a selection. It
asks the same question — which pixels belong with the one clicked — of the same
flood, then fills the mask that comes back instead of selecting it. So
**Tolerance**, **Contiguous** and **Anti-alias** mean exactly what they mean for
the wand, down to Tolerance being the per-channel *maximum* distance: 32 admits
anything within 32 levels on every channel, and one badly-off channel is enough to
reject a pixel.

**All Layers** decides what matches from the composite, so a boundary that exists
only on the layer below still stops the fill; the paint lands on the active layer
either way. **Mode** and **Opacity** apply as they do to the gradient, the marquee
confines the fill, the transparency lock is honoured, and it is one undo step.
Filling with erase mode on lays down the background colour, as a stroke does.

The **Gradient tool** is a **ramp** plus a **shape**. The ramp — a list of colour
stops — answers "what colour at 40% along"; the shape answers "how far along is
this pixel". They are independent, which is why any preset can be drawn as any of
the five types.

| Type | How a pixel's place on the ramp is found |
|---|---|
| **Linear** | Its distance along the drag |
| **Radial** | Its distance from where the drag began |
| **Angle** | Its angle around the start, sweeping a full turn anticlockwise from the drag's direction |
| **Reflected** | As linear, mirrored either side of the start |
| **Diamond** | Manhattan distance from the start, in the drag's frame |

The ramp's ends extend across the rest of the layer rather than stopping at the
drag, and a click without a drag draws nothing — both as Photoshop behaves. A
marquee confines the fill, **Mode** blends it against what is there, the
transparency lock is honoured, and the whole thing is one undo step.

Interpolation is in **straight alpha**, with colour and opacity interpolated
separately: "Foreground to Transparent" has to keep its colour all the way along
while only the opacity falls off, and premultiplied interpolation would drag that
colour toward black as it faded.

**Dither** is on by default, because an 8-bit ramp stretched across a wide canvas
bands visibly without it. It dithers the *quantisation* — half a level of noise
before rounding — and not the ramp position. Nudging the position instead turns
every hard-edged preset (Transparent Stripes) into a band of speckle, which is
what a test now guards against.

The 15 presets are CS6's default set, and the engine owns them: the options bar
asks for one by name and renders its swatch through the engine, the same contract
the Image ▸ Adjustments menu uses, so a preview cannot drift from what the tool
paints. The first two and Transparent Stripes are built from the *current*
foreground and background, so the swatch follows the colour swatches as they
change.

The **Clone Stamp** copies pixels from one part of the image to another. `Alt`+click
sets the source, and the stroke then copies whatever sits at that offset from the
brush — exactly as sampled, seam and all. That plainness is the whole difference
from the Healing Brush, which transplants the source's *texture* and takes the
destination's lighting; the Clone Stamp is for when you want the pixels
themselves.

What it copies is a **snapshot taken when each stroke begins**, not the layer as
it stands. With a short offset the destination overlaps the source, and reading
live would feed every dab the previous dab's output and trail the source down the
whole stroke. Photoshop samples per stroke for the same reason, which is why
cloning with a small offset repeats the source once rather than smearing forever.

**Aligned** (CS6's default) keeps the offset the first stroke established, so the
sample point travels with the cursor across strokes and several strokes build up
one continuous copy. Off, every stroke measures afresh from the source point, so
each one starts copying the same material again. **Sample** chooses what is read:
the active layer alone, the active layer composited with everything beneath it, or
the whole visible image — the paint always lands on the active layer.

Sample defaults to **Current Layer**, as CS6's does, and that is the one thing
about the tool that surprises people: if the material you Alt-clicked lives on a
different layer, there is genuinely nothing under the source point and the stroke
copies transparency. Photoshop is silent about it; we say so in the status bar when
the source is set, and setting Sample to All Layers clones what you can see.

Otherwise it is an ordinary brush stroke: the same dabs, spacing, jitter, opacity
and flow, the same live preview (showing the cloned pixels, not the foreground
colour), the same single undo step, confined by a marquee and refused on a locked
layer like any other paint. The sampled point is dropped when the document
changes — Photoshop keeps a clone source per document, ours is one per engine, and
carrying it into another image would mean cloning from coordinates that mean
nothing there.

The **Mixer Brush** carries paint. Two colours meet at every dab: the
**reservoir**, what the brush is loaded with, and the **pickup**, the coverage-
weighted average of what lies under the tip — averaged rather than read at the
centre, which is what makes the tool blend instead of clone. **Wet** decides how
much the canvas takes part, **Mix** the balance between the two colours, **Load**
how much paint the brush holds, and **Flow** how fast each dab deposits. At Wet 0
none of the canvas takes part and the tool is an ordinary brush; wet, it both
deposits a mixture and carries the pickup along, which is what smears colour
across a boundary. Load runs down as the stroke goes: a dry brush that has run out
stops, a wet one keeps smearing with no colour of its own. The reservoir survives
the stroke, as in Photoshop — Clean After Stroke and Load After Stroke are what
change that. Like colour replacement this edits the layer per dab, since each dab
reads what the last one left; a stroke is still one undo step and `Esc` abandons it.

The **Pencil** is the same engine with antialiasing switched off: every pixel is
either fully painted or untouched. That is the whole difference, and it is why
hardness has no effect on it and why it is the tool for touching up single-pixel
lines. Its **Auto Erase** is decided once from the pixel the stroke begins on —
start on the foreground colour and the whole stroke paints the background colour
instead.

Hardness 100 still leaves about a pixel and a half of feather, as Photoshop's
hard round does, and dab edges are area-sampled rather than point-sampled — a
sharp edge crosses a pixel, and one sample per pixel would land either fully
inside or fully outside, which is what makes a brush look pixelated.

Thumbnails are rendered **by the engine**: it lays one step of the brush into a
small image and that image is the thumbnail, so a thumbnail cannot drift from
what the brush paints. The diameter is printed under each, as CS6 does.

The one gap is the brushes built from a bitmap tip image, such as the oil and
texture-comb ones. Those are approximated with scatter and jitter rather than
omitted, since an approximation that paints is more use than an empty slot.

### Info · `F8`

CS6's live readout, in its layout: **RGB** and **CMYK** across the top (both
labelled 8-bit), **X/Y** cursor position and **W/H** selection size beneath,
then a two-column grid of **colour sampler** values numbered `#1` upward, the
document's memory footprint on a `Doc:` line, and a hint that changes with the
active tool.

Everything is a readout; the panel never writes to the document. The colour
blocks blank when the cursor leaves the canvas rather than holding their last
value. Sampler values re-read on every canvas change, so editing under a
sampler updates it without moving it.

Choosing the Color Sampler or the Ruler brings this panel forward, since their
values are the whole point of those tools.

With the **Ruler** active the panel changes as CS6's does: the CMYK block is
replaced by **A** (angle, in degrees) and **L** (length), and **W/H** report the
ruler's deltas rather than the selection's size. Switching away puts CMYK back.

### Layers · `F7`

Top-first, as in Photoshop, laid out row for row as CS6's panel is: the filter
row, the blend mode and Opacity, the Lock row and Fill, the list, and seven
glyphs along the foot.

Rows are painted by a delegate rather than left to Qt, because CS6's row is made
of things a stock list item does not draw: an eye in its own column with a
divider, a bordered thumbnail sized to the layer's aspect ratio, the name
(italic for Background), and a padlock badge. Clicking the eye toggles visibility *without*
selecting the row, as it does in CS6. Adjustment layers show the adjustment's own
glyph on white in place of a thumbnail.

- Show/hide, reorder, rename (double-click), duplicate, delete
- **Opacity** and **Fill opacity**, each with the popup slider behind its arrow
- All **27 blend modes**, in CS6's grouped order with the separators
- **Locks** — see below
- Clipping masks and layer masks; Duplicate, Delete, Merge Down and the clipping
  toggle live on the row's right-click menu, where Photoshop keeps them
- The footer's **new layer**, **new adjustment layer** (with the kind menu) and
  **layer mask** buttons
- The **filter row**: Kind, with the pixel and adjustment filters live, and the
  switch that turns filtering on (red while it is)

#### Locks

CS6's four Lock buttons, and they are enforced in the engine rather than by
greying out the UI — every editing entry point checks, so no tool can slip past.

| Lock | Effect |
|---|---|
| **Lock transparent pixels** | Painting may recolour what is already there but cannot give an empty pixel any coverage. Honoured by brushes, the Mixer Brush and Fill |
| **Lock image pixels** | No tool may edit the layer's pixels: brushes, the Mixer Brush, Color Replacement, healing, red-eye, filters, adjustments, Fill and Clear are all refused. This is the lock that makes a layer untouchable |
| **Lock position** | The layer cannot be moved, by drag or by arrow key |
| **Lock all** | All three at once. A fully locked layer additionally cannot be deleted or merged |

Locked layers carry a padlock badge on their row — solid for Lock All, outlined
for a partial lock, the distinction CS6 draws. Using a tool on a pixel-locked
layer puts up Photoshop's own alert: "Could not use the brush tool because the
layer is locked."

Not implemented: layer groups (folders), layer effects/styles, layer linking,
adjustment-layer parameter editing after creation, and the Channels / Navigator
panels. The footer and filter-row buttons for those are present but disabled,
so the panel keeps CS6's shape.

### Paths

One row per saved path, plus the "Work Path" the Pen tool starts on its own the
first time it is used with nothing selected here — exactly as Photoshop's panel
behaves. Selecting a row makes it the path the Pen, Path Selection and Direct
Selection tools act on; double-click a name to rename it.

- **New Path**, **Duplicate Path** and **Delete Path** (the last two on the
  row's right-click menu, where CS6 keeps them)
- **Fill Path** with the foreground colour, **Stroke Path** with the current
  brush, and **Load Path as a Selection** (prompts for a feather radius)
- Every row shows the same generic curve-and-anchors glyph rather than a live
  preview of its own shape — a simplification, the same one an adjustment
  layer's thumbnail already makes in the Layers panel

Not implemented: a live per-path thumbnail, and **Make Work Path from
Selection** — tracing a selection's contour into anchors is a real piece of
work of its own (marching squares, then simplifying the trace) that this pass
does not include. Its footer button is present, disabled.

---

## 6. Menus

✅ wired · ⛔ command exists in the keymap but has no menu entry or handler yet.

| Menu | Entries |
|---|---|
| **File** | New ✅ · Open ✅ · Save ✅ · Save As ✅ · Exit ✅ |
| **Edit** | Undo ✅ · Step Forward ✅ · Step Backward ✅ · Fill with Foreground ✅ · Fill with Background ✅ |
| **Image ▸ Adjustments** | Levels ✅ · Hue/Saturation ✅ · Color Balance ✅ · Black & White ✅ · Invert ✅ · Desaturate ✅ — then Posterize ✅ · Threshold ✅ · Brightness/Contrast ✅ · Exposure ✅ |
| **Image** | Canvas Size ✅ |
| **Layer** | New Layer ✅ · Layer via Copy ✅ · Create/Release Clipping Mask ✅ · Merge Down ✅ · Flatten Image ✅ · Delete Layer ✅ |
| **Select** | All ✅ · Deselect ✅ · Inverse ✅ · Feather ✅ |
| **Filter** | Blur ▸ Gaussian Blur ✅ · Sharpen ▸ Sharpen ✅ · Sharpen ▸ Unsharp Mask ✅ · Noise ▸ Add Noise ✅ |
| **View** | Zoom In ✅ · Zoom Out ✅ · Fit on Screen ✅ · Actual Pixels ✅ |
| **Window** | One toggle per panel, generated from the docks ✅ |
| **Help** | About ✅ |

Adjustments apply **destructively** from this menu. Non-destructive adjustment
layers are created from the Layers panel; their parameters cannot be reopened
for editing afterwards.

Many commands in the keymap have no menu home yet (Cut/Copy/Paste, Free
Transform, Image Size, Rulers, Guides, …). They are listed in [§7](#7-keyboard-shortcuts)
as unbound-in-UI so the keymap stays complete and CS6-faithful — adding the
menu entry is then the only work left.

---

## 7. Keyboard shortcuts

Defaults are CS6's, stored as data in
[shell/resources/shortcuts.json](../shell/resources/shortcuts.json). No key
combination is ever hard-coded in a widget.

**Status column**

- ✅ — bound and does something
- ⚪ — bound, but the command is not implemented yet (the key is reserved)
- ⛔ — CS6 has this and PhotoRust does not bind it at all

### 7.1 Tools

Single letters, no modifier. All ✅ select the tool; whether the tool *does*
anything is [§3](#3-the-tool-strip).

| Key | Tool | | Key | Tool |
|---|---|---|---|---|
| `V` | Move ✅ | | `S` | Clone Stamp ✅ |
| `M` | Marquee ✅ | | `Y` | History Brush ✅ |
| `Shift+M` | Cycle Rectangular ↔ Elliptical ✅ | | `E` | Eraser ✅ |
| `L` | Lasso ✅ | | `G` | Gradient ✅ |
| `Shift+L` | Cycle Lasso → Polygonal → Magnetic ✅ | | | |
| `Shift+W` | Cycle Quick Selection ↔ Magic Wand ✅ | | | |
| `Shift+C` | Cycle Crop → Perspective → Slice → Slice Select ✅ | | | |
| `Shift+I` | Cycle Eyedropper → Color Sampler → Ruler → Note → Count ✅ | | | |
| `Shift+J` | Cycle Spot Healing → Healing → Patch → Content-Aware Move → Red Eye ✅ | | | |
| `Shift+B` | Cycle Brush → Pencil → Color Replacement → Mixer Brush ✅ | | | |
| `Shift+G` | Cycle Gradient ↔ Paint Bucket ✅ | | | |
| `Shift+O` | Cycle Dodge → Burn → Sponge ✅ | | | |
| `Shift+P` | Cycle Pen ↔ Freeform Pen ✅ | | | |
| `Shift+A` | Cycle Path Selection ↔ Direct Selection ✅ | | | |
| `W` | Quick Selection ✅ | | `O` | Dodge ✅ |
| `C` | Crop ✅ | | `P` | Pen ✅ |
| `I` | Eyedropper ✅ | | `T` | Horizontal Type ⚪ |
| `J` | Spot Healing Brush ✅ | | `A` | Path Selection ✅ |
| `B` | Brush ✅ | | `U` | Rectangle ⚪ |
| `H` | Hand ✅ | | `R` | Rotate View ⚪ |
| `Z` | Zoom ✅ | | | |

| Key | Action | Status |
|---|---|---|
| `D` | Default foreground/background colours | ✅ |
| `X` | Switch foreground/background colours | ✅ |
| `Q` | Toggle Quick Mask mode | ⚪ button only |
| `F` | Cycle screen modes | ⚪ |

CS6 also binds these; PhotoRust does **not** yet (⛔): `[` / `]` brush size,
`{` / `}` brush hardness, `,` / `.` previous/next brush, `<` / `>` first/last
brush, `/` toggle preserve transparency.

### 7.2 File

| Linux | macOS | Command | Status |
|---|---|---|---|
| `Ctrl+N` | `Cmd+N` | New… | ✅ |
| `Ctrl+O` | `Cmd+O` | Open… | ✅ |
| `Alt+Ctrl+O` | `Opt+Cmd+O` | Browse in Bridge… | ⚪ |
| `Ctrl+W` | `Cmd+W` | Close | ✅ closes the active document, prompting if it has unsaved changes |
| `Alt+Ctrl+W` | `Opt+Cmd+W` | Close All | ⚪ |
| `Ctrl+S` | `Cmd+S` | Save | ✅ |
| `Shift+Ctrl+S` | `Shift+Cmd+S` | Save As… | ✅ |
| `Alt+Shift+Ctrl+S` | `Opt+Shift+Cmd+S` | Save Slices… | ✅ writes each slice as a PNG. CS6 puts this on Save for Web, whose dialog does not exist yet |
| `F12` | `F12` | Revert | ⚪ |
| `Alt+Shift+Ctrl+I` | `Opt+Shift+Cmd+I` | File Info… | ⚪ |
| `Ctrl+P` | `Cmd+P` | Print… | ⚪ |
| `Alt+Shift+Ctrl+P` | `Opt+Shift+Cmd+P` | Print One Copy | ⚪ |
| `Ctrl+Q` | `Cmd+Q` | Quit | ✅ |

⛔ Not bound: Close and Go to Bridge (`Shift+Ctrl+W`).
macOS-only in CS6, no Linux equivalent: Hide Photoshop `Cmd+H`, Hide Others
`Opt+Cmd+H`.

### 7.3 Edit

| Linux | macOS | Command | Status |
|---|---|---|---|
| `Ctrl+Z` | `Cmd+Z` | Undo | ✅ |
| `Shift+Ctrl+Z` | `Shift+Cmd+Z` | Step Forward | ✅ |
| `Alt+Ctrl+Z` | `Opt+Cmd+Z` | Step Backward | ✅ |
| `Shift+Ctrl+F` | `Shift+Cmd+F` | Fade… | ⚪ |
| `Ctrl+X` | `Cmd+X` | Cut | ⚪ |
| `Ctrl+C` | `Cmd+C` | Copy | ⚪ |
| `Shift+Ctrl+C` | `Shift+Cmd+C` | Copy Merged | ⚪ |
| `Ctrl+V` | `Cmd+V` | Paste | ⚪ |
| `Shift+Ctrl+V` | `Shift+Cmd+V` | Paste in Place | ⚪ |
| `Alt+Shift+Ctrl+V` | `Opt+Shift+Cmd+V` | Paste Into | ⚪ |
| `Shift+F5` | `Shift+F5` | Fill… | ⚪ |
| `Alt+Backspace` | `Opt+Delete` | Fill with Foreground Color | ✅ |
| `Ctrl+Backspace` | `Cmd+Delete` | Fill with Background Color | ✅ |
| `Ctrl+T` | `Cmd+T` | Free Transform | ⚪ |
| `Shift+Ctrl+T` | `Shift+Cmd+T` | Transform Again | ⚪ |
| `Shift+Ctrl+K` | `Shift+Cmd+K` | Color Settings… | ⚪ |
| `Alt+Shift+Ctrl+K` | `Opt+Shift+Cmd+K` | Keyboard Shortcuts… | ⚪ |
| `Alt+Shift+Ctrl+M` | `Opt+Shift+Cmd+M` | Menus… | ⚪ |
| `Ctrl+K` | `Cmd+K` | Preferences ▸ General… | ⚪ |

CS6 also gives Cut/Copy/Paste the function keys `F2`/`F3`/`F4`, and Undo `F1`.
PhotoRust binds `F1` to Help instead, as CS6 does on Windows.
⛔ Not bound: Content-Aware Scale (`Alt+Shift+Ctrl+C`).

### 7.4 Image

| Linux | macOS | Command | Status |
|---|---|---|---|
| `Ctrl+L` | `Cmd+L` | Levels… | ✅ |
| `Ctrl+M` | `Cmd+M` | Curves… | ⚪ |
| `Ctrl+U` | `Cmd+U` | Hue/Saturation… | ✅ |
| `Ctrl+B` | `Cmd+B` | Color Balance… | ✅ |
| `Alt+Shift+Ctrl+B` | `Opt+Shift+Cmd+B` | Black & White… | ✅ |
| `Ctrl+I` | `Cmd+I` | Invert | ✅ |
| `Shift+Ctrl+U` | `Shift+Cmd+U` | Desaturate | ✅ |
| `Shift+Ctrl+L` | `Shift+Cmd+L` | Auto Tone | ⚪ |
| `Alt+Shift+Ctrl+L` | `Opt+Shift+Cmd+L` | Auto Contrast | ⚪ |
| `Shift+Ctrl+B` | `Shift+Cmd+B` | Auto Color | ⚪ |
| `Alt+Ctrl+I` | `Opt+Cmd+I` | Image Size… | ⚪ |
| `Alt+Ctrl+C` | `Opt+Cmd+C` | Canvas Size… | ✅ |

Adjustments with a ✅ apply immediately at default strength — the parameter
dialogs are not built, so there is nothing to tune yet.

### 7.5 Layer

| Linux | macOS | Command | Status |
|---|---|---|---|
| `Shift+Ctrl+N` | `Shift+Cmd+N` | New Layer… | ✅ |
| `Ctrl+J` | `Cmd+J` | Layer via Copy | ✅ |
| `Shift+Ctrl+J` | `Shift+Cmd+J` | Layer via Cut | ⚪ |
| `Alt+Ctrl+G` | `Opt+Cmd+G` | Create/Release Clipping Mask | ✅ |
| `Ctrl+G` | `Cmd+G` | Group Layers | ⚪ |
| `Shift+Ctrl+G` | `Shift+Cmd+G` | Ungroup Layers | ⚪ |
| `Shift+Ctrl+]` | `Shift+Cmd+]` | Bring to Front | ⚪ |
| `Ctrl+]` | `Cmd+]` | Bring Forward | ⚪ |
| `Ctrl+[` | `Cmd+[` | Send Backward | ⚪ |
| `Shift+Ctrl+[` | `Shift+Cmd+[` | Send to Back | ⚪ |
| `Ctrl+E` | `Cmd+E` | Merge Down | ✅ |
| `Shift+Ctrl+E` | `Shift+Cmd+E` | Flatten Image | ✅ |
| `Del` | `Del` | Delete Layer | ✅ |

CS6 names `Ctrl+E` "Merge Layers" when several are selected; PhotoRust has no
multi-layer selection, so it is always Merge Down.

### 7.6 Select

| Linux | macOS | Command | Status |
|---|---|---|---|
| `Ctrl+A` | `Cmd+A` | All | ✅ |
| `Ctrl+D` | `Cmd+D` | Deselect | ✅ |
| `Shift+Ctrl+D` | `Shift+Cmd+D` | Reselect | ⚪ |
| `Shift+Ctrl+I` | `Shift+Cmd+I` | Inverse | ✅ |
| `Alt+Ctrl+A` | `Opt+Cmd+A` | All Layers | ⚪ |
| `Alt+Ctrl+R` | `Opt+Cmd+R` | Refine Edge… | ⚪ |
| `Shift+F6` | `Shift+F6` | Feather… | ✅ |

CS6 also binds Inverse to `Shift+F7`. ⛔ Not bound: Find Layers
(`Alt+Shift+Ctrl+F`).

### 7.7 Filter

| Linux | macOS | Command | Status |
|---|---|---|---|
| `Ctrl+F` | `Cmd+F` | Last Filter | ⚪ |
| `Shift+Ctrl+X` | `Shift+Cmd+X` | Liquify… | ⚪ |
| `Alt+Ctrl+V` | `Opt+Cmd+V` | Vanishing Point… | ⚪ |
| — | — | Gaussian Blur… | ✅ menu only |
| — | — | Sharpen | ✅ menu only |
| — | — | Unsharp Mask… | ✅ menu only |
| — | — | Add Noise… | ✅ menu only |

Fade Filter deliberately has **no** binding: CS6 reaches Fade through
Edit ▸ Fade (`Shift+Ctrl+F`), and two commands on one key is an ambiguous
shortcut in Qt, which kills both.
⛔ Not bound: Adaptive Wide Angle (`Shift+Ctrl+A`), Lens Correction
(`Shift+Ctrl+R`).

### 7.8 View

| Linux | macOS | Command | Status |
|---|---|---|---|
| `Ctrl++` | `Cmd++` | Zoom In — also `Ctrl+=` / `Ctrl+Shift+=` | ✅ |
| `Ctrl+-` | `Cmd+-` | Zoom Out — also `Ctrl+_` | ✅ |
| `Ctrl+0` | `Cmd+0` | Fit on Screen | ✅ |
| `Ctrl+1` | `Cmd+1` | Actual Pixels | ✅ |
| `Ctrl+Y` | `Cmd+Y` | Proof Colors | ⚪ |
| `Shift+Ctrl+Y` | `Shift+Cmd+Y` | Gamut Warning | ⚪ |
| `Ctrl+H` | `Ctrl+Cmd+H` | Extras | ⚪ |
| `Shift+Ctrl+H` | `Shift+Cmd+H` | Target Path | ⚪ |
| `Ctrl+'` | `Cmd+'` | Grid | ⚪ |
| `Ctrl+;` | `Cmd+;` | Guides | ⚪ |
| `Ctrl+R` | `Cmd+R` | Rulers | ⚪ |
| `Shift+Ctrl+;` | `Shift+Cmd+;` | Snap | ⚪ |
| `Alt+Ctrl+;` | `Opt+Cmd+;` | Lock Guides | ⚪ |

CS6 also accepts `Cmd+=` for Zoom In and `Opt+Cmd+0` for Actual Pixels.
Extras is the one entry where CS6 genuinely uses the Mac **Control** key
alongside Command; on Linux it is plain `Ctrl+H`.

### 7.9 Window and Help

| Linux | macOS | Command | Status |
|---|---|---|---|
| `F5` | `F5` | Brush panel | ⚪ |
| `F6` | `F6` | Color panel | ✅ |
| `F7` | `F7` | Layers panel | ✅ |
| `F8` | `F8` | Info panel | ⚪ |
| `Alt+F9` | `Opt+F9` | Actions panel | ⚪ |
| `F1` | `F1` | Photoshop Help | ⚪ |

macOS-only in CS6: Minimize `Ctrl+Cmd+M`. On macOS, CS6 also binds Help to
`Cmd+/`.

---

## 8. Mouse and canvas gestures

| Gesture | Effect |
|---|---|
| Wheel | Scroll vertically (and horizontally on a tilt wheel) |
| `Shift`+wheel | Scroll horizontally |
| `Ctrl`+wheel | Zoom, anchored on the pixel under the cursor |
| Middle-drag | Pan, from any tool |
| Hold `Space`, drag | Pan, from any tool — the cursor changes to the hand |
| `Alt`+click with Zoom | Zoom out |
| `Esc` | Abandon the stroke or marquee in progress |
| Arrow keys with Move | Nudge the active layer 1 px; `Shift` makes it 10 px |
| `Ctrl+Shift`-drag a selection tool | Add to the selection |
| `Ctrl+Alt`-drag a selection tool | Subtract from the selection |
| `Shift`-drag a selection tool | Add to the selection (CS6's own binding) |
| Drag with Lasso | Trace a freehand outline; release closes it back to the start |
| Click with Polygonal / Magnetic Lasso | Place a fastening point; click the first one again, double-click, or press `Enter` to close |
| Drag with Quick Selection | Grow the selection under the brush; it stops at edges. The ants follow the brush live |
| Click with Magic Wand | Select everything matching the clicked pixel within Tolerance |
| Drag with Crop | Set the crop box; drag its handles to adjust, drag inside to move |
| `Enter` or double-click with Crop | Apply the crop |
| `Esc` with Crop | Reset the box to the whole canvas |
| Drag with Perspective Crop | Drag out a box, then drag each corner onto the subject's corners; drag inside to move the whole quad |
| `Enter` or double-click with Perspective Crop | Straighten the quad into a rectangle and crop to it |
| Drag with Blur | Soften what the brush passes over. Dwelling on one spot goes on softening it |
| Drag with Sharpen | The same, in reverse: exaggerate what the brush passes over |
| Drag with Smudge | Drag the pixels along the stroke, as a finger through wet paint |
| Drag with Dodge / Burn | Lighten or darken what the brush passes over, hardest inside the chosen Range |
| Drag with Sponge | Drain colour toward grey, or lift it away |
| Click with Paint Bucket | Fill the area under the cursor with the foreground colour, out to Tolerance |
| Drag with Gradient | Set the ramp's direction and length. Only the axis line follows the cursor; the gradient is drawn on release, as in CS6 |
| `Shift`-drag with Gradient | Constrain the axis to 45° steps |
| `Alt`+click with Clone Stamp | Set the point to clone from; a crosshair marks it on the canvas |
| Drag with Clone Stamp | Copy the sampled pixels under the brush, verbatim. With no source set the stroke is refused and a dialog says so, as in Photoshop |
| Click with Pen | Place a corner anchor, extending the open subpath (or starting a new one) |
| Drag with Pen | Place a smooth anchor: the drag sets the outgoing handle, and the incoming one mirrors it |
| `Alt`+drag with Pen | Place a smooth anchor whose incoming segment stays whatever it already was — only the segment about to be drawn curves |
| Click the start anchor with Pen | Close the subpath |
| Hover the finished path with Pen (Auto Add/Delete on) | Click a segment to add an anchor there, or an anchor to remove it |
| `Enter`, double-click, or `Esc` with Pen | Stop extending the open subpath, without closing it or discarding its points |
| Drag with Freeform Pen | Draw freehand; released, the drag is fitted to a handful of corner anchors. Dragging back to the start closes the loop |
| Click a segment with Add Anchor Point | Insert an anchor there |
| Click an anchor with Delete Anchor Point | Remove it |
| Click a smooth anchor with Convert Point | Strip its handles, leaving a corner |
| Drag a corner anchor with Convert Point | Pull out a fresh symmetric pair of handles |
| Drag a handle with Convert Point | Break it free of the opposite handle |
| Drag with Path Selection | Move the whole subpath under the cursor |
| Drag an anchor with Direct Selection | Move just that anchor, carrying its handles |
| Drag a handle with Direct Selection | Reshape the curve; a smooth point's opposite handle follows the angle, keeping its own length |
| `Alt`+drag a handle with Direct Selection | Break that handle free of the smooth point permanently |
| `Alt`+click with Healing Brush | Set the source to sample from |
| Drag with Healing Brush | Transplant the source's texture, taking the destination's own lighting. With no source set the stroke is refused and a dialog says so, as in Photoshop |
| Drag with Patch / Content-Aware Move | Outline a region; drag inside it to choose where to sample from, or where to move it |
| Patch, Source mode | The selection is the flaw; the drag says where to repair it from |
| Patch, Destination mode | The selection is good material; the drag says where to apply it |

Patch and Content-Aware Move reconstruct every pixel of the region, so a large
area takes a moment; the cursor changes to a wait cursor while it works.

A **move copies** its pixels rather than re-solving them — only the overall
colour is adapted, by the amount **Color** asks for — so detail survives intact.
The gap left behind is filled in two stages: an inward sweep from the boundary
for a first guess, then search-and-vote passes in which every patch covering a
pixel votes on its colour, weighted by how well that patch fits. The voting is
what keeps the fill smooth; giving each pixel the centre of its own best match
mosaics unrelated sources together and shows as blocks.
| Drag or click with Red Eye | Neutralise red in the area |
| Drag with Slice | Cut a user slice; the rest of the canvas re-slices automatically around it |
| Click with Slice Select | Select a user slice; drag it or its handles to adjust |
| `Del` with Slice Select | Delete the selected slice |
| Click with Color Sampler / Count | Place a marker; drag to move, `Alt`+click to remove |
| Click with Note | Add a note and open its editor; click an existing note to edit it |
| Drag with Ruler | Measure; drag either end to adjust |
| `Backspace` with an open lasso | Take back the last fastening point |
| Click without dragging | Deselect (for Rectangular / Elliptical / Lasso) |
| Right-click with a selection tool | CS6's marquee context menu |

**Unsaved changes are only ever asked about when something would actually be
lost.** File ▸ New and File ▸ Open add a tab, so they never prompt. Closing a
document — its tab's ×, or File ▸ Close — prompts for that document. Quitting
prompts once per modified document, bringing each into view first so the
decision is made while looking at the right image.

The status bar's **zoom field** is editable, as Photoshop's is: type a percentage
and press Enter. It accepts `400`, `400%` and `66,7` alike, clamps out-of-range
values to the nearest limit rather than rejecting them, and puts the real value
back if what was typed makes no sense. It follows zoom changed anywhere else —
the wheel, the View menu, Fit on Screen — but leaves itself alone while being
typed into.

Zoom steps through CS6's own sequence (0.67%, 1%, 1.67%, … 1600%, 3200%) rather
than a smooth ramp. Below 200% the canvas is drawn smoothed; at 200% and above
it switches to nearest-neighbour so individual pixels stay crisp, exactly as
Photoshop does.

The **selection context menu** carries CS6's entries in CS6's grouping:
Deselect · Select Inverse · Feather… · Refine Edge… │ Save Selection… · Make
Work Path… │ Layer Via Copy · Layer Via Cut · New Layer… │ Free Transform ·
Transform Selection │ Fill… · Stroke… │ Last Filter · Fade…. The five that the
engine can do reuse the menu bar's own actions, so they show the same
shortcuts; the rest are listed but disabled, keeping the menu's shape without
pretending to work. It is drawn in the app's dark theme rather than CS6's
native light one, so it matches the rest of the UI.

**Marching ants** trace the selection mask's real 50% coverage contour, so an
elliptical selection reads as an ellipse and a subtracted region shows its
hole. A heavily feathered selection's ants sit inside the visible falloff —
that is correct, and is what Photoshop shows too: the outline marks where the
selection is half strength, not where it stops having an effect.

---

## 9. Remapping shortcuts

Shortcuts are **data, not code** (CLAUDE.md §9). Adding a feature means
registering its command in the registry and binding it in the keymap; a widget
never names a key itself.

Defaults ship in
[shell/resources/shortcuts.json](../shell/resources/shortcuts.json), staged next
to the executable at build time. Overrides are written to:

| Platform | Path |
|---|---|
| Linux | `~/.config/PhotoRust/shortcuts.json` |
| macOS | `~/Library/Application Support/PhotoRust/shortcuts.json` |

Only bindings that **differ from the defaults** are stored, so future changes to
the shipped keymap still reach users who have customised something else.

A binding entry is:

```json
{ "id": "layer.mergeDown", "key": "Ctrl+E", "name": "Merge Down" }
```

`key` uses Qt's portable sequence syntax, and an empty `key` is legitimate — the
command exists and can appear in menus, it just has no default binding.

Two commands must never share a key. Qt treats that as an ambiguous shortcut
and fires **neither** action, so a clash silently disables both. The registry
warns on the console and refuses the later binding rather than letting that
happen.

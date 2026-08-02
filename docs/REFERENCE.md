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
| **Tool strip** | Single column of 20 tools in four groups, then the colour swatch, Quick Mask and screen-mode buttons. |
| **Canvas viewport** | The document, centred, on the CS6 grey surround. Transparent pixels show a fixed-size checkerboard that does not scale with zoom, as in Photoshop. |
| **Dock area** | Right-hand side. Color and Swatches share a tab group; History and Layers stack below. Panels are dockable left or right, and float when dragged out. |
| **Status bar** | Zoom percentage, document size, and the live cursor position in document coordinates. |

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
| Rope loop with tail | Lasso | `L` | Lasso ✅ · Polygonal ⛔ · Magnetic ⛔ | first only |
| Dashed circle + sparkle | Quick Selection | `W` | Quick Selection ✅ · Magic Wand ⛔ | first only |
| Two overlapping corners | Crop | `C` | Crop ✅ · Perspective Crop ⛔ · Slice ⛔ · Slice Select ⛔ | first only |
| Pipette | Eyedropper | `I` | Eyedropper ✅ · Color Sampler ⛔ · Ruler ⛔ · Note ⛔ · Count ⛔ | first only |
| | | | | |
| Angled bandage | Spot Healing Brush | `J` | Spot Healing ✅ · Healing ⛔ · Patch ⛔ · Content-Aware Move ⛔ · Red Eye ⛔ | first only |
| Brush with bristles | **Brush** | `B` | Brush ✅ · Pencil ⛔ · Color Replacement ⛔ · Mixer Brush ⛔ | first only |
| Rubber stamp | Clone Stamp | `S` | Clone Stamp ✅ · Pattern Stamp ⛔ | first only |
| Brush + circular arrow | History Brush | `Y` | History Brush ✅ · Art History Brush ⛔ | first only |
| Angled eraser block | Eraser | `E` | Eraser ✅ · Background Eraser ⛔ · Magic Eraser ⛔ | first only |
| Rectangle fading out | Gradient | `G` | Gradient ✅ · Paint Bucket ⛔ | first only |
| Waterdrop | Blur | — | Blur ✅ · Sharpen ⛔ · Smudge ⛔ | first only |
| Circle with handle | Dodge | `O` | Dodge ✅ · Burn ⛔ · Sponge ⛔ | first only |
| | | | | |
| Nib with anchor point | Pen | `P` | Pen ✅ · Freeform Pen ⛔ · Add / Delete Anchor ⛔ · Convert Point ⛔ | ⛔ |
| Serif **T** | Horizontal Type | `T` | Horizontal ✅ · Vertical ⛔ · Horizontal Mask ⛔ · Vertical Mask ⛔ | ⛔ |
| Solid arrow pointer | Path Selection | `A` | Path Selection ✅ · Direct Selection ⛔ | ⛔ |
| Filled rectangle | Rectangle | `U` | Rectangle ✅ · Rounded Rectangle ⛔ · Ellipse ⛔ · Polygon ⛔ · Line ⛔ · Custom Shape ⛔ | ⛔ |
| | | | | |
| Open hand | Hand | `H` | Hand ✅ · Rotate View ⛔ | ✅ |
| Magnifier with **+** | Zoom | `Z` | — | ✅ |

**Works** describes the engine behind the tool, which is not the same as the
flyout being populated. Pen, Type, Path Selection and Shape are present in the
strip with correct icons and shortcuts, but selecting them does nothing on the
canvas yet.

The Marquee is the only group where more than the first entry is implemented.

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
| Brush, Eraser, Spot Healing, Clone Stamp, History Brush | **Size** (1–5000 px), **Hardness** (0–100%), **Opacity** (0–100%), **Flow** (1–100%) — all live, pushed to the engine on change |
| Marquee, Lasso, Quick Selection | Modifier hint: `Ctrl+Shift` = add · `Ctrl+Alt` = subtract · click = deselect |
| Marquee (Single Row / Single Column) | Hint changes to: click to select a line · `Ctrl+Shift` = add · `Ctrl+Alt` = subtract |
| Zoom | Hint: click to zoom in · `Alt`+click to zoom out |
| Move | Hint: drag to move the active layer · arrow keys nudge |
| Anything else | Name only |

CS6's selection **mode buttons** (new / add / subtract / intersect) are not
drawn yet — the operations are reachable through the modifier keys only. Brush
presets, blend mode and airbrush are likewise absent from the bar.

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

### Layers · `F7`

Top-first, as in Photoshop, with a thumbnail per layer.

- Show/hide, reorder, rename (double-click), duplicate, delete
- **Opacity** and **Fill opacity**
- All **27 blend modes**, in CS6's grouped order with the separators
- Clipping masks and layer masks
- Merge Down, Flatten Image

Not implemented: layer groups (folders), layer effects/styles, adjustment-layer
parameter editing after creation, and the Channels / Paths / Navigator panels.

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
| `W` | Quick Selection ✅ | | `O` | Dodge ✅ |
| `C` | Crop ✅ | | `P` | Pen ⚪ |
| `I` | Eyedropper ✅ | | `T` | Horizontal Type ⚪ |
| `J` | Spot Healing Brush ✅ | | `A` | Path Selection ⚪ |
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
| `Ctrl+W` | `Cmd+W` | Close | ⚪ |
| `Alt+Ctrl+W` | `Opt+Cmd+W` | Close All | ⚪ |
| `Ctrl+S` | `Cmd+S` | Save | ✅ |
| `Shift+Ctrl+S` | `Shift+Cmd+S` | Save As… | ✅ |
| `Alt+Shift+Ctrl+S` | `Opt+Shift+Cmd+S` | Save for Web… | ⚪ |
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
| Click without dragging | Deselect (for Rectangular / Elliptical) |

Zoom steps through CS6's own sequence (0.67%, 1%, 1.67%, … 1600%, 3200%) rather
than a smooth ramp. Below 200% the canvas is drawn smoothed; at 200% and above
it switches to nearest-neighbour so individual pixels stay crisp, exactly as
Photoshop does.

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

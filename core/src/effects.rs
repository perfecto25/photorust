//! Layer effects — Photoshop's Layer Style.
//!
//! Every effect here is derived from one thing: the layer's **alpha**. Blur it
//! and offset it and you have a drop shadow; blur its inverse and confine that
//! to the layer and you have an inner shadow; paint a colour through it and you
//! have an overlay; take the band just outside its edge and you have a stroke.
//! That is why they share one renderer rather than having one each.
//!
//! Effects draw *outside* the layer they belong to — a shadow reaches past
//! every edge — so they are rendered into canvas-space buffers and handed to
//! the compositor, which draws them under and over the layer's own pixels.
//!
//! **Not a GPU candidate yet** (CLAUDE.md §7 step 2). The expensive part is the
//! softening, and that is a box blur here rather than a Gaussian: three passes
//! with a running sum, which cost the same at radius 80 as at radius 5 and are
//! already fast enough to drag a slider against. The rest is a handful of
//! passes over one layer's bounding box.

use crate::blend::BlendMode;
use crate::buffer::{Pixmap, Rect, Rgba8};
use crate::gradient::{Gradient, GradientType};
use crate::layer::Layer;
use crate::pattern;

/// Where a stroke sits relative to the layer's edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum StrokePosition {
    #[default]
    Outside,
    Inside,
    Center,
}

impl StrokePosition {
    pub fn from_f32(v: f32) -> StrokePosition {
        match v.round() as i32 {
            1 => StrokePosition::Inside,
            2 => StrokePosition::Center,
            _ => StrokePosition::Outside,
        }
    }

    pub fn as_f32(self) -> f32 {
        match self {
            StrokePosition::Outside => 0.0,
            StrokePosition::Inside => 1.0,
            StrokePosition::Center => 2.0,
        }
    }
}

/// Drop Shadow and Inner Shadow, which differ only in which side of the edge
/// they fall on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowEffect {
    pub enabled: bool,
    /// Whether the effect is on the layer at all. An effect switched *off*
    /// keeps its row in the Layers panel with its eye closed, so this outlives
    /// `enabled` and is cleared only by Clear Layer Style.
    pub present: bool,
    pub blend_mode: BlendMode,
    pub color: Rgba8,
    pub opacity: f32,
    /// Direction the light comes *from*, in degrees, as CS6 states it.
    pub angle: f32,
    pub distance: f32,
    /// Pushes the shadow's edge outward (choke, for an inner shadow) before it
    /// is softened, `0.0..=1.0`.
    pub spread: f32,
    /// Blur radius in pixels — CS6 calls it Size.
    pub size: f32,
}

impl Default for ShadowEffect {
    fn default() -> Self {
        // CS6's own defaults, so a style switched on without touching anything
        // looks like Photoshop's.
        Self {
            enabled: false,
            present: false,
            blend_mode: BlendMode::Multiply,
            color: Rgba8::BLACK,
            opacity: 0.75,
            angle: 120.0,
            distance: 5.0,
            spread: 0.0,
            size: 5.0,
        }
    }
}

/// Outer Glow and Inner Glow: a shadow with no offset and a lighter default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlowEffect {
    pub enabled: bool,
    /// See [`ShadowEffect::present`].
    pub present: bool,
    pub blend_mode: BlendMode,
    pub color: Rgba8,
    pub opacity: f32,
    pub spread: f32,
    pub size: f32,
}

impl Default for GlowEffect {
    fn default() -> Self {
        Self {
            enabled: false,
            present: false,
            blend_mode: BlendMode::Screen,
            // CS6's pale yellow.
            color: Rgba8::opaque(255, 255, 190),
            opacity: 0.75,
            spread: 0.0,
            size: 5.0,
        }
    }
}

/// Color Overlay: one colour poured through the layer's alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorOverlayEffect {
    pub enabled: bool,
    /// See [`ShadowEffect::present`].
    pub present: bool,
    pub blend_mode: BlendMode,
    pub color: Rgba8,
    pub opacity: f32,
}

impl Default for ColorOverlayEffect {
    fn default() -> Self {
        Self {
            enabled: false,
            present: false,
            blend_mode: BlendMode::Normal,
            color: Rgba8::opaque(255, 0, 0),
            opacity: 1.0,
        }
    }
}

/// Gradient Overlay. Two stops rather than a full gradient editor, which is
/// the part of CS6's panel this does not yet reproduce.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientOverlayEffect {
    pub enabled: bool,
    /// See [`ShadowEffect::present`].
    pub present: bool,
    pub blend_mode: BlendMode,
    pub opacity: f32,
    pub from: Rgba8,
    pub to: Rgba8,
    /// Degrees, measured the same way as a shadow's angle.
    pub angle: f32,
    pub reverse: bool,
    /// Which of CS6's five ramp shapes to lay down. The same five the Gradient
    /// tool draws, from the same code.
    pub shape: GradientType,
    /// How far the ramp is stretched, 1.0 being CS6's 100%.
    pub scale: f32,
    /// Break up the banding a smooth ramp shows in a large flat area.
    pub dither: bool,
    /// Span the layer's own content rather than the whole canvas — CS6's
    /// "Align with Layer".
    pub align_with_layer: bool,
}

impl Default for GradientOverlayEffect {
    fn default() -> Self {
        Self {
            enabled: false,
            present: false,
            blend_mode: BlendMode::Normal,
            opacity: 1.0,
            from: Rgba8::BLACK,
            to: Rgba8::WHITE,
            angle: 90.0,
            reverse: false,
            shape: GradientType::Linear,
            scale: 1.0,
            dither: false,
            align_with_layer: true,
        }
    }
}

/// Pattern Overlay: a tile repeated across the layer.
///
/// The tiles are the engine's own generated set — see [`crate::pattern`] — not
/// Photoshop's artwork, which is Adobe's to ship.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatternOverlayEffect {
    pub enabled: bool,
    /// See [`ShadowEffect::present`].
    pub present: bool,
    pub blend_mode: BlendMode,
    pub opacity: f32,
    /// Index into [`crate::pattern::PATTERN_NAMES`].
    pub pattern: u32,
    /// 1.0 is CS6's 100%.
    pub scale: f32,
    /// Anchor the tiling to the layer rather than to the canvas, so moving the
    /// layer takes its pattern with it — CS6's "Link with Layer".
    pub link_with_layer: bool,
}

impl Default for PatternOverlayEffect {
    fn default() -> Self {
        Self {
            enabled: false,
            present: false,
            blend_mode: BlendMode::Normal,
            opacity: 1.0,
            pattern: 0,
            scale: 1.0,
            link_with_layer: true,
        }
    }
}

/// Stroke: the band along the layer's edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokeEffect {
    pub enabled: bool,
    /// See [`ShadowEffect::present`].
    pub present: bool,
    pub blend_mode: BlendMode,
    pub color: Rgba8,
    pub opacity: f32,
    pub size: f32,
    pub position: StrokePosition,
}

impl Default for StrokeEffect {
    fn default() -> Self {
        Self {
            enabled: false,
            present: false,
            blend_mode: BlendMode::Normal,
            // CS6 opens with red, which is deliberate on its part: a stroke
            // nobody can see is a control nobody can learn.
            color: Rgba8::opaque(255, 0, 0),
            opacity: 1.0,
            size: 3.0,
            // CS6 defaults to Outside, which suits the shape and type layers
            // it expects. Here the common layer is a photograph filling the
            // canvas, and an outside stroke on that falls off the edge and
            // shows nothing at all — so this one starts where it can be seen.
            position: StrokePosition::Inside,
        }
    }
}

/// Which of CS6's five bevels to build, and where it sits about the edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BevelStyle {
    OuterBevel,
    #[default]
    InnerBevel,
    Emboss,
    PillowEmboss,
    /// The bevel rides the Stroke effect's band, so it needs one to ride.
    StrokeEmboss,
}

impl BevelStyle {
    pub fn from_f32(v: f32) -> BevelStyle {
        match v.round() as i32 {
            0 => BevelStyle::OuterBevel,
            2 => BevelStyle::Emboss,
            3 => BevelStyle::PillowEmboss,
            4 => BevelStyle::StrokeEmboss,
            _ => BevelStyle::InnerBevel,
        }
    }

    pub fn as_f32(self) -> f32 {
        match self {
            BevelStyle::OuterBevel => 0.0,
            BevelStyle::InnerBevel => 1.0,
            BevelStyle::Emboss => 2.0,
            BevelStyle::PillowEmboss => 3.0,
            BevelStyle::StrokeEmboss => 4.0,
        }
    }
}

/// How hard the bevel's shoulder is.
///
/// Smooth is the Gaussian shoulder this renderer builds naturally. The two
/// chisels are **approximations**: Photoshop cuts them from a distance
/// transform, which gives a flat facet and a crisp arris, where these are the
/// same Gaussian pulled tighter. Close in spirit, not identical.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BevelTechnique {
    #[default]
    Smooth,
    ChiselHard,
    ChiselSoft,
}

impl BevelTechnique {
    pub fn from_f32(v: f32) -> BevelTechnique {
        match v.round() as i32 {
            1 => BevelTechnique::ChiselHard,
            2 => BevelTechnique::ChiselSoft,
            _ => BevelTechnique::Smooth,
        }
    }

    pub fn as_f32(self) -> f32 {
        match self {
            BevelTechnique::Smooth => 0.0,
            BevelTechnique::ChiselHard => 1.0,
            BevelTechnique::ChiselSoft => 2.0,
        }
    }

    /// How wide a shoulder to build for a given Size, as a fraction of it.
    fn shoulder(self) -> f32 {
        match self {
            BevelTechnique::Smooth => 1.0,
            BevelTechnique::ChiselHard => 0.35,
            BevelTechnique::ChiselSoft => 0.6,
        }
    }
}

/// Bevel & Emboss: the layer's edge lit as though it had thickness.
///
/// Unlike every other effect here, this one is not the alpha painted through
/// something — it is the alpha treated as a **height map**, differentiated into
/// surface normals and lit. That is why it carries two colours and two blend
/// modes: the lit side and the unlit side are separate effects that happen to
/// be computed together.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BevelEffect {
    pub enabled: bool,
    /// See [`ShadowEffect::present`].
    pub present: bool,
    pub style: BevelStyle,
    pub technique: BevelTechnique,
    /// How pronounced the slope is. 1.0 is CS6's 100%.
    pub depth: f32,
    /// False turns the bevel into a hollow — CS6's Direction: Down.
    pub up: bool,
    /// Width of the shoulder, in pixels.
    pub size: f32,
    /// A blur over the finished shading, in pixels.
    pub soften: f32,
    /// Where the light comes from, in degrees, as for a shadow.
    pub angle: f32,
    /// How high above the surface it sits, in degrees. At 90° it is overhead
    /// and the bevel flattens out.
    pub altitude: f32,
    pub highlight_mode: BlendMode,
    pub highlight_color: Rgba8,
    pub highlight_opacity: f32,
    pub shadow_mode: BlendMode,
    pub shadow_color: Rgba8,
    pub shadow_opacity: f32,
}

impl Default for BevelEffect {
    fn default() -> Self {
        // CS6's defaults.
        Self {
            enabled: false,
            present: false,
            style: BevelStyle::InnerBevel,
            technique: BevelTechnique::Smooth,
            depth: 1.0,
            up: true,
            size: 5.0,
            soften: 0.0,
            angle: 120.0,
            altitude: 30.0,
            highlight_mode: BlendMode::Screen,
            highlight_color: Rgba8::WHITE,
            highlight_opacity: 0.75,
            shadow_mode: BlendMode::Multiply,
            shadow_color: Rgba8::BLACK,
            shadow_opacity: 0.75,
        }
    }
}

/// Satin: the layer's own shape folded back over itself.
///
/// Two copies of the alpha, pushed apart along the angle and softened, then
/// differenced. Where they agree the result is empty; where they disagree it is
/// bright, which is what draws the folded, cloth-like bands the effect is named
/// for. Confined to the layer, so it reads as a sheen on the shape rather than
/// anything around it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SatinEffect {
    pub enabled: bool,
    /// See [`ShadowEffect::present`].
    pub present: bool,
    pub blend_mode: BlendMode,
    pub color: Rgba8,
    pub opacity: f32,
    pub angle: f32,
    pub distance: f32,
    pub size: f32,
    /// Swap the bands for the gaps between them.
    pub invert: bool,
}

impl Default for SatinEffect {
    fn default() -> Self {
        // CS6's defaults, Invert included — without it the effect reads as a
        // dark blob rather than a sheen.
        Self {
            enabled: false,
            present: false,
            blend_mode: BlendMode::Multiply,
            color: Rgba8::BLACK,
            opacity: 0.5,
            angle: 19.0,
            distance: 11.0,
            size: 14.0,
            invert: true,
        }
    }
}

/// Every effect on one layer.
///
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct LayerEffects {
    pub bevel: BevelEffect,
    pub satin: SatinEffect,
    pub drop_shadow: ShadowEffect,
    pub inner_shadow: ShadowEffect,
    pub outer_glow: GlowEffect,
    pub inner_glow: GlowEffect,
    pub color_overlay: ColorOverlayEffect,
    pub gradient_overlay: GradientOverlayEffect,
    pub pattern_overlay: PatternOverlayEffect,
    pub stroke: StrokeEffect,
    /// CS6's "Hide All Effects": the settings stay, they simply stop drawing.
    pub hidden: bool,
}

impl LayerEffects {
    /// Whether anything would be drawn.
    pub fn any_enabled(&self) -> bool {
        !self.hidden
            && (self.bevel.enabled
                || self.satin.enabled
                || self.drop_shadow.enabled
                || self.inner_shadow.enabled
                || self.outer_glow.enabled
                || self.inner_glow.enabled
                || self.color_overlay.enabled
                || self.gradient_overlay.enabled
                || self.pattern_overlay.enabled
                || self.stroke.enabled)
    }

    /// Whether the layer carries a style at all — what puts the `fx` on its
    /// row and decides if there is something to copy or clear.
    ///
    /// An effect the user has switched off still counts: it keeps its row in
    /// the Layers panel with the eye closed, exactly as in CS6, and only Clear
    /// Layer Style takes it away.
    pub fn any_present(&self) -> bool {
        self.bevel.present
            || self.satin.present
            || self.drop_shadow.present
            || self.inner_shadow.present
            || self.outer_glow.present
            || self.inner_glow.present
            || self.color_overlay.present
            || self.gradient_overlay.present
            || self.pattern_overlay.present
            || self.stroke.present
    }

    /// How much room around the region the renderer needs, in pixels.
    ///
    /// Two different things want this margin. The outward effects *draw* that
    /// far past the layer's edge. The inward ones are built from the layer's
    /// **inverse** — the hole it sits in — and have to be able to *read* that
    /// far outside it: an inner shadow at distance 65 pulls the hole 65 pixels
    /// into the layer, and with a narrower margin it would pull in nothing and
    /// draw nothing.
    pub fn extent(&self) -> i32 {
        // A Gaussian of radius r is cut off at three sigma by
        // `filters::convolve`, so that — not r — is how far a softened edge
        // actually reaches. Padding by less makes a region render disagree
        // with a whole-canvas one along the seam.
        const BLUR_REACH: f32 = 3.0;

        let mut reach = 0.0f32;
        if self.drop_shadow.enabled {
            reach = reach
                .max(self.drop_shadow.size * BLUR_REACH + self.drop_shadow.distance.abs());
        }
        if self.outer_glow.enabled {
            reach = reach.max(self.outer_glow.size * BLUR_REACH);
        }
        if self.stroke.enabled {
            // Inside or out: an inside stroke reads the inverse just as far as
            // an outside one draws.
            reach = reach.max(self.stroke.size * BLUR_REACH);
        }
        if self.inner_shadow.enabled {
            reach = reach
                .max(self.inner_shadow.size * BLUR_REACH + self.inner_shadow.distance.abs());
        }
        if self.inner_glow.enabled {
            reach = reach.max(self.inner_glow.size * BLUR_REACH);
        }
        if self.satin.enabled {
            // Confined to the layer, but built from copies of it dragged in
            // from outside.
            reach = reach.max(self.satin.size * BLUR_REACH + self.satin.distance.abs());
        }
        if self.bevel.enabled {
            // The shoulder is built from a blur of the alpha and then softened,
            // and an outer bevel lies wholly outside the layer. The blur's own
            // radius is a fraction of Size — see `render_bevel`.
            reach = reach.max(self.bevel.size + self.bevel.soften * BLUR_REACH);
        }
        reach.ceil().max(0.0) as i32 + 2
    }
}

// -- the key/value view ------------------------------------------------------
//
// The Layer Style dialog has some forty controls across seven effects. Giving
// each its own bridge call would be forty calls; sending the whole struct
// across would mean a second parser in C++ that could drift from this one. So
// the shell reads and writes one named number at a time — "dropShadow.size" —
// and this is the only place that knows what the names mean.
//
// Colours pack into a float exactly: 0xFFFFFF is 16777215, well inside f32's
// 24-bit integer range.

fn pack_color(c: Rgba8) -> f32 {
    (((c.r as u32) << 16) | ((c.g as u32) << 8) | c.b as u32) as f32
}

fn unpack_color(v: f32) -> Rgba8 {
    let packed = v.round().clamp(0.0, 16_777_215.0) as u32;
    Rgba8::opaque(
        ((packed >> 16) & 0xff) as u8,
        ((packed >> 8) & 0xff) as u8,
        (packed & 0xff) as u8,
    )
}

fn as_bool(v: f32) -> bool {
    v >= 0.5
}

fn from_bool(b: bool) -> f32 {
    if b {
        1.0
    } else {
        0.0
    }
}

impl LayerEffects {
    /// One setting, by name. Zero for a name that means nothing.
    pub fn value(&self, key: &str) -> f32 {
        let (effect, field) = match key.split_once('.') {
            Some(parts) => parts,
            None => return if key == "hidden" { from_bool(self.hidden) } else { 0.0 },
        };
        match effect {
            "bevel" => bevel_value(&self.bevel, field),
            "satin" => match field {
                "on" => from_bool(self.satin.enabled),
                "present" => from_bool(self.satin.present),
                "mode" => self.satin.blend_mode as i32 as f32,
                "color" => pack_color(self.satin.color),
                "opacity" => self.satin.opacity,
                "angle" => self.satin.angle,
                "distance" => self.satin.distance,
                "size" => self.satin.size,
                "invert" => from_bool(self.satin.invert),
                _ => 0.0,
            },
            "dropShadow" => shadow_value(&self.drop_shadow, field),
            "innerShadow" => shadow_value(&self.inner_shadow, field),
            "outerGlow" => glow_value(&self.outer_glow, field),
            "innerGlow" => glow_value(&self.inner_glow, field),
            "colorOverlay" => match field {
                "on" => from_bool(self.color_overlay.enabled),
                "present" => from_bool(self.color_overlay.present),
                "mode" => self.color_overlay.blend_mode as i32 as f32,
                "color" => pack_color(self.color_overlay.color),
                "opacity" => self.color_overlay.opacity,
                _ => 0.0,
            },
            "gradientOverlay" => match field {
                "on" => from_bool(self.gradient_overlay.enabled),
                "present" => from_bool(self.gradient_overlay.present),
                "mode" => self.gradient_overlay.blend_mode as i32 as f32,
                "opacity" => self.gradient_overlay.opacity,
                "from" => pack_color(self.gradient_overlay.from),
                "to" => pack_color(self.gradient_overlay.to),
                "angle" => self.gradient_overlay.angle,
                "reverse" => from_bool(self.gradient_overlay.reverse),
                "shape" => self.gradient_overlay.shape as i32 as f32,
                "scale" => self.gradient_overlay.scale,
                "dither" => from_bool(self.gradient_overlay.dither),
                "align" => from_bool(self.gradient_overlay.align_with_layer),
                _ => 0.0,
            },
            "patternOverlay" => match field {
                "on" => from_bool(self.pattern_overlay.enabled),
                "present" => from_bool(self.pattern_overlay.present),
                "mode" => self.pattern_overlay.blend_mode as i32 as f32,
                "opacity" => self.pattern_overlay.opacity,
                "pattern" => self.pattern_overlay.pattern as f32,
                "scale" => self.pattern_overlay.scale,
                "link" => from_bool(self.pattern_overlay.link_with_layer),
                _ => 0.0,
            },
            "stroke" => match field {
                "on" => from_bool(self.stroke.enabled),
                "present" => from_bool(self.stroke.present),
                "mode" => self.stroke.blend_mode as i32 as f32,
                "color" => pack_color(self.stroke.color),
                "opacity" => self.stroke.opacity,
                "size" => self.stroke.size,
                "position" => self.stroke.position.as_f32(),
                _ => 0.0,
            },
            _ => 0.0,
        }
    }

    /// Set one setting by name. False for a name that means nothing, so a
    /// typo in the shell shows up rather than being swallowed.
    pub fn set_value(&mut self, key: &str, value: f32) -> bool {
        let (effect, field) = match key.split_once('.') {
            Some(parts) => parts,
            None => {
                if key == "hidden" {
                    self.hidden = as_bool(value);
                    return true;
                }
                return false;
            }
        };
        match effect {
            "bevel" => set_bevel_value(&mut self.bevel, field, value),
            "satin" => match field {
                "on" => {
                    self.satin.enabled = as_bool(value);
                    self.satin.present |= self.satin.enabled;
                    true
                }
                "present" => {
                    self.satin.present = as_bool(value);
                    true
                }
                "mode" => {
                    self.satin.blend_mode = BlendMode::from_i32(value.round() as i32);
                    true
                }
                "color" => {
                    self.satin.color = unpack_color(value);
                    true
                }
                "opacity" => {
                    self.satin.opacity = value.clamp(0.0, 1.0);
                    true
                }
                "angle" => {
                    self.satin.angle = value;
                    true
                }
                "distance" => {
                    self.satin.distance = value.max(0.0);
                    true
                }
                "size" => {
                    self.satin.size = value.max(0.0);
                    true
                }
                "invert" => {
                    self.satin.invert = as_bool(value);
                    true
                }
                _ => false,
            },
            "dropShadow" => set_shadow_value(&mut self.drop_shadow, field, value),
            "innerShadow" => set_shadow_value(&mut self.inner_shadow, field, value),
            "outerGlow" => set_glow_value(&mut self.outer_glow, field, value),
            "innerGlow" => set_glow_value(&mut self.inner_glow, field, value),
            "colorOverlay" => match field {
                "on" => {
                    self.color_overlay.enabled = as_bool(value);
                    self.color_overlay.present |= self.color_overlay.enabled;
                    true
                }
                "present" => {
                    self.color_overlay.present = as_bool(value);
                    true
                }
                "mode" => {
                    self.color_overlay.blend_mode = BlendMode::from_i32(value.round() as i32);
                    true
                }
                "color" => {
                    self.color_overlay.color = unpack_color(value);
                    true
                }
                "opacity" => {
                    self.color_overlay.opacity = value.clamp(0.0, 1.0);
                    true
                }
                _ => false,
            },
            "gradientOverlay" => match field {
                "on" => {
                    self.gradient_overlay.enabled = as_bool(value);
                    self.gradient_overlay.present |= self.gradient_overlay.enabled;
                    true
                }
                "present" => {
                    self.gradient_overlay.present = as_bool(value);
                    true
                }
                "mode" => {
                    self.gradient_overlay.blend_mode = BlendMode::from_i32(value.round() as i32);
                    true
                }
                "opacity" => {
                    self.gradient_overlay.opacity = value.clamp(0.0, 1.0);
                    true
                }
                "from" => {
                    self.gradient_overlay.from = unpack_color(value);
                    true
                }
                "to" => {
                    self.gradient_overlay.to = unpack_color(value);
                    true
                }
                "angle" => {
                    self.gradient_overlay.angle = value;
                    true
                }
                "reverse" => {
                    self.gradient_overlay.reverse = as_bool(value);
                    true
                }
                "shape" => {
                    self.gradient_overlay.shape = GradientType::from_i32(value.round() as i32);
                    true
                }
                "scale" => {
                    self.gradient_overlay.scale = value.clamp(0.01, 10.0);
                    true
                }
                "dither" => {
                    self.gradient_overlay.dither = as_bool(value);
                    true
                }
                "align" => {
                    self.gradient_overlay.align_with_layer = as_bool(value);
                    true
                }
                _ => false,
            },
            "patternOverlay" => match field {
                "on" => {
                    self.pattern_overlay.enabled = as_bool(value);
                    self.pattern_overlay.present |= self.pattern_overlay.enabled;
                    true
                }
                "present" => {
                    self.pattern_overlay.present = as_bool(value);
                    true
                }
                "mode" => {
                    self.pattern_overlay.blend_mode = BlendMode::from_i32(value.round() as i32);
                    true
                }
                "opacity" => {
                    self.pattern_overlay.opacity = value.clamp(0.0, 1.0);
                    true
                }
                "pattern" => {
                    self.pattern_overlay.pattern = value.max(0.0).round() as u32;
                    true
                }
                "scale" => {
                    self.pattern_overlay.scale = value.clamp(0.1, 10.0);
                    true
                }
                "link" => {
                    self.pattern_overlay.link_with_layer = as_bool(value);
                    true
                }
                _ => false,
            },
            "stroke" => match field {
                "on" => {
                    self.stroke.enabled = as_bool(value);
                    self.stroke.present |= self.stroke.enabled;
                    true
                }
                "present" => {
                    self.stroke.present = as_bool(value);
                    true
                }
                "mode" => {
                    self.stroke.blend_mode = BlendMode::from_i32(value.round() as i32);
                    true
                }
                "color" => {
                    self.stroke.color = unpack_color(value);
                    true
                }
                "opacity" => {
                    self.stroke.opacity = value.clamp(0.0, 1.0);
                    true
                }
                "size" => {
                    self.stroke.size = value.max(0.0);
                    true
                }
                "position" => {
                    self.stroke.position = StrokePosition::from_f32(value);
                    true
                }
                _ => false,
            },
            _ => false,
        }
    }
}

fn bevel_value(fx: &BevelEffect, field: &str) -> f32 {
    match field {
        "on" => from_bool(fx.enabled),
        "present" => from_bool(fx.present),
        "style" => fx.style.as_f32(),
        "technique" => fx.technique.as_f32(),
        "depth" => fx.depth,
        "up" => from_bool(fx.up),
        "size" => fx.size,
        "soften" => fx.soften,
        "angle" => fx.angle,
        "altitude" => fx.altitude,
        "highlightMode" => fx.highlight_mode as i32 as f32,
        "highlightColor" => pack_color(fx.highlight_color),
        "highlightOpacity" => fx.highlight_opacity,
        "shadowMode" => fx.shadow_mode as i32 as f32,
        "shadowColor" => pack_color(fx.shadow_color),
        "shadowOpacity" => fx.shadow_opacity,
        _ => 0.0,
    }
}

fn set_bevel_value(fx: &mut BevelEffect, field: &str, value: f32) -> bool {
    match field {
        "on" => {
            fx.enabled = as_bool(value);
            fx.present |= fx.enabled;
        }
        "present" => fx.present = as_bool(value),
        "style" => fx.style = BevelStyle::from_f32(value),
        "technique" => fx.technique = BevelTechnique::from_f32(value),
        "depth" => fx.depth = value.clamp(0.0, 10.0),
        "up" => fx.up = as_bool(value),
        "size" => fx.size = value.max(0.0),
        "soften" => fx.soften = value.max(0.0),
        "angle" => fx.angle = value,
        "altitude" => fx.altitude = value.clamp(0.0, 90.0),
        "highlightMode" => fx.highlight_mode = BlendMode::from_i32(value.round() as i32),
        "highlightColor" => fx.highlight_color = unpack_color(value),
        "highlightOpacity" => fx.highlight_opacity = value.clamp(0.0, 1.0),
        "shadowMode" => fx.shadow_mode = BlendMode::from_i32(value.round() as i32),
        "shadowColor" => fx.shadow_color = unpack_color(value),
        "shadowOpacity" => fx.shadow_opacity = value.clamp(0.0, 1.0),
        _ => return false,
    }
    true
}

fn shadow_value(fx: &ShadowEffect, field: &str) -> f32 {
    match field {
        "on" => from_bool(fx.enabled),
        "present" => from_bool(fx.present),
        "mode" => fx.blend_mode as i32 as f32,
        "color" => pack_color(fx.color),
        "opacity" => fx.opacity,
        "angle" => fx.angle,
        "distance" => fx.distance,
        "spread" => fx.spread,
        "size" => fx.size,
        _ => 0.0,
    }
}

fn set_shadow_value(fx: &mut ShadowEffect, field: &str, value: f32) -> bool {
    match field {
        "on" => {
            fx.enabled = as_bool(value);
            // Switching an effect on is what puts it on the layer; switching
            // it off again leaves it there, closed-eyed.
            fx.present |= fx.enabled;
        }
        "present" => fx.present = as_bool(value),
        "mode" => fx.blend_mode = BlendMode::from_i32(value.round() as i32),
        "color" => fx.color = unpack_color(value),
        "opacity" => fx.opacity = value.clamp(0.0, 1.0),
        "angle" => fx.angle = value,
        "distance" => fx.distance = value.max(0.0),
        "spread" => fx.spread = value.clamp(0.0, 1.0),
        "size" => fx.size = value.max(0.0),
        _ => return false,
    }
    true
}

fn glow_value(fx: &GlowEffect, field: &str) -> f32 {
    match field {
        "on" => from_bool(fx.enabled),
        "present" => from_bool(fx.present),
        "mode" => fx.blend_mode as i32 as f32,
        "color" => pack_color(fx.color),
        "opacity" => fx.opacity,
        "spread" => fx.spread,
        "size" => fx.size,
        _ => 0.0,
    }
}

fn set_glow_value(fx: &mut GlowEffect, field: &str, value: f32) -> bool {
    match field {
        "on" => {
            fx.enabled = as_bool(value);
            fx.present |= fx.enabled;
        }
        "present" => fx.present = as_bool(value),
        "mode" => fx.blend_mode = BlendMode::from_i32(value.round() as i32),
        "color" => fx.color = unpack_color(value),
        "opacity" => fx.opacity = value.clamp(0.0, 1.0),
        "spread" => fx.spread = value.clamp(0.0, 1.0),
        "size" => fx.size = value.max(0.0),
        _ => return false,
    }
    true
}

/// One effect, rendered into canvas space and ready to composite.
pub struct RenderedEffect {
    /// Straight-alpha pixels in canvas coordinates.
    pub pixels: Pixmap,
    pub blend_mode: BlendMode,
    /// Whether it draws over the layer's own pixels or under them.
    pub above: bool,
}

/// Render a layer's effects over `region`.
///
/// The returned buffers are canvas-sized but only meaningful inside `region`:
/// callers composite a region at a time, and blurring the whole canvas to paint
/// a corner of it would be waste. Ordered back to front.
pub fn render(layer: &Layer, width: u32, height: u32, region: Rect) -> Vec<RenderedEffect> {
    let fx = &layer.effects;
    if !fx.any_enabled() || width == 0 || height == 0 {
        return Vec::new();
    }

    // The work area is the region plus everything an effect can drag into it
    // from outside — a shadow cast from off-region, most of all.
    let work = region.inflate(fx.extent().max(0) as u32);
    if work.is_empty() {
        return Vec::new();
    }

    let alpha = alpha_map(layer, work);
    if alpha.iter().all(|&a| a <= 0.0) {
        return Vec::new();
    }

    let mut out = Vec::new();

    // --- under the layer --------------------------------------------------
    if fx.drop_shadow.enabled {
        let (dx, dy) = offset_for(fx.drop_shadow.angle, fx.drop_shadow.distance);
        let mut silhouette = shift(&alpha, work, dx, dy);
        spread_and_blur(&mut silhouette, work, fx.drop_shadow.spread, fx.drop_shadow.size);
        out.push(RenderedEffect {
            pixels: paint(
                &silhouette,
                work,
                region,
                width,
                height,
                fx.drop_shadow.color,
                fx.drop_shadow.opacity,
            ),
            blend_mode: fx.drop_shadow.blend_mode,
            above: false,
        });
    }

    if fx.outer_glow.enabled {
        let mut halo = alpha.clone();
        spread_and_blur(&mut halo, work, fx.outer_glow.spread, fx.outer_glow.size);
        out.push(RenderedEffect {
            pixels: paint(
                &halo,
                work,
                region,
                width,
                height,
                fx.outer_glow.color,
                fx.outer_glow.opacity,
            ),
            blend_mode: fx.outer_glow.blend_mode,
            above: false,
        });
    }

    // --- over the layer ---------------------------------------------------
    // Bevel & Emboss goes on before the overlays, which is where CS6 stacks
    // it, so a Color Overlay can flatten it the way it does in Photoshop.
    if fx.bevel.enabled {
        // Stroke Emboss rides the Stroke's band, so that band has to be built
        // first — and without a stroke there is nothing to ride.
        let band = if fx.bevel.style == BevelStyle::StrokeEmboss && fx.stroke.enabled {
            Some(stroke_band(&alpha, work, fx.stroke.size, fx.stroke.position))
        } else {
            None
        };
        render_bevel(
            &alpha,
            work,
            region,
            width,
            height,
            &fx.bevel,
            band.as_deref(),
            &mut out,
        );
    }

    if fx.satin.enabled {
        out.push(RenderedEffect {
            pixels: satin(&alpha, work, region, width, height, &fx.satin),
            blend_mode: fx.satin.blend_mode,
            above: true,
        });
    }

    if fx.color_overlay.enabled {
        out.push(RenderedEffect {
            pixels: paint(
                &alpha,
                work,
                region,
                width,
                height,
                fx.color_overlay.color,
                fx.color_overlay.opacity,
            ),
            blend_mode: fx.color_overlay.blend_mode,
            above: true,
        });
    }

    if fx.gradient_overlay.enabled {
        out.push(RenderedEffect {
            pixels: gradient_through(&alpha, work, region, width, height, &fx.gradient_overlay),
            blend_mode: fx.gradient_overlay.blend_mode,
            above: true,
        });
    }

    if fx.pattern_overlay.enabled {
        out.push(RenderedEffect {
            pixels: pattern_through(
                &alpha,
                work,
                region,
                width,
                height,
                layer.offset,
                &fx.pattern_overlay,
            ),
            blend_mode: fx.pattern_overlay.blend_mode,
            above: true,
        });
    }

    if fx.inner_glow.enabled {
        let mut inner = inverted(&alpha);
        spread_and_blur(&mut inner, work, fx.inner_glow.spread, fx.inner_glow.size);
        // Confined to the layer: an inner glow that spilled outside would be
        // an outer one.
        multiply_into(&mut inner, &alpha);
        out.push(RenderedEffect {
            pixels: paint(
                &inner,
                work,
                region,
                width,
                height,
                fx.inner_glow.color,
                fx.inner_glow.opacity,
            ),
            blend_mode: fx.inner_glow.blend_mode,
            above: true,
        });
    }

    if fx.inner_shadow.enabled {
        let (dx, dy) = offset_for(fx.inner_shadow.angle, fx.inner_shadow.distance);
        // The shadow inside the top edge is the *hole* the layer sits in,
        // shifted toward the light and softened.
        let mut inner = shift(&inverted(&alpha), work, dx, dy);
        spread_and_blur(&mut inner, work, fx.inner_shadow.spread, fx.inner_shadow.size);
        multiply_into(&mut inner, &alpha);
        out.push(RenderedEffect {
            pixels: paint(
                &inner,
                work,
                region,
                width,
                height,
                fx.inner_shadow.color,
                fx.inner_shadow.opacity,
            ),
            blend_mode: fx.inner_shadow.blend_mode,
            above: true,
        });
    }

    if fx.stroke.enabled {
        let band = stroke_band(&alpha, work, fx.stroke.size, fx.stroke.position);
        out.push(RenderedEffect {
            pixels: paint(
                &band,
                work,
                region,
                width,
                height,
                fx.stroke.color,
                fx.stroke.opacity,
            ),
            blend_mode: fx.stroke.blend_mode,
            above: true,
        });
    }

    out
}

/// The layer's coverage over `rect`, in canvas coordinates.
///
/// Includes the layer mask and its master opacity, both of which an effect
/// follows. Fill opacity is left out on purpose: in Photoshop it scales the
/// layer's own pixels and leaves its effects alone, which is what makes a
/// shadow-only layer possible.
fn alpha_map(layer: &Layer, rect: Rect) -> Vec<f32> {
    let mut alpha = vec![0.0f32; (rect.width as usize) * (rect.height as usize)];
    let opacity = layer.opacity.clamp(0.0, 1.0);
    // The layer's own rectangle, for when its transparency is not allowed to
    // shape its effects.
    let footprint = Rect::new(
        layer.offset.0,
        layer.offset.1,
        layer.pixels.width(),
        layer.pixels.height(),
    );
    for y in 0..rect.height as i32 {
        for x in 0..rect.width as i32 {
            let (doc_x, doc_y) = (rect.x + x, rect.y + y);
            let px = layer.pixels.get(doc_x - layer.offset.0, doc_y - layer.offset.1);

            // Transparency Shapes Layer off: the effects follow the layer's
            // outline instead of its content, so a half-empty layer gets an
            // overlay across all of it.
            let coverage = if layer.transparency_shapes {
                px.a as f32 / 255.0
            } else if footprint.contains(doc_x, doc_y) {
                1.0
            } else {
                0.0
            };
            if coverage <= 0.0 {
                continue;
            }

            let mask = if layer.mask_hides_effects {
                layer.mask_at(doc_x, doc_y)
            } else {
                1.0
            };
            alpha[(y as usize) * rect.width as usize + x as usize] = coverage * mask * opacity;
        }
    }
    alpha
}

/// Where an effect's offset lands, from CS6's angle and distance.
///
/// The angle names the direction the light comes *from*, so the shadow falls
/// the other way; y is negated because the canvas counts downward.
fn offset_for(angle_degrees: f32, distance: f32) -> (f32, f32) {
    let radians = angle_degrees.to_radians();
    (-distance * radians.cos(), distance * radians.sin())
}

fn inverted(alpha: &[f32]) -> Vec<f32> {
    alpha.iter().map(|a| 1.0 - a).collect()
}

fn multiply_into(target: &mut [f32], by: &[f32]) {
    for (t, b) in target.iter_mut().zip(by.iter()) {
        *t *= b;
    }
}

/// Move a coverage map by a sub-pixel offset, sampling bilinearly.
fn shift(alpha: &[f32], rect: Rect, dx: f32, dy: f32) -> Vec<f32> {
    if dx == 0.0 && dy == 0.0 {
        return alpha.to_vec();
    }
    let (w, h) = (rect.width as i32, rect.height as i32);
    // Off the edge of the work area, the scene carries on as it was at the
    // edge — the rect is a window onto a larger canvas, not the end of it.
    // Reading zero instead would invent a hole in the layer, or fill one in.
    let at = |x: i32, y: i32| -> f32 {
        let x = x.clamp(0, w - 1);
        let y = y.clamp(0, h - 1);
        alpha[(y as usize) * rect.width as usize + x as usize]
    };

    let mut out = vec![0.0f32; alpha.len()];
    for y in 0..h {
        for x in 0..w {
            let sx = x as f32 - dx;
            let sy = y as f32 - dy;
            let x0 = sx.floor();
            let y0 = sy.floor();
            let fx = sx - x0;
            let fy = sy - y0;
            let (x0, y0) = (x0 as i32, y0 as i32);
            let v = at(x0, y0) * (1.0 - fx) * (1.0 - fy)
                + at(x0 + 1, y0) * fx * (1.0 - fy)
                + at(x0, y0 + 1) * (1.0 - fx) * fy
                + at(x0 + 1, y0 + 1) * fx * fy;
            out[(y as usize) * rect.width as usize + x as usize] = v;
        }
    }
    out
}

/// Soften a coverage map, having first pushed its edge outward by `spread`.
///
/// Spread is applied as a gain before the blur rather than as a true dilation:
/// at spread 1 the map is hard-edged and the blur has nothing to soften, which
/// is what Photoshop's slider does at its ends.
fn spread_and_blur(alpha: &mut [f32], rect: Rect, spread: f32, size: f32) {
    let spread = spread.clamp(0.0, 1.0);
    if size <= 0.0 {
        return;
    }

    blur_alpha(alpha, rect, size);

    if spread > 0.0 {
        // 1/(1-spread) gain, clipped: the more spread, the more of the blur's
        // falloff is pushed to full coverage.
        let gain = 1.0 / (1.0 - spread * 0.99);
        for a in alpha.iter_mut() {
            *a = (*a * gain).min(1.0);
        }
    }
}

/// The blur radius that spreads an edge over `width` pixels.
///
/// CS6 states a Bevel's and a Satin's Size as the *width* of the feature, not
/// as a blur radius. A step blurred by sigma climbs over about two and a half
/// of them, so handing Size straight to the blur makes the feature two and a
/// half times too wide — and far too soft, which for Satin flattens the fold
/// pattern into a wash.
fn sigma_for(width: f32) -> f32 {
    const RAMP_PER_SIGMA: f32 = 2.5;
    (width / RAMP_PER_SIGMA).max(0.25)
}

/// Soften a coverage map.
///
/// Three box passes rather than a Gaussian. They approximate one closely — the
/// standard result that repeated box filters converge on a Gaussian — and cost
/// the same whatever the radius, which is what matters here: a Satin or a Bevel
/// at Size 80 asks for a 481-tap kernel over a canvas-sized buffer, twice, on
/// *every* repaint. That is seconds per frame, and a live preview that shows
/// the last thing it managed to finish rather than what the sliders say.
///
/// The width is chosen so the three passes carry the same standard deviation as
/// the Gaussian they stand in for, which keeps `LayerEffects::extent` — three
/// sigma — the right amount of margin.
fn blur_alpha(alpha: &mut [f32], rect: Rect, radius: f32) {
    if radius <= 0.0 || rect.is_empty() {
        return;
    }
    // sigma² = n·(w²−1)/12 for n passes of width w.
    let width = (12.0 * radius * radius / 3.0 + 1.0).sqrt().round().max(3.0) as i32;
    let half = (width / 2).max(1);
    for _ in 0..3 {
        box_pass(alpha, rect, half, true);
        box_pass(alpha, rect, half, false);
    }
}

/// One box pass, along rows or columns, with a running sum.
///
/// Off the ends the line carries on as it started and ended, matching how the
/// rest of this module reads past the work area: it is a window onto a larger
/// canvas, not the edge of the world.
fn box_pass(map: &mut [f32], rect: Rect, half: i32, horizontal: bool) {
    let (w, h) = (rect.width as i32, rect.height as i32);
    let (lines, length) = if horizontal { (h, w) } else { (w, h) };
    if length <= 1 {
        return;
    }
    let stride = rect.width as usize;
    let window = (2 * half + 1) as f32;
    let mut line = vec![0.0f32; length as usize];

    for outer in 0..lines {
        let at = |i: i32| -> usize {
            if horizontal {
                (outer as usize) * stride + i as usize
            } else {
                (i as usize) * stride + outer as usize
            }
        };
        for i in 0..length {
            line[i as usize] = map[at(i)];
        }

        let clamped = |i: i32| line[i.clamp(0, length - 1) as usize];
        let mut sum: f32 = (-half..=half).map(clamped).sum();
        for i in 0..length {
            map[at(i)] = sum / window;
            sum += clamped(i + half + 1) - clamped(i - half);
        }
    }
}

/// The band along the layer's edge, on the side `position` asks for.
///
/// A stroke is "every pixel within `size` of the edge", which is a distance
/// transform. This uses the Gaussian instead: blurred coverage falls off
/// monotonically with distance from an edge, so thresholding it at the value a
/// Gaussian reaches one sigma out marks exactly the pixels within one sigma —
/// and a blur is something the engine already has, in a form that reaches the
/// GPU. The result has the rounded corners a real stroke has, and an
/// antialiased edge from the ramp either side of the threshold.
fn stroke_band(alpha: &[f32], rect: Rect, size: f32, position: StrokePosition) -> Vec<f32> {
    if size <= 0.0 {
        return vec![0.0; alpha.len()];
    }

    let (outward, inward) = match position {
        StrokePosition::Outside => (size, 0.0),
        StrokePosition::Inside => (0.0, size),
        StrokePosition::Center => (size / 2.0, size / 2.0),
    };

    let mut band = vec![0.0f32; alpha.len()];

    if outward > 0.0 {
        // Reaches out from the shape; the layer's own area is taken back off,
        // since an outside stroke rings the layer rather than covering it.
        let out = within(alpha, rect, outward);
        for i in 0..band.len() {
            band[i] = band[i].max(out[i] * (1.0 - alpha[i]));
        }
    }
    if inward > 0.0 {
        // The same, measured from the *hole* the layer sits in, which puts the
        // band inside the edge.
        let inside = within(&inverted(alpha), rect, inward);
        for i in 0..band.len() {
            band[i] = band[i].max(inside[i] * alpha[i]);
        }
    }
    band
}

/// Coverage of "within `distance` pixels of where `source` is solid".
fn within(source: &[f32], rect: Rect, distance: f32) -> Vec<f32> {
    // What a Gaussian of sigma = distance reads one sigma out from a straight
    // edge: 0.5·erfc(1/√2). Thresholding the blur here is what turns falloff
    // back into distance.
    const AT_ONE_SIGMA: f32 = 0.1587;

    let mut blurred = source.to_vec();
    blur_alpha(&mut blurred, rect, distance);

    // Half-width of the ramp across the threshold, chosen so the edge is about
    // a pixel wide: the Gaussian's slope there is pdf(1)/sigma per pixel.
    let ramp = (0.242 / distance.max(0.5) * 0.5).clamp(0.01, 0.2);
    for v in blurred.iter_mut() {
        let t = ((*v - (AT_ONE_SIGMA - ramp)) / (2.0 * ramp)).clamp(0.0, 1.0);
        // Smoothstep, so the band's edge does not have a crease in it.
        *v = t * t * (3.0 - 2.0 * t);
    }
    blurred
}

/// Bevel & Emboss: the alpha as a height map, differentiated and lit.
///
/// Produces two entries — the lit side and the unlit side — because CS6 gives
/// them separate colours, opacities and blend modes.
fn render_bevel(
    alpha: &[f32],
    work: Rect,
    region: Rect,
    width: u32,
    height: u32,
    bevel: &BevelEffect,
    stroke_band: Option<&[f32]>,
    out: &mut Vec<RenderedEffect>,
) {
    if bevel.size <= 0.0 {
        return;
    }

    // The shoulder: a blurred alpha reads as a ramp up the side of the shape,
    // which is exactly the surface a bevel wants.
    //
    let sigma = sigma_for(bevel.size * bevel.technique.shoulder());
    let mut heights = alpha.to_vec();
    blur_alpha(&mut heights, work, sigma);

    // Where the bevel is allowed to show, and — for Pillow Emboss — where the
    // slope has to be read the other way up.
    let confine: Vec<f32> = match bevel.style {
        BevelStyle::InnerBevel => alpha.to_vec(),
        BevelStyle::OuterBevel => inverted(alpha),
        BevelStyle::Emboss | BevelStyle::PillowEmboss => vec![1.0; alpha.len()],
        BevelStyle::StrokeEmboss => match stroke_band {
            Some(band) => band.to_vec(),
            // No stroke to emboss.
            None => return,
        },
    };

    let (w, h) = (work.width as i32, work.height as i32);
    let at = |map: &[f32], x: i32, y: i32| -> f32 {
        let x = x.clamp(0, w - 1);
        let y = y.clamp(0, h - 1);
        map[(y as usize) * work.width as usize + x as usize]
    };

    // The light, in screen coordinates: y counts downward, so a light from
    // above has a negative y.
    let theta = bevel.angle.to_radians();
    let phi = bevel.altitude.to_radians().clamp(0.0, std::f32::consts::FRAC_PI_2);
    let (cos_phi, sin_phi) = (phi.cos(), phi.sin());
    let light = [cos_phi * theta.cos(), -cos_phi * theta.sin(), sin_phi];
    // `light[2]` is only along for the ride: see the shading loop.

    // A blur of radius sigma has a peak slope of about 0.4/sigma, so scaling by
    // sigma keeps Depth meaning the same thing at every Size. The extra factor
    // is what makes 100% a *pronounced* bevel rather than a gentle incline:
    // it puts the shoulder's steepest point past 60°, where the highlight very
    // nearly reaches the opacity the user asked for — which is where CS6's 100%
    // sits. Beyond that the response saturates instead of clipping.
    const DEPTH_GAIN: f32 = 5.0;
    let slope = bevel.depth.max(0.0) * sigma.max(1.0) * DEPTH_GAIN;

    let mut shading = vec![0.0f32; alpha.len()];
    for y in 0..h {
        for x in 0..w {
            let dx = (at(&heights, x + 1, y) - at(&heights, x - 1, y)) * 0.5;
            let dy = (at(&heights, x, y + 1) - at(&heights, x, y - 1)) * 0.5;

            // Surface normal of the height field.
            let nx = -dx * slope;
            let ny = -dy * slope;
            let len = (nx * nx + ny * ny + 1.0).sqrt();

            // Only the *horizontal* part of the lighting is wanted. The
            // vertical part is the same wherever the surface is flat, so
            // including it would tint the whole layer instead of its edges —
            // and would cap the brightest highlight at a fraction of the
            // opacity the user asked for.
            //
            // Dividing by cos(altitude) undoes the light's own foreshortening,
            // so a low sun and a high one differ in *where* they light the
            // shoulder rather than in how much they can light it at all. At 90°
            // the horizontal part is zero and the bevel flattens away, which is
            // what CS6's Altitude does at its top.
            let horizontal = (nx * light[0] + ny * light[1]) / len;
            let mut lit = horizontal / cos_phi.max(1e-3);
            if !bevel.up {
                // Direction: Down turns the ridge into a trough.
                lit = -lit;
            }
            shading[(y as usize) * work.width as usize + x as usize] = lit.clamp(-1.0, 1.0);
        }
    }

    if bevel.soften > 0.0 {
        soften(&mut shading, work, bevel.soften);
    }

    // Split into the two sides and confine each.
    let mut highlight = vec![0.0f32; alpha.len()];
    let mut shadow = vec![0.0f32; alpha.len()];
    for i in 0..alpha.len() {
        let lit = shading[i];
        match bevel.style {
            BevelStyle::PillowEmboss => {
                // A ridge inside the shape and a groove outside it, which is
                // what makes the edge look pressed into the surface.
                let inside = alpha[i];
                highlight[i] = lit.max(0.0) * inside + (-lit).max(0.0) * (1.0 - inside);
                shadow[i] = (-lit).max(0.0) * inside + lit.max(0.0) * (1.0 - inside);
            }
            _ => {
                highlight[i] = lit.max(0.0) * confine[i];
                shadow[i] = (-lit).max(0.0) * confine[i];
            }
        }
    }

    out.push(RenderedEffect {
        pixels: paint(&shadow, work, region, width, height, bevel.shadow_color,
                      bevel.shadow_opacity),
        blend_mode: bevel.shadow_mode,
        above: true,
    });
    out.push(RenderedEffect {
        pixels: paint(&highlight, work, region, width, height, bevel.highlight_color,
                      bevel.highlight_opacity),
        blend_mode: bevel.highlight_mode,
        above: true,
    });
}

/// Satin: two softened copies of the layer, pushed apart and differenced.
fn satin(
    alpha: &[f32],
    work: Rect,
    region: Rect,
    width: u32,
    height: u32,
    satin: &SatinEffect,
) -> Pixmap {
    let (dx, dy) = offset_for(satin.angle, satin.distance);

    // One copy each way along the angle. Softening them before the difference
    // rather than after is what gives the bands their gradient: differencing
    // two hard shapes first would leave a flat cut-out to blur.
    let mut ahead = shift(alpha, work, dx, dy);
    let mut behind = shift(alpha, work, -dx, -dy);
    let sigma = sigma_for(satin.size);
    blur_alpha(&mut ahead, work, sigma);
    blur_alpha(&mut behind, work, sigma);

    let mut bands: Vec<f32> = ahead
        .iter()
        .zip(behind.iter())
        .map(|(a, b)| (a - b).abs())
        .collect();

    if satin.invert {
        for v in bands.iter_mut() {
            *v = 1.0 - *v;
        }
    }
    // A sheen on the shape, not a halo around it.
    multiply_into(&mut bands, alpha);

    paint(&bands, work, region, width, height, satin.color, satin.opacity)
}

/// Blur a signed map — the bevel's shading runs from -1 to 1, and `blur_alpha`
/// only carries what fits in a byte.
fn soften(map: &mut [f32], rect: Rect, radius: f32) {
    let mut shifted: Vec<f32> = map.iter().map(|v| (v + 1.0) * 0.5).collect();
    blur_alpha(&mut shifted, rect, radius);
    for (out, v) in map.iter_mut().zip(shifted.iter()) {
        *out = v * 2.0 - 1.0;
    }
}

/// Turn a coverage map into canvas-space pixels of one colour.
fn paint(
    alpha: &[f32],
    work: Rect,
    region: Rect,
    width: u32,
    height: u32,
    color: Rgba8,
    opacity: f32,
) -> Pixmap {
    let mut out = Pixmap::new(width, height);
    let opacity = opacity.clamp(0.0, 1.0) * (color.a as f32 / 255.0);
    for y in region.y..region.bottom() {
        for x in region.x..region.right() {
            let (lx, ly) = (x - work.x, y - work.y);
            if lx < 0 || ly < 0 || lx >= work.width as i32 || ly >= work.height as i32 {
                continue;
            }
            let a = alpha[(ly as usize) * work.width as usize + lx as usize] * opacity;
            if a <= 0.0 {
                continue;
            }
            out.set(
                x,
                y,
                Rgba8::new(
                    color.r,
                    color.g,
                    color.b,
                    (a * 255.0).round().clamp(0.0, 255.0) as u8,
                ),
            );
        }
    }
    out
}

/// Gradient Overlay: a ramp laid across the layer, in any of CS6's five shapes.
fn gradient_through(
    alpha: &[f32],
    work: Rect,
    region: Rect,
    width: u32,
    height: u32,
    effect: &GradientOverlayEffect,
) -> Pixmap {
    let ramp = Gradient::two_stop(effect.from, effect.to);
    let ramp = if effect.reverse { ramp.reversed() } else { ramp };

    // What the ramp spans: the layer's own content, or the whole canvas when
    // Align with Layer is off.
    let span = if effect.align_with_layer {
        match content_bounds(alpha, work) {
            Some(bounds) => bounds,
            None => return Pixmap::new(width, height),
        }
    } else {
        Rect::new(-work.x, -work.y, width, height)
    };

    let centre_x = span.x as f32 + span.width as f32 / 2.0;
    let centre_y = span.y as f32 + span.height as f32 / 2.0;
    let radians = effect.angle.to_radians();
    // Screen y counts downward, so the sine is negated to make 90° point up.
    let (dir_x, dir_y) = (radians.cos(), -radians.sin());

    // Half the span measured along the ramp's own direction, so the ends of
    // the ramp land on the ends of the layer at any angle.
    let half = ((span.width as f32 * dir_x.abs() + span.height as f32 * dir_y.abs()) / 2.0)
        * effect.scale.max(0.01);
    let half = half.max(1.0);

    // A linear ramp runs from one side to the other, so it starts at the near
    // edge. The other four are measured *outward* from a point — a radial's
    // distance, an angle's sweep, a reflection's mirror — so they start at the
    // middle, with the same reach.
    let (start, axis, length) = if effect.shape == GradientType::Linear {
        (
            (centre_x - dir_x * half, centre_y - dir_y * half),
            (dir_x * 2.0 * half, dir_y * 2.0 * half),
            2.0 * half,
        )
    } else {
        ((centre_x, centre_y), (dir_x * half, dir_y * half), half)
    };

    let opacity = effect.opacity.clamp(0.0, 1.0);
    let mut out = Pixmap::new(width, height);
    for y in region.y..region.bottom() {
        for x in region.x..region.right() {
            let (lx, ly) = (x - work.x, y - work.y);
            if lx < 0 || ly < 0 || lx >= work.width as i32 || ly >= work.height as i32 {
                continue;
            }
            let coverage = alpha[(ly as usize) * work.width as usize + lx as usize];
            if coverage <= 0.0 {
                continue;
            }

            // The five shapes come from the Gradient tool's own code, so a
            // Radial overlay and a Radial drag agree.
            let t = crate::gradient::position_at(
                effect.shape,
                lx as f32,
                ly as f32,
                start,
                axis,
                length,
            )
            .clamp(0.0, 1.0);
            let stop = ramp.sample(t);

            let mut rgb = [stop.r as f32, stop.g as f32, stop.b as f32];
            if effect.dither {
                // One noise value for all three channels: per-channel noise
                // would show as colour speckle rather than as grain.
                let n = crate::gradient::dither(x, y) * 1.5;
                for v in &mut rgb {
                    *v += n;
                }
            }
            let a = coverage * opacity * (stop.a as f32 / 255.0);
            let channel = |v: f32| v.round().clamp(0.0, 255.0) as u8;
            out.set(
                x,
                y,
                Rgba8::new(
                    channel(rgb[0]),
                    channel(rgb[1]),
                    channel(rgb[2]),
                    (a * 255.0).round().clamp(0.0, 255.0) as u8,
                ),
            );
        }
    }
    out
}

/// Pattern Overlay: a tile repeated through the layer's coverage.
fn pattern_through(
    alpha: &[f32],
    work: Rect,
    region: Rect,
    width: u32,
    height: u32,
    layer_offset: (i32, i32),
    effect: &PatternOverlayEffect,
) -> Pixmap {
    let Some(tile) = pattern::tile(effect.pattern as usize) else {
        return Pixmap::new(width, height);
    };
    let (tile_w, tile_h) = (tile.width() as f32, tile.height() as f32);
    if tile_w <= 0.0 || tile_h <= 0.0 {
        return Pixmap::new(width, height);
    }

    let scale = effect.scale.clamp(0.1, 10.0);
    // Linked to the layer, the tiling starts at the layer's corner and travels
    // with it; otherwise it is pinned to the canvas.
    let origin = if effect.link_with_layer {
        (layer_offset.0 as f32, layer_offset.1 as f32)
    } else {
        (0.0, 0.0)
    };

    let opacity = effect.opacity.clamp(0.0, 1.0);
    let mut out = Pixmap::new(width, height);
    for y in region.y..region.bottom() {
        for x in region.x..region.right() {
            let (lx, ly) = (x - work.x, y - work.y);
            if lx < 0 || ly < 0 || lx >= work.width as i32 || ly >= work.height as i32 {
                continue;
            }
            let coverage = alpha[(ly as usize) * work.width as usize + lx as usize];
            if coverage <= 0.0 {
                continue;
            }

            // Sampled at the scaled position rather than by scaling the tile,
            // so a pattern at 250% costs no more than one at 100% and stays
            // seamless — `rem_euclid` wraps the same way left of the origin as
            // right of it.
            let sx = ((x as f32 - origin.0) / scale).rem_euclid(tile_w) as i32;
            let sy = ((y as f32 - origin.1) / scale).rem_euclid(tile_h) as i32;
            let px = tile.get(sx, sy);
            let a = coverage * opacity * (px.a as f32 / 255.0);
            if a <= 0.0 {
                continue;
            }
            out.set(
                x,
                y,
                Rgba8::new(px.r, px.g, px.b, (a * 255.0).round().clamp(0.0, 255.0) as u8),
            );
        }
    }
    out
}

/// The part of the work area the layer actually covers, in work coordinates.
fn content_bounds(alpha: &[f32], work: Rect) -> Option<Rect> {
    let (mut min_x, mut min_y) = (work.width as i32, work.height as i32);
    let (mut max_x, mut max_y) = (-1i32, -1i32);
    for y in 0..work.height as i32 {
        for x in 0..work.width as i32 {
            if alpha[(y as usize) * work.width as usize + x as usize] > 0.0 {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    if max_x < min_x {
        return None;
    }
    Some(Rect::new(
        min_x,
        min_y,
        (max_x - min_x + 1) as u32,
        (max_y - min_y + 1) as u32,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::LayerId;

    /// A layer with an opaque square in the middle of a 40x40 canvas.
    fn square() -> Layer {
        let mut layer = Layer::new_raster(LayerId(1), "square", 40, 40);
        for y in 12..28i32 {
            for x in 12..28i32 {
                layer.pixels.set(x, y, Rgba8::WHITE);
            }
        }
        layer
    }

    fn canvas() -> Rect {
        Rect::from_size(40, 40)
    }

    #[test]
    fn every_setting_survives_a_round_trip_by_name() {
        // The shell reaches every control through these names; a typo in one
        // would silently do nothing, so each is exercised here.
        let keys = [
            "dropShadow.on", "dropShadow.mode", "dropShadow.color", "dropShadow.opacity",
            "dropShadow.angle", "dropShadow.distance", "dropShadow.spread", "dropShadow.size",
            "innerShadow.on", "innerShadow.mode", "innerShadow.color", "innerShadow.opacity",
            "innerShadow.angle", "innerShadow.distance", "innerShadow.spread",
            "innerShadow.size",
            "outerGlow.on", "outerGlow.mode", "outerGlow.color", "outerGlow.opacity",
            "outerGlow.spread", "outerGlow.size",
            "innerGlow.on", "innerGlow.mode", "innerGlow.color", "innerGlow.opacity",
            "innerGlow.spread", "innerGlow.size",
            "colorOverlay.on", "colorOverlay.mode", "colorOverlay.color",
            "colorOverlay.opacity",
            "gradientOverlay.on", "gradientOverlay.mode", "gradientOverlay.opacity",
            "gradientOverlay.from", "gradientOverlay.to", "gradientOverlay.angle",
            "gradientOverlay.reverse",
            "stroke.on", "stroke.mode", "stroke.color", "stroke.opacity", "stroke.size",
            "stroke.position",
            "bevel.on", "bevel.present", "bevel.style", "bevel.technique", "bevel.depth",
            "bevel.up", "bevel.size", "bevel.soften", "bevel.angle", "bevel.altitude",
            "bevel.highlightMode", "bevel.highlightColor", "bevel.highlightOpacity",
            "bevel.shadowMode", "bevel.shadowColor", "bevel.shadowOpacity",
            "satin.on", "satin.present", "satin.mode", "satin.color", "satin.opacity",
            "satin.angle", "satin.distance", "satin.size", "satin.invert",
            "gradientOverlay.shape", "gradientOverlay.scale", "gradientOverlay.dither",
            "gradientOverlay.align",
            "patternOverlay.on", "patternOverlay.present", "patternOverlay.mode",
            "patternOverlay.opacity", "patternOverlay.pattern", "patternOverlay.scale",
            "patternOverlay.link",
            "dropShadow.present", "innerShadow.present", "outerGlow.present",
            "innerGlow.present", "colorOverlay.present", "gradientOverlay.present",
            "stroke.present",
            "hidden",
        ];

        for key in keys {
            let mut fx = LayerEffects::default();
            // A value every field can hold: within 0..1 for the fractions,
            // a real blend mode, a colour that is not black or white.
            let probe: f32 = if key.ends_with("color") || key.ends_with("Color")
                || key.ends_with("from") || key.ends_with("to")
            {
                pack_color(Rgba8::opaque(18, 52, 86))
            } else if key.ends_with("mode") || key.ends_with("Mode") {
                BlendMode::Multiply as i32 as f32
            } else {
                1.0
            };
            assert!(fx.set_value(key, probe), "{key} was not recognised");
            assert_eq!(fx.value(key), probe, "{key} did not survive the round trip");
        }
    }

    #[test]
    fn switching_an_effect_off_leaves_it_on_the_layer() {
        // CS6 keeps the row in the Layers panel with its eye closed, so the
        // effect has to outlive being switched off.
        let mut fx = LayerEffects::default();
        assert!(!fx.any_present());

        fx.set_value("stroke.on", 1.0);
        assert!(fx.any_present());
        assert!(fx.any_enabled());

        fx.set_value("stroke.on", 0.0);
        assert!(fx.any_present(), "the effect left the layer when it was switched off");
        assert!(!fx.any_enabled(), "a switched-off effect must not draw");
        assert_eq!(fx.value("stroke.present"), 1.0);
    }

    #[test]
    fn settings_survive_being_switched_off_and_on_again() {
        let mut fx = LayerEffects::default();
        fx.set_value("stroke.on", 1.0);
        fx.set_value("stroke.size", 9.0);
        fx.set_value("stroke.on", 0.0);
        fx.set_value("stroke.on", 1.0);
        assert_eq!(fx.value("stroke.size"), 9.0);
    }

    #[test]
    fn an_unknown_setting_is_refused_rather_than_swallowed() {
        let mut fx = LayerEffects::default();
        assert!(!fx.set_value("dropShadow.wobble", 1.0));
        assert!(!fx.set_value("patternOverlay.wobble", 1.0));
        assert!(!fx.set_value("nonsense", 1.0));
        assert_eq!(fx.value("dropShadow.wobble"), 0.0);
    }

    #[test]
    fn a_colour_packs_and_unpacks_exactly() {
        for color in [Rgba8::BLACK, Rgba8::WHITE, Rgba8::opaque(1, 128, 254)] {
            assert_eq!(unpack_color(pack_color(color)), color);
        }
    }

    #[test]
    fn no_effects_renders_nothing() {
        let layer = square();
        assert!(render(&layer, 40, 40, canvas()).is_empty());
    }

    #[test]
    fn hidden_effects_render_nothing() {
        let mut layer = square();
        layer.effects.drop_shadow.enabled = true;
        assert_eq!(render(&layer, 40, 40, canvas()).len(), 1);

        // Hide All Effects keeps the settings and stops the drawing.
        layer.effects.hidden = true;
        assert!(render(&layer, 40, 40, canvas()).is_empty());
        assert!(layer.effects.drop_shadow.enabled, "the setting must survive");
    }

    #[test]
    fn a_drop_shadow_falls_away_from_the_light_and_sits_below() {
        let mut layer = square();
        layer.effects.drop_shadow.enabled = true;
        layer.effects.drop_shadow.angle = 120.0;
        layer.effects.drop_shadow.distance = 6.0;
        layer.effects.drop_shadow.size = 0.0;

        let rendered = render(&layer, 40, 40, canvas());
        assert_eq!(rendered.len(), 1);
        assert!(!rendered[0].above, "a drop shadow draws under its layer");

        // Light from 120° is up and to the left, so the shadow goes down and
        // to the right.
        let px = &rendered[0].pixels;
        assert!(px.get(29, 30).a > 0, "no shadow where it should have fallen");
        assert_eq!(px.get(8, 8).a, 0, "shadow fell toward the light");
    }

    #[test]
    fn a_bigger_shadow_reaches_further() {
        let reach = |size: f32| {
            let mut layer = square();
            layer.effects.drop_shadow.enabled = true;
            layer.effects.drop_shadow.distance = 0.0;
            layer.effects.drop_shadow.size = size;
            let rendered = render(&layer, 40, 40, canvas());
            let px = &rendered[0].pixels;
            // How far past the square's right edge any shadow is visible.
            (28..40i32).filter(|&x| px.get(x, 20).a > 0).count()
        };
        assert!(reach(8.0) > reach(2.0), "Size must widen the shadow");
    }

    #[test]
    fn a_bevel_lights_one_side_of_the_edge_and_darkens_the_other() {
        // A big square in a big canvas: a shape narrower than the bevel is a
        // dome, not a plateau, and has no flat middle to test against.
        let mut layer = Layer::new_raster(LayerId(1), "square", 80, 80);
        for y in 20..60i32 {
            for x in 20..60i32 {
                layer.pixels.set(x, y, Rgba8::WHITE);
            }
        }
        layer.effects.bevel.enabled = true;
        layer.effects.bevel.angle = 120.0;
        layer.effects.bevel.size = 5.0;

        let rendered = render(&layer, 80, 80, Rect::from_size(80, 80));
        // Two entries: the unlit side, then the lit one.
        assert_eq!(rendered.len(), 2);
        let shadow = &rendered[0].pixels;
        let highlight = &rendered[1].pixels;

        // The light comes from the upper left, so the top-left inside edge is
        // lit and the bottom-right inside edge is in shadow.
        assert!(highlight.get(22, 22).a > 0, "the lit edge is not lit");
        assert!(shadow.get(57, 57).a > 0, "the unlit edge is not shaded");
        assert_eq!(highlight.get(57, 57).a, 0, "the unlit edge picked up a highlight");
        // ...and the flat middle of the shape is neither.
        assert_eq!(highlight.get(40, 40).a, 0, "a flat surface must not shade");
        assert_eq!(shadow.get(40, 40).a, 0, "a flat surface must not shade");
    }

    #[test]
    fn size_is_the_width_of_the_bevel() {
        // CS6's Size is how wide the shoulder is, in pixels. Handing it to the
        // blur as a sigma instead made the bevel two and a half times too wide
        // and far too soft — enough to wash out a whole photograph.
        let width_for = |size: f32| {
            let mut layer = Layer::new_raster(LayerId(1), "bg", 120, 120);
            layer.pixels.fill(Rgba8::WHITE);
            layer.effects.bevel.enabled = true;
            layer.effects.bevel.size = size;
            // Straight down the left edge, lit from the left.
            layer.effects.bevel.angle = 180.0;
            let rendered = render(&layer, 120, 120, Rect::from_size(120, 120));
            let lit = &rendered[1].pixels;
            (0..60i32).filter(|&x| lit.get(x, 60).a > 2).count() as f32
        };

        let narrow = width_for(10.0);
        let wide = width_for(30.0);
        assert!(narrow > 4.0 && narrow < 20.0, "a 10px bevel came out {narrow}px wide");
        assert!(wide > 18.0 && wide < 50.0, "a 30px bevel came out {wide}px wide");
        assert!(wide > narrow);
    }

    #[test]
    fn direction_down_swaps_the_lit_and_unlit_sides() {
        let lit_corner = |up: bool| {
            let mut layer = square();
            layer.effects.bevel.enabled = true;
            layer.effects.bevel.angle = 120.0;
            layer.effects.bevel.size = 5.0;
            layer.effects.bevel.up = up;
            let rendered = render(&layer, 40, 40, canvas());
            rendered[1].pixels.get(13, 13).a
        };
        assert!(lit_corner(true) > 0);
        assert_eq!(lit_corner(false), 0, "Direction: Down must turn the ridge over");
    }

    #[test]
    fn an_inner_bevel_stays_inside_and_an_outer_one_stays_out() {
        let sample = |style: BevelStyle, x: i32, y: i32| {
            let mut layer = square();
            layer.effects.bevel.enabled = true;
            layer.effects.bevel.style = style;
            layer.effects.bevel.size = 4.0;
            let rendered = render(&layer, 40, 40, canvas());
            // Either side of the bevel counts as "something is drawn here".
            rendered[0].pixels.get(x, y).a.max(rendered[1].pixels.get(x, y).a)
        };

        // Just inside the square's edge, and just outside it.
        assert!(sample(BevelStyle::InnerBevel, 13, 20) > 0);
        assert_eq!(sample(BevelStyle::InnerBevel, 10, 20), 0);
        assert!(sample(BevelStyle::OuterBevel, 10, 20) > 0);
        assert_eq!(sample(BevelStyle::OuterBevel, 14, 20), 0);
        // Emboss straddles the edge, so it shows on both sides of it.
        assert!(sample(BevelStyle::Emboss, 13, 20) > 0);
        assert!(sample(BevelStyle::Emboss, 10, 20) > 0);
    }

    #[test]
    fn stroke_emboss_needs_a_stroke_to_ride() {
        let mut layer = square();
        layer.effects.bevel.enabled = true;
        layer.effects.bevel.style = BevelStyle::StrokeEmboss;
        assert!(render(&layer, 40, 40, canvas()).is_empty(), "there is no stroke to emboss");

        layer.effects.stroke.enabled = true;
        layer.effects.stroke.size = 4.0;
        let rendered = render(&layer, 40, 40, canvas());
        // Stroke plus the bevel's two sides.
        assert_eq!(rendered.len(), 3);
    }

    #[test]
    fn an_overhead_light_flattens_the_bevel() {
        let mut layer = square();
        layer.effects.bevel.enabled = true;
        layer.effects.bevel.size = 5.0;
        layer.effects.bevel.altitude = 90.0;

        // Straight overhead, every slope is lit alike and the edge disappears —
        // which is what CS6's Altitude does at its top.
        let rendered = render(&layer, 40, 40, canvas());
        for y in 0..40i32 {
            for x in 0..40i32 {
                assert_eq!(rendered[1].pixels.get(x, y).a, 0, "lit at {x},{y}");
            }
        }
    }

    #[test]
    fn satin_bands_the_layer_and_stays_on_it() {
        let mut layer = square();
        layer.effects.satin.enabled = true;
        layer.effects.satin.distance = 6.0;
        layer.effects.satin.size = 4.0;
        layer.effects.satin.invert = false;

        let rendered = render(&layer, 40, 40, canvas());
        assert_eq!(rendered.len(), 1);
        let px = &rendered[0].pixels;

        // Somewhere on the shape the two offset copies disagree, which is the
        // whole effect...
        let any = (12..28i32).any(|y| (12..28i32).any(|x| px.get(x, y).a > 0));
        assert!(any, "satin drew nothing at all");
        // ...and nowhere off it.
        assert_eq!(px.get(4, 4).a, 0, "satin is a sheen on the layer, not a halo");
        assert_eq!(px.get(35, 20).a, 0);
    }

    #[test]
    fn inverting_satin_swaps_the_bands_for_the_gaps() {
        let sample = |invert: bool| {
            let mut layer = square();
            layer.effects.satin.enabled = true;
            layer.effects.satin.distance = 6.0;
            layer.effects.satin.size = 4.0;
            layer.effects.satin.invert = invert;
            // Full opacity, so the numbers below are about the effect rather
            // than about CS6's 50% default.
            layer.effects.satin.opacity = 1.0;
            // The middle of the shape, where two copies of it agree and the
            // difference is therefore nothing.
            render(&layer, 40, 40, canvas())[0].pixels.get(20, 20).a
        };
        // In the middle the two offset copies very nearly agree, so there is
        // next to no band — and inverting turns that into near-full cover.
        assert!(sample(false) < 32, "unexpected band in the middle: {}", sample(false));
        assert!(sample(true) > 200, "inverted, the middle should be covered");
    }

    #[test]
    fn dissolve_scatters_an_effect_rather_than_washing_it() {
        // Dissolve turns partial coverage into a coin toss per pixel. The
        // compositor does that for a layer's own pixels; an effect set to
        // Dissolve has to get the same treatment, or it comes out as a smooth
        // wash indistinguishable from every other mode.
        use crate::document::Document;

        let mut d = Document::new(60, 60, Rgba8::WHITE);
        let id = d.active_layer_id();
        d.set_layer_effect_value(id, "colorOverlay.on", 1.0);
        d.set_layer_effect_value(id, "colorOverlay.color", 0.0);
        d.set_layer_effect_value(id, "colorOverlay.opacity", 0.5);
        d.set_layer_effect_value(id, "colorOverlay.mode", BlendMode::Dissolve as i32 as f32);
        d.commit_layer_effects();

        let composite = d.composite();
        let mut black = 0;
        let mut white = 0;
        let mut between = 0;
        for y in 0..60i32 {
            for x in 0..60i32 {
                match composite.get(x, y).r {
                    0 => black += 1,
                    255 => white += 1,
                    _ => between += 1,
                }
            }
        }
        assert_eq!(between, 0, "dissolve left {between} half-covered pixels");
        assert!(black > 0 && white > 0, "dissolve covered everything or nothing");
    }

    #[test]
    fn a_colour_overlay_covers_the_layer_and_nothing_else() {
        let mut layer = square();
        layer.effects.color_overlay.enabled = true;
        layer.effects.color_overlay.color = Rgba8::opaque(10, 200, 30);

        let rendered = render(&layer, 40, 40, canvas());
        assert_eq!(rendered.len(), 1);
        assert!(rendered[0].above, "an overlay draws over its layer");
        let px = &rendered[0].pixels;
        assert_eq!(px.get(20, 20), Rgba8::opaque(10, 200, 30));
        assert_eq!(px.get(2, 2).a, 0, "overlay leaked outside the layer");
    }

    #[test]
    fn an_inner_shadow_reaches_as_far_as_its_distance() {
        // An inner shadow is built from the hole the layer sits in, dragged
        // inward. If the renderer cannot see far enough outside the layer to
        // find that hole, it drags in nothing and draws nothing — which is
        // exactly what a too-small work area used to do at any real distance.
        let mut layer = Layer::new_raster(LayerId(1), "bg", 60, 60);
        layer.pixels.fill(Rgba8::WHITE);
        layer.effects.inner_shadow.enabled = true;
        layer.effects.inner_shadow.angle = 30.0;
        layer.effects.inner_shadow.distance = 20.0;
        layer.effects.inner_shadow.size = 2.0;

        let rendered = render(&layer, 60, 60, Rect::from_size(60, 60));
        let px = &rendered[0].pixels;
        // Light from the upper right, so the shadow lies along the top and the
        // right of the inside — and reaches well in from the edge.
        assert!(px.get(30, 2).a > 0, "nothing along the top");
        assert!(px.get(30, 12).a > 0, "the shadow did not reach in");
        assert!(px.get(57, 30).a > 0, "nothing along the right");
        assert_eq!(px.get(2, 55).a, 0, "shadow on the side the light comes from");
        assert_eq!(px.get(30, 30).a, 0, "an inner shadow is an edge, not a fill");
    }

    #[test]
    fn an_inner_glow_stays_inside_the_layer() {
        let mut layer = square();
        layer.effects.inner_glow.enabled = true;
        layer.effects.inner_glow.size = 4.0;

        let rendered = render(&layer, 40, 40, canvas());
        let px = &rendered[0].pixels;
        // Bright just inside the edge, nothing at all outside it.
        assert!(px.get(13, 20).a > 0, "no glow along the inside of the edge");
        assert_eq!(px.get(10, 20).a, 0, "an inner glow must not spill outside");
        // ...and it fades toward the middle.
        assert!(px.get(20, 20).a < px.get(13, 20).a);
    }

    #[test]
    fn an_outer_stroke_rings_the_layer_without_covering_it() {
        let mut layer = square();
        layer.effects.stroke.enabled = true;
        layer.effects.stroke.size = 3.0;
        layer.effects.stroke.position = StrokePosition::Outside;

        let rendered = render(&layer, 40, 40, canvas());
        let px = &rendered[0].pixels;
        assert!(px.get(10, 20).a > 0, "nothing outside the edge");
        assert_eq!(px.get(20, 20).a, 0, "an outside stroke must not fill the layer");
    }

    #[test]
    fn an_inside_stroke_stays_within_the_layer() {
        let mut layer = square();
        layer.effects.stroke.enabled = true;
        layer.effects.stroke.size = 3.0;
        layer.effects.stroke.position = StrokePosition::Inside;

        let rendered = render(&layer, 40, 40, canvas());
        let px = &rendered[0].pixels;
        assert_eq!(px.get(9, 20).a, 0, "an inside stroke must not spill outside");
        assert!(px.get(13, 20).a > 0, "nothing along the inside of the edge");
        assert_eq!(px.get(20, 20).a, 0, "a stroke is a band, not a fill");
    }

    #[test]
    fn a_gradient_overlay_runs_across_the_layer() {
        let mut layer = square();
        layer.effects.gradient_overlay.enabled = true;
        layer.effects.gradient_overlay.from = Rgba8::BLACK;
        layer.effects.gradient_overlay.to = Rgba8::WHITE;
        layer.effects.gradient_overlay.angle = 0.0;

        let rendered = render(&layer, 40, 40, canvas());
        let px = &rendered[0].pixels;
        // At 0° the ramp runs left to right across the square.
        assert!(px.get(13, 20).r < px.get(26, 20).r, "the ramp did not run");
        assert_eq!(px.get(2, 20).a, 0, "the ramp is confined to the layer");
    }

    #[test]
    fn a_radial_gradient_overlay_runs_from_the_middle_outward() {
        let mut layer = square();
        layer.effects.gradient_overlay.enabled = true;
        layer.effects.gradient_overlay.shape = GradientType::Radial;
        layer.effects.gradient_overlay.from = Rgba8::BLACK;
        layer.effects.gradient_overlay.to = Rgba8::WHITE;

        let rendered = render(&layer, 40, 40, canvas());
        let px = &rendered[0].pixels;
        // Dark in the middle, light at the corners of the shape — which a
        // linear ramp at any angle could not do.
        assert!(px.get(20, 20).r < 40);
        assert!(px.get(13, 13).r > px.get(18, 18).r);
        assert_eq!(px.get(2, 2).a, 0, "the ramp is confined to the layer");
    }

    #[test]
    fn gradient_overlay_scale_stretches_the_ramp() {
        let sample = |scale: f32| {
            let mut layer = square();
            layer.effects.gradient_overlay.enabled = true;
            layer.effects.gradient_overlay.angle = 0.0;
            layer.effects.gradient_overlay.scale = scale;
            render(&layer, 40, 40, canvas())[0].pixels.get(15, 20).r
        };
        // Stretched, less of the ramp fits across the layer, so a point near
        // its left edge sits closer to the middle of the ramp than to its end.
        assert!(sample(3.0) > sample(1.0), "Scale did not stretch the ramp");
    }

    #[test]
    fn a_pattern_overlay_tiles_the_layer_and_stops_at_its_edge() {
        let mut layer = square();
        layer.effects.pattern_overlay.enabled = true;
        layer.effects.pattern_overlay.pattern = 0; // Checkerboard
        // The tile is 64px with 32px squares; the test shape is 16px across,
        // so without scaling it would sit inside a single square.
        layer.effects.pattern_overlay.scale = 0.25;

        let rendered = render(&layer, 40, 40, canvas());
        assert_eq!(rendered.len(), 1);
        let px = &rendered[0].pixels;

        // The tile is not flat, so somewhere on the layer two pixels differ...
        let mut seen = std::collections::BTreeSet::new();
        for y in 12..28i32 {
            for x in 12..28i32 {
                seen.insert(px.get(x, y).r);
            }
        }
        assert!(seen.len() > 1, "the pattern came out flat");
        // ...and nothing lands outside it.
        assert_eq!(px.get(4, 4).a, 0);
    }

    #[test]
    fn effects_are_ordered_back_to_front() {
        let mut layer = square();
        layer.effects.drop_shadow.enabled = true;
        layer.effects.color_overlay.enabled = true;
        layer.effects.stroke.enabled = true;

        let rendered = render(&layer, 40, 40, canvas());
        assert_eq!(rendered.len(), 3);
        // Everything below the layer comes first, so the compositor can draw
        // the list in order around it.
        assert!(!rendered[0].above);
        assert!(rendered[1].above);
        assert!(rendered[2].above);
    }

    #[test]
    fn a_region_render_matches_the_whole_canvas() {
        // Effects are rendered a region at a time during a brush stroke; a
        // shadow cast from outside the region has to arrive in it all the same.
        let mut layer = square();
        layer.effects.drop_shadow.enabled = true;
        layer.effects.drop_shadow.size = 4.0;
        layer.effects.drop_shadow.distance = 6.0;

        let whole = render(&layer, 40, 40, canvas());
        let patch = Rect::new(28, 28, 8, 8);
        let partial = render(&layer, 40, 40, patch);

        for y in patch.y..patch.bottom() {
            for x in patch.x..patch.right() {
                assert_eq!(
                    partial[0].pixels.get(x, y),
                    whole[0].pixels.get(x, y),
                    "region render differs at {x},{y}"
                );
            }
        }
    }

    #[test]
    fn a_layer_mask_hides_the_effects_too() {
        let mut layer = square();
        layer.effects.color_overlay.enabled = true;
        let mut mask = Pixmap::new(40, 40);
        // Reveal only the left half of the square.
        for y in 0..40i32 {
            for x in 0..20i32 {
                mask.set(x, y, Rgba8::WHITE);
            }
        }
        layer.mask = Some(mask);

        let rendered = render(&layer, 40, 40, canvas());
        let px = &rendered[0].pixels;
        assert!(px.get(15, 20).a > 0);
        assert_eq!(px.get(25, 20).a, 0, "the effect ignored the layer mask");
    }

    #[test]
    fn fill_opacity_leaves_effects_alone() {
        // Photoshop's shadow-only trick: Fill at 0 hides the layer's pixels and
        // keeps its shadow.
        let mut layer = square();
        layer.effects.drop_shadow.enabled = true;
        layer.fill_opacity = 0.0;

        let rendered = render(&layer, 40, 40, canvas());
        assert!(rendered[0].pixels.get(29, 30).a > 0);
    }

    #[test]
    fn master_opacity_fades_effects() {
        let shadow_alpha = |opacity: f32| {
            let mut layer = square();
            layer.effects.drop_shadow.enabled = true;
            layer.effects.drop_shadow.size = 0.0;
            layer.opacity = opacity;
            render(&layer, 40, 40, canvas())[0].pixels.get(29, 30).a
        };
        assert!(shadow_alpha(0.5) < shadow_alpha(1.0));
    }
}

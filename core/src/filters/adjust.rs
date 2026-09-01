//! Per-pixel colour adjustments.
//!
//! Every variant is a pure function of a single pixel, which is what lets the
//! compositor evaluate adjustment layers on the fly instead of baking them in.

use crate::buffer::{Pixmap, Rgba8};

/// A colour transform from the Image ▸ Adjustments menu.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Adjustment {
    /// `brightness` and `contrast` are both `-1.0..=1.0`.
    BrightnessContrast { brightness: f32, contrast: f32 },
    /// `hue` is `-1.0..=1.0` (one full turn), the others `-1.0..=1.0`.
    HueSaturation {
        hue: f32,
        saturation: f32,
        lightness: f32,
        /// Which band of hues to move: 0 = Master (everything), then Reds,
        /// Yellows, Greens, Cyans, Blues, Magentas — the order of CS6's
        /// dropdown. Anything else is treated as Master.
        range: u8,
    },
    /// Input black/white points and gamma, then output black/white.
    Levels {
        in_black: f32,
        in_white: f32,
        gamma: f32,
        out_black: f32,
        out_white: f32,
    },
    Invert,
    /// Weighted desaturation using Rec. 601 luma, as Photoshop does.
    Desaturate,
    /// Photoshop's Image > Adjustments > Desaturate: HSL lightness,
    /// `(max + min) / 2`. Equivalent to Hue/Saturation with Saturation
    /// at -100, and deliberately *not* the same as [`Adjustment::Desaturate`]
    /// above, which is luma-weighted and backs Black & White.
    DesaturateLightness,
    /// Reduce to `levels` steps per channel.
    Posterize { levels: u32 },
    /// Hard cut at `threshold` (0..=255) on luma.
    Threshold { level: u8 },
    /// Multiply each channel independently.
    ColorBalance { r: f32, g: f32, b: f32 },
    Exposure { exposure: f32, offset: f32, gamma: f32 },
    /// `vibrance` is `-1.0..=1.0`, `saturation` is `-1.0..=1.0`.
    /// Vibrance selectively boosts less-saturated colours more.
    Vibrance { vibrance: f32, saturation: f32 },
    /// A curve, as a 256-entry lookup table, applied to one channel or to all
    /// three. This is what the Curves dialog draws — and what a Curves
    /// adjustment layer carries, so the curve stays editable rather than being
    /// baked into pixels.
    Curves { lut: [u8; 256], channel: u8 },
    /// A wash of colour over the image — Photoshop's Photo Filter. `density`
    /// is `0.0..=1.0`; preserving luminosity scales the result back to the
    /// brightness it started with, so a filter tints without darkening.
    PhotoFilter {
        color: [f32; 3],
        density: f32,
        preserve_luminosity: bool,
    },
    /// Each output channel mixed from all three inputs — Photoshop's Channel
    /// Mixer. `matrix` is row-major `[rr, rg, rb, gr, gg, gb, br, bg, bb]` as
    /// fractions, `constant` is the per-channel offset, and `monochrome` sends
    /// the red row's mix to all three outputs.
    ChannelMixer {
        matrix: [f32; 9],
        constant: [f32; 3],
        monochrome: bool,
    },
    /// Every tone replaced by the colour at that point along a ramp —
    /// Photoshop's Gradient Map. The ramp is baked to 256 samples so the
    /// adjustment stays a plain value the compositor can evaluate per pixel.
    GradientMap { ramp: [[u8; 3]; 256] },
    /// Per-colour-range CMYK nudges — Photoshop's Selective Color. Nine
    /// ranges (reds, yellows, greens, cyans, blues, magentas, whites,
    /// neutrals, blacks), each with cyan, magenta, yellow and black in
    /// `-1.0..=1.0`.
    SelectiveColor {
        ranges: [[f32; 4]; 9],
        /// Relative scales each nudge by how much of that ink is already
        /// there; absolute adds it outright.
        relative: bool,
    },
    /// A named look, applied as three channel tables — a cut-down Color
    /// Lookup.
    ///
    /// Photoshop's reads 3D LUT files (`.cube`, `.3dl`), which map a colour to
    /// any other colour. These are 1D per channel: they can warm, cool, crush
    /// or lift, but not swap a hue for an unrelated one. The presets are the
    /// engine's own for that reason — Adobe's `.look` files are Adobe's.
    ColorLookup { tables: [[u8; 256]; 3] },
    /// Colorize: desaturate then tint to an absolute hue and saturation.
    /// `hue` is `0.0..=1.0` (fraction of 360°), `saturation` `0.0..=1.0`,
    /// `lightness` `-1.0..=1.0`.
    Colorize { hue: f32, saturation: f32, lightness: f32 },
}

impl Default for Adjustment {
    fn default() -> Self {
        Adjustment::BrightnessContrast {
            brightness: 0.0,
            contrast: 0.0,
        }
    }
}

/// How much a pixel belongs to each of Selective Color's nine ranges.
///
/// Shared by the destructive adjustment and the layer, so the two cannot drift
/// apart on what counts as a "red".
pub fn selective_color_weights(r: f32, g: f32, b: f32, max: f32, min: f32) -> [f32; 9] {
    // Ranges: 0=Reds, 1=Yellows, 2=Greens, 3=Cyans, 4=Blues, 5=Magentas,
    //         6=Whites, 7=Neutrals, 8=Blacks
    let mut w = [0.0f32; 9];

    // Chromatic ranges — weight by how dominant the hue is
    let chroma = max - min;
    if chroma > 0.0 {
        // Reds: R dominant, hue near 0° or 360°
        if r >= g && r >= b {
            // Red is max
            if g >= b {
                // hue 0–60° (red–yellow)
                let red_w = (chroma * (1.0 - (g - b) / chroma)).min(chroma);
                w[0] = red_w;   // Reds
                w[1] = chroma - red_w; // Yellows
            } else {
                // hue 300–360° (magenta–red)
                let red_w = (chroma * (1.0 - (b - g) / chroma)).min(chroma);
                w[0] = red_w;   // Reds
                w[5] = chroma - red_w; // Magentas
            }
        } else if g >= r && g >= b {
            // Green is max
            if r >= b {
                // hue 60–120° (yellow–green)
                let grn_w = (chroma * (1.0 - (r - b) / chroma)).min(chroma);
                w[2] = grn_w;   // Greens
                w[1] = chroma - grn_w; // Yellows
            } else {
                // hue 120–180° (green–cyan)
                let grn_w = (chroma * (1.0 - (b - r) / chroma)).min(chroma);
                w[2] = grn_w;   // Greens
                w[3] = chroma - grn_w; // Cyans
            }
        } else {
            // Blue is max
            if g >= r {
                // hue 180–240° (cyan–blue)
                let blu_w = (chroma * (1.0 - (g - r) / chroma)).min(chroma);
                w[4] = blu_w;   // Blues
                w[3] = chroma - blu_w; // Cyans
            } else {
                // hue 240–300° (blue–magenta)
                let blu_w = (chroma * (1.0 - (r - g) / chroma)).min(chroma);
                w[4] = blu_w;   // Blues
                w[5] = chroma - blu_w; // Magentas
            }
        }
    }

    // Tonal ranges — Whites, Neutrals, Blacks
    // These use the min-of-RGB approach that Photoshop uses.
    w[6] = min;                         // Whites
    w[8] = 1.0 - max;                   // Blacks
    w[7] = 1.0 - (w[6] + w[8] + chroma).min(1.0); // Neutrals

    w
}

/// How much a hue belongs to one of Hue/Saturation's six colour ranges.
///
/// Master (and any range the caller invents) is 1.0 everywhere. The others are
/// full strength within 15° of their centre and feather to nothing by 45°,
/// which is where CS6 puts the outer handles of the range it draws between the
/// two spectrum bars. `hue` is a fraction of a turn, as [`rgb_to_hsl`] returns.
pub fn hue_range_weight(range: u8, hue: f32) -> f32 {
    const INNER: f32 = 15.0 / 360.0;
    const OUTER: f32 = 45.0 / 360.0;

    let centre = match range {
        1 => 0.0,           // Reds
        2 => 60.0 / 360.0,  // Yellows
        3 => 120.0 / 360.0, // Greens
        4 => 180.0 / 360.0, // Cyans
        5 => 240.0 / 360.0, // Blues
        6 => 300.0 / 360.0, // Magentas
        _ => return 1.0,    // Master
    };

    // Distance the short way round the wheel, so red at 0.99 is near red at 0.
    let mut distance = (hue - centre).abs();
    if distance > 0.5 {
        distance = 1.0 - distance;
    }

    if distance <= INNER {
        1.0
    } else if distance <= OUTER {
        1.0 - (distance - INNER) / (OUTER - INNER)
    } else {
        0.0
    }
}

/// A black-to-white ramp, sampled at every level.
pub fn greyscale_ramp() -> [[u8; 3]; 256] {
    let mut ramp = [[0u8; 3]; 256];
    for (i, stop) in ramp.iter_mut().enumerate() {
        *stop = [i as u8; 3];
    }
    ramp
}

/// A flag as the number [`Adjustment::value`] hands back for one.
fn flag(on: bool) -> f32 {
    if on {
        1.0
    } else {
        0.0
    }
}

/// Split an indexed parameter key — `"matrix4"` against `"matrix"` is `Some(4)`.
/// Out-of-range indices are rejected rather than clamped: a key naming an
/// element that does not exist is a caller's mistake, not a value to guess at.
fn indexed(key: &str, prefix: &str, count: usize) -> Option<usize> {
    let index: usize = key.strip_prefix(prefix)?.parse().ok()?;
    (index < count).then_some(index)
}

/// Split a Selective Color key — `"range3.magenta"` is range 3, ink 1.
fn selective_key(key: &str) -> Option<(usize, usize)> {
    let (range, ink) = key.split_once('.')?;
    let range = indexed(range, "range", 9)?;
    let ink = match ink {
        "cyan" => 0,
        "magenta" => 1,
        "yellow" => 2,
        "black" => 3,
        _ => return None,
    };
    Some((range, ink))
}

/// A lookup table that maps every level to itself.
pub fn identity_lut() -> [u8; 256] {
    let mut lut = [0u8; 256];
    for (i, v) in lut.iter_mut().enumerate() {
        *v = i as u8;
    }
    lut
}

/// The named looks [`Adjustment::ColorLookup`] offers, in menu order.
///
/// Photoshop's list is its shipped `.look` and `.cube` files, which are
/// Adobe's; these are the engine's own, built from the curves a 1D table can
/// express (see the variant's documentation for what that rules out).
pub const COLOR_LOOKUP_PRESETS: [&str; 7] = [
    "None",
    "Warm Contrast",
    "Cool Shadows",
    "Faded Film",
    "Bleach Bypass",
    "Crisp Warm",
    "Moonlight",
];

/// The three channel tables for a named look, or `None` for a name that is not
/// one — the panel offers only names from [`COLOR_LOOKUP_PRESETS`], so a miss
/// means the two lists have drifted apart.
pub fn color_lookup_tables(name: &str) -> Option<[[u8; 256]; 3]> {
    // Each look is a per-channel curve over the input level in 0..1, written
    // as a function rather than a baked table so it stays readable and
    // adjustable — 2304 magic numbers would be neither. The shapes in use are
    // an S-curve about mid-grey for contrast, a power for warmth or coolness,
    // and a lifted black point for the faded ones.
    type Curve = fn(f32) -> f32;
    let curves: [Curve; 3] = match name {
        "None" => return Some([identity_lut(); 3]),
        "Warm Contrast" => [
            |v| clamp01((v - 0.5) * 1.25 + 0.5).powf(0.92),
            |v| clamp01((v - 0.5) * 1.2 + 0.5),
            |v| clamp01((v - 0.5) * 1.2 + 0.5).powf(1.1),
        ],
        "Cool Shadows" => [
            |v| v.powf(1.12),
            |v| v.powf(1.02),
            |v| 0.06 + v.powf(0.9) * 0.94,
        ],
        "Faded Film" => [
            |v| 0.09 + clamp01((v - 0.5) * 0.82 + 0.5) * 0.88,
            |v| 0.08 + clamp01((v - 0.5) * 0.82 + 0.5) * 0.88,
            |v| 0.12 + clamp01((v - 0.5) * 0.8 + 0.5) * 0.84,
        ],
        "Bleach Bypass" => [
            |v| clamp01((v - 0.45) * 1.5 + 0.5),
            |v| clamp01((v - 0.45) * 1.45 + 0.5),
            |v| clamp01((v - 0.45) * 1.4 + 0.5),
        ],
        "Crisp Warm" => [
            |v| clamp01((v - 0.5) * 1.15 + 0.5).powf(0.85),
            |v| clamp01((v - 0.5) * 1.15 + 0.5).powf(0.98),
            |v| clamp01((v - 0.5) * 1.15 + 0.5).powf(1.18),
        ],
        "Moonlight" => [
            |v| v.powf(1.45) * 0.85,
            |v| v.powf(1.25) * 0.92,
            |v| 0.05 + v.powf(0.95) * 0.95,
        ],
        _ => return None,
    };

    let mut tables = [[0u8; 256]; 3];
    for (channel, curve) in curves.iter().enumerate() {
        for level in 0..256 {
            let v = curve(level as f32 / 255.0);
            tables[channel][level] = (clamp01(v) * 255.0).round() as u8;
        }
    }
    Some(tables)
}

impl Adjustment {
    pub fn name(&self) -> &'static str {
        match self {
            Adjustment::BrightnessContrast { .. } => "Brightness/Contrast",
            Adjustment::HueSaturation { .. } => "Hue/Saturation",
            Adjustment::Levels { .. } => "Levels",
            Adjustment::Invert => "Invert",
            Adjustment::Desaturate => "Black & White",
            Adjustment::DesaturateLightness => "Desaturate",
            Adjustment::Posterize { .. } => "Posterize",
            Adjustment::Threshold { .. } => "Threshold",
            Adjustment::ColorBalance { .. } => "Color Balance",
            Adjustment::Exposure { .. } => "Exposure",
            Adjustment::Vibrance { .. } => "Vibrance",
            Adjustment::Curves { .. } => "Curves",
            Adjustment::PhotoFilter { .. } => "Photo Filter",
            Adjustment::ChannelMixer { .. } => "Channel Mixer",
            Adjustment::GradientMap { .. } => "Gradient Map",
            Adjustment::SelectiveColor { .. } => "Selective Color",
            Adjustment::ColorLookup { .. } => "Color Lookup",
            Adjustment::Colorize { .. } => "Colorize",
        }
    }

    /// Sensible starting parameters, matching the defaults each dialog opens
    /// with in Photoshop.
    pub fn default_for(name: &str) -> Option<Adjustment> {
        Some(match name {
            "Brightness/Contrast" => Adjustment::BrightnessContrast {
                brightness: 0.0,
                contrast: 0.0,
            },
            "Hue/Saturation" => Adjustment::HueSaturation {
                hue: 0.0,
                saturation: 0.0,
                lightness: 0.0,
                range: 0,
            },
            "Levels" => Adjustment::Levels {
                in_black: 0.0,
                in_white: 1.0,
                gamma: 1.0,
                out_black: 0.0,
                out_white: 1.0,
            },
            "Invert" => Adjustment::Invert,
            "Black & White" => Adjustment::Desaturate,
            "Desaturate" => Adjustment::DesaturateLightness,
            "Posterize" => Adjustment::Posterize { levels: 4 },
            "Threshold" => Adjustment::Threshold { level: 128 },
            "Color Balance" => Adjustment::ColorBalance {
                r: 1.0,
                g: 1.0,
                b: 1.0,
            },
            "Exposure" => Adjustment::Exposure {
                exposure: 0.0,
                offset: 0.0,
                gamma: 1.0,
            },
            "Vibrance" => Adjustment::Vibrance {
                vibrance: 0.0,
                saturation: 0.0,
            },
            "Curves" => Adjustment::Curves {
                // The straight line: a curve that changes nothing until it is
                // drawn on.
                lut: identity_lut(),
                channel: 0,
            },
            "Photo Filter" => Adjustment::PhotoFilter {
                // CS6 opens on Warming Filter (85) at 25%.
                color: [0.929, 0.541, 0.196],
                density: 0.25,
                preserve_luminosity: true,
            },
            "Channel Mixer" => Adjustment::ChannelMixer {
                // The identity mix: every output takes all of its own input
                // and none of the others, so it starts as a no-op.
                matrix: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
                constant: [0.0; 3],
                monochrome: false,
            },
            "Gradient Map" => Adjustment::GradientMap {
                // Black to white: the map that leaves a greyscale image alone
                // and turns a colour one into its own luminance.
                ramp: greyscale_ramp(),
            },
            "Selective Color" => Adjustment::SelectiveColor {
                ranges: [[0.0; 4]; 9],
                relative: true,
            },
            "Color Lookup" => Adjustment::ColorLookup {
                tables: [identity_lut(); 3],
            },
            "Colorize" => Adjustment::Colorize {
                hue: 0.0,
                saturation: 0.25,
                lightness: 0.0,
            },
            _ => return None,
        })
    }

    /// Read one named parameter, for a panel that edits an adjustment layer in
    /// place.
    ///
    /// This and [`Adjustment::set_value`] are what let the Properties panel
    /// have one editing path instead of eighteen: it looks up the controls an
    /// adjustment gets, and each control knows only its own key. `None` means
    /// this adjustment has no such parameter — which is also how the panel
    /// tells that a control does not belong on the page.
    ///
    /// Indexed keys are `matrix0`…`matrix8` and `constant0`…`constant2` for
    /// the Channel Mixer, and `range<0-8>.<cyan|magenta|yellow|black>` for
    /// Selective Color. Flags read back as 0 or 1.
    pub fn value(&self, key: &str) -> Option<f32> {
        match self {
            Adjustment::BrightnessContrast {
                brightness,
                contrast,
            } => match key {
                "brightness" => Some(*brightness),
                "contrast" => Some(*contrast),
                _ => None,
            },

            Adjustment::HueSaturation {
                hue,
                saturation,
                lightness,
                range,
            } => match key {
                "hue" => Some(*hue),
                "saturation" => Some(*saturation),
                "lightness" => Some(*lightness),
                "range" => Some(*range as f32),
                "colorize" => Some(0.0),
                _ => None,
            },

            Adjustment::Colorize {
                hue,
                saturation,
                lightness,
            } => match key {
                "hue" => Some(*hue),
                "saturation" => Some(*saturation),
                "lightness" => Some(*lightness),
                // Colorize applies to the whole image, so it has no range of
                // its own; reporting Master keeps the panel's dropdown honest.
                "range" => Some(0.0),
                "colorize" => Some(1.0),
                _ => None,
            },

            Adjustment::Levels {
                in_black,
                in_white,
                gamma,
                out_black,
                out_white,
            } => match key {
                "inBlack" => Some(*in_black),
                "inWhite" => Some(*in_white),
                "gamma" => Some(*gamma),
                "outBlack" => Some(*out_black),
                "outWhite" => Some(*out_white),
                _ => None,
            },

            Adjustment::Posterize { levels } => match key {
                "levels" => Some(*levels as f32),
                _ => None,
            },

            Adjustment::Threshold { level } => match key {
                "level" => Some(*level as f32),
                _ => None,
            },

            Adjustment::ColorBalance { r, g, b } => match key {
                "red" => Some(*r),
                "green" => Some(*g),
                "blue" => Some(*b),
                _ => None,
            },

            Adjustment::Exposure {
                exposure,
                offset,
                gamma,
            } => match key {
                "exposure" => Some(*exposure),
                "offset" => Some(*offset),
                "gamma" => Some(*gamma),
                _ => None,
            },

            Adjustment::Vibrance {
                vibrance,
                saturation,
            } => match key {
                "vibrance" => Some(*vibrance),
                "saturation" => Some(*saturation),
                _ => None,
            },

            Adjustment::Curves { channel, .. } => match key {
                // The table itself is not a scalar; it arrives through
                // `Document::set_layer_adjustment` from the curve editor.
                "channel" => Some(*channel as f32),
                _ => None,
            },

            Adjustment::PhotoFilter {
                color,
                density,
                preserve_luminosity,
            } => match key {
                "red" => Some(color[0]),
                "green" => Some(color[1]),
                "blue" => Some(color[2]),
                "density" => Some(*density),
                "preserveLuminosity" => Some(flag(*preserve_luminosity)),
                _ => None,
            },

            Adjustment::ChannelMixer {
                matrix,
                constant,
                monochrome,
            } => {
                if let Some(i) = indexed(key, "matrix", 9) {
                    return Some(matrix[i]);
                }
                if let Some(i) = indexed(key, "constant", 3) {
                    return Some(constant[i]);
                }
                match key {
                    "monochrome" => Some(flag(*monochrome)),
                    _ => None,
                }
            }

            Adjustment::SelectiveColor { ranges, relative } => {
                if let Some((range, ink)) = selective_key(key) {
                    return Some(ranges[range][ink]);
                }
                match key {
                    "relative" => Some(flag(*relative)),
                    _ => None,
                }
            }

            // Nothing to set: Invert, the two desaturations, and the two that
            // carry a whole table rather than parameters.
            Adjustment::Invert
            | Adjustment::Desaturate
            | Adjustment::DesaturateLightness
            | Adjustment::GradientMap { .. }
            | Adjustment::ColorLookup { .. } => None,
        }
    }

    /// Write one named parameter. False if this adjustment has no such key,
    /// having changed nothing.
    ///
    /// `colorize` switches between [`Adjustment::HueSaturation`] and
    /// [`Adjustment::Colorize`], which is what CS6's checkbox does: the two are
    /// one dialog with two behaviours, not two adjustments. The three sliders
    /// carry across, and the caller is expected to push its own values
    /// afterwards — the two read their hue on different scales.
    pub fn set_value(&mut self, key: &str, v: f32) -> bool {
        match self {
            Adjustment::BrightnessContrast {
                brightness,
                contrast,
            } => match key {
                "brightness" => {
                    *brightness = v;
                    true
                }
                "contrast" => {
                    *contrast = v;
                    true
                }
                _ => false,
            },

            Adjustment::HueSaturation {
                hue,
                saturation,
                lightness,
                range,
            } => match key {
                "hue" => {
                    *hue = v;
                    true
                }
                "saturation" => {
                    *saturation = v;
                    true
                }
                "lightness" => {
                    *lightness = v;
                    true
                }
                "range" => {
                    *range = v.clamp(0.0, 6.0) as u8;
                    true
                }
                "colorize" => {
                    if v != 0.0 {
                        *self = Adjustment::Colorize {
                            hue: hue.rem_euclid(1.0),
                            saturation: saturation.max(0.0),
                            lightness: *lightness,
                        };
                    }
                    true
                }
                _ => false,
            },

            Adjustment::Colorize {
                hue,
                saturation,
                lightness,
            } => match key {
                "hue" => {
                    *hue = v;
                    true
                }
                "saturation" => {
                    *saturation = v;
                    true
                }
                "lightness" => {
                    *lightness = v;
                    true
                }
                "colorize" => {
                    if v == 0.0 {
                        *self = Adjustment::HueSaturation {
                            hue: 0.0,
                            saturation: *saturation,
                            lightness: *lightness,
                            range: 0,
                        };
                    }
                    true
                }
                // Accepted and ignored: Colorize has only the one range, and
                // refusing would make the panel look broken as the dropdown
                // moves.
                "range" => true,
                _ => false,
            },

            Adjustment::Levels {
                in_black,
                in_white,
                gamma,
                out_black,
                out_white,
            } => match key {
                // The black point is held below the white one: crossed over,
                // the span goes negative and the image turns to noise.
                "inBlack" => {
                    *in_black = v.clamp(0.0, *in_white - 0.004);
                    true
                }
                "inWhite" => {
                    *in_white = v.clamp(*in_black + 0.004, 1.0);
                    true
                }
                "gamma" => {
                    *gamma = v.clamp(0.01, 9.99);
                    true
                }
                "outBlack" => {
                    *out_black = v.clamp(0.0, 1.0);
                    true
                }
                "outWhite" => {
                    *out_white = v.clamp(0.0, 1.0);
                    true
                }
                _ => false,
            },

            Adjustment::Posterize { levels } => match key {
                "levels" => {
                    *levels = v.clamp(2.0, 255.0) as u32;
                    true
                }
                _ => false,
            },

            Adjustment::Threshold { level } => match key {
                "level" => {
                    *level = v.clamp(1.0, 255.0) as u8;
                    true
                }
                _ => false,
            },

            Adjustment::ColorBalance { r, g, b } => match key {
                "red" => {
                    *r = v;
                    true
                }
                "green" => {
                    *g = v;
                    true
                }
                "blue" => {
                    *b = v;
                    true
                }
                _ => false,
            },

            Adjustment::Exposure {
                exposure,
                offset,
                gamma,
            } => match key {
                "exposure" => {
                    *exposure = v;
                    true
                }
                "offset" => {
                    *offset = v;
                    true
                }
                "gamma" => {
                    *gamma = v.max(0.01);
                    true
                }
                _ => false,
            },

            Adjustment::Vibrance {
                vibrance,
                saturation,
            } => match key {
                "vibrance" => {
                    *vibrance = v;
                    true
                }
                "saturation" => {
                    *saturation = v;
                    true
                }
                _ => false,
            },

            Adjustment::Curves { channel, .. } => match key {
                "channel" => {
                    *channel = v.clamp(0.0, 3.0) as u8;
                    true
                }
                _ => false,
            },

            Adjustment::PhotoFilter {
                color,
                density,
                preserve_luminosity,
            } => match key {
                "red" => {
                    color[0] = clamp01(v);
                    true
                }
                "green" => {
                    color[1] = clamp01(v);
                    true
                }
                "blue" => {
                    color[2] = clamp01(v);
                    true
                }
                "density" => {
                    *density = clamp01(v);
                    true
                }
                "preserveLuminosity" => {
                    *preserve_luminosity = v != 0.0;
                    true
                }
                _ => false,
            },

            Adjustment::ChannelMixer {
                matrix,
                constant,
                monochrome,
            } => {
                if let Some(i) = indexed(key, "matrix", 9) {
                    matrix[i] = v;
                    return true;
                }
                if let Some(i) = indexed(key, "constant", 3) {
                    constant[i] = v;
                    return true;
                }
                match key {
                    "monochrome" => {
                        *monochrome = v != 0.0;
                        true
                    }
                    _ => false,
                }
            }

            Adjustment::SelectiveColor { ranges, relative } => {
                if let Some((range, ink)) = selective_key(key) {
                    ranges[range][ink] = v.clamp(-1.0, 1.0);
                    return true;
                }
                match key {
                    "relative" => {
                        *relative = v != 0.0;
                        true
                    }
                    _ => false,
                }
            }

            Adjustment::Invert
            | Adjustment::Desaturate
            | Adjustment::DesaturateLightness
            | Adjustment::GradientMap { .. }
            | Adjustment::ColorLookup { .. } => false,
        }
    }

    /// Transform one straight-alpha RGB triple in `[0, 1]`.
    ///
    /// Alpha is never touched — adjustments change colour, not coverage.
    pub fn apply_rgb(&self, c: [f32; 3]) -> [f32; 3] {
        match *self {
            Adjustment::BrightnessContrast {
                brightness,
                contrast,
            } => {
                // Contrast pivots around mid-grey so it does not shift overall
                // exposure. The `1 + contrast` slope maps -1 to flat grey and
                // +1 to double slope.
                let slope = 1.0 + contrast.clamp(-1.0, 1.0);
                map3(c, |v| clamp01((v - 0.5) * slope + 0.5 + brightness))
            }

            Adjustment::Curves { lut, channel } => {
                // The table is in 0..255, so the value goes out to that scale,
                // through the curve, and back. Channel 0 is the composite
                // curve and applies to all three, as it does in the dialog.
                let through = |v: f32| {
                    let i = (v.clamp(0.0, 1.0) * 255.0).round() as usize;
                    lut[i.min(255)] as f32 / 255.0
                };
                let mut out = c;
                match channel {
                    1 => out[0] = through(c[0]),
                    2 => out[1] = through(c[1]),
                    3 => out[2] = through(c[2]),
                    _ => out = map3(c, through),
                }
                out
            }

            Adjustment::PhotoFilter {
                color,
                density,
                preserve_luminosity,
            } => {
                let d = density.clamp(0.0, 1.0);
                let mut out = [
                    c[0] * (1.0 - d) + color[0] * d,
                    c[1] * (1.0 - d) + color[1] * d,
                    c[2] * (1.0 - d) + color[2] * d,
                ];
                if preserve_luminosity {
                    let before = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
                    let after = 0.299 * out[0] + 0.587 * out[1] + 0.114 * out[2];
                    if after > 1e-6 {
                        let scale = before / after;
                        out = map3(out, |v| v * scale);
                    }
                }
                map3(out, clamp01)
            }

            Adjustment::ChannelMixer {
                matrix,
                constant,
                monochrome,
            } => {
                let mix = |row: usize| {
                    c[0] * matrix[row * 3] + c[1] * matrix[row * 3 + 1]
                        + c[2] * matrix[row * 3 + 2]
                        + constant[row]
                };
                if monochrome {
                    // Monochrome takes the red row's mix for all three, which
                    // is how CS6 turns the panel into a black-and-white mixer.
                    let grey = clamp01(mix(0));
                    [grey, grey, grey]
                } else {
                    [clamp01(mix(0)), clamp01(mix(1)), clamp01(mix(2))]
                }
            }

            Adjustment::GradientMap { ramp } => {
                // Position along the ramp is the pixel's luminance, the same
                // Rec.601 grey the rest of the engine uses.
                let luma = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
                let i = (luma.clamp(0.0, 1.0) * 255.0).round() as usize;
                let stop = ramp[i.min(255)];
                [
                    stop[0] as f32 / 255.0,
                    stop[1] as f32 / 255.0,
                    stop[2] as f32 / 255.0,
                ]
            }

            Adjustment::SelectiveColor { ranges, relative } => {
                // Worked in CMY, as Photoshop's panel is — the same arithmetic
                // the destructive version does, from the same weights.
                let (mut cy, mut mg, mut yl) = (1.0 - c[0], 1.0 - c[1], 1.0 - c[2]);
                let max = c[0].max(c[1]).max(c[2]);
                let min = c[0].min(c[1]).min(c[2]);
                let weights = selective_color_weights(c[0], c[1], c[2], max, min);

                for (range, weight) in weights.iter().enumerate() {
                    if *weight <= 0.0 {
                        continue;
                    }
                    let [dc, dm, dy, dk] = ranges[range];
                    if dc == 0.0 && dm == 0.0 && dy == 0.0 && dk == 0.0 {
                        continue;
                    }
                    let w = *weight;
                    if relative {
                        cy += (dc * cy + dk * cy) * w;
                        mg += (dm * mg + dk * mg) * w;
                        yl += (dy * yl + dk * yl) * w;
                    } else {
                        cy += (dc + dk) * w;
                        mg += (dm + dk) * w;
                        yl += (dy + dk) * w;
                    }
                }
                [
                    1.0 - cy.clamp(0.0, 1.0),
                    1.0 - mg.clamp(0.0, 1.0),
                    1.0 - yl.clamp(0.0, 1.0),
                ]
            }

            Adjustment::ColorLookup { tables } => {
                let through = |v: f32, table: &[u8; 256]| {
                    let i = (v.clamp(0.0, 1.0) * 255.0).round() as usize;
                    table[i.min(255)] as f32 / 255.0
                };
                [
                    through(c[0], &tables[0]),
                    through(c[1], &tables[1]),
                    through(c[2], &tables[2]),
                ]
            }

            Adjustment::HueSaturation {
                hue,
                saturation,
                lightness,
                range,
            } => {
                let (h, s, l) = rgb_to_hsl(c);
                // Outside the chosen range the pixel is left exactly as it
                // came in — not run through HSL and back, which would cost it
                // a level or two to rounding for no reason.
                let weight = hue_range_weight(range, h);
                if weight <= 0.0 {
                    return c;
                }
                let h = (h + hue * weight).rem_euclid(1.0);
                // Positive saturation pushes toward full saturation
                // asymptotically rather than clipping abruptly.
                let s = if saturation >= 0.0 {
                    s + (1.0 - s) * saturation.min(1.0) * weight
                } else {
                    s * (1.0 + saturation.max(-1.0) * weight)
                };
                let l = if lightness >= 0.0 {
                    l + (1.0 - l) * lightness.min(1.0) * weight
                } else {
                    l * (1.0 + lightness.max(-1.0) * weight)
                };
                hsl_to_rgb(h, clamp01(s), clamp01(l))
            }

            Adjustment::Levels {
                in_black,
                in_white,
                gamma,
                out_black,
                out_white,
            } => {
                let span = (in_white - in_black).max(1e-6);
                let inv_gamma = 1.0 / gamma.max(1e-6);
                map3(c, |v| {
                    let n = clamp01((v - in_black) / span).powf(inv_gamma);
                    clamp01(out_black + n * (out_white - out_black))
                })
            }

            Adjustment::Invert => map3(c, |v| 1.0 - v),

            Adjustment::Desaturate => {
                let l = luma(c);
                [l, l, l]
            }

            Adjustment::DesaturateLightness => {
                let max = c[0].max(c[1]).max(c[2]);
                let min = c[0].min(c[1]).min(c[2]);
                let l = (max + min) * 0.5;
                [l, l, l]
            }

            Adjustment::Posterize { levels } => {
                let n = levels.max(2) as f32;
                // Quantise to `levels` evenly spaced buckets across [0,1].
                map3(c, |v| ((v * (n - 1.0)).round() / (n - 1.0)).clamp(0.0, 1.0))
            }

            Adjustment::Threshold { level } => {
                let t = level as f32 / 255.0;
                let v = if luma(c) >= t { 1.0 } else { 0.0 };
                [v, v, v]
            }

            Adjustment::ColorBalance { r, g, b } => [
                clamp01(c[0] * r),
                clamp01(c[1] * g),
                clamp01(c[2] * b),
            ],

            Adjustment::Exposure {
                exposure,
                offset,
                gamma,
            } => {
                let gain = 2f32.powf(exposure);
                let inv_gamma = 1.0 / gamma.max(1e-6);
                map3(c, |v| clamp01((v * gain + offset).max(0.0).powf(inv_gamma)))
            }

            Adjustment::Vibrance {
                vibrance,
                saturation,
            } => {
                let max = c[0].max(c[1]).max(c[2]);
                let min = c[0].min(c[1]).min(c[2]);
                let cur_sat = if max > 1e-6 { 1.0 - min / max } else { 0.0 };
                // Vibrance: scale factor rises as current saturation falls.
                let vib_scale = vibrance * (1.0 - cur_sat);
                let total = 1.0 + vib_scale + saturation;
                let l = luma(c);
                [
                    clamp01(l + (c[0] - l) * total),
                    clamp01(l + (c[1] - l) * total),
                    clamp01(l + (c[2] - l) * total),
                ]
            }

            Adjustment::Colorize {
                hue,
                saturation,
                lightness,
            } => {
                let (_, _, l) = rgb_to_hsl(c);
                let l = if lightness >= 0.0 {
                    l + (1.0 - l) * lightness.min(1.0)
                } else {
                    l * (1.0 + lightness.max(-1.0))
                };
                hsl_to_rgb(hue.rem_euclid(1.0), clamp01(saturation), clamp01(l))
            }
        }
    }

    /// Apply destructively to every pixel of a [`Pixmap`].
    pub fn apply_to(&self, pixmap: &mut Pixmap) {
        for px in pixmap.as_bytes_mut().chunks_exact_mut(4) {
            if px[3] == 0 {
                continue;
            }
            let c = [
                px[0] as f32 / 255.0,
                px[1] as f32 / 255.0,
                px[2] as f32 / 255.0,
            ];
            let out = self.apply_rgb(c);
            px[0] = to_u8(out[0]);
            px[1] = to_u8(out[1]);
            px[2] = to_u8(out[2]);
        }
    }

    /// Convenience wrapper for a single 8-bit pixel.
    pub fn apply_pixel(&self, p: Rgba8) -> Rgba8 {
        if p.a == 0 {
            return p;
        }
        let out = self.apply_rgb([
            p.r as f32 / 255.0,
            p.g as f32 / 255.0,
            p.b as f32 / 255.0,
        ]);
        Rgba8::new(to_u8(out[0]), to_u8(out[1]), to_u8(out[2]), p.a)
    }
}

#[inline]
fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

#[inline]
fn to_u8(v: f32) -> u8 {
    (clamp01(v) * 255.0 + 0.5) as u8
}

#[inline]
fn map3(c: [f32; 3], f: impl Fn(f32) -> f32) -> [f32; 3] {
    [f(c[0]), f(c[1]), f(c[2])]
}

/// Rec. 601 luma — the weighting Photoshop uses for greyscale conversion.
#[inline]
fn luma(c: [f32; 3]) -> f32 {
    0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
}

/// RGB → HSL, all components in `[0, 1]` (hue wraps at 1.0).
pub fn rgb_to_hsl(c: [f32; 3]) -> (f32, f32, f32) {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    let l = (max + min) / 2.0;
    let delta = max - min;

    if delta < 1e-6 {
        // Achromatic: hue is undefined, report 0.
        return (0.0, 0.0, l);
    }

    let s = if l > 0.5 {
        delta / (2.0 - max - min)
    } else {
        delta / (max + min)
    };

    let h = if max == c[0] {
        ((c[1] - c[2]) / delta).rem_euclid(6.0)
    } else if max == c[1] {
        (c[2] - c[0]) / delta + 2.0
    } else {
        (c[0] - c[1]) / delta + 4.0
    } / 6.0;

    (h.rem_euclid(1.0), s.clamp(0.0, 1.0), l)
}

/// HSL → RGB, inverse of [`rgb_to_hsl`].
pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    if s < 1e-6 {
        return [l, l, l];
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    [
        hue_to_channel(p, q, h + 1.0 / 3.0),
        hue_to_channel(p, q, h),
        hue_to_channel(p, q, h - 1.0 / 3.0),
    ]
}

fn hue_to_channel(p: f32, q: f32, t: f32) -> f32 {
    let t = t.rem_euclid(1.0);
    if t < 1.0 / 6.0 {
        p + (q - p) * 6.0 * t
    } else if t < 0.5 {
        q
    } else if t < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - t) * 6.0
    } else {
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-3;

    fn close3(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < EPS)
    }

    #[test]
    fn identity_parameters_are_no_ops() {
        let c = [0.3, 0.6, 0.9];
        let identities = [
            Adjustment::BrightnessContrast {
                brightness: 0.0,
                contrast: 0.0,
            },
            Adjustment::HueSaturation {
                hue: 0.0,
                saturation: 0.0,
                lightness: 0.0,
                range: 0,
            },
            Adjustment::Levels {
                in_black: 0.0,
                in_white: 1.0,
                gamma: 1.0,
                out_black: 0.0,
                out_white: 1.0,
            },
            Adjustment::ColorBalance {
                r: 1.0,
                g: 1.0,
                b: 1.0,
            },
            Adjustment::Exposure {
                exposure: 0.0,
                offset: 0.0,
                gamma: 1.0,
            },
            Adjustment::Vibrance {
                vibrance: 0.0,
                saturation: 0.0,
            },
        ];
        for a in identities {
            assert!(close3(a.apply_rgb(c), c), "{:?} was not identity", a);
        }
    }

    #[test]
    fn invert_is_its_own_inverse() {
        let c = [0.2, 0.5, 0.8];
        let once = Adjustment::Invert.apply_rgb(c);
        let twice = Adjustment::Invert.apply_rgb(once);
        assert!(close3(twice, c));
    }

    #[test]
    fn hsl_roundtrips() {
        let cases = [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.25, 0.5, 0.75],
            [0.5, 0.5, 0.5],
            [1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0],
        ];
        for c in cases {
            let (h, s, l) = rgb_to_hsl(c);
            assert!(close3(hsl_to_rgb(h, s, l), c), "failed for {:?}", c);
        }
    }

    #[test]
    fn hue_shift_of_one_full_turn_is_identity() {
        let c = [0.8, 0.3, 0.1];
        let a = Adjustment::HueSaturation {
            hue: 1.0,
            saturation: 0.0,
            lightness: 0.0,
            range: 0,
        };
        assert!(close3(a.apply_rgb(c), c));
    }

    #[test]
    fn desaturate_makes_all_channels_equal() {
        let out = Adjustment::Desaturate.apply_rgb([0.9, 0.2, 0.4]);
        assert!((out[0] - out[1]).abs() < EPS);
        assert!((out[1] - out[2]).abs() < EPS);
    }

    #[test]
    fn threshold_is_binary() {
        let a = Adjustment::Threshold { level: 128 };
        for c in [[0.0, 0.0, 0.0], [0.4, 0.4, 0.4], [1.0, 1.0, 1.0]] {
            let out = a.apply_rgb(c);
            assert!(out[0] == 0.0 || out[0] == 1.0);
        }
        assert_eq!(a.apply_rgb([1.0, 1.0, 1.0])[0], 1.0);
        assert_eq!(a.apply_rgb([0.0, 0.0, 0.0])[0], 0.0);
    }

    #[test]
    fn posterize_hits_the_endpoints() {
        let a = Adjustment::Posterize { levels: 4 };
        assert!((a.apply_rgb([0.0, 0.0, 0.0])[0] - 0.0).abs() < EPS);
        assert!((a.apply_rgb([1.0, 1.0, 1.0])[0] - 1.0).abs() < EPS);
    }

    #[test]
    fn posterize_guards_against_degenerate_level_counts() {
        // levels < 2 would divide by zero if not clamped.
        for levels in [0u32, 1] {
            let out = Adjustment::Posterize { levels }.apply_rgb([0.37, 0.5, 0.9]);
            assert!(out.iter().all(|v| v.is_finite()));
        }
    }

    #[test]
    fn levels_guards_against_zero_span_and_gamma() {
        let a = Adjustment::Levels {
            in_black: 0.5,
            in_white: 0.5,
            gamma: 0.0,
            out_black: 0.0,
            out_white: 1.0,
        };
        let out = a.apply_rgb([0.5, 0.2, 0.9]);
        assert!(out.iter().all(|v| v.is_finite()), "{:?}", out);
    }

    #[test]
    fn all_adjustments_stay_in_gamut() {
        let extreme = [
            Adjustment::BrightnessContrast {
                brightness: 1.0,
                contrast: 1.0,
            },
            Adjustment::BrightnessContrast {
                brightness: -1.0,
                contrast: -1.0,
            },
            Adjustment::HueSaturation {
                hue: 0.5,
                saturation: 1.0,
                lightness: 1.0,
                range: 0,
            },
            Adjustment::Exposure {
                exposure: 5.0,
                offset: 0.5,
                gamma: 0.1,
            },
            Adjustment::ColorBalance {
                r: 4.0,
                g: 4.0,
                b: 4.0,
            },
        ];
        for a in extreme {
            for c in [[0.0, 0.0, 0.0], [0.5, 0.5, 0.5], [1.0, 1.0, 1.0]] {
                let out = a.apply_rgb(c);
                for v in out {
                    assert!(v.is_finite() && (0.0..=1.0).contains(&v), "{:?} -> {:?}", a, out);
                }
            }
        }
    }

    #[test]
    fn apply_to_skips_transparent_pixels() {
        let mut pm = Pixmap::new(2, 2);
        Adjustment::Invert.apply_to(&mut pm);
        // Inverting a transparent pixel would turn it white if alpha were
        // ignored; it must stay untouched.
        assert!(pm.as_bytes().iter().all(|&b| b == 0));
    }

    #[test]
    fn apply_pixel_preserves_alpha() {
        let p = Rgba8::new(10, 20, 30, 77);
        let out = Adjustment::Invert.apply_pixel(p);
        assert_eq!(out.a, 77);
        assert_eq!(out.r, 245);
    }

    #[test]
    #[test]
    fn a_curve_maps_every_channel_or_just_one() {
        let mut lut = identity_lut();
        // A curve that drives everything to white.
        for v in lut.iter_mut() {
            *v = 255;
        }

        let composite = Adjustment::Curves { lut, channel: 0 };
        assert_eq!(composite.apply_rgb([0.2, 0.4, 0.6]), [1.0, 1.0, 1.0]);

        // Channel 2 is green, and only green moves.
        let green = Adjustment::Curves { lut, channel: 2 };
        let out = green.apply_rgb([0.2, 0.4, 0.6]);
        assert_eq!(out[1], 1.0);
        assert!((out[0] - 0.2).abs() < 0.01);
        assert!((out[2] - 0.6).abs() < 0.01);
    }

    #[test]
    fn the_default_curve_changes_nothing() {
        let flat = Adjustment::default_for("Curves").expect("Curves");
        let before = [0.1, 0.5, 0.9];
        let after = flat.apply_rgb(before);
        for i in 0..3 {
            assert!((after[i] - before[i]).abs() < 0.01, "channel {i} moved");
        }
    }

    #[test]
    fn every_adjustment_layer_the_menu_offers_has_a_default() {
        // The Layer ▸ New Adjustment Layer menu greys an entry the engine
        // cannot make. These are the ones it must be able to.
        for name in [
            "Brightness/Contrast", "Levels", "Curves", "Exposure",
            "Vibrance", "Hue/Saturation", "Color Balance", "Black & White",
            "Photo Filter", "Channel Mixer", "Color Lookup",
            "Invert", "Posterize", "Threshold", "Gradient Map", "Selective Color",
        ] {
            let adjustment = Adjustment::default_for(name)
                .unwrap_or_else(|| panic!("{name} has no adjustment layer"));
            assert_eq!(adjustment.name(), name, "{name} answers to another name");
        }
    }

    #[test]
    fn the_new_adjustments_start_as_no_ops_except_where_cs6_does_not() {
        // A fresh adjustment layer should leave the image alone, so that
        // adding one and then editing it is a smooth path rather than a jolt.
        let probe = [0.2, 0.5, 0.8];
        for name in ["Channel Mixer", "Color Lookup", "Selective Color"] {
            let out = Adjustment::default_for(name).unwrap().apply_rgb(probe);
            for i in 0..3 {
                assert!((out[i] - probe[i]).abs() < 0.01, "{name} moved channel {i}");
            }
        }

        // Photo Filter is the exception: CS6 opens it on a warming filter at
        // 25%, which is a visible change from the moment it is added.
        let filtered = Adjustment::default_for("Photo Filter").unwrap().apply_rgb(probe);
        assert!(filtered[0] > probe[0], "the warming filter should warm");
    }

    #[test]
    fn a_gradient_map_replaces_tone_with_the_ramp() {
        let mut ramp = greyscale_ramp();
        // A map that sends everything to pure blue.
        for stop in ramp.iter_mut() {
            *stop = [0, 0, 255];
        }
        let out = Adjustment::GradientMap { ramp }.apply_rgb([0.9, 0.2, 0.1]);
        assert_eq!(out, [0.0, 0.0, 1.0]);

        // The default black-to-white ramp turns colour into its own luminance.
        let grey = Adjustment::default_for("Gradient Map").unwrap().apply_rgb([1.0, 0.0, 0.0]);
        assert!((grey[0] - 0.299).abs() < 0.01);
        assert_eq!(grey[0], grey[1]);
    }

    #[test]
    fn a_monochrome_channel_mix_sends_one_row_to_all_three() {
        let mixer = Adjustment::ChannelMixer {
            // All red, nothing else.
            matrix: [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            constant: [0.0; 3],
            monochrome: true,
        };
        let out = mixer.apply_rgb([0.6, 0.1, 0.9]);
        assert!((out[0] - 0.6).abs() < 0.01);
        assert_eq!(out[0], out[1]);
        assert_eq!(out[1], out[2]);
    }

    #[test]
    fn a_hue_range_moves_its_own_colours_and_leaves_the_rest() {
        // Reds only, turned a third of the way round the wheel.
        let reds = Adjustment::HueSaturation {
            hue: 1.0 / 3.0,
            saturation: 0.0,
            lightness: 0.0,
            range: 1,
        };
        // Pure red is at the centre of the range, so it moves the whole way.
        let moved = reds.apply_rgb([1.0, 0.0, 0.0]);
        assert!(close3(moved, [0.0, 1.0, 0.0]), "red should become green, got {:?}", moved);
        // Pure blue is 120° from the nearest edge of Reds — untouched, and
        // bit-for-bit so, rather than merely close.
        assert_eq!(reds.apply_rgb([0.0, 0.0, 1.0]), [0.0, 0.0, 1.0]);
        // Master moves everything.
        let mut master = reds;
        master.set_value("range", 0.0);
        assert!(close3(master.apply_rgb([0.0, 0.0, 1.0]), [1.0, 0.0, 0.0]));
    }

    #[test]
    fn a_hue_range_feathers_at_its_edges() {
        // 30° from red is halfway through the feather, so a colour there gets
        // half the shift — the point of the feather being that a range does
        // not end at a visible seam.
        let at = |degrees: f32| hue_range_weight(1, degrees / 360.0);
        assert_eq!(at(0.0), 1.0);
        assert_eq!(at(15.0), 1.0);
        assert!((at(30.0) - 0.5).abs() < EPS);
        assert_eq!(at(45.0), 0.0);
        assert_eq!(at(90.0), 0.0);
        // The wheel wraps: 350° is 10° from red, still full strength.
        assert_eq!(at(350.0), 1.0);
        // Master is everywhere, including the far side of the wheel.
        assert_eq!(hue_range_weight(0, 0.5), 1.0);
    }

    #[test]
    fn hue_saturation_parameters_round_trip_by_name() {
        let mut a = Adjustment::default_for("Hue/Saturation").unwrap();
        assert!(a.set_value("hue", -0.1));
        assert!(a.set_value("saturation", 0.4));
        assert!(a.set_value("range", 3.0));
        assert_eq!(a.value("hue"), Some(-0.1));
        assert_eq!(a.value("saturation"), Some(0.4));
        assert_eq!(a.value("range"), Some(3.0));
        assert_eq!(a.value("colorize"), Some(0.0));

        // A key it has no parameter for changes nothing and says so.
        assert!(!a.set_value("gamma", 2.0));
        assert_eq!(a.value("gamma"), None);
        // Neither does an adjustment with no parameters at all.
        assert_eq!(Adjustment::Invert.value("hue"), None);
    }

    #[test]
    fn colorize_switches_the_adjustment_rather_than_setting_a_flag() {
        let mut a = Adjustment::default_for("Hue/Saturation").unwrap();
        a.set_value("saturation", 0.3);
        a.set_value("lightness", -0.2);

        assert!(a.set_value("colorize", 1.0));
        assert_eq!(a.name(), "Colorize");
        // The sliders carry across, so ticking the box does not lose the work.
        assert_eq!(a.value("saturation"), Some(0.3));
        assert_eq!(a.value("lightness"), Some(-0.2));
        assert_eq!(a.value("colorize"), Some(1.0));

        assert!(a.set_value("colorize", 0.0));
        assert_eq!(a.name(), "Hue/Saturation");
        assert_eq!(a.value("lightness"), Some(-0.2));
    }

    #[test]
    fn default_for_covers_every_named_adjustment() {
        let names = [
            "Brightness/Contrast",
            "Hue/Saturation",
            "Levels",
            "Invert",
            "Black & White",
            "Posterize",
            "Threshold",
            "Color Balance",
            "Exposure",
        ];
        for n in names {
            let a = Adjustment::default_for(n).unwrap_or_else(|| panic!("missing {}", n));
            assert_eq!(a.name(), n);
        }
        assert!(Adjustment::default_for("Nope").is_none());
    }

    #[test]
    fn every_parameter_the_panel_edits_round_trips() {
        // One key per adjustment that has any, with a value it should hand
        // straight back. If a variant grows a parameter, it belongs here — the
        // Properties panel can only edit what these two functions expose.
        let cases: [(&str, &str, f32); 12] = [
            ("Brightness/Contrast", "brightness", 0.4),
            ("Hue/Saturation", "hue", -0.25),
            ("Levels", "gamma", 1.6),
            ("Posterize", "levels", 7.0),
            ("Threshold", "level", 200.0),
            ("Color Balance", "green", 1.3),
            ("Exposure", "exposure", -1.5),
            ("Vibrance", "vibrance", 0.6),
            ("Curves", "channel", 2.0),
            ("Photo Filter", "density", 0.75),
            ("Channel Mixer", "matrix4", 0.5),
            ("Selective Color", "range3.yellow", -0.4),
        ];
        for (name, key, value) in cases {
            let mut a = Adjustment::default_for(name).unwrap_or_else(|| panic!("no {name}"));
            assert!(a.set_value(key, value), "{name} refused {key}");
            assert_eq!(a.value(key), Some(value), "{name} lost {key}");
            assert!(!a.set_value("notAKey", 1.0), "{name} accepted a bad key");
            assert_eq!(a.value("notAKey"), None);
        }

        // Flags come back as 0 or 1 rather than as a separate type, so one
        // control can drive any key.
        let mut mixer = Adjustment::default_for("Channel Mixer").unwrap();
        assert_eq!(mixer.value("monochrome"), Some(0.0));
        assert!(mixer.set_value("monochrome", 1.0));
        assert_eq!(mixer.value("monochrome"), Some(1.0));

        // An index past the end of the array is refused, not clamped onto a
        // neighbour.
        assert!(!mixer.set_value("matrix9", 1.0));
        assert!(!mixer.set_value("constant3", 1.0));
        // And the ones with nothing to set say so.
        assert!(!Adjustment::Invert.set_value("brightness", 1.0));
    }

    #[test]
    fn levels_keeps_its_black_point_below_its_white_one() {
        let mut levels = Adjustment::default_for("Levels").unwrap();
        levels.set_value("inWhite", 0.5);
        // Dragged past the white point, the black point stops short of it —
        // crossed over, the span goes negative and the image turns to noise.
        levels.set_value("inBlack", 0.9);
        let black = levels.value("inBlack").unwrap();
        assert!(black < 0.5, "black point {black} passed the white point");
    }

    #[test]
    fn every_color_lookup_preset_has_tables_and_is_identifiable() {
        for name in COLOR_LOOKUP_PRESETS {
            let tables = color_lookup_tables(name).unwrap_or_else(|| panic!("no {name}"));
            // Every table is a full 256 entries and monotonic-ish at the ends,
            // so a look cannot invert the image by accident.
            assert!(tables[0][0] <= tables[0][255], "{name} runs backwards");
        }
        assert_eq!(color_lookup_tables("None"), Some([identity_lut(); 3]));
        assert!(color_lookup_tables("Not A Look").is_none());

        // The looks are distinct, which is what lets the panel name the one a
        // layer is carrying by comparing tables.
        let warm = color_lookup_tables("Warm Contrast").unwrap();
        let cool = color_lookup_tables("Cool Shadows").unwrap();
        assert_ne!(warm, cool);
        assert_ne!(warm, [identity_lut(); 3]);
    }
}

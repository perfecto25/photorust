//! Photoshop blend modes.
//!
//! Formulas follow the PDF 1.7 / W3C compositing spec, which is what Photoshop
//! implements. All functions take and return straight-alpha channel values
//! normalised to `[0, 1]`.
//!
//! Modes split into two families:
//!
//! * **Separable** — each of R, G, B is blended independently. Most modes.
//! * **Non-separable** — the RGB triple is transformed as a unit (Hue,
//!   Saturation, Color, Luminosity, and the Darker/Lighter Color pair).
//!
//! [`BlendMode::Dissolve`] is neither: it is a stochastic *alpha* operation
//! rather than a colour one, so the compositor handles it directly and
//! [`blend_rgb`] treats it as Normal.

/// The CS6 blend-mode set, in the order the Layers panel lists them.
///
/// Discriminants are part of the FFI surface — the C++ combo box sends these
/// integers back across the bridge. Do not renumber; append instead.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
#[repr(i32)]
pub enum BlendMode {
    #[default]
    Normal = 0,
    Dissolve = 1,

    Darken = 2,
    Multiply = 3,
    ColorBurn = 4,
    LinearBurn = 5,
    DarkerColor = 6,

    Lighten = 7,
    Screen = 8,
    ColorDodge = 9,
    LinearDodge = 10,
    LighterColor = 11,

    Overlay = 12,
    SoftLight = 13,
    HardLight = 14,
    VividLight = 15,
    LinearLight = 16,
    PinLight = 17,
    HardMix = 18,

    Difference = 19,
    Exclusion = 20,
    Subtract = 21,
    Divide = 22,

    Hue = 23,
    Saturation = 24,
    Color = 25,
    Luminosity = 26,
}

impl BlendMode {
    /// Every mode, in Layers-panel order. Drives the C++ combo box so the two
    /// sides cannot drift apart.
    pub const ALL: [BlendMode; 27] = [
        BlendMode::Normal,
        BlendMode::Dissolve,
        BlendMode::Darken,
        BlendMode::Multiply,
        BlendMode::ColorBurn,
        BlendMode::LinearBurn,
        BlendMode::DarkerColor,
        BlendMode::Lighten,
        BlendMode::Screen,
        BlendMode::ColorDodge,
        BlendMode::LinearDodge,
        BlendMode::LighterColor,
        BlendMode::Overlay,
        BlendMode::SoftLight,
        BlendMode::HardLight,
        BlendMode::VividLight,
        BlendMode::LinearLight,
        BlendMode::PinLight,
        BlendMode::HardMix,
        BlendMode::Difference,
        BlendMode::Exclusion,
        BlendMode::Subtract,
        BlendMode::Divide,
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
    ];

    /// Maps back from the FFI integer. Unknown values fall back to `Normal`
    /// rather than panicking across the bridge.
    pub fn from_i32(v: i32) -> BlendMode {
        BlendMode::ALL
            .iter()
            .copied()
            .find(|m| *m as i32 == v)
            .unwrap_or(BlendMode::Normal)
    }

    /// Display name as it appears in the Layers panel.
    pub fn name(&self) -> &'static str {
        match self {
            BlendMode::Normal => "Normal",
            BlendMode::Dissolve => "Dissolve",
            BlendMode::Darken => "Darken",
            BlendMode::Multiply => "Multiply",
            BlendMode::ColorBurn => "Color Burn",
            BlendMode::LinearBurn => "Linear Burn",
            BlendMode::DarkerColor => "Darker Color",
            BlendMode::Lighten => "Lighten",
            BlendMode::Screen => "Screen",
            BlendMode::ColorDodge => "Color Dodge",
            BlendMode::LinearDodge => "Linear Dodge (Add)",
            BlendMode::LighterColor => "Lighter Color",
            BlendMode::Overlay => "Overlay",
            BlendMode::SoftLight => "Soft Light",
            BlendMode::HardLight => "Hard Light",
            BlendMode::VividLight => "Vivid Light",
            BlendMode::LinearLight => "Linear Light",
            BlendMode::PinLight => "Pin Light",
            BlendMode::HardMix => "Hard Mix",
            BlendMode::Difference => "Difference",
            BlendMode::Exclusion => "Exclusion",
            BlendMode::Subtract => "Subtract",
            BlendMode::Divide => "Divide",
            BlendMode::Hue => "Hue",
            BlendMode::Saturation => "Saturation",
            BlendMode::Color => "Color",
            BlendMode::Luminosity => "Luminosity",
        }
    }

    /// Whether the mode transforms R/G/B as a unit. Callers that special-case
    /// separable modes for speed use this to pick a path.
    pub fn is_non_separable(&self) -> bool {
        matches!(
            self,
            BlendMode::Hue
                | BlendMode::Saturation
                | BlendMode::Color
                | BlendMode::Luminosity
                | BlendMode::DarkerColor
                | BlendMode::LighterColor
        )
    }

    /// Photoshop inserts separator lines between these groups in the dropdown.
    /// The index is the position *before* which a separator is drawn.
    pub const GROUP_BREAKS: [usize; 5] = [2, 7, 12, 19, 23];
}

#[inline]
fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Separable blend functions: B(backdrop, source)
// ---------------------------------------------------------------------------

#[inline]
fn multiply(b: f32, s: f32) -> f32 {
    b * s
}

#[inline]
fn screen(b: f32, s: f32) -> f32 {
    b + s - b * s
}

#[inline]
fn hard_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        multiply(b, 2.0 * s)
    } else {
        screen(b, 2.0 * s - 1.0)
    }
}

#[inline]
fn color_burn(b: f32, s: f32) -> f32 {
    if b >= 1.0 {
        1.0
    } else if s <= 0.0 {
        0.0
    } else {
        1.0 - clamp01((1.0 - b) / s)
    }
}

#[inline]
fn color_dodge(b: f32, s: f32) -> f32 {
    if b <= 0.0 {
        0.0
    } else if s >= 1.0 {
        1.0
    } else {
        clamp01(b / (1.0 - s))
    }
}

#[inline]
fn soft_light(b: f32, s: f32) -> f32 {
    // The W3C `D(b)` helper — a smooth ramp that avoids the harsh knee a plain
    // sqrt would give at b = 0.25.
    let d = if b <= 0.25 {
        ((16.0 * b - 12.0) * b + 4.0) * b
    } else {
        b.sqrt()
    };
    if s <= 0.5 {
        b - (1.0 - 2.0 * s) * b * (1.0 - b)
    } else {
        b + (2.0 * s - 1.0) * (d - b)
    }
}

#[inline]
fn vivid_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        color_burn(b, 2.0 * s)
    } else {
        color_dodge(b, 2.0 * s - 1.0)
    }
}

#[inline]
fn linear_light(b: f32, s: f32) -> f32 {
    clamp01(b + 2.0 * s - 1.0)
}

#[inline]
fn pin_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        b.min(2.0 * s)
    } else {
        b.max(2.0 * s - 1.0)
    }
}

/// Apply a separable blend mode to one channel.
///
/// Non-separable modes and `Dissolve` return the source unchanged here; the
/// caller is expected to route those through [`blend_rgb`] instead.
pub fn blend_channel(mode: BlendMode, b: f32, s: f32) -> f32 {
    match mode {
        BlendMode::Normal | BlendMode::Dissolve => s,
        BlendMode::Darken => b.min(s),
        BlendMode::Multiply => multiply(b, s),
        BlendMode::ColorBurn => color_burn(b, s),
        BlendMode::LinearBurn => clamp01(b + s - 1.0),
        BlendMode::Lighten => b.max(s),
        BlendMode::Screen => screen(b, s),
        BlendMode::ColorDodge => color_dodge(b, s),
        BlendMode::LinearDodge => clamp01(b + s),
        // Overlay is Hard Light with the operands swapped.
        BlendMode::Overlay => hard_light(s, b),
        BlendMode::SoftLight => soft_light(b, s),
        BlendMode::HardLight => hard_light(b, s),
        BlendMode::VividLight => vivid_light(b, s),
        BlendMode::LinearLight => linear_light(b, s),
        BlendMode::PinLight => pin_light(b, s),
        // Hard Mix is Linear Light pushed to the nearest extreme.
        BlendMode::HardMix => {
            if linear_light(b, s) < 0.5 {
                0.0
            } else {
                1.0
            }
        }
        BlendMode::Difference => (b - s).abs(),
        BlendMode::Exclusion => b + s - 2.0 * b * s,
        BlendMode::Subtract => clamp01(b - s),
        BlendMode::Divide => {
            if s <= 0.0 {
                1.0
            } else {
                clamp01(b / s)
            }
        }
        // Handled by `blend_rgb`.
        BlendMode::DarkerColor
        | BlendMode::LighterColor
        | BlendMode::Hue
        | BlendMode::Saturation
        | BlendMode::Color
        | BlendMode::Luminosity => s,
    }
}

// ---------------------------------------------------------------------------
// Non-separable helpers (PDF 1.7 §11.3.5.3)
// ---------------------------------------------------------------------------

#[inline]
fn lum(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

/// Pull any out-of-gamut channel back into `[0, 1]` by compressing the colour
/// toward its own luminosity, which preserves hue.
fn clip_color(mut c: [f32; 3]) -> [f32; 3] {
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);

    if n < 0.0 {
        let d = l - n;
        if d > f32::EPSILON {
            for ch in c.iter_mut() {
                *ch = l + (*ch - l) * l / d;
            }
        } else {
            c = [l, l, l];
        }
    }
    if x > 1.0 {
        let d = x - l;
        if d > f32::EPSILON {
            for ch in c.iter_mut() {
                *ch = l + (*ch - l) * (1.0 - l) / d;
            }
        } else {
            c = [l, l, l];
        }
    }
    c
}

fn set_lum(c: [f32; 3], l: f32) -> [f32; 3] {
    let d = l - lum(c);
    clip_color([c[0] + d, c[1] + d, c[2] + d])
}

#[inline]
fn sat(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

/// Rescale `c` to have saturation `s`, keeping the relative ordering of the
/// channels (and therefore the hue).
fn set_sat(c: [f32; 3], s: f32) -> [f32; 3] {
    // Index of the smallest, middle and largest channel.
    let (mut imin, mut imid, mut imax) = (0usize, 1usize, 2usize);
    if c[imin] > c[imid] {
        std::mem::swap(&mut imin, &mut imid);
    }
    if c[imin] > c[imax] {
        std::mem::swap(&mut imin, &mut imax);
    }
    if c[imid] > c[imax] {
        std::mem::swap(&mut imid, &mut imax);
    }

    let mut out = [0.0f32; 3];
    let range = c[imax] - c[imin];
    if range > f32::EPSILON {
        out[imid] = (c[imid] - c[imin]) * s / range;
        out[imax] = s;
    }
    // `out[imin]` stays 0 — a fully desaturated channel.
    out
}

/// Apply a blend mode to a whole RGB triple.
///
/// This is the general entry point: separable modes are dispatched per channel,
/// non-separable ones transform the triple as a unit.
pub fn blend_rgb(mode: BlendMode, b: [f32; 3], s: [f32; 3]) -> [f32; 3] {
    match mode {
        BlendMode::Hue => set_lum(set_sat(s, sat(b)), lum(b)),
        BlendMode::Saturation => set_lum(set_sat(b, sat(s)), lum(b)),
        BlendMode::Color => set_lum(s, lum(b)),
        BlendMode::Luminosity => set_lum(b, lum(s)),
        // Darker/Lighter Color pick one pixel wholesale by luminosity rather
        // than mixing channels — that is what makes them non-separable.
        BlendMode::DarkerColor => {
            if lum(s) < lum(b) {
                s
            } else {
                b
            }
        }
        BlendMode::LighterColor => {
            if lum(s) > lum(b) {
                s
            } else {
                b
            }
        }
        _ => [
            blend_channel(mode, b[0], s[0]),
            blend_channel(mode, b[1], s[1]),
            blend_channel(mode, b[2], s[2]),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-4;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < EPS
    }

    #[test]
    fn from_i32_roundtrips_every_mode() {
        for m in BlendMode::ALL {
            assert_eq!(BlendMode::from_i32(m as i32), m, "{:?}", m);
        }
    }

    #[test]
    fn from_i32_rejects_unknown_values() {
        assert_eq!(BlendMode::from_i32(-1), BlendMode::Normal);
        assert_eq!(BlendMode::from_i32(9999), BlendMode::Normal);
    }

    #[test]
    fn normal_returns_source() {
        assert!(close(blend_channel(BlendMode::Normal, 0.2, 0.8), 0.8));
    }

    #[test]
    fn multiply_and_screen_are_duals() {
        // screen(b,s) == 1 - multiply(1-b, 1-s)
        for &(b, s) in &[(0.2f32, 0.8f32), (0.5, 0.5), (0.9, 0.1)] {
            let m = blend_channel(BlendMode::Multiply, 1.0 - b, 1.0 - s);
            let sc = blend_channel(BlendMode::Screen, b, s);
            assert!(close(sc, 1.0 - m), "b={} s={}", b, s);
        }
    }

    #[test]
    fn overlay_is_hard_light_swapped() {
        for &(b, s) in &[(0.2f32, 0.8f32), (0.7, 0.3), (0.5, 0.5)] {
            let o = blend_channel(BlendMode::Overlay, b, s);
            let h = blend_channel(BlendMode::HardLight, s, b);
            assert!(close(o, h), "b={} s={}", b, s);
        }
    }

    #[test]
    fn burn_and_dodge_handle_extremes_without_nan() {
        assert!(close(blend_channel(BlendMode::ColorBurn, 1.0, 0.0), 1.0));
        assert!(close(blend_channel(BlendMode::ColorBurn, 0.5, 0.0), 0.0));
        assert!(close(blend_channel(BlendMode::ColorDodge, 0.0, 1.0), 0.0));
        assert!(close(blend_channel(BlendMode::ColorDodge, 0.5, 1.0), 1.0));
        for m in [BlendMode::ColorBurn, BlendMode::ColorDodge, BlendMode::Divide] {
            for b in [0.0f32, 0.5, 1.0] {
                for s in [0.0f32, 0.5, 1.0] {
                    let v = blend_channel(m, b, s);
                    assert!(v.is_finite(), "{:?} b={} s={} -> {}", m, b, s, v);
                    assert!((0.0..=1.0).contains(&v), "{:?} out of range: {}", m, v);
                }
            }
        }
    }

    #[test]
    fn all_separable_modes_stay_in_range() {
        let samples = [0.0f32, 0.13, 0.25, 0.5, 0.75, 0.87, 1.0];
        for m in BlendMode::ALL {
            if m.is_non_separable() {
                continue;
            }
            for &b in &samples {
                for &s in &samples {
                    let v = blend_channel(m, b, s);
                    assert!(v.is_finite(), "{:?} produced {}", m, v);
                    assert!(
                        (-EPS..=1.0 + EPS).contains(&v),
                        "{:?} b={} s={} -> {}",
                        m,
                        b,
                        s,
                        v
                    );
                }
            }
        }
    }

    #[test]
    fn hard_mix_is_binary() {
        for b in [0.0f32, 0.3, 0.6, 1.0] {
            for s in [0.0f32, 0.3, 0.6, 1.0] {
                let v = blend_channel(BlendMode::HardMix, b, s);
                assert!(v == 0.0 || v == 1.0, "got {}", v);
            }
        }
    }

    #[test]
    fn difference_with_self_is_black() {
        assert!(close(blend_channel(BlendMode::Difference, 0.6, 0.6), 0.0));
    }

    #[test]
    fn luminosity_takes_source_luma_and_backdrop_hue() {
        let backdrop = [0.8, 0.2, 0.2]; // red
        let source = [0.5, 0.5, 0.5]; // mid grey
        let out = blend_rgb(BlendMode::Luminosity, backdrop, source);
        // Result should carry the source's luminosity...
        assert!(close(lum(out), lum(source)), "lum was {}", lum(out));
        // ...while still reading as red (R clearly the dominant channel).
        assert!(out[0] > out[1] && out[0] > out[2]);
    }

    #[test]
    fn color_takes_source_hue_and_backdrop_luma() {
        let backdrop = [0.5, 0.5, 0.5];
        let source = [0.9, 0.1, 0.1];
        let out = blend_rgb(BlendMode::Color, backdrop, source);
        assert!(close(lum(out), lum(backdrop)), "lum was {}", lum(out));
        assert!(out[0] > out[1]);
    }

    #[test]
    fn non_separable_modes_stay_in_gamut() {
        let cases = [
            ([0.0f32, 0.0, 0.0], [1.0f32, 1.0, 1.0]),
            ([1.0, 1.0, 1.0], [0.0, 0.0, 0.0]),
            ([0.9, 0.1, 0.4], [0.2, 0.8, 0.3]),
            ([0.5, 0.5, 0.5], [1.0, 0.0, 0.0]),
        ];
        for m in BlendMode::ALL.iter().filter(|m| m.is_non_separable()) {
            for (b, s) in cases {
                let out = blend_rgb(*m, b, s);
                for (i, v) in out.iter().enumerate() {
                    assert!(v.is_finite(), "{:?} ch{} -> {}", m, i, v);
                    assert!(
                        (-EPS..=1.0 + EPS).contains(v),
                        "{:?} ch{} out of gamut: {}",
                        m,
                        i,
                        v
                    );
                }
            }
        }
    }

    #[test]
    fn darker_and_lighter_color_pick_a_whole_pixel() {
        let dark = [0.1, 0.1, 0.1];
        let light = [0.9, 0.9, 0.9];
        assert_eq!(blend_rgb(BlendMode::DarkerColor, light, dark), dark);
        assert_eq!(blend_rgb(BlendMode::LighterColor, dark, light), light);
        // The backdrop wins ties, matching Photoshop.
        assert_eq!(blend_rgb(BlendMode::DarkerColor, dark, light), dark);
    }

    #[test]
    fn set_sat_on_flat_color_stays_flat() {
        // A fully desaturated input has no channel ordering to preserve, so the
        // result must not divide by zero.
        let out = set_sat([0.5, 0.5, 0.5], 0.7);
        assert_eq!(out, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn every_mode_has_a_distinct_name() {
        let mut names: Vec<&str> = BlendMode::ALL.iter().map(|m| m.name()).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "duplicate blend mode name");
    }
}

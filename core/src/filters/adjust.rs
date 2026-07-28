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
    /// Reduce to `levels` steps per channel.
    Posterize { levels: u32 },
    /// Hard cut at `threshold` (0..=255) on luma.
    Threshold { level: u8 },
    /// Multiply each channel independently.
    ColorBalance { r: f32, g: f32, b: f32 },
    Exposure { exposure: f32, offset: f32, gamma: f32 },
}

impl Default for Adjustment {
    fn default() -> Self {
        Adjustment::BrightnessContrast {
            brightness: 0.0,
            contrast: 0.0,
        }
    }
}

impl Adjustment {
    pub fn name(&self) -> &'static str {
        match self {
            Adjustment::BrightnessContrast { .. } => "Brightness/Contrast",
            Adjustment::HueSaturation { .. } => "Hue/Saturation",
            Adjustment::Levels { .. } => "Levels",
            Adjustment::Invert => "Invert",
            Adjustment::Desaturate => "Black & White",
            Adjustment::Posterize { .. } => "Posterize",
            Adjustment::Threshold { .. } => "Threshold",
            Adjustment::ColorBalance { .. } => "Color Balance",
            Adjustment::Exposure { .. } => "Exposure",
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
            _ => return None,
        })
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

            Adjustment::HueSaturation {
                hue,
                saturation,
                lightness,
            } => {
                let (h, s, l) = rgb_to_hsl(c);
                let h = (h + hue).rem_euclid(1.0);
                // Positive saturation pushes toward full saturation
                // asymptotically rather than clipping abruptly.
                let s = if saturation >= 0.0 {
                    s + (1.0 - s) * saturation.min(1.0)
                } else {
                    s * (1.0 + saturation.max(-1.0))
                };
                let l = if lightness >= 0.0 {
                    l + (1.0 - l) * lightness.min(1.0)
                } else {
                    l * (1.0 + lightness.max(-1.0))
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
}

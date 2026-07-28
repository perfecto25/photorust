//! Filters and adjustments.
//!
//! Two distinct things live here:
//!
//! * [`Adjustment`] — a cheap, per-pixel colour transform. Adjustments are
//!   evaluated by the compositor on the fly, which is what makes adjustment
//!   layers non-destructive.
//! * [`Filter`] — a neighbourhood operation (convolution and friends) applied
//!   destructively to a [`Pixmap`].

pub mod adjust;
pub mod convolve;

pub use adjust::Adjustment;
pub use convolve::{gaussian_blur, sharpen, unsharp_mask, Kernel};

use crate::buffer::Pixmap;

/// A destructive image operation from the Filter menu.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Filter {
    /// Radius in pixels.
    GaussianBlur { radius: f32 },
    /// Box blur — cheaper, used for previews.
    BoxBlur { radius: u32 },
    Sharpen,
    UnsharpMask {
        amount: f32,
        radius: f32,
        threshold: u8,
    },
    /// Add monochrome or colour noise. `amount` is 0..=1.
    Noise { amount: f32, monochromatic: bool },
}

impl Filter {
    pub fn name(&self) -> &'static str {
        match self {
            Filter::GaussianBlur { .. } => "Gaussian Blur",
            Filter::BoxBlur { .. } => "Box Blur",
            Filter::Sharpen => "Sharpen",
            Filter::UnsharpMask { .. } => "Unsharp Mask",
            Filter::Noise { .. } => "Add Noise",
        }
    }

    /// Apply the filter to `pixmap` in place.
    pub fn apply(&self, pixmap: &mut Pixmap) {
        match *self {
            Filter::GaussianBlur { radius } => gaussian_blur(pixmap, radius),
            Filter::BoxBlur { radius } => convolve::box_blur(pixmap, radius),
            Filter::Sharpen => sharpen(pixmap),
            Filter::UnsharpMask {
                amount,
                radius,
                threshold,
            } => unsharp_mask(pixmap, amount, radius, threshold),
            Filter::Noise {
                amount,
                monochromatic,
            } => add_noise(pixmap, amount, monochromatic),
        }
    }
}

/// Deterministic value noise.
///
/// Seeded from pixel coordinates rather than a RNG so that re-running a filter
/// during an undo/redo replay reproduces the exact same image.
fn add_noise(pixmap: &mut Pixmap, amount: f32, monochromatic: bool) {
    let amount = amount.clamp(0.0, 1.0);
    if amount <= 0.0 {
        return;
    }
    let width = pixmap.width();
    let magnitude = amount * 255.0;

    for y in 0..pixmap.height() {
        for x in 0..width {
            let base = hash2(x, y);
            let jitter = |salt: u32| -> i32 {
                let h = hash2(base.wrapping_add(salt), salt);
                // Map the hash into a symmetric [-magnitude, +magnitude].
                (((h % 2001) as f32 / 1000.0 - 1.0) * magnitude) as i32
            };

            let px = pixmap.get(x as i32, y as i32);
            if px.a == 0 {
                continue;
            }
            let (dr, dg, db) = if monochromatic {
                let d = jitter(0);
                (d, d, d)
            } else {
                (jitter(0), jitter(1), jitter(2))
            };
            pixmap.set(
                x as i32,
                y as i32,
                crate::buffer::Rgba8::new(
                    (px.r as i32 + dr).clamp(0, 255) as u8,
                    (px.g as i32 + dg).clamp(0, 255) as u8,
                    (px.b as i32 + db).clamp(0, 255) as u8,
                    px.a,
                ),
            );
        }
    }
}

/// Cheap integer hash used to derive reproducible per-pixel noise.
#[inline]
fn hash2(x: u32, y: u32) -> u32 {
    let mut h = x.wrapping_mul(0x9E3779B1) ^ y.wrapping_mul(0x85EBCA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545F491);
    h ^= h >> 13;
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Rgba8;

    #[test]
    fn noise_is_deterministic() {
        let make = || {
            let mut pm = Pixmap::filled(8, 8, Rgba8::new(128, 128, 128, 255));
            add_noise(&mut pm, 0.5, false);
            pm
        };
        assert_eq!(make().as_bytes(), make().as_bytes());
    }

    #[test]
    fn noise_leaves_transparent_pixels_alone() {
        let mut pm = Pixmap::new(4, 4);
        add_noise(&mut pm, 1.0, false);
        assert!(pm.as_bytes().iter().all(|&b| b == 0));
    }

    #[test]
    fn zero_amount_noise_is_a_no_op() {
        let mut pm = Pixmap::filled(4, 4, Rgba8::new(100, 100, 100, 255));
        let before = pm.as_bytes().to_vec();
        add_noise(&mut pm, 0.0, false);
        assert_eq!(pm.as_bytes(), &before[..]);
    }

    #[test]
    fn monochromatic_noise_keeps_channels_equal() {
        let mut pm = Pixmap::filled(8, 8, Rgba8::new(128, 128, 128, 255));
        add_noise(&mut pm, 0.3, true);
        for y in 0..8 {
            for x in 0..8 {
                let p = pm.get(x, y);
                assert_eq!(p.r, p.g);
                assert_eq!(p.g, p.b);
            }
        }
    }

    #[test]
    fn noise_preserves_alpha() {
        let mut pm = Pixmap::filled(4, 4, Rgba8::new(10, 10, 10, 200));
        add_noise(&mut pm, 1.0, false);
        for y in 0..4 {
            for x in 0..4 {
                assert_eq!(pm.get(x, y).a, 200);
            }
        }
    }

    #[test]
    fn filters_have_names() {
        let all = [
            Filter::GaussianBlur { radius: 1.0 },
            Filter::BoxBlur { radius: 1 },
            Filter::Sharpen,
            Filter::UnsharpMask {
                amount: 1.0,
                radius: 1.0,
                threshold: 0,
            },
            Filter::Noise {
                amount: 0.1,
                monochromatic: true,
            },
        ];
        for f in all {
            assert!(!f.name().is_empty());
        }
    }
}

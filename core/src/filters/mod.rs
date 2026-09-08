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
pub use convolve::{gaussian_blur, sharpen, unsharp_mask, Kernel, RadialQuality};

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

    // The rest of CS6's Blur submenu. All of them are per-pixel work over a
    // neighbourhood and would suit the GPU (CLAUDE.md §7), but none is
    // enabled there: the backend's one shader is the Gaussian, and the
    // measurements in docs/gpu-migration.md are that a single filter which
    // uploads its input and reads the result straight back rarely pays for
    // the trip. They belong in a batch with the rest of the filter stack, if
    // that is ever done, rather than one at a time.
    /// One flat colour: the mean of everything in range.
    Average,
    /// CS6's fixed-strength blurs, which take no radius — a soft 3×3 and a
    /// stronger one. The numbers are what makes them "Blur" and "Blur More"
    /// rather than a Gaussian you have to dial in.
    Blur,
    BlurMore,
    /// `angle` in degrees, `distance` in pixels.
    MotionBlur { angle: f32, distance: f32 },
    /// `amount` as a percentage; `spin` turns about the centre rather than
    /// running in and out from it. `center` is in normalized 0..1 coordinates
    /// — CS6's Blur Center box, which is draggable and defaults to the middle
    /// — and `quality` is how densely the path each pixel travels is sampled.
    RadialBlur {
        amount: f32,
        spin: bool,
        center: (f32, f32),
        quality: RadialQuality,
    },
    /// Blurs within a region but not across its edges: a neighbour counts
    /// only if it is within `threshold` of the centre pixel.
    SurfaceBlur { radius: u32, threshold: u32 },
}

impl Filter {
    pub fn name(&self) -> &'static str {
        match self {
            Filter::GaussianBlur { .. } => "Gaussian Blur",
            Filter::BoxBlur { .. } => "Box Blur",
            Filter::Sharpen => "Sharpen",
            Filter::UnsharpMask { .. } => "Unsharp Mask",
            Filter::Noise { .. } => "Add Noise",
            Filter::Average => "Average",
            Filter::Blur => "Blur",
            Filter::BlurMore => "Blur More",
            Filter::MotionBlur { .. } => "Motion Blur",
            Filter::RadialBlur { .. } => "Radial Blur",
            Filter::SurfaceBlur { .. } => "Surface Blur",
        }
    }

    /// Build a filter from the name the Filter menu uses, and the numbers its
    /// dialog collected.
    ///
    /// The shell speaks in menu names because that is what it has — a menu
    /// item and the values from its dialog — and this keeps the mapping in one
    /// place where both `applyFilter` and the preview can reach it. An
    /// unrecognised name is `None` rather than a guess.
    ///
    /// `p` is positional, in the order the dialog lists its controls. Five
    /// slots because Radial Blur needs them all: amount, method, the two
    /// coordinates of its centre, and quality.
    pub fn from_menu_name(name: &str, p: [f32; 5]) -> Option<Filter> {
        let [p1, p2, p3, p4, p5] = p;
        Some(match name {
            "Gaussian Blur" => Filter::GaussianBlur { radius: p1.max(0.0) },
            "Box Blur" => Filter::BoxBlur {
                radius: p1.max(0.0) as u32,
            },
            "Average" => Filter::Average,
            "Blur" => Filter::Blur,
            "Blur More" => Filter::BlurMore,
            "Motion Blur" => Filter::MotionBlur {
                angle: p1,
                distance: p2.max(0.0),
            },
            "Radial Blur" => Filter::RadialBlur {
                amount: p1.max(0.0),
                spin: p2 != 0.0,
                center: (p3, p4),
                quality: RadialQuality::from_i32(p5 as i32),
            },
            "Surface Blur" => Filter::SurfaceBlur {
                radius: p1.max(0.0) as u32,
                threshold: p2.max(0.0) as u32,
            },
            "Sharpen" => Filter::Sharpen,
            "Unsharp Mask" => Filter::UnsharpMask {
                amount: p1,
                radius: p2,
                threshold: 0,
            },
            "Add Noise" => Filter::Noise {
                amount: p1.clamp(0.0, 1.0),
                monochromatic: p2 != 0.0,
            },
            _ => return None,
        })
    }

    /// How far, in pixels, a result pixel reaches for its input.
    ///
    /// This is what lets a preview of one region be computed from a crop: pad
    /// the crop by the reach, filter it, and the middle is identical to what
    /// the whole image would have produced.
    ///
    /// `None` means the operation cannot be cropped at all. Average is the
    /// mean of every pixel there is, and Radial Blur sweeps about a centre
    /// given as a fraction of the whole image — neither has an answer that a
    /// region can be asked for on its own, so a preview of them has to filter
    /// the whole layer first.
    pub fn reach(&self) -> Option<u32> {
        match *self {
            // Three sigma covers a Gaussian to well under a level of 255.
            Filter::GaussianBlur { radius } => Some((radius * 3.0).ceil().max(1.0) as u32),
            Filter::Blur => Some(3),
            Filter::BlurMore => Some(6),
            Filter::BoxBlur { radius } => Some(radius),
            Filter::SurfaceBlur { radius, .. } => Some(radius),
            // The line is centred on the pixel, so it reaches half its length
            // in the worst direction.
            Filter::MotionBlur { distance, .. } => Some((distance / 2.0).ceil().max(1.0) as u32),
            Filter::UnsharpMask { radius, .. } => Some((radius * 3.0).ceil().max(1.0) as u32),
            Filter::Sharpen => Some(1),
            // Noise reads no neighbours at all, but it is seeded from each
            // pixel's coordinates so that undo/redo reproduces it. A crop
            // taken out of the layer would be at different coordinates and
            // would show a different grain from the one the user is about to
            // get, so it goes the whole-layer way with the other two.
            Filter::Noise { .. } => None,
            Filter::Average | Filter::RadialBlur { .. } => None,
        }
    }

    /// Apply the filter to `pixmap` in place.
    pub fn apply(&self, pixmap: &mut Pixmap) {
        match *self {
            Filter::GaussianBlur { radius } => {
                convolve::gaussian_blur_accelerated(pixmap, radius)
            }
            Filter::BoxBlur { radius } => convolve::box_blur(pixmap, radius),
            Filter::Average => convolve::average(pixmap),
            // CS6's two fixed blurs are a gentle Gaussian and a stronger one;
            // the radii are chosen to match how far each visibly softens.
            Filter::Blur => convolve::gaussian_blur_accelerated(pixmap, 0.7),
            Filter::BlurMore => convolve::gaussian_blur_accelerated(pixmap, 2.0),
            Filter::MotionBlur { angle, distance } => {
                convolve::motion_blur(pixmap, angle, distance)
            }
            Filter::RadialBlur {
                amount,
                spin,
                center,
                quality,
            } => convolve::radial_blur(pixmap, amount, spin, center, quality),
            Filter::SurfaceBlur { radius, threshold } => {
                convolve::surface_blur(pixmap, radius, threshold)
            }
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

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
pub mod distort;
pub mod pixelate;
pub mod render;

pub use adjust::Adjustment;
pub use convolve::{gaussian_blur, sharpen, sharpen_edges, sharpen_more, unsharp_mask, Kernel,
                   RadialQuality, SharpenRemove};
pub use distort::{EdgeMode, RippleSize, SpherizeMode, WaveKind, ZigZagStyle, SHEAR_POINTS};
pub use pixelate::{MezzotintType, ScreenAngles, DEFAULT_SCREEN_ANGLES};
pub use render::{FlameOptions, FlameShape, FlameStyle, FlameType};

use crate::buffer::Pixmap;

/// A destructive image operation from the Filter menu.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Filter {
    /// Radius in pixels.
    GaussianBlur { radius: f32 },
    /// Box blur — cheaper, used for previews.
    BoxBlur { radius: u32 },
    Sharpen,
    /// CS6's other two fixed-strength sharpeners. `SharpenMore` is a heavier
    /// dose of `Sharpen`; `SharpenEdges` confines it to where there is an
    /// edge to sharpen. Neither takes a parameter, as in CS6.
    SharpenMore,
    SharpenEdges,
    /// CS6's Smart Sharpen: an unsharp mask against the kind of blur you say
    /// the softness came from, holding back detail below a noise floor.
    /// `angle` is only read for a motion streak.
    SmartSharpen {
        amount: f32,
        radius: f32,
        reduce_noise: f32,
        remove: SharpenRemove,
        angle: f32,
    },
    UnsharpMask {
        amount: f32,
        radius: f32,
        threshold: u8,
    },
    /// Add monochrome or colour noise. `amount` is a percentage, as CS6's
    /// slider is; `gaussian` picks its Gaussian distribution over the uniform
    /// one, which clumps rather than spreading evenly.
    Noise {
        amount: f32,
        monochromatic: bool,
        gaussian: bool,
    },
    /// The median of everything within `radius` — Filter ▸ Noise ▸ Median.
    Median { radius: u32 },
    /// The same, but only where a pixel is further than `threshold` from that
    /// median — Filter ▸ Noise ▸ Dust & Scratches.
    DustAndScratches { radius: u32, threshold: u32 },

    // Filter ▸ Distort. These move the picture about rather than mixing
    // neighbours together, so they live in their own module (`distort`) and
    // share one bilinear back end. Displace is not here: it takes a second
    // image, which no amount of numbers can carry, so it has its own path
    // through the bridge.
    /// Squeeze the middle in, or push it out. `amount` is -100 to 100.
    Pinch { amount: f32 },
    /// Wrap the picture into a disc, or unroll one back into a rectangle.
    PolarCoordinates { to_polar: bool },
    /// Ripple the picture as though seen through water.
    Ripple { amount: f32, size: RippleSize },
    /// Push each row sideways by an amount read off a curve.
    Shear {
        offsets: [f32; SHEAR_POINTS],
        wrap: bool,
    },
    /// Wrap the picture onto a ball, or press it into a bowl.
    Spherize { amount: f32, mode: SpherizeMode },
    /// Wind the picture into a spiral. `angle` in degrees.
    Twirl { angle: f32 },
    /// A sum of waves. `seed` is what CS6's Randomize button re-rolls.
    Wave {
        generators: u32,
        wavelength: (f32, f32),
        amplitude: (f32, f32),
        scale: (f32, f32),
        kind: WaveKind,
        wrap: bool,
        seed: u32,
    },
    /// Break the picture into irregular polygonal cells, each filled with the
    /// average of what was under it — Filter ▸ Pixelate ▸ Crystallize — and
    /// the same thing on a plain square grid, which is Mosaic.
    Crystallize { cell_size: u32 },
    Mosaic { cell_size: u32 },
    /// Rebuild the picture out of scattered dabs on a ground of the
    /// background colour — Filter ▸ Pixelate ▸ Pointillize. The colour is the
    /// document's rather than anything the dialog asks for, so the bridge
    /// fills it in; see `Engine::filter_for`.
    Pointillize {
        cell_size: u32,
        background: crate::buffer::Rgba8,
    },
    /// Clump neighbouring pixels of similar colour into flat patches, and
    /// four shifted copies averaged together. Neither takes a parameter, as
    /// in CS6 — they are meant to be repeated with Ctrl+F.
    Facet,
    Fragment,
    /// Throw every channel to one end or the other on a random pattern —
    /// Filter ▸ Pixelate ▸ Mezzotint.
    Mezzotint { kind: MezzotintType },
    /// Redraw the picture as a four-colour press would — Filter ▸ Pixelate ▸
    /// Color Halftone. `max_radius` is the size of a dot at full ink.
    ColorHalftone {
        max_radius: f32,
        angles: ScreenAngles,
    },
    /// Break the picture into rings that alternate the way they are pushed.
    ZigZag {
        amount: f32,
        ridges: u32,
        style: ZigZagStyle,
    },

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
            Filter::SharpenMore => "Sharpen More",
            Filter::SharpenEdges => "Sharpen Edges",
            Filter::SmartSharpen { .. } => "Smart Sharpen",
            Filter::UnsharpMask { .. } => "Unsharp Mask",
            Filter::Noise { .. } => "Add Noise",
            Filter::Median { .. } => "Median",
            Filter::DustAndScratches { .. } => "Dust & Scratches",
            Filter::Pinch { .. } => "Pinch",
            Filter::PolarCoordinates { .. } => "Polar Coordinates",
            Filter::Ripple { .. } => "Ripple",
            Filter::Shear { .. } => "Shear",
            Filter::Spherize { .. } => "Spherize",
            Filter::Twirl { .. } => "Twirl",
            Filter::Wave { .. } => "Wave",
            Filter::ZigZag { .. } => "ZigZag",
            Filter::ColorHalftone { .. } => "Color Halftone",
            Filter::Crystallize { .. } => "Crystallize",
            Filter::Mosaic { .. } => "Mosaic",
            Filter::Pointillize { .. } => "Pointillize",
            Filter::Facet => "Facet",
            Filter::Fragment => "Fragment",
            Filter::Mezzotint { .. } => "Mezzotint",
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
    /// `p` is positional, in the order the dialog lists its controls, and a
    /// value the dialog did not offer reads as zero. Slice rather than a fixed
    /// array because the count is a property of the filter, not of the bridge:
    /// Blur wants one, Radial Blur five, Wave ten, and Shear a whole sampled
    /// curve.
    pub fn from_menu_name(name: &str, p: &[f32]) -> Option<Filter> {
        let at = |i: usize| p.get(i).copied().unwrap_or(0.0);
        let (p1, p2, p3, p4, p5) = (at(0), at(1), at(2), at(3), at(4));
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
            "Sharpen More" => Filter::SharpenMore,
            "Sharpen Edges" => Filter::SharpenEdges,
            "Smart Sharpen" => Filter::SmartSharpen {
                amount: p1.max(0.0),
                radius: p2.max(0.0),
                reduce_noise: p3.clamp(0.0, 100.0),
                remove: SharpenRemove::from_i32(p4 as i32),
                angle: p5,
            },
            "Unsharp Mask" => Filter::UnsharpMask {
                amount: p1,
                radius: p2,
                threshold: 0,
            },
            "Add Noise" => Filter::Noise {
                amount: p1.max(0.0),
                gaussian: p2 != 0.0,
                monochromatic: p3 != 0.0,
            },
            "Median" => Filter::Median {
                radius: p1.max(0.0) as u32,
            },
            "Dust & Scratches" => Filter::DustAndScratches {
                radius: p1.max(0.0) as u32,
                threshold: p2.max(0.0) as u32,
            },
            "Pinch" => Filter::Pinch { amount: p1 },
            "Polar Coordinates" => Filter::PolarCoordinates {
                to_polar: p1 != 0.0,
            },
            "Ripple" => Filter::Ripple {
                amount: p1,
                size: RippleSize::from_i32(p2 as i32),
            },
            "Shear" => {
                // The curve first, then the Undefined Areas flag after it, so
                // that adding a point to the curve would not renumber it.
                let mut offsets = [0.0f32; SHEAR_POINTS];
                for (i, slot) in offsets.iter_mut().enumerate() {
                    *slot = at(i);
                }
                Filter::Shear {
                    offsets,
                    wrap: at(SHEAR_POINTS) != 0.0,
                }
            }
            "Spherize" => Filter::Spherize {
                amount: p1,
                mode: SpherizeMode::from_i32(p2 as i32),
            },
            "Twirl" => Filter::Twirl { angle: p1 },
            "Wave" => Filter::Wave {
                generators: p1.max(1.0) as u32,
                wavelength: (p2, p3),
                amplitude: (p4, p5),
                scale: (at(5), at(6)),
                kind: WaveKind::from_i32(at(7) as i32),
                wrap: at(8) != 0.0,
                seed: at(9) as u32,
            },
            "Facet" => Filter::Facet,
            "Fragment" => Filter::Fragment,
            "Mezzotint" => Filter::Mezzotint {
                kind: MezzotintType::from_i32(p1 as i32),
            },
            "Crystallize" => Filter::Crystallize {
                cell_size: p1.max(1.0) as u32,
            },
            "Mosaic" => Filter::Mosaic {
                cell_size: p1.max(1.0) as u32,
            },
            "Pointillize" => Filter::Pointillize {
                cell_size: p1.max(1.0) as u32,
                background: crate::buffer::Rgba8::WHITE,
            },
            "Color Halftone" => Filter::ColorHalftone {
                max_radius: p1.max(1.0),
                // Falling back to the standard press angles rather than to
                // zero, which would stack all four screens on top of one
                // another and produce a plaid instead of a halftone.
                angles: [
                    p.get(1).copied().unwrap_or(DEFAULT_SCREEN_ANGLES[0]),
                    p.get(2).copied().unwrap_or(DEFAULT_SCREEN_ANGLES[1]),
                    p.get(3).copied().unwrap_or(DEFAULT_SCREEN_ANGLES[2]),
                    p.get(4).copied().unwrap_or(DEFAULT_SCREEN_ANGLES[3]),
                ],
            },
            "ZigZag" => Filter::ZigZag {
                amount: p1,
                ridges: p2.max(1.0) as u32,
                style: ZigZagStyle::from_i32(p3 as i32),
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
            // All three are built on a radius-1 Gaussian, which reaches three
            // pixels; Sharpen Edges then looks one further for its gradient.
            Filter::Sharpen | Filter::SharpenMore => Some(3),
            Filter::SharpenEdges => Some(4),
            // Whichever blur it is sharpening against, three sigma of a
            // Gaussian is the widest of the three at a given radius.
            Filter::SmartSharpen { radius, .. } => Some((radius * 3.0).ceil().max(1.0) as u32),
            // Noise reads no neighbours at all, but it is seeded from each
            // pixel's coordinates so that undo/redo reproduces it. A crop
            // taken out of the layer would be at different coordinates and
            // would show a different grain from the one the user is about to
            // get, so it goes the whole-layer way with the other two.
            Filter::Noise { .. } => None,
            Filter::Median { radius } | Filter::DustAndScratches { radius, .. } => Some(radius),
            // A distortion can fetch a pixel from anywhere in the image, so
            // there is no crop that could answer for a region on its own.
            Filter::Pinch { .. }
            | Filter::PolarCoordinates { .. }
            | Filter::Ripple { .. }
            | Filter::Shear { .. }
            | Filter::Spherize { .. }
            | Filter::Twirl { .. }
            | Filter::Wave { .. }
            | Filter::ZigZag { .. } => None,
            // The screen is laid on the canvas, so a crop would land on a
            // different part of the lattice and its dots would not line up
            // with the ones either side of it.
            Filter::ColorHalftone { .. }
            | Filter::Crystallize { .. }
            | Filter::Mosaic { .. }
            | Filter::Pointillize { .. } => None,
            // The pattern is laid on the canvas, so a crop would land on a
            // different part of it and its grain would not line up with the
            // pixels either side.
            Filter::Mezzotint { .. } => None,
            // Both read only a small neighbourhood, so a padded crop answers
            // for a region exactly.
            Filter::Facet => Some(1),
            Filter::Fragment => Some(4),
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
            Filter::SharpenMore => convolve::sharpen_more(pixmap),
            Filter::SharpenEdges => convolve::sharpen_edges(pixmap),
            Filter::SmartSharpen {
                amount,
                radius,
                reduce_noise,
                remove,
                angle,
            } => convolve::smart_sharpen(pixmap, amount, radius, reduce_noise, remove, angle),
            Filter::UnsharpMask {
                amount,
                radius,
                threshold,
            } => unsharp_mask(pixmap, amount, radius, threshold),
            Filter::Noise {
                amount,
                monochromatic,
                gaussian,
            } => add_noise(pixmap, amount, monochromatic, gaussian),
            Filter::Median { radius } => convolve::median_filter(pixmap, radius),
            Filter::DustAndScratches { radius, threshold } => {
                convolve::dust_and_scratches(pixmap, radius, threshold)
            }
            Filter::Pinch { amount } => distort::pinch(pixmap, amount),
            Filter::PolarCoordinates { to_polar } => {
                distort::polar_coordinates(pixmap, to_polar)
            }
            Filter::Ripple { amount, size } => distort::ripple(pixmap, amount, size),
            Filter::Shear { offsets, wrap } => distort::shear(pixmap, &offsets, wrap),
            Filter::Spherize { amount, mode } => distort::spherize(pixmap, amount, mode),
            Filter::Twirl { angle } => distort::twirl(pixmap, angle),
            Filter::Wave {
                generators,
                wavelength,
                amplitude,
                scale,
                kind,
                wrap,
                seed,
            } => distort::wave(pixmap, generators, wavelength, amplitude, scale, kind, wrap, seed),
            Filter::ZigZag {
                amount,
                ridges,
                style,
            } => distort::zigzag(pixmap, amount, ridges, style),
            Filter::ColorHalftone { max_radius, angles } => {
                pixelate::color_halftone(pixmap, max_radius, angles)
            }
            Filter::Crystallize { cell_size } => pixelate::crystallize(pixmap, cell_size),
            Filter::Mosaic { cell_size } => pixelate::mosaic(pixmap, cell_size),
            Filter::Pointillize {
                cell_size,
                background,
            } => pixelate::pointillize(pixmap, cell_size, background),
            Filter::Facet => pixelate::facet(pixmap),
            Filter::Fragment => pixelate::fragment(pixmap),
            Filter::Mezzotint { kind } => pixelate::mezzotint(pixmap, kind),
        }
    }
}

/// Deterministic value noise.
///
/// Seeded from pixel coordinates rather than a RNG so that re-running a filter
/// during an undo/redo replay reproduces the exact same image.
fn add_noise(pixmap: &mut Pixmap, amount: f32, monochromatic: bool, gaussian: bool) {
    // CS6's slider is a percentage of the tonal range, running to 400 where
    // the picture is entirely gone. Half of it either way, so 100% spans the
    // whole range from a mid grey.
    let magnitude = (amount.clamp(0.0, 400.0) / 100.0) * 127.5;
    if magnitude <= 0.0 {
        return;
    }
    let width = pixmap.width();

    for y in 0..pixmap.height() {
        for x in 0..width {
            let base = hash2(x, y);
            let jitter = |salt: u32| -> i32 {
                let h = hash2(base.wrapping_add(salt), salt);
                // A hash in [-1, 1].
                let u = (h % 2001) as f32 / 1000.0 - 1.0;
                if !gaussian {
                    return (u * magnitude) as i32;
                }
                // Gaussian noise clumps: most of it lands near zero and the
                // occasional sample goes a long way out, which is the grainier
                // look CS6's second option gives. Built by adding three
                // independent hashes rather than taking a logarithm — the sum
                // is near enough a bell for this, and it costs three
                // multiplications.
                let v = (hash2(h, salt.wrapping_add(101)) % 2001) as f32 / 1000.0 - 1.0;
                let w = (hash2(h, salt.wrapping_add(211)) % 2001) as f32 / 1000.0 - 1.0;
                ((u + v + w) / 3.0 * 1.732 * magnitude) as i32
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
            add_noise(&mut pm, 50.0, false, false);
            pm
        };
        assert_eq!(make().as_bytes(), make().as_bytes());
    }

    #[test]
    fn noise_leaves_transparent_pixels_alone() {
        let mut pm = Pixmap::new(4, 4);
        add_noise(&mut pm, 100.0, false, false);
        assert!(pm.as_bytes().iter().all(|&b| b == 0));
    }

    #[test]
    fn zero_amount_noise_is_a_no_op() {
        let mut pm = Pixmap::filled(4, 4, Rgba8::new(100, 100, 100, 255));
        let before = pm.as_bytes().to_vec();
        add_noise(&mut pm, 0.0, false, false);
        assert_eq!(pm.as_bytes(), &before[..]);
    }

    #[test]
    fn monochromatic_noise_keeps_channels_equal() {
        let mut pm = Pixmap::filled(8, 8, Rgba8::new(128, 128, 128, 255));
        add_noise(&mut pm, 30.0, true, false);
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
        add_noise(&mut pm, 100.0, false, false);
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
                amount: 10.0,
                monochromatic: true,
                gaussian: false,
            },
            Filter::Median { radius: 2 },
            Filter::DustAndScratches {
                radius: 2,
                threshold: 15,
            },
        ];
        for f in all {
            assert!(!f.name().is_empty());
        }
    }
}

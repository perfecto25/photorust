//! The focus tools: **Blur** and **Sharpen**.
//!
//! One tool with its sign flipped. Both read the same 3×3 neighbourhood and both
//! move a pixel along the line between itself and that neighbourhood's average —
//! Blur *toward* it, Sharpen *away* from it. Everything else, from the dab loop
//! to Strength and Mode, is shared, which is why they share a module as they
//! share a button in CS6.
//!
//! Neither is the corresponding *filter*. Filter ▸ Blur ▸ Gaussian Blur and
//! Filter ▸ Sharpen are one pass over a whole layer at a radius the user picks
//! (see [`crate::filters::convolve`]). These are brushes: they work only where
//! the tip passes, and **the more it passes the stronger the effect gets**.
//!
//! That accumulation is the character of both tools, and it is why each dab
//! applies straight to the layer rather than accumulating into a mask the way a
//! paint stroke does: every dab has to work on what the last one left. A mask
//! would give one blur however long you dwelt.
//!
//! The kernel is a fixed 3×3 — deliberately small. Photoshop's focus tools do not
//! scale their radius with the brush; a big brush works a *wider area* by the
//! same amount per dab, and depth comes from working the same spot. A radius that
//! grew with the brush would turn a single click of a large tip into a smeared
//! hole.

use crate::blend::{blend_rgb, BlendMode};
use crate::brush::Brush;
use crate::buffer::{Pixmap, Rect, Rgba8};

/// Which way the pair moves a pixel relative to its neighbourhood.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(i32)]
pub enum FocusMode {
    /// Toward the average: softer.
    #[default]
    Blur = 0,
    /// Away from it: crisper.
    Sharpen = 1,
}

impl FocusMode {
    pub fn from_i32(v: i32) -> FocusMode {
        match v {
            1 => FocusMode::Sharpen,
            _ => FocusMode::Blur,
        }
    }
}

/// The focus tools' options bar.
#[derive(Clone, Copy, Debug)]
pub struct FocusOptions {
    /// Which of the pair this is.
    pub focus: FocusMode,
    /// How much each dab applies, `0.0..=1.0`. CS6's **Strength**.
    pub strength: f32,
    /// Which part of the pixel the tool is allowed to touch. CS6 offers a cut
    /// down list here — Normal, Darken, Lighten, Hue, Saturation, Color,
    /// Luminosity — because the rest make no sense for a tool whose source *is*
    /// the destination, worked on.
    pub mode: BlendMode,
    /// Read the neighbourhood from the composite rather than the active layer.
    /// The softened pixels are still written to the active layer alone.
    pub sample_all_layers: bool,
    /// Sharpen only: hold the result inside the neighbourhood's own range, which
    /// is what stops repeated passes throwing haloes and speckle. CS6 calls this
    /// **Protect Detail** and ships with it on.
    pub protect_detail: bool,
    /// The layer's Lock Transparent Pixels. Set from the layer, not the bar.
    pub preserve_alpha: bool,
}

impl Default for FocusOptions {
    fn default() -> Self {
        // CS6 opens both on Strength 50%, Mode Normal, sampling the current
        // layer, with Protect Detail ticked.
        Self {
            focus: FocusMode::Blur,
            strength: 0.5,
            mode: BlendMode::Normal,
            sample_all_layers: false,
            protect_detail: true,
            preserve_alpha: false,
        }
    }
}

/// The 3×3 Gaussian the pair works against, normalised.
const KERNEL: [f32; 9] = [
    1.0 / 16.0, 2.0 / 16.0, 1.0 / 16.0,
    2.0 / 16.0, 4.0 / 16.0, 2.0 / 16.0,
    1.0 / 16.0, 2.0 / 16.0, 1.0 / 16.0,
];

/// Apply one dab, working `pixels` in place. Returns the region changed.
///
/// `(cx, cy)` is in the pixmap's own coordinates. `sampled` is what Sample All
/// Layers reads the neighbourhood from, with its top-left in `pixels`'
/// coordinates; without it the layer reads itself.
pub fn apply_dab(
    pixels: &mut Pixmap,
    sampled: Option<(&Pixmap, (i32, i32))>,
    brush: &Brush,
    cx: f32,
    cy: f32,
    pressure: f32,
    options: &FocusOptions,
) -> Rect {
    let radius = brush.radius() * pressure.clamp(0.05, 1.0);
    let strength = options.strength.clamp(0.0, 1.0);
    if radius <= 0.0 || strength <= 0.0 {
        return Rect::default();
    }

    let bounds = Rect::new(
        (cx - radius - 1.0).floor() as i32,
        (cy - radius - 1.0).floor() as i32,
        (radius * 2.0 + 3.0) as u32,
        (radius * 2.0 + 3.0) as u32,
    );
    let region = bounds.intersect(&pixels.rect());
    if region.is_empty() {
        return Rect::default();
    }

    // The neighbourhood is read from a copy: blurring in place would feed each
    // pixel the already-softened one beside it, and the dab would smear in
    // whichever direction the loop happens to run.
    let source = match sampled {
        Some((from, origin)) => Source { pixels: from, origin },
        None => Source { pixels, origin: (0, 0) },
    };
    let read = region.inflate(1);
    let snapshot = crop_from(&source, read);

    let dab = Brush { size: radius * 2.0, ..*brush };
    let mut dirty = Rect::default();

    for y in region.y..region.bottom() {
        for x in region.x..region.right() {
            let cover = dab.pixel_coverage(
                x as f32 + 0.5 - cx,
                y as f32 + 0.5 - cy,
                brush.angle,
                brush.roundness,
            );
            if cover <= 0.0 {
                continue;
            }
            let mut weight = (cover * strength).clamp(0.0, 1.0);
            if weight <= 0.0 {
                continue;
            }

            let dst = pixels.get(x, y);
            if options.preserve_alpha {
                if dst.a == 0 {
                    continue;
                }
                weight *= dst.a as f32 / 255.0;
            }

            let averaged = kernel_at(&snapshot, read, x, y);
            let worked = match options.focus {
                FocusMode::Blur => averaged,
                // Sharpen is the same average, subtracted rather than added: the
                // pixel is pushed as far the other side of its neighbourhood as
                // the blur would have pulled it toward it.
                FocusMode::Sharpen => {
                    let bounds = options
                        .protect_detail
                        .then(|| neighbourhood_range(&snapshot, read, x, y));
                    sharpen(dst, averaged, bounds)
                }
            };
            let target = restrict(dst, worked, options.mode, options.preserve_alpha);
            let out = lerp(dst, target, weight);
            if out != dst {
                pixels.set(x, y, out);
                dirty = dirty.union(&Rect::new(x, y, 1, 1));
            }
        }
    }

    dirty
}

/// Where the neighbourhood is read from, and where that buffer sits relative to
/// the pixels being written.
struct Source<'a> {
    pixels: &'a Pixmap,
    origin: (i32, i32),
}

/// Copy `region` out of `source`, in the *target's* coordinates. Reads outside
/// the buffer come back transparent, and the clamp below keeps the kernel from
/// pulling them in at the edges.
fn crop_from(source: &Source<'_>, region: Rect) -> Pixmap {
    let mut out = Pixmap::new(region.width.max(1), region.height.max(1));
    for y in 0..region.height as i32 {
        for x in 0..region.width as i32 {
            let sx = region.x + x - source.origin.0;
            let sy = region.y + y - source.origin.1;
            out.set(x, y, source.pixels.get(sx, sy));
        }
    }
    out
}

/// The 3×3 Gaussian at `(x, y)`, reading `snapshot` which covers `region`.
///
/// Weighted in **premultiplied** colour: at the edge of a layer the neighbours
/// are transparent, and averaging their straight-alpha colour — black, by
/// convention — would draw a dark halo inward. Premultiplied, a transparent
/// neighbour contributes nothing but its (zero) coverage, which is what softening
/// an edge should do.
fn kernel_at(snapshot: &Pixmap, region: Rect, x: i32, y: i32) -> Rgba8 {
    let (mut r, mut g, mut b, mut a) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for ky in -1..=1i32 {
        for kx in -1..=1i32 {
            let w = KERNEL[((ky + 1) * 3 + kx + 1) as usize];
            // Clamp at the edges rather than sampling outside, so a blur at the
            // canvas border does not fade into nothing.
            let sx = (x + kx - region.x).clamp(0, region.width as i32 - 1);
            let sy = (y + ky - region.y).clamp(0, region.height as i32 - 1);
            let p = snapshot.get(sx, sy);
            let alpha = p.a as f32 / 255.0;
            r += p.r as f32 * alpha * w;
            g += p.g as f32 * alpha * w;
            b += p.b as f32 * alpha * w;
            a += alpha * w;
        }
    }
    if a <= 1e-6 {
        return Rgba8::TRANSPARENT;
    }
    // Back to straight alpha.
    let c = |v: f32| (v / a).round().clamp(0.0, 255.0) as u8;
    Rgba8::new(c(r), c(g), c(b), (a * 255.0).round().clamp(0.0, 255.0) as u8)
}

/// The pixel reflected through its neighbourhood average — the sharpened value.
///
/// `dst + (dst - average)` doubles the pixel's departure from its surroundings,
/// which is exactly a 3×3 unsharp mask at amount 1. `bounds`, when given, is the
/// neighbourhood's own min and max per channel: clamping to it is **Protect
/// Detail**, and it is what keeps repeated passes from overshooting into haloes
/// and blown speckle.
fn sharpen(dst: Rgba8, average: Rgba8, bounds: Option<([u8; 3], [u8; 3])>) -> Rgba8 {
    let mut out = [0u8; 3];
    let d = [dst.r, dst.g, dst.b];
    let a = [average.r, average.g, average.b];
    for c in 0..3 {
        let mut v = d[c] as f32 + (d[c] as f32 - a[c] as f32);
        if let Some((low, high)) = bounds {
            v = v.clamp(low[c] as f32, high[c] as f32);
        }
        out[c] = v.round().clamp(0.0, 255.0) as u8;
    }
    // Sharpening is about detail, not coverage: alpha is left where it is.
    Rgba8::new(out[0], out[1], out[2], dst.a)
}

/// Per-channel min and max over the 3×3 neighbourhood.
fn neighbourhood_range(snapshot: &Pixmap, region: Rect, x: i32, y: i32) -> ([u8; 3], [u8; 3]) {
    let mut low = [255u8; 3];
    let mut high = [0u8; 3];
    for ky in -1..=1i32 {
        for kx in -1..=1i32 {
            let sx = (x + kx - region.x).clamp(0, region.width as i32 - 1);
            let sy = (y + ky - region.y).clamp(0, region.height as i32 - 1);
            let p = snapshot.get(sx, sy);
            for (c, v) in [p.r, p.g, p.b].iter().enumerate() {
                low[c] = low[c].min(*v);
                high[c] = high[c].max(*v);
            }
        }
    }
    (low, high)
}

/// Narrow the worked pixel to the part of it `mode` allows through.
///
/// Normal takes the blur whole, alpha included — which is what softens the edge
/// of a layer. Every other mode is a restriction: Luminosity blurs the shading
/// and leaves the colour, Color blurs the colour and leaves the shading, and
/// none of them may change coverage.
fn restrict(dst: Rgba8, worked: Rgba8, mode: BlendMode, preserve_alpha: bool) -> Rgba8 {
    if mode == BlendMode::Normal {
        return if preserve_alpha { Rgba8 { a: dst.a, ..worked } } else { worked };
    }
    let d = [dst.r as f32 / 255.0, dst.g as f32 / 255.0, dst.b as f32 / 255.0];
    let s = [
        worked.r as f32 / 255.0,
        worked.g as f32 / 255.0,
        worked.b as f32 / 255.0,
    ];
    let out = blend_rgb(mode, d, s);
    let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    Rgba8::new(c(out[0]), c(out[1]), c(out[2]), dst.a)
}

/// Straight-alpha interpolation.
fn lerp(from: Rgba8, to: Rgba8, t: f32) -> Rgba8 {
    let t = t.clamp(0.0, 1.0);
    let c = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8;
    Rgba8::new(c(from.r, to.r), c(from.g, to.g), c(from.b, to.b), c(from.a, to.a))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brush() -> Brush {
        Brush { size: 20.0, hardness: 1.0, ..Brush::default() }
    }

    /// A hard black/white edge down the middle — the thing a blur should soften.
    fn edge() -> Pixmap {
        let mut pm = Pixmap::filled(40, 40, Rgba8::WHITE);
        pm.fill_rect(Rect::new(20, 0, 20, 40), Rgba8::BLACK);
        pm
    }

    #[test]
    fn a_dab_softens_the_edge_it_covers() {
        let mut pm = edge();
        let options = FocusOptions { strength: 1.0, ..FocusOptions::default() };
        let dirty = apply_dab(&mut pm, None, &brush(), 20.0, 20.0, 1.0, &options);
        assert!(!dirty.is_empty());

        // The pixels either side of the boundary have moved toward each other.
        let light = pm.get(19, 20).r;
        let dark = pm.get(20, 20).r;
        assert!(light < 255, "the light side was untouched: {light}");
        assert!(dark > 0, "the dark side was untouched: {dark}");
    }

    #[test]
    fn dwelling_deepens_the_blur() {
        // The defining behaviour: each dab softens what the last one left, so a
        // second pass is softer than the first. A mask-based stroke could not do
        // this — it would give one blur however long you stayed.
        let mut once = edge();
        let mut many = edge();
        let options = FocusOptions { strength: 1.0, ..FocusOptions::default() };
        apply_dab(&mut once, None, &brush(), 20.0, 20.0, 1.0, &options);
        for _ in 0..8 {
            apply_dab(&mut many, None, &brush(), 20.0, 20.0, 1.0, &options);
        }

        // Width of the transition: how many pixels are neither black nor white.
        let spread = |pm: &Pixmap| {
            (10..30).filter(|x| { let v = pm.get(*x, 20).r; v > 8 && v < 247 }).count()
        };
        assert!(spread(&many) > spread(&once),
                "eight dabs were no softer than one: {} vs {}", spread(&many), spread(&once));
    }

    #[test]
    fn nothing_outside_the_dab_is_touched() {
        let mut pm = edge();
        let before = pm.clone();
        let options = FocusOptions { strength: 1.0, ..FocusOptions::default() };
        apply_dab(&mut pm, None, &brush(), 20.0, 20.0, 1.0, &options);
        // The brush is 20px across at (20, 20), so the far corner is well clear.
        assert_eq!(pm.get(2, 2), before.get(2, 2));
        assert_eq!(pm.get(38, 38), before.get(38, 38));
    }

    #[test]
    fn strength_scales_how_much_one_dab_does() {
        let mut soft = edge();
        let mut hard = edge();
        apply_dab(&mut soft, None, &brush(), 20.0, 20.0, 1.0,
                  &FocusOptions { strength: 0.1, ..FocusOptions::default() });
        apply_dab(&mut hard, None, &brush(), 20.0, 20.0, 1.0,
                  &FocusOptions { strength: 1.0, ..FocusOptions::default() });

        let moved = |pm: &Pixmap| 255 - pm.get(19, 20).r as i32;
        assert!(moved(&hard) > moved(&soft) * 2,
                "strength barely mattered: {} vs {}", moved(&hard), moved(&soft));
    }

    #[test]
    fn a_flat_area_is_left_alone() {
        // There is nothing to average out where everything already matches, so
        // the tool must not shift the colour by rounding.
        let mut pm = Pixmap::filled(40, 40, Rgba8::opaque(123, 45, 67));
        let options = FocusOptions { strength: 1.0, ..FocusOptions::default() };
        for _ in 0..5 {
            apply_dab(&mut pm, None, &brush(), 20.0, 20.0, 1.0, &options);
        }
        assert_eq!(pm.get(20, 20), Rgba8::opaque(123, 45, 67));
    }

    #[test]
    fn blurring_the_edge_of_a_layer_does_not_darken_it() {
        // The premultiplied-average case: transparent neighbours are black by
        // convention, and averaging them straight would draw a dark rim inward.
        let mut pm = Pixmap::new(40, 40);
        pm.fill_rect(Rect::new(0, 0, 20, 40), Rgba8::opaque(240, 200, 40));
        let options = FocusOptions { strength: 1.0, ..FocusOptions::default() };
        apply_dab(&mut pm, None, &brush(), 20.0, 20.0, 1.0, &options);

        let px = pm.get(18, 20);
        assert!(px.r > 200 && px.g > 160, "the edge darkened toward black: {px:?}");
        assert!(pm.get(20, 20).a > 0, "the edge did not soften outward at all");
    }

    #[test]
    fn luminosity_mode_blurs_the_shading_and_leaves_the_colour() {
        // Two hues at the same brightness: in Luminosity there is no shading
        // difference to blur, so nothing should move.
        let mut pm = Pixmap::filled(40, 40, Rgba8::opaque(200, 100, 100));
        pm.fill_rect(Rect::new(20, 0, 20, 40), Rgba8::opaque(100, 200, 100));
        let before = pm.clone();
        let options = FocusOptions {
            strength: 1.0,
            mode: BlendMode::Luminosity,
            ..FocusOptions::default()
        };
        apply_dab(&mut pm, None, &brush(), 20.0, 20.0, 1.0, &options);

        let moved = (pm.get(19, 20).r as i32 - before.get(19, 20).r as i32).abs();
        assert!(moved <= 12, "Luminosity blurred the colour too: moved {moved}");
    }

    #[test]
    fn the_transparency_lock_keeps_the_edge_where_it_is() {
        let mut pm = Pixmap::new(40, 40);
        pm.fill_rect(Rect::new(0, 0, 20, 40), Rgba8::opaque(240, 200, 40));
        let options = FocusOptions {
            strength: 1.0,
            preserve_alpha: true,
            ..FocusOptions::default()
        };
        apply_dab(&mut pm, None, &brush(), 20.0, 20.0, 1.0, &options);
        assert_eq!(pm.get(20, 20).a, 0, "the blur spread coverage past the lock");
        assert_eq!(pm.get(10, 20).a, 255, "the opaque side lost coverage");
    }

    /// A step between two mid-tones, so sharpening has room to push either way
    /// without running into black or white.
    fn step() -> Pixmap {
        let mut pm = Pixmap::filled(40, 40, Rgba8::opaque(100, 100, 100));
        pm.fill_rect(Rect::new(20, 0, 20, 40), Rgba8::opaque(160, 160, 160));
        pm
    }

    #[test]
    fn sharpen_pushes_a_pixel_away_from_its_neighbourhood() {
        let mut pm = step();
        let before = pm.clone();
        let options = FocusOptions {
            focus: FocusMode::Sharpen,
            strength: 1.0,
            protect_detail: false,
            ..FocusOptions::default()
        };
        apply_dab(&mut pm, None, &brush(), 20.0, 20.0, 1.0, &options);

        // The step got steeper: the dark side of it darker, the light side
        // lighter. That is the definition of the tool.
        assert!(pm.get(19, 20).r < before.get(19, 20).r, "the dark side did not darken");
        assert!(pm.get(20, 20).r > before.get(20, 20).r, "the light side did not lighten");
    }

    #[test]
    fn sharpen_leaves_a_straight_ramp_alone() {
        // Sharpening exaggerates a pixel's departure from the average of its
        // neighbours — its *curvature*. A straight ramp has none: every pixel is
        // already the average of the two beside it, so an even gradient survives
        // untouched however hard it is worked. Worth pinning, because a tool that
        // banded a smooth gradient would look broken.
        let mut pm = Pixmap::new(40, 40);
        for x in 0..40i32 {
            let v = (x * 6) as u8;
            for y in 0..40 {
                pm.set(x, y, Rgba8::opaque(v, v, v));
            }
        }
        let before = pm.clone();

        let options = FocusOptions {
            focus: FocusMode::Sharpen,
            strength: 1.0,
            protect_detail: false,
            ..FocusOptions::default()
        };
        for _ in 0..4 {
            apply_dab(&mut pm, None, &brush(), 20.0, 20.0, 1.0, &options);
        }
        for x in 14..27 {
            let moved = (pm.get(x, 20).r as i32 - before.get(x, 20).r as i32).abs();
            assert!(moved <= 1, "the ramp moved by {moved} at x={x}");
        }
    }

    #[test]
    fn sharpen_undoes_what_blur_did() {
        // The clearest statement that the two are one tool with its sign
        // flipped: blur a ramp, sharpen it back, and it moves toward where it
        // started rather than further away.
        let mut pm = edge();
        let original = pm.clone();
        let blur = FocusOptions { strength: 1.0, ..FocusOptions::default() };
        apply_dab(&mut pm, None, &brush(), 20.0, 20.0, 1.0, &blur);
        let blurred = pm.clone();

        let sharpen = FocusOptions {
            focus: FocusMode::Sharpen,
            strength: 1.0,
            protect_detail: false,
            ..FocusOptions::default()
        };
        apply_dab(&mut pm, None, &brush(), 20.0, 20.0, 1.0, &sharpen);

        let gap = |a: &Pixmap, b: &Pixmap| {
            (14..26).map(|x| (a.get(x, 20).r as i32 - b.get(x, 20).r as i32).abs()).sum::<i32>()
        };
        assert!(gap(&pm, &original) < gap(&blurred, &original),
                "sharpening did not walk the blur back");
    }

    #[test]
    fn protect_detail_holds_the_result_inside_the_neighbourhood() {
        // Without it, pass after pass overshoots into blown white and crushed
        // black at every edge. With it, a pixel may not leave the range its own
        // neighbours span.
        let mut loose = edge();
        let mut protected = edge();
        let base = FocusOptions {
            focus: FocusMode::Sharpen,
            strength: 1.0,
            ..FocusOptions::default()
        };
        for _ in 0..6 {
            apply_dab(&mut loose, None, &brush(), 20.0, 20.0, 1.0,
                      &FocusOptions { protect_detail: false, ..base });
            apply_dab(&mut protected, None, &brush(), 20.0, 20.0, 1.0, &base);
        }

        // The light side of a black/white edge cannot go above white either way;
        // what Protect Detail stops is the *mid* tones being driven to the ends.
        let overshoot = |pm: &Pixmap| {
            (14..26).filter(|x| { let v = pm.get(*x, 20).r; v == 0 || v == 255 }).count()
        };
        assert!(overshoot(&protected) <= overshoot(&loose),
                "Protect Detail made the overshoot worse");
        // And it never invents a value the neighbourhood did not already span.
        for x in 14..26 {
            let v = protected.get(x, 20).r;
            assert!(v == 0 || v == 255 || (1..255).contains(&v));
        }
    }

    #[test]
    fn sharpening_a_flat_area_changes_nothing() {
        // Nothing departs from its neighbourhood, so there is nothing to
        // exaggerate — and rounding must not invent a drift.
        let mut pm = Pixmap::filled(40, 40, Rgba8::opaque(123, 45, 67));
        let options = FocusOptions {
            focus: FocusMode::Sharpen,
            strength: 1.0,
            ..FocusOptions::default()
        };
        for _ in 0..5 {
            apply_dab(&mut pm, None, &brush(), 20.0, 20.0, 1.0, &options);
        }
        assert_eq!(pm.get(20, 20), Rgba8::opaque(123, 45, 67));
    }

    #[test]
    fn sample_all_layers_reads_the_buffer_it_is_given() {
        // The neighbourhood comes from the composite; the softened pixels still
        // land on the layer passed in.
        let mut layer = Pixmap::filled(40, 40, Rgba8::WHITE);
        let mut composite = Pixmap::filled(40, 40, Rgba8::WHITE);
        composite.fill_rect(Rect::new(20, 0, 20, 40), Rgba8::BLACK);

        let options = FocusOptions { strength: 1.0, ..FocusOptions::default() };
        apply_dab(&mut layer, Some((&composite, (0, 0))), &brush(), 20.0, 20.0, 1.0, &options);
        // The layer was flat white; the edge it never had has been blurred onto it.
        assert!(layer.get(21, 20).r < 200, "the composite's edge was not sampled");
    }
}

//! Layer compositing.
//!
//! Walks the stack bottom-to-top applying the general Porter-Duff *over*
//! operator with a per-mode blend function, as specified in PDF 1.7 §11.3:
//!
//! ```text
//! ar = as + ab*(1 - as)
//! Cr = (1 - as/ar)*Cb + (as/ar)*[ (1 - ab)*Cs + ab*B(Cb, Cs) ]
//! ```
//!
//! Rows are independent, so the outer loop is parallelised with rayon.

use crate::blend::{blend_rgb, BlendMode};
use crate::buffer::{Pixmap, Rect, Rgba8};
use crate::layer::{Layer, LayerKind, LayerStack};
use rayon::prelude::*;

/// Result of a composite, in straight alpha.
pub struct CompositeResult {
    pub pixels: Pixmap,
    /// The region that was recomputed.
    pub dirty: Rect,
}

/// Composite the whole stack into a `width` x `height` image.
pub fn composite(stack: &LayerStack, width: u32, height: u32) -> Pixmap {
    composite_region(stack, width, height, Rect::from_size(width, height)).pixels
}

/// Composite only `region`, returning a full-canvas image with the rest left
/// transparent. Used for incremental repaints during a brush stroke.
pub fn composite_region(
    stack: &LayerStack,
    width: u32,
    height: u32,
    region: Rect,
) -> CompositeResult {
    let canvas = Rect::from_size(width, height);
    let region = region.intersect(&canvas);
    let mut out = Pixmap::new(width, height);

    if region.is_empty() || stack.is_empty() {
        return CompositeResult {
            pixels: out,
            dirty: region,
        };
    }

    let layers = stack.as_slice();
    let stride = out.stride();
    let y0 = region.y as usize;
    let y1 = region.bottom() as usize;

    out.as_bytes_mut()
        .par_chunks_exact_mut(stride)
        .enumerate()
        .filter(|(y, _)| *y >= y0 && *y < y1)
        .for_each(|(y, row)| {
            composite_row(layers, y as i32, region, row);
        });

    CompositeResult {
        pixels: out,
        dirty: region,
    }
}

/// Composite one scanline across `region`, writing straight-alpha RGBA8.
fn composite_row(layers: &[Layer], y: i32, region: Rect, row: &mut [u8]) {
    for x in region.x..region.right() {
        // Backdrop accumulator, straight alpha, normalised.
        let mut back_rgb = [0.0f32; 3];
        let mut back_a = 0.0f32;

        for (i, layer) in layers.iter().enumerate() {
            if layer.is_invisible() {
                continue;
            }

            // A clipping layer is confined to the alpha of the first
            // non-clipping layer beneath it — its clipping base.
            let clip = if layer.clipping {
                clip_coverage(layers, i, x, y)
            } else {
                1.0
            };
            if clip <= 0.0 {
                continue;
            }

            match &layer.kind {
                LayerKind::Adjustment(adj) => {
                    // Adjustment layers recolour the accumulated backdrop
                    // rather than contributing coverage of their own.
                    if back_a > 0.0 {
                        let adjusted = adj.apply_rgb(back_rgb);
                        let strength =
                            layer.effective_alpha() * layer.mask_at(x, y) * clip;
                        for c in 0..3 {
                            back_rgb[c] += (adjusted[c] - back_rgb[c]) * strength;
                        }
                    }
                    continue;
                }
                LayerKind::SolidColor(color) => {
                    let src_a =
                        (color.a as f32 / 255.0) * layer.effective_alpha() * layer.mask_at(x, y) * clip;
                    let src_rgb = [
                        color.r as f32 / 255.0,
                        color.g as f32 / 255.0,
                        color.b as f32 / 255.0,
                    ];
                    blend_over(
                        &mut back_rgb,
                        &mut back_a,
                        src_rgb,
                        src_a,
                        layer.blend_mode,
                    );
                    continue;
                }
                LayerKind::Raster => {}
            }

            let px = layer
                .pixels
                .get(x - layer.offset.0, y - layer.offset.1);
            if px.a == 0 {
                continue;
            }

            let mut src_a =
                (px.a as f32 / 255.0) * layer.effective_alpha() * layer.mask_at(x, y) * clip;

            if layer.blend_mode == BlendMode::Dissolve {
                // Dissolve converts partial alpha into a stochastic all-or-
                // nothing choice. The threshold is hashed from the pixel
                // position so the pattern is stable across repaints instead of
                // shimmering.
                src_a = if dissolve_threshold(x, y) < src_a { 1.0 } else { 0.0 };
            }

            if src_a <= 0.0 {
                continue;
            }

            let src_rgb = [
                px.r as f32 / 255.0,
                px.g as f32 / 255.0,
                px.b as f32 / 255.0,
            ];
            blend_over(&mut back_rgb, &mut back_a, src_rgb, src_a, layer.blend_mode);
        }

        // `row` spans the full canvas width, so index by absolute x.
        let i = x as usize * 4;
        row[i] = to_u8(back_rgb[0]);
        row[i + 1] = to_u8(back_rgb[1]);
        row[i + 2] = to_u8(back_rgb[2]);
        row[i + 3] = to_u8(back_a);
    }
}

/// The general blend-then-composite step, mutating the backdrop in place.
#[inline]
fn blend_over(
    back_rgb: &mut [f32; 3],
    back_a: &mut f32,
    src_rgb: [f32; 3],
    src_a: f32,
    mode: BlendMode,
) {
    let ab = *back_a;
    let ar = src_a + ab * (1.0 - src_a);
    if ar <= 0.0 {
        *back_rgb = [0.0; 3];
        *back_a = 0.0;
        return;
    }

    // Blending only applies where the backdrop actually exists; where it does
    // not, the source shows through unmodified. That is the `(1-ab)*Cs + ab*B`
    // term.
    let blended = blend_rgb(mode, *back_rgb, src_rgb);
    let ratio = src_a / ar;

    for c in 0..3 {
        let mixed = (1.0 - ab) * src_rgb[c] + ab * blended[c];
        back_rgb[c] = (1.0 - ratio) * back_rgb[c] + ratio * mixed;
    }
    *back_a = ar;
}

/// Alpha of the clipping base for the layer at `index`.
///
/// Scans downward past any other clipping layers to the first ordinary layer,
/// which is what the whole clipping group is masked by.
fn clip_coverage(layers: &[Layer], index: usize, x: i32, y: i32) -> f32 {
    for base in layers[..index].iter().rev() {
        if base.clipping {
            continue;
        }
        if base.is_invisible() {
            return 0.0;
        }
        let px = base.pixels.get(x - base.offset.0, y - base.offset.1);
        return (px.a as f32 / 255.0) * base.mask_at(x, y);
    }
    // No base beneath it — nothing to clip to, so it is fully hidden.
    0.0
}

/// Stable pseudo-random threshold in `[0, 1)` for Dissolve.
#[inline]
fn dissolve_threshold(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x9E3779B1) ^ (y as u32).wrapping_mul(0x85EBCA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545F491);
    h ^= h >> 13;
    (h % 10_000) as f32 / 10_000.0
}

#[inline]
fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Flatten the stack into a single opaque raster over `background`.
pub fn flatten(stack: &LayerStack, width: u32, height: u32, background: Rgba8) -> Pixmap {
    let composited = composite(stack, width, height);
    let mut out = Pixmap::filled(width, height, background);

    for y in 0..height {
        for x in 0..width {
            let src = composited.get(x as i32, y as i32);
            if src.a == 0 {
                continue;
            }
            let dst = out.get(x as i32, y as i32);
            let sa = src.a as f32 / 255.0;
            out.set(
                x as i32,
                y as i32,
                Rgba8::new(
                    lerp_u8(dst.r, src.r, sa),
                    lerp_u8(dst.g, src.g, sa),
                    lerp_u8(dst.b, src.b, sa),
                    255,
                ),
            );
        }
    }
    out
}

#[inline]
fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::LayerId;

    fn solid_layer(stack: &mut LayerStack, color: Rgba8, w: u32, h: u32) -> LayerId {
        let id = stack.allocate_id();
        stack.push(Layer::new_filled(id, "l", w, h, color));
        id
    }

    #[test]
    fn empty_stack_composites_to_transparent() {
        let stack = LayerStack::new();
        let out = composite(&stack, 4, 4);
        assert!(out.as_bytes().iter().all(|&b| b == 0));
    }

    #[test]
    fn single_opaque_layer_passes_through() {
        let mut stack = LayerStack::new();
        solid_layer(&mut stack, Rgba8::new(200, 100, 50, 255), 4, 4);
        let out = composite(&stack, 4, 4);
        assert_eq!(out.get(2, 2), Rgba8::new(200, 100, 50, 255));
    }

    #[test]
    fn opaque_top_layer_hides_the_one_below() {
        let mut stack = LayerStack::new();
        solid_layer(&mut stack, Rgba8::new(255, 0, 0, 255), 4, 4);
        solid_layer(&mut stack, Rgba8::new(0, 0, 255, 255), 4, 4);
        let out = composite(&stack, 4, 4);
        assert_eq!(out.get(1, 1), Rgba8::new(0, 0, 255, 255));
    }

    #[test]
    fn half_opacity_blends_evenly() {
        let mut stack = LayerStack::new();
        solid_layer(&mut stack, Rgba8::BLACK, 4, 4);
        let top = solid_layer(&mut stack, Rgba8::WHITE, 4, 4);
        stack.by_id_mut(top).unwrap().opacity = 0.5;

        let out = composite(&stack, 4, 4);
        let p = out.get(2, 2);
        assert!((p.r as i32 - 128).abs() <= 2, "got {}", p.r);
        assert_eq!(p.a, 255);
    }

    #[test]
    fn hidden_layers_are_skipped() {
        let mut stack = LayerStack::new();
        solid_layer(&mut stack, Rgba8::new(255, 0, 0, 255), 4, 4);
        let top = solid_layer(&mut stack, Rgba8::new(0, 255, 0, 255), 4, 4);
        stack.by_id_mut(top).unwrap().visible = false;

        let out = composite(&stack, 4, 4);
        assert_eq!(out.get(1, 1), Rgba8::new(255, 0, 0, 255));
    }

    #[test]
    fn multiply_over_white_returns_the_source() {
        let mut stack = LayerStack::new();
        solid_layer(&mut stack, Rgba8::WHITE, 4, 4);
        let top = solid_layer(&mut stack, Rgba8::new(128, 64, 32, 255), 4, 4);
        stack.by_id_mut(top).unwrap().blend_mode = BlendMode::Multiply;

        let out = composite(&stack, 4, 4);
        let p = out.get(2, 2);
        assert!((p.r as i32 - 128).abs() <= 1, "got {:?}", p);
        assert!((p.g as i32 - 64).abs() <= 1, "got {:?}", p);
    }

    #[test]
    fn screen_over_black_returns_the_source() {
        let mut stack = LayerStack::new();
        solid_layer(&mut stack, Rgba8::BLACK, 4, 4);
        let top = solid_layer(&mut stack, Rgba8::new(128, 64, 32, 255), 4, 4);
        stack.by_id_mut(top).unwrap().blend_mode = BlendMode::Screen;

        let out = composite(&stack, 4, 4);
        let p = out.get(2, 2);
        assert!((p.r as i32 - 128).abs() <= 1, "got {:?}", p);
    }

    #[test]
    fn blend_mode_over_transparent_backdrop_shows_source_unmodified() {
        // With no backdrop, the `(1-ab)*Cs` term must dominate — a Multiply
        // layer on an empty canvas should not vanish to black.
        let mut stack = LayerStack::new();
        let id = stack.allocate_id();
        stack.push(Layer::new_filled(id, "l", 4, 4, Rgba8::new(200, 150, 100, 255)));
        stack.by_id_mut(id).unwrap().blend_mode = BlendMode::Multiply;

        let out = composite(&stack, 4, 4);
        assert_eq!(out.get(2, 2), Rgba8::new(200, 150, 100, 255));
    }

    #[test]
    fn layer_offset_shifts_content() {
        let mut stack = LayerStack::new();
        let id = stack.allocate_id();
        let mut l = Layer::new_filled(id, "l", 2, 2, Rgba8::WHITE);
        l.offset = (2, 2);
        stack.push(l);

        let out = composite(&stack, 4, 4);
        assert_eq!(out.get(0, 0).a, 0, "content leaked to the origin");
        assert_eq!(out.get(3, 3), Rgba8::WHITE);
    }

    #[test]
    fn layers_partly_off_canvas_are_clipped_not_wrapped() {
        let mut stack = LayerStack::new();
        let id = stack.allocate_id();
        let mut l = Layer::new_filled(id, "l", 4, 4, Rgba8::WHITE);
        l.offset = (-2, -2);
        stack.push(l);

        let out = composite(&stack, 4, 4);
        assert_eq!(out.get(0, 0), Rgba8::WHITE);
        assert_eq!(out.get(3, 3).a, 0);
    }

    #[test]
    fn mask_hides_the_layer() {
        let mut stack = LayerStack::new();
        solid_layer(&mut stack, Rgba8::new(255, 0, 0, 255), 4, 4);
        let top = solid_layer(&mut stack, Rgba8::new(0, 0, 255, 255), 4, 4);
        stack.by_id_mut(top).unwrap().add_hide_all_mask();

        let out = composite(&stack, 4, 4);
        assert_eq!(out.get(1, 1), Rgba8::new(255, 0, 0, 255));
    }

    #[test]
    fn adjustment_layer_recolors_the_backdrop() {
        let mut stack = LayerStack::new();
        solid_layer(&mut stack, Rgba8::new(255, 255, 255, 255), 4, 4);
        let id = stack.allocate_id();
        stack.push(Layer::new_adjustment(
            id,
            "Invert",
            crate::filters::Adjustment::Invert,
        ));

        let out = composite(&stack, 4, 4);
        let p = out.get(2, 2);
        assert!(p.r < 5 && p.g < 5 && p.b < 5, "expected inverted, got {:?}", p);
        assert_eq!(p.a, 255, "adjustment layer must not change coverage");
    }

    #[test]
    fn adjustment_layer_over_nothing_stays_transparent() {
        let mut stack = LayerStack::new();
        let id = stack.allocate_id();
        stack.push(Layer::new_adjustment(
            id,
            "Invert",
            crate::filters::Adjustment::Invert,
        ));
        let out = composite(&stack, 4, 4);
        assert!(out.as_bytes().iter().all(|&b| b == 0));
    }

    #[test]
    fn clipping_layer_is_confined_to_its_base() {
        let mut stack = LayerStack::new();
        // Base covers only the left half.
        let base_id = stack.allocate_id();
        let mut base = Layer::new_raster(base_id, "base", 4, 4);
        base.pixels.fill_rect(Rect::new(0, 0, 2, 4), Rgba8::new(255, 0, 0, 255));
        stack.push(base);

        let top_id = stack.allocate_id();
        let mut top = Layer::new_filled(top_id, "clip", 4, 4, Rgba8::new(0, 0, 255, 255));
        top.clipping = true;
        stack.push(top);

        let out = composite(&stack, 4, 4);
        assert_eq!(out.get(0, 0), Rgba8::new(0, 0, 255, 255), "should show inside base");
        assert_eq!(out.get(3, 0).a, 0, "should be clipped outside base");
    }

    #[test]
    fn clipping_layer_with_no_base_is_hidden() {
        let mut stack = LayerStack::new();
        let id = stack.allocate_id();
        let mut l = Layer::new_filled(id, "clip", 4, 4, Rgba8::WHITE);
        l.clipping = true;
        stack.push(l);

        let out = composite(&stack, 4, 4);
        assert!(out.as_bytes().iter().all(|&b| b == 0));
    }

    #[test]
    fn dissolve_is_stable_across_repeated_composites() {
        let mut stack = LayerStack::new();
        solid_layer(&mut stack, Rgba8::BLACK, 8, 8);
        let top = solid_layer(&mut stack, Rgba8::WHITE, 8, 8);
        {
            let l = stack.by_id_mut(top).unwrap();
            l.blend_mode = BlendMode::Dissolve;
            l.opacity = 0.5;
        }
        let a = composite(&stack, 8, 8);
        let b = composite(&stack, 8, 8);
        assert_eq!(a.as_bytes(), b.as_bytes(), "dissolve pattern shimmered");
    }

    #[test]
    fn dissolve_produces_only_full_or_zero_source_coverage() {
        let mut stack = LayerStack::new();
        let top = solid_layer(&mut stack, Rgba8::WHITE, 8, 8);
        {
            let l = stack.by_id_mut(top).unwrap();
            l.blend_mode = BlendMode::Dissolve;
            l.opacity = 0.5;
        }
        let out = composite(&stack, 8, 8);
        for y in 0..8 {
            for x in 0..8 {
                let a = out.get(x, y).a;
                assert!(a == 0 || a == 255, "partial dissolve alpha {}", a);
            }
        }
    }

    #[test]
    fn composite_region_only_touches_that_region() {
        let mut stack = LayerStack::new();
        solid_layer(&mut stack, Rgba8::WHITE, 8, 8);

        let r = composite_region(&stack, 8, 8, Rect::new(2, 2, 3, 3));
        assert_eq!(r.dirty, Rect::new(2, 2, 3, 3));
        assert_eq!(r.pixels.get(3, 3), Rgba8::WHITE);
        assert_eq!(r.pixels.get(0, 0).a, 0, "wrote outside the region");
        assert_eq!(r.pixels.get(7, 7).a, 0, "wrote outside the region");
    }

    #[test]
    fn composite_region_clips_to_the_canvas() {
        let mut stack = LayerStack::new();
        solid_layer(&mut stack, Rgba8::WHITE, 4, 4);
        let r = composite_region(&stack, 4, 4, Rect::new(-5, -5, 100, 100));
        assert_eq!(r.dirty, Rect::from_size(4, 4));
        assert_eq!(r.pixels.get(0, 0), Rgba8::WHITE);
    }

    #[test]
    fn flatten_fills_transparent_areas_with_the_background() {
        let mut stack = LayerStack::new();
        let id = stack.allocate_id();
        let mut l = Layer::new_raster(id, "l", 4, 4);
        l.pixels.fill_rect(Rect::new(0, 0, 2, 4), Rgba8::new(255, 0, 0, 255));
        stack.push(l);

        let out = flatten(&stack, 4, 4, Rgba8::WHITE);
        assert_eq!(out.get(0, 0), Rgba8::new(255, 0, 0, 255));
        assert_eq!(out.get(3, 0), Rgba8::WHITE);
        // Flattening always yields a fully opaque image.
        for y in 0..4 {
            for x in 0..4 {
                assert_eq!(out.get(x, y).a, 255);
            }
        }
    }

    #[test]
    fn every_blend_mode_composites_without_panicking() {
        for mode in BlendMode::ALL {
            let mut stack = LayerStack::new();
            solid_layer(&mut stack, Rgba8::new(90, 140, 200, 255), 4, 4);
            let top = solid_layer(&mut stack, Rgba8::new(200, 60, 120, 180), 4, 4);
            stack.by_id_mut(top).unwrap().blend_mode = mode;

            let out = composite(&stack, 4, 4);
            let p = out.get(2, 2);
            assert_eq!(p.a, 255, "{:?} lost opacity", mode);
        }
    }
}

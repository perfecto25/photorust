//! The brush engine.
//!
//! A stroke is rendered as a series of overlapping *dabs* spaced along the
//! path, which is how Photoshop's brush works — the "Spacing" setting in the
//! Brush panel is literally the gap between dabs, as a fraction of diameter.
//!
//! Stamping dabs directly onto the layer would compound alpha wherever they
//! overlap, making a slow stroke darker than a fast one. Instead each stroke
//! accumulates coverage into a scratch mask (taking the *maximum* at every
//! pixel) and composites that onto the layer once at the end.

use crate::buffer::{Pixmap, Rect, Rgba8};
use crate::selection::Selection;

/// Brush settings, mirroring the tool options bar.
#[derive(Clone, Copy, Debug)]
pub struct Brush {
    /// Diameter in pixels.
    pub size: f32,
    /// `0.0` = fully soft falloff, `1.0` = hard edge.
    pub hardness: f32,
    /// Master opacity of the stroke, `0.0..=1.0`.
    pub opacity: f32,
    /// Per-dab flow, `0.0..=1.0`. Low flow builds up gradually.
    pub flow: f32,
    /// Dab spacing as a fraction of diameter. Photoshop's default is 25%.
    pub spacing: f32,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            size: 20.0,
            hardness: 1.0,
            opacity: 1.0,
            flow: 1.0,
            spacing: 0.25,
        }
    }
}

impl Brush {
    pub fn radius(&self) -> f32 {
        self.size / 2.0
    }

    /// Coverage of a single dab at distance `d` from its centre, `0.0..=1.0`.
    ///
    /// Hardness sets where the falloff begins: at hardness 1 the edge is a
    /// one-pixel antialiased step, at hardness 0 it ramps from the centre out.
    pub fn falloff(&self, d: f32) -> f32 {
        let r = self.radius();
        if r <= 0.0 {
            return 0.0;
        }
        if d >= r {
            return 0.0;
        }
        let hardness = self.hardness.clamp(0.0, 1.0);
        // Where the solid core ends. Always leave ~1px for antialiasing so a
        // fully hard brush still has a smooth edge.
        let core = (r * hardness).min(r - 0.5).max(0.0);
        if d <= core {
            return 1.0;
        }
        let ramp = (r - core).max(1e-6);
        let t = (d - core) / ramp;
        // Smoothstep gives a more natural shoulder than a linear ramp.
        let t = t.clamp(0.0, 1.0);
        1.0 - (t * t * (3.0 - 2.0 * t))
    }
}

/// Accumulates a single stroke's coverage before it is composited.
///
/// Held across mouse-move events for the duration of one drag.
pub struct StrokeMask {
    coverage: Pixmap,
    /// Region touched so far, so the compositor can repaint just that area.
    dirty: Rect,
    /// Where the last dab was stamped, for spacing along the path.
    last_point: Option<(f32, f32)>,
    /// Distance carried over from the previous segment, so spacing stays even
    /// across event boundaries rather than resetting at each mouse-move.
    residual: f32,
}

impl StrokeMask {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            coverage: Pixmap::new(width, height),
            dirty: Rect::default(),
            last_point: None,
            residual: 0.0,
        }
    }

    pub fn dirty(&self) -> Rect {
        self.dirty
    }

    pub fn is_empty(&self) -> bool {
        self.dirty.is_empty()
    }

    /// Coverage accumulated at a point, `0.0..=1.0`.
    pub fn coverage_at(&self, x: i32, y: i32) -> f32 {
        self.coverage.get(x, y).a as f32 / 255.0
    }

    /// Begin a stroke at `(x, y)`, stamping the first dab.
    pub fn begin(&mut self, brush: &Brush, x: f32, y: f32, pressure: f32) {
        self.last_point = None;
        self.residual = 0.0;
        self.stamp(brush, x, y, pressure);
        self.last_point = Some((x, y));
    }

    /// Extend the stroke to `(x, y)`, stamping evenly spaced dabs along the way.
    pub fn extend(&mut self, brush: &Brush, x: f32, y: f32, pressure: f32) {
        let Some((lx, ly)) = self.last_point else {
            self.begin(brush, x, y, pressure);
            return;
        };

        let dx = x - lx;
        let dy = y - ly;
        let dist = (dx * dx + dy * dy).sqrt();
        // Spacing is a fraction of diameter; clamp so a tiny spacing value
        // cannot request an unbounded number of dabs.
        let step = (brush.size * brush.spacing.max(0.01)).max(0.5);

        if dist < 1e-6 {
            return;
        }

        // Walk the segment, carrying `residual` so dab spacing is continuous
        // across separate calls.
        let mut travelled = step - self.residual;
        while travelled <= dist {
            let t = travelled / dist;
            self.stamp(brush, lx + dx * t, ly + dy * t, pressure);
            travelled += step;
        }
        self.residual = (self.residual + dist) % step;
        self.last_point = Some((x, y));
    }

    /// Stamp one dab, taking the maximum against existing coverage.
    fn stamp(&mut self, brush: &Brush, cx: f32, cy: f32, pressure: f32) {
        let pressure = pressure.clamp(0.0, 1.0);
        let radius = brush.radius() * pressure.max(0.05);
        if radius <= 0.0 {
            return;
        }
        let flow = (brush.flow * pressure).clamp(0.0, 1.0);
        if flow <= 0.0 {
            return;
        }

        // A dab affects the disc of `radius`, plus a pixel for antialiasing.
        let x0 = (cx - radius - 1.0).floor() as i32;
        let y0 = (cy - radius - 1.0).floor() as i32;
        let x1 = (cx + radius + 1.0).ceil() as i32;
        let y1 = (cy + radius + 1.0).ceil() as i32;

        let bounds = Rect::new(x0, y0, (x1 - x0).max(0) as u32, (y1 - y0).max(0) as u32);
        let clipped = bounds.intersect(&self.coverage.rect());
        if clipped.is_empty() {
            return;
        }

        // Scale the falloff curve to the pressure-adjusted radius.
        let scaled = Brush {
            size: radius * 2.0,
            ..*brush
        };

        for y in clipped.y..clipped.bottom() {
            for x in clipped.x..clipped.right() {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let d = (dx * dx + dy * dy).sqrt();
                let cov = scaled.falloff(d) * flow;
                if cov <= 0.0 {
                    continue;
                }
                let existing = self.coverage.get(x, y).a as f32 / 255.0;
                // Max, not sum: overlapping dabs within one stroke must not
                // darken the result.
                let next = existing.max(cov);
                let v = (next * 255.0 + 0.5) as u8;
                self.coverage.set(x, y, Rgba8::new(v, v, v, v));
            }
        }

        self.dirty = self.dirty.union(&clipped);
    }

    /// Composite the accumulated stroke onto `target` in `color`.
    ///
    /// `selection` and `lock_transparency` gate where paint may land, matching
    /// the marquee and the Layers-panel lock respectively.
    pub fn composite_onto(
        &self,
        target: &mut Pixmap,
        color: Rgba8,
        opacity: f32,
        offset: (i32, i32),
        selection: Option<&Selection>,
        lock_transparency: bool,
    ) -> Rect {
        if self.dirty.is_empty() {
            return Rect::default();
        }
        let opacity = opacity.clamp(0.0, 1.0);

        // An empty selection means "no marquee", i.e. paint freely — but that
        // test walks the mask, so it is answered once here rather than per
        // pixel. Asking inside the loop made one dab cost O(dab × canvas) and
        // froze the app as soon as a marquee was active.
        let selection = selection.filter(|sel| !sel.is_empty());

        let region = Rect::new(
            self.dirty.x - offset.0,
            self.dirty.y - offset.1,
            self.dirty.width,
            self.dirty.height,
        )
        .intersect(&target.rect());

        for y in region.y..region.bottom() {
            for x in region.x..region.right() {
                let doc_x = x + offset.0;
                let doc_y = y + offset.1;

                let mut alpha = self.coverage_at(doc_x, doc_y) * opacity;
                if alpha <= 0.0 {
                    continue;
                }
                if let Some(sel) = selection {
                    alpha *= sel.coverage_at(doc_x, doc_y);
                    if alpha <= 0.0 {
                        continue;
                    }
                }

                let dst = target.get(x, y);
                if lock_transparency {
                    // Paint may only darken existing pixels, never extend the
                    // layer's coverage.
                    if dst.a == 0 {
                        continue;
                    }
                    alpha *= dst.a as f32 / 255.0;
                }

                target.set(x, y, source_over(dst, color, alpha));
            }
        }
        region
    }

    /// Discard accumulated coverage, ready for the next stroke.
    pub fn reset(&mut self) {
        if !self.dirty.is_empty() {
            // Only clear what was touched — clearing the whole canvas on every
            // stroke would dominate the cost of short strokes.
            for y in self.dirty.y..self.dirty.bottom() {
                for x in self.dirty.x..self.dirty.right() {
                    self.coverage.set(x, y, Rgba8::TRANSPARENT);
                }
            }
        }
        self.dirty = Rect::default();
        self.last_point = None;
        self.residual = 0.0;
    }

    /// Resize the scratch buffer to a new canvas size, discarding coverage.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.coverage = Pixmap::new(width, height);
        self.dirty = Rect::default();
        self.last_point = None;
        self.residual = 0.0;
    }
}

/// Straight-alpha source-over of `src` at coverage `alpha` onto `dst`.
pub fn source_over(dst: Rgba8, src: Rgba8, alpha: f32) -> Rgba8 {
    let sa = (src.a as f32 / 255.0) * alpha.clamp(0.0, 1.0);
    if sa <= 0.0 {
        return dst;
    }
    let da = dst.a as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        return Rgba8::TRANSPARENT;
    }
    // Composite in premultiplied space, then convert back to straight alpha.
    let mix = |s: u8, d: u8| -> u8 {
        let s = s as f32 / 255.0;
        let d = d as f32 / 255.0;
        let v = (s * sa + d * da * (1.0 - sa)) / out_a;
        (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
    };
    Rgba8::new(
        mix(src.r, dst.r),
        mix(src.g, dst.g),
        mix(src.b, dst.b),
        (out_a * 255.0 + 0.5) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falloff_is_one_at_centre_and_zero_past_the_edge() {
        let b = Brush {
            size: 20.0,
            hardness: 1.0,
            ..Default::default()
        };
        assert_eq!(b.falloff(0.0), 1.0);
        assert_eq!(b.falloff(10.0), 0.0);
        assert_eq!(b.falloff(50.0), 0.0);
    }

    #[test]
    fn soft_brush_falls_off_gradually() {
        let soft = Brush {
            size: 20.0,
            hardness: 0.0,
            ..Default::default()
        };
        let mid = soft.falloff(5.0);
        assert!(mid > 0.0 && mid < 1.0, "expected partial coverage, got {}", mid);
        // Coverage must decrease monotonically outward.
        assert!(soft.falloff(2.0) > soft.falloff(5.0));
        assert!(soft.falloff(5.0) > soft.falloff(8.0));
    }

    #[test]
    fn hard_brush_still_antialiases_its_rim() {
        let hard = Brush {
            size: 20.0,
            hardness: 1.0,
            ..Default::default()
        };
        let rim = hard.falloff(9.8);
        assert!(rim > 0.0 && rim < 1.0, "hard edge was not antialiased: {}", rim);
    }

    #[test]
    fn zero_size_brush_paints_nothing() {
        let b = Brush {
            size: 0.0,
            ..Default::default()
        };
        assert_eq!(b.falloff(0.0), 0.0);
    }

    #[test]
    fn a_dab_marks_coverage_and_dirty_region() {
        let mut mask = StrokeMask::new(64, 64);
        let brush = Brush {
            size: 10.0,
            ..Default::default()
        };
        mask.begin(&brush, 32.0, 32.0, 1.0);

        assert!(mask.coverage_at(32, 32) > 0.9);
        assert_eq!(mask.coverage_at(0, 0), 0.0);
        assert!(!mask.dirty().is_empty());
        assert!(mask.dirty().contains(32, 32));
    }

    #[test]
    fn overlapping_dabs_do_not_compound() {
        let brush = Brush {
            size: 10.0,
            flow: 0.5,
            ..Default::default()
        };
        let mut mask = StrokeMask::new(64, 64);
        mask.begin(&brush, 32.0, 32.0, 1.0);
        let after_one = mask.coverage_at(32, 32);

        // Stamping the same spot repeatedly must not darken it.
        for _ in 0..10 {
            mask.extend(&brush, 32.0, 32.0, 1.0);
        }
        let after_many = mask.coverage_at(32, 32);
        assert!(
            (after_many - after_one).abs() < 1e-3,
            "coverage compounded: {} -> {}",
            after_one,
            after_many
        );
    }

    #[test]
    fn a_stroke_paints_a_continuous_line() {
        let mut mask = StrokeMask::new(64, 64);
        let brush = Brush {
            size: 8.0,
            spacing: 0.25,
            ..Default::default()
        };
        mask.begin(&brush, 10.0, 32.0, 1.0);
        mask.extend(&brush, 50.0, 32.0, 1.0);

        // Every point along the line should have been covered — no gaps.
        for x in 12..48 {
            assert!(
                mask.coverage_at(x, 32) > 0.5,
                "gap in stroke at x={} ({})",
                x,
                mask.coverage_at(x, 32)
            );
        }
    }

    #[test]
    fn spacing_is_continuous_across_separate_extends() {
        // Many tiny extends must produce the same continuous line as one long
        // one — the residual carry is what makes this work.
        let brush = Brush {
            size: 8.0,
            spacing: 0.5,
            ..Default::default()
        };
        let mut mask = StrokeMask::new(64, 64);
        mask.begin(&brush, 10.0, 32.0, 1.0);
        for i in 1..=40 {
            mask.extend(&brush, 10.0 + i as f32, 32.0, 1.0);
        }
        for x in 12..48 {
            assert!(mask.coverage_at(x, 32) > 0.5, "gap at x={}", x);
        }
    }

    #[test]
    fn extend_without_begin_starts_a_stroke() {
        let mut mask = StrokeMask::new(32, 32);
        let brush = Brush::default();
        mask.extend(&brush, 16.0, 16.0, 1.0);
        assert!(mask.coverage_at(16, 16) > 0.0);
    }

    #[test]
    fn strokes_clip_to_the_canvas_without_panicking() {
        let mut mask = StrokeMask::new(16, 16);
        let brush = Brush {
            size: 20.0,
            ..Default::default()
        };
        mask.begin(&brush, -50.0, -50.0, 1.0);
        mask.extend(&brush, 100.0, 100.0, 1.0);
        assert!(mask.coverage_at(8, 8) > 0.0);
    }

    #[test]
    fn reset_clears_coverage_and_dirty() {
        let mut mask = StrokeMask::new(32, 32);
        mask.begin(&Brush::default(), 16.0, 16.0, 1.0);
        assert!(!mask.is_empty());

        mask.reset();
        assert!(mask.is_empty());
        assert_eq!(mask.coverage_at(16, 16), 0.0);
    }

    #[test]
    fn composite_paints_the_color_onto_the_target() {
        let mut mask = StrokeMask::new(32, 32);
        let brush = Brush {
            size: 10.0,
            ..Default::default()
        };
        mask.begin(&brush, 16.0, 16.0, 1.0);

        let mut target = Pixmap::new(32, 32);
        let red = Rgba8::new(255, 0, 0, 255);
        mask.composite_onto(&mut target, red, 1.0, (0, 0), None, false);

        let p = target.get(16, 16);
        assert!(p.a > 250, "expected opaque paint, got {:?}", p);
        assert_eq!(p.r, 255);
        assert_eq!(target.get(0, 0).a, 0);
    }

    #[test]
    fn composite_respects_stroke_opacity() {
        let mut mask = StrokeMask::new(32, 32);
        mask.begin(
            &Brush {
                size: 10.0,
                ..Default::default()
            },
            16.0,
            16.0,
            1.0,
        );

        let mut target = Pixmap::new(32, 32);
        mask.composite_onto(&mut target, Rgba8::WHITE, 0.5, (0, 0), None, false);
        let a = target.get(16, 16).a;
        assert!((a as i32 - 128).abs() <= 3, "expected ~50% alpha, got {}", a);
    }

    #[test]
    fn composite_honours_the_selection() {
        let mut mask = StrokeMask::new(32, 32);
        mask.begin(
            &Brush {
                size: 20.0,
                ..Default::default()
            },
            16.0,
            16.0,
            1.0,
        );

        let mut sel = Selection::new(32, 32);
        sel.apply_rect(
            Rect::new(0, 0, 16, 32),
            crate::selection::SelectionOp::Replace,
        );

        let mut target = Pixmap::new(32, 32);
        mask.composite_onto(&mut target, Rgba8::WHITE, 1.0, (0, 0), Some(&sel), false);

        assert!(target.get(10, 16).a > 0, "inside selection should be painted");
        assert_eq!(target.get(20, 16).a, 0, "outside selection must be masked");
    }

    #[test]
    fn empty_selection_does_not_block_painting() {
        // No marquee means paint everywhere, not nowhere.
        let mut mask = StrokeMask::new(32, 32);
        mask.begin(
            &Brush {
                size: 10.0,
                ..Default::default()
            },
            16.0,
            16.0,
            1.0,
        );
        let sel = Selection::new(32, 32);
        let mut target = Pixmap::new(32, 32);
        mask.composite_onto(&mut target, Rgba8::WHITE, 1.0, (0, 0), Some(&sel), false);
        assert!(target.get(16, 16).a > 0);
    }

    #[test]
    fn lock_transparency_prevents_extending_coverage() {
        let mut mask = StrokeMask::new(32, 32);
        mask.begin(
            &Brush {
                size: 20.0,
                ..Default::default()
            },
            16.0,
            16.0,
            1.0,
        );

        // Target has content only on the left half.
        let mut target = Pixmap::new(32, 32);
        target.fill_rect(Rect::new(0, 0, 16, 32), Rgba8::new(0, 0, 255, 255));

        mask.composite_onto(&mut target, Rgba8::new(255, 0, 0, 255), 1.0, (0, 0), None, true);

        assert_eq!(target.get(10, 16).r, 255, "existing pixels should repaint");
        assert_eq!(target.get(20, 16).a, 0, "transparent area must stay empty");
    }

    #[test]
    fn composite_respects_layer_offset() {
        let mut mask = StrokeMask::new(64, 64);
        mask.begin(
            &Brush {
                size: 10.0,
                ..Default::default()
            },
            40.0,
            40.0,
            1.0,
        );

        // Layer sits at (32,32) in document space, so the dab at doc (40,40)
        // lands at layer-local (8,8).
        let mut target = Pixmap::new(32, 32);
        mask.composite_onto(&mut target, Rgba8::WHITE, 1.0, (32, 32), None, false);
        assert!(target.get(8, 8).a > 0, "offset was not applied");
    }

    #[test]
    fn source_over_of_opaque_source_replaces_destination() {
        let dst = Rgba8::new(0, 0, 255, 255);
        let src = Rgba8::new(255, 0, 0, 255);
        assert_eq!(source_over(dst, src, 1.0), src);
    }

    #[test]
    fn source_over_with_zero_alpha_is_a_no_op() {
        let dst = Rgba8::new(1, 2, 3, 200);
        assert_eq!(source_over(dst, Rgba8::WHITE, 0.0), dst);
    }

    #[test]
    fn source_over_onto_transparent_keeps_source_color() {
        // Compositing onto nothing must not darken the source toward black.
        let out = source_over(Rgba8::TRANSPARENT, Rgba8::new(200, 100, 50, 255), 0.5);
        assert_eq!(out.r, 200);
        assert_eq!(out.g, 100);
        assert!((out.a as i32 - 128).abs() <= 2);
    }
}

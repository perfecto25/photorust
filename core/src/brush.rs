//! The brush engine.
//!
//! A stroke is rendered as a series of overlapping *dabs* spaced along the
//! path, which is how Photoshop's brush works — the "Spacing" setting in the
//! Brush panel is literally the gap between dabs, as a fraction of diameter.
//!
//! Stamping dabs directly onto the layer would compound alpha wherever they
//! overlap, making a slow stroke darker than a fast one. Instead each stroke
//! accumulates coverage into a scratch mask and composites that onto the layer
//! once at the end, scaled by the brush's opacity.
//!
//! Within that mask a dab lays its `flow` worth of paint *over* what is already
//! there, exactly as Photoshop's does. Two things follow, and both matter:
//! a low-flow brush builds up where its dabs overlap, and a soft brush at full
//! flow saturates to a solid core — which is what stops a soft stroke reading
//! as a row of beads. Taking the maximum instead leaves the dab's own falloff
//! showing between one centre and the next: at Photoshop's default 25% spacing
//! a soft dab has fallen to about 0.84 halfway to its neighbour, and that
//! ripple runs visibly down the middle of the stroke.
//!
//! Speed still cannot darken a stroke, because dabs are placed by *distance*
//! along the path — the spacing setting — not by how many mouse events arrived.

use crate::buffer::{Pixmap, Rect, Rgba8};
use crate::selection::Selection;

/// Seed every stroke's randomness starts from.
///
/// Fixed rather than time-based so a stroke is reproducible: the live preview
/// draws the same dabs the commit does, and redo matches undo.
const STROKE_SEED: u32 = 0x1D87_2B41;

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

    // -- tip shape, the Brush panel's "Brush Tip Shape" section --
    /// Minor axis as a fraction of the major one: `1.0` is round, lower values
    /// flatten the tip into a chisel. Photoshop calls this Roundness.
    pub roundness: f32,
    /// Rotation of the tip in degrees. Only visible when `roundness < 1`.
    pub angle: f32,

    // -- scattering, the panel's "Scattering" section --
    /// How far dabs stray from the stroke, as a fraction of diameter. `0.0`
    /// keeps them on the path.
    pub scatter: f32,
    /// Dabs laid at each step. Photoshop's Count; more than one is what makes a
    /// spatter or grass brush deposit a cluster rather than a single mark.
    pub count: u32,

    // -- shape dynamics --
    /// Random size variation per dab, `0.0..=1.0` of the diameter.
    pub size_jitter: f32,
    /// Random rotation per dab, in degrees.
    pub angle_jitter: f32,
    /// Random roundness variation per dab, `0.0..=1.0`.
    pub roundness_jitter: f32,

    /// Whether dab edges are antialiased.
    ///
    /// False is the **Pencil**: every pixel is either fully painted or not
    /// touched at all. That hard, stepped edge is the whole point of the tool,
    /// and it is the only thing that distinguishes it from a hard brush.
    pub antialias: bool,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            size: 20.0,
            hardness: 1.0,
            opacity: 1.0,
            flow: 1.0,
            spacing: 0.25,
            roundness: 1.0,
            angle: 0.0,
            scatter: 0.0,
            count: 1,
            size_jitter: 0.0,
            angle_jitter: 0.0,
            roundness_jitter: 0.0,
            antialias: true,
        }
    }
}

impl Brush {
    pub fn radius(&self) -> f32 {
        self.size / 2.0
    }

    /// Coverage at an offset from the dab's centre, honouring tip shape.
    ///
    /// The offset is rotated into the tip's own frame and the minor axis
    /// stretched back to circular, so an elliptical chisel tip can reuse the
    /// same radial falloff curve as a round one.
    pub fn coverage_at(&self, dx: f32, dy: f32, angle: f32, roundness: f32) -> f32 {
        self.falloff(self.tip_distance(dx, dy, angle, roundness))
    }

    /// Where the dab's solid centre ends and the falloff begins.
    ///
    /// A full pixel is always left for the edge, even at maximum hardness — a
    /// hard round brush in Photoshop still has about a pixel of softness, and
    /// without it the stroke is a staircase rather than a line.
    pub fn core_radius(&self) -> f32 {
        let r = self.radius();
        // About a pixel and a half of feather at maximum hardness, which is what
        // Photoshop's hard round has. Scaled down on tiny brushes, where a fixed
        // 1.5px would leave no solid core at all.
        let feather = 1.5f32.min(r * 0.5);
        (r * self.hardness.clamp(0.0, 1.0)).min(r - feather).max(0.0)
    }

    /// Distance from the dab centre in the tip's own frame, where an elliptical
    /// tip has been mapped back onto a circle.
    pub fn tip_distance(&self, dx: f32, dy: f32, angle: f32, roundness: f32) -> f32 {
        let roundness = roundness.clamp(0.05, 1.0);
        let (sin, cos) = (-angle.to_radians()).sin_cos();
        let rx = dx * cos - dy * sin;
        let ry = dx * sin + dy * cos;
        (rx * rx + (ry / roundness) * (ry / roundness)).sqrt()
    }

    /// Coverage of the whole pixel centred `(dx, dy)` from the dab's centre.
    ///
    /// One sample per pixel is exact wherever the falloff is flat, but a sharp
    /// edge crosses a pixel and the single sample lands either fully inside or
    /// fully outside — which is what makes a hard brush look pixelated. Where
    /// the edge is sharp, the pixel's *area* is sampled instead, so boundary
    /// pixels get partial coverage and the stroke reads as smooth.
    pub fn pixel_coverage(&self, dx: f32, dy: f32, angle: f32, roundness: f32) -> f32 {
        let r = self.radius();

        // The Pencil: a pixel is in or out, with nothing between. Hardness has
        // no meaning here — there is no edge to soften.
        if !self.antialias {
            return if self.tip_distance(dx, dy, angle, roundness) <= r {
                1.0
            } else {
                0.0
            };
        }

        let core = self.core_radius();

        // A ramp two pixels wide or more is already a smooth gradient; averaging
        // it would be indistinguishable from sampling it once.
        if r - core >= 2.5 {
            return self.coverage_at(dx, dy, angle, roundness);
        }

        let t = self.tip_distance(dx, dy, angle, roundness);
        // Only the band the edge passes through needs the extra work. 1.2 covers
        // a pixel's half-diagonal with room to spare.
        if t < core - 1.2 {
            return 1.0;
        }
        if t > r + 1.2 {
            return 0.0;
        }

        const GRID: i32 = 4;
        let mut sum = 0.0;
        for sy in 0..GRID {
            for sx in 0..GRID {
                let ox = (sx as f32 + 0.5) / GRID as f32 - 0.5;
                let oy = (sy as f32 + 0.5) / GRID as f32 - 0.5;
                sum += self.coverage_at(dx + ox, dy + oy, angle, roundness);
            }
        }
        sum / (GRID * GRID) as f32
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
        let core = self.core_radius();
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
    /// Random state for scatter and jitter.
    ///
    /// Advanced per dab and reset at the start of each stroke, so a stroke
    /// renders identically every time it is replayed — the live preview and the
    /// committed result must agree, and so must undo and redo.
    rng: u32,
}

impl StrokeMask {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            coverage: Pixmap::new(width, height),
            dirty: Rect::default(),
            last_point: None,
            residual: 0.0,
            rng: STROKE_SEED,
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
        self.rng = STROKE_SEED;
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

    /// A deterministic random value in `0.0..1.0`.
    fn random(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        ((self.rng >> 8) & 0xFF_FFFF) as f32 / 16_777_215.0
    }

    /// A deterministic random value in `-1.0..1.0`.
    fn signed_random(&mut self) -> f32 {
        self.random() * 2.0 - 1.0
    }

    /// Lay one step of the brush: `count` dabs, scattered and jittered.
    ///
    /// Photoshop's Count and Scattering work at this level rather than per
    /// stroke — one position along the path deposits a whole cluster, which is
    /// what gives spatter and grass brushes their texture.
    fn stamp(&mut self, brush: &Brush, cx: f32, cy: f32, pressure: f32) {
        let count = brush.count.clamp(1, 16);
        for _ in 0..count {
            let (mut x, mut y) = (cx, cy);
            if brush.scatter > 0.0 {
                // Scatter is a fraction of diameter, spread either side of the
                // path in both axes.
                let reach = brush.size * brush.scatter;
                x += self.signed_random() * reach;
                y += self.signed_random() * reach;
            }

            let size = if brush.size_jitter > 0.0 {
                // Jitter only ever shrinks, as Photoshop's does: the setting is
                // the *minimum* fraction the dab may fall to.
                brush.size * (1.0 - brush.size_jitter * self.random()).max(0.05)
            } else {
                brush.size
            };
            let angle = brush.angle + brush.angle_jitter * self.signed_random();
            let roundness = if brush.roundness_jitter > 0.0 {
                (brush.roundness - brush.roundness_jitter * self.random()).clamp(0.05, 1.0)
            } else {
                brush.roundness
            };

            self.stamp_dab(brush, x, y, pressure, size, angle, roundness);
        }
    }

    /// Stamp one dab over the coverage already there.
    #[allow(clippy::too_many_arguments)]
    fn stamp_dab(
        &mut self,
        brush: &Brush,
        cx: f32,
        cy: f32,
        pressure: f32,
        size: f32,
        angle: f32,
        roundness: f32,
    ) {
        let pressure = pressure.clamp(0.0, 1.0);
        let radius = (size / 2.0) * pressure.max(0.05);
        if radius <= 0.0 {
            return;
        }
        let flow = (brush.flow * pressure).clamp(0.0, 1.0);
        if flow <= 0.0 {
            return;
        }

        // A rotated ellipse's extent in x and y is bounded by its major axis, so
        // the disc of `radius` covers it whatever the angle. Plus a pixel for
        // antialiasing.
        let x0 = (cx - radius - 1.0).floor() as i32;
        let y0 = (cy - radius - 1.0).floor() as i32;
        let x1 = (cx + radius + 1.0).ceil() as i32;
        let y1 = (cy + radius + 1.0).ceil() as i32;

        let bounds = Rect::new(x0, y0, (x1 - x0).max(0) as u32, (y1 - y0).max(0) as u32);
        let clipped = bounds.intersect(&self.coverage.rect());
        if clipped.is_empty() {
            return;
        }

        // A 1px brush has a radius of 0.5, and every surrounding pixel centre is
        // at least 0.707 away from a dab landing on a pixel boundary — so the
        // falloff below would find no coverage anywhere and paint nothing at
        // all. Photoshop's 1px brush marks exactly one pixel, so do that.
        if radius < 0.75 {
            let (px, py) = (cx.floor() as i32, cy.floor() as i32);
            if self.coverage.rect().contains(px, py) {
                let existing = self.coverage.get(px, py).a as f32 / 255.0;
                let v = ((existing + flow * (1.0 - existing)) * 255.0 + 0.5) as u8;
                self.coverage.set(px, py, Rgba8::new(v, v, v, v));
                self.dirty = self.dirty.union(&Rect::new(px, py, 1, 1));
            }
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
                let cov = scaled.pixel_coverage(dx, dy, angle, roundness) * flow;
                if cov <= 0.0 {
                    continue;
                }
                let existing = self.coverage.get(x, y).a as f32 / 255.0;
                // Over, not max: a dab covers what is under it in proportion to
                // what it leaves uncovered, so overlapping dabs saturate toward
                // full coverage rather than tracing each dab's own falloff (see
                // the module comment). Never past full, so the stroke still
                // tops out at the brush's opacity however slowly it is drawn.
                let next = existing + cov * (1.0 - existing);
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

    /// Composite *source pixels* through the stroke's coverage — the Clone
    /// Stamp.
    ///
    /// The twin of [`StrokeMask::composite_onto`], and deliberately so: opacity,
    /// the selection and the transparency lock behave identically, and the only
    /// difference is where the colour comes from. `source` is in `target`'s
    /// coordinates and `source_offset` is added to a target pixel to find it, so
    /// `(-40, 0)` clones from forty pixels to the left.
    ///
    /// Pixels whose source falls outside the snapshot are left alone. There is
    /// nothing to clone from there, and painting transparency instead would
    /// punch holes in the layer.
    #[allow(clippy::too_many_arguments)]
    pub fn composite_source_onto(
        &self,
        target: &mut Pixmap,
        source: &Pixmap,
        source_offset: (i32, i32),
        opacity: f32,
        offset: (i32, i32),
        selection: Option<&Selection>,
        lock_transparency: bool,
    ) -> Rect {
        if self.dirty.is_empty() {
            return Rect::default();
        }
        let opacity = opacity.clamp(0.0, 1.0);
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

                let (sx, sy) = (x + source_offset.0, y + source_offset.1);
                if !source.rect().contains(sx, sy) {
                    continue;
                }
                let src = source.get(sx, sy);

                let dst = target.get(x, y);
                if lock_transparency {
                    if dst.a == 0 {
                        continue;
                    }
                    alpha *= dst.a as f32 / 255.0;
                }

                target.set(x, y, source_over(dst, src, alpha));
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
    fn a_stroke_never_paints_past_full_coverage() {
        // Dabs build on each other, so the guard against a slow stroke coming
        // out darker is that coverage saturates: going over the same ground
        // again and again reaches full and stops there, and the composite then
        // scales it by the brush's opacity.
        let brush = Brush {
            size: 10.0,
            flow: 0.5,
            ..Default::default()
        };
        let mut mask = StrokeMask::new(64, 64);
        mask.begin(&brush, 20.0, 32.0, 1.0);
        for _ in 0..20 {
            mask.extend(&brush, 44.0, 32.0, 1.0);
            mask.extend(&brush, 20.0, 32.0, 1.0);
        }
        let covered = mask.coverage_at(32, 32);
        assert!(covered > 0.99, "repeated passes did not reach full coverage: {covered}");
        assert!(covered <= 1.0, "coverage ran past full: {covered}");
    }

    #[test]
    fn a_soft_stroke_does_not_bead_along_its_middle() {
        // The regression behind "the eraser is choppy": with dabs combined by
        // maximum, a soft dab's own falloff showed between one centre and the
        // next — about 16% down at Photoshop's default 25% spacing, which reads
        // as a string of beads. Laying each dab over the last leaves the middle
        // of the stroke essentially flat.
        let brush = Brush {
            size: 38.0,
            hardness: 0.0,
            flow: 1.0,
            spacing: 0.25,
            ..Default::default()
        };
        let mut mask = StrokeMask::new(180, 64);
        mask.begin(&brush, 20.0, 32.0, 1.0);
        mask.extend(&brush, 160.0, 32.0, 1.0);

        let mut lowest = 1.0f32;
        let mut highest = 0.0f32;
        for x in 40..140 {
            let covered = mask.coverage_at(x, 32);
            lowest = lowest.min(covered);
            highest = highest.max(covered);
        }
        assert!(lowest > 0.95, "the middle of the stroke thinned to {lowest}");
        assert!(
            highest - lowest < 0.03,
            "the stroke ripples from {lowest} to {highest} along its middle"
        );
    }

    #[test]
    fn low_flow_builds_up_where_dabs_overlap() {
        // Flow is how much paint one dab lays down, so a quarter-flow brush
        // reaches much further than a quarter after a stroke's worth of
        // overlapping dabs — Photoshop's airbrush-like build-up.
        let brush = Brush {
            size: 20.0,
            hardness: 1.0,
            flow: 0.25,
            spacing: 0.25,
            ..Default::default()
        };
        let mut mask = StrokeMask::new(64, 64);
        mask.begin(&brush, 10.0, 32.0, 1.0);
        let one_dab = mask.coverage_at(10, 32);
        assert!((one_dab - 0.25).abs() < 0.02, "a single dab laid {one_dab}, not its flow");

        mask.extend(&brush, 50.0, 32.0, 1.0);
        let built_up = mask.coverage_at(30, 32);
        assert!(built_up > 0.6, "overlapping dabs did not build up: {built_up}");
        assert!(built_up <= 1.0);
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

    #[test]
    fn a_flat_tip_paints_wider_than_it_is_tall() {
        // Roundness squashes the minor axis; at angle 0 that is the vertical.
        let brush = Brush { size: 40.0, hardness: 1.0, roundness: 0.25, ..Brush::default() };
        let mut mask = StrokeMask::new(80, 80);
        mask.begin(&brush, 40.0, 40.0, 1.0);

        let across = |horizontal: bool| {
            let mut n = 0;
            for i in 0..80 {
                let (x, y) = if horizontal { (i, 40) } else { (40, i) };
                if mask.coverage_at(x, y) > 0.5 {
                    n += 1;
                }
            }
            n
        };
        let (w, h) = (across(true), across(false));
        assert!(w > h * 2, "a flat tip should be much wider than tall, got {}x{}", w, h);
    }

    #[test]
    fn tip_angle_rotates_the_shape() {
        // The same flat tip at 90 degrees should be tall rather than wide.
        let brush = Brush {
            size: 40.0,
            hardness: 1.0,
            roundness: 0.25,
            angle: 90.0,
            ..Brush::default()
        };
        let mut mask = StrokeMask::new(80, 80);
        mask.begin(&brush, 40.0, 40.0, 1.0);

        let across = |horizontal: bool| {
            let mut n = 0;
            for i in 0..80 {
                let (x, y) = if horizontal { (i, 40) } else { (40, i) };
                if mask.coverage_at(x, y) > 0.5 {
                    n += 1;
                }
            }
            n
        };
        let (w, h) = (across(true), across(false));
        assert!(h > w * 2, "at 90 degrees the tip should be tall, got {}x{}", w, h);
    }

    #[test]
    fn a_round_tip_is_unaffected_by_angle() {
        let mut a = StrokeMask::new(60, 60);
        a.begin(&Brush { size: 20.0, ..Brush::default() }, 30.0, 30.0, 1.0);
        let mut b = StrokeMask::new(60, 60);
        b.begin(&Brush { size: 20.0, angle: 37.0, ..Brush::default() }, 30.0, 30.0, 1.0);

        for y in 0..60 {
            for x in 0..60 {
                assert!(
                    (a.coverage_at(x, y) - b.coverage_at(x, y)).abs() < 1e-6,
                    "rotating a round tip changed it at {}, {}",
                    x,
                    y
                );
            }
        }
    }

    #[test]
    fn scatter_spreads_dabs_off_the_path() {
        let plain = Brush { size: 8.0, hardness: 1.0, ..Brush::default() };
        let scattered = Brush { scatter: 1.5, count: 8, ..plain };

        let extent = |brush: &Brush| -> i32 {
            let mut mask = StrokeMask::new(120, 120);
            mask.begin(brush, 60.0, 60.0, 1.0);
            let mut top = 120;
            let mut bottom = -1;
            for y in 0..120 {
                for x in 0..120 {
                    if mask.coverage_at(x, y) > 0.1 {
                        top = top.min(y);
                        bottom = bottom.max(y);
                    }
                }
            }
            bottom - top
        };

        let spread = extent(&scattered);
        let tight = extent(&plain);
        assert!(spread > tight * 2, "scatter did not spread the dabs: {} vs {}", spread, tight);
    }

    #[test]
    fn count_deposits_more_than_one_dab() {
        // With scatter, a higher count must cover more ground.
        let one = Brush { size: 6.0, hardness: 1.0, scatter: 1.0, count: 1, ..Brush::default() };
        let many = Brush { count: 12, ..one };

        let inked = |brush: &Brush| -> usize {
            let mut mask = StrokeMask::new(100, 100);
            mask.begin(brush, 50.0, 50.0, 1.0);
            let mut n = 0;
            for y in 0..100 {
                for x in 0..100 {
                    if mask.coverage_at(x, y) > 0.1 {
                        n += 1;
                    }
                }
            }
            n
        };
        assert!(inked(&many) > inked(&one) * 3, "a high count deposited no extra ink");
    }

    #[test]
    fn a_stroke_renders_the_same_way_twice() {
        // Scatter and jitter are random, but a stroke has to be reproducible:
        // the live preview must match the commit, and redo must match undo.
        let brush = Brush {
            size: 12.0,
            scatter: 1.0,
            count: 6,
            size_jitter: 0.6,
            angle_jitter: 90.0,
            roundness: 0.4,
            ..Brush::default()
        };

        let render = || {
            let mut mask = StrokeMask::new(120, 60);
            mask.begin(&brush, 20.0, 30.0, 1.0);
            mask.extend(&brush, 60.0, 30.0, 1.0);
            mask.extend(&brush, 100.0, 30.0, 1.0);
            let mut out = Vec::new();
            for y in 0..60 {
                for x in 0..120 {
                    out.push((mask.coverage_at(x, y) * 255.0) as u8);
                }
            }
            out
        };
        assert_eq!(render(), render(), "the same stroke rendered differently");
    }

    #[test]
    fn size_jitter_only_ever_shrinks() {
        // Photoshop's jitter varies downward from the set size, so a jittered
        // brush must never paint wider than an unjittered one.
        let plain = Brush { size: 30.0, hardness: 1.0, ..Brush::default() };
        let jittered = Brush { size_jitter: 0.9, ..plain };

        let width = |brush: &Brush| -> i32 {
            let mut mask = StrokeMask::new(80, 80);
            mask.begin(brush, 40.0, 40.0, 1.0);
            let mut n = 0;
            for x in 0..80 {
                if mask.coverage_at(x, 40) > 0.1 {
                    n += 1;
                }
            }
            n
        };
        assert!(width(&jittered) <= width(&plain), "jitter made the dab larger");
    }

    #[test]
    fn a_one_pixel_brush_marks_exactly_one_pixel() {
        // A radius of 0.5 puts every neighbouring pixel centre beyond the dab,
        // so without a special case this painted nothing at all.
        let brush = Brush { size: 1.0, hardness: 1.0, ..Brush::default() };
        let mut mask = StrokeMask::new(20, 20);
        mask.begin(&brush, 10.0, 10.0, 1.0);

        let mut inked = Vec::new();
        for y in 0..20 {
            for x in 0..20 {
                if mask.coverage_at(x, y) > 0.5 {
                    inked.push((x, y));
                }
            }
        }
        assert_eq!(inked, vec![(10, 10)], "a 1px brush should mark one pixel");
    }

    #[test]
    fn a_one_pixel_brush_draws_a_continuous_line() {
        let brush = Brush { size: 1.0, hardness: 1.0, spacing: 0.25, ..Brush::default() };
        let mut mask = StrokeMask::new(40, 20);
        mask.begin(&brush, 5.0, 10.0, 1.0);
        mask.extend(&brush, 34.0, 10.0, 1.0);

        let mut gaps = 0;
        for x in 5..34 {
            if mask.coverage_at(x, 10) <= 0.5 {
                gaps += 1;
            }
        }
        assert_eq!(gaps, 0, "the 1px line had {} gaps", gaps);
    }

    #[test]
    fn a_hard_brush_still_has_an_antialiased_edge() {
        // The bug this guards: coverage used to be sampled once at each pixel
        // centre, so a sharp edge landed either fully in or fully out and a hard
        // brush painted a staircase. Boundary pixels must get partial coverage.
        let brush = Brush { size: 9.0, hardness: 1.0, ..Brush::default() };
        let mut mask = StrokeMask::new(30, 30);
        mask.begin(&brush, 15.0, 15.0, 1.0);

        let mut partial = 0;
        for x in 0..30 {
            let c = mask.coverage_at(x, 15);
            if c > 0.02 && c < 0.98 {
                partial += 1;
            }
        }
        assert!(partial >= 2, "no antialiasing across the dab: {} partial pixels", partial);
    }

    #[test]
    fn a_stroke_edge_is_graded_rather_than_a_step() {
        // Along a horizontal stroke the top edge should fade over a row or two,
        // not jump straight from nothing to solid.
        let brush = Brush { size: 9.0, hardness: 1.0, ..Brush::default() };
        let mut mask = StrokeMask::new(60, 30);
        mask.begin(&brush, 10.0, 15.0, 1.0);
        mask.extend(&brush, 50.0, 15.0, 1.0);

        let mut graded = 0;
        for y in 0..30 {
            let c = mask.coverage_at(30, y);
            if c > 0.02 && c < 0.98 {
                graded += 1;
            }
        }
        assert!(graded >= 2, "the stroke edge is a hard step: {} graded rows", graded);
    }

    #[test]
    fn a_tiny_brush_keeps_a_solid_core() {
        // The feather is capped at half the radius, so a 3px hard brush is not
        // reduced to a soft blob with nothing solid in it.
        let brush = Brush { size: 3.0, hardness: 1.0, ..Brush::default() };
        assert!(brush.core_radius() > 0.0, "a 3px hard brush has no solid core");

        // Centred on a pixel rather than a boundary, so the middle pixel really
        // is the middle. A 3px tip has little room for a core, so the bar is
        // "clearly the darkest thing here" rather than fully opaque.
        let mut mask = StrokeMask::new(20, 20);
        mask.begin(&brush, 10.5, 10.5, 1.0);
        let centre = mask.coverage_at(10, 10);
        assert!(centre > 0.9, "the centre of a 3px brush is only {:.2}", centre);
        assert!(centre > mask.coverage_at(11, 10) + 0.1, "the tip has no discernible core");
    }

    #[test]
    fn a_soft_brush_is_not_area_sampled_needlessly() {
        // A wide falloff is already smooth, so it takes the cheap path — this
        // checks the cheap path still produces the same gradient.
        let brush = Brush { size: 40.0, hardness: 0.0, ..Brush::default() };
        let mut mask = StrokeMask::new(60, 60);
        mask.begin(&brush, 30.0, 30.0, 1.0);

        // Coverage should fall monotonically from the centre outward.
        let mut last = 1.1f32;
        for x in 30..50 {
            let c = mask.coverage_at(x, 30);
            assert!(c <= last + 1e-3, "soft falloff rose at x = {}", x);
            last = c;
        }
        assert!(mask.coverage_at(30, 30) > 0.9, "the centre should be solid");
    }

    #[test]
    fn the_pencil_paints_no_partial_pixels() {
        // Aliased by definition: every pixel is fully painted or untouched.
        let pencil = Brush { size: 9.0, antialias: false, ..Brush::default() };
        let mut mask = StrokeMask::new(40, 40);
        mask.begin(&pencil, 20.0, 20.0, 1.0);
        mask.extend(&pencil, 34.0, 30.0, 1.0);

        for y in 0..40 {
            for x in 0..40 {
                let c = mask.coverage_at(x, y);
                assert!(
                    c <= 0.001 || c >= 0.999,
                    "the pencil left a partial pixel at {}, {}: {:.3}",
                    x, y, c
                );
            }
        }
    }

    #[test]
    fn the_pencil_ignores_hardness() {
        // There is no edge to soften, so a soft setting must paint the same disc.
        let hard = Brush { size: 11.0, hardness: 1.0, antialias: false, ..Brush::default() };
        let soft = Brush { hardness: 0.0, ..hard };

        let render = |brush: &Brush| {
            let mut mask = StrokeMask::new(30, 30);
            mask.begin(brush, 15.0, 15.0, 1.0);
            let mut out = Vec::new();
            for y in 0..30 {
                for x in 0..30 {
                    out.push(mask.coverage_at(x, y) > 0.5);
                }
            }
            out
        };
        assert_eq!(render(&hard), render(&soft), "hardness changed the pencil's mark");
    }

    #[test]
    fn the_pencil_still_covers_the_same_ground_as_the_brush() {
        // Aliasing must not shrink the mark: a pencil and a hard brush of the
        // same size should paint close to the same width.
        let width = |brush: &Brush| {
            let mut mask = StrokeMask::new(40, 40);
            mask.begin(brush, 20.0, 20.0, 1.0);
            let mut n = 0;
            for x in 0..40 {
                if mask.coverage_at(x, 20) > 0.5 {
                    n += 1;
                }
            }
            n
        };
        let brush = Brush { size: 12.0, hardness: 1.0, ..Brush::default() };
        let pencil = Brush { antialias: false, ..brush };
        let (b, p) = (width(&brush), width(&pencil));
        assert!((b as i32 - p as i32).abs() <= 2, "pencil {} vs brush {} px wide", p, b);
    }
}

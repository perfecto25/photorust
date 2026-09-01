//! What a fill layer pours.
//!
//! A fill layer has no pixels: its colour is *evaluated* per pixel by the
//! compositor, from a description that stays editable. That is the whole point
//! of one — a gradient laid down as pixels is a gradient you can no longer
//! re-angle.
//!
//! The three kinds are a solid colour (which needs no description beyond the
//! colour itself, so it lives in [`crate::layer::LayerKind`] directly), a
//! gradient, and a pattern.

use crate::buffer::{Pixmap, Rect, Rgba8};
use crate::gradient::{Gradient, GradientType};

/// A gradient fill — Layer ▸ New Fill Layer ▸ Gradient.
#[derive(Clone, Debug)]
pub struct GradientFill {
    /// The ramp itself, stops and all — a preset from
    /// [`crate::gradient::preset`], or anything else built by hand.
    pub gradient: Gradient,
    pub shape: GradientType,
    /// Degrees, measured as everywhere else here: the direction the ramp runs
    /// *from*, with 90° pointing up.
    pub angle: f32,
    /// 1.0 is CS6's 100%.
    pub scale: f32,
    pub reverse: bool,
    pub dither: bool,
    /// Span the layer rather than the whole canvas. A fill layer covers the
    /// canvas, so the two agree today; the flag is carried so they can differ
    /// once a fill layer can be smaller than its document.
    pub align_with_layer: bool,
}

impl Default for GradientFill {
    fn default() -> Self {
        Self {
            gradient: Gradient::two_stop(Rgba8::BLACK, Rgba8::WHITE),
            shape: GradientType::Linear,
            angle: 90.0,
            scale: 1.0,
            reverse: false,
            dither: false,
            align_with_layer: true,
        }
    }
}

impl GradientFill {
    /// The colour at a point, given the area the ramp spans.
    ///
    /// Builds the ramp each call, so the compositor uses [`GradientFill::ramp`]
    /// once and [`GradientFill::sample`] per pixel instead.
    pub fn color_at(&self, x: i32, y: i32, span: Rect) -> Rgba8 {
        let ramp = self.ramp();
        self.sample(&ramp, x, y, span)
    }

    /// As [`GradientFill::color_at`], with the ramp already built.
    ///
    /// The compositor asks per pixel, and building a two-stop gradient for each
    /// of them would be the most expensive part of drawing one.
    pub fn sample(&self, ramp: &Gradient, x: i32, y: i32, span: Rect) -> Rgba8 {
        let centre_x = span.x as f32 + span.width as f32 / 2.0;
        let centre_y = span.y as f32 + span.height as f32 / 2.0;
        let radians = self.angle.to_radians();
        // Screen y counts downward, so the sine is negated to make 90° point up.
        let (dir_x, dir_y) = (radians.cos(), -radians.sin());

        let half = ((span.width as f32 * dir_x.abs() + span.height as f32 * dir_y.abs())
            / 2.0)
            * self.scale.max(0.01);
        let half = half.max(1.0);

        // A linear ramp runs from one side to the other, so it starts at the
        // near edge. The other four are measured outward from a point, so they
        // start at the middle — the same split the Gradient Overlay effect makes.
        let (start, axis, length) = if self.shape == GradientType::Linear {
            (
                (centre_x - dir_x * half, centre_y - dir_y * half),
                (dir_x * 2.0 * half, dir_y * 2.0 * half),
                2.0 * half,
            )
        } else {
            ((centre_x, centre_y), (dir_x * half, dir_y * half), half)
        };

        let t = crate::gradient::position_at(self.shape, x as f32, y as f32, start, axis, length)
            .clamp(0.0, 1.0);
        let stop = ramp.sample(t);
        if !self.dither {
            return stop;
        }

        // One noise value for all three channels: per-channel noise would show
        // as colour speckle rather than as grain.
        let n = crate::gradient::dither(x, y) * 1.5;
        let channel = |v: u8| (v as f32 + n).round().clamp(0.0, 255.0) as u8;
        Rgba8::new(channel(stop.r), channel(stop.g), channel(stop.b), stop.a)
    }

    /// The ramp this fill draws with, reversed if it asks to be.
    pub fn ramp(&self) -> Gradient {
        if self.reverse {
            self.gradient.reversed()
        } else {
            self.gradient.clone()
        }
    }
}

/// A pattern fill — Layer ▸ New Fill Layer ▸ Pattern.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatternFill {
    /// Index into [`crate::pattern::PATTERN_NAMES`].
    pub pattern: u32,
    /// 1.0 is CS6's 100%.
    pub scale: f32,
    /// Anchor the tiling to the layer rather than the canvas.
    pub link_with_layer: bool,
}

impl Default for PatternFill {
    fn default() -> Self {
        Self {
            pattern: 0,
            scale: 1.0,
            link_with_layer: true,
        }
    }
}

impl PatternFill {
    /// The colour at a point, from an already-generated tile.
    ///
    /// The tile is passed in rather than fetched: generating one is procedural
    /// work, and doing it per pixel would cost more than everything else the
    /// compositor does put together.
    pub fn color_at(&self, tile: &Pixmap, x: i32, y: i32, origin: (i32, i32)) -> Rgba8 {
        let (tw, th) = (tile.width() as f32, tile.height() as f32);
        if tw <= 0.0 || th <= 0.0 {
            return Rgba8::TRANSPARENT;
        }
        let scale = self.scale.clamp(0.1, 10.0);
        let anchor = if self.link_with_layer {
            (origin.0 as f32, origin.1 as f32)
        } else {
            (0.0, 0.0)
        };
        // Sampled at the scaled position rather than by scaling the tile, so a
        // pattern at 250% costs no more than one at 100% — and `rem_euclid`
        // wraps the same way left of the origin as right of it.
        let sx = ((x as f32 - anchor.0) / scale).rem_euclid(tw) as i32;
        let sy = ((y as f32 - anchor.1) / scale).rem_euclid(th) as i32;
        tile.get(sx, sy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_linear_gradient_runs_across_its_span() {
        let fill = GradientFill { angle: 0.0, ..GradientFill::default() };
        let span = Rect::from_size(100, 40);
        // At 0° the ramp runs left to right, black to white.
        assert!(fill.color_at(2, 20, span).r < 40);
        assert!(fill.color_at(97, 20, span).r > 200);
    }

    #[test]
    fn a_radial_gradient_starts_in_the_middle() {
        let fill = GradientFill { shape: GradientType::Radial, ..GradientFill::default() };
        let span = Rect::from_size(100, 100);
        assert!(fill.color_at(50, 50, span).r < 40, "the middle is the start of the ramp");
        assert!(fill.color_at(2, 2, span).r > 200);
    }

    #[test]
    fn reversing_swaps_the_ends() {
        let span = Rect::from_size(100, 40);
        let plain = GradientFill { angle: 0.0, ..GradientFill::default() };
        let reversed = GradientFill { reverse: true, ..plain.clone() };
        assert!(reversed.color_at(2, 20, span).r > plain.color_at(2, 20, span).r);
    }

    #[test]
    fn a_pattern_repeats_and_scales() {
        let tile = crate::pattern::tile(0).expect("checkerboard");
        let fill = PatternFill::default();
        // One tile along is the same pixel again.
        let a = fill.color_at(&tile, 5, 5, (0, 0));
        let b = fill.color_at(&tile, 5 + tile.width() as i32, 5, (0, 0));
        assert_eq!(a, b);

        // At double size the tile covers twice the ground, so a point half a
        // tile in matches the point a quarter of a tile in at full size.
        let doubled = PatternFill { scale: 2.0, ..fill };
        assert_eq!(doubled.color_at(&tile, 20, 20, (0, 0)), fill.color_at(&tile, 10, 10, (0, 0)));
    }

    #[test]
    fn a_linked_pattern_moves_with_its_layer() {
        let tile = crate::pattern::tile(0).expect("checkerboard");
        let linked = PatternFill::default();
        let pinned = PatternFill { link_with_layer: false, ..linked };

        // Moved ten pixels, the linked tiling reads as though nothing moved...
        assert_eq!(
            linked.color_at(&tile, 15, 15, (10, 10)),
            linked.color_at(&tile, 5, 5, (0, 0))
        );
        // ...while the pinned one stays where the canvas is.
        assert_eq!(
            pinned.color_at(&tile, 15, 15, (10, 10)),
            pinned.color_at(&tile, 15, 15, (0, 0))
        );
    }
}

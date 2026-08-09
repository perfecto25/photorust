//! Cloning — the engine behind the Clone Stamp.
//!
//! The Clone Stamp copies pixels from one part of the image to another. What it
//! copies is a **snapshot taken when the stroke begins**, not the layer as it
//! stands: with a small offset the destination overlaps the source, and reading
//! live would feed each dab its own output and smear the stroke along itself.
//! Photoshop samples per stroke for the same reason, which is why cloning with a
//! short offset repeats the source once rather than trailing forever.
//!
//! The copy itself is deliberately plain — pixels land exactly as they were
//! sampled, seam and all. That is the whole difference from the Healing Brush,
//! which transplants the source's texture and takes the destination's lighting
//! (see [`crate::healing::clone_region`]).
//!
//! The stroke reuses the ordinary brush machinery: dabs, spacing, jitter,
//! opacity and flow all accumulate into a [`crate::brush::StrokeMask`] exactly
//! as a paint stroke does, and only the thing being composited differs — source
//! pixels instead of one colour.

use crate::buffer::Pixmap;
use crate::compositor;
use crate::layer::{Layer, LayerStack};

/// Where the Clone Stamp reads from — CS6's **Sample** menu.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(i32)]
pub enum CloneSampling {
    /// The active layer alone. CS6's default, and the only one that behaves
    /// predictably when the layers below are unrelated.
    #[default]
    CurrentLayer = 0,
    /// The active layer composited with everything beneath it.
    CurrentAndBelow = 1,
    /// The whole visible image.
    AllLayers = 2,
}

impl CloneSampling {
    pub fn from_i32(v: i32) -> CloneSampling {
        match v {
            1 => CloneSampling::CurrentAndBelow,
            2 => CloneSampling::AllLayers,
            _ => CloneSampling::CurrentLayer,
        }
    }
}

/// The source of one clone stroke, in the **active layer's** coordinates.
pub struct CloneStroke {
    /// What the stroke copies from: the snapshot taken when it began.
    pub source: Pixmap,
    /// Added to a destination pixel to find its source, so `(-40, 0)` samples
    /// forty pixels to the left.
    pub offset: (i32, i32),
}

/// Snapshot the pixels a clone stroke will sample, in `target`'s own
/// coordinates.
///
/// Sampling below the active layer means compositing, which happens in document
/// space — so the result is copied back into the layer's frame here, and every
/// caller downstream can work in one coordinate system.
pub fn snapshot(
    stack: &LayerStack,
    target: &Layer,
    width: u32,
    height: u32,
    sampling: CloneSampling,
) -> Pixmap {
    let composited = match sampling {
        CloneSampling::CurrentLayer => return target.pixels.clone(),
        CloneSampling::AllLayers => compositor::composite(stack, width, height),
        CloneSampling::CurrentAndBelow => {
            // Everything up to and including the active layer. Without the
            // active layer itself in there, cloning "current and below" would
            // ignore what the user can plainly see they are pointing at.
            let mut below = LayerStack::new();
            for layer in stack.iter() {
                below.push(layer.clone());
                if layer.id == target.id {
                    break;
                }
            }
            compositor::composite(&below, width, height)
        }
    };

    let (w, h) = (target.pixels.width(), target.pixels.height());
    let mut out = Pixmap::new(w, h);
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            out.set(x, y, composited.get(x + target.offset.0, y + target.offset.1));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::{Rect, Rgba8};
    use crate::layer::Layer;

    fn stack_with_two() -> (LayerStack, u32, u32) {
        let mut stack = LayerStack::new();
        let lower = stack.allocate_id();
        stack.push(Layer::new_filled(lower, "Background", 16, 16, Rgba8::opaque(200, 0, 0)));
        let upper = stack.allocate_id();
        let mut top = Layer::new_raster(upper, "Layer 1", 16, 16);
        top.pixels.fill_rect(Rect::new(0, 0, 8, 16), Rgba8::opaque(0, 0, 200));
        stack.push(top);
        (stack, 16, 16)
    }

    #[test]
    fn sampling_the_current_layer_ignores_what_is_below() {
        let (stack, w, h) = stack_with_two();
        let top = stack.get(1).unwrap();
        let shot = snapshot(&stack, top, w, h, CloneSampling::CurrentLayer);
        // The right half of the upper layer is empty, and stays empty: the red
        // background below is none of this sampling mode's business.
        assert_eq!(shot.get(12, 8).a, 0);
        assert_eq!(shot.get(4, 8), Rgba8::opaque(0, 0, 200));
    }

    #[test]
    fn sampling_below_sees_through_to_the_layer_beneath() {
        let (stack, w, h) = stack_with_two();
        let top = stack.get(1).unwrap();
        let shot = snapshot(&stack, top, w, h, CloneSampling::CurrentAndBelow);
        assert_eq!(shot.get(12, 8), Rgba8::opaque(200, 0, 0), "the layer below was not sampled");
        assert_eq!(shot.get(4, 8), Rgba8::opaque(0, 0, 200), "the active layer was dropped");
    }

    #[test]
    fn a_snapshot_is_taken_in_the_layers_own_frame() {
        // An offset layer must still be sampled at its own coordinates, or a
        // clone on a moved layer would come from the wrong place.
        let (mut stack, w, h) = stack_with_two();
        if let Some(top) = stack.get_mut(1) {
            top.offset = (4, 0);
        }
        let top = stack.get(1).unwrap();
        let shot = snapshot(&stack, top, w, h, CloneSampling::CurrentAndBelow);
        // Layer x=0 sits at document x=4, which the upper layer covers in blue.
        assert_eq!(shot.get(0, 8), Rgba8::opaque(0, 0, 200));
        // Layer x=8 sits at document x=12, past the blue: red shows through.
        assert_eq!(shot.get(8, 8), Rgba8::opaque(200, 0, 0));
    }
}

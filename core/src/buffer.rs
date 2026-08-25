//! Pixel buffers.
//!
//! The engine works in 8-bit straight (non-premultiplied) RGBA. Straight alpha
//! is what PSD stores and what the blend-mode formulas in [`crate::blend`] are
//! defined against, so keeping it as the canonical form avoids a round-trip
//! through premultiplied space on every composite.
//!
//! Premultiplication happens once, at the very end, when handing the result to
//! Qt (`QImage::Format_RGBA8888_Premultiplied`).

/// A single straight-alpha RGBA pixel.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct Rgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba8 {
    pub const TRANSPARENT: Rgba8 = Rgba8 { r: 0, g: 0, b: 0, a: 0 };
    pub const BLACK: Rgba8 = Rgba8 { r: 0, g: 0, b: 0, a: 255 };
    pub const WHITE: Rgba8 = Rgba8 { r: 255, g: 255, b: 255, a: 255 };

    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn opaque(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

/// An axis-aligned rectangle in image space.
///
/// `x`/`y` may be negative — layers are allowed to sit partly outside the
/// canvas, exactly as in Photoshop.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }

    pub const fn from_size(width: u32, height: u32) -> Self {
        Self { x: 0, y: 0, width, height }
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub fn right(&self) -> i32 {
        self.x + self.width as i32
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.height as i32
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    /// Geometric intersection. Returns an empty rect when they do not overlap.
    pub fn intersect(&self, other: &Rect) -> Rect {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = self.right().min(other.right());
        let y1 = self.bottom().min(other.bottom());
        if x1 <= x0 || y1 <= y0 {
            Rect::default()
        } else {
            Rect::new(x0, y0, (x1 - x0) as u32, (y1 - y0) as u32)
        }
    }

    /// Smallest rect containing both. An empty operand is ignored, so this can
    /// be folded over a sequence to accumulate a dirty region.
    pub fn union(&self, other: &Rect) -> Rect {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }
        let x0 = self.x.min(other.x);
        let y0 = self.y.min(other.y);
        let x1 = self.right().max(other.right());
        let y1 = self.bottom().max(other.bottom());
        Rect::new(x0, y0, (x1 - x0) as u32, (y1 - y0) as u32)
    }

    /// Grow by `n` pixels on every side, clamping the origin at `i32` range.
    pub fn inflate(&self, n: u32) -> Rect {
        if self.is_empty() {
            return *self;
        }
        Rect::new(
            self.x - n as i32,
            self.y - n as i32,
            self.width + n * 2,
            self.height + n * 2,
        )
    }
}

/// A dense, row-major RGBA image that can store 8, 16, or 32-bit per
/// component. The raw byte layout changes with the depth, but the public
/// `get`/`set` API always speaks `Rgba8` so existing code is unaffected.
#[derive(Clone)]
pub struct Pixmap {
    width: u32,
    height: u32,
    /// Bytes per component: 1 = u8, 2 = u16, 4 = f32.
    bpc: u8,
    data: Vec<u8>,
}

impl PartialEq for Pixmap {
    fn eq(&self, other: &Self) -> bool {
        self.width == other.width
            && self.height == other.height
            && self.bpc == other.bpc
            && self.data == other.data
    }
}
impl Eq for Pixmap {}

impl Pixmap {
    /// Allocate a fully transparent 8-bit pixmap.
    pub fn new(width: u32, height: u32) -> Self {
        Self::new_with_depth(width, height, 1)
    }

    /// Allocate a fully transparent pixmap at the given depth.
    pub fn new_with_depth(width: u32, height: u32, bpc: u8) -> Self {
        let bpc = match bpc { 2 | 4 => bpc, _ => 1 };
        Self {
            width,
            height,
            bpc,
            data: vec![0u8; (width as usize) * (height as usize) * 4 * bpc as usize],
        }
    }

    /// Allocate and fill with a single colour (always 8-bit).
    pub fn filled(width: u32, height: u32, color: Rgba8) -> Self {
        let mut pm = Self::new(width, height);
        pm.fill(color);
        pm
    }

    /// Wrap existing 8-bit bytes. Returns `None` unless `data.len() == w * h * 4`.
    pub fn from_raw(width: u32, height: u32, data: Vec<u8>) -> Option<Self> {
        if data.len() != (width as usize) * (height as usize) * 4 {
            return None;
        }
        Some(Self { width, height, bpc: 1, data })
    }

    /// Bytes per component (1 = 8-bit, 2 = 16-bit, 4 = 32-bit float).
    pub fn bpc(&self) -> u8 {
        self.bpc
    }

    /// Bit depth as shown in the UI (8, 16, or 32).
    pub fn bit_depth(&self) -> u8 {
        self.bpc * 8
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn rect(&self) -> Rect {
        Rect::from_size(self.width, self.height)
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Bytes per row.
    pub fn stride(&self) -> usize {
        self.width as usize * 4 * self.bpc as usize
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Consume the pixmap and return its bytes. Used to hand ownership to Qt.
    pub fn into_bytes(self) -> Vec<u8> {
        self.data
    }

    /// Immutable row `y`. Panics if `y` is out of range.
    pub fn row(&self, y: u32) -> &[u8] {
        let start = y as usize * self.stride();
        &self.data[start..start + self.stride()]
    }

    /// Mutable row `y`. Panics if `y` is out of range.
    pub fn row_mut(&mut self, y: u32) -> &mut [u8] {
        let stride = self.stride();
        let start = y as usize * stride;
        &mut self.data[start..start + stride]
    }

    /// Iterate rows in parallel-friendly chunks.
    pub fn rows(&self) -> impl Iterator<Item = &[u8]> {
        self.data.chunks_exact(self.stride())
    }

    pub fn rows_mut(&mut self) -> impl Iterator<Item = &mut [u8]> {
        let stride = self.stride();
        self.data.chunks_exact_mut(stride)
    }

    /// Read a pixel as `Rgba8`, converting from the internal depth.
    pub fn get(&self, x: i32, y: i32) -> Rgba8 {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return Rgba8::TRANSPARENT;
        }
        let px_offset = y as usize * self.width as usize + x as usize;
        match self.bpc {
            2 => {
                let i = px_offset * 8;
                let r = u16::from_ne_bytes([self.data[i], self.data[i + 1]]);
                let g = u16::from_ne_bytes([self.data[i + 2], self.data[i + 3]]);
                let b = u16::from_ne_bytes([self.data[i + 4], self.data[i + 5]]);
                let a = u16::from_ne_bytes([self.data[i + 6], self.data[i + 7]]);
                Rgba8::new(
                    (r >> 8) as u8,
                    (g >> 8) as u8,
                    (b >> 8) as u8,
                    (a >> 8) as u8,
                )
            }
            4 => {
                let i = px_offset * 16;
                let r = f32::from_ne_bytes([self.data[i], self.data[i+1], self.data[i+2], self.data[i+3]]);
                let g = f32::from_ne_bytes([self.data[i+4], self.data[i+5], self.data[i+6], self.data[i+7]]);
                let b = f32::from_ne_bytes([self.data[i+8], self.data[i+9], self.data[i+10], self.data[i+11]]);
                let a = f32::from_ne_bytes([self.data[i+12], self.data[i+13], self.data[i+14], self.data[i+15]]);
                Rgba8::new(
                    (r.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
                    (g.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
                    (b.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
                    (a.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
                )
            }
            _ => {
                let i = px_offset * 4;
                Rgba8::new(self.data[i], self.data[i + 1], self.data[i + 2], self.data[i + 3])
            }
        }
    }

    /// Write a pixel from `Rgba8`, converting to the internal depth.
    pub fn set(&mut self, x: i32, y: i32, px: Rgba8) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let px_offset = y as usize * self.width as usize + x as usize;
        match self.bpc {
            2 => {
                let i = px_offset * 8;
                for (c, v) in [(0, px.r), (1, px.g), (2, px.b), (3, px.a)] {
                    let v16: u16 = (v as u16) << 8 | v as u16;
                    let bytes = v16.to_ne_bytes();
                    self.data[i + c * 2] = bytes[0];
                    self.data[i + c * 2 + 1] = bytes[1];
                }
            }
            4 => {
                let i = px_offset * 16;
                for (c, v) in [(0, px.r), (1, px.g), (2, px.b), (3, px.a)] {
                    let vf = v as f32 / 255.0;
                    let bytes = vf.to_ne_bytes();
                    self.data[i + c * 4..i + c * 4 + 4].copy_from_slice(&bytes);
                }
            }
            _ => {
                let i = px_offset * 4;
                self.data[i] = px.r;
                self.data[i + 1] = px.g;
                self.data[i + 2] = px.b;
                self.data[i + 3] = px.a;
            }
        }
    }

    pub fn fill(&mut self, color: Rgba8) {
        match self.bpc {
            2 => {
                let vals: [u16; 4] = [
                    (color.r as u16) << 8 | color.r as u16,
                    (color.g as u16) << 8 | color.g as u16,
                    (color.b as u16) << 8 | color.b as u16,
                    (color.a as u16) << 8 | color.a as u16,
                ];
                for px in self.data.chunks_exact_mut(8) {
                    for (c, v) in vals.iter().enumerate() {
                        let b = v.to_ne_bytes();
                        px[c * 2] = b[0];
                        px[c * 2 + 1] = b[1];
                    }
                }
            }
            4 => {
                let vals: [f32; 4] = [
                    color.r as f32 / 255.0,
                    color.g as f32 / 255.0,
                    color.b as f32 / 255.0,
                    color.a as f32 / 255.0,
                ];
                for px in self.data.chunks_exact_mut(16) {
                    for (c, v) in vals.iter().enumerate() {
                        px[c * 4..c * 4 + 4].copy_from_slice(&v.to_ne_bytes());
                    }
                }
            }
            _ => {
                for px in self.data.chunks_exact_mut(4) {
                    px[0] = color.r;
                    px[1] = color.g;
                    px[2] = color.b;
                    px[3] = color.a;
                }
            }
        }
    }

    /// Fill only within `rect`, clipped to the pixmap.
    pub fn fill_rect(&mut self, rect: Rect, color: Rgba8) {
        let r = rect.intersect(&self.rect());
        if r.is_empty() {
            return;
        }
        for y in r.y..r.bottom() {
            for x in r.x..r.right() {
                self.set(x, y, color);
            }
        }
    }

    pub fn clear(&mut self) {
        self.data.fill(0);
    }

    /// Copy out a sub-region. Areas outside the source read as transparent.
    pub fn crop(&self, rect: Rect) -> Pixmap {
        let mut out = Pixmap::new_with_depth(rect.width, rect.height, self.bpc);
        for y in 0..rect.height {
            for x in 0..rect.width {
                let px = self.get(rect.x + x as i32, rect.y + y as i32);
                out.set(x as i32, y as i32, px);
            }
        }
        out
    }

    /// Convert this pixmap to a different bit depth, returning a new pixmap.
    pub fn convert_depth(&self, new_bpc: u8) -> Pixmap {
        let new_bpc = match new_bpc { 2 | 4 => new_bpc, _ => 1 };
        if new_bpc == self.bpc {
            return self.clone();
        }
        let npixels = self.width as usize * self.height as usize;
        let mut out = Pixmap::new_with_depth(self.width, self.height, new_bpc);

        match (self.bpc, new_bpc) {
            (1, 2) => {
                // 8→16: expand u8 to u16 (v * 257 maps 0→0, 255→65535)
                for p in 0..npixels {
                    let si = p * 4;
                    let di = p * 8;
                    for c in 0..4 {
                        let v = self.data[si + c] as u16;
                        let v16 = v << 8 | v;
                        let b = v16.to_ne_bytes();
                        out.data[di + c * 2] = b[0];
                        out.data[di + c * 2 + 1] = b[1];
                    }
                }
            }
            (1, 4) => {
                // 8→32: expand u8 to f32 in [0,1]
                for p in 0..npixels {
                    let si = p * 4;
                    let di = p * 16;
                    for c in 0..4 {
                        let vf = self.data[si + c] as f32 / 255.0;
                        out.data[di + c * 4..di + c * 4 + 4].copy_from_slice(&vf.to_ne_bytes());
                    }
                }
            }
            (2, 1) => {
                // 16→8: take high byte of u16
                for p in 0..npixels {
                    let si = p * 8;
                    let di = p * 4;
                    for c in 0..4 {
                        let v16 = u16::from_ne_bytes([
                            self.data[si + c * 2],
                            self.data[si + c * 2 + 1],
                        ]);
                        out.data[di + c] = (v16 >> 8) as u8;
                    }
                }
            }
            (2, 4) => {
                // 16→32: u16 to f32
                for p in 0..npixels {
                    let si = p * 8;
                    let di = p * 16;
                    for c in 0..4 {
                        let v16 = u16::from_ne_bytes([
                            self.data[si + c * 2],
                            self.data[si + c * 2 + 1],
                        ]);
                        let vf = v16 as f32 / 65535.0;
                        out.data[di + c * 4..di + c * 4 + 4].copy_from_slice(&vf.to_ne_bytes());
                    }
                }
            }
            (4, 1) => {
                // 32→8: clamp f32 to [0,1] then scale to u8
                for p in 0..npixels {
                    let si = p * 16;
                    let di = p * 4;
                    for c in 0..4 {
                        let vf = f32::from_ne_bytes([
                            self.data[si + c * 4],
                            self.data[si + c * 4 + 1],
                            self.data[si + c * 4 + 2],
                            self.data[si + c * 4 + 3],
                        ]);
                        out.data[di + c] = (vf.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    }
                }
            }
            (4, 2) => {
                // 32→16: clamp f32 then scale to u16
                for p in 0..npixels {
                    let si = p * 16;
                    let di = p * 8;
                    for c in 0..4 {
                        let vf = f32::from_ne_bytes([
                            self.data[si + c * 4],
                            self.data[si + c * 4 + 1],
                            self.data[si + c * 4 + 2],
                            self.data[si + c * 4 + 3],
                        ]);
                        let v16 = (vf.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
                        let b = v16.to_ne_bytes();
                        out.data[di + c * 2] = b[0];
                        out.data[di + c * 2 + 1] = b[1];
                    }
                }
            }
            _ => {}
        }
        out
    }

    /// Return an 8-bit copy if not already 8-bit.
    pub fn to_8bit(&self) -> Pixmap {
        if self.bpc == 1 {
            return self.clone();
        }
        self.convert_depth(1)
    }

    /// Convert to premultiplied alpha in place.
    ///
    /// Qt's `Format_RGBA8888_Premultiplied` is the fast path for painting, so
    /// the composited result is converted once before crossing the bridge.
    pub fn premultiply(&mut self) {
        for px in self.data.chunks_exact_mut(4) {
            let a = px[3] as u32;
            if a == 255 {
                continue;
            }
            if a == 0 {
                px[0] = 0;
                px[1] = 0;
                px[2] = 0;
                continue;
            }
            // +127 rounds to nearest rather than truncating, which otherwise
            // darkens semi-transparent edges over repeated conversions.
            px[0] = ((px[0] as u32 * a + 127) / 255) as u8;
            px[1] = ((px[1] as u32 * a + 127) / 255) as u8;
            px[2] = ((px[2] as u32 * a + 127) / 255) as u8;
        }
    }

    /// Inverse of [`Pixmap::premultiply`].
    pub fn unpremultiply(&mut self) {
        for px in self.data.chunks_exact_mut(4) {
            let a = px[3] as u32;
            if a == 255 || a == 0 {
                continue;
            }
            px[0] = ((px[0] as u32 * 255 + a / 2) / a).min(255) as u8;
            px[1] = ((px[1] as u32 * 255 + a / 2) / a).min(255) as u8;
            px[2] = ((px[2] as u32 * 255 + a / 2) / a).min(255) as u8;
        }
    }

    /// Approximate memory footprint in bytes. Used by the history stack to
    /// decide when to evict old snapshots.
    /// A copy turned or mirrored, for putting a photograph the right way up.
    ///
    /// The quarter turns swap the axes, so the copy's width is this one's
    /// height. Every case is a straight remapping of whole pixels: nothing is
    /// resampled and nothing is lost, which is what makes applying a camera's
    /// orientation on the way in harmless.
    pub fn transformed(&self, how: crate::metadata::Orientation) -> Pixmap {
        use crate::metadata::Orientation;

        let (w, h) = (self.width as i32, self.height as i32);
        let (out_w, out_h) = if how.swaps_axes() {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        };

        let mut out = Pixmap::new_with_depth(out_w, out_h, self.bpc);
        for y in 0..h {
            for x in 0..w {
                // Where this pixel lands in the copy.
                let (nx, ny) = match how {
                    Orientation::Upright => (x, y),
                    Orientation::FlipHorizontal => (w - 1 - x, y),
                    Orientation::Rotate180 => (w - 1 - x, h - 1 - y),
                    Orientation::FlipVertical => (x, h - 1 - y),
                    Orientation::Transpose => (y, x),
                    Orientation::Rotate90Cw => (h - 1 - y, x),
                    Orientation::Transverse => (h - 1 - y, w - 1 - x),
                    Orientation::Rotate90Ccw => (y, w - 1 - x),
                };
                out.set(nx, ny, self.get(x, y));
            }
        }
        out
    }

    pub fn byte_size(&self) -> usize {
        self.data.len()
    }
}

impl std::fmt::Debug for Pixmap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pixmap")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 3x2 pixmap whose pixels each say where they are: red = x, green = y.
    fn positional() -> Pixmap {
        let mut pm = Pixmap::new(3, 2);
        for y in 0..2 {
            for x in 0..3 {
                pm.set(x, y, Rgba8::opaque(x as u8, y as u8, 0));
            }
        }
        pm
    }

    #[test]
    fn a_quarter_turn_swaps_the_axes_and_moves_the_corner() {
        use crate::metadata::Orientation;
        let turned = positional().transformed(Orientation::Rotate90Cw);
        assert_eq!((turned.width(), turned.height()), (2, 3));
        // Turning clockwise sends the top-left corner to the top-right.
        assert_eq!(turned.get(1, 0), Rgba8::opaque(0, 0, 0));
        assert_eq!(turned.get(1, 2), Rgba8::opaque(2, 0, 0));
    }

    #[test]
    fn a_counter_turn_is_the_inverse_of_a_turn() {
        use crate::metadata::Orientation;
        let original = positional();
        let round_trip = original
            .transformed(Orientation::Rotate90Cw)
            .transformed(Orientation::Rotate90Ccw);
        assert_eq!(round_trip.as_bytes(), original.as_bytes());
    }

    #[test]
    fn flips_mirror_the_axis_they_name() {
        use crate::metadata::Orientation;
        let flipped = positional().transformed(Orientation::FlipHorizontal);
        assert_eq!((flipped.width(), flipped.height()), (3, 2));
        assert_eq!(flipped.get(0, 0), Rgba8::opaque(2, 0, 0), "x was not mirrored");

        let flipped = positional().transformed(Orientation::FlipVertical);
        assert_eq!(flipped.get(0, 0), Rgba8::opaque(0, 1, 0), "y was not mirrored");
    }

    #[test]
    fn the_diagonal_mirrors_are_not_quarter_turns() {
        // The pair that is easiest to get wrong: a transpose mirrors along the
        // main diagonal, so a pixel's coordinates simply swap.
        use crate::metadata::Orientation;
        let transposed = positional().transformed(Orientation::Transpose);
        assert_eq!(transposed.get(0, 2), Rgba8::opaque(2, 0, 0));
        assert_eq!(transposed.get(1, 0), Rgba8::opaque(0, 1, 0));
    }

    #[test]
    fn leaving_a_pixmap_upright_changes_nothing() {
        use crate::metadata::Orientation;
        let original = positional();
        let same = original.transformed(Orientation::Upright);
        assert_eq!(same.as_bytes(), original.as_bytes());
    }

    #[test]
    fn new_pixmap_is_transparent() {
        let pm = Pixmap::new(4, 4);
        assert_eq!(pm.get(0, 0), Rgba8::TRANSPARENT);
        assert_eq!(pm.as_bytes().len(), 4 * 4 * 4);
    }

    #[test]
    fn out_of_bounds_reads_are_transparent() {
        let pm = Pixmap::filled(2, 2, Rgba8::WHITE);
        assert_eq!(pm.get(-1, 0), Rgba8::TRANSPARENT);
        assert_eq!(pm.get(0, 5), Rgba8::TRANSPARENT);
        assert_eq!(pm.get(1, 1), Rgba8::WHITE);
    }

    #[test]
    fn out_of_bounds_writes_are_dropped() {
        let mut pm = Pixmap::new(2, 2);
        pm.set(-1, 0, Rgba8::WHITE);
        pm.set(9, 9, Rgba8::WHITE);
        assert!(pm.as_bytes().iter().all(|&b| b == 0));
    }

    #[test]
    fn rect_intersect_and_union() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(5, 5, 10, 10);
        assert_eq!(a.intersect(&b), Rect::new(5, 5, 5, 5));
        assert_eq!(a.union(&b), Rect::new(0, 0, 15, 15));

        let disjoint = Rect::new(100, 100, 5, 5);
        assert!(a.intersect(&disjoint).is_empty());
    }

    #[test]
    fn union_ignores_empty_operands() {
        let a = Rect::default();
        let b = Rect::new(3, 3, 4, 4);
        assert_eq!(a.union(&b), b);
        assert_eq!(b.union(&a), b);
    }

    #[test]
    fn premultiply_roundtrips_within_one_unit() {
        let mut pm = Pixmap::new(1, 1);
        pm.set(0, 0, Rgba8::new(200, 100, 50, 128));
        pm.premultiply();
        pm.unpremultiply();
        let px = pm.get(0, 0);
        // Rounding through 8-bit premultiplied space is lossy; ±2 is the
        // expected worst case at half alpha.
        assert!((px.r as i32 - 200).abs() <= 2, "r drifted: {}", px.r);
        assert!((px.g as i32 - 100).abs() <= 2, "g drifted: {}", px.g);
        assert!((px.b as i32 - 50).abs() <= 2, "b drifted: {}", px.b);
        assert_eq!(px.a, 128);
    }

    #[test]
    fn premultiply_zeroes_fully_transparent_pixels() {
        let mut pm = Pixmap::new(1, 1);
        pm.set(0, 0, Rgba8::new(200, 100, 50, 0));
        pm.premultiply();
        assert_eq!(pm.get(0, 0), Rgba8::TRANSPARENT);
    }

    #[test]
    fn fill_rect_clips_to_bounds() {
        let mut pm = Pixmap::new(4, 4);
        pm.fill_rect(Rect::new(2, 2, 10, 10), Rgba8::WHITE);
        assert_eq!(pm.get(3, 3), Rgba8::WHITE);
        assert_eq!(pm.get(1, 1), Rgba8::TRANSPARENT);
    }

    #[test]
    fn crop_outside_source_is_transparent() {
        let pm = Pixmap::filled(4, 4, Rgba8::WHITE);
        let c = pm.crop(Rect::new(2, 2, 4, 4));
        assert_eq!(c.get(0, 0), Rgba8::WHITE);
        assert_eq!(c.get(3, 3), Rgba8::TRANSPARENT);
    }
}

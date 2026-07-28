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

/// A dense, row-major RGBA8 image.
#[derive(Clone, PartialEq, Eq)]
pub struct Pixmap {
    width: u32,
    height: u32,
    /// `width * height * 4` bytes, row-major, no padding.
    data: Vec<u8>,
}

impl Pixmap {
    /// Allocate a fully transparent pixmap.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0u8; (width as usize) * (height as usize) * 4],
        }
    }

    /// Allocate and fill with a single colour.
    pub fn filled(width: u32, height: u32, color: Rgba8) -> Self {
        let mut pm = Self::new(width, height);
        pm.fill(color);
        pm
    }

    /// Wrap existing bytes. Returns `None` unless `data.len() == w * h * 4`.
    pub fn from_raw(width: u32, height: u32, data: Vec<u8>) -> Option<Self> {
        if data.len() != (width as usize) * (height as usize) * 4 {
            return None;
        }
        Some(Self { width, height, data })
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
        self.width as usize * 4
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

    /// Read a pixel. Out-of-bounds reads return transparent, which lets callers
    /// sample freely near edges without bounds-checking every access.
    pub fn get(&self, x: i32, y: i32) -> Rgba8 {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return Rgba8::TRANSPARENT;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        Rgba8::new(self.data[i], self.data[i + 1], self.data[i + 2], self.data[i + 3])
    }

    /// Write a pixel. Out-of-bounds writes are silently dropped.
    pub fn set(&mut self, x: i32, y: i32, px: Rgba8) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        self.data[i] = px.r;
        self.data[i + 1] = px.g;
        self.data[i + 2] = px.b;
        self.data[i + 3] = px.a;
    }

    pub fn fill(&mut self, color: Rgba8) {
        for px in self.data.chunks_exact_mut(4) {
            px[0] = color.r;
            px[1] = color.g;
            px[2] = color.b;
            px[3] = color.a;
        }
    }

    /// Fill only within `rect`, clipped to the pixmap.
    pub fn fill_rect(&mut self, rect: Rect, color: Rgba8) {
        let r = rect.intersect(&self.rect());
        if r.is_empty() {
            return;
        }
        for y in r.y..r.bottom() {
            let row = self.row_mut(y as u32);
            for x in r.x..r.right() {
                let i = x as usize * 4;
                row[i] = color.r;
                row[i + 1] = color.g;
                row[i + 2] = color.b;
                row[i + 3] = color.a;
            }
        }
    }

    pub fn clear(&mut self) {
        self.data.fill(0);
    }

    /// Copy out a sub-region. Areas outside the source read as transparent.
    pub fn crop(&self, rect: Rect) -> Pixmap {
        let mut out = Pixmap::new(rect.width, rect.height);
        for y in 0..rect.height {
            for x in 0..rect.width {
                let px = self.get(rect.x + x as i32, rect.y + y as i32);
                out.set(x as i32, y as i32, px);
            }
        }
        out
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

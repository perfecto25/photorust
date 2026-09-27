//! Pixel buffers.
//!
//! The engine works in 8-bit straight (non-premultiplied) RGBA. Straight alpha
//! is what PSD stores and what the blend-mode formulas in [`crate::blend`] are
//! defined against, so keeping it as the canonical form avoids a round-trip
//! through premultiplied space on every composite.
//!
//! Premultiplication happens once, at the very end, when handing the result to
//! Qt (`QImage::Format_RGBA8888_Premultiplied`).

use std::sync::atomic::{AtomicU64, Ordering};

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
pub struct Pixmap {
    width: u32,
    height: u32,
    /// Bytes per component: 1 = u8, 2 = u16, 4 = f32.
    bpc: u8,
    data: Vec<u8>,
    /// Which content this is, so history can tell an untouched layer from a
    /// changed one without comparing a gigabyte of it. See [`Pixmap::stamp`].
    stamp: AtomicU64,
}

/// The next stamp [`Pixmap::share_stamp`] hands out. Starts at 1: 0 means
/// "not known to match anything".
static NEXT_STAMP: AtomicU64 = AtomicU64::new(1);

impl Clone for Pixmap {
    fn clone(&self) -> Self {
        // A copy holds the same bytes, so it keeps the stamp.
        Self {
            width: self.width,
            height: self.height,
            bpc: self.bpc,
            data: self.data.clone(),
            stamp: AtomicU64::new(self.stamp.load(Ordering::Relaxed)),
        }
    }
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
            stamp: AtomicU64::new(0),
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
        Some(Self { width, height, bpc: 1, data, stamp: AtomicU64::new(0) })
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

    /// Which content this pixmap holds: two pixmaps with the same nonzero
    /// stamp hold the same bytes. 0 promises nothing.
    ///
    /// Committing a history step compares the whole layer stack with the
    /// copy history keeps, and on a 16000-pixel map that was a gigabyte per
    /// commit — every step of a Move-tool drag kept every core busy for it.
    /// Stamps let it skip every layer the step did not touch.
    ///
    /// The rule that keeps it true: every `&mut self` method clears the stamp
    /// before it can change a byte, a copy keeps it, and only
    /// [`Pixmap::share_stamp`] sets one — on two pixmaps already known to be
    /// equal. A new `&mut self` method must call `touch` first.
    pub fn stamp(&self) -> u64 {
        self.stamp.load(Ordering::Relaxed)
    }

    /// Mark `self` and `other` as holding the same content, which the caller
    /// has just made so. Through `&` for `other`, since what history makes
    /// its copy agree with is the live stack, which it only borrows.
    pub fn share_stamp(&mut self, other: &Pixmap) {
        let stamp = match other.stamp() {
            0 => NEXT_STAMP.fetch_add(1, Ordering::Relaxed),
            s => s,
        };
        other.stamp.store(stamp, Ordering::Relaxed);
        *self.stamp.get_mut() = stamp;
    }

    /// About to change: no longer the content any stamp promised.
    #[inline]
    fn touch(&mut self) {
        *self.stamp.get_mut() = 0;
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        self.touch();
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
        self.touch();
        let stride = self.stride();
        let start = y as usize * stride;
        &mut self.data[start..start + stride]
    }

    /// Iterate rows in parallel-friendly chunks.
    pub fn rows(&self) -> impl Iterator<Item = &[u8]> {
        self.data.chunks_exact(self.stride())
    }

    pub fn rows_mut(&mut self) -> impl Iterator<Item = &mut [u8]> {
        self.touch();
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
        self.touch();
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
        self.touch();
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
        self.touch();
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
        self.touch();
        self.data.fill(0);
    }

    /// Copy out a sub-region. Areas outside the source read as transparent.
    ///
    /// Byte-exact: `get`/`set` would narrow a 16- or 32-bit pixel to `Rgba8`
    /// and back, which is a real loss for callers that crop a region only to
    /// put it back — see [`Pixmap::blit`].
    pub fn crop(&self, rect: Rect) -> Pixmap {
        let mut out = Pixmap::new_with_depth(rect.width, rect.height, self.bpc);
        let inside = rect.intersect(&self.rect());
        if inside.is_empty() {
            return out;
        }

        let bytes_per_px = 4 * self.bpc as usize;
        let span = inside.width as usize * bytes_per_px;
        let src_stride = self.stride();
        let out_stride = out.stride();
        let src_x = inside.x as usize * bytes_per_px;
        let out_x = (inside.x - rect.x) as usize * bytes_per_px;
        for dy in 0..inside.height as usize {
            let out_y = (inside.y - rect.y) as usize + dy;
            let src_row = &self.data[(inside.y as usize + dy) * src_stride + src_x..][..span];
            out.data[out_y * out_stride + out_x..][..span].copy_from_slice(src_row);
        }
        out
    }

    /// Copy `src` in at `(x, y)`, replacing what is there.
    ///
    /// The inverse of [`Pixmap::crop`]: `crop` then `blit` at the same origin
    /// round-trips a region *byte for byte* when the depths match, which is
    /// what lets the live stroke preview borrow a region of the active layer
    /// and put it back — a round-trip through `Rgba8` would quantize a 16- or
    /// 32-bit layer on every mouse-move. Pixels landing outside this pixmap
    /// are dropped.
    pub fn blit(&mut self, src: &Pixmap, x: i32, y: i32) {
        self.touch();
        // Clip to the overlap, in this pixmap's coordinates.
        let dst = Rect::new(x, y, src.width(), src.height()).intersect(&self.rect());
        if dst.is_empty() {
            return;
        }

        if src.bpc != self.bpc {
            for dy in 0..dst.height {
                for dx in 0..dst.width {
                    let (px, py) = (dst.x + dx as i32, dst.y + dy as i32);
                    self.set(px, py, src.get(px - x, py - y));
                }
            }
            return;
        }

        let bytes_per_px = 4 * self.bpc as usize;
        let span = dst.width as usize * bytes_per_px;
        for dy in 0..dst.height {
            let sy = (dst.y - y) as usize + dy as usize;
            let sx = (dst.x - x) as usize * bytes_per_px;
            let dest_y = dst.y as usize + dy as usize;
            let dest_x = dst.x as usize * bytes_per_px;
            let src_row = &src.data[sy * src.stride() + sx..][..span];
            let stride = self.stride();
            self.data[dest_y * stride + dest_x..][..span].copy_from_slice(src_row);
        }
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

    /// The tight box around every pixel that is not fully transparent, in the
    /// pixmap's own coordinates; empty if there are none.
    ///
    /// Asked for after every stroke — the Properties panel shows it, and the
    /// transform controls are drawn round it — so it must not cost a look at
    /// every pixel. It works inward from the edges: the first and last rows
    /// with anything in them, then, row by row between those in parallel, the
    /// first and last pixel. An opaque layer answers every question at its
    /// first pixel, so a gigabyte photograph costs one pixel a row; only a
    /// mostly empty layer is scanned through, and then on every core.
    pub fn content_bounds(&self) -> Rect {
        use rayon::prelude::*;
        let (w, h) = (self.width as usize, self.height as usize);
        if w == 0 || h == 0 {
            return Rect::default();
        }
        let visible = |x: usize, y: usize| -> bool {
            if self.bpc == 1 {
                self.data[(y * w + x) * 4 + 3] > 0
            } else {
                self.get(x as i32, y as i32).a > 0
            }
        };
        let row_has = |y: usize| (0..w).any(|x| visible(x, y));
        let Some(top) = (0..h).find(|&y| row_has(y)) else {
            return Rect::default();
        };
        let bottom = (top..h).rev().find(|&y| row_has(y)).unwrap_or(top);
        let (left, right) = (top..=bottom)
            .into_par_iter()
            .filter_map(|y| {
                let first = (0..w).find(|&x| visible(x, y))?;
                let last = (first..w).rev().find(|&x| visible(x, y)).unwrap_or(first);
                Some((first, last))
            })
            .reduce(|| (usize::MAX, 0), |a, b| (a.0.min(b.0), a.1.max(b.1)));
        Rect::new(left as i32, top as i32, (right - left + 1) as u32, (bottom - top + 1) as u32)
    }

    /// Convert to premultiplied alpha in place.
    ///
    /// Qt's `Format_RGBA8888_Premultiplied` is the fast path for painting, so
    /// the composited result is converted once before crossing the bridge.
    pub fn premultiply(&mut self) {
        self.touch();
        // In parallel: every pixel stands alone, and on a gigabyte composite
        // one thread is most of a second.
        use rayon::prelude::*;
        self.data.par_chunks_exact_mut(4).for_each(|px| {
            let a = px[3] as u32;
            if a == 255 {
                return;
            }
            if a == 0 {
                px[0] = 0;
                px[1] = 0;
                px[2] = 0;
                return;
            }
            // +127 rounds to nearest rather than truncating, which otherwise
            // darkens semi-transparent edges over repeated conversions.
            px[0] = ((px[0] as u32 * a + 127) / 255) as u8;
            px[1] = ((px[1] as u32 * a + 127) / 255) as u8;
            px[2] = ((px[2] as u32 * a + 127) / 255) as u8;
        });
    }

    /// Inverse of [`Pixmap::premultiply`].
    pub fn unpremultiply(&mut self) {
        self.touch();
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
mod content_bounds_tests {
    use super::*;

    #[test]
    fn an_opaque_pixmap_is_its_own_bounds() {
        let p = Pixmap::filled(300, 200, Rgba8::new(1, 2, 3, 255));
        assert_eq!(p.content_bounds(), Rect::new(0, 0, 300, 200));
    }

    #[test]
    fn an_empty_pixmap_has_no_bounds() {
        assert!(Pixmap::new(300, 200).content_bounds().is_empty());
        assert!(Pixmap::new(0, 0).content_bounds().is_empty());
    }

    #[test]
    fn scattered_pixels_are_boxed_tightly() {
        // The two points that set left and right are on rows away from the
        // top and bottom, so each edge has to be found separately.
        let mut p = Pixmap::new(400, 300);
        for (x, y) in [(120, 40), (30, 150), (370, 90), (200, 260)] {
            p.set(x, y, Rgba8::new(0, 0, 0, 1));
        }
        assert_eq!(p.content_bounds(), Rect::new(30, 40, 341, 221));
    }

    #[test]
    fn one_pixel_is_a_one_pixel_box() {
        let mut p = Pixmap::new(50, 50);
        p.set(49, 0, Rgba8::new(9, 9, 9, 200));
        assert_eq!(p.content_bounds(), Rect::new(49, 0, 1, 1));
    }

    #[test]
    fn deep_pixmaps_are_bounded_too() {
        let mut p = Pixmap::new_with_depth(64, 64, 2);
        p.set(10, 20, Rgba8::new(0, 0, 0, 255));
        p.set(40, 30, Rgba8::new(0, 0, 0, 255));
        assert_eq!(p.content_bounds(), Rect::new(10, 20, 31, 11));
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
    fn crop_and_blit_round_trip_a_region_exactly() {
        let mut pm = Pixmap::new(8, 8);
        for y in 0..8 {
            for x in 0..8 {
                pm.set(x, y, Rgba8::opaque((x * 30) as u8, (y * 30) as u8, 7));
            }
        }
        let before = pm.as_bytes().to_vec();

        let region = Rect::new(2, 3, 4, 3);
        let backup = pm.crop(region);
        pm.fill_rect(region, Rgba8::opaque(1, 2, 3));
        assert_ne!(pm.as_bytes(), &before[..], "the region was not disturbed");

        pm.blit(&backup, region.x, region.y);
        assert_eq!(pm.as_bytes(), &before[..], "the region did not come back");
    }

    #[test]
    fn a_blit_round_trip_keeps_more_than_eight_bits() {
        // The live stroke preview borrows a region of the layer and puts it
        // back on every mouse-move. Going through `Rgba8` on the way would
        // quietly flatten a 16-bit document to 8 as the user painted.
        let mut pm = Pixmap::new_with_depth(4, 2, 2);
        let raw: Vec<u8> = (0..pm.as_bytes().len()).map(|i| (i * 7 % 251) as u8).collect();
        pm.as_bytes_mut().copy_from_slice(&raw);

        let region = Rect::new(1, 0, 2, 2);
        let backup = pm.crop(region);
        pm.fill_rect(region, Rgba8::TRANSPARENT);
        pm.blit(&backup, region.x, region.y);

        assert_eq!(pm.as_bytes(), &raw[..], "16-bit pixels lost precision on the way back");
    }

    #[test]
    fn a_blit_clips_at_the_edges() {
        let mut pm = Pixmap::new(4, 4);
        let patch = Pixmap::filled(3, 3, Rgba8::opaque(9, 9, 9));
        pm.blit(&patch, 2, 2);
        assert_eq!(pm.get(3, 3), Rgba8::opaque(9, 9, 9));
        assert_eq!(pm.get(1, 1), Rgba8::TRANSPARENT);

        // Entirely outside, and straddling the origin: neither may panic.
        pm.blit(&patch, 40, 40);
        pm.blit(&patch, -2, -2);
        assert_eq!(pm.get(0, 0), Rgba8::opaque(9, 9, 9));
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
    fn a_stamp_is_shared_by_copies_and_lost_by_any_change() {
        // History skips comparing two pixmaps with the same stamp, so a
        // change that kept its stamp would never be undoable. Every way in.
        type Change = fn(&mut Pixmap);
        let changes: [(&str, Change); 10] = [
            ("as_bytes_mut", |p| p.as_bytes_mut()[0] = 9),
            ("row_mut", |p| p.row_mut(0)[0] = 9),
            ("rows_mut", |p| p.rows_mut().next().unwrap()[0] = 9),
            ("set", |p| p.set(0, 0, Rgba8::WHITE)),
            ("fill", |p| p.fill(Rgba8::WHITE)),
            ("fill_rect", |p| p.fill_rect(Rect::new(0, 0, 1, 1), Rgba8::WHITE)),
            ("clear", |p| p.clear()),
            ("blit", |p| p.blit(&Pixmap::filled(1, 1, Rgba8::WHITE), 0, 0)),
            ("premultiply", |p| p.premultiply()),
            ("unpremultiply", |p| p.unpremultiply()),
        ];
        for (name, change) in changes {
            let original = Pixmap::filled(4, 4, Rgba8::new(10, 20, 30, 128));
            let mut copy = original.clone();
            copy.share_stamp(&original);
            assert_ne!(original.stamp(), 0);
            assert_eq!(copy.stamp(), original.stamp());
            assert_eq!(copy.clone().stamp(), original.stamp(), "a clone keeps the stamp");
            change(&mut copy);
            assert_eq!(copy.stamp(), 0, "{name} kept the stamp");
        }
    }

    #[test]
    fn new_pixmaps_promise_nothing() {
        assert_eq!(Pixmap::new(2, 2).stamp(), 0);
        assert_eq!(Pixmap::from_raw(1, 1, vec![0; 4]).unwrap().stamp(), 0);
        // Two separately shared pairs never collide.
        let (a, b) = (Pixmap::new(1, 1), Pixmap::new(1, 1));
        let (mut ca, mut cb) = (a.clone(), b.clone());
        ca.share_stamp(&a);
        cb.share_stamp(&b);
        assert_ne!(a.stamp(), b.stamp());
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

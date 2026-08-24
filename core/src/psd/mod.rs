//! `.psd` reading and writing.
//!
//! # Status
//!
//! **Partial.** Implemented today:
//!
//! * Full header parsing and validation.
//! * The colour-mode and image-resource sections (skipped, but correctly
//!   length-delimited so the parser stays in sync).
//! * The layer-and-mask section: layer records, bounds, names, opacity, blend
//!   mode, clipping and flags.
//! * The composite image data for RAW and RLE (PackBits) compression, 8-bit,
//!   in Greyscale and RGB colour modes.
//!
//! Not yet implemented — these return [`PsdError::Unsupported`] rather than
//! silently producing wrong pixels:
//!
//! * ZIP-compressed channels, 16- and 32-bit depths, CMYK/Lab/Indexed/Duotone.
//! * Layer masks: their channels are read past, so a masked layer opens
//!   unmasked.
//! * Layer effects, smart objects, adjustment-layer parameters.
//! * Type layers are read as far as their text, font, size, colour and
//!   justification (see [`text`]); anything finer — per-character runs,
//!   warping, paragraph settings — is not.
//! * Writing. [`write_psd`] emits a valid single-layer file only.
//!
//! The format is documented in Adobe's "Photoshop File Format Specification";
//! `libpsd` is a useful cross-reference for the parts the spec glosses over.

use crate::blend::BlendMode;
use crate::buffer::{Pixmap, Rgba8};
use crate::layer::{Layer, LayerKind, LayerStack};

pub mod text;
pub mod text_write;

/// Everything that can go wrong reading a PSD.
#[derive(Debug, PartialEq, Eq)]
pub enum PsdError {
    /// Missing the `8BPS` magic.
    BadSignature,
    /// Version field was not 1 (PSD) — 2 means PSB, which we do not read.
    BadVersion(u16),
    /// Ran off the end of the buffer.
    UnexpectedEof {
        offset: usize,
        wanted: usize,
        available: usize,
    },
    /// Structurally valid but uses a feature that is not implemented.
    Unsupported(String),
    /// Dimensions outside what the format permits.
    InvalidDimensions { width: u32, height: u32 },
}

impl std::fmt::Display for PsdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PsdError::BadSignature => write!(f, "not a Photoshop file (bad signature)"),
            PsdError::BadVersion(v) => {
                write!(f, "unsupported PSD version {} (only version 1 is read)", v)
            }
            PsdError::UnexpectedEof {
                offset,
                wanted,
                available,
            } => write!(
                f,
                "unexpected end of file at offset {}: wanted {} bytes, {} available",
                offset, wanted, available
            ),
            PsdError::Unsupported(what) => write!(f, "unsupported PSD feature: {}", what),
            PsdError::InvalidDimensions { width, height } => {
                write!(f, "invalid image dimensions {}x{}", width, height)
            }
        }
    }
}

impl std::error::Error for PsdError {}

/// PSD colour modes, from the header.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u16)]
pub enum ColorMode {
    Bitmap = 0,
    Grayscale = 1,
    Indexed = 2,
    Rgb = 3,
    Cmyk = 4,
    Multichannel = 7,
    Duotone = 8,
    Lab = 9,
}

impl ColorMode {
    fn from_u16(v: u16) -> Option<ColorMode> {
        Some(match v {
            0 => ColorMode::Bitmap,
            1 => ColorMode::Grayscale,
            2 => ColorMode::Indexed,
            3 => ColorMode::Rgb,
            4 => ColorMode::Cmyk,
            7 => ColorMode::Multichannel,
            8 => ColorMode::Duotone,
            9 => ColorMode::Lab,
            _ => return None,
        })
    }
}

/// The 26-byte PSD file header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PsdHeader {
    pub channels: u16,
    pub width: u32,
    pub height: u32,
    pub depth: u16,
    pub color_mode: ColorMode,
}

/// A parsed PSD file.
pub struct PsdFile {
    pub header: PsdHeader,
    /// Layer metadata. Pixel data is not yet populated — see the module docs.
    pub layers: LayerStack,
    /// The flattened composite Photoshop stores at the end of the file.
    pub composite: Option<Pixmap>,
}

/// A little cursor over the file bytes that reports EOF rather than panicking.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], PsdError> {
        if self.remaining() < n {
            return Err(PsdError::UnexpectedEof {
                offset: self.pos,
                wanted: n,
                available: self.remaining(),
            });
        }
        let out = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    fn skip(&mut self, n: usize) -> Result<(), PsdError> {
        self.take(n).map(|_| ())
    }

    fn u8(&mut self) -> Result<u8, PsdError> {
        Ok(self.take(1)?[0])
    }

    // PSD is big-endian throughout.
    fn u16(&mut self) -> Result<u16, PsdError> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn i16(&mut self) -> Result<i16, PsdError> {
        Ok(self.u16()? as i16)
    }

    fn u32(&mut self) -> Result<u32, PsdError> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn i32(&mut self) -> Result<i32, PsdError> {
        Ok(self.u32()? as i32)
    }

    /// A Pascal string padded to a multiple of `pad` bytes.
    fn pascal_string(&mut self, pad: usize) -> Result<String, PsdError> {
        let len = self.u8()? as usize;
        let bytes = self.take(len)?;
        let s = String::from_utf8_lossy(bytes).into_owned();
        // The length byte counts toward the padding.
        let total = len + 1;
        let rem = total % pad;
        if rem != 0 {
            self.skip(pad - rem)?;
        }
        Ok(s)
    }
}

/// Parse the 26-byte header.
pub fn parse_header(data: &[u8]) -> Result<PsdHeader, PsdError> {
    let mut r = Reader::new(data);

    if r.take(4)? != b"8BPS" {
        return Err(PsdError::BadSignature);
    }
    let version = r.u16()?;
    if version != 1 {
        return Err(PsdError::BadVersion(version));
    }
    // Six reserved bytes, always zero.
    r.skip(6)?;

    let channels = r.u16()?;
    let height = r.u32()?;
    let width = r.u32()?;
    let depth = r.u16()?;
    let mode_raw = r.u16()?;

    // PSD caps documents at 30,000 px per side.
    if width == 0 || height == 0 || width > 30_000 || height > 30_000 {
        return Err(PsdError::InvalidDimensions { width, height });
    }

    let color_mode = ColorMode::from_u16(mode_raw)
        .ok_or_else(|| PsdError::Unsupported(format!("colour mode {}", mode_raw)))?;

    Ok(PsdHeader {
        channels,
        width,
        height,
        depth,
        color_mode,
    })
}

/// Parse a PSD file.
pub fn parse(data: &[u8]) -> Result<PsdFile, PsdError> {
    let header = parse_header(data)?;

    if header.depth != 8 {
        return Err(PsdError::Unsupported(format!(
            "{}-bit channels (only 8-bit is implemented)",
            header.depth
        )));
    }
    if !matches!(header.color_mode, ColorMode::Rgb | ColorMode::Grayscale) {
        return Err(PsdError::Unsupported(format!(
            "{:?} colour mode (only RGB and Grayscale are implemented)",
            header.color_mode
        )));
    }

    let mut r = Reader::new(data);
    r.skip(26)?;

    // Colour mode data — only non-empty for Indexed and Duotone.
    let color_data_len = r.u32()? as usize;
    r.skip(color_data_len)?;

    // Image resources — thumbnails, guides, ICC profile, etc.
    let resources_len = r.u32()? as usize;
    r.skip(resources_len)?;

    // Layer and mask information.
    let layer_section_len = r.u32()? as usize;
    let layer_section_end = r.pos + layer_section_len;
    let layers = if layer_section_len > 0 {
        parse_layer_section(&mut r, layer_section_end)?
    } else {
        LayerStack::new()
    };
    // Re-sync in case the layer parser stopped early.
    r.pos = layer_section_end.min(r.data.len());

    let composite = parse_composite(&mut r, &header).ok();

    Ok(PsdFile {
        header,
        layers,
        composite,
    })
}

/// Parse the layer records. Pixel data is skipped for now.
fn parse_layer_section(r: &mut Reader<'_>, section_end: usize) -> Result<LayerStack, PsdError> {
    let mut stack = LayerStack::new();

    let layer_info_len = r.u32()? as usize;
    if layer_info_len == 0 {
        return Ok(stack);
    }
    let layer_info_end = (r.pos + layer_info_len).min(section_end);

    // A negative count means the first alpha channel holds transparency data.
    let raw_count = r.i16()?;
    let count = raw_count.unsigned_abs() as usize;

    struct Record {
        /// What the layer says, for a type layer.
        text: Option<text::PsdText>,
        top: i32,
        left: i32,
        bottom: i32,
        right: i32,
        /// Each channel's id and the length of its data, in file order.
        channels: Vec<(i16, usize)>,
        blend_mode: BlendMode,
        opacity: u8,
        clipping: bool,
        hidden: bool,
        name: String,
    }
    let mut records = Vec::with_capacity(count);

    for _ in 0..count {
        let top = r.i32()?;
        let left = r.i32()?;
        let bottom = r.i32()?;
        let right = r.i32()?;

        let channel_count = r.u16()? as usize;
        // Each channel: a 2-byte id saying which one it is, and the length of
        // its data further down the file. Both are needed to read the pixels:
        // the ids say which plane is red and which is transparency, and the
        // lengths are the only way to walk from one channel to the next.
        let mut channels = Vec::with_capacity(channel_count);
        for _ in 0..channel_count {
            let id = r.i16()?;
            let length = r.u32()? as usize;
            channels.push((id, length));
        }

        // Blend mode signature, always '8BIM'.
        r.skip(4)?;
        let mode_key = r.take(4)?;
        let blend_mode = blend_mode_from_key(mode_key);

        let opacity = r.u8()?;
        let clipping = r.u8()? != 0;
        let flags = r.u8()?;
        // Bit 1 is "transparency protected"; bit 2 is the visibility flag,
        // which is set when the layer is *hidden*.
        let hidden = flags & 0x02 != 0;
        r.skip(1)?; // filler

        let extra_len = r.u32()? as usize;
        let extra_end = r.pos + extra_len;

        // Layer mask data.
        let mask_len = r.u32()? as usize;
        r.skip(mask_len)?;
        // Blending ranges.
        let ranges_len = r.u32()? as usize;
        r.skip(ranges_len)?;

        // The legacy Pascal name, padded to 4 bytes. A Unicode name may follow
        // in an additional-info block, which is not read yet.
        let name = r.pascal_string(4).unwrap_or_default();

        // The additional layer information blocks, which is where a type layer
        // keeps what it says.
        let text = find_type_tool(r, extra_end);
        r.pos = extra_end.min(r.data.len());

        records.push(Record {
            text,
            top,
            left,
            bottom,
            right,
            channels,
            blend_mode,
            opacity,
            clipping,
            hidden,
            name,
        });
    }

    // The channel data for every layer follows, in the same order as the
    // records, each channel headed by its own compression flag.
    for rec in records {
        let width = (rec.right - rec.left).max(0) as u32;
        let height = (rec.bottom - rec.top).max(0) as u32;

        let id = stack.allocate_id();
        let name = if rec.name.is_empty() {
            format!("Layer {}", stack.len() + 1)
        } else {
            rec.name.clone()
        };
        let mut layer = Layer::new_raster(id, name, width, height);
        layer.offset = (rec.left, rec.top);
        layer.blend_mode = rec.blend_mode;
        layer.opacity = rec.opacity as f32 / 255.0;
        layer.clipping = rec.clipping;
        layer.visible = !rec.hidden;

        read_layer_pixels(r, &rec.channels, &mut layer, width, height);
        if let Some(psd_text) = rec.text {
            layer.text = Some(to_text_content(&psd_text, &layer));
        }
        stack.push(layer);
    }

    // Whatever the channel walk made of the file, carry on from where the
    // section says it ends: one layer with an unreadable channel must not throw
    // the composite off too.
    r.pos = layer_info_end.min(r.data.len());

    Ok(stack)
}

/// Walk a layer's additional information blocks looking for its type data.
///
/// Each block is a signature, a four-character key and a length, so the ones
/// that are not wanted can be stepped over exactly. Leaves the reader where it
/// found it: the caller resumes from the section's stated end either way.
fn find_type_tool(r: &mut Reader<'_>, extra_end: usize) -> Option<text::PsdText> {
    let resume = r.pos;
    let mut found = None;

    while r.pos + 12 <= extra_end {
        let signature = r.take(4).ok()?;
        // '8BIM' and '8B64' are the two Photoshop writes; anything else means
        // the walk has lost its place and should stop rather than guess.
        if signature != b"8BIM" && signature != b"8B64" {
            break;
        }
        let Ok(key) = r.take(4) else { break };
        let key: [u8; 4] = match key.try_into() {
            Ok(key) => key,
            Err(_) => break,
        };
        let Ok(length) = r.u32() else { break };
        let length = length as usize;
        // Block lengths are padded to an even number of bytes.
        let padded = length + (length & 1);
        let Ok(body) = r.take(length) else { break };

        if &key == b"TySh" {
            found = text::parse_type_tool(body);
            // The first one is the layer's; there is no second.
            r.pos = resume;
            return found;
        }
        r.pos = (r.pos + (padded - length)).min(r.data.len());
    }

    r.pos = resume;
    found
}

/// Turn what the PSD says into the type record our own layers carry.
///
/// The origin is worked out from the layer's pixel bounds rather than from the
/// block's transform. Photoshop measures from the first line's *baseline*, and
/// where that sits depends on the font's ascent — which the engine cannot know,
/// since fonts live in the shell (CLAUDE.md §2). Anchoring to the rasterized
/// bounds instead is right to within a pixel or two, and it is only used if the
/// user reopens the text: until then the layer draws Photoshop's own pixels.
fn to_text_content(psd: &text::PsdText, layer: &Layer) -> crate::layer::TextContent {
    use crate::layer::{TextAlign, TextContent, TextRun};

    let bounds = layer.bounds();
    let origin_x = match psd.align {
        TextAlign::Center => bounds.x as f32 + bounds.width as f32 / 2.0,
        TextAlign::Right => bounds.right() as f32,
        TextAlign::Left => bounds.x as f32,
    };

    TextContent {
        // One run: see `text::apply_engine_data` on why the formatting of the
        // first character is taken to be the formatting of the whole layer.
        runs: vec![TextRun {
            // Photoshop separates lines with a carriage return; ours uses a
            // newline, and a stray CR would otherwise be drawn as a glyph.
            text: psd.text.replace('\r', "\n"),
            family: psd.family.clone(),
            style: psd.style.clone(),
            size: psd.size,
            color: psd.color,
        }],
        align: psd.align,
        antialias: true,
        vertical: false,
        origin: (origin_x, bounds.y as f32),
    }
}

/// Read one layer's channels into its pixels.
///
/// A layer stores each channel separately — red, green, blue and transparency
/// as four planes — and they have to be interleaved to become pixels. The
/// transparency channel is the one that matters most here: without it every
/// layer would be a solid rectangle of its bounding box, which is exactly how a
/// PSD looks when its channels are skipped.
///
/// Anything unreadable leaves the layer transparent rather than failing the
/// open. A file with one odd layer should still come up.
fn read_layer_pixels(
    r: &mut Reader<'_>,
    channels: &[(i16, usize)],
    layer: &mut Layer,
    width: u32,
    height: u32,
) {
    let per_channel = width as usize * height as usize;
    let mut red = Vec::new();
    let mut green = Vec::new();
    let mut blue = Vec::new();
    let mut alpha = Vec::new();

    for (id, length) in channels {
        let end = (r.pos + length).min(r.data.len());
        // Channel ids: 0/1/2 are the colour planes, -1 is transparency, and
        // -2/-3 are the layer's masks — which have bounds of their own and are
        // not applied yet, so they are stepped over.
        let wanted = matches!(id, 0 | 1 | 2 | -1) && per_channel > 0;
        if wanted {
            if let Ok(plane) = read_channel(r, width, height) {
                match id {
                    0 => red = plane,
                    1 => green = plane,
                    2 => blue = plane,
                    _ => alpha = plane,
                }
            }
        }
        // Always resume at the channel's stated end, whatever the decode did:
        // the next channel's position depends on this length, not on how far
        // the decoder happened to read.
        r.pos = end;
    }

    if per_channel == 0 {
        return;
    }

    let bytes = layer.pixels.as_bytes_mut();
    for i in 0..per_channel {
        let at = |plane: &Vec<u8>, fallback: u8| plane.get(i).copied().unwrap_or(fallback);
        let o = i * 4;
        // A greyscale layer has only one colour plane, so green and blue fall
        // back to red rather than to black.
        let r8 = at(&red, 0);
        bytes[o] = r8;
        bytes[o + 1] = if green.is_empty() { r8 } else { at(&green, 0) };
        bytes[o + 2] = if blue.is_empty() { r8 } else { at(&blue, 0) };
        // No transparency channel means the layer is fully opaque within its
        // bounds, which is how Photoshop writes a background layer.
        bytes[o + 3] = if alpha.is_empty() { 255 } else { at(&alpha, 255) };
    }
}

/// Decode one channel's plane, starting at its compression flag.
fn read_channel(r: &mut Reader<'_>, width: u32, height: u32) -> Result<Vec<u8>, PsdError> {
    let compression = r.u16()?;
    let (w, h) = (width as usize, height as usize);
    let per_channel = w * h;

    match compression {
        0 => Ok(r.take(per_channel)?.to_vec()),
        1 => {
            // Unlike the composite, a layer channel's row lengths sit directly
            // in front of its own rows rather than being pooled for the whole
            // image.
            let mut row_lengths = Vec::with_capacity(h);
            for _ in 0..h {
                row_lengths.push(r.u16()? as usize);
            }
            let mut plane = Vec::with_capacity(per_channel);
            for length in row_lengths {
                let packed = r.take(length)?;
                unpack_bits(packed, w, &mut plane);
            }
            plane.resize(per_channel, 0);
            Ok(plane)
        }
        other => Err(PsdError::Unsupported(format!(
            "compression method {other} in layer data"
        ))),
    }
}

/// Decode the flattened composite at the end of the file.
fn parse_composite(r: &mut Reader<'_>, header: &PsdHeader) -> Result<Pixmap, PsdError> {
    let compression = r.u16()?;
    let width = header.width as usize;
    let height = header.height as usize;
    let channel_count = header.channels as usize;
    let per_channel = width * height;

    let mut planes: Vec<Vec<u8>> = Vec::with_capacity(channel_count);

    match compression {
        // Raw, uncompressed.
        0 => {
            for _ in 0..channel_count {
                planes.push(r.take(per_channel)?.to_vec());
            }
        }
        // RLE (PackBits). All row lengths come first, for every channel.
        1 => {
            let total_rows = height * channel_count;
            let mut row_lengths = Vec::with_capacity(total_rows);
            for _ in 0..total_rows {
                row_lengths.push(r.u16()? as usize);
            }
            for c in 0..channel_count {
                let mut plane = Vec::with_capacity(per_channel);
                for row in 0..height {
                    let len = row_lengths[c * height + row];
                    let packed = r.take(len)?;
                    unpack_bits(packed, width, &mut plane);
                }
                plane.resize(per_channel, 0);
                planes.push(plane);
            }
        }
        2 | 3 => {
            return Err(PsdError::Unsupported(
                "ZIP-compressed image data".to_string(),
            ))
        }
        other => {
            return Err(PsdError::Unsupported(format!(
                "compression method {}",
                other
            )))
        }
    }

    // Interleave the planar channels into RGBA.
    let mut pm = Pixmap::new(header.width, header.height);
    let bytes = pm.as_bytes_mut();
    let grayscale = header.color_mode == ColorMode::Grayscale;

    for i in 0..per_channel {
        let (r8, g8, b8) = if grayscale {
            let v = planes.first().map_or(0, |p| p[i]);
            (v, v, v)
        } else {
            (
                planes.first().map_or(0, |p| p[i]),
                planes.get(1).map_or(0, |p| p[i]),
                planes.get(2).map_or(0, |p| p[i]),
            )
        };
        // A 4th channel on RGB (or 2nd on greyscale) is alpha.
        let alpha_plane = if grayscale { 1 } else { 3 };
        let a8 = planes.get(alpha_plane).map_or(255, |p| p[i]);

        let o = i * 4;
        bytes[o] = r8;
        bytes[o + 1] = g8;
        bytes[o + 2] = b8;
        bytes[o + 3] = a8;
    }

    Ok(pm)
}

/// PackBits decompression, appending exactly `expected` bytes.
fn unpack_bits(src: &[u8], expected: usize, out: &mut Vec<u8>) {
    let start = out.len();
    let mut i = 0;
    while i < src.len() && out.len() - start < expected {
        let n = src[i] as i8;
        i += 1;
        if n >= 0 {
            // Literal run of n+1 bytes.
            let count = n as usize + 1;
            let end = (i + count).min(src.len());
            out.extend_from_slice(&src[i..end]);
            i = end;
        } else if n != -128 {
            // Repeat the next byte 1-n times. -128 is a no-op by spec.
            if i >= src.len() {
                break;
            }
            let count = (1 - n as i32) as usize;
            let byte = src[i];
            i += 1;
            out.extend(std::iter::repeat(byte).take(count));
        }
    }
    // Pad a short row rather than desynchronising every row after it.
    out.resize(start + expected, 0);
}

/// Map a PSD 4-character blend key to a [`BlendMode`].
fn blend_mode_from_key(key: &[u8]) -> BlendMode {
    match key {
        b"norm" => BlendMode::Normal,
        b"diss" => BlendMode::Dissolve,
        b"dark" => BlendMode::Darken,
        b"mul " => BlendMode::Multiply,
        b"idiv" => BlendMode::ColorBurn,
        b"lbrn" => BlendMode::LinearBurn,
        b"dkCl" => BlendMode::DarkerColor,
        b"lite" => BlendMode::Lighten,
        b"scrn" => BlendMode::Screen,
        b"div " => BlendMode::ColorDodge,
        b"lddg" => BlendMode::LinearDodge,
        b"lgCl" => BlendMode::LighterColor,
        b"over" => BlendMode::Overlay,
        b"sLit" => BlendMode::SoftLight,
        b"hLit" => BlendMode::HardLight,
        b"vLit" => BlendMode::VividLight,
        b"lLit" => BlendMode::LinearLight,
        b"pLit" => BlendMode::PinLight,
        b"hMix" => BlendMode::HardMix,
        b"diff" => BlendMode::Difference,
        b"smud" => BlendMode::Exclusion,
        b"fsub" => BlendMode::Subtract,
        b"fdiv" => BlendMode::Divide,
        b"hue " => BlendMode::Hue,
        b"sat " => BlendMode::Saturation,
        b"colr" => BlendMode::Color,
        b"lum " => BlendMode::Luminosity,
        _ => BlendMode::Normal,
    }
}

/// The inverse of [`blend_mode_from_key`].
///
/// Unused until layer *writing* lands; kept here so the two tables stay
/// side by side and the round-trip test can hold them to each other.
#[allow(dead_code)]
fn blend_mode_key(mode: BlendMode) -> &'static [u8; 4] {
    match mode {
        BlendMode::Normal => b"norm",
        BlendMode::Dissolve => b"diss",
        BlendMode::Darken => b"dark",
        BlendMode::Multiply => b"mul ",
        BlendMode::ColorBurn => b"idiv",
        BlendMode::LinearBurn => b"lbrn",
        BlendMode::DarkerColor => b"dkCl",
        BlendMode::Lighten => b"lite",
        BlendMode::Screen => b"scrn",
        BlendMode::ColorDodge => b"div ",
        BlendMode::LinearDodge => b"lddg",
        BlendMode::LighterColor => b"lgCl",
        BlendMode::Overlay => b"over",
        BlendMode::SoftLight => b"sLit",
        BlendMode::HardLight => b"hLit",
        BlendMode::VividLight => b"vLit",
        BlendMode::LinearLight => b"lLit",
        BlendMode::PinLight => b"pLit",
        BlendMode::HardMix => b"hMix",
        BlendMode::Difference => b"diff",
        BlendMode::Exclusion => b"smud",
        BlendMode::Subtract => b"fsub",
        BlendMode::Divide => b"fdiv",
        BlendMode::Hue => b"hue ",
        BlendMode::Saturation => b"sat ",
        BlendMode::Color => b"colr",
        BlendMode::Luminosity => b"lum ",
    }
}

/// Write a minimal, valid PSD holding `image` as a single flattened layer.
///
/// This produces a file Photoshop and other readers will open, but it does not
/// preserve the layer stack — see the module docs.
/// Write a document's whole layer stack.
///
/// The counterpart to [`parse`], and the thing that makes a file saved here
/// still a *project* when it is opened again — in Photoshop or in this program.
/// Writing only the flattened picture, as this used to, loses every layer the
/// moment the file is saved.
///
/// `composite` is the flattened image, which the file carries as well: it is
/// what other programs show, and what Photoshop falls back to.
///
/// Three things are folded away rather than written as their own structures,
/// because each needs a PSD feature this writer does not have yet. All three
/// preserve *appearance* — the file looks right — and cost editability:
///
/// - a layer **mask** is multiplied into the layer's own alpha;
/// - a **fill layer** (a shape) is rasterized to pixels.
///
/// Type layers are written with a `TySh` block so Photoshop reopens them as
/// editable text rather than as a picture of text.
pub fn write_layered_psd(stack: &LayerStack, composite: &Pixmap) -> Vec<u8> {
    let width = composite.width();
    let height = composite.height();
    let mut out = Vec::new();

    write_header(&mut out, width, height);
    out.extend_from_slice(&0u32.to_be_bytes()); // colour mode data: empty
    out.extend_from_slice(&0u32.to_be_bytes()); // image resources: empty

    // -- layer and mask information ------------------------------------------
    let mut layer_info = Vec::new();
    // Photoshop reads a negative count as "the first alpha channel is
    // transparency for the merged result"; a plain positive count is what an
    // ordinary layered file has.
    layer_info.extend_from_slice(&(stack.len() as i16).to_be_bytes());

    // The records come first and the pixels after, so both are built here and
    // joined below.
    let mut channel_data = Vec::new();
    for layer in stack.iter() {
        let pixels = flattened_layer(layer);
        write_layer_record(&mut layer_info, layer, &pixels);
        write_layer_channels(&mut channel_data, &pixels);
    }
    layer_info.extend_from_slice(&channel_data);
    pad_to_even(&mut layer_info);

    let mut section = Vec::new();
    section.extend_from_slice(&(layer_info.len() as u32).to_be_bytes());
    section.extend_from_slice(&layer_info);
    section.extend_from_slice(&0u32.to_be_bytes()); // no global layer mask

    out.extend_from_slice(&(section.len() as u32).to_be_bytes());
    out.extend_from_slice(&section);

    write_composite(&mut out, composite);
    out
}

/// One layer's pixels as they are to be written: its own, with any mask
/// multiplied in, or a fill layer rasterized.
fn flattened_layer(layer: &Layer) -> Pixmap {
    let mut pixels = match layer.kind {
        LayerKind::SolidColor(color) => {
            // A fill layer has no pixels of its own — it is a colour the
            // compositor pours through the mask — so it is rasterized at the
            // size of whatever shapes it.
            let (w, h) = match layer.mask.as_ref() {
                Some(mask) => (mask.width(), mask.height()),
                None => (layer.pixels.width(), layer.pixels.height()),
            };
            Pixmap::filled(w, h, color)
        }
        _ => layer.pixels.clone(),
    };

    if let Some(mask) = layer.mask.as_ref() {
        if layer.mask_enabled {
            for y in 0..pixels.height() as i32 {
                for x in 0..pixels.width() as i32 {
                    let coverage = mask.get(x, y).a as u32;
                    let mut px = pixels.get(x, y);
                    px.a = ((px.a as u32 * coverage) / 255) as u8;
                    pixels.set(x, y, px);
                }
            }
        }
    }
    pixels
}

/// The fixed part of a layer: where it is, how it blends, and what it is called.
fn write_layer_record(out: &mut Vec<u8>, layer: &Layer, pixels: &Pixmap) {
    let (left, top) = layer.offset;
    let right = left + pixels.width() as i32;
    let bottom = top + pixels.height() as i32;

    out.extend_from_slice(&top.to_be_bytes());
    out.extend_from_slice(&left.to_be_bytes());
    out.extend_from_slice(&bottom.to_be_bytes());
    out.extend_from_slice(&right.to_be_bytes());

    // Four channels: transparency first, then red, green and blue, each headed
    // by its own compression flag — which is the two bytes added below.
    let per_channel = pixels.width() as usize * pixels.height() as usize;
    out.extend_from_slice(&4u16.to_be_bytes());
    for id in [-1i16, 0, 1, 2] {
        out.extend_from_slice(&id.to_be_bytes());
        out.extend_from_slice(&((per_channel + 2) as u32).to_be_bytes());
    }

    out.extend_from_slice(b"8BIM");
    out.extend_from_slice(blend_mode_key(layer.blend_mode));
    out.push((layer.opacity.clamp(0.0, 1.0) * 255.0 + 0.5) as u8);
    out.push(u8::from(layer.clipping));
    // Bit 1 marks a hidden layer; bit 0 is the transparency-protected flag.
    let mut flags = 0x08; // "obsolete" bit Photoshop always sets
    if !layer.visible {
        flags |= 0x02;
    }
    out.push(flags);
    out.push(0); // filler

    // Extra data: no mask (it was folded into the alpha), no blending ranges,
    // the name as a Pascal string, and — for type layers — a TySh block.
    let mut extra = Vec::new();
    extra.extend_from_slice(&0u32.to_be_bytes()); // layer mask data: none
    extra.extend_from_slice(&0u32.to_be_bytes()); // blending ranges: none
    write_pascal_string(&mut extra, &layer.name);

    if let Some(text) = layer.text.as_ref() {
        let (left, top) = layer.offset;
        let bounds = (
            0.0f32,
            0.0,
            pixels.width() as f32,
            pixels.height() as f32,
        );
        let tysh = text_write::type_tool_block(text, bounds);

        // Additional layer information: `8BIM` + key + length + data, padded
        // to an even length. The length field is the *actual* data length;
        // padding is implicit and not counted.
        extra.extend_from_slice(b"8BIM");
        extra.extend_from_slice(b"TySh");
        extra.extend_from_slice(&(tysh.len() as u32).to_be_bytes());
        extra.extend_from_slice(&tysh);
        if tysh.len() % 2 == 1 {
            extra.push(0);
        }
        let _ = (left, top);
    }

    out.extend_from_slice(&(extra.len() as u32).to_be_bytes());
    out.extend_from_slice(&extra);
}

/// A layer's channel data, in the order its record listed them.
fn write_layer_channels(out: &mut Vec<u8>, pixels: &Pixmap) {
    let src = pixels.as_bytes();
    let count = pixels.width() as usize * pixels.height() as usize;

    // Transparency first, then the colour planes: the same order as the ids.
    for channel in [3usize, 0, 1, 2] {
        out.extend_from_slice(&0u16.to_be_bytes()); // raw, not RLE
        for i in 0..count {
            out.push(src[i * 4 + channel]);
        }
    }
}

/// A Pascal string: one length byte, then the bytes, padded so the whole thing
/// is a multiple of four.
fn write_pascal_string(out: &mut Vec<u8>, name: &str) {
    let bytes: Vec<u8> = name.bytes().take(255).collect();
    out.push(bytes.len() as u8);
    out.extend_from_slice(&bytes);
    while (out.len() % 4) != 0 {
        out.push(0);
    }
}

fn pad_to_even(out: &mut Vec<u8>) {
    if out.len() % 2 == 1 {
        out.push(0);
    }
}

fn write_header(out: &mut Vec<u8>, width: u32, height: u32) {
    out.extend_from_slice(b"8BPS");
    out.extend_from_slice(&1u16.to_be_bytes()); // version
    out.extend_from_slice(&[0u8; 6]); // reserved
    out.extend_from_slice(&4u16.to_be_bytes()); // channels: RGBA
    out.extend_from_slice(&height.to_be_bytes());
    out.extend_from_slice(&width.to_be_bytes());
    out.extend_from_slice(&8u16.to_be_bytes()); // depth
    out.extend_from_slice(&(ColorMode::Rgb as u16).to_be_bytes());
}

/// The flattened image at the end of the file, raw and planar.
fn write_composite(out: &mut Vec<u8>, image: &Pixmap) {
    out.extend_from_slice(&0u16.to_be_bytes()); // compression: raw
    let src = image.as_bytes();
    for channel in 0..4 {
        for i in 0..(image.width() as usize * image.height() as usize) {
            out.push(src[i * 4 + channel]);
        }
    }
}

pub fn write_psd(image: &Pixmap) -> Vec<u8> {
    let width = image.width();
    let height = image.height();
    let mut out = Vec::new();

    // -- header --
    out.extend_from_slice(b"8BPS");
    out.extend_from_slice(&1u16.to_be_bytes()); // version
    out.extend_from_slice(&[0u8; 6]); // reserved
    out.extend_from_slice(&4u16.to_be_bytes()); // channels: RGBA
    out.extend_from_slice(&height.to_be_bytes());
    out.extend_from_slice(&width.to_be_bytes());
    out.extend_from_slice(&8u16.to_be_bytes()); // depth
    out.extend_from_slice(&(ColorMode::Rgb as u16).to_be_bytes());

    // -- colour mode data: empty for RGB --
    out.extend_from_slice(&0u32.to_be_bytes());
    // -- image resources: empty --
    out.extend_from_slice(&0u32.to_be_bytes());
    // -- layer and mask info: empty, i.e. a flattened file --
    out.extend_from_slice(&0u32.to_be_bytes());

    // -- image data, raw planar --
    out.extend_from_slice(&0u16.to_be_bytes()); // compression: raw
    let src = image.as_bytes();
    for channel in 0..4 {
        for i in 0..(width as usize * height as usize) {
            out.push(src[i * 4 + channel]);
        }
    }
    out
}

/// Build a [`Pixmap`] from a parsed file, preferring the composite.
pub fn to_pixmap(file: &PsdFile) -> Pixmap {
    file.composite.clone().unwrap_or_else(|| {
        Pixmap::filled(file.header.width, file.header.height, Rgba8::TRANSPARENT)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid PSD header for tests.
    fn header_bytes(width: u32, height: u32, channels: u16, depth: u16, mode: u16) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"8BPS");
        v.extend_from_slice(&1u16.to_be_bytes());
        v.extend_from_slice(&[0u8; 6]);
        v.extend_from_slice(&channels.to_be_bytes());
        v.extend_from_slice(&height.to_be_bytes());
        v.extend_from_slice(&width.to_be_bytes());
        v.extend_from_slice(&depth.to_be_bytes());
        v.extend_from_slice(&mode.to_be_bytes());
        v
    }

    /// A whole PSD with one 2x2 layer of a known colour, written the way
    /// Photoshop writes an uncompressed one.
    ///
    /// Built here rather than checked in as a fixture so the bytes that matter
    /// — the channel ids, their lengths, and the order they are read in — are
    /// visible in the test that depends on them.
    fn psd_with_one_layer(alpha: u8) -> Vec<u8> {
        let mut v = header_bytes(2, 2, 3, 8, 3);
        v.extend_from_slice(&0u32.to_be_bytes()); // no colour mode data
        v.extend_from_slice(&0u32.to_be_bytes()); // no image resources

        // -- the layer section, whose length is filled in once it is built ---
        let mut layers = Vec::new();
        let mut info = Vec::new();
        info.extend_from_slice(&1i16.to_be_bytes()); // one layer

        // Bounds: the whole 2x2 canvas.
        info.extend_from_slice(&0i32.to_be_bytes()); // top
        info.extend_from_slice(&0i32.to_be_bytes()); // left
        info.extend_from_slice(&2i32.to_be_bytes()); // bottom
        info.extend_from_slice(&2i32.to_be_bytes()); // right

        // Four channels: red, green, blue and transparency. Each is a
        // compression flag plus four raw bytes.
        let channel_bytes = 2 + 4;
        info.extend_from_slice(&4u16.to_be_bytes());
        for id in [0i16, 1, 2, -1] {
            info.extend_from_slice(&id.to_be_bytes());
            info.extend_from_slice(&(channel_bytes as u32).to_be_bytes());
        }

        info.extend_from_slice(b"8BIM");
        info.extend_from_slice(b"norm");
        info.push(255); // opacity
        info.push(0); // not clipping
        info.push(0); // flags: visible
        info.push(0); // filler

        // Extra data: no mask, no blending ranges, and a Pascal name padded to
        // four bytes.
        let mut extra = Vec::new();
        extra.extend_from_slice(&0u32.to_be_bytes());
        extra.extend_from_slice(&0u32.to_be_bytes());
        extra.push(5);
        extra.extend_from_slice(b"Paint");
        extra.extend_from_slice(&[0, 0]); // pad 6 bytes to 8
        info.extend_from_slice(&(extra.len() as u32).to_be_bytes());
        info.extend_from_slice(&extra);

        // The channel data itself, in the order the ids were listed.
        for value in [200u8, 100, 50, alpha] {
            info.extend_from_slice(&0u16.to_be_bytes()); // raw
            info.extend_from_slice(&[value; 4]);
        }

        layers.extend_from_slice(&(info.len() as u32).to_be_bytes());
        layers.extend_from_slice(&info);
        layers.extend_from_slice(&0u32.to_be_bytes()); // no global mask

        v.extend_from_slice(&(layers.len() as u32).to_be_bytes());
        v.extend_from_slice(&layers);

        // A flattened composite, so the file is complete: raw, three channels.
        v.extend_from_slice(&0u16.to_be_bytes());
        v.extend_from_slice(&[9u8; 12]);
        v
    }

    /// The same file, with the layer carrying the `TySh` block that makes it a
    /// type layer.
    fn psd_with_type_layer() -> Vec<u8> {
        let plain = psd_with_one_layer(255);

        // Rebuild it with the extra block appended to the layer's extra data.
        // Easier and clearer than patching lengths in place: the extra section
        // is the only part that changes.
        let engine = "<< /ResourceDict << /FontSet [ << /Name (Impact) >> ] >> \
                      /EngineDict << /StyleRun << /RunArray [ << /StyleSheet << \
                      /StyleSheetData << /FontSize 48.0 >> >> >> ] >> >> >>";
        let type_block = {
            let mut block = Vec::new();
            block.extend_from_slice(&1u16.to_be_bytes());
            for value in [1.0f64, 0.0, 0.0, 1.0, 0.0, 0.0] {
                block.extend_from_slice(&value.to_be_bytes());
            }
            block.extend_from_slice(&50u16.to_be_bytes());
            block.extend_from_slice(&16u32.to_be_bytes());

            // Descriptor: name, class, one item — the text itself.
            block.extend_from_slice(&1u32.to_be_bytes());
            block.extend_from_slice(&0u16.to_be_bytes());
            block.extend_from_slice(&0u32.to_be_bytes());
            block.extend_from_slice(b"null");
            block.extend_from_slice(&2u32.to_be_bytes());

            block.extend_from_slice(&0u32.to_be_bytes());
            block.extend_from_slice(b"Txt ");
            block.extend_from_slice(b"TEXT");
            let units: Vec<u16> = "TESTTEST".encode_utf16().chain(std::iter::once(0)).collect();
            block.extend_from_slice(&(units.len() as u32).to_be_bytes());
            for unit in units {
                block.extend_from_slice(&unit.to_be_bytes());
            }

            block.extend_from_slice(&(10u32).to_be_bytes());
            block.extend_from_slice(b"EngineData");
            block.extend_from_slice(b"tdta");
            block.extend_from_slice(&(engine.len() as u32).to_be_bytes());
            block.extend_from_slice(engine.as_bytes());
            block
        };

        let mut extra_addition = Vec::new();
        extra_addition.extend_from_slice(b"8BIM");
        extra_addition.extend_from_slice(b"TySh");
        extra_addition.extend_from_slice(&(type_block.len() as u32).to_be_bytes());
        extra_addition.extend_from_slice(&type_block);
        if type_block.len() % 2 == 1 {
            extra_addition.push(0);
        }

        splice_extra_block(&plain, &extra_addition)
    }

    /// Insert `addition` into the single layer's extra data, fixing up the
    /// three lengths that describe it.
    fn splice_extra_block(psd: &[u8], addition: &[u8]) -> Vec<u8> {
        let mut v = psd.to_vec();

        // Layout of the file built by `psd_with_one_layer`: header (26), then
        // two empty sections (4 each), then the layer section's length.
        let section_len_at = 26 + 4 + 4;
        let info_len_at = section_len_at + 4;
        // The layer record's extra length sits after the record's fixed part:
        // bounds (16), channel count (2), four channels (6 each), signature and
        // mode (8), opacity/clipping/flags/filler (4).
        let extra_len_at = info_len_at + 4 + 2 + 16 + 2 + 24 + 8 + 4;
        let extra_at = extra_len_at + 4;

        let read = |v: &Vec<u8>, at: usize| {
            u32::from_be_bytes([v[at], v[at + 1], v[at + 2], v[at + 3]]) as usize
        };
        let extra_len = read(&v, extra_len_at);
        let insert_at = extra_at + extra_len;

        v.splice(insert_at..insert_at, addition.iter().copied());

        let bump = |v: &mut Vec<u8>, at: usize, by: usize| {
            let value = read(v, at) + by;
            v[at..at + 4].copy_from_slice(&(value as u32).to_be_bytes());
        };
        bump(&mut v, extra_len_at, addition.len());
        bump(&mut v, info_len_at, addition.len());
        bump(&mut v, section_len_at, addition.len());
        v
    }

    /// A stack of three layers, each recognisable: a background, a small red
    /// square offset into the canvas, and a half-opacity multiply layer.
    fn stack_to_write() -> LayerStack {
        let mut stack = LayerStack::new();

        let id = stack.allocate_id();
        stack.push(Layer::new_filled(id, "Background", 4, 4, Rgba8::WHITE));

        let id = stack.allocate_id();
        let mut square = Layer::new_raster(id, "Red Square", 2, 2);
        square.pixels.fill(Rgba8::opaque(220, 30, 30));
        square.offset = (1, 1);
        stack.push(square);

        let id = stack.allocate_id();
        let mut shade = Layer::new_raster(id, "Shade", 4, 4);
        shade.pixels.fill(Rgba8::new(0, 0, 255, 128));
        shade.blend_mode = BlendMode::Multiply;
        shade.opacity = 0.5;
        shade.visible = false;
        stack.push(shade);

        stack
    }

    #[test]
    fn a_written_file_keeps_its_layers() {
        // The regression behind "saving a PSD flattens it": the writer emitted
        // only the composite, so every layer was lost on save.
        let stack = stack_to_write();
        let composite = Pixmap::filled(4, 4, Rgba8::WHITE);
        let bytes = write_layered_psd(&stack, &composite);

        let file = parse(&bytes).expect("what was written did not parse");
        assert_eq!(file.layers.len(), 3);
        let names: Vec<&str> = file.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, vec!["Background", "Red Square", "Shade"]);
    }

    #[test]
    fn a_written_layers_pixels_and_place_survive_the_round_trip() {
        let stack = stack_to_write();
        let bytes = write_layered_psd(&stack, &Pixmap::filled(4, 4, Rgba8::WHITE));
        let file = parse(&bytes).unwrap();

        let square = file.layers.get(1).unwrap();
        assert_eq!(square.offset, (1, 1), "the layer moved");
        assert_eq!(square.pixels.width(), 2);
        assert_eq!(square.pixels.get(0, 0), Rgba8::opaque(220, 30, 30));
        // Transparency comes back too, which is what stops a layer being a
        // solid block of its bounding box.
        assert_eq!(file.layers.get(2).unwrap().pixels.get(0, 0).a, 128);
    }

    #[test]
    fn a_written_layers_settings_survive_the_round_trip() {
        let stack = stack_to_write();
        let bytes = write_layered_psd(&stack, &Pixmap::filled(4, 4, Rgba8::WHITE));
        let file = parse(&bytes).unwrap();

        let shade = file.layers.get(2).unwrap();
        assert_eq!(shade.blend_mode, BlendMode::Multiply);
        assert!((shade.opacity - 0.5).abs() < 0.01);
        assert!(!shade.visible, "a hidden layer came back visible");
    }

    #[test]
    fn a_mask_is_written_into_the_layers_own_transparency() {
        // Masks are not written as their own channel yet, so they are folded
        // into the alpha: the file looks right, and the mask is no longer
        // separately editable.
        let mut stack = LayerStack::new();
        let id = stack.allocate_id();
        let mut layer = Layer::new_raster(id, "Masked", 2, 2);
        layer.pixels.fill(Rgba8::opaque(10, 20, 30));
        layer.add_reveal_all_mask();
        if let Some(mask) = layer.mask.as_mut() {
            mask.set(0, 0, Rgba8::new(0, 0, 0, 0));
        }
        stack.push(layer);

        let bytes = write_layered_psd(&stack, &Pixmap::filled(2, 2, Rgba8::WHITE));
        let file = parse(&bytes).unwrap();
        let written = file.layers.get(0).unwrap();
        assert_eq!(written.pixels.get(0, 0).a, 0, "the mask was dropped");
        assert_eq!(written.pixels.get(1, 1).a, 255);
    }

    #[test]
    fn a_fill_layer_is_written_as_the_colour_it_shows() {
        // A shape layer has no pixels of its own; writing it as-is would put an
        // empty layer in the file.
        let mut stack = LayerStack::new();
        let id = stack.allocate_id();
        let mut shape = Layer::new_raster(id, "Rectangle 1", 0, 0);
        shape.kind = LayerKind::SolidColor(Rgba8::opaque(10, 200, 10));
        let mut mask = Pixmap::new(2, 2);
        mask.fill(Rgba8::new(255, 255, 255, 255));
        mask.set(0, 0, Rgba8::new(0, 0, 0, 0));
        shape.mask = Some(mask);
        stack.push(shape);

        let bytes = write_layered_psd(&stack, &Pixmap::filled(2, 2, Rgba8::WHITE));
        let file = parse(&bytes).unwrap();
        let written = file.layers.get(0).unwrap();
        assert_eq!(written.pixels.get(1, 1), Rgba8::opaque(10, 200, 10));
        assert_eq!(written.pixels.get(0, 0).a, 0, "the shape's mask was ignored");
    }

    #[test]
    fn the_composite_is_written_alongside_the_layers() {
        // Other programs show this, and Photoshop falls back to it.
        let stack = stack_to_write();
        let composite = Pixmap::filled(4, 4, Rgba8::opaque(1, 2, 3));
        let file = parse(&write_layered_psd(&stack, &composite)).unwrap();
        assert_eq!(file.composite.unwrap().get(2, 2), Rgba8::opaque(1, 2, 3));
    }

    #[test]
    fn a_type_layer_arrives_as_text_and_not_only_pixels() {
        // The regression behind "the TESTTEST layer cannot be edited": the
        // layer's `TySh` block was skipped, so it opened as a picture of text.
        let file = parse(&psd_with_type_layer()).expect("did not parse");
        let layer = file.layers.get(0).expect("no layer");

        let content = layer.text.as_ref().expect("the layer is not type");
        assert_eq!(content.text(), "TESTTEST");
        assert_eq!(content.first_run().unwrap().family, "Impact");
        assert_eq!(content.first_run().unwrap().size, 48.0);

        // And it keeps Photoshop's own rendering until it is edited.
        assert_eq!(layer.pixels.get(0, 0), Rgba8::new(200, 100, 50, 255));
    }

    #[test]
    fn an_ordinary_layer_is_not_mistaken_for_type() {
        let file = parse(&psd_with_one_layer(255)).expect("did not parse");
        assert!(file.layers.get(0).unwrap().text.is_none());
    }

    #[test]
    fn a_truncated_type_layer_does_not_panic() {
        let full = psd_with_type_layer();
        for cut in 0..full.len() {
            let _ = parse(&full[..cut]);
        }
    }

    #[test]
    fn a_layered_file_comes_back_with_its_layers_pixels() {
        // The regression behind "PSDs open flat": the records were read and the
        // channel data skipped, so every layer was an empty rectangle and only
        // the composite had anything in it.
        let file = parse(&psd_with_one_layer(255)).expect("did not parse");
        assert_eq!(file.layers.len(), 1);

        let layer = file.layers.get(0).unwrap();
        assert_eq!(layer.name, "Paint");
        assert_eq!(layer.pixels.width(), 2);
        assert_eq!(layer.pixels.get(0, 0), Rgba8::new(200, 100, 50, 255));
        assert_eq!(layer.pixels.get(1, 1), Rgba8::new(200, 100, 50, 255));
    }

    #[test]
    fn a_layers_transparency_channel_is_its_alpha() {
        // Without this the layer is a solid block of its bounding box, which is
        // what makes a skipped transparency channel so obvious on screen.
        let file = parse(&psd_with_one_layer(0)).expect("did not parse");
        assert_eq!(file.layers.get(0).unwrap().pixels.get(0, 0).a, 0);
    }

    #[test]
    fn a_layers_metadata_survives_alongside_its_pixels() {
        let file = parse(&psd_with_one_layer(128)).expect("did not parse");
        let layer = file.layers.get(0).unwrap();
        assert_eq!(layer.offset, (0, 0));
        assert!(layer.visible);
        assert_eq!(layer.opacity, 1.0);
        assert_eq!(layer.pixels.get(0, 0).a, 128);
    }

    #[test]
    fn a_truncated_layered_file_does_not_panic() {
        // Every prefix of a real file, including ones cut mid-channel.
        let full = psd_with_one_layer(255);
        for cut in 0..full.len() {
            let _ = parse(&full[..cut]);
        }
    }

    #[test]
    fn rejects_a_bad_signature() {
        let mut data = header_bytes(4, 4, 3, 8, 3);
        data[0] = b'X';
        assert_eq!(parse_header(&data), Err(PsdError::BadSignature));
    }

    #[test]
    fn rejects_psb_version() {
        let mut data = header_bytes(4, 4, 3, 8, 3);
        data[4..6].copy_from_slice(&2u16.to_be_bytes());
        assert_eq!(parse_header(&data), Err(PsdError::BadVersion(2)));
    }

    #[test]
    fn rejects_zero_and_oversized_dimensions() {
        assert!(matches!(
            parse_header(&header_bytes(0, 4, 3, 8, 3)),
            Err(PsdError::InvalidDimensions { .. })
        ));
        assert!(matches!(
            parse_header(&header_bytes(40_000, 4, 3, 8, 3)),
            Err(PsdError::InvalidDimensions { .. })
        ));
    }

    #[test]
    fn truncated_input_reports_eof_not_panic() {
        let data = header_bytes(4, 4, 3, 8, 3);
        for cut in 0..data.len() {
            let result = parse_header(&data[..cut]);
            assert!(result.is_err(), "truncation at {} should fail", cut);
        }
    }

    #[test]
    fn parses_a_valid_header() {
        let h = parse_header(&header_bytes(640, 480, 3, 8, 3)).unwrap();
        assert_eq!(h.width, 640);
        assert_eq!(h.height, 480);
        assert_eq!(h.channels, 3);
        assert_eq!(h.depth, 8);
        assert_eq!(h.color_mode, ColorMode::Rgb);
    }

    #[test]
    fn unknown_color_mode_is_unsupported_not_a_panic() {
        assert!(matches!(
            parse_header(&header_bytes(4, 4, 3, 8, 99)),
            Err(PsdError::Unsupported(_))
        ));
    }

    #[test]
    fn sixteen_bit_depth_is_rejected_explicitly() {
        let mut data = header_bytes(4, 4, 3, 16, 3);
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&0u32.to_be_bytes());
        match parse(&data) {
            Err(PsdError::Unsupported(msg)) => assert!(msg.contains("16-bit"), "{}", msg),
            other => panic!("expected Unsupported, got {:?}", other.err()),
        }
    }

    #[test]
    fn cmyk_is_rejected_explicitly() {
        let mut data = header_bytes(4, 4, 4, 8, 4);
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&0u32.to_be_bytes());
        assert!(matches!(parse(&data), Err(PsdError::Unsupported(_))));
    }

    #[test]
    fn blend_mode_keys_round_trip() {
        for mode in BlendMode::ALL {
            let key = blend_mode_key(mode);
            assert_eq!(blend_mode_from_key(key), mode, "{:?}", mode);
        }
    }

    #[test]
    fn unknown_blend_key_defaults_to_normal() {
        assert_eq!(blend_mode_from_key(b"zzzz"), BlendMode::Normal);
    }

    #[test]
    fn packbits_decodes_literal_runs() {
        // 0x02 => copy the next 3 bytes literally.
        let mut out = Vec::new();
        unpack_bits(&[0x02, 1, 2, 3], 3, &mut out);
        assert_eq!(out, vec![1, 2, 3]);
    }

    #[test]
    fn packbits_decodes_repeat_runs() {
        // 0xFE is -2 => repeat the next byte 3 times.
        let mut out = Vec::new();
        unpack_bits(&[0xFE, 7], 3, &mut out);
        assert_eq!(out, vec![7, 7, 7]);
    }

    #[test]
    fn packbits_ignores_the_noop_marker() {
        let mut out = Vec::new();
        unpack_bits(&[0x80, 0x00, 5], 1, &mut out);
        assert_eq!(out, vec![5]);
    }

    #[test]
    fn packbits_pads_a_short_row() {
        let mut out = Vec::new();
        unpack_bits(&[0x00, 9], 4, &mut out);
        assert_eq!(out.len(), 4, "row was not padded to the expected width");
        assert_eq!(out[0], 9);
    }

    #[test]
    fn packbits_does_not_overrun_truncated_input() {
        let mut out = Vec::new();
        // Claims 4 literal bytes but supplies only 1.
        unpack_bits(&[0x03, 1], 4, &mut out);
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn written_psd_parses_back_with_the_same_pixels() {
        let mut src = Pixmap::new(4, 3);
        src.set(0, 0, Rgba8::new(255, 0, 0, 255));
        src.set(3, 2, Rgba8::new(0, 128, 64, 200));

        let bytes = write_psd(&src);
        let parsed = parse(&bytes).expect("round trip failed");

        assert_eq!(parsed.header.width, 4);
        assert_eq!(parsed.header.height, 3);

        let out = to_pixmap(&parsed);
        assert_eq!(out.get(0, 0), Rgba8::new(255, 0, 0, 255));
        assert_eq!(out.get(3, 2), Rgba8::new(0, 128, 64, 200));
    }

    #[test]
    fn written_psd_starts_with_the_magic() {
        let bytes = write_psd(&Pixmap::new(2, 2));
        assert_eq!(&bytes[..4], b"8BPS");
    }

    #[test]
    fn parse_is_robust_against_arbitrary_truncation() {
        let full = write_psd(&Pixmap::filled(8, 8, Rgba8::WHITE));
        // Every prefix must either parse or error — never panic.
        for cut in 0..full.len() {
            let _ = parse(&full[..cut]);
        }
    }

    #[test]
    fn parse_is_robust_against_corrupted_length_fields() {
        let mut data = write_psd(&Pixmap::filled(4, 4, Rgba8::WHITE));
        // Corrupt the image-resources length to a huge value.
        let resources_at = 26 + 4;
        data[resources_at..resources_at + 4].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
        assert!(parse(&data).is_err(), "should reject an impossible length");
    }

    #[test]
    fn errors_render_a_useful_message() {
        let e = PsdError::Unsupported("ZIP".into());
        assert!(e.to_string().contains("ZIP"));
        assert!(PsdError::BadSignature.to_string().contains("Photoshop"));
    }
}

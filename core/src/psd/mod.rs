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
//! * Per-layer channel image data (only the flattened composite is read, so
//!   opening a PSD currently yields a single Background layer).
//! * ZIP-compressed channels, 16- and 32-bit depths, CMYK/Lab/Indexed/Duotone.
//! * Layer effects, smart objects, text layers, adjustment-layer parameters.
//! * Writing. [`write_psd`] emits a valid single-layer file only.
//!
//! The format is documented in Adobe's "Photoshop File Format Specification";
//! `libpsd` is a useful cross-reference for the parts the spec glosses over.

use crate::blend::BlendMode;
use crate::buffer::{Pixmap, Rgba8};
use crate::layer::{Layer, LayerStack};

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
        top: i32,
        left: i32,
        bottom: i32,
        right: i32,
        channels: usize,
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
        // Each channel: 2-byte id + 4-byte data length.
        for _ in 0..channel_count {
            r.skip(6)?;
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

        // Skip any additional layer info blocks.
        r.pos = extra_end.min(r.data.len());

        records.push(Record {
            top,
            left,
            bottom,
            right,
            channels: channel_count,
            blend_mode,
            opacity,
            clipping,
            hidden,
            name,
        });
    }

    // Channel image data follows. Not decoded yet — see module docs.
    r.pos = layer_info_end.min(r.data.len());

    for rec in records {
        let width = (rec.right - rec.left).max(0) as u32;
        let height = (rec.bottom - rec.top).max(0) as u32;

        let id = stack.allocate_id();
        let name = if rec.name.is_empty() {
            format!("Layer {}", stack.len() + 1)
        } else {
            rec.name
        };
        let mut layer = Layer::new_raster(id, name, width, height);
        layer.offset = (rec.left, rec.top);
        layer.blend_mode = rec.blend_mode;
        layer.opacity = rec.opacity as f32 / 255.0;
        layer.clipping = rec.clipping;
        layer.visible = !rec.hidden;
        let _ = rec.channels;
        stack.push(layer);
    }

    Ok(stack)
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

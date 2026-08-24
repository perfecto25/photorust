//! Reading what a file says about itself — the engine behind File ▸ File Info.
//!
//! Photoshop's File Info shows a stack of standards: XMP, EXIF, IPTC, GPS and
//! more. What is read here is the part that is both widely present and cheap to
//! get right:
//!
//! - **EXIF**, parsed properly out of JPEG and TIFF — camera, lens, exposure,
//!   orientation and resolution. This is the tab a photographer actually opens.
//! - **The XMP packet**, lifted whole from any format that carries one, for the
//!   Raw Data view.
//!
//! IPTC, GPS and the rest are *not* read. They are listed in the dialog and say
//! so, rather than being quietly absent — the same way the tool flyouts list
//! variants that are not built yet.
//!
//! Nothing here writes. File Info in Photoshop is an editor; ours is a reader,
//! because writing XMP back means rewriting a file we did not author, and doing
//! that badly loses data that was never ours to lose.

/// One thing a file says about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// Which pane of the dialog it belongs on, e.g. "Camera Data".
    pub category: String,
    pub label: String,
    pub value: String,
}

impl Field {
    fn new(category: &str, label: &str, value: impl Into<String>) -> Self {
        Self {
            category: category.to_string(),
            label: label.to_string(),
            value: value.into(),
        }
    }
}

const CAMERA: &str = "Camera Data";
const BASIC: &str = "Basic";

/// Everything read out of one file.
#[derive(Clone, Debug, Default)]
pub struct Metadata {
    pub fields: Vec<Field>,
    /// The XMP packet as it appears in the file, if it has one.
    pub xmp: Option<String>,
    /// The EXIF orientation tag, 1-8, when the file carries one.
    ///
    /// Kept as a number as well as a field, because this one is not only shown:
    /// a camera that was held sideways records the pixels as the sensor read
    /// them and leaves this to say which way up they go. Every viewer worth the
    /// name applies it, which is why a photo can look upright everywhere and
    /// sideways in a program that skips it.
    pub orientation: Option<u32>,
}

/// How a file's pixels have to be turned to be the right way up.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Orientation {
    #[default]
    Upright,
    FlipHorizontal,
    Rotate180,
    FlipVertical,
    /// Mirrored along the top-left/bottom-right diagonal.
    Transpose,
    Rotate90Cw,
    /// Mirrored along the other diagonal.
    Transverse,
    Rotate90Ccw,
}

impl Orientation {
    /// From the EXIF tag's value. Anything outside 1-8 is a file being strange,
    /// and leaving those alone is safer than guessing.
    pub fn from_exif(value: u32) -> Orientation {
        match value {
            2 => Orientation::FlipHorizontal,
            3 => Orientation::Rotate180,
            4 => Orientation::FlipVertical,
            5 => Orientation::Transpose,
            6 => Orientation::Rotate90Cw,
            7 => Orientation::Transverse,
            8 => Orientation::Rotate90Ccw,
            _ => Orientation::Upright,
        }
    }

    /// Whether applying it swaps width and height.
    pub fn swaps_axes(self) -> bool {
        matches!(
            self,
            Orientation::Transpose
                | Orientation::Rotate90Cw
                | Orientation::Transverse
                | Orientation::Rotate90Ccw
        )
    }
}

/// Read what `bytes` say about themselves.
///
/// Unrecognised or truncated files come back empty rather than failing: File
/// Info showing nothing is a fair answer for a file with nothing to show, and
/// there is no partial state to report.
pub fn read(bytes: &[u8]) -> Metadata {
    let mut meta = Metadata {
        xmp: find_xmp(bytes),
        ..Metadata::default()
    };

    if let Some(exif) = find_exif(bytes) {
        let mut orientation = None;
        read_exif(exif, &mut meta.fields, &mut orientation);
        meta.orientation = orientation;
    }
    meta
}

// ---------------------------------------------------------------------------
// Finding the blocks
// ---------------------------------------------------------------------------

/// The TIFF header an EXIF block starts with, wherever it is embedded.
fn find_exif(bytes: &[u8]) -> Option<&[u8]> {
    // A TIFF *is* the EXIF structure, so it can be parsed where it lies.
    if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        return Some(bytes);
    }
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return None;
    }

    // JPEG: walk the segment chain looking for the APP1 that starts "Exif\0\0".
    // Walking rather than searching, because the byte pattern can occur inside
    // compressed image data and a false hit would be parsed as an IFD.
    let mut at = 2usize;
    while at + 4 <= bytes.len() {
        if bytes[at] != 0xFF {
            return None;
        }
        let marker = bytes[at + 1];
        // Standalone markers carry no length; start-of-scan means the segments
        // are over and the rest is entropy-coded data.
        if marker == 0xD8 || (0xD0..=0xD7).contains(&marker) {
            at += 2;
            continue;
        }
        if marker == 0xDA || marker == 0xD9 {
            return None;
        }

        let length = u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]) as usize;
        if length < 2 || at + 2 + length > bytes.len() {
            return None;
        }
        let payload = &bytes[at + 4..at + 2 + length];
        if marker == 0xE1 && payload.starts_with(b"Exif\0\0") {
            return Some(&payload[6..]);
        }
        at += 2 + length;
    }
    None
}

/// The XMP packet, wherever it is.
///
/// Found by scanning for its own opening and closing tags rather than by
/// unpicking each format's container — PSD keeps it in an image resource, PNG
/// in an `iTXt` chunk, JPEG in an APP1 — because the packet is self-delimiting
/// XML and every format stores it verbatim. That is what makes one scan
/// correct for all of them.
fn find_xmp(bytes: &[u8]) -> Option<String> {
    const OPEN: &[u8] = b"<x:xmpmeta";
    const CLOSE: &[u8] = b"</x:xmpmeta>";

    let start = find(bytes, OPEN)?;
    let end = find(&bytes[start..], CLOSE)? + start + CLOSE.len();
    Some(String::from_utf8_lossy(&bytes[start..end]).into_owned())
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

// ---------------------------------------------------------------------------
// EXIF
// ---------------------------------------------------------------------------

/// A reader over the TIFF structure EXIF is stored in, carrying its byte order.
struct Tiff<'a> {
    bytes: &'a [u8],
    big_endian: bool,
}

impl<'a> Tiff<'a> {
    fn new(bytes: &'a [u8]) -> Option<Self> {
        let big_endian = match bytes.get(..2)? {
            b"MM" => true,
            b"II" => false,
            _ => return None,
        };
        Some(Self { bytes, big_endian })
    }

    fn u16_at(&self, at: usize) -> Option<u16> {
        let raw = self.bytes.get(at..at + 2)?.try_into().ok()?;
        Some(if self.big_endian {
            u16::from_be_bytes(raw)
        } else {
            u16::from_le_bytes(raw)
        })
    }

    fn u32_at(&self, at: usize) -> Option<u32> {
        let raw = self.bytes.get(at..at + 4)?.try_into().ok()?;
        Some(if self.big_endian {
            u32::from_be_bytes(raw)
        } else {
            u32::from_le_bytes(raw)
        })
    }
}

/// One entry of an image file directory.
struct Entry {
    tag: u16,
    format: u16,
    count: u32,
    /// The value itself when it fits in four bytes, or where to find it.
    offset: usize,
}

/// Sizes of the TIFF value formats, indexed by their format code.
const FORMAT_SIZES: [usize; 13] = [0, 1, 1, 2, 4, 8, 1, 1, 2, 4, 8, 4, 8];

fn read_exif(bytes: &[u8], fields: &mut Vec<Field>, orientation: &mut Option<u32>) {
    let Some(tiff) = Tiff::new(bytes) else { return };
    let Some(first_ifd) = tiff.u32_at(4) else { return };

    let mut entries = Vec::new();
    collect_ifd(&tiff, first_ifd as usize, &mut entries, 0);
    if entries.is_empty() {
        return;
    }

    // Camera and lens.
    push_text(&tiff, &entries, 0x010F, CAMERA, "Make", fields);
    push_text(&tiff, &entries, 0x0110, CAMERA, "Model", fields);
    push_text(&tiff, &entries, 0xA434, CAMERA, "Lens", fields);
    push_text(&tiff, &entries, 0x013B, CAMERA, "Artist", fields);

    // The shot.
    if let Some(value) = rational(&tiff, &entries, 0x920A) {
        fields.push(Field::new(CAMERA, "Focal Length", format!("{value:.0} mm")));
    }
    if let Some(value) = rational(&tiff, &entries, 0x829A) {
        // Under a second, photographers read the reciprocal: 1/250, not 0.004.
        let shown = if value > 0.0 && value < 1.0 {
            format!("1/{:.0} sec", 1.0 / value)
        } else {
            format!("{value:.1} sec")
        };
        fields.push(Field::new(CAMERA, "Exposure Time", shown));
    }
    if let Some(value) = rational(&tiff, &entries, 0x829D) {
        fields.push(Field::new(CAMERA, "F-Stop", format!("f/{value:.1}")));
    }
    if let Some(value) = integer(&tiff, &entries, 0x8827) {
        fields.push(Field::new(CAMERA, "ISO Speed Rating", format!("ISO {value}")));
    }
    if let Some(value) = integer(&tiff, &entries, 0x9209) {
        // The low bit is the only part everyone agrees on: did it fire.
        let fired = value & 1 == 1;
        fields.push(Field::new(
            CAMERA,
            "Flash",
            if fired { "Did fire" } else { "Did not fire" },
        ));
    }
    if let Some(value) = integer(&tiff, &entries, 0x0112) {
        fields.push(Field::new(CAMERA, "Orientation", orientation_name(value)));
        *orientation = Some(value);
    }
    if let Some(value) = rational(&tiff, &entries, 0x011A) {
        let unit = match integer(&tiff, &entries, 0x0128) {
            Some(3) => "Pixels per Centimeter",
            _ => "Pixels per Inch",
        };
        fields.push(Field::new(CAMERA, "Resolution", format!("{value:.2} {unit}")));
    }

    // When, which the Basic pane shows alongside the file's own dates.
    push_text(&tiff, &entries, 0x9003, BASIC, "Date Taken", fields);
    push_text(&tiff, &entries, 0x0131, BASIC, "Application", fields);
    push_text(&tiff, &entries, 0x010E, BASIC, "Description", fields);
    push_text(&tiff, &entries, 0x8298, BASIC, "Copyright Notice", fields);
}

/// Read an image file directory and, through the EXIF pointer tag, whichever
/// sub-directory hangs off it.
///
/// `depth` stops a file whose pointers loop back on themselves from spinning
/// here forever — a malformed file must not hang the application.
fn collect_ifd(tiff: &Tiff, at: usize, entries: &mut Vec<Entry>, depth: u32) {
    const MAX_DEPTH: u32 = 4;
    if depth > MAX_DEPTH || at == 0 {
        return;
    }
    let Some(count) = tiff.u16_at(at) else { return };

    let mut sub_ifds = Vec::new();
    for i in 0..count as usize {
        let entry_at = at + 2 + i * 12;
        let (Some(tag), Some(format), Some(value_count)) = (
            tiff.u16_at(entry_at),
            tiff.u16_at(entry_at + 2),
            tiff.u32_at(entry_at + 4),
        ) else {
            return;
        };

        // A value of four bytes or fewer is stored in the entry itself; longer
        // ones put an offset there instead.
        let size = FORMAT_SIZES.get(format as usize).copied().unwrap_or(0);
        let total = size.saturating_mul(value_count as usize);
        let offset = if total > 4 {
            match tiff.u32_at(entry_at + 8) {
                Some(offset) => offset as usize,
                None => return,
            }
        } else {
            entry_at + 8
        };

        // The EXIF and GPS sub-directories, which hold most of what is worth
        // showing.
        if tag == 0x8769 || tag == 0x8825 {
            if let Some(sub) = tiff.u32_at(entry_at + 8) {
                sub_ifds.push(sub as usize);
            }
            continue;
        }

        entries.push(Entry { tag, format, count: value_count, offset });
    }

    for sub in sub_ifds {
        collect_ifd(tiff, sub, entries, depth + 1);
    }
}

fn entry<'e>(entries: &'e [Entry], tag: u16) -> Option<&'e Entry> {
    entries.iter().find(|e| e.tag == tag)
}

fn push_text(
    tiff: &Tiff,
    entries: &[Entry],
    tag: u16,
    category: &str,
    label: &str,
    fields: &mut Vec<Field>,
) {
    if let Some(text) = text(tiff, entries, tag) {
        if !text.is_empty() {
            fields.push(Field::new(category, label, text));
        }
    }
}

fn text(tiff: &Tiff, entries: &[Entry], tag: u16) -> Option<String> {
    let entry = entry(entries, tag)?;
    // Format 2 is ASCII; anything else under this tag is a file being strange.
    if entry.format != 2 {
        return None;
    }
    let end = entry.offset + entry.count as usize;
    let raw = tiff.bytes.get(entry.offset..end.min(tiff.bytes.len()))?;
    let text = String::from_utf8_lossy(raw);
    Some(text.trim_end_matches(['\0', ' ']).to_string())
}

fn integer(tiff: &Tiff, entries: &[Entry], tag: u16) -> Option<u32> {
    let entry = entry(entries, tag)?;
    match entry.format {
        3 => tiff.u16_at(entry.offset).map(u32::from),
        4 => tiff.u32_at(entry.offset),
        _ => None,
    }
}

/// A rational value — EXIF stores exposure, aperture and focal length as pairs
/// of integers rather than as decimals.
fn rational(tiff: &Tiff, entries: &[Entry], tag: u16) -> Option<f64> {
    let entry = entry(entries, tag)?;
    if entry.format != 5 && entry.format != 10 {
        return None;
    }
    let numerator = tiff.u32_at(entry.offset)?;
    let denominator = tiff.u32_at(entry.offset + 4)?;
    if denominator == 0 {
        return None;
    }

    if entry.format == 10 {
        // Signed rational: reinterpret both halves.
        return Some(f64::from(numerator as i32) / f64::from(denominator as i32));
    }
    Some(f64::from(numerator) / f64::from(denominator))
}

fn orientation_name(value: u32) -> String {
    let name = match value {
        1 => "Normal",
        2 => "Flipped horizontally",
        3 => "Rotated 180°",
        4 => "Flipped vertically",
        5 => "Transposed",
        6 => "Rotated 90° clockwise",
        7 => "Transversed",
        8 => "Rotated 90° counter-clockwise",
        _ => "Unknown",
    };
    format!("{value} ({name})")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A little-endian TIFF with one IFD holding Make, Model and an exposure.
    fn tiff_with_camera() -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"II*\0");
        out.extend_from_slice(&8u32.to_le_bytes()); // first IFD at byte 8

        let entries: [(u16, u16, u32, u32); 3] = [
            // Make: 6 bytes of ASCII, stored out of line at 0x50.
            (0x010F, 2, 6, 0x50),
            // Model: 6 bytes, at 0x58.
            (0x0110, 2, 6, 0x58),
            // Exposure time: a rational at 0x60.
            (0x829A, 5, 1, 0x60),
        ];

        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        for (tag, format, count, value) in entries {
            out.extend_from_slice(&tag.to_le_bytes());
            out.extend_from_slice(&format.to_le_bytes());
            out.extend_from_slice(&count.to_le_bytes());
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(&0u32.to_le_bytes()); // no next IFD

        out.resize(0x50, 0);
        out.extend_from_slice(b"Canon\0");
        out.resize(0x58, 0);
        out.extend_from_slice(b"EOS 5\0");
        out.resize(0x60, 0);
        out.extend_from_slice(&1u32.to_le_bytes()); // 1/250 sec
        out.extend_from_slice(&250u32.to_le_bytes());
        out
    }

    fn value_of(meta: &Metadata, label: &str) -> Option<String> {
        meta.fields
            .iter()
            .find(|f| f.label == label)
            .map(|f| f.value.clone())
    }

    #[test]
    fn it_reads_camera_fields_out_of_a_tiff() {
        let meta = read(&tiff_with_camera());
        assert_eq!(value_of(&meta, "Make").as_deref(), Some("Canon"));
        assert_eq!(value_of(&meta, "Model").as_deref(), Some("EOS 5"));
    }

    #[test]
    fn a_short_exposure_is_shown_as_a_fraction() {
        // 0.004 sec is how the file stores it and not how anyone reads it.
        let meta = read(&tiff_with_camera());
        assert_eq!(value_of(&meta, "Exposure Time").as_deref(), Some("1/250 sec"));
    }

    #[test]
    fn exif_inside_a_jpeg_is_found_through_its_segments() {
        // The same directory, wrapped in the APP1 segment a JPEG carries it in.
        let exif = tiff_with_camera();
        let mut jpeg = vec![0xFF, 0xD8];
        let payload_len = exif.len() + 6 + 2;
        jpeg.extend_from_slice(&[0xFF, 0xE1]);
        jpeg.extend_from_slice(&(payload_len as u16).to_be_bytes());
        jpeg.extend_from_slice(b"Exif\0\0");
        jpeg.extend_from_slice(&exif);
        jpeg.extend_from_slice(&[0xFF, 0xD9]);

        let meta = read(&jpeg);
        assert_eq!(value_of(&meta, "Make").as_deref(), Some("Canon"));
    }

    #[test]
    fn the_xmp_packet_comes_back_whole() {
        let mut bytes = b"\x89PNG\r\n\x1a\n....".to_vec();
        bytes.extend_from_slice(
            br#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF/></x:xmpmeta>"#,
        );
        bytes.extend_from_slice(b"trailing junk");

        let meta = read(&bytes);
        let xmp = meta.xmp.expect("no packet found");
        assert!(xmp.starts_with("<x:xmpmeta"));
        assert!(xmp.ends_with("</x:xmpmeta>"));
        assert!(!xmp.contains("trailing junk"), "the scan ran past the packet");
    }

    #[test]
    fn the_orientation_tag_comes_back_as_a_number_too() {
        // Photographs from a camera held sideways are the whole reason: the
        // number is what puts them upright, not the text.
        let mut bytes = tiff_with_camera();
        // Add an orientation entry to the directory: bump the count and write
        // it after the three that are there.
        bytes[8] = 4;
        let entry_at = 8 + 2 + 3 * 12;
        let entry = [
            0x12u8, 0x01, // tag 0x0112, little endian
            0x03, 0x00, // format 3 (short)
            0x01, 0x00, 0x00, 0x00, // one value
            0x06, 0x00, 0x00, 0x00, // 6: rotate 90° clockwise
        ];
        bytes.splice(entry_at..entry_at, entry);

        let meta = read(&bytes);
        assert_eq!(meta.orientation, Some(6));
        assert_eq!(
            Orientation::from_exif(6),
            Orientation::Rotate90Cw,
            "the tag was read as the wrong turn"
        );
    }

    #[test]
    fn an_orientation_outside_the_standard_is_left_alone() {
        // Better an untouched photograph than one turned on a guess.
        assert_eq!(Orientation::from_exif(0), Orientation::Upright);
        assert_eq!(Orientation::from_exif(9), Orientation::Upright);
    }

    #[test]
    fn only_the_quarter_turns_swap_the_axes() {
        assert!(Orientation::Rotate90Cw.swaps_axes());
        assert!(Orientation::Rotate90Ccw.swaps_axes());
        assert!(Orientation::Transpose.swaps_axes());
        assert!(Orientation::Transverse.swaps_axes());
        assert!(!Orientation::Rotate180.swaps_axes());
        assert!(!Orientation::FlipHorizontal.swaps_axes());
    }

    #[test]
    fn a_file_with_nothing_to_say_reads_empty() {
        let meta = read(b"not an image at all");
        assert!(meta.fields.is_empty());
        assert!(meta.xmp.is_none());
    }

    #[test]
    fn a_truncated_file_does_not_panic() {
        let full = tiff_with_camera();
        for cut in 0..full.len() {
            let _ = read(&full[..cut]);
        }
    }

    #[test]
    fn a_directory_that_points_at_itself_does_not_hang() {
        // IFD0 at byte 8 with an EXIF pointer back to byte 8. Depth-limited
        // rather than visited-tracked: the limit is what makes this terminate.
        let mut out = Vec::new();
        out.extend_from_slice(b"II*\0");
        out.extend_from_slice(&8u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&0x8769u16.to_le_bytes()); // EXIF sub-IFD pointer
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&8u32.to_le_bytes()); // ...pointing at itself
        out.extend_from_slice(&0u32.to_le_bytes());

        let _ = read(&out);
    }
}

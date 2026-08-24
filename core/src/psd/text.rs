//! Reading a PSD type layer — the `TySh` block.
//!
//! Photoshop stores a type layer twice over. The pixels are in the ordinary
//! channels, like any other layer, and *what it says* is in an additional layer
//! information block keyed `TySh`. Skip that block and the layer opens as the
//! picture of some text rather than as text, which is what makes it
//! uneditable.
//!
//! The block is two formats nested:
//!
//! - a **descriptor**, Photoshop's general key/value tree, holding the string
//!   itself under `Txt `;
//! - **EngineData**, the text engine's own dump, holding how it is set — font,
//!   size, colour, justification.
//!
//! The descriptor is walked properly, because finding one key means stepping
//! correctly over every other type. EngineData is *scanned* rather than parsed:
//! it is a large nested format of which we want four values, all of which are
//! written as plain ASCII within it, and a scan for those is a tenth of the
//! code with a failure mode — a missing value falling back to a default — that
//! costs the user a font rather than the layer.

use crate::buffer::Rgba8;
use crate::layer::TextAlign;

/// What a type layer says and how it is set.
#[derive(Clone, Debug, PartialEq)]
pub struct PsdText {
    pub text: String,
    /// Family and style split out of the PostScript name EngineData carries.
    pub family: String,
    pub style: String,
    pub size: f32,
    pub color: Rgba8,
    pub align: TextAlign,
}

impl Default for PsdText {
    fn default() -> Self {
        Self {
            text: String::new(),
            family: String::new(),
            style: "Regular".to_string(),
            size: 12.0,
            color: Rgba8::BLACK,
            align: TextAlign::Left,
        }
    }
}

/// Read a `TySh` block. `None` when it is not one we understand, which leaves
/// the layer as ordinary pixels rather than failing the file.
pub fn parse_type_tool(block: &[u8]) -> Option<PsdText> {
    let mut r = Cursor::new(block);

    r.skip(2)?; // version
    r.skip(6 * 8)?; // the 2x3 transform, which the caller does not need
    r.skip(2)?; // text version
    r.skip(4)?; // descriptor version

    let descriptor = read_descriptor(&mut r)?;

    let mut text = PsdText {
        text: descriptor.text.unwrap_or_default(),
        ..PsdText::default()
    };
    if let Some(engine) = descriptor.engine_data {
        apply_engine_data(&engine, &mut text);
    }
    if text.text.is_empty() {
        return None;
    }
    Some(text)
}

/// The two values worth having out of the text descriptor.
#[derive(Default)]
struct Descriptor {
    text: Option<String>,
    engine_data: Option<Vec<u8>>,
}

/// A little cursor that reports running out rather than panicking.
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let slice = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    fn skip(&mut self, n: usize) -> Option<()> {
        self.take(n).map(|_| ())
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }

    fn f64(&mut self) -> Option<f64> {
        Some(f64::from_be_bytes(self.take(8)?.try_into().ok()?))
    }

    /// A four-character type or key code.
    fn key4(&mut self) -> Option<[u8; 4]> {
        self.take(4)?.try_into().ok()
    }

    /// A length-prefixed key: four bytes of length, or zero meaning the key is
    /// itself a four-character code.
    fn key(&mut self) -> Option<Vec<u8>> {
        let length = self.u32()? as usize;
        if length == 0 {
            return Some(self.key4()?.to_vec());
        }
        Some(self.take(length)?.to_vec())
    }

    /// A UTF-16BE string, four-byte length in *characters* including the
    /// trailing null Photoshop writes.
    fn unicode_string(&mut self) -> Option<String> {
        let count = self.u32()? as usize;
        let bytes = self.take(count.checked_mul(2)?)?;
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        let text = String::from_utf16_lossy(&units);
        Some(text.trim_end_matches('\0').to_string())
    }
}

/// Walk a descriptor, keeping the two items that matter.
///
/// Everything else still has to be *stepped over* exactly, since the items are
/// laid end to end with no lengths to skip by — which is why this understands
/// every type rather than searching for the keys it wants.
fn read_descriptor(r: &mut Cursor<'_>) -> Option<Descriptor> {
    r.unicode_string()?; // the descriptor's own name, always empty in practice
    let class_length = r.u32()? as usize;
    if class_length == 0 {
        r.skip(4)?;
    } else {
        r.skip(class_length)?;
    }

    let mut out = Descriptor::default();
    let count = r.u32()? as usize;
    // A count in the thousands means the block is not what we think it is.
    if count > 4096 {
        return None;
    }

    for _ in 0..count {
        let key = r.key()?;
        let kind = r.key4()?;

        match (&kind, key.as_slice()) {
            (b"TEXT", b"Txt ") => out.text = Some(r.unicode_string()?),
            (b"tdta", b"EngineData") => {
                let length = r.u32()? as usize;
                out.engine_data = Some(r.take(length)?.to_vec());
            }
            _ => skip_value(r, &kind)?,
        }
    }
    Some(out)
}

/// Step over one descriptor value of the given type.
fn skip_value(r: &mut Cursor<'_>, kind: &[u8; 4]) -> Option<()> {
    match kind {
        b"obj " | b"Objc" | b"GlbO" => {
            // A nested descriptor, read for its structure and discarded.
            read_descriptor(r)?;
        }
        b"VlLs" => {
            let count = r.u32()? as usize;
            if count > 65536 {
                return None;
            }
            for _ in 0..count {
                let kind = r.key4()?;
                skip_value(r, &kind)?;
            }
        }
        b"doub" => r.skip(8)?,
        b"UntF" => {
            r.skip(4)?; // unit
            r.skip(8)?; // value
        }
        b"TEXT" => {
            r.unicode_string()?;
        }
        b"enum" | b"type" | b"GlbC" => {
            // Two length-prefixed keys: the type and the value.
            r.key()?;
            r.key()?;
        }
        b"long" | b"comp" => r.skip(4)?,
        b"bool" => r.skip(1)?,
        b"alis" | b"tdta" => {
            let length = r.u32()? as usize;
            r.skip(length)?;
        }
        b"ObAr" => {
            // An object array: version, name, class, then typed entries.
            r.skip(4)?;
            r.unicode_string()?;
            r.key()?;
            let count = r.u32()? as usize;
            if count > 65536 {
                return None;
            }
            for _ in 0..count {
                r.key()?;
                let kind = r.key4()?;
                skip_value(r, &kind)?;
            }
        }
        b"UnFl" => {
            r.skip(4)?;
            let count = r.u32()? as usize;
            r.skip(count.checked_mul(8)?)?;
        }
        // An unknown type has no length to skip by, so the walk has to stop
        // here rather than guess and read rubbish.
        _ => return None,
    }
    Some(())
}

// ---------------------------------------------------------------------------
// EngineData
// ---------------------------------------------------------------------------

/// Pull the font, size, colour and justification out of an EngineData blob.
///
/// Only the *first* style run is read. Photoshop stores a run array, and a
/// piece of text with two fonts in it has two entries; taking the first gives
/// the whole layer the formatting its opening character has. Our own type model
/// has runs and could hold more, but mapping Photoshop's run lengths onto them
/// means reading the whole engine format properly — so this is deliberately the
/// first step, and the layer's pixels are unchanged until it is edited.
fn apply_engine_data(data: &[u8], text: &mut PsdText) {
    // The blob is ASCII with binary stretches; lossy is fine, the keys are all
    // in the ASCII range.
    let engine = String::from_utf8_lossy(data);

    if let Some(size) = find_number(&engine, "/FontSize ") {
        text.size = size as f32;
    }
    if let Some(justification) = find_number(&engine, "/Justification ") {
        text.align = match justification as i32 {
            1 => TextAlign::Right,
            2 => TextAlign::Center,
            _ => TextAlign::Left,
        };
    }
    if let Some(color) = find_fill_color(&engine) {
        text.color = color;
    }
    if let Some(name) = find_font_name(&engine) {
        let (family, style) = split_postscript_name(&name);
        text.family = family;
        text.style = style;
    }
}

/// The number written after `key`.
fn find_number(engine: &str, key: &str) -> Option<f64> {
    let at = engine.find(key)? + key.len();
    let rest = &engine[at..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// The first `/FillColor` in the run, whose `/Values` are alpha, red, green and
/// blue as fractions.
fn find_fill_color(engine: &str) -> Option<Rgba8> {
    let at = engine.find("/FillColor")?;
    let values_at = engine[at..].find("/Values [")? + at + "/Values [".len();
    let end = engine[values_at..].find(']')? + values_at;

    let numbers: Vec<f32> = engine[values_at..end]
        .split_whitespace()
        .filter_map(|n| n.parse().ok())
        .collect();
    if numbers.len() < 4 {
        return None;
    }

    let channel = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    Some(Rgba8::new(
        channel(numbers[1]),
        channel(numbers[2]),
        channel(numbers[3]),
        channel(numbers[0]),
    ))
}

/// The first font name in the resource dictionary.
fn find_font_name(engine: &str) -> Option<String> {
    let set_at = engine.find("/FontSet")?;
    let name_at = engine[set_at..].find("/Name")? + set_at;
    let open = engine[name_at..].find('(')? + name_at + 1;
    let close = engine[open..].find(')')? + open;
    let name = engine[open..close].trim();

    // Photoshop writes UTF-16 in these parentheses when the name needs it,
    // which lands here as text riddled with nulls; dropping them leaves the
    // ASCII name, which is what font names almost always are.
    let cleaned: String = name.chars().filter(|c| *c != '\0' && *c != '\u{feff}' && *c != '\u{fffd}').collect();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// Split a PostScript font name into the family and style a font database
/// wants: `Georgia-Bold` becomes `Georgia` and `Bold`.
///
/// A heuristic, and knowingly so: PostScript names are one token by design and
/// the hyphen convention is a convention, not a rule. Getting it wrong costs
/// the reopened text its style, not its content.
fn split_postscript_name(name: &str) -> (String, String) {
    match name.split_once('-') {
        Some((family, style)) if !style.is_empty() => {
            // "BoldItalic" reads better as "Bold Italic", which is what a font
            // database lists it under.
            let mut spaced = String::new();
            for (i, c) in style.char_indices() {
                if i > 0 && c.is_uppercase() {
                    spaced.push(' ');
                }
                spaced.push(c);
            }
            (family.to_string(), spaced)
        }
        _ => (name.to_string(), "Regular".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unicode(text: &str) -> Vec<u8> {
        let mut out = Vec::new();
        let units: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        out.extend_from_slice(&(units.len() as u32).to_be_bytes());
        for unit in units {
            out.extend_from_slice(&unit.to_be_bytes());
        }
        out
    }

    /// A `TySh` block holding `text`, with an EngineData blob describing how it
    /// is set. Built by hand so the layout under test is visible.
    fn type_tool_block(text: &str, engine: &str) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&1u16.to_be_bytes()); // version
        for value in [1.0f64, 0.0, 0.0, 1.0, 40.0, 80.0] {
            v.extend_from_slice(&value.to_be_bytes()); // transform
        }
        v.extend_from_slice(&50u16.to_be_bytes()); // text version
        v.extend_from_slice(&16u32.to_be_bytes()); // descriptor version

        // The text descriptor: an empty name, a class, and two items.
        v.extend_from_slice(&unicode(""));
        v.extend_from_slice(&0u32.to_be_bytes());
        v.extend_from_slice(b"null");
        v.extend_from_slice(&2u32.to_be_bytes());

        v.extend_from_slice(&0u32.to_be_bytes());
        v.extend_from_slice(b"Txt ");
        v.extend_from_slice(b"TEXT");
        v.extend_from_slice(&unicode(text));

        v.extend_from_slice(&(10u32).to_be_bytes());
        v.extend_from_slice(b"EngineData");
        v.extend_from_slice(b"tdta");
        v.extend_from_slice(&(engine.len() as u32).to_be_bytes());
        v.extend_from_slice(engine.as_bytes());
        v
    }

    const ENGINE: &str = "<< /ResourceDict << /FontSet [ << /Name (Georgia-Bold) >> ] >> \
                          /EngineDict << /StyleRun << /RunArray [ << /StyleSheet << \
                          /StyleSheetData << /Font 0 /FontSize 72.0 /FillColor << \
                          /Values [ 1.0 0.2 0.4 0.6 ] >> >> >> >> ] >> \
                          /ParagraphRun << /RunArray [ << /ParagraphSheet << \
                          /Properties << /Justification 2 >> >> >> ] >> >> >>";

    #[test]
    fn it_reads_the_string_out_of_a_type_layer() {
        // The whole point: without this the layer is a picture of its text.
        let block = type_tool_block("TESTTEST", ENGINE);
        let text = parse_type_tool(&block).expect("no type data found");
        assert_eq!(text.text, "TESTTEST");
    }

    #[test]
    fn it_reads_how_the_text_is_set() {
        let block = type_tool_block("Hello", ENGINE);
        let text = parse_type_tool(&block).unwrap();
        assert_eq!(text.family, "Georgia");
        assert_eq!(text.style, "Bold");
        assert_eq!(text.size, 72.0);
        assert_eq!(text.align, TextAlign::Center);
        // Values are alpha first, then red, green, blue, as fractions.
        assert_eq!(text.color, Rgba8::new(51, 102, 153, 255));
    }

    #[test]
    fn text_with_more_than_one_line_survives() {
        let block = type_tool_block("two\rlines", ENGINE);
        assert_eq!(parse_type_tool(&block).unwrap().text, "two\rlines");
    }

    #[test]
    fn a_layer_without_engine_data_still_gives_its_text() {
        // Only the string is essential; everything else has a default.
        let block = type_tool_block("Plain", "<< >>");
        let text = parse_type_tool(&block).unwrap();
        assert_eq!(text.text, "Plain");
        assert_eq!(text.size, 12.0);
        assert_eq!(text.style, "Regular");
    }

    #[test]
    fn a_block_that_is_not_a_type_tool_is_declined() {
        assert!(parse_type_tool(b"nonsense").is_none());
        assert!(parse_type_tool(&[]).is_none());
    }

    #[test]
    fn a_truncated_block_does_not_panic() {
        let full = type_tool_block("TESTTEST", ENGINE);
        for cut in 0..full.len() {
            let _ = parse_type_tool(&full[..cut]);
        }
    }

    #[test]
    fn postscript_names_split_into_family_and_style() {
        assert_eq!(split_postscript_name("Georgia-Bold"), ("Georgia".into(), "Bold".into()));
        assert_eq!(
            split_postscript_name("Helvetica-BoldOblique"),
            ("Helvetica".into(), "Bold Oblique".into())
        );
        // No hyphen: the whole name is the family.
        assert_eq!(split_postscript_name("Impact"), ("Impact".into(), "Regular".into()));
    }
}

//! Writing a PSD type layer — the `TySh` block.
//!
//! The mirror of [`super::text`]. A type layer's pixels go into the ordinary
//! channels like any other layer's, and what makes Photoshop treat it as *type*
//! rather than as a picture of some text is this block sitting in the layer's
//! additional information. Without it a file we write opens in Photoshop with
//! the text rasterized and the Type tool refusing it.
//!
//! Two formats again, nested:
//!
//! - the **descriptor**, carrying the string and the layer-level choices;
//! - **EngineData**, the text engine's own dump, carrying how it is set.
//!
//! Reading could be selective — walk to the four values worth having and ignore
//! the rest. Writing cannot. Photoshop builds a live text object out of what is
//! here, so the dump has to be complete enough to construct one: the run arrays
//! have to agree with the string's length, every run has to name a font in the
//! font set, and the sheets a run refers to have to exist. Anything short of
//! that and Photoshop falls back to the pixels, which is exactly the failure
//! this exists to fix.
//!
//! What is deliberately *not* written: warping (the descriptor says "no warp"),
//! kinsoku and mojikumi sets (empty — they matter for East Asian line breaking,
//! which we do not offer), and Photoshop's `Rendered` cache, which it rebuilds.

use crate::buffer::Rgba8;
use crate::layer::{TextAlign, TextContent, TextRun};

/// The `TySh` block for `text`.
///
/// `bounds` is the text's box in the layer's own space — left, top, right,
/// bottom relative to the text origin — which is what the descriptor's `bounds`
/// and `boundingBox` want.
pub fn type_tool_block(text: &TextContent, bounds: (f32, f32, f32, f32)) -> Vec<u8> {
    let mut out = Vec::new();

    out.extend_from_slice(&1u16.to_be_bytes()); // version

    // The 2×3 transform: no scale or rotation, translated to where the text was
    // clicked. This is what puts the reopened text back under the same pixels.
    for value in [
        1.0f64,
        0.0,
        0.0,
        1.0,
        f64::from(text.origin.0),
        f64::from(text.origin.1),
    ] {
        out.extend_from_slice(&value.to_be_bytes());
    }

    out.extend_from_slice(&50u16.to_be_bytes()); // text version
    out.extend_from_slice(&16u32.to_be_bytes()); // descriptor version
    write_text_descriptor(&mut out, text, bounds);

    out.extend_from_slice(&1u16.to_be_bytes()); // warp version
    out.extend_from_slice(&16u32.to_be_bytes()); // descriptor version
    write_warp_descriptor(&mut out);

    // The warp's own bounds, which Photoshop writes as four integers and, for
    // unwarped text, leaves at zero.
    for _ in 0..4 {
        out.extend_from_slice(&0i32.to_be_bytes());
    }

    out
}

// ---------------------------------------------------------------------------
// The text descriptor
// ---------------------------------------------------------------------------

fn write_text_descriptor(out: &mut Vec<u8>, text: &TextContent, bounds: (f32, f32, f32, f32)) {
    begin_descriptor(out, "TxLr", 8);

    key(out, "Txt ");
    out.extend_from_slice(b"TEXT");
    unicode_string(out, &engine_text(text));

    key(out, "textGridding");
    enumerated(out, "textGridding", "None");

    key(out, "Ornt");
    enumerated(out, "Ornt", if text.vertical { "Vrtc" } else { "Hrzn" });

    key(out, "AntA");
    enumerated(
        out,
        "Annt",
        if text.antialias {
            "antiAliasSharp"
        } else {
            "antiAliasNone"
        },
    );

    // `bounds` is the type's own box and `boundingBox` the space it occupies;
    // for point text with no warp they are the same rectangle.
    key(out, "bounds");
    write_bounds(out, "bounds", bounds);
    key(out, "boundingBox");
    write_bounds(out, "boundingBox", bounds);

    key(out, "TextIndex");
    out.extend_from_slice(b"long");
    out.extend_from_slice(&0i32.to_be_bytes());

    key(out, "EngineData");
    out.extend_from_slice(b"tdta");
    let engine = engine_data(text);
    out.extend_from_slice(&(engine.len() as u32).to_be_bytes());
    out.extend_from_slice(&engine);
}

fn write_bounds(out: &mut Vec<u8>, class: &str, bounds: (f32, f32, f32, f32)) {
    out.extend_from_slice(b"Objc");
    begin_descriptor(out, class, 4);
    for (name, value) in [
        ("Left", bounds.0),
        ("Top ", bounds.1),
        ("Rght", bounds.2),
        ("Btom", bounds.3),
    ] {
        key(out, name);
        out.extend_from_slice(b"UntF");
        out.extend_from_slice(b"#Pnt");
        out.extend_from_slice(&f64::from(value).to_be_bytes());
    }
}

/// The warp descriptor, saying there is no warp — which still has to be said.
fn write_warp_descriptor(out: &mut Vec<u8>) {
    begin_descriptor(out, "warp", 5);

    key(out, "warpStyle");
    enumerated(out, "warpStyle", "warpNone");

    for name in ["warpValue", "warpPerspective", "warpPerspectiveOther"] {
        key(out, name);
        out.extend_from_slice(b"doub");
        out.extend_from_slice(&0.0f64.to_be_bytes());
    }

    key(out, "warpRotate");
    enumerated(out, "Ornt", "Hrzn");
}

/// A descriptor's opening: its own (always empty) name, its class, and how many
/// items follow.
fn begin_descriptor(out: &mut Vec<u8>, class: &str, items: u32) {
    unicode_string(out, "");
    key(out, class);
    out.extend_from_slice(&items.to_be_bytes());
}

/// A key or class code: four-character codes are written with a zero length,
/// anything longer with its length in front.
fn key(out: &mut Vec<u8>, name: &str) {
    if name.len() == 4 {
        out.extend_from_slice(&0u32.to_be_bytes());
    } else {
        out.extend_from_slice(&(name.len() as u32).to_be_bytes());
    }
    out.extend_from_slice(name.as_bytes());
}

fn enumerated(out: &mut Vec<u8>, kind: &str, value: &str) {
    out.extend_from_slice(b"enum");
    key(out, kind);
    key(out, value);
}

/// A UTF-16BE string, its length counted in characters and including the
/// trailing null.
fn unicode_string(out: &mut Vec<u8>, text: &str) {
    let units: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    out.extend_from_slice(&(units.len() as u32).to_be_bytes());
    for unit in units {
        out.extend_from_slice(&unit.to_be_bytes());
    }
}

// ---------------------------------------------------------------------------
// EngineData
// ---------------------------------------------------------------------------

/// The text as the engine wants it: line breaks are carriage returns.
///
/// Photoshop ends a line with `\r` and nothing else; a `\n` left in the string
/// shows up as a missing glyph box rather than as a new line.
fn engine_text(text: &TextContent) -> String {
    text.text().replace("\r\n", "\r").replace('\n', "\r")
}

/// The EngineData text, which always ends with a trailing `\r`.
///
/// Photoshop appends a paragraph-closing carriage return to every text block's
/// EngineData `/Text` value. The run length arrays must account for this extra
/// character. The descriptor's `Txt ` field does *not* include it — it carries
/// the raw text, null-terminated — so the two representations of the string
/// differ by exactly this trailing `\r`.
fn engine_text_with_cr(text: &TextContent) -> String {
    let mut body = engine_text(text);
    if !body.ends_with('\r') {
        body.push('\r');
    }
    body
}

/// Assembles the EngineData dump line by line as raw bytes, because string
/// values are binary UTF-16 inside parentheses — not printable text.
struct Dump {
    out: Vec<u8>,
}

impl Dump {
    fn new() -> Self {
        Self { out: vec![b'\n', b'\n'] }
    }

    fn line(&mut self, depth: usize, text: &str) {
        for _ in 0..depth {
            self.out.push(b'\t');
        }
        self.out.extend_from_slice(text.as_bytes());
        self.out.push(b'\n');
    }

    fn open(&mut self, depth: usize, name: &str) {
        self.line(depth, name);
        self.line(depth, "<<");
    }

    fn close(&mut self, depth: usize) {
        self.line(depth, ">>");
    }
}

fn engine_data(text: &TextContent) -> Vec<u8> {
    let body = engine_text_with_cr(text);
    let fonts = font_set(text);

    let mut d = Dump::new();
    d.line(0, "<<");

    d.open(1, "/EngineDict");
    d.open(2, "/Editor");
    engine_string_line(&mut d, 3, "/Text", &body);
    d.close(2);

    write_paragraph_run(&mut d, text, &body);
    write_style_run(&mut d, text, &fonts);
    write_grid_info(&mut d);

    // 0 is "none"; 1 is Sharp, which is the setting our own antialiasing means.
    d.line(2, &format!("/AntiAlias {}", if text.antialias { 1 } else { 0 }));
    d.line(2, "/UseFractionalGlyphWidths true");
    d.close(1);

    // -- the resources those runs point at -----------------------------------
    // Written twice under two names, as Photoshop does: `ResourceDict` is what
    // this layer uses, `DocumentResources` what the document offers. Keeping
    // them identical is correct for a file whose only text is this layer.
    for name in ["/ResourceDict", "/DocumentResources"] {
        d.open(1, name);
        write_resources(&mut d, text, &fonts);
        d.close(1);
    }

    d.close(0);

    // The EngineData is a `tdta` item whose length is written into the
    // descriptor. An even length keeps the TySh block aligned without
    // needing a padding byte that the parser would see as leftover data.
    if d.out.len() % 2 == 1 {
        d.out.push(b'\n');
    }
    d.out
}

fn write_paragraph_run(d: &mut Dump, text: &TextContent, body: &str) {
    // Justification is a paragraph property, so alignment lives here rather
    // than with the character runs.
    let justification = match text.align {
        TextAlign::Left => 0,
        TextAlign::Right => 1,
        TextAlign::Center => 2,
    };

    d.open(2, "/ParagraphRun");
    d.open(3, "/DefaultRunData");
    write_paragraph_sheet(d, 4, justification);
    d.close(3);

    let lengths = paragraph_lengths(body);
    d.line(3, "/RunArray [");
    for _ in &lengths {
        d.line(4, "<<");
        write_paragraph_sheet(d, 5, justification);
        d.line(4, ">>");
    }
    d.line(3, "]");
    d.line(3, &format!("/RunLengthArray [ {} ]", join_numbers(&lengths)));
    d.line(3, "/IsJoinable 1");
    d.close(2);
}

fn write_paragraph_sheet(d: &mut Dump, depth: usize, justification: u8) {
    d.open(depth, "/ParagraphSheet");
    d.line(depth + 1, "/DefaultStyleSheet 0");
    d.open(depth + 1, "/Properties");
    d.line(depth + 2, &format!("/Justification {justification}"));
    d.line(depth + 2, "/FirstLineIndent 0.0");
    d.line(depth + 2, "/StartIndent 0.0");
    d.line(depth + 2, "/EndIndent 0.0");
    d.line(depth + 2, "/SpaceBefore 0.0");
    d.line(depth + 2, "/SpaceAfter 0.0");
    d.line(depth + 2, "/AutoHyphenate true");
    d.line(depth + 2, "/HyphenatedWordSize 6");
    d.line(depth + 2, "/PreHyphen 2");
    d.line(depth + 2, "/PostHyphen 3");
    d.line(depth + 2, "/ConsecutiveHyphens 8");
    d.line(depth + 2, "/Zone 36.0");
    d.line(depth + 2, "/WordSpacing [ .8 1.0 1.33 ]");
    d.line(depth + 2, "/LetterSpacing [ 0.0 0.0 0.0 ]");
    d.line(depth + 2, "/GlyphSpacing [ 1.0 1.0 1.0 ]");
    d.line(depth + 2, "/AutoLeading 1.2");
    d.line(depth + 2, "/LeadingType 0");
    d.line(depth + 2, "/Hanging false");
    d.line(depth + 2, "/Burasagari false");
    d.line(depth + 2, "/KinsokuOrder 0");
    d.line(depth + 2, "/EveryLineComposer false");
    d.close(depth + 1);
    d.close(depth);
    d.open(depth, "/Adjustments");
    d.line(depth + 1, "/Axis [ 1.0 0.0 1.0 ]");
    d.line(depth + 1, "/XY [ 0.0 0.0 ]");
    d.close(depth);
}

fn write_style_run(d: &mut Dump, text: &TextContent, fonts: &[String]) {
    d.open(2, "/StyleRun");
    d.open(3, "/DefaultRunData");
    write_style_sheet(d, 4, text.runs.first(), fonts);
    d.close(3);

    d.line(3, "/RunArray [");
    for run in &text.runs {
        d.line(4, "<<");
        write_style_sheet(d, 5, Some(run), fonts);
        d.line(4, ">>");
    }
    d.line(3, "]");

    // One length per run, in characters. These have to add up to the
    // EngineData text length (which includes the trailing \r that
    // `engine_text_with_cr` appends). The last run absorbs that extra
    // character, matching Photoshop's own convention.
    let mut lengths: Vec<usize> = text.runs.iter().map(|run| utf16_len(&run.text)).collect();
    if let Some(last) = lengths.last_mut() {
        *last += 1; // the trailing \r
    }
    d.line(3, &format!("/RunLengthArray [ {} ]", join_numbers(&lengths)));
    d.line(3, "/IsJoinable 2");
    d.close(2);
}

fn write_style_sheet(d: &mut Dump, depth: usize, run: Option<&TextRun>, fonts: &[String]) {
    let default = TextRun {
        text: String::new(),
        family: String::new(),
        style: "Regular".to_string(),
        size: 12.0,
        color: Rgba8::BLACK,
    };
    let run = run.unwrap_or(&default);
    let font = font_index(fonts, run);

    d.open(depth, "/StyleSheet");
    d.open(depth + 1, "/StyleSheetData");
    d.line(depth + 2, &format!("/Font {font}"));
    d.line(depth + 2, &format!("/FontSize {}", number(run.size)));
    d.line(depth + 2, "/AutoLeading true");
    d.line(depth + 2, &format!("/Leading {}", number(run.size * 1.2)));
    d.line(depth + 2, "/HorizontalScale 1.0");
    d.line(depth + 2, "/VerticalScale 1.0");
    d.line(depth + 2, "/Tracking 0");
    d.line(depth + 2, "/BaselineShift 0.0");
    d.line(depth + 2, "/AutoKerning true");
    d.line(depth + 2, "/Kerning 0");
    d.line(depth + 2, "/FontCaps 0");
    d.line(depth + 2, "/FontBaseline 0");
    d.line(depth + 2, "/Underline false");
    d.line(depth + 2, "/Strikethrough false");
    d.line(depth + 2, "/Ligatures true");
    d.line(depth + 2, "/StyleRunAlignment 2");
    d.line(depth + 2, "/NoBreak false");
    d.open(depth + 2, "/FillColor");
    // Type 1 is RGB, and the values run alpha first as fractions.
    d.line(depth + 3, "/Type 1");
    d.line(depth + 3, &format!("/Values [ {} ]", color_values(run.color)));
    d.close(depth + 2);
    d.line(depth + 2, "/FillFlag true");
    d.line(depth + 2, "/StrokeFlag false");
    d.close(depth + 1);
    d.close(depth);
}

fn write_grid_info(d: &mut Dump) {
    d.open(2, "/GridInfo");
    d.line(3, "/GridIsOn false");
    d.line(3, "/ShowGrid false");
    d.line(3, "/GridSize 18.0");
    d.line(3, "/GridLeading 22.0");
    d.open(3, "/GridColor");
    d.line(4, "/Type 1");
    d.line(4, "/Values [ 0.0 0.0 0.0 1.0 ]");
    d.close(3);
    d.open(3, "/GridLeadingFillColor");
    d.line(4, "/Type 1");
    d.line(4, "/Values [ 0.0 0.0 0.0 1.0 ]");
    d.close(3);
    d.line(3, "/AlignLineHeightToGridFlags false");
    d.close(2);
}

fn write_resources(d: &mut Dump, text: &TextContent, fonts: &[String]) {
    // Empty rather than absent: these govern East Asian line breaking, which we
    // do not offer, but the keys are part of the shape Photoshop reads.
    d.line(2, "/KinsokuSet [");
    d.line(2, "]");
    d.line(2, "/MojiKumiSet [");
    d.line(2, "]");
    d.line(2, "/TheNormalStyleSheet 0");
    d.line(2, "/TheNormalParagraphSheet 0");

    let justification = match text.align {
        TextAlign::Left => 0,
        TextAlign::Right => 1,
        TextAlign::Center => 2,
    };
    d.line(2, "/ParagraphSheetSet [");
    d.line(3, "<<");
    engine_string_line(d, 4, "/Name", "Normal RGB");
    d.line(4, "/DefaultStyleSheet 0");
    write_paragraph_sheet(d, 4, justification);
    d.line(3, ">>");
    d.line(2, "]");

    d.line(2, "/StyleSheetSet [");
    d.line(3, "<<");
    engine_string_line(d, 4, "/Name", "Normal RGB");
    write_style_sheet(d, 4, text.runs.first(), fonts);
    d.line(3, ">>");
    d.line(2, "]");

    d.line(2, "/FontSet [");
    for name in fonts {
        d.line(3, "<<");
        engine_string_line(d, 4, "/Name", name);
        d.line(4, "/Script 0");
        d.line(4, "/FontType 1");
        d.line(4, "/Synthetic 0");
        d.line(3, ">>");
    }
    d.line(2, "]");

    d.line(2, "/SuperscriptSize .583");
    d.line(2, "/SuperscriptPosition .333");
    d.line(2, "/SubscriptSize .583");
    d.line(2, "/SubscriptPosition .333");
    d.line(2, "/SmallCapSize .7");
}

// ---------------------------------------------------------------------------
// Fonts, strings and numbers
// ---------------------------------------------------------------------------

/// The fonts the runs use, in the order they first appear.
///
/// Every run's `/Font` is an index into this, so it has to be built the same way
/// twice — once here and once by [`font_index`] — which is why both go through
/// [`postscript_name`].
fn font_set(text: &TextContent) -> Vec<String> {
    let mut fonts: Vec<String> = Vec::new();
    for run in &text.runs {
        let name = postscript_name(&run.family, &run.style);
        if !fonts.contains(&name) {
            fonts.push(name);
        }
    }
    if fonts.is_empty() {
        fonts.push("Helvetica".to_string());
    }
    fonts
}

fn font_index(fonts: &[String], run: &TextRun) -> usize {
    let name = postscript_name(&run.family, &run.style);
    fonts.iter().position(|f| *f == name).unwrap_or(0)
}

/// A family and style written as one PostScript name: `Georgia` + `Bold` gives
/// `Georgia-Bold`.
///
/// The inverse of the reader's `split_postscript_name`, and a heuristic in the
/// same way. Photoshop matches fonts by this name, so a family whose real
/// PostScript name is not its display name run together — `Times New Roman`
/// is `TimesNewRomanPSMT` — will be substituted on opening. That costs the
/// reopened text its typeface, not its content, and there is no better answer
/// available without shipping a font-name database.
fn postscript_name(family: &str, style: &str) -> String {
    let family: String = family.chars().filter(|c| !c.is_whitespace()).collect();
    let family = if family.is_empty() {
        "Helvetica".to_string()
    } else {
        family
    };

    let style: String = style.chars().filter(|c| !c.is_whitespace()).collect();
    if style.is_empty() || style.eq_ignore_ascii_case("regular") {
        family
    } else {
        format!("{family}-{style}")
    }
}

/// A string as EngineData writes one: UTF-16BE inside parentheses, behind a
/// byte-order mark. Writes raw binary bytes, which is what Photoshop does and
/// what the reader expects (it strips `\0` and BOM characters).
fn engine_string_bytes(text: &str) -> Vec<u8> {
    let mut out = vec![b'('];
    out.extend_from_slice(&[0xFE, 0xFF]); // BOM
    for unit in text.encode_utf16() {
        for byte in unit.to_be_bytes() {
            match byte {
                b'(' | b')' | b'\\' => {
                    out.push(b'\\');
                    out.push(byte);
                }
                _ => out.push(byte),
            }
        }
    }
    out.push(b')');
    out
}

/// Write a line like `/Text (...)` where the value is a binary UTF-16 string.
fn engine_string_line(d: &mut Dump, depth: usize, key: &str, text: &str) {
    for _ in 0..depth {
        d.out.push(b'\t');
    }
    d.out.extend_from_slice(key.as_bytes());
    d.out.push(b' ');
    d.out.extend_from_slice(&engine_string_bytes(text));
    d.out.push(b'\n');
}

/// How many characters a paragraph is, one per paragraph in `body`.
///
/// A paragraph runs up to and including its carriage return, so the lengths add
/// up to the whole string — which is the property Photoshop checks.
fn paragraph_lengths(body: &str) -> Vec<usize> {
    let mut lengths = Vec::new();
    let mut current = 0usize;
    for c in body.chars() {
        current += c.len_utf16();
        if c == '\r' {
            lengths.push(current);
            current = 0;
        }
    }
    if current > 0 || lengths.is_empty() {
        lengths.push(current);
    }
    lengths
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

fn join_numbers(values: &[usize]) -> String {
    values
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

/// A number with a decimal point on it, which is how the format writes reals —
/// an integer without one is a different type to the engine.
fn number(value: f32) -> String {
    let text = format!("{value:.2}");
    let trimmed = text.trim_end_matches('0');
    if trimmed.ends_with('.') {
        format!("{trimmed}0")
    } else {
        trimmed.to_string()
    }
}

/// A colour as the engine writes one: alpha first, then red, green and blue, all
/// as fractions.
fn color_values(color: Rgba8) -> String {
    let fraction = |v: u8| number(f32::from(v) / 255.0);
    format!(
        "{} {} {} {}",
        fraction(color.a),
        fraction(color.r),
        fraction(color.g),
        fraction(color.b)
    )
}

#[cfg(test)]
mod tests {
    use super::super::text::parse_type_tool;
    use super::*;

    fn content(runs: Vec<TextRun>) -> TextContent {
        TextContent {
            runs,
            align: TextAlign::Left,
            antialias: true,
            vertical: false,
            origin: (40.0, 80.0),
        }
    }

    fn run(text: &str, family: &str, style: &str, size: f32, color: Rgba8) -> TextRun {
        TextRun {
            text: text.to_string(),
            family: family.to_string(),
            style: style.to_string(),
            size,
            color,
        }
    }

    fn one_run(text: &str) -> TextContent {
        content(vec![run(text, "Georgia", "Bold", 72.0, Rgba8::new(51, 102, 153, 255))])
    }

    /// The whole point: what we write, we can read back as text.
    #[test]
    fn a_written_block_reads_back_as_text() {
        let block = type_tool_block(&one_run("TESTTEST"), (0.0, -20.0, 200.0, 10.0));
        let parsed = parse_type_tool(&block).expect("not recognized as a type layer");
        assert_eq!(parsed.text, "TESTTEST");
    }

    #[test]
    fn the_settings_survive_the_round_trip() {
        let block = type_tool_block(&one_run("Hello"), (0.0, 0.0, 100.0, 20.0));
        let parsed = parse_type_tool(&block).unwrap();
        assert_eq!(parsed.family, "Georgia");
        assert_eq!(parsed.style, "Bold");
        assert_eq!(parsed.size, 72.0);
        assert_eq!(parsed.color, Rgba8::new(51, 102, 153, 255));
    }

    #[test]
    fn alignment_survives_the_round_trip() {
        for align in [TextAlign::Left, TextAlign::Center, TextAlign::Right] {
            let mut text = one_run("Hello");
            text.align = align;
            let block = type_tool_block(&text, (0.0, 0.0, 10.0, 10.0));
            assert_eq!(parse_type_tool(&block).unwrap().align, align);
        }
    }

    #[test]
    fn a_font_with_no_style_is_named_without_a_hyphen() {
        // Round-tripping through the reader's split: no hyphen means Regular.
        let text = content(vec![run("Hi", "Impact", "Regular", 24.0, Rgba8::BLACK)]);
        let parsed = parse_type_tool(&type_tool_block(&text, (0.0, 0.0, 1.0, 1.0))).unwrap();
        assert_eq!(parsed.family, "Impact");
        assert_eq!(parsed.style, "Regular");
    }

    #[test]
    fn every_run_lands_in_the_font_set_once() {
        let text = content(vec![
            run("A", "Georgia", "Bold", 12.0, Rgba8::BLACK),
            run("B", "Impact", "Regular", 12.0, Rgba8::BLACK),
            run("C", "Georgia", "Bold", 48.0, Rgba8::BLACK),
        ]);
        let fonts = font_set(&text);
        assert_eq!(fonts, vec!["Georgia-Bold".to_string(), "Impact".to_string()]);
        // The third run shares the first's font, and so its index.
        assert_eq!(font_index(&fonts, &text.runs[2]), 0);
    }

    /// Photoshop walks the string and the run arrays together; if they disagree
    /// on the length it rejects the block and the layer opens as pixels.
    #[test]
    fn the_run_lengths_add_up_to_the_text() {
        let text = content(vec![
            run("one\ntwo", "Georgia", "Bold", 12.0, Rgba8::BLACK),
            run(" three", "Impact", "Regular", 12.0, Rgba8::BLACK),
        ]);
        // The EngineData body includes a trailing \r that the last style run
        // absorbs. Style lengths + 1 (for the \r) must equal the body length,
        // and paragraph lengths must also add up.
        let body = engine_text_with_cr(&text);
        let mut style_lengths: Vec<usize> = text.runs.iter().map(|r| utf16_len(&r.text)).collect();
        if let Some(last) = style_lengths.last_mut() {
            *last += 1;
        }
        let total_styles: usize = style_lengths.iter().sum();
        assert_eq!(total_styles, utf16_len(&body));
        assert_eq!(paragraph_lengths(&body).iter().sum::<usize>(), utf16_len(&body));
    }

    #[test]
    fn line_breaks_are_written_as_carriage_returns() {
        let text = content(vec![run("a\nb\r\nc", "Georgia", "Bold", 12.0, Rgba8::BLACK)]);
        assert_eq!(engine_text(&text), "a\rb\rc");
        assert_eq!(paragraph_lengths("a\rb\rc"), vec![2, 2, 1]);
    }

    #[test]
    fn a_single_paragraph_still_gets_one_length() {
        assert_eq!(paragraph_lengths("plain"), vec![5]);
        assert_eq!(paragraph_lengths(""), vec![0]);
        // A trailing return closes its paragraph and opens no other.
        assert_eq!(paragraph_lengths("end\r"), vec![4]);
    }

    #[test]
    fn multiline_text_survives_the_round_trip() {
        let text = content(vec![run("two\nlines", "Georgia", "Bold", 12.0, Rgba8::BLACK)]);
        let parsed = parse_type_tool(&type_tool_block(&text, (0.0, 0.0, 1.0, 1.0))).unwrap();
        assert_eq!(parsed.text, "two\rlines");
    }

    #[test]
    fn strings_are_written_as_binary_utf16_behind_a_byte_order_mark() {
        let bytes = engine_string_bytes("AB");
        // BOM (0xFE 0xFF), then 0x00 'A' 0x00 'B', in parens.
        assert_eq!(bytes, b"(\xfe\xff\x00A\x00B)");
        // Parentheses are escaped so they do not close the string early.
        let bytes = engine_string_bytes("(");
        assert_eq!(bytes, b"(\xfe\xff\x00\\()");
    }

    #[test]
    fn numbers_are_written_as_reals() {
        assert_eq!(number(72.0), "72.0");
        assert_eq!(number(12.5), "12.5");
        assert_eq!(number(0.0), "0.0");
    }

    #[test]
    fn a_run_with_no_font_named_still_writes_a_usable_one() {
        let text = content(vec![run("Hi", "", "", 12.0, Rgba8::BLACK)]);
        assert_eq!(font_set(&text), vec!["Helvetica".to_string()]);
        assert!(parse_type_tool(&type_tool_block(&text, (0.0, 0.0, 1.0, 1.0))).is_some());
    }

    #[test]
    fn empty_text_is_still_structurally_valid() {
        // The reader declines it — text with nothing in it is not a type layer
        // — but writing it must not panic on the way.
        let text = content(vec![run("", "Georgia", "Regular", 12.0, Rgba8::BLACK)]);
        let block = type_tool_block(&text, (0.0, 0.0, 0.0, 0.0));
        assert!(!block.is_empty());
    }
}

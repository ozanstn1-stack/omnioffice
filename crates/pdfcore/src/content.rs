//! In-place editing of text runs inside a page content stream.
//!
//! This is the first half of the V3.6 "content-stream objects" work and it is
//! deliberately conservative: only the text operand of a text-showing
//! operator is touched (`Tj`, `'`, `"` and single-string `TJ`), nothing else
//! in the stream moves, fonts stay the same and the edit is written as an
//! incremental update, so the original bytes - and any signature over them -
//! stay untouched. Vector graphics and multi-string `TJ` runs are reported as
//! read-only with the reason instead of being guessed at.
//!
//! Positions and widths are approximations: a real advance needs the font's
//! glyph widths, which simple-font editing does not require. The values are
//! good enough to order and highlight runs in the UI, and the docs say so.

use crate::error::{PdfError, PdfResult};
use lopdf::content::{Content, Operation};
use lopdf::Encoding;
use lopdf::{Dictionary, Document, Object, ObjectId, StringFormat};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// The largest a font's `/ToUnicode` CMap may inflate to. A bigger one is
/// treated as hostile (a decompression bomb) and the font is decoded without
/// it.
const MAX_CMAP_BYTES: usize = 8 * 1024 * 1024;

#[cfg(test)]
thread_local! {
    /// How many font encodings were built on this thread, for the tests.
    static ENCODINGS_BUILT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The text encodings of the fonts of a document, each built once.
///
/// Building an encoding for a Type0 font inflates and parses its whole
/// `/ToUnicode` stream, which is far too slow to repeat for every string. The
/// cache is keyed by the font dictionary, so a font shared by many pages is
/// parsed once for all of them.
pub(crate) struct FontCache<'a> {
    doc: &'a Document,
    /// Font resources of the page being read, by resource name.
    page_fonts: BTreeMap<Vec<u8>, &'a Dictionary>,
    /// Built encodings by font dictionary address; `None` when unusable.
    encodings: HashMap<usize, Option<Encoding<'a>>>,
    warnings: Vec<String>,
}

impl<'a> FontCache<'a> {
    pub(crate) fn new(doc: &'a Document) -> Self {
        Self { doc, page_fonts: BTreeMap::new(), encodings: HashMap::new(), warnings: Vec::new() }
    }

    /// Makes the fonts of `page_id` the ones strings are decoded with.
    fn select_page(&mut self, page_id: ObjectId) {
        self.page_fonts = self.doc.get_page_fonts(page_id).unwrap_or_default();
    }

    /// The encoding of the font resource `name` of the selected page.
    fn encoding(&mut self, name: Option<&[u8]>) -> Option<&Encoding<'a>> {
        let name = name?;
        let dictionary: &'a Dictionary = self.page_fonts.get(name).copied()?;
        let key = dictionary as *const Dictionary as usize;
        let (doc, warnings) = (self.doc, &mut self.warnings);
        self.encodings
            .entry(key)
            .or_insert_with(|| {
                #[cfg(test)]
                ENCODINGS_BUILT.with(|count| count.set(count.get() + 1));
                match dictionary.get_font_encoding_with_limit(doc, MAX_CMAP_BYTES) {
                    Ok(encoding) => Some(encoding),
                    Err(lopdf::Error::Decompress(lopdf::DecompressError::MemoryLimitExceeded { .. })) => {
                        warnings.push(format!(
                            "The ToUnicode map of font {} is larger than {} MB and was ignored.",
                            String::from_utf8_lossy(name),
                            MAX_CMAP_BYTES / (1024 * 1024)
                        ));
                        None
                    }
                    Err(_) => None,
                }
            })
            .as_ref()
    }

    /// Takes the warnings collected so far.
    fn take_warnings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.warnings)
    }
}

/// One text-showing operation on a page, with what the editor can do with it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextRunInfo {
    /// 1-based page number.
    pub page: u32,
    /// Stable index of the run among the page's runs (stream order).
    pub index: u32,
    pub text: String,
    /// Font resource name (`Tf`), for display.
    pub font: Option<String>,
    pub font_size_pt: f64,
    /// Approximate text-space origin of the run (page points).
    pub x: f64,
    pub y: f64,
    /// Approximate advance width of the run (page points).
    pub width_pt: f64,
    pub render_mode: i64,
    /// False when the run cannot be edited in place (see `note`).
    pub editable: bool,
    pub note: Option<String>,
}

/// One requested text replacement.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextRunEdit {
    pub page: u32,
    pub index: u32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEditReport {
    pub edited: u32,
    pub warnings: Vec<String>,
}

/// The location of the string inside an operation: a direct operand (`Tj`,
/// `'`, `"`) or an element of a `TJ` array.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OperandPath {
    Operand(usize),
    ArrayElement(usize, usize),
}

#[derive(Debug, Clone)]
struct RunSite {
    operation: usize,
    path: OperandPath,
    info: TextRunInfo,
}

#[derive(Debug, Clone)]
struct TextState {
    font: Option<Vec<u8>>,
    size: f64,
    leading: f64,
    char_space: f64,
    word_space: f64,
    h_scale: f64,
    rise: f64,
    render_mode: i64,
}

impl Default for TextState {
    fn default() -> Self {
        Self {
            font: None,
            size: 12.0,
            leading: 0.0,
            char_space: 0.0,
            word_space: 0.0,
            h_scale: 1.0,
            rise: 0.0,
            render_mode: 0,
        }
    }
}

fn number(operand: Option<&Object>) -> Option<f64> {
    match operand {
        Some(Object::Integer(value)) => Some(*value as f64),
        Some(Object::Real(value)) => Some(f64::from(*value)),
        _ => None,
    }
}

/// Approximate advance of a text string in page points.
fn estimate_width(text: &str, state: &TextState) -> f64 {
    let base = state.size * 0.5;
    text.chars()
        .map(|character| {
            let mut advance = base + state.char_space;
            if character == ' ' {
                advance += state.word_space;
            }
            advance * state.h_scale
        })
        .sum()
}

/// Decodes a PDF text string the way the reader does: the font encoding when
/// it works, a UTF-16 BOM second, raw bytes last.
fn decode_string(fonts: &mut FontCache, font: Option<&[u8]>, raw: &[u8]) -> String {
    if let Some(encoding) = fonts.encoding(font) {
        if let Ok(text) = encoding.bytes_to_string(raw) {
            if !text.contains('\u{fffd}') {
                return text;
            }
        }
    }
    if raw.len() >= 2 && raw[0] == 0xFE && raw[1] == 0xFF {
        let units: Vec<u16> = raw[2..].as_chunks::<2>().0.iter().map(|pair| u16::from_be_bytes(*pair)).collect();
        return String::from_utf16_lossy(&units);
    }
    raw.iter().map(|byte| *byte as char).collect()
}

/// Encodes new text with the run's font, or returns `None` when the encoding
/// cannot represent it exactly. The round trip is checked, so a lossy encoder
/// (replacement characters, wrong CIDs) never reaches the file.
fn encode_string(fonts: &mut FontCache, font: Option<&[u8]>, text: &str) -> Option<Vec<u8>> {
    if let Some(bytes) = fonts.encoding(font).map(|encoding| encoding.string_to_bytes(text)) {
        if decode_string(fonts, font, &bytes) == text {
            return Some(bytes);
        }
    }
    if text.chars().all(|character| (character as u32) < 0x80) {
        let bytes = text.as_bytes().to_vec();
        if decode_string(fonts, font, &bytes) == text {
            return Some(bytes);
        }
    }
    None
}

/// Walks one page's decoded content, tracking the graphics/text state, and
/// returns the content plus every text run with its edit location.
fn analyze_page(
    doc: &Document,
    fonts: &mut FontCache,
    page: u32,
    page_id: ObjectId,
) -> PdfResult<(Content<Vec<Operation>>, Vec<RunSite>)> {
    fonts.select_page(page_id);
    let content = doc
        .get_and_decode_page_content(page_id)
        .map_err(|error| PdfError::ProcessingFailed(format!("content stream: {error}")))?;
    let mut sites = Vec::new();
    let mut state = TextState::default();
    let mut stack: Vec<TextState> = Vec::new();
    let mut x = 0.0f64;
    let mut y = 0.0f64;
    // Start of the current line: `Td`, `T*`, `'` and `"` move relative to it,
    // not to the end of the last run shown.
    let mut line = (0.0f64, 0.0f64);
    for (operation_index, operation) in content.operations.iter().enumerate() {
        let operator = operation.operator.as_str();
        match operator {
            "q" => stack.push(state.clone()),
            "Q" => {
                if let Some(saved) = stack.pop() {
                    state = saved;
                }
            }
            "BT" => {
                x = 0.0;
                y = 0.0;
                line = (0.0, 0.0);
            }
            "Tf" => {
                if let Some(Object::Name(name)) = operation.operands.first() {
                    state.font = Some(name.clone());
                }
                if let Some(size) = number(operation.operands.get(1)) {
                    state.size = size;
                }
            }
            "TL" => {
                if let Some(leading) = number(operation.operands.first()) {
                    state.leading = leading;
                }
            }
            "Tc" => {
                if let Some(value) = number(operation.operands.first()) {
                    state.char_space = value;
                }
            }
            "Tw" => {
                if let Some(value) = number(operation.operands.first()) {
                    state.word_space = value;
                }
            }
            "Tz" => {
                if let Some(value) = number(operation.operands.first()) {
                    state.h_scale = value / 100.0;
                }
            }
            "Ts" => {
                if let Some(value) = number(operation.operands.first()) {
                    state.rise = value;
                }
            }
            "Tr" => {
                if let Some(value) = number(operation.operands.first()) {
                    state.render_mode = value as i64;
                }
            }
            "Tm" => {
                if let (Some(tx), Some(ty)) = (number(operation.operands.get(4)), number(operation.operands.get(5))) {
                    x = tx;
                    y = ty;
                    line = (tx, ty);
                }
            }
            "Td" | "TD" => {
                if let (Some(tx), Some(ty)) = (number(operation.operands.first()), number(operation.operands.get(1))) {
                    line = (line.0 + tx, line.1 + ty);
                    (x, y) = line;
                    if operator == "TD" {
                        state.leading = -ty;
                    }
                }
            }
            "T*" => {
                line.1 -= state.leading;
                (x, y) = line;
            }
            "Tj" | "'" | "\"" => {
                if operator == "'" || operator == "\"" {
                    line.1 -= state.leading;
                    (x, y) = line;
                }
                let Some((string_index, object)) =
                    operation.operands.iter().enumerate().find(|(_, operand)| matches!(operand, Object::String(_, _)))
                else {
                    continue;
                };
                let Object::String(raw, _) = object else { continue };
                let text = decode_string(fonts, state.font.as_deref(), raw);
                let width = estimate_width(&text, &state);
                sites.push(RunSite {
                    operation: operation_index,
                    path: OperandPath::Operand(string_index),
                    info: TextRunInfo {
                        page,
                        index: sites.len() as u32,
                        text: text.clone(),
                        font: state.font.as_ref().map(|name| String::from_utf8_lossy(name).to_string()),
                        font_size_pt: state.size,
                        x,
                        y: y + state.rise,
                        width_pt: width,
                        render_mode: state.render_mode,
                        editable: true,
                        note: None,
                    },
                });
                x += width;
            }
            "TJ" => {
                let Some(Object::Array(items)) = operation.operands.first() else { continue };
                let strings: Vec<usize> = items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| matches!(item, Object::String(_, _)))
                    .map(|(index, _)| index)
                    .collect();
                if strings.is_empty() {
                    continue;
                }
                // Producers such as TeX write word spaces as kerning (`-333`)
                // between strings rather than as space characters; read a
                // shift of more than a fifth of an em as a space.
                let mut text = String::new();
                let mut gap = false;
                for item in items {
                    match item {
                        Object::String(raw, _) => {
                            let piece = decode_string(fonts, state.font.as_deref(), raw);
                            if gap && !text.is_empty() && !text.ends_with(' ') && !piece.starts_with(' ') {
                                text.push(' ');
                            }
                            text.push_str(&piece);
                            gap = false;
                        }
                        _ => gap |= number(Some(item)).is_some_and(|value| value < -200.0),
                    }
                }
                let width = estimate_width(&text, &state);
                let editable = strings.len() == 1;
                sites.push(RunSite {
                    operation: operation_index,
                    path: OperandPath::ArrayElement(0, strings[0]),
                    info: TextRunInfo {
                        page,
                        index: sites.len() as u32,
                        text,
                        font: state.font.as_ref().map(|name| String::from_utf8_lossy(name).to_string()),
                        font_size_pt: state.size,
                        x,
                        y: y + state.rise,
                        width_pt: width,
                        render_mode: state.render_mode,
                        editable,
                        note: if editable {
                            None
                        } else {
                            Some("The run is split across several strings in a TJ array; editing it in place is not supported.".into())
                        },
                    },
                });
                // Advance through the array: strings add, kerning numbers pull.
                for item in items {
                    match item {
                        Object::String(raw, _) => {
                            let piece = decode_string(fonts, state.font.as_deref(), raw);
                            x += estimate_width(&piece, &state);
                        }
                        _ => {
                            if let Some(value) = number(Some(item)) {
                                x -= (value / 1000.0) * state.size * state.h_scale;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok((content, sites))
}

/// The text runs of one page of an already loaded document.
pub(crate) fn page_text_runs(
    doc: &Document,
    fonts: &mut FontCache,
    page: u32,
    page_id: ObjectId,
) -> PdfResult<Vec<TextRunInfo>> {
    let (_, sites) = analyze_page(doc, fonts, page, page_id)?;
    Ok(sites.into_iter().map(|site| site.info).collect())
}

/// Lists the text runs of every page.
pub fn list_text_runs_in_file(path: &Path, password: Option<&str>) -> PdfResult<Vec<TextRunInfo>> {
    let doc = crate::docutil::load_document(path, password)?;
    let mut runs = Vec::new();
    let mut fonts = FontCache::new(&doc);
    for (page, page_id) in doc.get_pages() {
        let (_, sites) = analyze_page(&doc, &mut fonts, page, page_id)?;
        runs.extend(sites.into_iter().map(|site| site.info));
    }
    Ok(runs)
}

/// Applies text replacements and appends the result as a new revision.
///
/// The original bytes are preserved byte for byte (see
/// [`crate::incremental::apply_difference`]), so signatures over the previous
/// revision stay valid and the change is visible as a later revision.
pub fn edit_text_runs(pdf: &[u8], edits: &[TextRunEdit]) -> PdfResult<(Vec<u8>, TextEditReport)> {
    if edits.is_empty() {
        return Err(PdfError::InvalidInput("no text edits were supplied".into()));
    }
    let mut doc = Document::load_mem(pdf).map_err(|error| PdfError::InvalidPdf(error.to_string()))?;
    if doc.trailer.get(b"Encrypt").is_ok() {
        return Err(PdfError::PasswordRequired);
    }
    let pages = doc.get_pages();
    let mut report = TextEditReport { edited: 0, warnings: Vec::new() };

    let mut by_page: std::collections::BTreeMap<u32, Vec<&TextRunEdit>> = std::collections::BTreeMap::new();
    for edit in edits {
        by_page.entry(edit.page).or_default().push(edit);
    }

    for (page, page_edits) in by_page {
        let Some(page_id) = pages.get(&page).copied() else {
            report.warnings.push(format!("Page {page} does not exist."));
            continue;
        };
        let mut fonts = FontCache::new(&doc);
        let (mut content, sites) = analyze_page(&doc, &mut fonts, page, page_id)?;
        let mut changed = false;
        for edit in page_edits {
            let Some(site) = sites.iter().find(|site| site.info.index == edit.index) else {
                report.warnings.push(format!("Text run {} on page {page} no longer exists.", edit.index));
                continue;
            };
            if !site.info.editable {
                report.warnings.push(format!(
                    "Text run {} on page {page} is not editable: {}",
                    edit.index,
                    site.info.note.as_deref().unwrap_or("read-only run")
                ));
                continue;
            }
            if edit.text == site.info.text {
                continue;
            }
            if edit.text.chars().count() > 2000 {
                report
                    .warnings
                    .push(format!("Text run {} on page {page} is longer than the 2000-character limit.", edit.index));
                continue;
            }
            let Some(bytes) = encode_string(&mut fonts, site.info.font.as_deref().map(str::as_bytes), &edit.text)
            else {
                report.warnings.push(format!(
                    "The font of text run {} on page {page} cannot represent the replacement text; the run was left unchanged.",
                    edit.index
                ));
                continue;
            };
            let operation = &mut content.operations[site.operation];
            let format = match site.path {
                OperandPath::Operand(index) => match operation.operands.get(index) {
                    Some(Object::String(_, format)) => *format,
                    _ => StringFormat::Literal,
                },
                OperandPath::ArrayElement(operand_index, element_index) => {
                    match operation.operands.get(operand_index) {
                        Some(Object::Array(items)) => match items.get(element_index) {
                            Some(Object::String(_, format)) => *format,
                            _ => StringFormat::Literal,
                        },
                        _ => StringFormat::Literal,
                    }
                }
            };
            match site.path {
                OperandPath::Operand(index) => {
                    if let Some(slot) = operation.operands.get_mut(index) {
                        *slot = Object::String(bytes, format);
                    } else {
                        continue;
                    }
                }
                OperandPath::ArrayElement(operand_index, element_index) => {
                    let Some(Object::Array(items)) = operation.operands.get_mut(operand_index) else { continue };
                    let Some(slot) = items.get_mut(element_index) else { continue };
                    *slot = Object::String(bytes, format);
                }
            }
            changed = true;
            report.edited += 1;
        }
        report.warnings.extend(fonts.take_warnings());
        if changed {
            let encoded =
                content.encode().map_err(|error| PdfError::ProcessingFailed(format!("encode content: {error}")))?;
            doc.change_page_content(page_id, encoded)
                .map_err(|error| PdfError::ProcessingFailed(format!("write content: {error}")))?;
        }
    }

    if report.edited == 0 {
        let detail = if report.warnings.is_empty() {
            "no text run matched the edit".to_string()
        } else {
            report.warnings.join(" ")
        };
        return Err(PdfError::InvalidInput(format!("No text was changed. {detail}")));
    }

    let output = crate::incremental::apply_difference(pdf, &doc)?;
    Ok((output, report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Dictionary};

    /// A one-page PDF with a simple WinAnsi Helvetica font and the given text
    /// content stream.
    fn sample_with(content: &str) -> Vec<u8> {
        let mut doc = Document::with_version("1.7");
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        });
        crate::docutil::add_resource_entry(&mut doc, page_id, b"Font", "F1", Object::Reference(font_id)).unwrap();
        let stream_id = doc.add_object(lopdf::Stream::new(Dictionary::new(), content.as_bytes().to_vec()));
        doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap().set("Contents", Object::Reference(stream_id));
        let pages_id = doc.add_object(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
        });
        doc.get_object_mut(page_id).unwrap().as_dict_mut().unwrap().set("Parent", Object::Reference(pages_id));
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) });
        doc.trailer.set("Root", Object::Reference(catalog_id));
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    #[test]
    fn lists_text_runs_with_position_and_font() {
        let bytes = sample_with("BT\n/F1 24 Tf\n72 700 Td\n(Hello world) Tj\nET\n");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("runs.pdf");
        std::fs::write(&path, &bytes).unwrap();
        let runs = list_text_runs_in_file(&path, None).expect("list");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "Hello world");
        assert_eq!(runs[0].font.as_deref(), Some("F1"));
        assert_eq!(runs[0].font_size_pt, 24.0);
        assert_eq!((runs[0].x, runs[0].y), (72.0, 700.0));
        assert!(runs[0].editable);
    }

    #[test]
    fn replaces_a_run_as_an_incremental_revision() {
        let bytes = sample_with("BT\n/F1 24 Tf\n72 700 Td\n(Hello world) Tj\nET\n");
        let (output, report) =
            edit_text_runs(&bytes, &[TextRunEdit { page: 1, index: 0, text: "Merhaba dünya".into() }]).expect("edit");
        assert_eq!(report.edited, 1);
        assert!(output.starts_with(&bytes), "the original bytes must be preserved");
        let reloaded = Document::load_mem(&output).expect("reload");
        let text = reloaded.extract_text(&[1]).unwrap_or_default();
        assert!(text.contains("Merhaba dünya"), "extracted: {text:?}");
        assert!(!text.contains("Hello world"));
    }

    #[test]
    fn replays_edits_on_the_current_text() {
        let bytes = sample_with("BT\n/F1 12 Tf\n50 800 Td\n(First) Tj\n0 -20 Td\n(Second) Tj\nET\n");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("two.pdf");
        std::fs::write(&path, &bytes).unwrap();
        let runs = list_text_runs_in_file(&path, None).expect("list");
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text, "First");
        assert_eq!(runs[1].text, "Second");
        let (output, report) =
            edit_text_runs(&bytes, &[TextRunEdit { page: 1, index: 1, text: "Second edition".into() }]).expect("edit");
        assert_eq!(report.edited, 1);
        let reloaded = Document::load_mem(&output).expect("reload");
        let text = reloaded.extract_text(&[1]).unwrap_or_default();
        assert!(text.contains("First"));
        assert!(text.contains("Second edition"), "extracted: {text:?}");
    }

    #[test]
    fn refuses_text_the_font_cannot_encode() {
        let bytes = sample_with("BT\n/F1 12 Tf\n50 800 Td\n(Second) Tj\nET\n");
        let error = edit_text_runs(&bytes, &[TextRunEdit { page: 1, index: 0, text: "İkinci".into() }])
            .expect_err("WinAnsi cannot represent İ");
        assert!(format!("{error}").contains("cannot represent"), "{error}");
    }

    #[test]
    fn refuses_a_multi_string_tj_run() {
        let bytes = sample_with("BT\n/F1 12 Tf\n50 800 Td\n[(Hel) -50 (lo)] TJ\nET\n");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tj.pdf");
        std::fs::write(&path, &bytes).unwrap();
        let runs = list_text_runs_in_file(&path, None).expect("list");
        assert_eq!(runs.len(), 1);
        assert!(!runs[0].editable);
        assert!(runs[0].note.as_deref().unwrap_or_default().contains("TJ array"));
        let error = edit_text_runs(&bytes, &[TextRunEdit { page: 1, index: 0, text: "Hello".into() }])
            .expect_err("must refuse");
        assert!(format!("{error}").contains("No text was changed"));
    }

    #[test]
    fn td_moves_from_the_line_start_not_the_end_of_the_run() {
        let bytes = sample_with("BT\n/F1 12 Tf\n72 700 Td\n(A long first line) Tj\n0 -14 Td\n(Next) Tj\nET\n");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lines.pdf");
        std::fs::write(&path, &bytes).unwrap();
        let runs = list_text_runs_in_file(&path, None).expect("list");
        assert_eq!((runs[1].x, runs[1].y), (72.0, 686.0));
    }

    #[test]
    fn tj_kerning_wider_than_a_space_reads_as_a_space() {
        let bytes = sample_with("BT\n/F1 12 Tf\n72 700 Td\n[(Hello) -333 (world) -20 (s)] TJ\nET\n");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kern.pdf");
        std::fs::write(&path, &bytes).unwrap();
        let runs = list_text_runs_in_file(&path, None).expect("list");
        assert_eq!(runs[0].text, "Hello worlds");
    }

    #[test]
    fn reports_an_unknown_page_without_touching_the_file() {
        let bytes = sample_with("BT\n/F1 12 Tf\n50 800 Td\n(Hi) Tj\nET\n");
        let error =
            edit_text_runs(&bytes, &[TextRunEdit { page: 9, index: 0, text: "Bye".into() }]).expect_err("must fail");
        assert!(format!("{error}").contains("No text was changed"));
    }

    /// A one-page PDF whose only font is a Type0 Identity-H font with the
    /// given `/ToUnicode` CMap, showing `strings` two-byte strings.
    fn type0_sample(cmap: Vec<u8>, strings: usize) -> Vec<u8> {
        let mut doc = Document::with_version("1.7");
        let cmap_id = doc.add_object(lopdf::Stream::new(Dictionary::new(), cmap));
        let descendant = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "CIDFontType2", "BaseFont" => "Test",
            "CIDSystemInfo" => dictionary! { "Registry" => Object::string_literal("Adobe"),
                "Ordering" => Object::string_literal("Identity"), "Supplement" => 0 },
        });
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "Test", "Encoding" => "Identity-H",
            "DescendantFonts" => vec![Object::Reference(descendant)], "ToUnicode" => cmap_id,
        });
        let mut content = String::from("BT /F1 12 Tf 72 700 Td\n");
        for _ in 0..strings {
            content.push_str("<0041> Tj [<0041> -300 <0041>] TJ 0 -14 Td\n");
        }
        content.push_str("ET\n");
        let stream_id = doc.add_object(lopdf::Stream::new(Dictionary::new(), content.into_bytes()));
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
            "Contents" => stream_id,
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![Object::Reference(page_id)], "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => Object::Reference(pages_id) });
        doc.trailer.set("Root", Object::Reference(catalog_id));
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes
    }

    const SMALL_CMAP: &str = "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
        /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
        /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
        1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
        1 beginbfrange\n<0041> <0041> <0058>\nendbfrange\n\
        endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n";

    #[test]
    fn a_type0_to_unicode_map_is_parsed_once_per_font_not_per_string() {
        let bytes = type0_sample(SMALL_CMAP.as_bytes().to_vec(), 40);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("type0.pdf");
        std::fs::write(&path, &bytes).unwrap();
        ENCODINGS_BUILT.with(|count| count.set(0));
        let runs = list_text_runs_in_file(&path, None).expect("list");
        assert_eq!(runs.len(), 80);
        assert_eq!(runs[0].text, "X");
        assert_eq!(runs[1].text, "X X");
        assert_eq!(ENCODINGS_BUILT.with(std::cell::Cell::get), 1);
    }

    #[test]
    fn an_oversized_to_unicode_map_is_skipped_with_a_warning() {
        let mut cmap = SMALL_CMAP.as_bytes().to_vec();
        cmap.extend(std::iter::repeat_n(b' ', MAX_CMAP_BYTES + 1));
        let mut doc = Document::load_mem(&type0_sample(Vec::new(), 2)).unwrap();
        let cmap_id = doc
            .objects
            .iter()
            .find_map(|(id, object)| {
                matches!(object, Object::Stream(stream) if stream.content.is_empty()).then_some(*id)
            })
            .expect("cmap stream");
        let mut stream = lopdf::Stream::new(Dictionary::new(), cmap);
        stream.compress().unwrap();
        doc.objects.insert(cmap_id, Object::Stream(stream));
        let (_, page_id) = doc.get_pages().into_iter().next().unwrap();
        let mut fonts = FontCache::new(&doc);
        let runs = page_text_runs(&doc, &mut fonts, 1, page_id).expect("runs");
        // Without the CMap the string is read as raw bytes.
        assert_eq!(runs.len(), 4);
        assert_eq!(runs[0].text, "\0A");
        assert_eq!(fonts.take_warnings().len(), 1);
    }
}

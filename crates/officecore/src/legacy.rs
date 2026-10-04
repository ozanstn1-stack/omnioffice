//! Word 97-2003 (`.doc`) and PowerPoint 97-2003 (`.ppt`) import.
//!
//! Both formats are OLE2 Compound File Binary (CFB) containers. There is no
//! complete pure-Rust layout engine for them, so this importer extracts what
//! the binary streams make reliably available - text runs, paragraph breaks,
//! table cell marks and slide boundaries - and reports everything else as a
//! warning. Nothing is ever written back to the binary format: the editor
//! saves to DOCX/PPTX or to the native `.oswk` unit.
//!
//! * Word: the FIB's piece table (CLX in the `0Table`/`1Table` stream) gives
//!   the exact text pieces, including the compressed (single-byte) runs.
//! * PowerPoint: the `PowerPoint Document` stream is a record tree; text lives
//!   in `TextCharsAtom`/`TextBytesAtom`/`CString` records inside `Slide`
//!   containers.

use crate::encoding;
use crate::error::{OfficeError, OfficeResult};
use crate::io;
use crate::model::{Block, Deck, Slide, SlideObject, TextDocument, TextFrame, TextParagraph};
use std::io::Read;
use std::path::Path;

/// Result of a legacy import: the model plus what was not preserved.
#[derive(Debug)]
pub struct LegacyRead<T> {
    pub document: T,
    pub warnings: Vec<String>,
}

const MAX_LEGACY_TEXT_BYTES: usize = 64 * 1024 * 1024;
const MAX_SLIDES: usize = 500;
const MAX_RECORD_DEPTH: usize = 64;

fn le_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let slice = bytes.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

fn le_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn open_compound(path: &Path) -> OfficeResult<cfb::CompoundFile<std::fs::File>> {
    let file = std::fs::File::open(path).map_err(|error| OfficeError::from_io(error, path))?;
    cfb::CompoundFile::open(file).map_err(|error| {
        OfficeError::corrupt(format!(
            "{} is not an OLE2 compound document (Word/PowerPoint 97-2003): {error}",
            path.display()
        ))
    })
}

fn read_stream(compound: &mut cfb::CompoundFile<std::fs::File>, name: &str) -> OfficeResult<Vec<u8>> {
    let mut stream =
        compound.open_stream(name).map_err(|_| OfficeError::corrupt(format!("the {name} stream is missing")))?;
    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .map_err(|error| OfficeError::corrupt(format!("the {name} stream could not be read: {error}")))?;
    if bytes.len() > MAX_LEGACY_TEXT_BYTES {
        return Err(OfficeError::corrupt(format!("the {name} stream is too large to import")));
    }
    Ok(bytes)
}

// ---------------------------------------------------------------------------
// Word 97-2003 (.doc)
// ---------------------------------------------------------------------------

/// The Word binary magic (`wIdent`) at the start of the `WordDocument` stream.
const FIB_IDENT: u16 = 0xA5EC;

/// Reads `.doc` into the Writer model (text and paragraph breaks).
pub fn read_doc_file(path: &Path) -> OfficeResult<LegacyRead<TextDocument>> {
    let mut compound = open_compound(path)?;
    let word = read_stream(&mut compound, "WordDocument")?;
    if le_u16(&word, 0) != Some(FIB_IDENT) {
        return Err(OfficeError::corrupt("the WordDocument stream has no Word 97 identifier"));
    }
    let flags = le_u16(&word, 0x0A).unwrap_or(0);
    let table_name = if flags & 0x0200 != 0 { "1Table" } else { "0Table" };
    let table = read_stream(&mut compound, table_name).unwrap_or_default();
    let text = extract_word_text(&word, &table)?;

    let title = io::file_stem(path);
    let mut document = TextDocument::new_blank(&title);
    document.blocks.clear();
    for line in split_word_paragraphs(&text) {
        document.blocks.push(Block::paragraph(&line));
    }
    if document.blocks.is_empty() {
        document.blocks.push(Block::paragraph(""));
    }
    Ok(LegacyRead {
        document,
        warnings: vec![
            "Word 97-2003 import keeps the text and paragraph breaks. Character formatting, tables, headers, footnotes, images and revisions are not imported. Save as .docx or .oswk to keep your edits.".into(),
        ],
    })
}

/// Pulls the main document text through the piece table (or the raw
/// `fcMin..fcMac` range when there is no usable CLX).
fn extract_word_text(word: &[u8], table: &[u8]) -> OfficeResult<String> {
    let ccp_text = le_u32(word, 0x40).unwrap_or(0) as usize;
    if let Some(text) = extract_piece_table(word, table, ccp_text)? {
        return Ok(text);
    }
    // No piece table: the old 8-bit format stored the text between fcMin and
    // fcMac, one or two bytes per character.
    let fc_min = le_u32(word, 0x18).unwrap_or(0) as usize;
    let fc_mac = le_u32(word, 0x1C).unwrap_or(0) as usize;
    let end = fc_mac.min(word.len()).max(fc_min);
    let bytes = word.get(fc_min..end).unwrap_or(&[]);
    Ok(encoding::decode_legacy(bytes))
}

/// Parses the CLX (piece table) and returns `None` when the structure is not
/// present or not trustworthy.
fn extract_piece_table(word: &[u8], table: &[u8], ccp_text: usize) -> OfficeResult<Option<String>> {
    let (Some(fc_clx), Some(lcb_clx)) = (le_u32(word, 0x01A2), le_u32(word, 0x01A6)) else {
        return Ok(None);
    };
    let start = fc_clx as usize;
    let end = start.saturating_add(lcb_clx as usize);
    let Some(clx) = table.get(start..end) else {
        return Ok(None);
    };
    // The CLX is a sequence of Prc (clxt=0x01) blocks followed by one Pcdt
    // (clxt=0x02). Anything else means the table cannot be trusted.
    let mut offset = 0usize;
    while clx.get(offset) == Some(&0x01) {
        let Some(cb) = le_u16(clx, offset + 1) else { return Ok(None) };
        offset += 3 + cb as usize;
    }
    if clx.get(offset) != Some(&0x02) {
        return Ok(None);
    }
    let Some(lcb) = le_u32(clx, offset + 1) else { return Ok(None) };
    let pcd_start = offset + 5;
    let Some(plc) = clx.get(pcd_start..pcd_start.saturating_add(lcb as usize)) else {
        return Ok(None);
    };
    if plc.len() < 12 {
        return Ok(None);
    }
    let pieces = (plc.len() - 4) / 12;
    if pieces == 0 || pieces > 100_000 {
        return Ok(None);
    }
    // CPs: pieces+1 u32 values, then pieces 8-byte PCDs.
    let mut cps = Vec::with_capacity(pieces + 1);
    for index in 0..=pieces {
        let Some(cp) = le_u32(plc, index * 4) else { return Ok(None) };
        cps.push(cp as usize);
    }
    let pcd_base = (pieces + 1) * 4;
    let mut out = String::new();
    let mut emitted = 0usize;
    for index in 0..pieces {
        let Some(fc_raw) = le_u32(plc, pcd_base + index * 8 + 2) else { return Ok(None) };
        let chars = cps[index + 1].saturating_sub(cps[index]);
        if ccp_text > 0 && emitted >= ccp_text {
            break;
        }
        let take = if ccp_text > 0 { chars.min(ccp_text - emitted) } else { chars };
        let compressed = fc_raw & 0x4000_0000 != 0;
        let fc = (fc_raw & 0x3FFF_FFFF) as usize;
        if compressed {
            let Some(bytes) = word.get(fc..fc.saturating_add(take)) else { return Ok(None) };
            out.push_str(&encoding::decode_legacy(bytes));
        } else {
            let byte_len = take.saturating_mul(2);
            let Some(bytes) = word.get(fc..fc.saturating_add(byte_len)) else { return Ok(None) };
            out.push_str(&decode_utf16le(bytes));
        }
        emitted += take;
    }
    Ok(Some(out))
}

fn decode_utf16le(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
    String::from_utf16_lossy(&units)
}

/// Turns Word's control characters into the paragraph text the editor shows.
fn split_word_paragraphs(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for character in text.chars() {
        match character {
            // Paragraph end and page/line breaks all become a new paragraph.
            '\r' | '\u{0B}' | '\u{0C}' => {
                out.push(std::mem::take(&mut current));
            }
            // Cell and row marks are table structure; a tab keeps the cells
            // readable even though the table grid itself is not reconstructed.
            '\u{07}' => current.push('\t'),
            // Field marks, picture/footnote/drawn-object anchors are dropped.
            '\u{01}' | '\u{02}' | '\u{08}' | '\u{13}'..='\u{15}' => {}
            character if (character as u32) < 0x20 && character != '\n' && character != '\t' => {}
            character => current.push(character),
        }
    }
    out.push(current);
    // A trailing paragraph mark produces one empty paragraph; trim it.
    while out.len() > 1 && out.last().map(|line| line.trim().is_empty()).unwrap_or(false) {
        out.pop();
    }
    out.iter().map(|line| line.trim_end().to_string()).collect()
}

// ---------------------------------------------------------------------------
// PowerPoint 97-2003 (.ppt)
// ---------------------------------------------------------------------------

const PPT_SLIDE: u16 = 0x03EE;
const PPT_NOTES: u16 = 0x03F0;
const PPT_MAIN_MASTER: u16 = 0x03F8;
const PPT_DOCUMENT: u16 = 0x03E8;
const PPT_HANDOUT: u16 = 0x03F4;
const PPT_TEXT_CHARS: u16 = 0x0FA0;
const PPT_TEXT_BYTES: u16 = 0x0FA8;
const PPT_CSTRING: u16 = 0x0FBA;

/// Reads `.ppt` into the Impress model (one text frame per slide).
pub fn read_ppt_file(path: &Path) -> OfficeResult<LegacyRead<Deck>> {
    let mut compound = open_compound(path)?;
    let stream = read_stream(&mut compound, "PowerPoint Document")?;
    let mut slides: Vec<Vec<String>> = Vec::new();
    walk_ppt_records(&stream, &mut slides);
    if slides.len() > MAX_SLIDES {
        slides.truncate(MAX_SLIDES);
    }

    let title = io::file_stem(path);
    let mut deck = Deck::new_blank(&title);
    deck.slides.clear();
    for texts in &slides {
        deck.slides.push(slide_from_texts(texts));
    }
    if deck.slides.is_empty() {
        deck.slides.push(Slide::default());
    }
    let slide_count = deck.slides.len();
    Ok(LegacyRead {
        document: deck,
        warnings: vec![format!(
            "PowerPoint 97-2003 import kept the text of {slide_count} slide(s). Shapes, images, animations, themes and formatting are not imported. Save as .pptx or .oswk to keep your edits."
        )],
    })
}

/// Walks the record tree and collects the text atoms of every Slide container.
fn walk_ppt_records(bytes: &[u8], slides: &mut Vec<Vec<String>>) {
    visit_ppt_records(bytes, 0, bytes.len(), false, slides, 0);
}

fn visit_ppt_records(
    bytes: &[u8],
    mut offset: usize,
    end: usize,
    in_slide: bool,
    slides: &mut Vec<Vec<String>>,
    depth: usize,
) {
    if depth > MAX_RECORD_DEPTH {
        return;
    }
    while offset + 8 <= end {
        let Some(ver_inst) = le_u16(bytes, offset) else { return };
        let Some(rec_type) = le_u16(bytes, offset + 2) else { return };
        let Some(len) = le_u32(bytes, offset + 4) else { return };
        let len = len as usize;
        let body = offset + 8;
        let Some(rec_end) = body.checked_add(len).filter(|value| *value <= end) else {
            return; // A truncated record ends the walk; keep what was read.
        };
        let is_container = ver_inst & 0x000F == 0x000F;
        if in_slide {
            match rec_type {
                PPT_TEXT_CHARS => {
                    if let Some(text) = bytes.get(body..rec_end) {
                        if let Some(current) = slides.last_mut() {
                            current.push(decode_utf16le(text));
                        }
                    }
                }
                PPT_TEXT_BYTES | PPT_CSTRING => {
                    if let Some(text) = bytes.get(body..rec_end) {
                        if let Some(current) = slides.last_mut() {
                            let decoded = encoding::decode_legacy(text).trim_end_matches('\0').to_string();
                            current.push(decoded);
                        }
                    }
                }
                _ => {}
            }
        }
        if is_container {
            match rec_type {
                PPT_SLIDE => {
                    slides.push(Vec::new());
                    visit_ppt_records(bytes, body, rec_end, true, slides, depth + 1);
                }
                PPT_NOTES | PPT_MAIN_MASTER | PPT_DOCUMENT | PPT_HANDOUT => {
                    // Masters, notes pages and handouts are not the slide deck.
                }
                _ => visit_ppt_records(bytes, body, rec_end, in_slide, slides, depth + 1),
            }
        }
        offset = rec_end;
    }
}

/// One slide from its text atoms: a short first atom becomes the title, the
/// rest is stacked below as body text.
fn slide_from_texts(texts: &[String]) -> Slide {
    let mut slide = Slide::default();
    let mut lines: Vec<String> =
        texts.iter().map(|text| clean_ppt_text(text)).filter(|text| !text.is_empty()).collect();
    if lines.is_empty() {
        return slide;
    }
    let first_is_title = lines[0].chars().count() <= 90 && lines.len() > 1;
    let title_text = if first_is_title { lines.remove(0) } else { String::new() };
    let has_title = !title_text.is_empty();

    let mut z = 1;
    if has_title {
        let mut title = SlideObject::new("text", 60.0, 40.0, 840.0, 90.0);
        title.z = z;
        z += 1;
        title.placeholder = Some("title".into());
        title.name = "Title".into();
        title.text = Some(text_frame(&[title_text]));
        slide.objects.push(title);
    }
    let mut body = SlideObject::new("text", 60.0, if has_title { 150.0 } else { 60.0 }, 840.0, 330.0);
    body.z = z;
    body.placeholder = Some("body".into());
    body.name = "Content".into();
    body.text = Some(text_frame(&lines));
    slide.objects.push(body);
    slide
}

fn text_frame(lines: &[String]) -> TextFrame {
    TextFrame {
        paragraphs: lines
            .iter()
            .map(|line| TextParagraph { text: line.clone(), bullet: false, ..Default::default() })
            .collect(),
        ..Default::default()
    }
}

/// PPT text uses CR as paragraph end and vertical tab as a line break.
fn clean_ppt_text(text: &str) -> String {
    let mut out = String::new();
    for character in text.chars() {
        match character {
            '\r' | '\u{0B}' | '\n' => out.push('\n'),
            character if (character as u32) < 0x20 => {}
            character => out.push(character),
        }
    }
    out.lines().map(str::trim).collect::<Vec<_>>().join("\n").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_stream(compound: &mut cfb::CompoundFile<std::fs::File>, name: &str, bytes: &[u8]) {
        let mut stream = compound.create_stream(name).expect("create stream");
        stream.write_all(bytes).expect("write stream");
    }

    /// Builds a minimal but structurally valid Word 97 file: FIB + one
    /// uncompressed UTF-16 piece + one compressed piece.
    fn write_test_doc(path: &Path, text: &str) {
        let mut compound = cfb::create(path).expect("cfb");
        // WordDocument: 0x800 bytes of FIB, then the text pieces at 0x800.
        let mut word = vec![0u8; 0x800];
        word[0..2].copy_from_slice(&FIB_IDENT.to_le_bytes());
        word[0x0A] = 0x00; // use 0Table
        word[0x18..0x1C].copy_from_slice(&0x800u32.to_le_bytes()); // fcMin
                                                                   // Text: first piece UTF-16 at 0x800, second compressed at 0x900.
        let utf16: Vec<u8> = text.encode_utf16().flat_map(|unit| unit.to_le_bytes()).collect();
        word.splice(0x800..0x800, utf16.iter().copied());
        let ansi = b"\rsecond line";
        let ansi_offset = 0x800 + utf16.len();
        word.splice(ansi_offset..ansi_offset, ansi.iter().copied());
        let ccp = (text.chars().count() + "\rsecond line".chars().count()) as u32;
        word[0x40..0x44].copy_from_slice(&ccp.to_le_bytes());

        // 0Table: a CLX with one Pcdt covering both pieces.
        let cp0 = 0u32;
        let cp1 = text.chars().count() as u32;
        let cp2 = ccp;
        let mut plc = Vec::new();
        plc.extend_from_slice(&cp0.to_le_bytes());
        plc.extend_from_slice(&cp1.to_le_bytes());
        plc.extend_from_slice(&cp2.to_le_bytes());
        // PCD 1: uncompressed UTF-16 at 0x800.
        plc.extend_from_slice(&0u16.to_le_bytes());
        plc.extend_from_slice(&0x800u32.to_le_bytes());
        plc.extend_from_slice(&0u16.to_le_bytes());
        // PCD 2: compressed at ansi_offset (flag bit 30).
        plc.extend_from_slice(&0u16.to_le_bytes());
        plc.extend_from_slice(&(0x4000_0000u32 | ansi_offset as u32).to_le_bytes());
        plc.extend_from_slice(&0u16.to_le_bytes());
        let mut clx = vec![0x02];
        clx.extend_from_slice(&(plc.len() as u32).to_le_bytes());
        clx.extend_from_slice(&plc);
        let fc_clx = 0x100u32;
        word[0x01A2..0x01A6].copy_from_slice(&fc_clx.to_le_bytes());
        word[0x01A6..0x01AA].copy_from_slice(&(clx.len() as u32).to_le_bytes());

        let mut table = vec![0u8; fc_clx as usize];
        table.extend_from_slice(&clx);

        write_stream(&mut compound, "/WordDocument", &word);
        write_stream(&mut compound, "/0Table", &table);
    }

    fn record(rec_type: u16, body: &[u8], container: bool) -> Vec<u8> {
        let mut out = Vec::new();
        let ver_inst: u16 = if container { 0x000F } else { 0x0000 };
        out.extend_from_slice(&ver_inst.to_le_bytes());
        out.extend_from_slice(&rec_type.to_le_bytes());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(body);
        out
    }

    fn write_test_ppt(path: &Path) {
        let mut compound = cfb::create(path).expect("cfb");
        let mut slide1 = Vec::new();
        let title = "Başlık bir\r".encode_utf16().flat_map(|u| u.to_le_bytes()).collect::<Vec<u8>>();
        slide1.extend_from_slice(&record(PPT_TEXT_CHARS, &title, false));
        slide1.extend_from_slice(&record(PPT_TEXT_BYTES, b"Body line\r", false));
        let mut slide2 = Vec::new();
        let second = "Second slide".encode_utf16().flat_map(|u| u.to_le_bytes()).collect::<Vec<u8>>();
        slide2.extend_from_slice(&record(PPT_TEXT_CHARS, &second, false));
        let mut document = Vec::new();
        document.extend_from_slice(&record(PPT_SLIDE, &slide1, true));
        document.extend_from_slice(&record(PPT_SLIDE, &slide2, true));
        // A notes container must be ignored.
        let notes = record(
            PPT_TEXT_CHARS,
            &"not a slide".encode_utf16().flat_map(|u| u.to_le_bytes()).collect::<Vec<u8>>(),
            false,
        );
        document.extend_from_slice(&record(PPT_NOTES, &notes, true));
        write_stream(&mut compound, "/PowerPoint Document", &document);
    }

    #[test]
    fn doc_piece_table_imports_text_and_paragraphs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("legacy.doc");
        write_test_doc(&path, "Merhaba dünya");
        let read = read_doc_file(&path).expect("read doc");
        let text: Vec<String> = read.document.blocks.iter().map(Block::plain_text).collect();
        assert_eq!(text, vec!["Merhaba dünya", "second line"]);
        assert!(read.warnings[0].contains("Word 97-2003"));
    }

    #[test]
    fn ppt_slide_text_is_imported_per_slide() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("legacy.ppt");
        write_test_ppt(&path);
        let read = read_ppt_file(&path).expect("read ppt");
        assert_eq!(read.document.slides.len(), 2, "the notes container must not become a slide");
        let first = read.document.slides[0]
            .objects
            .iter()
            .filter_map(|object| object.text.as_ref())
            .map(TextFrame::plain)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(first.contains("Başlık bir"));
        assert!(first.contains("Body line"));
        let second = read.document.slides[1]
            .objects
            .iter()
            .filter_map(|object| object.text.as_ref())
            .map(TextFrame::plain)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(second.contains("Second slide"));
    }

    #[test]
    fn non_ole_input_is_reported_as_corrupt() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("fake.doc");
        std::fs::write(&path, b"not an ole file").expect("write");
        let error = read_doc_file(&path).expect_err("must fail");
        assert!(format!("{error}").contains("OLE2"));
    }
}

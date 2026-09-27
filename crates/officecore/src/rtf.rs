//! RTF (Rich Text Format) import and export.
//!
//! Supports the practical subset: character formatting, paragraph alignment
//! and indents, lists as literal markers, tables, images, page breaks and
//! simple headers/footers. Unsupported RTF destinations are skipped on import
//! and reported as warnings.

use crate::error::{OfficeError, OfficeResult};
use crate::model::*;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct RtfRead {
    pub document: TextDocument,
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

fn hex_color(value: &str) -> Option<(u8, u8, u8)> {
    let cleaned = value.trim().trim_start_matches('#');
    let expanded = if cleaned.len() == 3 {
        cleaned.chars().flat_map(|ch| [ch, ch]).collect::<String>()
    } else {
        cleaned.to_string()
    };
    if expanded.len() != 6 {
        return None;
    }
    let red = u8::from_str_radix(&expanded[0..2], 16).ok()?;
    let green = u8::from_str_radix(&expanded[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&expanded[4..6], 16).ok()?;
    Some((red, green, blue))
}

fn escape_rtf(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' | '{' | '}' => {
                out.push('\\');
                out.push(ch);
            }
            '\n' => out.push_str("\\line "),
            '\t' => out.push_str("\\tab "),
            ch if (ch as u32) < 128 => out.push(ch),
            ch => {
                let code = ch as i32;
                let signed = if code > 32767 { code - 65536 } else { code };
                out.push_str(&format!("\\u{signed}?"));
            }
        }
    }
    out
}

struct RtfTables {
    fonts: Vec<String>,
    colors: Vec<(u8, u8, u8)>,
    /// Revision authors in `\revtbl` order; `\revauth` carries the index.
    authors: Vec<String>,
}

impl RtfTables {
    fn font_index(&mut self, font: &str) -> usize {
        if let Some(index) = self.fonts.iter().position(|existing| existing == font) {
            return index;
        }
        self.fonts.push(font.to_string());
        self.fonts.len() - 1
    }

    fn color_index(&mut self, color: &str) -> Option<usize> {
        let rgb = hex_color(color)?;
        if let Some(index) = self.colors.iter().position(|existing| *existing == rgb) {
            return Some(index);
        }
        self.colors.push(rgb);
        Some(self.colors.len() - 1)
    }

    fn author_index(&mut self, author: &str) -> usize {
        if let Some(index) = self.authors.iter().position(|existing| existing == author) {
            return index;
        }
        self.authors.push(author.to_string());
        self.authors.len() - 1
    }
}

/// Note bodies keyed by id, resolved while writing runs. RTF footnotes cannot
/// nest, so note bodies are written with an empty context.
#[derive(Default, Clone)]
struct RtfNotes {
    footnotes: HashMap<String, Footnote>,
    endnotes: HashMap<String, Footnote>,
}

impl RtfNotes {
    fn for_document(document: &TextDocument) -> Self {
        Self {
            footnotes: document.footnotes.iter().map(|note| (note.id.clone(), note.clone())).collect(),
            endnotes: document.endnotes.iter().map(|note| (note.id.clone(), note.clone())).collect(),
        }
    }

    fn footnote_for(&self, run: &Run) -> Option<&Footnote> {
        run.footnote.as_ref().and_then(|id| self.footnotes.get(id))
    }

    fn endnote_for(&self, run: &Run) -> Option<&Footnote> {
        run.endnote.as_ref().and_then(|id| self.endnotes.get(id))
    }
}

/// RTF writes `\revdttm` as a packed 32-bit DTTM (6-bit minute, 5-bit hour,
/// day, month, year-1900, weekday); the seconds of our ISO dates are dropped
/// because the format has no second field.
fn iso_to_dttm(date: &str) -> Option<u32> {
    let date = date.trim();
    if date.len() < 16 {
        return None;
    }
    let year: u32 = date.get(0..4)?.parse().ok()?;
    let month: u32 = date.get(5..7)?.parse().ok()?;
    let day: u32 = date.get(8..10)?.parse().ok()?;
    let hour: u32 = date.get(11..13)?.parse().ok()?;
    let minute: u32 = date.get(14..16)?.parse().ok()?;
    if !(1900..=2140).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    Some((minute & 0x3F) | ((hour & 0x1F) << 6) | ((day & 0x1F) << 11) | ((month & 0x0F) << 16) | (((year - 1900) & 0x1FF) << 20))
}

fn dttm_to_iso(value: u32) -> String {
    let minute = value & 0x3F;
    let hour = (value >> 6) & 0x1F;
    let day = (value >> 11) & 0x1F;
    let month = (value >> 16) & 0x0F;
    let year = ((value >> 20) & 0x1FF) + 1900;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:00Z")
}

fn collect_fonts(document: &TextDocument) -> Vec<String> {
    let mut fonts: Vec<String> = Vec::new();
    for style in &document.styles {
        if let Some(font) = &style.font {
            if !fonts.contains(font) {
                fonts.push(font.clone());
            }
        }
    }
    let mut add_blocks = |blocks: &[Block]| {
        for block in blocks {
            if let Block::Paragraph { runs, .. } = block {
                for run in runs {
                    if let Some(font) = &run.font {
                        if !fonts.contains(font) {
                            fonts.push(font.clone());
                        }
                    }
                }
            }
        }
    };
    add_blocks(&document.blocks);
    add_blocks(&document.header);
    add_blocks(&document.footer);
    if fonts.is_empty() {
        fonts.push("Calibri".into());
    }
    fonts
}

fn paragraph_style_index(document: &TextDocument, style_id: &str) -> usize {
    document.styles.iter().position(|style| style.id == style_id).unwrap_or(0)
}

/// Emits one note as a real RTF `\footnote` destination. `\chftn` is the
/// automatic reference anchor. RTF has no per-note endnote class, so endnotes
/// are the same destination plus our own ignorable `{\*\oswkendnote}` marker:
/// Word skips it and renders the note as a footnote, while this reader can
/// restore the class on round trip. The note text itself is never emitted at
/// the reference position.
fn write_note(out: &mut String, note: &Footnote, endnote: bool, tables: &mut RtfTables) {
    out.push_str("{\\footnote \\chftn ");
    if endnote {
        out.push_str("{\\*\\oswkendnote}");
    }
    for run in &note.runs {
        if run.text.is_empty() {
            continue;
        }
        write_run(out, run, tables, &RtfNotes::default());
    }
    out.push('}');
}

/// Revision properties are character properties, so they live inside the run
/// group the text is wrapped in. `\revauthN` indexes the `\revtbl` header
/// table; `\revdttmN` is the DTTM packed timestamp (second precision is lost).
fn write_revision_controls(out: &mut String, revision: &RevisionMark, tables: &mut RtfTables) {
    match revision.kind.as_str() {
        "delete" => out.push_str("\\deleted "),
        "insert" => out.push_str("\\revised "),
        // Formatting revisions have no faithful RTF text-mark equivalent this
        // writer can emit (\crauth/\crdate are formatting properties, not text
        // marks), so they keep their formatting and are documented as
        // not representable.
        _ => return,
    }
    if !revision.author.is_empty() {
        let index = tables.author_index(&revision.author);
        out.push_str(&format!("\\revauth{index} "));
    }
    if let Some(dttm) = iso_to_dttm(&revision.date) {
        out.push_str(&format!("\\revdttm{dttm} "));
    }
}

fn write_run(out: &mut String, run: &Run, tables: &mut RtfTables, notes: &RtfNotes) {
    if let Some(note) = notes.footnote_for(run) {
        write_note(out, note, false, tables);
    }
    if let Some(note) = notes.endnote_for(run) {
        write_note(out, note, true, tables);
    }
    out.push('{');
    if let Some(revision) = &run.revision {
        write_revision_controls(out, revision, tables);
    }
    if let Some(font) = &run.font {
        let index = tables.font_index(font);
        out.push_str(&format!("\\f{index} "));
    }
    if let Some(size) = run.size_pt {
        out.push_str(&format!("\\fs{} ", (size * 2.0).round() as i64));
    }
    if run.bold {
        out.push_str("\\b ");
    }
    if run.italic {
        out.push_str("\\i ");
    }
    if run.underline {
        out.push_str("\\ul ");
    }
    if run.strike {
        out.push_str("\\strike ");
    }
    if let Some(color) = &run.color {
        if let Some(index) = tables.color_index(color) {
            out.push_str(&format!("\\cf{} ", index + 1));
        }
    }
    if let Some(highlight) = &run.highlight {
        if let Some(index) = tables.color_index(highlight) {
            out.push_str(&format!("\\highlight{} ", index + 1));
        }
    }
    if run.superscript {
        out.push_str("\\super ");
    }
    if run.subscript {
        out.push_str("\\sub ");
    }
    out.push_str(&escape_rtf(&run.text));
    out.push('}');
}

fn write_block(out: &mut String, block: &Block, tables: &mut RtfTables, document: &TextDocument, notes: &RtfNotes, depth: usize) {
    match block {
        Block::Paragraph { props, runs } => {
            out.push_str("\\pard");
            let style_index = paragraph_style_index(document, &props.style);
            out.push_str(&format!("\\s{style_index}"));
            match props.align.as_str() {
                "center" => out.push_str("\\qc"),
                "right" => out.push_str("\\qr"),
                "justify" => out.push_str("\\qj"),
                _ => out.push_str("\\ql"),
            }
            if props.space_before_pt > 0.0 {
                out.push_str(&format!("\\sb{}", (props.space_before_pt * 20.0).round() as i64));
            }
            if props.space_after_pt > 0.0 {
                out.push_str(&format!("\\sa{}", (props.space_after_pt * 20.0).round() as i64));
            }
            if props.line_spacing > 0.0 {
                out.push_str(&format!("\\sl{}", (props.line_spacing * 240.0).round() as i64));
                out.push_str("\\slmult1");
            }
            let mut indent_left = props.indent_left_pt;
            if let Some(list) = &props.list {
                indent_left += 18.0 * (list.level as f64 + 1.0);
                let marker = if list.kind == "number" {
                    format!("{}.\\tab ", list.start)
                } else {
                    "\\u8226?\\tab ".to_string()
                };
                out.push_str(&format!("\\li{} \\fi-360 ", (indent_left * 20.0).round() as i64));
                out.push('{');
                out.push_str(&marker);
                out.push('}');
            } else {
                if indent_left > 0.0 {
                    out.push_str(&format!("\\li{}", (indent_left * 20.0).round() as i64));
                }
                if props.indent_right_pt > 0.0 {
                    out.push_str(&format!("\\ri{}", (props.indent_right_pt * 20.0).round() as i64));
                }
                if props.first_line_pt != 0.0 {
                    out.push_str(&format!("\\fi{}", (props.first_line_pt * 20.0).round() as i64));
                }
            }
            out.push(' ');
            for run in runs {
                write_run(out, run, tables, notes);
            }
            out.push_str("\\par\n");
        }
        Block::Table { table } => {
            if depth > 2 {
                return;
            }
            let column_count = table.rows.iter().map(|row| row.cells.len()).max().unwrap_or(1).max(1);
            let total = 9000i64;
            let column_width = total / column_count as i64;
            for row in &table.rows {
                out.push_str("\\trowd\\trgaph108");
                for index in 0..row.cells.len() {
                    out.push_str(&format!("\\cellx{}", column_width * (index as i64 + 1)));
                }
                out.push('\n');
                for cell in &row.cells {
                    out.push_str("\\pard\\intbl ");
                    if cell.blocks.is_empty() {
                        out.push_str("\\par");
                    }
                    for block in &cell.blocks {
                        write_block(out, block, tables, document, notes, depth + 1);
                    }
                    out.push_str("\\cell ");
                }
                out.push_str("\\row\n");
            }
            out.push_str("\\pard\\par\n");
        }
        Block::Image { image, width_pt, height_pt, .. } => {
            let bytes = image.bytes();
            if bytes.is_empty() {
                return;
            }
            let control = if image.mime.contains("jpeg") || image.mime.contains("jpg") { "\\jpegblip" } else { "\\pngblip" };
            let pixel_width = (width_pt * 15.0).round().max(1.0) as i64;
            let pixel_height = (height_pt * 15.0).round().max(1.0) as i64;
            out.push_str(&format!("{{\\pict{control}\\picw{pixel_width}\\pich{pixel_height}\\picwgoal{}\\pichgoal{} ", (width_pt * 20.0) as i64, (height_pt * 20.0) as i64));
            let mut hex = String::with_capacity(bytes.len() * 2);
            for byte in bytes {
                hex.push_str(&format!("{byte:02x}"));
                if hex.len() >= 128 {
                    out.push_str(&hex);
                    out.push('\n');
                    hex.clear();
                }
            }
            out.push_str(&hex);
            out.push_str("}\\par\n");
        }
        Block::PageBreak => out.push_str("\\page\n"),
        Block::SectionBreak { .. } => out.push_str("\\page\n"),
        Block::Rule => out.push_str("\\pard\\brdrb\\brdrs\\brdrw6 \\par\n"),
        Block::Toc { entries } => {
            for entry in entries {
                let text = if entry.page > 0 {
                    format!("{} .... {}", entry.text, entry.page)
                } else {
                    entry.text.clone()
                };
                let indent = (entry.level.saturating_sub(1) as i64) * 240;
                out.push_str(&format!("\\pard\\li{indent} {}\\par\n", escape_rtf(&text)));
            }
        }
    }
}

fn collect_revision_authors_runs(runs: &[Run], authors: &mut Vec<String>) {
    for run in runs {
        if let Some(revision) = &run.revision {
            if !revision.author.is_empty() && !authors.iter().any(|existing| existing == &revision.author) {
                authors.push(revision.author.clone());
            }
        }
    }
}

fn collect_revision_authors_blocks(blocks: &[Block], authors: &mut Vec<String>) {
    for block in blocks {
        match block {
            Block::Paragraph { runs, .. } => collect_revision_authors_runs(runs, authors),
            Block::Table { table } => {
                for row in &table.rows {
                    for cell in &row.cells {
                        collect_revision_authors_blocks(&cell.blocks, authors);
                    }
                }
            }
            _ => {}
        }
    }
}

/// The `\revtbl` header must list every author before the body references its
/// index, so authors are gathered up front from body, headers, footers and
/// note bodies. Index 0 is the conventional "Unknown" entry: Word writes it
/// and maps `\revauthN` to the Nth group, so keeping the slot makes the table
/// readable in Word as well as by this reader.
fn collect_revision_authors(document: &TextDocument) -> Vec<String> {
    let mut authors = vec!["Unknown".to_string()];
    collect_revision_authors_blocks(&document.blocks, &mut authors);
    collect_revision_authors_blocks(&document.header, &mut authors);
    collect_revision_authors_blocks(&document.footer, &mut authors);
    for note in document.footnotes.iter().chain(document.endnotes.iter()) {
        collect_revision_authors_runs(&note.runs, &mut authors);
    }
    authors
}

pub fn write_rtf(document: &TextDocument) -> OfficeResult<Vec<u8>> {
    let fonts = collect_fonts(document);
    let authors = collect_revision_authors(document);
    let notes = RtfNotes::for_document(document);
    let mut tables = RtfTables { fonts, colors: Vec::new(), authors };
    let mut body = String::new();
    for block in &document.blocks {
        write_block(&mut body, block, &mut tables, document, &notes, 0);
    }
    let mut header = String::new();
    for block in &document.header {
        write_block(&mut header, block, &mut tables, document, &notes, 0);
    }
    let mut footer = String::new();
    for block in &document.footer {
        write_block(&mut footer, block, &mut tables, document, &notes, 0);
    }

    let mut out = String::from("{\\rtf1\\ansi\\ansicpg1252\\deff0\\nouicompat\\deflang1033\n");
    out.push_str("{\\fonttbl");
    for (index, font) in tables.fonts.iter().enumerate() {
        out.push_str(&format!("{{\\f{index}\\fnil\\fcharset0 {};}}", font.replace(';', "")));
    }
    out.push_str("}\n{\\colortbl;");
    for (red, green, blue) in &tables.colors {
        out.push_str(&format!("\\red{red}\\green{green}\\blue{blue};"));
    }
    out.push_str("}\n{\\stylesheet");
    for (index, style) in document.styles.iter().enumerate() {
        out.push_str(&format!("{{\\s{index}\\sbasedon0\\snext0 {};}}", escape_rtf(&style.name)));
    }
    out.push_str("}\n");
    // Revision author names; RTF's \revtbl only carries names (no timestamps),
    // which is why each revision also stores its own \revdttm. The "Unknown"
    // slot is always present, so only emit the table when a real author was
    // collected.
    if tables.authors.len() > 1 {
        out.push_str("{\\*\\revtbl ");
        for author in &tables.authors {
            out.push_str(&format!("{{{author};}}", author = escape_rtf(author)));
        }
        out.push_str("}\n");
    }
    let page = &document.page;
    out.push_str(&format!(
        "\\paperw{}\\paperh{}\\margl{}\\margr{}\\margt{}\\margb{}",
        (page.width_pt * 20.0).round() as i64,
        (page.height_pt * 20.0).round() as i64,
        (page.margin_left_pt * 20.0).round() as i64,
        (page.margin_right_pt * 20.0).round() as i64,
        (page.margin_top_pt * 20.0).round() as i64,
        (page.margin_bottom_pt * 20.0).round() as i64
    ));
    if page.orientation == "landscape" {
        out.push_str("\\landscape");
    }
    // RTF places every note as a footnote; \aendnotes\aenddoc at least makes a
    // document with only endnotes render them at the end in Word. Mixed
    // footnotes/endnotes keep the default footnote placement, and the endnote
    // class survives only through the {\*\oswkendnote} marker.
    if !document.endnotes.is_empty() && document.footnotes.is_empty() {
        out.push_str("\\aendnotes\\aenddoc");
    }
    if !header.is_empty() {
        out.push_str("\n{\\header ");
        out.push_str(&header);
        out.push('}');
    }
    if !footer.is_empty() {
        out.push_str("\n{\\footer ");
        out.push_str(&footer);
        out.push('}');
    }
    out.push('\n');
    out.push_str(&body);
    out.push_str("}\n");
    Ok(out.into_bytes())
}

pub fn write_rtf_file(path: &Path, document: &TextDocument) -> OfficeResult<()> {
    crate::io::write_atomic(path, &write_rtf(document)?)
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

#[derive(Default, Clone)]
struct CharFormat {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    color: Option<String>,
    highlight: Option<String>,
    font: Option<String>,
    size_pt: Option<f64>,
    superscript: bool,
    subscript: bool,
}

#[derive(Default, Clone)]
struct ParaFormat {
    align: String,
    indent_left_pt: f64,
    indent_right_pt: f64,
    first_line_pt: f64,
    space_before_pt: f64,
    space_after_pt: f64,
    line_spacing: f64,
    style_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Destination {
    Body,
    Header,
    Footer,
}

struct Reader {
    fonts: Vec<String>,
    colors: Vec<(u8, u8, u8)>,
    char_format: CharFormat,
    para_format: ParaFormat,
    blocks: Vec<Block>,
    runs: Vec<Run>,
    header_blocks: Vec<Block>,
    footer_blocks: Vec<Block>,
    warnings: Vec<String>,
    destination: Destination,
    destination_stack: Vec<Destination>,
    cell_blocks: Vec<Block>,
    in_table: bool,
    current_row: Vec<TableCell>,
    pict: Option<PictState>,
    skip_unicode: i32,
    depth: usize,
    /// Revision authors from `\revtbl`, indexed the way `\revauth` refers to them.
    revision_authors: Vec<String>,
    /// Current character revision, scoped like the RTF group it was set in.
    revision: Option<RevisionMark>,
    revision_stack: Vec<Option<RevisionMark>>,
    revision_seq: usize,
    /// Author index waiting for a `\revised`/`\deleted`/`\crauth` to attach to.
    revauth: Option<usize>,
    /// While a `\footnote` destination is open, text lands here instead of in
    /// the body runs; the reference run is inserted when the group closes.
    note_runs: Vec<Run>,
    note_depth: Option<usize>,
    note_endnote: bool,
    note_seq: usize,
    footnotes: Vec<Footnote>,
    endnotes: Vec<Footnote>,
}

#[derive(Default)]
struct PictState {
    hex: String,
    is_jpeg: bool,
    is_png: bool,
    width_goal_pt: f64,
    height_goal_pt: f64,
}

impl Reader {
    fn new() -> Self {
        Self {
            fonts: Vec::new(),
            colors: Vec::new(),
            char_format: CharFormat::default(),
            para_format: ParaFormat { align: "left".into(), line_spacing: 1.15, ..Default::default() },
            blocks: Vec::new(),
            runs: Vec::new(),
            header_blocks: Vec::new(),
            footer_blocks: Vec::new(),
            warnings: Vec::new(),
            destination: Destination::Body,
            destination_stack: Vec::new(),
            cell_blocks: Vec::new(),
            in_table: false,
            current_row: Vec::new(),
            pict: None,
            skip_unicode: 0,
            depth: 0,
            revision_authors: Vec::new(),
            revision: None,
            revision_stack: Vec::new(),
            revision_seq: 0,
            revauth: None,
            note_runs: Vec::new(),
            note_depth: None,
            note_endnote: false,
            note_seq: 0,
            footnotes: Vec::new(),
            endnotes: Vec::new(),
        }
    }

    fn target_blocks(&mut self) -> &mut Vec<Block> {
        match self.destination {
            Destination::Header => &mut self.header_blocks,
            Destination::Footer => &mut self.footer_blocks,
            Destination::Body => &mut self.blocks,
        }
    }

    fn push_block(&mut self, block: Block) {
        if self.in_table {
            self.cell_blocks.push(block);
        } else {
            self.target_blocks().push(block);
        }
    }

    fn flush_run(&mut self) {
        if self.runs.is_empty() {
            return;
        }
        let runs = std::mem::take(&mut self.runs);
        let format = &self.para_format;
        let mut props = ParaProps::default();
        props.align = if format.align.is_empty() { "left".into() } else { format.align.clone() };
        props.indent_left_pt = format.indent_left_pt;
        props.indent_right_pt = format.indent_right_pt;
        props.first_line_pt = format.first_line_pt;
        props.space_before_pt = format.space_before_pt;
        props.space_after_pt = format.space_after_pt;
        props.line_spacing = if format.line_spacing > 0.0 { format.line_spacing } else { 1.15 };
        if props.align == "left" && format.style_index > 0 {
            props.style = format!("Style{}", format.style_index);
        }
        self.push_block(Block::Paragraph { props, runs });
    }

    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.pict.is_some() {
            return;
        }
        let format = self.char_format.clone();
        let run = Run {
            text: text.to_string(),
            bold: format.bold,
            italic: format.italic,
            underline: format.underline,
            strike: format.strike,
            color: format.color,
            highlight: format.highlight,
            font: format.font,
            size_pt: format.size_pt,
            superscript: format.superscript,
            subscript: format.subscript,
            revision: self.revision.clone(),
            ..Default::default()
        };
        // Text inside a \footnote destination belongs to the note, not to the
        // surrounding paragraph.
        let target = if self.note_depth.is_some() { &mut self.note_runs } else { &mut self.runs };
        if let Some(last) = target.last_mut() {
            if last.bold == run.bold
                && last.italic == run.italic
                && last.underline == run.underline
                && last.strike == run.strike
                && last.color == run.color
                && last.highlight == run.highlight
                && last.font == run.font
                && last.size_pt == run.size_pt
                && last.revision == run.revision
            {
                last.text.push_str(&run.text);
                return;
            }
        }
        target.push(run);
    }

    /// Creates (or reuses) the current revision mark, attaching the pending
    /// `\revauth` author when one was recorded before the mark itself.
    fn start_revision(&mut self, kind: &str) {
        if self.revision.is_none() {
            self.revision_seq += 1;
            let author = self.revauth.take().and_then(|index| self.revision_authors.get(index)).cloned().unwrap_or_default();
            self.revision = Some(RevisionMark { id: format!("rev{}", self.revision_seq), kind: kind.to_string(), author, date: String::new(), original: None });
        } else if let Some(mark) = self.revision.as_mut() {
            mark.kind = kind.to_string();
        }
    }

    /// Restores the revision state of the group that is closing. A pending
    /// `\revauth` index is only meaningful inside its own group, so it is
    /// dropped once no revision is active.
    fn close_revision_group(&mut self) {
        self.revision = self.revision_stack.pop().flatten();
        if self.revision.is_none() {
            self.revauth = None;
        }
    }

    /// Closes an open `\footnote` destination: stores the body as a note and
    /// places the reference run at the current body position.
    fn finish_note(&mut self) {
        if self.note_depth.is_none() {
            return;
        }
        self.note_depth = None;
        self.note_seq += 1;
        let endnote = self.note_endnote;
        self.note_endnote = false;
        let id = format!("{}{}", if endnote { "en" } else { "fn" }, self.note_seq);
        let mut runs = std::mem::take(&mut self.note_runs);
        while runs.last().map(|run| run.text == "\n").unwrap_or(false) {
            runs.pop();
        }
        if runs.is_empty() {
            runs.push(Run::default());
        }
        let note = Footnote { id: id.clone(), runs, marker: String::new() };
        if endnote {
            self.endnotes.push(note);
        } else {
            self.footnotes.push(note);
        }
        self.runs.push(Run {
            footnote: if endnote { None } else { Some(id.clone()) },
            endnote: if endnote { Some(id) } else { None },
            ..Default::default()
        });
    }
}

fn parse_color_table(reader: &mut Reader, value: &str) {
    for entry in value.split(';') {
        let mut red = 0u8;
        let mut green = 0u8;
        let mut blue = 0u8;
        let mut at = 0usize;
        let bytes = entry.as_bytes();
        while at + 1 < bytes.len() {
            if bytes[at] != b'\\' {
                at += 1;
                continue;
            }
            let rest = &entry[at + 1..];
            let (keyword, length) = match rest.find(|ch: char| !ch.is_ascii_alphabetic()) {
                Some(index) => (&rest[..index], index),
                None => (rest, rest.len()),
            };
            let number: String = rest[length..].chars().take_while(|ch| ch.is_ascii_digit() || *ch == '-').collect();
            let parsed = number.parse::<i32>().unwrap_or(0).clamp(0, 255) as u8;
            match keyword {
                "red" => red = parsed,
                "green" => green = parsed,
                "blue" => blue = parsed,
                _ => {}
            }
            at += 1 + length + number.len();
        }
        reader.colors.push((red, green, blue));
    }
}

fn color_for(colors: &[(u8, u8, u8)], index: i32) -> Option<String> {
    if index <= 0 {
        return None;
    }
    colors.get((index - 1) as usize).map(|(red, green, blue)| format!("#{red:02X}{green:02X}{blue:02X}"))
}

/// Parses `{\*\revtbl {Author1;}{Author2;}}`: every subgroup is one author
/// name, in the order `\revauthN` indexes. Note that the table carries only
/// names; timestamps live on each revision as `\revdttm`.
fn parse_revtbl(reader: &mut Reader, raw: &str) {
    let mut name = String::new();
    let mut depth = 0i32;
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '{' => {
                depth += 1;
                name.clear();
            }
            '}' => {
                if depth >= 1 {
                    let trimmed = name.trim();
                    if !trimmed.is_empty() {
                        reader.revision_authors.push(trimmed.to_string());
                    }
                }
                depth -= 1;
                name.clear();
            }
            '\\' => {
                // Skip control words (e.g. `\revtbl`) and their trailing space.
                while let Some(next) = chars.peek() {
                    if next.is_ascii_alphanumeric() {
                        chars.next();
                    } else {
                        if *next == ' ' {
                            chars.next();
                        }
                        break;
                    }
                }
            }
            _ if depth >= 1 => {
                if ch != ';' {
                    name.push(ch);
                }
            }
            _ => {}
        }
    }
}

fn apply_pict_control(reader: &mut Reader, word: &str, param: Option<i32>) {
    let Some(pict) = reader.pict.as_mut() else { return };
    match word {
        "pngblip" => pict.is_png = true,
        "jpegblip" | "jpgblip" => pict.is_jpeg = true,
        "picwgoal" => pict.width_goal_pt = param.unwrap_or(0) as f64 / 20.0,
        "pichgoal" => pict.height_goal_pt = param.unwrap_or(0) as f64 / 20.0,
        _ => {}
    }
}

fn apply_control(reader: &mut Reader, word: &str, param: Option<i32>) {
    match word {
        "b" => reader.char_format.bold = param != Some(0),
        "i" => reader.char_format.italic = param != Some(0),
        "ul" => reader.char_format.underline = param != Some(0),
        "ulnone" => reader.char_format.underline = false,
        "strike" => reader.char_format.strike = param != Some(0),
        "super" => {
            reader.char_format.superscript = true;
            reader.char_format.subscript = false;
        }
        "sub" => {
            reader.char_format.subscript = true;
            reader.char_format.superscript = false;
        }
        "nosupersub" => {
            reader.char_format.superscript = false;
            reader.char_format.subscript = false;
        }
        "f" => {
            let index = param.unwrap_or(0).max(0) as usize;
            reader.char_format.font = reader.fonts.get(index).filter(|font| !font.is_empty()).cloned();
        }
        "fs" => {
            if let Some(value) = param {
                reader.char_format.size_pt = Some(value as f64 / 2.0);
            }
        }
        "cf" => reader.char_format.color = color_for(&reader.colors, param.unwrap_or(0)),
        "highlight" => reader.char_format.highlight = color_for(&reader.colors, param.unwrap_or(0)),
        "ql" => reader.para_format.align = "left".into(),
        "qc" => reader.para_format.align = "center".into(),
        "qr" => reader.para_format.align = "right".into(),
        "qj" => reader.para_format.align = "justify".into(),
        "li" => reader.para_format.indent_left_pt = param.unwrap_or(0) as f64 / 20.0,
        "ri" => reader.para_format.indent_right_pt = param.unwrap_or(0) as f64 / 20.0,
        "fi" => reader.para_format.first_line_pt = param.unwrap_or(0) as f64 / 20.0,
        "sb" => reader.para_format.space_before_pt = param.unwrap_or(0) as f64 / 20.0,
        "sa" => reader.para_format.space_after_pt = param.unwrap_or(0) as f64 / 20.0,
        "sl" => {
            if let Some(value) = param {
                if value > 1 {
                    reader.para_format.line_spacing = value as f64 / 240.0;
                }
            }
        }
        "line" => reader.push_text("\n"),
        "par" => {
            // A paragraph break inside a footnote destination separates note
            // paragraphs instead of flushing the body paragraph.
            if reader.note_depth.is_some() {
                reader.push_text("\n");
            } else {
                reader.flush_run();
            }
        }
        "tab" => reader.push_text("\t"),
        "page" => {
            reader.flush_run();
            reader.push_block(Block::PageBreak);
        }
        "pard" => {
            reader.para_format = ParaFormat { align: "left".into(), line_spacing: 1.15, ..Default::default() };
        }
        "plain" => reader.char_format = CharFormat::default(),
        "intbl" => {
            reader.in_table = true;
            reader.flush_run();
        }
        "cell" => {
            reader.flush_run();
            let blocks = std::mem::take(&mut reader.cell_blocks);
            reader.current_row.push(TableCell { blocks, ..Default::default() });
        }
        "row" => {
            let cells = std::mem::take(&mut reader.current_row);
            reader.in_table = false;
            if !cells.is_empty() {
                let row = TableRow { cells, height_pt: None, header: false };
                let appended = if let Some(Block::Table { table }) = reader.target_blocks().last_mut() {
                    table.rows.push(row.clone());
                    true
                } else {
                    false
                };
                if !appended {
                    reader.target_blocks().push(Block::Table { table: TableData { rows: vec![row], ..Default::default() } });
                }
            }
        }
        "s" => reader.para_format.style_index = param.unwrap_or(0).max(0) as usize,
        // Tracked changes: \revised/\deleted mark the text, \revauth indexes
        // the \revtbl author table and \revdttm carries the timestamp.
        "revised" => reader.start_revision("insert"),
        "deleted" => reader.start_revision("delete"),
        "revauth" => {
            let index = param.unwrap_or(0).max(0) as usize;
            reader.revauth = Some(index);
            let author = reader.revision_authors.get(index).cloned().unwrap_or_default();
            if let Some(mark) = reader.revision.as_mut() {
                if mark.author.is_empty() {
                    mark.author = author;
                }
            }
        }
        "revdttm" => {
            if let Some(value) = param {
                if let Some(mark) = reader.revision.as_mut() {
                    mark.date = dttm_to_iso(value as u32);
                }
            }
        }
        // Formatting revisions use \crauth/\crdate instead of \revised.
        "crauth" => reader.start_revision("format"),
        "crdate" => {
            if let Some(value) = param {
                if let Some(mark) = reader.revision.as_mut() {
                    mark.date = dttm_to_iso(value as u32);
                }
            }
        }
        _ => {}
    }
}

fn decode_pict(reader: &mut Reader) {
    let Some(pict) = reader.pict.take() else { return };
    let clean: String = pict.hex.chars().filter(|ch| ch.is_ascii_hexdigit()).collect();
    if clean.len() < 8 {
        return;
    }
    let chars: Vec<char> = clean.chars().collect();
    let mut bytes = Vec::with_capacity(chars.len() / 2);
    for pair in chars.chunks(2) {
        if pair.len() < 2 {
            break;
        }
        bytes.push(u8::from_str_radix(&format!("{}{}", pair[0], pair[1]), 16).unwrap_or(0));
    }
    let is_jpeg = pict.is_jpeg || bytes.starts_with(&[0xFF, 0xD8]);
    let mime = if is_jpeg { "image/jpeg" } else { "image/png" };
    let image = ImageData {
        name: format!("image.{}", if is_jpeg { "jpg" } else { "png" }),
        mime: mime.into(),
        data_base64: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes),
        alt: String::new(),
    };
    reader.flush_run();
    reader.push_block(Block::Image {
        image,
        width_pt: if pict.width_goal_pt > 1.0 { pict.width_goal_pt } else { 320.0 },
        height_pt: if pict.height_goal_pt > 1.0 { pict.height_goal_pt } else { 200.0 },
        align: "center".into(),
        caption: String::new(),
    });
}

/// Returns the index just past the `}` that closes the group starting at `start`.
fn skip_group(chars: &[char], mut index: usize) -> usize {
    let mut depth = 1i32;
    while index < chars.len() && depth > 0 {
        match chars[index] {
            '{' => depth += 1,
            '}' => depth -= 1,
            '\\' => index += 1,
            _ => {}
        }
        index += 1;
    }
    index
}

const IGNORABLE_GROUPS: [&str; 17] = [
    "fonttbl",
    "colortbl",
    "stylesheet",
    "info",
    "generator",
    "listtable",
    "listoverridetable",
    "rsidtbl",
    "xmlnstbl",
    "themedata",
    "colorschememapping",
    "latentstyles",
    "datastore",
    "mmathPr",
    "revtbl",
    "filetbl",
    "objdata",
];

pub fn read_rtf(bytes: &[u8]) -> OfficeResult<RtfRead> {
    let text = crate::zip::decode_utf8(bytes, "rtf")?;
    if !text.trim_start().starts_with("{\\rtf") {
        return Err(OfficeError::unsupported("The file does not look like RTF content."));
    }
    let mut reader = Reader::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    let mut word = String::new();

    while i < chars.len() {
        if reader.depth > 128 {
            break;
        }
        let ch = chars[i];
        match ch {
            '{' => {
                reader.flush_run();
                reader.depth += 1;
                reader.destination_stack.push(reader.destination);
                // Revision properties are scoped like character formatting;
                // remember the outer state so a `{\deleted ...}` group cannot
                // leak into the following text.
                reader.revision_stack.push(reader.revision.clone());
                i += 1;
            }
            '}' => {
                if reader.pict.is_some() {
                    decode_pict(&mut reader);
                    reader.close_revision_group();
                    reader.destination = reader.destination_stack.pop().unwrap_or(Destination::Body);
                    reader.depth = reader.depth.saturating_sub(1);
                    i += 1;
                    continue;
                }
                let closing_note = reader.note_depth == Some(reader.depth);
                reader.close_revision_group();
                if closing_note {
                    reader.finish_note();
                }
                reader.flush_run();
                reader.destination = reader.destination_stack.pop().unwrap_or(Destination::Body);
                reader.depth = reader.depth.saturating_sub(1);
                i += 1;
            }
            '\\' => {
                i += 1;
                if i >= chars.len() {
                    break;
                }
                let next = chars[i];
                if next == '\\' || next == '{' || next == '}' {
                    reader.push_text(&next.to_string());
                    i += 1;
                    continue;
                }
                if next == '\'' {
                    let mut hex = String::new();
                    for _ in 0..2 {
                        i += 1;
                        if i < chars.len() {
                            hex.push(chars[i]);
                        }
                    }
                    i += 1;
                    if let Ok(value) = u8::from_str_radix(&hex, 16) {
                        if let Ok(decoded) = crate::zip::decode_utf8(&[value], "text") {
                            reader.push_text(&decoded);
                        }
                    }
                    continue;
                }
                if next == '*' {
                    // Ignorable destinations are skipped, but two of them carry
                    // data we care about: \revtbl (revision authors) and our own
                    // \oswkendnote marker (endnote class).
                    let mut probe = i + 2;
                    let mut name = String::new();
                    while probe < chars.len() && chars[probe].is_ascii_alphabetic() {
                        name.push(chars[probe]);
                        probe += 1;
                    }
                    let end = skip_group(&chars, i + 1);
                    let content: String = if end > i + 1 { chars[(i + 1)..(end - 1)].iter().collect() } else { String::new() };
                    if name == "revtbl" {
                        parse_revtbl(&mut reader, &content);
                    } else if name == "oswkendnote" && reader.note_depth.is_some() {
                        reader.note_endnote = true;
                    }
                    i = end;
                    reader.close_revision_group();
                    reader.depth = reader.depth.saturating_sub(1);
                    reader.destination_stack.pop();
                    continue;
                }
                word.clear();
                let mut value: i64 = 0;
                let mut has_param = false;
                let mut negative = false;
                while i < chars.len() && chars[i].is_ascii_alphabetic() {
                    word.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() && (chars[i] == '-' || chars[i].is_ascii_digit()) {
                    if chars[i] == '-' {
                        negative = true;
                        i += 1;
                    }
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        value = value * 10 + (chars[i] as i64 - '0' as i64);
                        i += 1;
                    }
                    has_param = true;
                }
                if i < chars.len() && chars[i] == ' ' {
                    i += 1;
                }
                let param = if has_param { Some(if negative { -(value as i32) } else { value as i32 }) } else { None };

                if IGNORABLE_GROUPS.contains(&word.as_str()) {
                    let start = i;
                    let end = skip_group(&chars, i);
                    let content: String = if end > start { chars[start..end - 1].iter().collect() } else { String::new() };
                    match word.as_str() {
                        "colortbl" => parse_color_table(&mut reader, &content),
                        "fonttbl" => collect_font_names(&mut reader, &content),
                        "revtbl" => parse_revtbl(&mut reader, &content),
                        _ => {}
                    }
                    i = end;
                    reader.close_revision_group();
                    reader.depth = reader.depth.saturating_sub(1);
                    reader.destination_stack.pop();
                    continue;
                }

                match word.as_str() {
                    "pict" => reader.pict = Some(PictState::default()),
                    // A footnote destination: text is collected into note_runs
                    // and the reference run is placed when the group closes.
                    // The \chftn anchor inside is a control word, so it never
                    // reaches the paragraph text.
                    "footnote" => {
                        reader.note_runs.clear();
                        reader.note_endnote = false;
                        reader.note_depth = Some(reader.depth);
                    }
                    "header" | "headerl" | "headerr" | "headerf" => reader.destination = Destination::Header,
                    "footer" | "footerl" | "footerr" | "footerf" => reader.destination = Destination::Footer,
                    "bin" => {
                        if let Some(count) = param {
                            i = (i + count.max(0) as usize).min(chars.len());
                        }
                    }
                    "u" => {
                        if let Some(value) = param {
                            let code = if value < 0 { (value + 65536) as u32 } else { value as u32 };
                            if let Some(ch) = char::from_u32(code) {
                                reader.push_text(&ch.to_string());
                            }
                            reader.skip_unicode = 1;
                        }
                    }
                    "uc" => {
                        if let Some(value) = param {
                            reader.skip_unicode = value.max(0);
                        }
                    }
                    _ => {
                        if reader.pict.is_some() {
                            apply_pict_control(&mut reader, &word, param);
                        } else {
                            apply_control(&mut reader, &word, param);
                        }
                    }
                }
                continue;
            }
            '\r' | '\n' => {
                i += 1;
            }
            _ => {
                if reader.pict.is_some() {
                    if let Some(pict) = reader.pict.as_mut() {
                        if ch.is_ascii_hexdigit() || ch == ' ' {
                            pict.hex.push(ch);
                        }
                    }
                    i += 1;
                    continue;
                }
                if reader.skip_unicode > 0 && !ch.is_ascii_control() {
                    reader.skip_unicode -= 1;
                    i += 1;
                    continue;
                }
                let start = i;
                while i < chars.len() && !matches!(chars[i], '\\' | '{' | '}' | '\r' | '\n') {
                    i += 1;
                }
                if start == i {
                    i += 1;
                } else {
                    let slice: String = chars[start..i].iter().collect();
                    reader.push_text(&slice);
                }
            }
        }
    }

    reader.flush_run();
    if reader.blocks.is_empty() {
        reader.blocks.push(Block::paragraph(""));
    }
    let mut warnings = reader.warnings;
    if !reader.header_blocks.is_empty() || !reader.footer_blocks.is_empty() {
        warnings.push("RTF headers/footers were imported as document headers/footers.".into());
    }
    warnings.push("RTF import keeps text, basic formatting, tables and images; complex RTF features are simplified.".into());
    warnings.sort();
    warnings.dedup();
    let mut document = TextDocument::new_blank("Imported RTF");
    document.blocks = reader.blocks;
    document.header = reader.header_blocks;
    document.footer = reader.footer_blocks;
    document.footnotes = reader.footnotes;
    document.endnotes = reader.endnotes;
    Ok(RtfRead { document, warnings })
}

fn collect_font_names(reader: &mut Reader, raw: &str) {
    let mut current_index: Option<usize> = None;
    let mut name = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            let mut word = String::new();
            while let Some(next) = chars.peek() {
                if next.is_ascii_alphabetic() {
                    word.push(*next);
                    chars.next();
                } else {
                    break;
                }
            }
            let mut number = String::new();
            while let Some(next) = chars.peek() {
                if next.is_ascii_digit() {
                    number.push(*next);
                    chars.next();
                } else {
                    break;
                }
            }
            if word == "f" {
                if let Some(index) = current_index {
                    store_font(reader, index, &name);
                }
                current_index = number.parse::<usize>().ok();
                name.clear();
            }
        } else if ch == ';' {
            if let Some(index) = current_index {
                store_font(reader, index, &name);
            }
            name.clear();
            current_index = None;
        } else {
            name.push(ch);
        }
    }
    if let Some(index) = current_index {
        store_font(reader, index, &name);
    }
}

fn store_font(reader: &mut Reader, index: usize, name: &str) {
    while reader.fonts.len() <= index {
        reader.fonts.push(String::new());
    }
    if !name.trim().is_empty() {
        reader.fonts[index] = name.trim().to_string();
    }
}

pub fn read_rtf_file(path: &Path) -> OfficeResult<RtfRead> {
    let bytes = crate::io::read_bytes(path)?;
    read_rtf(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> TextDocument {
        let mut document = TextDocument::new_blank("Sample");
        document.blocks = vec![
            Block::Paragraph {
                props: ParaProps::default(),
                runs: vec![
                    Run { text: "Title ".into(), bold: true, ..Default::default() },
                    Run { text: "Türkçe karakterler: ğüşiöç ".into(), italic: true, color: Some("#FF0000".into()), ..Default::default() },
                ],
            },
            Block::Paragraph {
                props: ParaProps { align: "center".into(), ..Default::default() },
                runs: vec![Run { text: "Centered".into(), ..Default::default() }],
            },
            Block::Paragraph {
                props: ParaProps { list: Some(ListInfo { kind: "bullet".into(), level: 0, start: 1, marker: "•".into() }), ..Default::default() },
                runs: vec![Run { text: "item".into(), ..Default::default() }],
            },
        ];
        document
    }

    #[test]
    fn roundtrip_formatting() {
        let bytes = write_rtf(&sample()).unwrap();
        let read = read_rtf(&bytes).unwrap();
        let text = read.document.plain_text();
        assert!(text.contains("Title"), "text was {text}");
        assert!(text.contains("Türkçe"), "text was {text}");
        let bold = read.document.blocks.iter().find_map(|block| match block {
            Block::Paragraph { runs, .. } => runs.iter().find(|run| run.text.contains("Title")),
            _ => None,
        });
        assert!(bold.map(|run| run.bold).unwrap_or(false));
        let centered = read.document.blocks.iter().find_map(|block| match block {
            Block::Paragraph { props, runs } if runs.iter().any(|run| run.text.contains("Centered")) => Some(props.align.clone()),
            _ => None,
        });
        assert_eq!(centered.as_deref(), Some("center"));
    }

    #[test]
    fn exports_rtf_header() {
        let bytes = write_rtf(&sample()).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.starts_with("{\\rtf1"));
        assert!(text.contains("\\fonttbl"));
        assert!(text.contains("\\par"));
    }

    #[test]
    fn rejects_non_rtf() {
        assert!(read_rtf(b"hello world").is_err());
        let _ = read_rtf(b"{\\rtf1\\ansi");
    }

    #[test]
    fn roundtrip_table() {
        let mut document = TextDocument::new_blank("T");
        let mut table = TableData::simple(2, 2, 400.0);
        table.rows[0].cells[0].blocks = vec![Block::paragraph("A")];
        table.rows[1].cells[1].blocks = vec![Block::paragraph("B")];
        document.blocks = vec![Block::Table { table }];
        let bytes = write_rtf(&document).unwrap();
        let read = read_rtf(&bytes).unwrap();
        let text = read.document.plain_text();
        assert!(text.contains('A') && text.contains('B'), "text was {text}");
        let table = read.document.blocks.iter().find_map(|block| match block {
            Block::Table { table } => Some(table),
            _ => None,
        });
        assert!(table.is_some(), "table missing");
    }

    #[test]
    fn skips_ignorable_destinations() {
        let rtf = br"{\rtf1\ansi{\*\generator Riched20 10.0}{\fonttbl{\f0\fnil Calibri;}}\f0\fs24 Hello \b bold\b0 .\par}";
        let read = read_rtf(rtf).unwrap();
        let text = read.document.plain_text();
        assert!(text.contains("Hello"), "text was {text}");
        assert!(!text.contains("Riched20"));
    }

    #[test]
    fn roundtrip_notes_and_revisions() {
        let mut document = TextDocument::new_blank("Notes and revisions");
        document.footnotes = vec![Footnote { id: "fn-a".into(), runs: vec![Run { text: "First note".into(), ..Default::default() }], marker: String::new() }];
        document.endnotes = vec![Footnote { id: "en-a".into(), runs: vec![Run { text: "End note".into(), ..Default::default() }], marker: String::new() }];
        document.blocks = vec![Block::Paragraph {
            props: ParaProps::default(),
            runs: vec![
                Run { text: "Body ".into(), ..Default::default() },
                Run {
                    text: "inserted text".into(),
                    revision: Some(RevisionMark { id: "r1".into(), kind: "insert".into(), author: "Alice".into(), date: "2026-01-01T00:00:00Z".into(), original: None }),
                    ..Default::default()
                },
                Run { footnote: Some("fn-a".into()), ..Default::default() },
                Run {
                    text: "removed text".into(),
                    revision: Some(RevisionMark { id: "r2".into(), kind: "delete".into(), author: "Bob".into(), date: "2026-02-03T04:05:00Z".into(), original: None }),
                    ..Default::default()
                },
                Run { endnote: Some("en-a".into()), ..Default::default() },
            ],
        }];

        let bytes = write_rtf(&document).unwrap();
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(text.contains("{\\footnote \\chftn "), "text: {text}");
        assert!(text.contains("{\\*\\oswkendnote}"), "text: {text}");
        assert!(text.contains("{\\*\\revtbl {Unknown;}{Alice;}{Bob;}}"), "text: {text}");
        assert!(text.contains("{\\revised \\revauth1 \\revdttm"), "text: {text}");
        assert!(text.contains("{\\deleted \\revauth2 \\revdttm"), "text: {text}");
        // Both note classes are present, so Word placement stays untouched.
        assert!(!text.contains("\\aendnotes"));

        let read = read_rtf(&bytes).unwrap();
        assert_eq!(read.document.footnotes.len(), 1, "warnings: {:?}", read.warnings);
        assert_eq!(read.document.endnotes.len(), 1);
        assert!(read.document.footnotes[0].runs.iter().any(|run| run.text.contains("First note")));
        assert!(read.document.endnotes[0].runs.iter().any(|run| run.text.contains("End note")));
        assert_eq!(read.document.footnote_order().len(), 1);
        assert_eq!(read.document.endnote_order().len(), 1);

        let body = read.document.blocks.iter().map(Block::plain_text).collect::<Vec<_>>().join("\n");
        assert!(body.contains("Body"));
        assert!(body.contains("inserted text") && body.contains("removed text"), "body: {body}");
        assert!(!body.contains("First note") && !body.contains("End note"), "note body leaked: {body}");

        let revisions: Vec<(String, String, String)> = read
            .document
            .blocks
            .iter()
            .flat_map(|block| match block {
                Block::Paragraph { runs, .. } => runs
                    .iter()
                    .filter_map(|run| run.revision.as_ref().map(|revision| (revision.kind.clone(), revision.author.clone(), revision.date.clone())))
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            })
            .collect();
        assert!(
            revisions.iter().any(|(kind, author, date)| kind == "insert" && author == "Alice" && date == "2026-01-01T00:00:00Z"),
            "revisions: {revisions:?}"
        );
        assert!(
            revisions.iter().any(|(kind, author, date)| kind == "delete" && author == "Bob" && date == "2026-02-03T04:05:00Z"),
            "revisions: {revisions:?}"
        );
    }

    #[test]
    fn endnote_only_documents_request_endnote_placement() {
        let mut document = TextDocument::new_blank("Endnotes");
        document.endnotes = vec![Footnote { id: "en-a".into(), runs: vec![Run { text: "Only note".into(), ..Default::default() }], marker: String::new() }];
        document.blocks = vec![Block::Paragraph {
            props: ParaProps::default(),
            runs: vec![
                Run { text: "Body".into(), ..Default::default() },
                Run { endnote: Some("en-a".into()), ..Default::default() },
            ],
        }];
        let bytes = write_rtf(&document).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("\\aendnotes\\aenddoc"), "text: {text}");
    }

    #[test]
    fn parses_hand_written_footnote_and_deletion() {
        let rtf = br"{\rtf1\ansi{\*\revtbl{Unknown;}{Alice;}}{\fonttbl{\f0\fnil Calibri;}}\f0\fs24 Body text{\footnote \chftn Note from fixture} more {\deleted\revauth1 removed by Alice} tail\par}";
        let read = read_rtf(rtf).unwrap();
        assert_eq!(read.document.footnotes.len(), 1);
        assert!(read.document.endnotes.is_empty());
        assert!(read.document.footnotes[0].runs.iter().any(|run| run.text.contains("Note from fixture")));
        assert_eq!(read.document.footnote_order().len(), 1);
        let body = read.document.plain_text();
        assert!(body.contains("Body text"), "body: {body}");
        assert!(!body.contains("Note from fixture"), "note body leaked: {body}");
        let (revision, text) = read
            .document
            .blocks
            .iter()
            .find_map(|block| match block {
                Block::Paragraph { runs, .. } => runs
                    .iter()
                    .find_map(|run| run.revision.clone().map(|revision| (revision, run.text.clone()))),
                _ => None,
            })
            .expect("a deleted run should carry a delete revision");
        assert_eq!(revision.kind, "delete");
        assert_eq!(revision.author, "Alice");
        assert!(text.contains("removed by Alice"), "text: {text}");
    }
}


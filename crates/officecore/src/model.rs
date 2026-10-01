//! Shared document model for Writer, Calc and Impress.
//!
//! The model is the in-app source of truth. File formats (DOCX, ODT, XLSX,
//! PPTX, ...) are import/export targets; the native `.oswk` unit format is the
//! model serialized as JSON, so a round-trip never loses anything the suite
//! understands. Fields are optional/defaulted so older files keep opening when
//! the model grows.

use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MM_TO_PT: f64 = 72.0 / 25.4;
pub const PT_TO_MM: f64 = 25.4 / 72.0;
/// CSS pixels at the conventional 96 dpi used by the editors.
pub const PT_TO_PX: f64 = 96.0 / 72.0;
pub const PX_TO_PT: f64 = 72.0 / 96.0;

pub fn pt_to_px(pt: f64) -> f64 {
    pt * PT_TO_PX
}

pub fn px_to_pt(px: f64) -> f64 {
    px * PX_TO_PT
}

// ---------------------------------------------------------------------------
// Shared
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct DocMetadata {
    pub title: String,
    pub author: String,
    pub subject: String,
    pub keywords: String,
    pub creator: String,
    pub last_modified_by: String,
    pub created: String,
    pub modified: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ImageData {
    pub name: String,
    pub mime: String,
    pub data_base64: String,
    pub alt: String,
}

impl Default for ImageData {
    fn default() -> Self {
        Self { name: "image".into(), mime: "image/png".into(), data_base64: String::new(), alt: String::new() }
    }
}

impl ImageData {
    pub fn from_bytes(name: &str, bytes: &[u8]) -> Self {
        let mime = guess_mime(name, bytes);
        Self {
            name: name.to_string(),
            mime,
            data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
            alt: String::new(),
        }
    }

    pub fn from_path(path: &std::path::Path) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "image".into());
        Ok(Self::from_bytes(&name, &bytes))
    }

    pub fn bytes(&self) -> Vec<u8> {
        base64::engine::general_purpose::STANDARD.decode(self.data_base64.as_bytes()).unwrap_or_default()
    }

    pub fn is_empty(&self) -> bool {
        self.data_base64.is_empty()
    }

    /// Intrinsic size in pixels (0,0 when the format is unknown to the decoder).
    pub fn pixel_size(&self) -> (u32, u32) {
        image::load_from_memory(&self.bytes())
            .map(|image| (image.width(), image.height()))
            .unwrap_or((0, 0))
    }

    pub fn extension(&self) -> &'static str {
        match self.mime.as_str() {
            "image/jpeg" | "image/jpg" => "jpg",
            "image/gif" => "gif",
            "image/bmp" => "bmp",
            "image/tiff" => "tiff",
            "image/webp" => "webp",
            "image/svg+xml" => "svg",
            _ => "png",
        }
    }
}

pub fn guess_mime(name: &str, bytes: &[u8]) -> String {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        return "image/jpeg".into();
    }
    if lower.ends_with(".gif") {
        return "image/gif".into();
    }
    if lower.ends_with(".bmp") {
        return "image/bmp".into();
    }
    if lower.ends_with(".webp") {
        return "image/webp".into();
    }
    if lower.ends_with(".svg") {
        return "image/svg+xml".into();
    }
    if lower.ends_with(".png") {
        return "image/png".into();
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        return "image/jpeg".into();
    }
    if bytes.starts_with(b"\x89PNG") {
        return "image/png".into();
    }
    "image/png".into()
}

/// A cell-based anchor used by floating sheet objects (images).
///
/// `address` is the top-left cell (`D2`); the offsets are the EMU distances
/// from that cell's top-left corner, exactly as an OOXML drawing stores them.
/// `to_*` is only filled when the source anchor was a `twoCellAnchor`, so a
/// package written elsewhere keeps its bottom-right corner even though the
/// exporter itself emits `oneCellAnchor` from `width_px`/`height_px`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CellAnchor {
    pub address: String,
    pub col_off_emu: i64,
    pub row_off_emu: i64,
    #[serde(default)]
    pub to_address: Option<String>,
    #[serde(default)]
    pub to_col_off_emu: i64,
    #[serde(default)]
    pub to_row_off_emu: i64,
}

impl CellAnchor {
    /// A bare anchor with no offsets, e.g. `A1`.
    pub fn at(address: &str) -> Self {
        Self { address: address.to_string(), ..Default::default() }
    }
}

// ---------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PageSetup {
    pub size: String,
    pub width_pt: f64,
    pub height_pt: f64,
    pub orientation: String,
    pub margin_top_pt: f64,
    pub margin_right_pt: f64,
    pub margin_bottom_pt: f64,
    pub margin_left_pt: f64,
    pub columns: u32,
    pub column_spacing_pt: f64,
    pub header_distance_pt: f64,
    pub footer_distance_pt: f64,
    pub different_first_page: bool,
}

impl Default for PageSetup {
    fn default() -> Self {
        Self {
            size: "a4".into(),
            width_pt: 595.28,
            height_pt: 841.89,
            orientation: "portrait".into(),
            margin_top_pt: 72.0,
            margin_right_pt: 72.0,
            margin_bottom_pt: 72.0,
            margin_left_pt: 72.0,
            columns: 1,
            column_spacing_pt: 24.0,
            header_distance_pt: 36.0,
            footer_distance_pt: 36.0,
            different_first_page: false,
        }
    }
}

impl PageSetup {
    pub fn from_preset(size: &str, orientation: &str) -> Self {
        let (width, height) = match size {
            "a5" => (419.53, 595.28),
            "letter" => (612.0, 792.0),
            "legal" => (612.0, 1008.0),
            "a3" => (841.89, 1190.55),
            _ => (595.28, 841.89),
        };
        let landscape = orientation == "landscape";
        Self {
            size: size.to_string(),
            width_pt: if landscape { height } else { width },
            height_pt: if landscape { width } else { height },
            orientation: orientation.to_string(),
            ..Default::default()
        }
    }

    pub fn apply_orientation(&mut self, orientation: &str) {
        let landscape = orientation == "landscape";
        let currently_landscape = self.width_pt > self.height_pt;
        if landscape != currently_landscape {
            std::mem::swap(&mut self.width_pt, &mut self.height_pt);
        }
        self.orientation = orientation.to_string();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ParaStyle {
    pub id: String,
    pub name: String,
    pub based_on: Option<String>,
    pub next: Option<String>,
    pub font: Option<String>,
    pub size_pt: Option<f64>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strike: Option<bool>,
    pub color: Option<String>,
    pub highlight: Option<String>,
    pub align: Option<String>,
    pub line_spacing: Option<f64>,
    pub space_before_pt: Option<f64>,
    pub space_after_pt: Option<f64>,
    pub indent_left_pt: Option<f64>,
    pub indent_right_pt: Option<f64>,
    pub first_line_pt: Option<f64>,
    pub outline_level: Option<u32>,
    pub keep_with_next: Option<bool>,
    pub page_break_before: Option<bool>,
}

impl Default for ParaStyle {
    fn default() -> Self {
        Self {
            id: "Normal".into(),
            name: "Normal".into(),
            based_on: None,
            next: None,
            font: None,
            size_pt: None,
            bold: None,
            italic: None,
            underline: None,
            strike: None,
            color: None,
            highlight: None,
            align: None,
            line_spacing: None,
            space_before_pt: None,
            space_after_pt: None,
            indent_left_pt: None,
            indent_right_pt: None,
            first_line_pt: None,
            outline_level: None,
            keep_with_next: None,
            page_break_before: None,
        }
    }
}

/// The style catalogue every Writer document starts with (fully original design).
pub fn default_styles() -> Vec<ParaStyle> {
    let mut list = Vec::new();
    let normal = ParaStyle {
        font: Some("Calibri".into()),
        size_pt: Some(11.0),
        line_spacing: Some(1.15),
        space_after_pt: Some(8.0),
        color: Some("#1f2328".into()),
        ..Default::default()
    };
    list.push(normal);

    let mut title = ParaStyle { id: "Title".into(), name: "Title".into(), based_on: Some("Normal".into()), next: Some("Subtitle".into()), ..Default::default() };
    title.font = Some("Calibri Light".into());
    title.size_pt = Some(28.0);
    title.bold = Some(true);
    title.color = Some("#0f172a".into());
    title.space_after_pt = Some(6.0);
    list.push(title);

    let mut subtitle = ParaStyle { id: "Subtitle".into(), name: "Subtitle".into(), based_on: Some("Normal".into()), next: Some("Normal".into()), ..Default::default() };
    subtitle.size_pt = Some(15.0);
    subtitle.italic = Some(true);
    subtitle.color = Some("#475569".into());
    subtitle.space_after_pt = Some(14.0);
    list.push(subtitle);

    for (index, size) in [(1u32, 20.0f64), (2, 16.0), (3, 13.0), (4, 11.5), (5, 11.0), (6, 10.5)] {
        let mut heading = ParaStyle {
            id: format!("Heading{index}"),
            name: format!("Heading {index}"),
            based_on: Some("Normal".into()),
            next: Some("Normal".into()),
            ..Default::default()
        };
        heading.font = Some("Calibri Light".into());
        heading.size_pt = Some(size);
        heading.bold = Some(true);
        heading.color = Some(if index == 1 { "#1d4ed8".into() } else { "#334155".to_string() });
        heading.space_before_pt = Some(if index == 1 { 16.0 } else { 12.0 });
        heading.space_after_pt = Some(4.0);
        heading.keep_with_next = Some(true);
        heading.outline_level = Some(index - 1);
        list.push(heading);
    }

    let mut quote = ParaStyle { id: "Quote".into(), name: "Quote".into(), based_on: Some("Normal".into()), ..Default::default() };
    quote.italic = Some(true);
    quote.color = Some("#334155".into());
    quote.indent_left_pt = Some(24.0);
    quote.indent_right_pt = Some(24.0);
    quote.space_before_pt = Some(8.0);
    quote.space_after_pt = Some(8.0);
    list.push(quote);

    let mut caption = ParaStyle { id: "Caption".into(), name: "Caption".into(), based_on: Some("Normal".into()), ..Default::default() };
    caption.size_pt = Some(9.5);
    caption.italic = Some(true);
    caption.align = Some("center".into());
    caption.color = Some("#64748b".into());
    list.push(caption);

    let mut code = ParaStyle { id: "Code".into(), name: "Code".into(), based_on: Some("Normal".into()), ..Default::default() };
    code.font = Some("Consolas".into());
    code.size_pt = Some(10.0);
    code.space_after_pt = Some(0.0);
    list.push(code);

    list
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ListInfo {
    pub kind: String,
    pub level: u32,
    pub start: u32,
    pub marker: String,
}

impl Default for ListInfo {
    fn default() -> Self {
        Self { kind: "bullet".into(), level: 0, start: 1, marker: "•".into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ParaProps {
    pub style: String,
    pub align: String,
    pub line_spacing: f64,
    pub space_before_pt: f64,
    pub space_after_pt: f64,
    pub indent_left_pt: f64,
    pub indent_right_pt: f64,
    pub first_line_pt: f64,
    pub list: Option<ListInfo>,
    pub page_break_before: bool,
    /// Pagination rules written to DOCX as `w:keepNext` / `w:keepLines`.
    #[serde(default)]
    pub keep_with_next: bool,
    #[serde(default)]
    pub keep_together: bool,
}

impl Default for ParaProps {
    fn default() -> Self {
        Self {
            style: "Normal".into(),
            align: "left".into(),
            line_spacing: 1.15,
            space_before_pt: 0.0,
            space_after_pt: 8.0,
            indent_left_pt: 0.0,
            indent_right_pt: 0.0,
            first_line_pt: 0.0,
            list: None,
            page_break_before: false,
            keep_with_next: false,
            keep_together: false,
        }
    }
}

/// The formatting a run carried before a tracked formatting change.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RunFormat {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub color: Option<String>,
    pub highlight: Option<String>,
    pub font: Option<String>,
    pub size_pt: Option<f64>,
}

/// A tracked revision attached to a run.
///
/// `kind` is `insert`, `delete` or `format`. Deleted text stays in the model
/// (shown struck through while revisions are visible) until the revision is
/// accepted or rejected, which is what makes accept/reject lossless.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RevisionMark {
    pub id: String,
    pub kind: String,
    pub author: String,
    pub date: String,
    /// Formatting before the change; only set for `format` revisions.
    pub original: Option<RunFormat>,
}

/// A resolved document field, e.g. a cross reference or a date field.
///
/// `kind` is one of `page`, `pages`, `date`, `time`, `title`, `author`,
/// `ref`, `refPage`, `footnote`, `bookmark`, `figure`, `table`. `target`
/// names the bookmark or footnote the field points at; `cached` is the last
/// rendered value so the field never has to be recomputed to be displayed.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct FieldRef {
    pub kind: String,
    pub target: String,
    pub cached: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub color: Option<String>,
    pub highlight: Option<String>,
    pub font: Option<String>,
    pub size_pt: Option<f64>,
    pub link: Option<String>,
    pub comment: Option<String>,
    pub superscript: bool,
    pub subscript: bool,
    /// Footnote id this run is the reference for.
    #[serde(default)]
    pub footnote: Option<String>,
    /// Endnote id this run is the reference for.
    #[serde(default)]
    pub endnote: Option<String>,
    /// A document field rendered at this position (page number, cross
    /// reference, date, ...).
    #[serde(default)]
    pub field: Option<FieldRef>,
    /// Tracked revision, when the run was inserted, deleted or reformatted
    /// while suggest mode was on.
    #[serde(default)]
    pub revision: Option<RevisionMark>,
}

impl Run {
    /// True when the run is a tracked deletion (kept in the model until the
    /// revision is accepted or rejected).
    pub fn is_deleted(&self) -> bool {
        self.revision.as_ref().map(|revision| revision.kind == "delete").unwrap_or(false)
    }

    /// Builds a `format` revision mark capturing the run's current formatting.
    pub fn format_snapshot(&self) -> RunFormat {
        RunFormat {
            bold: self.bold,
            italic: self.italic,
            underline: self.underline,
            strike: self.strike,
            color: self.color.clone(),
            highlight: self.highlight.clone(),
            font: self.font.clone(),
            size_pt: self.size_pt,
        }
    }

    pub fn apply_format(&mut self, format: &RunFormat) {
        self.bold = format.bold;
        self.italic = format.italic;
        self.underline = format.underline;
        self.strike = format.strike;
        self.color = format.color.clone();
        self.highlight = format.highlight.clone();
        self.font = format.font.clone();
        self.size_pt = format.size_pt;
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TableCell {
    pub blocks: Vec<Block>,
    pub colspan: u32,
    pub rowspan: u32,
    pub background: Option<String>,
    pub align: String,
    pub valign: String,
    pub width_pt: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TableRow {
    pub cells: Vec<TableCell>,
    pub height_pt: Option<f64>,
    pub header: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TableData {
    pub rows: Vec<TableRow>,
    pub column_widths_pt: Vec<f64>,
    pub borders: bool,
    pub border_color: String,
    pub align: String,
}

impl TableData {
    pub fn simple(rows: u32, columns: u32, width_pt: f64) -> Self {
        let column_width = if columns > 0 { width_pt / columns as f64 } else { width_pt };
        let table_rows = (0..rows)
            .map(|row| TableRow {
                cells: (0..columns).map(|_| TableCell::default()).collect(),
                height_pt: None,
                header: row == 0,
            })
            .collect();
        Self {
            rows: table_rows,
            column_widths_pt: vec![column_width; columns as usize],
            borders: true,
            border_color: "#94a3b8".into(),
            align: "left".into(),
        }
    }
}

/// One line of a table of contents.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TocEntry {
    pub text: String,
    pub level: u32,
    pub page: u32,
    pub anchor: u32,
}

/// Properties of one Writer section.
///
/// A section owns its page setup, headers and footers. The document-level
/// `page` / `header` / `footer` fields are the **first** section; every
/// [`Block::SectionBreak`] starts the next one and carries its own copy, so a
/// document with no breaks behaves exactly as it did before V3.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SectionProps {
    pub page: PageSetup,
    pub header: Vec<Block>,
    pub footer: Vec<Block>,
    pub first_header: Vec<Block>,
    pub first_footer: Vec<Block>,
    pub even_header: Vec<Block>,
    pub even_footer: Vec<Block>,
    pub different_first_page: bool,
    pub different_odd_even: bool,
    pub columns: u32,
    /// How the section starts: `newPage`, `continuous`, `oddPage` or `evenPage`.
    pub start: String,
}

impl Default for SectionProps {
    fn default() -> Self {
        Self {
            page: PageSetup::default(),
            header: Vec::new(),
            footer: Vec::new(),
            first_header: Vec::new(),
            first_footer: Vec::new(),
            even_header: Vec::new(),
            even_footer: Vec::new(),
            different_first_page: false,
            different_odd_even: false,
            columns: 1,
            start: "newPage".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Block {
    Paragraph { props: ParaProps, runs: Vec<Run> },
    Table { table: TableData },
    Image { image: ImageData, width_pt: f64, height_pt: f64, align: String, caption: String },
    PageBreak,
    Rule,
    /// A table of contents whose entries were last updated in the editor; the
    /// layout and DOCX export render them as static text.
    Toc { #[serde(default)] entries: Vec<TocEntry> },
    /// Starts a new section. The block itself renders no content; the section
    /// it carries applies from this point until the next break (or the end of
    /// the document).
    SectionBreak { #[serde(default)] section: SectionProps },
}

impl Default for Block {
    fn default() -> Self {
        Block::Paragraph { props: ParaProps::default(), runs: vec![Run::default()] }
    }
}

impl Block {
    pub fn paragraph(text: &str) -> Self {
        Block::Paragraph {
            props: ParaProps::default(),
            runs: vec![Run { text: text.to_string(), ..Default::default() }],
        }
    }

    pub fn heading(text: &str, level: u32) -> Self {
        let props = ParaProps { style: format!("Heading{level}"), ..Default::default() };
        Block::Paragraph { props, runs: vec![Run { text: text.to_string(), ..Default::default() }] }
    }

    pub fn plain_text(&self) -> String {
        match self {
            Block::Paragraph { runs, .. } => runs.iter().map(|run| run.text.as_str()).collect::<Vec<_>>().join(""),
            Block::Table { table } => table
                .rows
                .iter()
                .map(|row| {
                    row.cells
                        .iter()
                        .map(|cell| cell.blocks.iter().map(Block::plain_text).collect::<Vec<_>>().join(" "))
                        .collect::<Vec<_>>()
                        .join("\t")
                })
                .collect::<Vec<_>>()
                .join("\n"),
            Block::Image { caption, .. } => caption.clone(),
            Block::PageBreak => "\n".into(),
            Block::Rule => "".into(),
            Block::Toc { entries } => entries.iter().map(|entry| entry.text.clone()).collect::<Vec<_>>().join("\n"),
            Block::SectionBreak { .. } => "".into(),
        }
    }

    /// True when the block is a section break.
    pub fn is_section_break(&self) -> bool {
        matches!(self, Block::SectionBreak { .. })
    }
}

/// A footnote or endnote. `runs` is the note body; numbering is automatic by
/// position, so inserting a note in the middle renumbers later notes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Footnote {
    pub id: String,
    pub runs: Vec<Run>,
    /// Optional explicit marker; empty means automatic numbering.
    pub marker: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Bookmark {
    pub id: String,
    pub name: String,
    pub block: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CommentReply {
    pub author: String,
    pub text: String,
    pub created: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Comment {
    pub id: String,
    pub author: String,
    pub text: String,
    pub created: String,
    pub resolved: bool,
    #[serde(default)]
    pub modified: String,
    #[serde(default)]
    pub replies: Vec<CommentReply>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TextDocument {
    pub id: String,
    pub title: String,
    /// The final section's page setup. Section breaks override it from their
    /// position onwards.
    pub page: PageSetup,
    pub styles: Vec<ParaStyle>,
    pub blocks: Vec<Block>,
    pub header: Vec<Block>,
    pub footer: Vec<Block>,
    pub comments: Vec<Comment>,
    pub metadata: DocMetadata,
    /// Footnotes, numbered in the order their references first appear.
    #[serde(default)]
    pub footnotes: Vec<Footnote>,
    #[serde(default)]
    pub endnotes: Vec<Footnote>,
    #[serde(default)]
    pub bookmarks: Vec<Bookmark>,
    /// Suggest mode: edits are recorded as revisions instead of being applied.
    #[serde(default)]
    pub track_changes: bool,
    /// Whether pending revisions are rendered in the editor.
    #[serde(default = "default_true")]
    pub show_revisions: bool,
}

fn default_true() -> bool {
    true
}

impl Default for TextDocument {
    fn default() -> Self {
        Self {
            id: String::new(),
            title: "Untitled document".into(),
            page: PageSetup::default(),
            styles: default_styles(),
            blocks: vec![Block::paragraph("")],
            header: Vec::new(),
            footer: Vec::new(),
            comments: Vec::new(),
            metadata: DocMetadata::default(),
            footnotes: Vec::new(),
            endnotes: Vec::new(),
            bookmarks: Vec::new(),
            track_changes: false,
            show_revisions: true,
        }
    }
}

impl TextDocument {
    pub fn new_blank(title: &str) -> Self {
        Self { id: uuid::Uuid::new_v4().to_string(), title: title.to_string(), ..Default::default() }
    }

    pub fn plain_text(&self) -> String {
        self.blocks.iter().map(Block::plain_text).collect::<Vec<_>>().join("\n")
    }

    pub fn word_count(&self) -> usize {
        self.plain_text().split_whitespace().count()
    }

    /// The section properties in effect for block `block_index`: the most
    /// recent section break at or before it, or the first section.
    pub fn section_for_block(&self, block_index: usize) -> SectionProps {
        let mut current = self.first_section();
        for (index, block) in self.blocks.iter().enumerate() {
            if index > block_index {
                break;
            }
            if let Block::SectionBreak { section } = block {
                current = section.clone();
            }
        }
        current
    }

    /// The first section (document-level page setup, headers and footers).
    pub fn first_section(&self) -> SectionProps {
        SectionProps {
            page: self.page.clone(),
            header: self.header.clone(),
            footer: self.footer.clone(),
            first_header: Vec::new(),
            first_footer: Vec::new(),
            even_header: Vec::new(),
            even_footer: Vec::new(),
            different_first_page: self.page.different_first_page,
            different_odd_even: false,
            columns: self.page.columns,
            start: "newPage".into(),
        }
    }

    /// All section properties in document order: the first section is the
    /// document-level page setup, every section break starts the next one.
    pub fn all_sections(&self) -> Vec<SectionProps> {
        let mut sections = vec![self.first_section()];
        sections.extend(self.blocks.iter().filter_map(|block| match block {
            Block::SectionBreak { section } => Some(section.clone()),
            _ => None,
        }));
        sections
    }

    /// Footnote ids in reference order across the whole document.
    pub fn footnote_order(&self) -> Vec<String> {
        let mut order = Vec::new();
        for block in &self.blocks {
            collect_note_refs(block, &mut order, false);
        }
        order
    }

    pub fn endnote_order(&self) -> Vec<String> {
        let mut order = Vec::new();
        for block in &self.blocks {
            collect_note_refs(block, &mut order, true);
        }
        order
    }

    /// The automatic number of a footnote (1-based), or None when unknown.
    pub fn footnote_number(&self, id: &str) -> Option<usize> {
        self.footnote_order().iter().position(|candidate| candidate == id).map(|index| index + 1)
    }

    pub fn endnote_number(&self, id: &str) -> Option<usize> {
        self.endnote_order().iter().position(|candidate| candidate == id).map(|index| index + 1)
    }
}

fn collect_note_refs(block: &Block, out: &mut Vec<String>, endnotes: bool) {
    match block {
        Block::Paragraph { runs, .. } => {
            for run in runs {
                let reference = if endnotes { run.endnote.as_ref() } else { run.footnote.as_ref() };
                if let Some(id) = reference {
                    if !out.iter().any(|candidate| candidate == id) {
                        out.push(id.clone());
                    }
                }
            }
        }
        Block::Table { table } => {
            for row in &table.rows {
                for cell in &row.cells {
                    for block in &cell.blocks {
                        collect_note_refs(block, out, endnotes);
                    }
                }
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Calc
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
#[derive(Default)]
pub enum CellValue {
    #[default]
    Empty,
    Number(f64),
    Text(String),
    Bool(bool),
    Error(String),
}


#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct BorderStyle {
    pub style: String,
    pub color: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CellBorders {
    pub top: Option<BorderStyle>,
    pub right: Option<BorderStyle>,
    pub bottom: Option<BorderStyle>,
    pub left: Option<BorderStyle>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CellStyle {
    pub font: Option<String>,
    pub size_pt: Option<f64>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub color: Option<String>,
    pub fill: Option<String>,
    pub align: String,
    pub valign: String,
    pub wrap: bool,
    pub rotation: i32,
    pub borders: CellBorders,
    pub number_format: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Cell {
    pub value: CellValue,
    pub formula: Option<String>,
    pub style: CellStyle,
    pub comment: Option<String>,
    /// Hyperlink target; the cell text is the label.
    pub link: Option<String>,
}

/// Paper, orientation and print options for one sheet.
///
/// Mirrors the `pageSetup`/`printOptions`/`printMargins`/`headerFooter` parts
/// of an XLSX so a print-ready sheet survives a round trip through the native
/// format. The V3.1 fields are covered by the container-level
/// `#[serde(default)]`, so older `.oswk` units load unchanged and pull the
/// intended defaults (not zero) from [`PrintSettings::default`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PrintSettings {
    /// Excel paper size code; 9 is A4, 1 is Letter.
    pub paper_size: u32,
    pub landscape: bool,
    /// Percentage scale, 10..400.
    pub scale: u32,
    pub fit_to_width: u32,
    pub fit_to_height: u32,
    pub center_horizontally: bool,
    #[serde(default)]
    pub center_vertically: bool,
    pub print_gridlines: bool,
    pub print_headings: bool,
    /// Row number repeated at the top of every page, e.g. "1:1".
    pub print_titles_rows: Option<String>,
    /// Column letter repeated at the left of every page, e.g. "A:A".
    #[serde(default)]
    pub print_titles_cols: Option<String>,
    /// The printed range as a relative A1 range (`A1:D40`), from
    /// `_xlnm.Print_Area`.
    #[serde(default)]
    pub print_area: Option<String>,
    pub different_first_page: bool,
    pub different_odd_even: bool,
    pub header: String,
    pub footer: String,
    /// Page margins in inches, as OOXML stores them. Missing keys in an older
    /// unit fall back to the container-level `Default`, not to zero.
    pub margin_left: f64,
    pub margin_right: f64,
    pub margin_top: f64,
    pub margin_bottom: f64,
    pub margin_header: f64,
    pub margin_footer: f64,
    /// `firstHeader`/`firstFooter`/`evenHeader`/`evenFooter` format strings,
    /// active when the matching `different_*` flag is set. The odd header and
    /// footer stay in `header`/`footer`.
    #[serde(default)]
    pub first_header: String,
    #[serde(default)]
    pub first_footer: String,
    #[serde(default)]
    pub even_header: String,
    #[serde(default)]
    pub even_footer: String,
    /// Manual horizontal page breaks as 0-based row indexes (`<rowBreaks>`).
    #[serde(default)]
    pub row_breaks: Vec<u32>,
    /// Manual vertical page breaks as 0-based column indexes (`<colBreaks>`).
    #[serde(default)]
    pub col_breaks: Vec<u32>,
}

impl Default for PrintSettings {
    fn default() -> Self {
        Self {
            paper_size: 9,
            landscape: false,
            scale: 100,
            fit_to_width: 1,
            fit_to_height: 0,
            center_horizontally: false,
            center_vertically: false,
            print_gridlines: false,
            print_headings: false,
            print_titles_rows: None,
            print_titles_cols: None,
            print_area: None,
            different_first_page: false,
            different_odd_even: false,
            header: String::new(),
            footer: String::new(),
            margin_left: 0.7,
            margin_right: 0.7,
            margin_top: 0.75,
            margin_bottom: 0.75,
            margin_header: 0.3,
            margin_footer: 0.3,
            first_header: String::new(),
            first_footer: String::new(),
            even_header: String::new(),
            even_footer: String::new(),
            row_breaks: Vec::new(),
            col_breaks: Vec::new(),
        }
    }
}

impl Cell {
    pub fn is_empty(&self) -> bool {
        matches!(self.value, CellValue::Empty)
            && self.formula.is_none()
            && self.style == CellStyle::default()
            && self.comment.is_none()
            // A hyperlink on a blank cell is real content (and exactly what the
            // XLSX importer produces); treating it as empty dropped the link on
            // the next export.
            && self.link.is_none()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct MergeRange {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ChartSeries {
    pub name: String,
    pub range: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ChartData {
    pub kind: String,
    pub title: String,
    pub categories: String,
    pub series: Vec<ChartSeries>,
    pub legend: bool,
    pub x_title: String,
    pub y_title: String,
    pub stacked: bool,
    pub show_labels: bool,
    /// Cached category labels (ChartML `c:strCache`). Empty means the chart only
    /// carries ranges, which is how documents written before V3.1 import.
    #[serde(default)]
    pub categories_cache: Vec<String>,
    /// Cached series values aligned with `series` (ChartML `c:numCache`); an
    /// empty inner vector means that series only carries a range.
    #[serde(default)]
    pub series_values_cache: Vec<Vec<f64>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ChartPlacement {
    pub id: String,
    pub chart: ChartData,
    pub anchor: String,
    pub width_px: f64,
    pub height_px: f64,
}

/// A picture floating over a worksheet.
///
/// The bytes live in [`ImageData`]; the anchor and size mirror the `xdr:pic`
/// in the drawing part so an XLSX import/export round trip is stable.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SheetImage {
    pub image: ImageData,
    pub anchor: CellAnchor,
    pub width_px: f64,
    pub height_px: f64,
    /// Clockwise rotation in degrees, as written to `a:xfrm/@rot`.
    pub rotation_deg: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CondRule {
    pub id: String,
    pub range: String,
    pub kind: String,
    pub values: Vec<String>,
    pub fill: Option<String>,
    pub color: Option<String>,
    pub top_n: Option<u32>,
    pub stop_if_true: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Validation {
    pub id: String,
    pub range: String,
    pub kind: String,
    pub values: Vec<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub message: String,
    pub allow_blank: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct FilterState {
    pub range: String,
    pub column: u32,
    pub values: Vec<String>,
}

/// One aggregated column of a pivot table.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PivotValueField {
    pub field: String,
    pub aggregation: String,
}

/// A filter on one pivot source field; an empty list keeps everything.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PivotFilter {
    pub field: String,
    pub values: Vec<String>,
}

/// A pivot table definition over a cell range whose first row is headers.
///
/// The definition is the source of truth in `.oswk`; XLSX export materialises
/// the computed grid as plain values at `anchor` and reports that the result
/// is not a live Excel pivot table.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PivotTable {
    pub id: String,
    pub name: String,
    pub source_sheet: String,
    pub source: String,
    pub rows: Vec<String>,
    pub columns: Vec<String>,
    pub values: Vec<PivotValueField>,
    pub filters: Vec<PivotFilter>,
    pub anchor: String,
}

/// One column of a spreadsheet table. A calculated column carries a formula;
/// the formula is written to every body cell of the column and exported as a
/// real XLSX calculated column.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TableColumn {
    pub name: String,
    pub formula: Option<String>,
}

/// A structured spreadsheet table (the Excel "ListObject"): a named range with
/// a header row, an optional totals row, banded rows, an optional filter and
/// calculated columns. Structured references such as `Sales[Amount]` resolve
/// against `name` and the column names.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SpreadsheetTable {
    pub id: String,
    pub name: String,
    /// The full table range including the header and totals rows, e.g. `A1:D21`.
    pub range: String,
    pub has_headers: bool,
    pub has_totals: bool,
    pub banded_rows: bool,
    pub banded_columns: bool,
    pub header_fill: Option<String>,
    pub header_bold: bool,
    /// Optional built-in table style name (kept for XLSX round trips).
    pub style_name: String,
    pub columns: Vec<TableColumn>,
    /// The table's own filter state; `None` means no filter buttons.
    pub filter: Option<FilterState>,
}

impl SpreadsheetTable {
    pub fn new(name: &str, range: &str, columns: Vec<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            range: range.to_string(),
            has_headers: true,
            has_totals: false,
            banded_rows: true,
            banded_columns: false,
            header_fill: Some("#1D4ED8".into()),
            header_bold: true,
            style_name: "TableStyleMedium2".into(),
            columns: columns.into_iter().map(|name| TableColumn { name, formula: None }).collect(),
            filter: None,
        }
    }

    /// The body range (headers and totals excluded) as A1-style corners.
    pub fn body_range(&self) -> Option<(String, String)> {
        let ((start_row, start_col), (end_row, end_col)) = crate::address::parse_range(&self.range)?;
        let first_body = if self.has_headers { start_row + 1 } else { start_row };
        let last_body = if self.has_totals { end_row.saturating_sub(1) } else { end_row };
        if first_body > last_body || start_col > end_col {
            return None;
        }
        Some((crate::address::format(first_body, start_col), crate::address::format(last_body, end_col)))
    }

    /// The header range of the table, when it has headers.
    pub fn header_range(&self) -> Option<String> {
        if !self.has_headers {
            return None;
        }
        let ((start_row, start_col), (_, end_col)) = crate::address::parse_range(&self.range)?;
        Some(format!("{}:{}", crate::address::format(start_row, start_col), crate::address::format(start_row, end_col)))
    }

    /// The totals range of the table, when it has one.
    pub fn totals_range(&self) -> Option<String> {
        if !self.has_totals {
            return None;
        }
        let ((_, start_col), (end_row, end_col)) = crate::address::parse_range(&self.range)?;
        Some(format!("{}:{}", crate::address::format(end_row, start_col), crate::address::format(end_row, end_col)))
    }
}

/// Sheet protection as stored in `<sheetProtection>`.
///
/// The password never appears in clear: `password_hash` is the legacy 16-bit
/// XOR hash and `hash_value`/`salt_value`/`spin_count` carry the modern
/// SHA-512 verifier, both exactly as Excel wrote them. The editor cannot
/// unlock a protected sheet without the password and does not attempt to; the
/// fields exist so a protected workbook round-trips byte-for-attribute.
/// `options` holds the boolean attributes that were set (see `Sheet::protection`);
/// true means the corresponding action stays locked.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SheetProtection {
    /// True when `<sheetProtection>` says the sheet itself is protected.
    pub enabled: bool,
    /// Legacy `password` attribute (16-bit hash), when present.
    pub password_hash: Option<String>,
    /// Modern `algorithmName` (usually `SHA-512`).
    pub algorithm_name: String,
    pub hash_value: String,
    pub salt_value: String,
    pub spin_count: u32,
    /// Locked-action attribute names that are on, sorted for a stable order:
    /// `formatCells`, `formatColumns`, `formatRows`, `insertRows`, ... .
    pub options: Vec<String>,
}

/// The recognised `<sheetProtection>` boolean attributes, in schema order.
pub const SHEET_PROTECTION_OPTIONS: [&str; 15] = [
    "objects",
    "scenarios",
    "formatCells",
    "formatColumns",
    "formatRows",
    "insertColumns",
    "insertRows",
    "insertHyperlinks",
    "deleteColumns",
    "deleteRows",
    "selectLockedCells",
    "sort",
    "autoFilter",
    "pivotTables",
    "selectUnlockedCells",
];

/// A pivot cache/table read back from a package, kept as its original parts.
///
/// The grid a pivot renders is already in the sheet cells, so the editor never
/// recomputes from the cache. These raw parts exist purely so a re-export
/// writes the same live Excel pivot instead of silently flattening it; the
/// records are base64 because `.oswk` is JSON.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PreservedPivot {
    /// The pivot table's `name` attribute, e.g. `PivotTable1`.
    pub name: String,
    /// Sheet the pivot table belongs to.
    pub sheet: String,
    /// The `cacheId` from the workbook's `<pivotCaches>` list.
    pub cache_id: u32,
    /// Raw `xl/pivotCache/pivotCacheDefinitionN.xml` text.
    pub definition_xml: String,
    /// Raw `xl/pivotCache/pivotCacheRecordsN.xml` bytes, base64 encoded.
    #[serde(default)]
    pub records_base64: Option<String>,
    /// Raw `xl/pivotTables/pivotTableN.xml` text.
    pub table_xml: String,
    /// Original records part path, kept so the re-export keeps the same target.
    #[serde(default)]
    pub records_part: Option<String>,
    /// `sheet!range` of the cache source, for display and warnings.
    pub source: String,
    /// Cache field names in order.
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Sheet {
    pub id: String,
    pub name: String,
    pub row_count: u32,
    pub col_count: u32,
    pub cells: BTreeMap<String, Cell>,
    pub col_widths: BTreeMap<u32, f64>,
    pub row_heights: BTreeMap<u32, f64>,
    pub merges: Vec<MergeRange>,
    pub freeze_rows: u32,
    pub freeze_cols: u32,
    pub charts: Vec<ChartPlacement>,
    /// Pictures floating over the sheet (V3.1 XLSX import/export).
    #[serde(default)]
    pub images: Vec<SheetImage>,
    pub pivot_tables: Vec<PivotTable>,
    /// Structured tables defined over this sheet.
    #[serde(default)]
    pub tables: Vec<SpreadsheetTable>,
    pub conditional: Vec<CondRule>,
    pub validations: Vec<Validation>,
    pub filter: Option<FilterState>,
    pub show_gridlines: bool,
    pub tab_color: Option<String>,
    /// Print layout; kept in the native format and written to XLSX.
    pub print: PrintSettings,
    /// Legacy sheet-protection hash; empty means the sheet is unprotected.
    pub sheet_protection: String,
    /// Full `<sheetProtection>` state (V3.1). `sheet_protection` keeps the
    /// legacy password hash so older units and the frontend stay compatible.
    #[serde(default)]
    pub protection: SheetProtection,
}

impl Default for Sheet {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "Sheet1".into(),
            row_count: 200,
            col_count: 26,
            cells: BTreeMap::new(),
            col_widths: BTreeMap::new(),
            row_heights: BTreeMap::new(),
            merges: Vec::new(),
            freeze_rows: 0,
            freeze_cols: 0,
            charts: Vec::new(),
            images: Vec::new(),
            pivot_tables: Vec::new(),
            tables: Vec::new(),
            conditional: Vec::new(),
            validations: Vec::new(),
            filter: None,
            show_gridlines: true,
            tab_color: None,
            print: PrintSettings::default(),
            sheet_protection: String::new(),
            protection: SheetProtection::default(),
        }
    }
}

impl Sheet {
    pub fn new(name: &str) -> Self {
        Self { id: uuid::Uuid::new_v4().to_string(), name: name.to_string(), ..Default::default() }
    }

    pub fn set(&mut self, address: &str, cell: Cell) {
        if cell.is_empty() {
            self.cells.remove(address);
        } else {
            self.cells.insert(address.to_string(), cell);
        }
    }

    pub fn get(&self, address: &str) -> Option<&Cell> {
        self.cells.get(address)
    }

    /// Number of cells that are not empty (statistics pane).
    pub fn used_cells(&self) -> usize {
        self.cells.len()
    }

    pub fn extend_for(&mut self, address: &str) {
        if let Some((row, col)) = crate::address::parse(address) {
            if row + 1 > self.row_count {
                self.row_count = row + 1 + 50;
            }
            if col + 1 > self.col_count {
                self.col_count = col + 1 + 5;
            }
        }
    }
}

/// A workbook- or sheet-scoped defined name.
///
/// `definition` holds the raw target - a range (`Data!A1:A99`), a cell, a
/// constant or a formula - so a name can point at anything a formula can
/// express. `sheet` is `None` for a workbook-level name, which is what makes
/// `VAT_RATE` visible from every sheet.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct NamedRange {
    pub name: String,
    pub definition: String,
    pub sheet: Option<String>,
    pub comment: String,
}

impl NamedRange {
    /// True when the name is visible from every sheet.
    pub fn is_workbook_scope(&self) -> bool {
        self.sheet.as_deref().map(str::trim).unwrap_or("").is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Workbook {
    pub id: String,
    pub title: String,
    pub sheets: Vec<Sheet>,
    pub active_sheet: usize,
    /// Defined names, workbook-level and per-sheet.
    pub names: Vec<NamedRange>,
    pub metadata: DocMetadata,
    /// Pivot caches/tables imported raw from a package (V3.1); re-exported
    /// as-is so a live Excel pivot survives an edit-and-save cycle.
    #[serde(default)]
    pub preserved_pivots: Vec<PreservedPivot>,
}

impl Default for Workbook {
    fn default() -> Self {
        Self {
            id: String::new(),
            title: "Untitled spreadsheet".into(),
            sheets: vec![Sheet::new("Sheet1")],
            active_sheet: 0,
            names: Vec::new(),
            metadata: DocMetadata::default(),
            preserved_pivots: Vec::new(),
        }
    }
}

impl Workbook {
    pub fn new_blank(title: &str) -> Self {
        Self { id: uuid::Uuid::new_v4().to_string(), title: title.to_string(), ..Default::default() }
    }

    /// Names visible from `sheet`: workbook-level names plus that sheet's own.
    pub fn names_for(&self, sheet: &str) -> Vec<&NamedRange> {
        self.names
            .iter()
            .filter(|entry| entry.is_workbook_scope() || entry.sheet.as_deref() == Some(sheet))
            .collect()
    }

    pub fn unique_sheet_name(&self, base: &str) -> String {
        let mut index = 1;
        loop {
            let name = if index == 1 { base.to_string() } else { format!("{base}{index}") };
            if !self.sheets.iter().any(|sheet| sheet.name == name) {
                return name;
            }
            index += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Impress
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SlideSize {
    pub preset: String,
    pub width_pt: f64,
    pub height_pt: f64,
}

impl Default for SlideSize {
    fn default() -> Self {
        // 16:9 widescreen in points (13.333in x 7.5in).
        Self { preset: "16:9".into(), width_pt: 960.0, height_pt: 540.0 }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TextParagraph {
    pub text: String,
    pub level: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub size_pt: Option<f64>,
    pub color: Option<String>,
    pub align: String,
    pub bullet: bool,
    pub runs: Vec<Run>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TextFrame {
    pub paragraphs: Vec<TextParagraph>,
    pub valign: String,
    pub font: Option<String>,
    pub size_pt: Option<f64>,
    pub color: Option<String>,
    pub align: String,
}

impl TextFrame {
    pub fn plain(&self) -> String {
        self.paragraphs.iter().map(|p| p.text.as_str()).collect::<Vec<_>>().join("\n")
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ShapeStyle {
    pub fill: Option<String>,
    pub stroke: Option<String>,
    pub stroke_width_pt: f64,
    pub opacity: f64,
    pub corner_radius_pt: f64,
    pub shadow: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct LineSpec {
    pub x2: f64,
    pub y2: f64,
    pub begin_arrow: bool,
    pub end_arrow: bool,
    pub dash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SlideObject {
    pub id: String,
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub rotation: f64,
    pub z: i32,
    pub text: Option<TextFrame>,
    pub image: Option<ImageData>,
    pub style: Option<ShapeStyle>,
    pub line: Option<LineSpec>,
    pub table: Option<TableData>,
    pub chart: Option<ChartData>,
    /// Flat group membership kept for files written before V3; new groups use
    /// the `group` kind with `children`.
    pub group_id: Option<String>,
    /// Children of a `group` object. Coordinates are absolute; moving or
    /// resizing the group transforms every child.
    #[serde(default)]
    pub children: Vec<SlideObject>,
    /// Placeholder role on a master or layout: `title`, `body`, `subtitle`,
    /// `footer`, `slideNumber`, `date`.
    #[serde(default)]
    pub placeholder: Option<String>,
    pub name: String,
}

impl Default for SlideObject {
    fn default() -> Self {
        Self {
            id: String::new(),
            kind: "rect".into(),
            x: 0.0,
            y: 0.0,
            w: 200.0,
            h: 100.0,
            rotation: 0.0,
            z: 0,
            text: None,
            image: None,
            style: None,
            line: None,
            table: None,
            chart: None,
            group_id: None,
            children: Vec::new(),
            placeholder: None,
            name: String::new(),
        }
    }
}

impl SlideObject {
    pub fn new(kind: &str, x: f64, y: f64, w: f64, h: f64) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            kind: kind.to_string(),
            x,
            y,
            w,
            h,
            z: 1,
            ..Default::default()
        }
    }
}

/// One animation applied to a slide object during the slideshow.
///
/// `kind` is `entrance`, `emphasis` or `exit`; `trigger` is `onClick`,
/// `withPrevious` or `afterPrevious`. The model is deliberately small: it is
/// the subset the built-in slideshow can actually execute.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Animation {
    pub id: String,
    pub object_id: String,
    pub kind: String,
    pub effect: String,
    pub trigger: String,
    pub duration_ms: u32,
    pub delay_ms: u32,
    pub order: u32,
}

/// A layout inside a master. Placeholder objects on the layout are inherited
/// by slides that use it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SlideLayout {
    pub id: String,
    pub name: String,
    /// `title`, `titleContent`, `twoContent`, `section`, `blank`, ...
    pub kind: String,
    pub objects: Vec<SlideObject>,
}

/// A slide master: theme, background and the layouts slides inherit from.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SlideMaster {
    pub id: String,
    pub name: String,
    pub theme: String,
    pub background: Option<String>,
    pub objects: Vec<SlideObject>,
    pub layouts: Vec<SlideLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Slide {
    pub id: String,
    /// Legacy editor layout preset; kept so old files keep their meaning.
    pub layout: String,
    /// Master this slide inherits from; `None` means the deck default.
    #[serde(default)]
    pub master_id: Option<String>,
    /// Layout inside the master; `None` means no inherited layout.
    #[serde(default)]
    pub layout_id: Option<String>,
    pub background: Option<String>,
    pub transition: Option<String>,
    pub transition_ms: u32,
    pub objects: Vec<SlideObject>,
    #[serde(default)]
    pub animations: Vec<Animation>,
    pub notes: String,
}

impl Default for Slide {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            layout: "titleContent".into(),
            master_id: None,
            layout_id: None,
            background: None,
            transition: None,
            transition_ms: 500,
            objects: Vec::new(),
            animations: Vec::new(),
            notes: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Deck {
    pub id: String,
    pub title: String,
    pub size: SlideSize,
    pub theme: String,
    pub slides: Vec<Slide>,
    /// Slide masters with their layouts. Empty means the deck uses the
    /// built-in editor theme and no inheritance.
    #[serde(default)]
    pub masters: Vec<SlideMaster>,
    pub metadata: DocMetadata,
}

impl Default for Deck {
    fn default() -> Self {
        Self {
            id: String::new(),
            title: "Untitled presentation".into(),
            size: SlideSize::default(),
            theme: "minimal".into(),
            slides: vec![Slide::default()],
            masters: Vec::new(),
            metadata: DocMetadata::default(),
        }
    }
}

impl Deck {
    pub fn new_blank(title: &str) -> Self {
        Self { id: uuid::Uuid::new_v4().to_string(), title: title.to_string(), ..Default::default() }
    }

    pub fn master(&self, id: &str) -> Option<&SlideMaster> {
        self.masters.iter().find(|master| master.id == id)
    }

    /// The master a slide inherits from: its own, or the first deck master.
    pub fn master_for(&self, slide: &Slide) -> Option<&SlideMaster> {
        slide.master_id.as_deref().and_then(|id| self.master(id)).or_else(|| self.masters.first())
    }

    pub fn layout_for(&self, slide: &Slide) -> Option<&SlideLayout> {
        let master = self.master_for(slide)?;
        let layout_id = slide.layout_id.as_deref()?;
        master.layouts.iter().find(|layout| layout.id == layout_id)
    }

    /// The objects a slide shows, including inherited master and layout
    /// objects. Object ids from the slide are kept; inherited objects are
    /// returned with a `master:`/`layout:` id prefix so the editor can tell
    /// them apart and render them as non-editable background content.
    pub fn inherited_objects(&self, slide: &Slide) -> Vec<SlideObject> {
        let mut out = Vec::new();
        if let Some(master) = self.master_for(slide) {
            for object in &master.objects {
                let mut inherited = object.clone();
                inherited.id = format!("master:{}", object.id);
                out.push(inherited);
            }
            if let Some(layout) = self.layout_for(slide) {
                for object in &layout.objects {
                    let mut inherited = object.clone();
                    inherited.id = format!("layout:{}", object.id);
                    out.push(inherited);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writer_model_roundtrips_json() {
        let mut document = TextDocument::new_blank("Test");
        document.blocks.push(Block::heading("Intro", 1));
        let mut table = TableData::simple(2, 3, 400.0);
        table.rows[1].cells[0].blocks.push(Block::paragraph("cell"));
        document.blocks.push(Block::Table { table });
        let json = serde_json::to_string(&document).unwrap();
        let back: TextDocument = serde_json::from_str(&json).unwrap();
        assert_eq!(back.blocks.len(), 3);
    }

    #[test]
    fn workbook_default_shape() {
        let workbook = Workbook::new_blank("Test");
        assert_eq!(workbook.sheets.len(), 1);
        assert_eq!(workbook.sheets[0].row_count, 200);
        assert_eq!(workbook.unique_sheet_name("Sheet"), "Sheet");
        let mut workbook = workbook;
        workbook.sheets.push(Sheet::new("Sheet"));
        assert_eq!(workbook.unique_sheet_name("Sheet"), "Sheet2");
    }

    #[test]
    fn image_roundtrip() {
        let image = ImageData::from_bytes("x.png", &[0x89, b'P', b'N', b'G']);
        assert_eq!(image.mime, "image/png");
        assert_eq!(image.bytes().len(), 4);
        assert_eq!(image.extension(), "png");
    }
}

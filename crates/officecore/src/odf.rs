//! OpenDocument Format support: ODT (Writer), ODS (Calc) and ODP (Impress).
//!
//! Written directly as ODF 1.2 packages so LibreOffice, OpenOffice and
//! OnlyOffice open the results. Import is tolerant: unknown constructs are
//! skipped with warnings, and nothing is ever executed from the package.

use crate::error::{OfficeError, OfficeResult};
use crate::model::*;
use crate::xml::{parse_xml, XmlNode, XmlWriter};
use crate::zip::{ZipReader, ZipWriter};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

const NS: &str = concat!(
    "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
    "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" ",
    "xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" ",
    "xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" ",
    "xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" ",
    "xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" ",
    "xmlns:xlink=\"http://www.w3.org/1999/xlink\" ",
    "xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" ",
    "xmlns:number=\"urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0\" ",
    "xmlns:meta=\"urn:oasis:names:tc:opendocument:xmlns:meta:1.0\" ",
    "xmlns:dc=\"http://purl.org/dc/elements/1.1/\" ",
    "xmlns:presentation=\"urn:oasis:names:tc:opendocument:xmlns:presentation:1.0\""
);

fn cm(points: f64) -> String {
    format!("{:.3}cm", points * crate::model::PT_TO_MM / 10.0)
}

fn parse_cm(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    let number = trimmed.trim_end_matches(|ch: char| ch.is_ascii_alphabetic() || ch == '%');
    let value = number.parse::<f64>().ok()?;
    if trimmed.ends_with("cm") || trimmed.ends_with("mm") || trimmed.ends_with("in") || trimmed.ends_with("pt") {
        if trimmed.ends_with("mm") {
            return Some(value / 10.0 * crate::model::MM_TO_PT);
        }
        if trimmed.ends_with("in") {
            return Some(value * 72.0);
        }
        if trimmed.ends_with("pt") {
            return Some(value);
        }
        return Some(value * 10.0 * crate::model::MM_TO_PT);
    }
    Some(value)
}

fn escape(value: &str) -> String {
    crate::xml::escape_attr(value)
}

#[derive(Default)]
struct AutoStyles {
    paragraphs: Vec<(String, String)>,
    texts: Vec<(String, String)>,
    cells: Vec<(String, String)>,
    graphics: Vec<(String, String)>,
    paragraph_keys: HashMap<String, String>,
    text_keys: HashMap<String, String>,
    cell_keys: HashMap<String, String>,
    graphic_keys: HashMap<String, String>,
}

impl AutoStyles {
    fn paragraph(&mut self, xml: String) -> String {
        if let Some(name) = self.paragraph_keys.get(&xml) {
            return name.clone();
        }
        let name = format!("P{}", self.paragraphs.len() + 1);
        self.paragraphs.push((name.clone(), xml.clone()));
        self.paragraph_keys.insert(xml, name.clone());
        name
    }

    fn text(&mut self, xml: String) -> String {
        if let Some(name) = self.text_keys.get(&xml) {
            return name.clone();
        }
        let name = format!("T{}", self.texts.len() + 1);
        self.texts.push((name.clone(), xml.clone()));
        self.text_keys.insert(xml, name.clone());
        name
    }

    fn cell(&mut self, xml: String) -> String {
        if let Some(name) = self.cell_keys.get(&xml) {
            return name.clone();
        }
        let name = format!("C{}", self.cells.len() + 1);
        self.cells.push((name.clone(), xml.clone()));
        self.cell_keys.insert(xml, name.clone());
        name
    }

    fn graphic(&mut self, xml: String) -> String {
        if let Some(name) = self.graphic_keys.get(&xml) {
            return name.clone();
        }
        let name = format!("G{}", self.graphics.len() + 1);
        self.graphics.push((name.clone(), xml.clone()));
        self.graphic_keys.insert(xml, name.clone());
        name
    }

    fn xml(&self) -> String {
        let mut out = String::from("<office:automatic-styles>");
        for (name, xml) in &self.paragraphs {
            out.push_str(&format!("<style:style style:name=\"{name}\" style:family=\"paragraph\">{xml}</style:style>"));
        }
        for (name, xml) in &self.texts {
            out.push_str(&format!("<style:style style:name=\"{name}\" style:family=\"text\">{xml}</style:style>"));
        }
        for (name, xml) in &self.cells {
            out.push_str(&format!(
                "<style:style style:name=\"{name}\" style:family=\"table-cell\">{xml}</style:style>"
            ));
        }
        for (name, xml) in &self.graphics {
            out.push_str(&format!("<style:style style:name=\"{name}\" style:family=\"graphic\">{xml}</style:style>"));
        }
        out.push_str("</office:automatic-styles>");
        out
    }
}

#[derive(Default)]
struct Media {
    items: Vec<(String, Vec<u8>)>,
    index: usize,
}

impl Media {
    fn add(&mut self, image: &ImageData) -> Option<String> {
        if image.is_empty() {
            return None;
        }
        self.index += 1;
        let name = format!("image{}.{}", self.index, image.extension());
        self.items.push((name.clone(), image.bytes()));
        Some(name)
    }
}

/// Writer-side note lookup: citation numbers come from the document's
/// reference order (`TextDocument::footnote_order` / `endnote_order`) so the
/// `text:note-citation` values match what the editor shows. `comments` places
/// the body's annotations; it stays empty for headers, footers and note bodies.
#[derive(Default)]
struct NoteContext {
    footnotes: HashMap<String, (usize, Footnote)>,
    endnotes: HashMap<String, (usize, Footnote)>,
    comments: CommentAnchors,
}

impl NoteContext {
    fn for_document(document: &TextDocument) -> Self {
        let mut context = NoteContext::default();
        for (index, id) in document.footnote_order().iter().enumerate() {
            if let Some(note) = document.footnotes.iter().find(|note| &note.id == id) {
                context.footnotes.insert(id.clone(), (index + 1, note.clone()));
            }
        }
        for (index, id) in document.endnote_order().iter().enumerate() {
            if let Some(note) = document.endnotes.iter().find(|note| &note.id == id) {
                context.endnotes.insert(id.clone(), (index + 1, note.clone()));
            }
        }
        context
    }

    fn reference(&self, id: &str, endnote: bool) -> Option<&(usize, Footnote)> {
        if endnote {
            self.endnotes.get(id)
        } else {
            self.footnotes.get(id)
        }
    }
}

/// LibreOffice's extension namespace, declared on content.xml for the
/// `loext:resolved` comment flag.
const LOEXT_NS: &str = "xmlns:loext=\"urn:org:documentfoundation:names:experimental:office:xmlns:loext:1.0\"";

/// Writer-side `office:annotation` placement for the ODT body.
///
/// The annotation is written before the first body run anchored to a comment
/// and `office:annotation-end` after the last one, so a range can span runs and
/// paragraphs. A comment whose anchored runs carry no content becomes a point
/// annotation (no end marker), which is how LibreOffice stores a comment made
/// without a selection. Comments no body run refers to are written as point
/// annotations at the start of the first paragraph, so the export keeps them.
#[derive(Default)]
struct CommentAnchors {
    comments: HashMap<String, Comment>,
    /// Anchored body runs per comment id, and whether any of them has content.
    anchors: HashMap<String, (usize, bool)>,
    /// Anchored runs written so far per comment id.
    written: std::cell::RefCell<HashMap<String, usize>>,
    unanchored: std::cell::RefCell<Vec<Comment>>,
}

impl CommentAnchors {
    fn for_document(document: &TextDocument) -> Self {
        let mut anchors = HashMap::new();
        count_comment_runs(&document.blocks, &mut anchors);
        let mut comments = HashMap::new();
        let mut unanchored = Vec::new();
        for comment in &document.comments {
            if comments.contains_key(&comment.id) {
                continue;
            }
            if !anchors.contains_key(&comment.id) {
                unanchored.push(comment.clone());
            }
            comments.insert(comment.id.clone(), comment.clone());
        }
        Self { comments, anchors, written: Default::default(), unanchored: std::cell::RefCell::new(unanchored) }
    }

    /// Writes the comments no body run refers to; only the first call writes.
    fn write_unanchored(&self, writer: &mut XmlWriter) {
        for comment in self.unanchored.take() {
            writer.raw(&annotation_xml(&comment));
        }
    }

    /// Opens the annotation when `run` is the first body run of its comment.
    fn before_run(&self, writer: &mut XmlWriter, run: &Run) {
        let Some(comment) = run.comment.as_deref().and_then(|id| self.comments.get(id)) else { return };
        let mut written = self.written.borrow_mut();
        let count = written.entry(comment.id.clone()).or_insert(0);
        *count += 1;
        if *count == 1 {
            writer.raw(&annotation_xml(comment));
        }
    }

    /// Closes a ranged annotation after the last body run of its comment.
    fn after_run(&self, writer: &mut XmlWriter, run: &Run) {
        let Some(id) = run.comment.as_deref().filter(|id| self.comments.contains_key(*id)) else { return };
        let Some(&(total, has_content)) = self.anchors.get(id) else { return };
        if has_content && self.written.borrow().get(id) == Some(&total) {
            writer.raw(&format!("<office:annotation-end office:name=\"{}\"/>", escape(id)));
        }
    }
}

/// Counts the runs `write_blocks` writes for each comment id (paragraphs and
/// table cells, at any depth).
fn count_comment_runs(blocks: &[Block], anchors: &mut HashMap<String, (usize, bool)>) {
    for block in blocks {
        match block {
            Block::Paragraph { runs, .. } => {
                for run in runs {
                    if let Some(id) = &run.comment {
                        let entry = anchors.entry(id.clone()).or_insert((0, false));
                        entry.0 += 1;
                        entry.1 |= !run.text.is_empty() || run.footnote.is_some() || run.endnote.is_some();
                    }
                }
            }
            Block::Table { table } => {
                for cell in table.rows.iter().flat_map(|row| row.cells.iter()) {
                    count_comment_runs(&cell.blocks, anchors);
                }
            }
            _ => {}
        }
    }
}

/// One `office:annotation` element. ODF has no portable reply threading, so
/// replies follow the comment text as "Re: author: text" paragraphs, the same
/// convention the DOCX writer uses; reply line breaks stay inside the reply
/// paragraph so the reader can tell replies from comment lines.
fn annotation_xml(comment: &Comment) -> String {
    let mut out = format!("<office:annotation office:name=\"{}\"", escape(&comment.id));
    if comment.resolved {
        out.push_str(" loext:resolved=\"true\"");
    }
    out.push_str(&format!("><dc:creator>{}</dc:creator>", crate::xml::escape_text(&comment.author)));
    if !comment.created.is_empty() {
        out.push_str(&format!("<dc:date>{}</dc:date>", crate::xml::escape_text(&comment.created)));
    }
    for line in comment.text.split('\n') {
        out.push_str(&format!("<text:p>{}</text:p>", crate::xml::escape_text(line)));
    }
    for reply in &comment.replies {
        let text = reply.text.split('\n').map(crate::xml::escape_text).collect::<Vec<_>>().join("<text:line-break/>");
        out.push_str(&format!("<text:p>Re: {}: {text}</text:p>", crate::xml::escape_text(&reply.author)));
    }
    out.push_str("</office:annotation>");
    out
}

/// `text:id` must be an XML ID (an NCName). Imported ids are often numeric
/// ("1") or UUID-ish; anything unusable falls back to `ftnN`/`ednN` so the
/// package stays valid and our own reader still restores a stable reference.
fn note_export_id(note: &Footnote, endnote: bool, number: usize) -> String {
    let candidate = note.id.trim();
    let first_ok = candidate.chars().next().map(|ch| ch.is_ascii_alphabetic() || ch == '_').unwrap_or(false);
    let rest_ok = candidate.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_'));
    if first_ok && rest_ok {
        candidate.to_string()
    } else {
        format!("{}{}", if endnote { "edn" } else { "ftn" }, number)
    }
}

/// One inline `text:note` element: citation plus body paragraphs. The body
/// runs are written with an empty note context, so notes inside notes (which
/// ODF forbids anyway) cannot recurse.
fn note_xml(note: &Footnote, endnote: bool, number: usize, styles: &mut AutoStyles) -> String {
    let id = note_export_id(note, endnote, number);
    let class = if endnote { "endnote" } else { "footnote" };
    let citation = if note.marker.trim().is_empty() { number.to_string() } else { note.marker.clone() };
    let mut writer = XmlWriter::new();
    write_runs(&mut writer, &note.runs, styles, &NoteContext::default());
    let body = writer.finish();
    format!(
        "<text:note text:id=\"{}\" text:note-class=\"{class}\"><text:note-citation>{}</text:note-citation><text:note-body>{}</text:note-body></text:note>",
        escape(&id),
        crate::xml::escape_text(&citation),
        if body.is_empty() { "<text:p/>".to_string() } else { format!("<text:p>{body}</text:p>") }
    )
}

fn paragraph_style_xml(props: &ParaProps, page_break: bool) -> String {
    let mut properties = String::new();
    match props.align.as_str() {
        "center" => properties.push_str(" fo:text-align=\"center\""),
        "right" => properties.push_str(" fo:text-align=\"right\""),
        "justify" => properties.push_str(" fo:text-align=\"justify\""),
        _ => {}
    }
    if props.line_spacing > 0.0 {
        properties.push_str(&format!(" fo:line-height=\"{:.0}%\"", props.line_spacing * 100.0));
    }
    if props.space_before_pt > 0.0 {
        properties.push_str(&format!(" fo:margin-top=\"{}\"", cm(props.space_before_pt)));
    }
    if props.space_after_pt > 0.0 {
        properties.push_str(&format!(" fo:margin-bottom=\"{}\"", cm(props.space_after_pt)));
    }
    let mut indent = props.indent_left_pt;
    if let Some(list) = &props.list {
        indent += 18.0 * (list.level as f64 + 1.0);
    }
    if indent > 0.0 {
        properties.push_str(&format!(" fo:margin-left=\"{}\"", cm(indent)));
    }
    if props.indent_right_pt > 0.0 {
        properties.push_str(&format!(" fo:margin-right=\"{}\"", cm(props.indent_right_pt)));
    }
    if props.first_line_pt != 0.0 {
        properties.push_str(&format!(" fo:text-indent=\"{}\"", cm(props.first_line_pt)));
    }
    if page_break {
        properties.push_str(" fo:break-before=\"page\"");
    }
    if props.tabs.is_empty() {
        return format!("<style:paragraph-properties{properties}/>");
    }
    // Tab stops are child elements, so the properties element cannot
    // self-close when they are present.
    let mut tab_stops = String::from("<style:tab-stops>");
    for tab in &props.tabs {
        let kind = match tab.align.as_str() {
            "center" => "center",
            "right" => "right",
            "decimal" => "char",
            _ => "left",
        };
        let character = if tab.align == "decimal" { " style:char=\".\"" } else { "" };
        tab_stops.push_str(&format!(
            "<style:tab-stop style:position=\"{}\" style:type=\"{kind}\"{character}/>",
            cm(tab.pos_pt)
        ));
    }
    tab_stops.push_str("</style:tab-stops>");
    format!("<style:paragraph-properties{properties}>{tab_stops}</style:paragraph-properties>")
}

fn run_style_xml(run: &Run) -> String {
    let mut properties = String::new();
    if run.bold {
        properties.push_str(" fo:font-weight=\"bold\"");
    }
    if run.italic {
        properties.push_str(" fo:font-style=\"italic\"");
    }
    if let Some(font) = &run.font {
        properties.push_str(&format!(" fo:font-family=\"{}\"", escape(font)));
    }
    if let Some(size) = run.size_pt {
        properties.push_str(&format!(" fo:font-size=\"{size}pt\""));
    }
    if let Some(color) = &run.color {
        properties.push_str(&format!(" fo:color=\"{}\"", escape(color)));
    }
    if let Some(highlight) = &run.highlight {
        properties.push_str(&format!(" fo:background-color=\"{}\"", escape(highlight)));
    }
    if run.underline {
        properties.push_str(" style:text-underline-style=\"solid\" style:text-underline-width=\"auto\" style:text-underline-color=\"font-color\"");
    }
    if run.strike {
        properties.push_str(" style:text-line-through-style=\"solid\"");
    }
    if run.superscript {
        properties.push_str(" style:text-position=\"super 58%\"");
    } else if run.subscript {
        properties.push_str(" style:text-position=\"sub 58%\"");
    }
    format!("<style:text-properties{properties}/>")
}

fn write_runs(writer: &mut XmlWriter, runs: &[Run], styles: &mut AutoStyles, notes: &NoteContext) {
    notes.comments.write_unanchored(writer);
    for run in runs {
        notes.comments.before_run(writer, run);
        let text = run.text.replace('\n', " ");
        if !text.is_empty() {
            let content = crate::xml::escape_text(&text);
            if run.link.is_some()
                || run.bold
                || run.italic
                || run.underline
                || run.strike
                || run.color.is_some()
                || run.highlight.is_some()
                || run.font.is_some()
                || run.size_pt.is_some()
                || run.superscript
                || run.subscript
            {
                let name = styles.text(run_style_xml(run));
                writer.raw(&format!("<text:span text:style-name=\"{name}\">{content}</text:span>"));
            } else {
                writer.raw(&content);
            }
        }
        // Note references are emitted inline at the run position, after the
        // run text; the note body never lands in the paragraph runs.
        if let Some(id) = &run.footnote {
            if let Some((number, note)) = notes.reference(id, false) {
                writer.raw(&note_xml(note, false, *number, styles));
            }
        }
        if let Some(id) = &run.endnote {
            if let Some((number, note)) = notes.reference(id, true) {
                writer.raw(&note_xml(note, true, *number, styles));
            }
        }
        if let Some(url) = &run.link {
            let last = writer.as_str().len();
            let _ = last;
            let _ = url;
        }
        notes.comments.after_run(writer, run);
    }
}

fn write_blocks(
    writer: &mut XmlWriter,
    blocks: &[Block],
    styles: &mut AutoStyles,
    media: &mut Media,
    notes: &NoteContext,
    list_depth: u32,
) {
    let mut index = 0usize;
    while index < blocks.len() {
        let block = &blocks[index];
        match block {
            Block::Paragraph { props, runs } => {
                if let Some(list) = &props.list {
                    // Collect the consecutive list items at this level.
                    writer.raw(&format!(
                        "<text:list text:style-name=\"{}\">",
                        if list.kind == "number" { "LN" } else { "LB" }
                    ));
                    while index < blocks.len() {
                        match &blocks[index] {
                            Block::Paragraph { props: inner, runs: inner_runs } if inner.list.is_some() => {
                                let name = styles.paragraph(paragraph_style_xml(inner, false));
                                writer.raw(&format!("<text:list-item><text:p text:style-name=\"{name}\">"));
                                write_runs(writer, inner_runs, styles, notes);
                                writer.raw("</text:p></text:list-item>");
                                index += 1;
                            }
                            _ => break,
                        }
                    }
                    writer.raw("</text:list>");
                    continue;
                }
                let is_heading = props.style.starts_with("Heading");
                let name = styles.paragraph(paragraph_style_xml(props, props.page_break_before));
                if is_heading {
                    let level = props.style.trim_start_matches("Heading").parse::<u32>().unwrap_or(1).clamp(1, 10);
                    writer.raw(&format!("<text:h text:outline-level=\"{level}\" text:style-name=\"{name}\">"));
                    write_runs(writer, runs, styles, notes);
                    writer.raw("</text:h>");
                } else {
                    writer.raw(&format!("<text:p text:style-name=\"{name}\">"));
                    write_runs(writer, runs, styles, notes);
                    writer.raw("</text:p>");
                }
            }
            Block::Table { table } => write_table(writer, table, styles, media, notes),
            Block::Image { image, width_pt, height_pt, wrap, .. } => {
                let Some(name) = media.add(image) else {
                    index += 1;
                    continue;
                };
                // Inline images keep the plain paragraph-anchored frame; square
                // and top/bottom wrapping need a graphic style that says how the
                // text flows around the frame.
                let style = match wrap.as_str() {
                    "square" => Some(styles.graphic("<style:graphic-properties style:wrap=\"parallel\"/>".into())),
                    "topBottom" => Some(styles.graphic("<style:graphic-properties style:wrap=\"none\"/>".into())),
                    _ => None,
                };
                let style_attr = style.map(|name| format!(" draw:style-name=\"{name}\"")).unwrap_or_default();
                writer.raw(&format!(
                    "<text:p text:style-name=\"{}\"><draw:frame draw:name=\"{}\"{style_attr} text:anchor-type=\"paragraph\" svg:width=\"{}\" svg:height=\"{}\"><draw:image xlink:href=\"Pictures/{}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/></draw:frame></text:p>",
                    styles.paragraph(paragraph_style_xml(&ParaProps { align: "center".into(), ..Default::default() }, false)),
                    escape(&image.name),
                    cm(width_pt.max(24.0)),
                    cm(height_pt.max(18.0)),
                    name
                ));
            }
            Block::PageBreak => {
                let props = ParaProps { page_break_before: true, ..Default::default() };
                let name = styles.paragraph(paragraph_style_xml(&props, true));
                writer.raw(&format!("<text:p text:style-name=\"{name}\"/>"));
            }
            Block::Rule => {
                writer.raw("<text:p>- - - - -</text:p>");
            }
            Block::Toc { entries } => {
                for entry in entries {
                    let text =
                        if entry.page > 0 { format!("{} .... {}", entry.text, entry.page) } else { entry.text.clone() };
                    writer.raw(&format!("<text:p>{}</text:p>", crate::xml::escape_text(&text)));
                }
            }
            Block::SectionBreak { .. } => {
                let props = ParaProps { page_break_before: true, ..Default::default() };
                let name = styles.paragraph(paragraph_style_xml(&props, true));
                writer.raw(&format!("<text:p text:style-name=\"{name}\"/>"));
            }
        }
        index += 1;
    }
    let _ = list_depth;
}

/// How many grid columns a table spans, counting colspans and the columns a
/// rowspan keeps covered in the rows below it. `TableRow.cells` holds only the
/// cells that start in a row, so the plain `cells.len()` undercounts merged
/// tables.
fn table_grid_columns(table: &TableData) -> usize {
    let mut open: Vec<usize> = Vec::new();
    let mut columns = 0usize;
    for row in &table.rows {
        // `open` counts the rows still covered after the current one.
        for remaining in open.iter_mut() {
            *remaining = remaining.saturating_sub(1);
        }
        let mut column = 0usize;
        for cell in &row.cells {
            while column < open.len() && open[column] > 0 {
                column += 1;
            }
            let span = cell.colspan.max(1) as usize;
            while open.len() < column + span {
                open.push(0);
            }
            if cell.rowspan > 1 {
                for slot in open.iter_mut().skip(column).take(span) {
                    *slot = cell.rowspan as usize;
                }
            }
            column += span;
        }
        while column < open.len() && open[column] > 0 {
            column += 1;
        }
        columns = columns.max(column);
    }
    columns
}

fn write_table(
    writer: &mut XmlWriter,
    table: &TableData,
    styles: &mut AutoStyles,
    media: &mut Media,
    notes: &NoteContext,
) {
    let columns = table_grid_columns(table).max(table.column_widths_pt.len()).max(1);
    let mut widths = table.column_widths_pt.clone();
    let fallback = if widths.is_empty() { 90.0 } else { widths.iter().sum::<f64>() / widths.len() as f64 };
    while widths.len() < columns {
        widths.push(fallback);
    }
    widths.truncate(columns);
    writer.raw("<table:table>");
    for (index, width) in widths.iter().enumerate() {
        writer.raw(&format!(
            "<table:table-column table:style-name=\"co{}\" style:column-width=\"{}\"/>",
            index,
            cm(*width)
        ));
    }
    // Rows the model stores contain only the cells that start in them; every
    // grid position a span covers gets a `table:covered-table-cell` placeholder.
    let mut open: Vec<usize> = Vec::new();
    for row in &table.rows {
        // `open` counts the rows still covered after the current one.
        for remaining in open.iter_mut() {
            *remaining = remaining.saturating_sub(1);
        }
        writer.raw("<table:table-row>");
        let mut column = 0usize;
        for cell in &row.cells {
            while column < open.len() && open[column] > 0 {
                writer.raw("<table:covered-table-cell/>");
                column += 1;
            }
            let span = cell.colspan.max(1) as usize;
            while open.len() < column + span {
                open.push(0);
            }
            let mut attributes = String::new();
            if cell.colspan > 1 {
                attributes.push_str(&format!(" table:number-columns-spanned=\"{}\"", cell.colspan));
            }
            if cell.rowspan > 1 {
                attributes.push_str(&format!(" table:number-rows-spanned=\"{}\"", cell.rowspan));
            }
            writer.raw(&format!("<table:table-cell office:value-type=\"string\"{attributes}>"));
            if cell.blocks.is_empty() {
                writer.raw("<text:p/>");
            } else {
                write_blocks(writer, &cell.blocks, styles, media, notes, 0);
            }
            writer.raw("</table:table-cell>");
            if cell.rowspan > 1 {
                for slot in open.iter_mut().skip(column).take(span) {
                    *slot = cell.rowspan as usize;
                }
            }
            column += span;
        }
        while column < open.len() && open[column] > 0 {
            writer.raw("<table:covered-table-cell/>");
            column += 1;
        }
        writer.raw("</table:table-row>");
    }
    writer.raw("</table:table>");
}

fn named_styles_xml(document: &TextDocument) -> String {
    let mut out = String::from("<office:styles>");
    out.push_str("<style:default-style style:family=\"paragraph\"><style:paragraph-properties/><style:text-properties fo:font-size=\"11pt\"/></style:default-style>");
    for style in &document.styles {
        let mut properties = String::new();
        if style.bold == Some(true) {
            properties.push_str(" fo:font-weight=\"bold\"");
        }
        if style.italic == Some(true) {
            properties.push_str(" fo:font-style=\"italic\"");
        }
        if let Some(font) = &style.font {
            properties.push_str(&format!(" fo:font-family=\"{}\"", escape(font)));
        }
        if let Some(size) = style.size_pt {
            properties.push_str(&format!(" fo:font-size=\"{size}pt\""));
        }
        if let Some(color) = &style.color {
            properties.push_str(&format!(" fo:color=\"{}\"", escape(color)));
        }
        let mut paragraph = String::new();
        if let Some(align) = &style.align {
            paragraph.push_str(&format!(" fo:text-align=\"{}\"", escape(align)));
        }
        if let Some(value) = style.space_before_pt {
            paragraph.push_str(&format!(" fo:margin-top=\"{}\"", cm(value)));
        }
        if let Some(value) = style.space_after_pt {
            paragraph.push_str(&format!(" fo:margin-bottom=\"{}\"", cm(value)));
        }
        if let Some(value) = style.line_spacing {
            paragraph.push_str(&format!(" fo:line-height=\"{:.0}%\"", value * 100.0));
        }
        if let Some(value) = style.keep_with_next {
            if value {
                paragraph.push_str(" fo:keep-with-next=\"always\"");
            }
        }
        let outline = style
            .outline_level
            .map(|level| format!(" style:default-outline-level=\"{}\"", level + 1))
            .unwrap_or_default();
        out.push_str(&format!(
            "<style:style style:name=\"{}\" style:family=\"paragraph\"{outline}>{}{}<style:text-properties{properties}/></style:style>",
            escape(&style.name),
            if paragraph.is_empty() { String::new() } else { format!("<style:paragraph-properties{paragraph}/>") },
            ""
        ));
        if let Some(parent) = &style.based_on {
            // basedOn is expressed via style:parent-style-name; rewrite is not
            // worth the complexity for the built-in catalogue.
            let _ = parent;
        }
    }
    out.push_str("<text:list-style style:name=\"LB\">");
    for level in 1..=9 {
        out.push_str(&format!("<text:list-level-style-bullet text:level=\"{level}\" text:bullet-char=\"•\"><style:list-level-properties text:space-before=\"{}cm\" text:min-label-width=\"0.6cm\"/></text:list-level-style-bullet>", (level as f64 - 1.0) * 0.6));
    }
    out.push_str("</text:list-style>");
    out.push_str("<text:list-style style:name=\"LN\">");
    for level in 1..=9 {
        out.push_str(&format!("<text:list-level-style-number text:level=\"{level}\" style:num-format=\"1\" style:num-suffix=\".\"><style:list-level-properties text:space-before=\"{}cm\" text:min-label-width=\"0.6cm\"/></text:list-level-style-number>", (level as f64 - 1.0) * 0.6));
    }
    out.push_str("</text:list-style>");
    out.push_str("</office:styles>");
    out
}

/// The automatic styles the watermark frame needs, declared in styles.xml.
fn watermark_styles_xml(watermark: &Watermark) -> String {
    let color = watermark.color.as_deref().and_then(crate::io::normalize_hex).unwrap_or_else(|| "#C0C0C0".into());
    let opacity = watermark.opacity.clamp(0.0, 1.0);
    let weight = if watermark.bold { "bold" } else { "normal" };
    format!(
        concat!(
            "<style:style style:name=\"WatermarkG\" style:family=\"graphic\">",
            "<style:graphic-properties style:wrap=\"run-through\" style:run-through=\"background\" draw:opacity=\"{}\"/>",
            "</style:style>",
            "<style:style style:name=\"WatermarkP\" style:family=\"paragraph\">",
            "<style:paragraph-properties fo:text-align=\"center\"/></style:style>",
            "<style:style style:name=\"WatermarkT\" style:family=\"text\">",
            "<style:text-properties fo:color=\"{}\" fo:font-size=\"{}pt\" fo:font-weight=\"{}\"/></style:style>"
        ),
        opacity,
        escape(&color),
        watermark.font_pt.max(1.0),
        weight,
    )
}

/// The visible watermark: a run-through frame with a rotated text box, written
/// into the default master page's header so LibreOffice draws it behind the
/// page content.
fn watermark_frame_xml(watermark: &Watermark) -> String {
    format!(
        concat!(
            "<draw:frame draw:name=\"Watermark\" text:anchor-type=\"page\" draw:style-name=\"WatermarkG\" draw:z-index=\"0\" ",
            "svg:width=\"16cm\" svg:height=\"6cm\" draw:transform=\"rotate({rotation})\">",
            "<draw:text-box><text:p text:style-name=\"WatermarkP\"><text:span text:style-name=\"WatermarkT\">{text}</text:span></text:p></draw:text-box>",
            "</draw:frame>"
        ),
        rotation = watermark.rotation,
        text = crate::xml::escape_text(&watermark.text),
    )
}

fn master_styles_xml(document: &TextDocument) -> String {
    let page = &document.page;
    let landscape = page.orientation == "landscape";
    let mut layout = format!(
        "<style:page-layout style:name=\"pm1\"><style:page-layout-properties fo:page-width=\"{}\" fo:page-height=\"{}\" style:print-orientation=\"{}\" fo:margin-top=\"{}\" fo:margin-bottom=\"{}\" fo:margin-left=\"{}\" fo:margin-right=\"{}\"",
        cm(if landscape { page.height_pt } else { page.width_pt }),
        cm(if landscape { page.width_pt } else { page.height_pt }),
        if landscape { "landscape" } else { "portrait" },
        cm(page.margin_top_pt),
        cm(page.margin_bottom_pt),
        cm(page.margin_left_pt),
        cm(page.margin_right_pt)
    );
    if page.columns > 1 {
        layout.push_str(&format!("><style:columns fo:column-count=\"{}\" fo:column-gap=\"{}\"/></style:page-layout-properties></style:page-layout>", page.columns, cm(page.column_spacing_pt)));
    } else {
        layout.push_str("/></style:page-layout>");
    }
    let mut automatic = String::new();
    let mut watermark = String::new();
    if let Some(value) = &document.watermark {
        automatic = format!("<office:automatic-styles>{}</office:automatic-styles>", watermark_styles_xml(value));
        watermark = watermark_frame_xml(value);
    }
    let mut header_footer = String::new();
    let mut media = Media::default();
    let notes = NoteContext::for_document(document);
    if !document.header.is_empty() || !watermark.is_empty() {
        let mut writer = XmlWriter::new();
        let mut styles = AutoStyles::default();
        write_blocks(&mut writer, &document.header, &mut styles, &mut media, &notes, 0);
        writer.raw(&watermark);
        header_footer.push_str(&format!("<style:header>{}</style:header>", writer.finish()));
    }
    if !document.footer.is_empty() {
        let mut writer = XmlWriter::new();
        let mut styles = AutoStyles::default();
        write_blocks(&mut writer, &document.footer, &mut styles, &mut media, &notes, 0);
        header_footer.push_str(&format!("<style:footer>{}</style:footer>", writer.finish()));
    }
    format!(
        "{automatic}<office:master-styles>{layout}<style:master-page style:name=\"Standard\" style:page-layout-name=\"pm1\">{header_footer}</style:master-page></office:master-styles>"
    )
}

fn manifest() -> String {
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\" manifest:version=\"1.2\">\
        <manifest:file-entry manifest:full-path=\"/\" manifest:media-type=\"application/vnd.oasis.opendocument.text\"/>\
        <manifest:file-entry manifest:full-path=\"content.xml\" manifest:media-type=\"text/xml\"/>\
        <manifest:file-entry manifest:full-path=\"styles.xml\" manifest:media-type=\"text/xml\"/>\
        <manifest:file-entry manifest:full-path=\"meta.xml\" manifest:media-type=\"text/xml\"/>\
        <manifest:file-entry manifest:full-path=\"Pictures/\" manifest:media-type=\"\"/>\
        <manifest:file-entry manifest:full-path=\"Images/\" manifest:media-type=\"\"/>\
        </manifest:manifest>".to_string()
}

fn manifest_for(mime: &str) -> String {
    manifest().replace("application/vnd.oasis.opendocument.text", mime)
}

/// The `meta:user-defined` entries that carry a watermark losslessly; the
/// visible run-through frame is only the fallback for foreign files.
fn watermark_meta_xml(watermark: &Watermark) -> String {
    let entries = [
        ("OSAK:Watermark:Text", watermark.text.clone()),
        ("OSAK:Watermark:Color", watermark.color.clone().unwrap_or_default()),
        ("OSAK:Watermark:Opacity", watermark.opacity.to_string()),
        ("OSAK:Watermark:Rotation", watermark.rotation.to_string()),
        ("OSAK:Watermark:FontPt", watermark.font_pt.to_string()),
        ("OSAK:Watermark:Bold", watermark.bold.to_string()),
    ];
    entries
        .iter()
        .map(|(name, value)| {
            format!("<meta:user-defined meta:name=\"{}\" meta:value=\"{}\"/>", escape(name), escape(value))
        })
        .collect()
}

fn meta_xml(title: &str, generator: &str, watermark: Option<&Watermark>) -> String {
    let user_defined = watermark.map(watermark_meta_xml).unwrap_or_default();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-meta {NS} office:version=\"1.2\"><office:meta><meta:generator>{}</meta:generator><dc:title>{}</dc:title>{user_defined}</office:meta></office:document-meta>",
        crate::xml::escape_text(generator),
        crate::xml::escape_text(title)
    )
}

fn settings_xml() -> String {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-settings {NS} office:version=\"1.2\"><office:settings/></office:document-settings>")
}

#[derive(Debug, Clone)]
pub struct TextRead {
    pub document: TextDocument,
    pub warnings: Vec<String>,
}

pub fn write_odt(document: &TextDocument) -> OfficeResult<Vec<u8>> {
    let mut styles = AutoStyles::default();
    let mut media = Media::default();
    let mut notes = NoteContext::for_document(document);
    notes.comments = CommentAnchors::for_document(document);
    let mut body = XmlWriter::new();
    write_blocks(&mut body, &document.blocks, &mut styles, &mut media, &notes, 0);
    // A body without any paragraph still needs a home for unanchored comments.
    let mut unanchored = XmlWriter::new();
    notes.comments.write_unanchored(&mut unanchored);
    let unanchored = unanchored.finish();
    if !unanchored.is_empty() {
        body.raw(&format!("<text:p>{unanchored}</text:p>"));
    }
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NS} {LOEXT_NS} office:version=\"1.2\">{}{}<office:body><office:text text:style-name=\"Standard\">{}</office:text></office:body></office:document-content>",
        styles.xml(),
        named_styles_xml(document),
        body.finish()
    );
    let styles_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NS} office:version=\"1.2\">{}{}</office:document-styles>",
        named_styles_xml(document),
        master_styles_xml(document)
    );
    let mut zip = ZipWriter::new();
    zip.add_text("mimetype", "application/vnd.oasis.opendocument.text");
    zip.add_text("META-INF/manifest.xml", &manifest());
    zip.add_text("content.xml", &content);
    zip.add_text("styles.xml", &styles_xml);
    zip.add_text("meta.xml", &meta_xml(&document.title, "OmniOffice", document.watermark.as_ref()));
    zip.add_text("settings.xml", &settings_xml());
    for (name, data) in &media.items {
        zip.add(&format!("Pictures/{name}"), data);
    }
    Ok(zip.finish())
}

pub fn write_odt_file(path: &Path, document: &TextDocument) -> OfficeResult<()> {
    crate::io::write_atomic(path, &write_odt(document)?)
}

// ---------------------------------------------------------------------------
// ODT import
// ---------------------------------------------------------------------------

/// A named style's graphic formatting, resolved from `style:graphic-properties`
/// so frames can report their text wrapping and the watermark can be detected.
#[derive(Default, Clone)]
struct GraphicStyle {
    /// The `style:wrap` value (`parallel`, `none`, `run-through`, ...).
    wrap: String,
    /// Whether the style puts the frame behind the text.
    run_through: bool,
    /// `draw:opacity`, 0..1.
    opacity: f64,
}

/// Named styles resolved from content.xml's automatic styles and styles.xml:
/// paragraph formatting, run formatting and graphic styles.
#[derive(Default)]
struct ReadStyles {
    paragraphs: HashMap<String, ParaProps>,
    texts: HashMap<String, Run>,
    graphics: HashMap<String, GraphicStyle>,
}

/// Overlays the attributes of one `style:text-properties` element onto `run`.
/// Attributes that are absent leave the inherited value alone.
fn apply_text_properties(run: &mut Run, properties: &XmlNode) {
    if let Some(weight) = properties.attr_any_ns("font-weight") {
        if weight == "bold" {
            run.bold = true;
        } else if weight == "normal" {
            run.bold = false;
        }
    }
    if let Some(style) = properties.attr_any_ns("font-style") {
        if style == "italic" || style == "oblique" {
            run.italic = true;
        } else if style == "normal" {
            run.italic = false;
        }
    }
    if let Some(font) = properties.attr_any_ns("font-family") {
        run.font = Some(font.to_string());
    }
    if let Some(size) = properties.attr_any_ns("font-size").and_then(parse_cm) {
        run.size_pt = Some(size);
    }
    if let Some(color) = properties.attr_any_ns("color") {
        run.color = crate::io::normalize_hex(color);
    }
    if let Some(background) = properties.attr_any_ns("background-color") {
        if !background.starts_with("transparent") {
            run.highlight = crate::io::normalize_hex(background);
        }
    }
    if properties.attr_any_ns("text-underline-style").is_some() {
        run.underline = true;
    }
    if properties.attr_any_ns("text-line-through-style").is_some() {
        run.strike = true;
    }
    if let Some(position) = properties.attr_any_ns("text-position") {
        if position.starts_with("super") {
            run.superscript = true;
            run.subscript = false;
        } else if position.starts_with("sub") {
            run.subscript = true;
            run.superscript = false;
        }
    }
}

/// The formatting of one span: the named text style first, then any direct
/// `style:text-properties` on the span itself. Spans without a style name keep
/// the plain inline-properties behavior.
fn read_span_style(node: &XmlNode, text_styles: &HashMap<String, Run>) -> Run {
    let mut run = node.attr_any_ns("style-name").and_then(|name| text_styles.get(name)).cloned().unwrap_or_default();
    let mut properties = Vec::new();
    node.find_all("style:text-properties", &mut properties);
    let Some(properties) = properties.first() else { return run };
    apply_text_properties(&mut run, properties);
    run
}

/// Reader-side note and comment accumulator. Notes and comments are collected
/// while paragraphs are parsed and attached to the document once the whole
/// body is read.
#[derive(Default)]
struct NoteReadState {
    footnotes: Vec<Footnote>,
    endnotes: Vec<Footnote>,
    sequence: usize,
    comments: Vec<Comment>,
    /// Ids already used by `comments`, so a new id is checked in constant time
    /// (a file with thousands of annotations used to take quadratic time).
    comment_ids: HashSet<String>,
    /// Next number tried for an unnamed annotation's `odt-comment-N` id.
    next_comment_number: usize,
    /// Names of the annotations that have an `office:annotation-end` in the
    /// part being read; any other annotation is a point comment.
    ranged_comments: std::collections::HashSet<String>,
    /// Ranged comments open at the current position, innermost last. Ranges
    /// may span paragraphs.
    open_comments: Vec<String>,
    /// A point comment waiting for the next run with content.
    pending_comment: Option<String>,
}

impl NoteReadState {
    /// Prepares comment anchoring for a new part (content.xml or styles.xml).
    fn start_comment_part(&mut self, root: &XmlNode) {
        let mut ends = Vec::new();
        root.find_all("annotation-end", &mut ends);
        self.ranged_comments = ends.iter().filter_map(|end| end.attr_any_ns("name")).map(str::to_string).collect();
        self.open_comments.clear();
        self.pending_comment = None;
    }

    /// Resolves the annotation markers in one paragraph's runs.
    fn anchor_paragraph(&mut self, runs: &mut Vec<Run>) {
        anchor_comments(runs, &self.ranged_comments, &mut self.open_comments, &mut self.pending_comment);
    }
}

/// `FieldRef::kind` of the placeholder runs `node_text_runs` leaves where an
/// `office:annotation` / `office:annotation-end` sits. `anchor_comments` turns
/// them into `Run::comment` anchors and removes them before a paragraph is
/// stored; the ODT reader produces no other field runs.
const ANNOTATION_START: &str = "odf-annotation-start";
const ANNOTATION_END: &str = "odf-annotation-end";

fn annotation_marker(kind: &str, name: &str) -> Run {
    Run {
        field: Some(FieldRef { kind: kind.into(), target: name.into(), cached: String::new() }),
        ..Default::default()
    }
}

/// Replaces the annotation markers in `runs` with anchors. Runs between an
/// annotation and its end marker belong to that comment (the innermost one
/// when ranges nest). An annotation without an end marker is a point comment
/// and anchors the next run with content, the way DOCX import attaches a
/// comment range start to the following text.
fn anchor_comments(
    runs: &mut Vec<Run>,
    ranged: &std::collections::HashSet<String>,
    open: &mut Vec<String>,
    pending: &mut Option<String>,
) {
    let mut anchored = Vec::with_capacity(runs.len());
    for mut run in std::mem::take(runs) {
        let marker = run.field.as_ref().filter(|field| field.kind == ANNOTATION_START || field.kind == ANNOTATION_END);
        if let Some(field) = marker {
            if field.kind == ANNOTATION_END {
                open.retain(|id| *id != field.target);
            } else if ranged.contains(&field.target) {
                open.push(field.target.clone());
            } else {
                *pending = Some(field.target.clone());
            }
            continue;
        }
        if run.comment.is_none() {
            run.comment = open.last().cloned();
        }
        if run.comment.is_none() && (!run.text.is_empty() || run.footnote.is_some() || run.endnote.is_some()) {
            run.comment = pending.take();
        }
        anchored.push(run);
    }
    *runs = anchored;
}

/// Plain text of one annotation paragraph (spaces, tabs and line breaks
/// included).
fn annotation_paragraph_text(paragraph: &XmlNode, styles: &ReadStyles) -> String {
    let mut runs = Vec::new();
    let mut scratch = NoteReadState::default();
    for inner in &paragraph.children {
        node_text_runs(inner, &mut runs, &mut scratch, styles);
    }
    let mut text = paragraph.text.clone();
    for run in runs {
        text.push_str(&run.text);
    }
    text
}

/// Reads one inline `office:annotation` into `notes.comments` and leaves a
/// start marker in `runs`, so the author, date and comment body never become
/// paragraph text. `office:name` is the comment id (our writer stores the id
/// there); replies written as "Re: author: text" paragraphs become `replies`
/// again, as in DOCX import, and `loext:resolved` is LibreOffice's flag.
fn read_annotation(node: &XmlNode, runs: &mut Vec<Run>, notes: &mut NoteReadState, styles: &ReadStyles) {
    let name = node.attr_any_ns("name").filter(|name| !name.is_empty());
    let id = match name {
        Some(name) if !notes.comment_ids.contains(name) => name.to_string(),
        // Unnamed (or duplicate) annotations get a fresh id and, having no
        // end marker of their own, anchor as point comments.
        _ => loop {
            notes.next_comment_number = notes.next_comment_number.max(notes.comments.len()) + 1;
            let candidate = format!("odt-comment-{}", notes.next_comment_number);
            if !notes.comment_ids.contains(&candidate) {
                break candidate;
            }
        },
    };
    notes.comment_ids.insert(id.clone());
    let created = node.child("date").map(XmlNode::deep_text).unwrap_or_default().trim().to_string();
    let mut comment = Comment {
        id: id.clone(),
        author: node.child("creator").map(XmlNode::deep_text).unwrap_or_else(|| "Unknown".into()),
        text: String::new(),
        created: created.clone(),
        resolved: node.attr_any_ns("resolved") == Some("true"),
        modified: created,
        replies: Vec::new(),
    };
    let mut paragraphs = Vec::new();
    node.find_all("p", &mut paragraphs);
    let mut lines = Vec::new();
    for paragraph in paragraphs {
        let text = annotation_paragraph_text(paragraph, styles);
        match text.strip_prefix("Re: ").and_then(|rest| rest.split_once(": ")) {
            Some((author, reply)) => comment.replies.push(CommentReply {
                author: author.to_string(),
                text: reply.to_string(),
                created: String::new(),
            }),
            None => lines.push(text),
        }
    }
    comment.text = lines.join("\n");
    notes.comments.push(comment);
    runs.push(annotation_marker(ANNOTATION_START, &id));
}

/// Reads one inline `text:note`: a reference run goes into `runs` at this
/// position while the body goes to `notes`, so note text never leaks into the
/// surrounding paragraph.
fn read_note(node: &XmlNode, runs: &mut Vec<Run>, notes: &mut NoteReadState, styles: &ReadStyles) {
    let endnote = node.attr_any_ns("note-class") == Some("endnote");
    notes.sequence += 1;
    let id = node
        .attr_any_ns("id")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}{}", if endnote { "en" } else { "fn" }, notes.sequence));
    let number = if endnote { notes.endnotes.len() + 1 } else { notes.footnotes.len() + 1 };
    let citation = node.child("note-citation").map(XmlNode::deep_text).unwrap_or_default();
    let citation = citation.trim().to_string();
    // Keep an explicit marker only when it is not the automatic number; that
    // way our own exports (whose citation *is* the number) stay automatic.
    let marker =
        if citation.is_empty() || citation.parse::<usize>().ok() == Some(number) { String::new() } else { citation };
    let mut body_runs = Vec::new();
    if let Some(body) = node.child("note-body") {
        let mut first_paragraph = true;
        for paragraph in body.children.iter().filter(|child| matches!(child.local_name(), "p" | "h")) {
            if !first_paragraph {
                body_runs.push(Run { text: "\n".into(), ..Default::default() });
            }
            first_paragraph = false;
            if !paragraph.text.is_empty() {
                body_runs.push(Run { text: paragraph.text.clone(), ..Default::default() });
            }
            for inner in &paragraph.children {
                node_text_runs(inner, &mut body_runs, notes, styles);
            }
        }
    }
    // The note body is read before the surrounding paragraph is anchored, so
    // its markers are resolved on their own and body ranges do not reach in.
    anchor_comments(&mut body_runs, &notes.ranged_comments, &mut Vec::new(), &mut None);
    if body_runs.is_empty() {
        body_runs.push(Run::default());
    }
    let note = Footnote { id: id.clone(), runs: body_runs, marker };
    if endnote {
        notes.endnotes.push(note);
        runs.push(Run { endnote: Some(id), ..Default::default() });
    } else {
        notes.footnotes.push(note);
        runs.push(Run { footnote: Some(id), ..Default::default() });
    }
}

fn node_text_runs(node: &XmlNode, runs: &mut Vec<Run>, notes: &mut NoteReadState, styles: &ReadStyles) {
    match node.local_name() {
        "span" => {
            let base = read_span_style(node, &styles.texts);
            if !node.text.is_empty() {
                runs.push(Run { text: node.text.clone(), ..base.clone() });
            }
            for child in &node.children {
                match child.local_name() {
                    "s" => {
                        let count = child.attr_any_ns("c").and_then(|value| value.parse::<usize>().ok()).unwrap_or(1);
                        runs.push(Run { text: " ".repeat(count), ..base.clone() });
                    }
                    "tab" => runs.push(Run { text: "\t".into(), ..base.clone() }),
                    "line-break" => runs.push(Run { text: "\n".into(), ..base.clone() }),
                    "note" => read_note(child, runs, notes, styles),
                    "annotation" | "annotation-end" => node_text_runs(child, runs, notes, styles),
                    _ => {
                        let text = child.deep_text();
                        if !text.is_empty() {
                            runs.push(Run { text, ..base.clone() });
                        }
                    }
                }
            }
        }
        "a" => {
            let link = node.attr("href").map(str::to_string);
            if !node.text.is_empty() {
                runs.push(Run { text: node.text.clone(), link: link.clone(), ..Default::default() });
            }
            for child in &node.children {
                let mut inner = Vec::new();
                node_text_runs(child, &mut inner, notes, styles);
                for mut run in inner {
                    if run.link.is_none() {
                        run.link = link.clone();
                    }
                    runs.push(run);
                }
            }
        }
        "note" => read_note(node, runs, notes, styles),
        // Comments must not reach the catch-all below, which would fold their
        // author, date and text into the paragraph.
        "annotation" => read_annotation(node, runs, notes, styles),
        "annotation-end" => runs.push(annotation_marker(ANNOTATION_END, node.attr_any_ns("name").unwrap_or_default())),
        "s" => {
            let count = node.attr_any_ns("c").and_then(|value| value.parse::<usize>().ok()).unwrap_or(1);
            runs.push(Run { text: " ".repeat(count), ..Default::default() });
        }
        "tab" => runs.push(Run { text: "\t".into(), ..Default::default() }),
        "line-break" => runs.push(Run { text: "\n".into(), ..Default::default() }),
        "page-break" => runs.push(Run { text: "\n".into(), ..Default::default() }),
        _ => {
            let text = node.direct_text();
            if !text.is_empty() {
                runs.push(Run { text, ..Default::default() });
            }
            for child in &node.children {
                node_text_runs(child, runs, notes, styles);
            }
        }
    }
}

fn paragraph_props(node: &XmlNode, styles: &ReadStyles) -> ParaProps {
    if let Some(name) = node.attr_any_ns("style-name") {
        styles.paragraphs.get(name).cloned().unwrap_or_default()
    } else {
        ParaProps::default()
    }
}

/// The wrapping of a frame's graphic style, mapped to the model's `inline` /
/// `square` / `topBottom`.
fn frame_wrap(frame: &XmlNode, styles: &ReadStyles) -> String {
    let wrap = frame
        .attr_any_ns("style-name")
        .and_then(|name| styles.graphics.get(name))
        .map(|style| style.wrap.as_str())
        .unwrap_or_default();
    match wrap {
        "parallel" | "dynamic" | "left" | "right" | "biggest" => "square".into(),
        "none" | "run-through" => "topBottom".into(),
        _ => "inline".into(),
    }
}

/// Whether a frame's graphic style puts it behind the text (a watermark or
/// another decoration that is not document content).
fn frame_run_through(frame: &XmlNode, styles: &ReadStyles) -> bool {
    let styled = frame
        .attr_any_ns("style-name")
        .and_then(|name| styles.graphics.get(name))
        .map(|style| style.run_through)
        .unwrap_or(false);
    styled || frame.attr_any_ns("run-through") == Some("background")
}

/// Reads a `draw:frame` (or a bare `draw:image`) into a `Block::Image`. `None`
/// when the node holds no image or its bytes are missing from the package.
fn read_image_block(
    node: &XmlNode,
    styles: &ReadStyles,
    reader: &ZipReader,
    warnings: &mut Vec<String>,
) -> Option<Block> {
    let mut images = Vec::new();
    node.find_all("image", &mut images);
    let image_node = images.first()?;
    let href = image_node.attr_any_ns("href")?;
    let width = node.attr_any_ns("width").and_then(parse_cm).unwrap_or(320.0);
    let height = node.attr_any_ns("height").and_then(parse_cm).unwrap_or(200.0);
    let path = href.trim_start_matches("./");
    let data = match reader.read(path) {
        Ok(data) => data,
        Err(_) => {
            warnings.push("An embedded image could not be read from the package.".into());
            return None;
        }
    };
    let name = path.rsplit('/').next().unwrap_or("image.png").to_string();
    Some(Block::Image {
        image: ImageData::from_bytes(&name, &data),
        width_pt: width,
        height_pt: height,
        align: "center".into(),
        caption: String::new(),
        wrap: frame_wrap(node, styles),
    })
}

fn read_blocks(
    node: &XmlNode,
    styles: &ReadStyles,
    reader: &ZipReader,
    warnings: &mut Vec<String>,
    notes: &mut NoteReadState,
) -> Vec<Block> {
    let mut blocks = Vec::new();
    for child in &node.children {
        match child.local_name() {
            "p" | "h" => {
                let mut runs = Vec::new();
                if !child.text.is_empty() {
                    runs.push(Run { text: child.text.clone(), ..Default::default() });
                }
                // Images written inside a paragraph (`<text:p><draw:frame>`)
                // used to be dropped: the frame walker only produced text. A
                // frame with an image becomes a block of its own, and the
                // paragraph is kept when it still has text.
                let mut images = Vec::new();
                for inner in &child.children {
                    if matches!(inner.local_name(), "frame" | "image") {
                        if frame_run_through(inner, styles) {
                            // Behind-text decoration (the watermark), read
                            // separately from meta.xml or the frame itself.
                            continue;
                        }
                        if let Some(image) = read_image_block(inner, styles, reader, warnings) {
                            images.push(image);
                            continue;
                        }
                    }
                    node_text_runs(inner, &mut runs, notes, styles);
                }
                notes.anchor_paragraph(&mut runs);
                let mut props = paragraph_props(child, styles);
                if child.local_name() == "h" {
                    let level =
                        child.attr_any_ns("outline-level").and_then(|value| value.parse::<u32>().ok()).unwrap_or(1);
                    props.style = format!("Heading{level}");
                }
                let has_text = runs
                    .iter()
                    .any(|run| !run.text.trim().is_empty() || run.footnote.is_some() || run.endnote.is_some());
                let has_images = !images.is_empty();
                blocks.extend(images);
                if !has_images || has_text {
                    blocks.push(Block::Paragraph { props, runs });
                }
            }
            "list" => {
                for item in child.children_named("list-item") {
                    for inner in &item.children {
                        if inner.local_name() == "p" || inner.local_name() == "h" {
                            let mut runs = Vec::new();
                            if !inner.text.is_empty() {
                                runs.push(Run { text: inner.text.clone(), ..Default::default() });
                            }
                            for part in &inner.children {
                                node_text_runs(part, &mut runs, notes, styles);
                            }
                            notes.anchor_paragraph(&mut runs);
                            let mut props = paragraph_props(inner, styles);
                            let level =
                                inner.attr_any_ns("level").and_then(|value| value.parse::<u32>().ok()).unwrap_or(0);
                            props.list =
                                Some(ListInfo { kind: "bullet".into(), level, start: 1, marker: "•".into() });
                            blocks.push(Block::Paragraph { props, runs });
                        }
                    }
                }
            }
            "list-header" => {}
            "table" => {
                let mut widths = Vec::new();
                for column in child.children_named("table-column") {
                    let width = column.attr_any_ns("column-width").and_then(parse_cm).unwrap_or(90.0);
                    let repeat = column
                        .attr_any_ns("number-columns-repeated")
                        .and_then(|value| value.parse::<usize>().ok())
                        .unwrap_or(1)
                        .clamp(1, 1024);
                    for _ in 0..repeat {
                        widths.push(width);
                    }
                }
                let mut rows: Vec<TableRow> = Vec::new();
                // Per grid column, the origin cell an open rowspan continues
                // (row index in `rows`, cell index in that row).
                let mut open_origins: Vec<Option<(usize, usize)>> = Vec::new();
                let mut max_column = 0usize;
                for row_node in child.children_named("table-row") {
                    let mut row = TableRow::default();
                    let previous_origins = std::mem::take(&mut open_origins);
                    // Origins started in this row, applied once the row is stored.
                    let mut pending_origins: Vec<(usize, usize, usize)> = Vec::new();
                    let mut column = 0usize;
                    for cell_node in row_node
                        .children
                        .iter()
                        .filter(|node| matches!(node.local_name(), "table-cell" | "covered-table-cell"))
                    {
                        let mut cell = TableCell::default();
                        if let Some(span) =
                            cell_node.attr_any_ns("number-columns-spanned").and_then(|value| value.parse::<u32>().ok())
                        {
                            cell.colspan = span.max(1);
                        }
                        if let Some(span) =
                            cell_node.attr_any_ns("number-rows-spanned").and_then(|value| value.parse::<u32>().ok())
                        {
                            cell.rowspan = span.max(1);
                        }
                        let span = cell.colspan.max(1) as usize;
                        if cell_node.local_name() == "covered-table-cell" {
                            // A covered position continues the origin from the
                            // row above. `number-rows-spanned` already counts
                            // every row, so the origin only grows when the
                            // count is missing (defensive) or too small. One
                            // with no origin at all is malformed and skipped.
                            if let Some(origin) = previous_origins.get(column).copied().flatten() {
                                let spanned = (rows.len() - origin.0 + 1) as u32;
                                if let Some(target) =
                                    rows.get_mut(origin.0).and_then(|origin_row| origin_row.cells.get_mut(origin.1))
                                {
                                    target.rowspan = target.rowspan.max(spanned);
                                }
                                while open_origins.len() < column + span {
                                    open_origins.push(None);
                                }
                                for slot in open_origins.iter_mut().skip(column).take(span) {
                                    *slot = Some(origin);
                                }
                            }
                            column += span;
                            continue;
                        }
                        let mut nested_warnings = Vec::new();
                        cell.blocks = read_blocks(cell_node, styles, reader, &mut nested_warnings, notes);
                        if cell.blocks.is_empty() {
                            cell.blocks.push(Block::paragraph(""));
                        }
                        let cell_index = row.cells.len();
                        row.cells.push(cell);
                        pending_origins.push((column, span, cell_index));
                        column += span;
                    }
                    if !row.cells.is_empty() {
                        let row_index = rows.len();
                        rows.push(row);
                        for (column, span, cell_index) in pending_origins {
                            while open_origins.len() < column + span {
                                open_origins.push(None);
                            }
                            for slot in open_origins.iter_mut().skip(column).take(span) {
                                *slot = Some((row_index, cell_index));
                            }
                        }
                    }
                    max_column = max_column.max(column);
                }
                while widths.len() < max_column.max(1) {
                    widths.push(90.0);
                }
                blocks.push(Block::Table {
                    table: TableData {
                        rows,
                        column_widths_pt: widths,
                        borders: true,
                        border_color: "#94A3B8".into(),
                        align: "left".into(),
                    },
                });
            }
            "section" => {
                blocks.extend(read_blocks(child, styles, reader, warnings, notes));
            }
            // Not valid ODF outside a paragraph: keep the comment, unanchored.
            "annotation" => read_annotation(child, &mut Vec::new(), notes, styles),
            _ => {
                if child.local_name() == "frame" {
                    if frame_run_through(child, styles) {
                        // The watermark (or another behind-text decoration) is
                        // not document content.
                        continue;
                    }
                    let mut images = Vec::new();
                    child.find_all("image", &mut images);
                    if !images.is_empty() {
                        if let Some(image) = read_image_block(child, styles, reader, warnings) {
                            blocks.push(image);
                        }
                    } else {
                        warnings.push("Text boxes and embedded objects are imported as plain content.".into());
                        let mut inner = Vec::new();
                        for part in &child.children {
                            if part.local_name() == "text-box" {
                                inner.extend(read_blocks(part, styles, reader, warnings, notes));
                            }
                        }
                        blocks.extend(inner);
                    }
                }
            }
        }
    }
    blocks
}

fn collect_styles(root: &XmlNode, styles: &mut ReadStyles) {
    let mut nodes = Vec::new();
    root.find_all("style", &mut nodes);
    for style in nodes {
        // `style:name` is the real attribute; `name` never exists and used to
        // make every named style resolve to the default.
        let Some(name) = style.attr_any_ns("name") else { continue };
        let mut props = ParaProps::default();
        let mut properties = Vec::new();
        style.find_all("paragraph-properties", &mut properties);
        if let Some(properties) = properties.first() {
            if let Some(align) = properties.attr_any_ns("text-align") {
                props.align = match align {
                    "center" => "center".into(),
                    "right" => "right".into(),
                    "justify" => "justify".into(),
                    _ => "left".into(),
                };
            }
            if let Some(value) = properties.attr_any_ns("margin-top").and_then(parse_cm) {
                props.space_before_pt = value;
            }
            if let Some(value) = properties.attr_any_ns("margin-bottom").and_then(parse_cm) {
                props.space_after_pt = value;
            }
            if let Some(value) = properties.attr_any_ns("margin-left").and_then(parse_cm) {
                props.indent_left_pt = value;
            }
            if let Some(value) = properties.attr_any_ns("text-indent").and_then(parse_cm) {
                props.first_line_pt = value;
            }
            if let Some(value) = properties.attr_any_ns("line-height") {
                if let Ok(percent) = value.trim_end_matches('%').parse::<f64>() {
                    props.line_spacing = percent / 100.0;
                }
            }
            if properties.attr_any_ns("break-before") == Some("page") {
                props.page_break_before = true;
            }
            if let Some(tab_stops) = properties.child("tab-stops") {
                for tab in tab_stops.children_named("tab-stop") {
                    let pos_pt = tab.attr_any_ns("position").and_then(parse_cm).unwrap_or(0.0);
                    let align = match tab.attr_any_ns("type") {
                        Some("center") => "center",
                        Some("right") => "right",
                        Some("char") => "decimal",
                        _ => "left",
                    };
                    props.tabs.push(TabStop { pos_pt, align: align.into() });
                }
            }
        }
        styles.paragraphs.insert(name.to_string(), props);
        let mut text_properties = Vec::new();
        style.find_all("text-properties", &mut text_properties);
        if let Some(properties) = text_properties.first() {
            let mut run = Run::default();
            apply_text_properties(&mut run, properties);
            styles.texts.insert(name.to_string(), run);
        }
        let mut graphic_properties = Vec::new();
        style.find_all("graphic-properties", &mut graphic_properties);
        if let Some(properties) = graphic_properties.first() {
            let wrap = properties.attr_any_ns("wrap").unwrap_or_default().to_string();
            let run_through = properties.attr_any_ns("run-through") == Some("background");
            let opacity = properties.attr_any_ns("opacity").and_then(parse_opacity).unwrap_or(1.0);
            styles.graphics.insert(name.to_string(), GraphicStyle { wrap, run_through, opacity });
        }
    }
}

/// A `draw:opacity` value, which may be a fraction (`0.18`) or a percentage
/// (`18%`).
fn parse_opacity(value: &str) -> Option<f64> {
    let value = value.trim();
    if let Some(percent) = value.strip_suffix('%') {
        return percent.trim().parse::<f64>().ok().map(|part| (part / 100.0).clamp(0.0, 1.0));
    }
    value.parse::<f64>().ok().map(|part| if part > 1.0 { (part / 100.0).clamp(0.0, 1.0) } else { part.clamp(0.0, 1.0) })
}

/// `draw:transform="rotate(45)"` -> 45.0. Transforms may list several
/// operations; the angle is in degrees.
fn parse_rotate(transform: &str) -> Option<f64> {
    let start = transform.find("rotate(")? + "rotate(".len();
    let rest = &transform[start..];
    let end = rest.find(')')?;
    rest[..end].trim().trim_end_matches("deg").trim().parse::<f64>().ok()
}

/// The watermark our writer persisted in `meta:user-defined` entries. The
/// values are preferred over the visible frame because they are lossless.
fn watermark_from_meta(root: &XmlNode) -> Option<Watermark> {
    let mut values = HashMap::new();
    let mut nodes = Vec::new();
    root.find_all("user-defined", &mut nodes);
    for node in nodes {
        if let (Some(name), Some(value)) = (node.attr_any_ns("name"), node.attr_any_ns("value")) {
            values.insert(name.to_string(), value.to_string());
        }
    }
    let text = values.get("OSAK:Watermark:Text")?.clone();
    Some(Watermark {
        text,
        color: values.get("OSAK:Watermark:Color").filter(|color| !color.is_empty()).cloned(),
        opacity: values
            .get("OSAK:Watermark:Opacity")
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or_else(|| Watermark::default().opacity),
        rotation: values
            .get("OSAK:Watermark:Rotation")
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or_else(|| Watermark::default().rotation),
        font_pt: values
            .get("OSAK:Watermark:FontPt")
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or_else(|| Watermark::default().font_pt),
        bold: values.get("OSAK:Watermark:Bold").map(|value| value == "true").unwrap_or(true),
    })
}

/// Fallback for foreign packages: the first run-through frame in the master
/// styles whose text is not empty is the watermark.
fn watermark_from_frame(root: &XmlNode, styles: &ReadStyles) -> Option<Watermark> {
    let mut frames = Vec::new();
    root.find_all("frame", &mut frames);
    for frame in frames {
        if !frame_run_through(frame, styles) {
            continue;
        }
        let graphic = frame.attr_any_ns("style-name").and_then(|name| styles.graphics.get(name));
        let text = frame.deep_text().trim().to_string();
        if text.is_empty() {
            continue;
        }
        let mut spans = Vec::new();
        frame.find_all("span", &mut spans);
        let text_style = spans
            .first()
            .and_then(|span| span.attr_any_ns("style-name"))
            .and_then(|name| styles.texts.get(name))
            .cloned()
            .unwrap_or_default();
        return Some(Watermark {
            text,
            color: text_style.color,
            opacity: graphic.map(|style| style.opacity).unwrap_or(1.0).clamp(0.0, 1.0),
            rotation: frame
                .attr_any_ns("transform")
                .and_then(parse_rotate)
                .unwrap_or_else(|| Watermark::default().rotation),
            font_pt: text_style.size_pt.unwrap_or_else(|| Watermark::default().font_pt),
            bold: text_style.bold,
        });
    }
    None
}

/// `parse_xml` appends text that follows a child element to the parent's
/// `text`, so mixed paragraph content loses its order: "a <span>b</span> c"
/// reads as "a c" + "b", and notes and annotation anchors land in the wrong
/// place. Before an ODT part is parsed, text that follows a child element
/// inside `text:p`, `text:h`, `text:span` or `text:a` is wrapped in an
/// unstyled `text:span`, which the run walker reads in document order. The
/// rest of the part is copied byte for byte; XML the tokenizer rejects is
/// returned unchanged so `parse_xml` reports it.
fn wrap_trailing_text(xml: &str) -> String {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut out = String::with_capacity(xml.len() + xml.len() / 16);
    let mut copied = 0usize;
    // Per open element: (holds paragraph content, has had a child element).
    let mut stack: Vec<(bool, bool)> = Vec::new();
    let mut wrapping = false;
    loop {
        let before = reader.buffer_position() as usize;
        let event = match reader.read_event() {
            Ok(Event::Eof) => break,
            Ok(event) => event,
            Err(_) => return xml.to_string(),
        };
        let is_text = matches!(event, Event::Text(_) | Event::CData(_) | Event::GeneralRef(_));
        if is_text && !wrapping && stack.last() == Some(&(true, true)) {
            out.push_str(&xml[copied..before]);
            out.push_str("<text:span>");
            copied = before;
            wrapping = true;
        } else if !is_text && wrapping {
            out.push_str(&xml[copied..before]);
            out.push_str("</text:span>");
            copied = before;
            wrapping = false;
        }
        match &event {
            Event::Start(start) => {
                if let Some(parent) = stack.last_mut() {
                    parent.1 = true;
                }
                stack.push((matches!(start.local_name().into_inner(), "p" | "h" | "span" | "a"), false));
            }
            Event::Empty(_) => {
                if let Some(parent) = stack.last_mut() {
                    parent.1 = true;
                }
            }
            Event::End(_) => {
                stack.pop();
            }
            _ => {}
        }
    }
    out.push_str(&xml[copied..]);
    if wrapping {
        out.push_str("</text:span>");
    }
    out
}

pub fn read_odt(bytes: &[u8]) -> OfficeResult<TextRead> {
    let reader = ZipReader::open(bytes.to_vec())?;
    if !reader.contains("content.xml") {
        return Err(OfficeError::corrupt("The package does not contain content.xml (not an ODF document)."));
    }
    let mut warnings = Vec::new();
    let mut read_styles = ReadStyles::default();
    if let Ok(text) = reader.read_text("styles.xml") {
        if let Ok(root) = parse_xml(&text) {
            collect_styles(&root, &mut read_styles);
        }
    }
    let text = reader.read_text("content.xml")?;
    let root = parse_xml(&wrap_trailing_text(&text))?;
    collect_styles(&root, &mut read_styles);
    let mut document = TextDocument::new_blank("Imported document");
    let mut note_state = NoteReadState::default();
    note_state.start_comment_part(&root);
    let container = root.child("body").and_then(|body| body.child("text")).unwrap_or(&root);
    document.blocks = read_blocks(container, &read_styles, &reader, &mut warnings, &mut note_state);
    if document.blocks.is_empty() {
        document.blocks.push(Block::paragraph(""));
    }
    if let Ok(meta) = reader.read_text("meta.xml") {
        if let Ok(root) = parse_xml(&meta) {
            if let Some(title) = root.child("dc:title").map(XmlNode::deep_text) {
                if !title.is_empty() {
                    document.title = title.clone();
                    document.metadata.title = title;
                }
            }
            if let Some(author) = root.child("dc:creator").map(XmlNode::deep_text) {
                document.metadata.author = author;
            }
            if let Some(watermark) = watermark_from_meta(&root) {
                document.watermark = Some(watermark);
            }
        }
    }
    // Page setup from the master page.
    if let Ok(styles) = reader.read_text("styles.xml") {
        if let Ok(root) = parse_xml(&wrap_trailing_text(&styles)) {
            note_state.start_comment_part(&root);
            let mut layouts = Vec::new();
            root.find_all("page-layout-properties", &mut layouts);
            if let Some(properties) = layouts.first() {
                if let Some(value) = properties.attr_any_ns("page-width").and_then(parse_cm) {
                    document.page.width_pt = value;
                }
                if let Some(value) = properties.attr_any_ns("page-height").and_then(parse_cm) {
                    document.page.height_pt = value;
                }
                if let Some(value) = properties.attr_any_ns("margin-top").and_then(parse_cm) {
                    document.page.margin_top_pt = value;
                }
                if let Some(value) = properties.attr_any_ns("margin-bottom").and_then(parse_cm) {
                    document.page.margin_bottom_pt = value;
                }
                if let Some(value) = properties.attr_any_ns("margin-left").and_then(parse_cm) {
                    document.page.margin_left_pt = value;
                }
                if let Some(value) = properties.attr_any_ns("margin-right").and_then(parse_cm) {
                    document.page.margin_right_pt = value;
                }
                document.page.orientation = if document.page.width_pt > document.page.height_pt {
                    "landscape".into()
                } else {
                    "portrait".into()
                };
                document.page.size = "custom".into();
            }
            // Header / footer content.
            let mut headers = Vec::new();
            root.find_all("header", &mut headers);
            if let Some(header) = headers.first() {
                document.header = read_blocks(header, &read_styles, &reader, &mut warnings, &mut note_state);
            }
            let mut footers = Vec::new();
            root.find_all("footer", &mut footers);
            if let Some(footer) = footers.first() {
                document.footer = read_blocks(footer, &read_styles, &reader, &mut warnings, &mut note_state);
            }
            // Foreign packages carry no meta watermark entries; the visible
            // run-through frame in the master styles is the fallback.
            if document.watermark.is_none() {
                document.watermark = watermark_from_frame(&root, &read_styles);
            }
        }
    }
    document.footnotes = note_state.footnotes;
    document.endnotes = note_state.endnotes;
    document.comments = note_state.comments;
    if reader.names().any(|name| name.starts_with("Object")) {
        warnings.push("Embedded objects in the document were not imported.".into());
    }
    warnings.sort();
    warnings.dedup();
    Ok(TextRead { document, warnings })
}

pub fn read_odt_file(path: &Path) -> OfficeResult<TextRead> {
    let bytes = crate::io::read_bytes(path)?;
    let mut result = read_odt(&bytes)?;
    if result.document.title.starts_with("Imported") {
        result.document.title = crate::io::file_stem(path);
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// ODS
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SheetRead {
    pub workbook: Workbook,
    pub warnings: Vec<String>,
}

fn odf_formula_to_ours(formula: &str) -> String {
    crate::formula::from_odf(formula)
}

fn our_formula_to_odf(formula: &str) -> String {
    crate::formula::to_odf(formula)
}

fn cell_style_xml(style: &CellStyle) -> String {
    let mut text = String::new();
    if style.bold {
        text.push_str(" fo:font-weight=\"bold\"");
    }
    if style.italic {
        text.push_str(" fo:font-style=\"italic\"");
    }
    if style.underline {
        text.push_str(" style:text-underline-style=\"solid\"");
    }
    if let Some(size) = style.size_pt {
        text.push_str(&format!(" fo:font-size=\"{size}pt\""));
    }
    if let Some(color) = &style.color {
        text.push_str(&format!(" fo:color=\"{}\"", escape(color)));
    }
    let mut cell = String::new();
    if let Some(fill) = &style.fill {
        cell.push_str(&format!(" fo:background-color=\"{}\"", escape(fill)));
    }
    match style.align.as_str() {
        "center" => cell.push_str(" style:text-align=\"center\""),
        "right" => cell.push_str(" style:text-align=\"right\""),
        _ => {}
    }
    if style.wrap {
        cell.push_str(" fo:wrap-option=\"wrap\"");
    }
    if let Some(border) = &style.borders.top {
        if border.style != "none" {
            cell.push_str(&format!(" fo:border-top=\"0.05pt solid {}\"", escape(&border.color)));
        }
    }
    if let Some(border) = &style.borders.bottom {
        if border.style != "none" {
            cell.push_str(&format!(" fo:border-bottom=\"0.05pt solid {}\"", escape(&border.color)));
        }
    }
    format!("<style:table-cell-properties{cell}/><style:text-properties{text}/>")
}

// ---------------------------------------------------------------------------
// ODS charts (embedded chart objects) and pivot output
// ---------------------------------------------------------------------------

/// Namespaces a chart sub-document needs on top of [`NS`]. `loext` carries the
/// literal series names; ODF 1.2 can only name a series through a cell.
const CHART_NS: &str = concat!(
    "xmlns:chart=\"urn:oasis:names:tc:opendocument:xmlns:chart:1.0\" ",
    "xmlns:loext=\"urn:org:documentfoundation:names:experimental:office:xmlns:loext:1.0\""
);

/// LibreOffice's default column width (2.258cm) and row height (0.452cm) in
/// points. Only used to find an anchor cell for a chart that sits on the sheet
/// (`table:shapes`) instead of inside a cell.
const ODS_DEFAULT_COLUMN_PT: f64 = 64.0;
const ODS_DEFAULT_ROW_PT: f64 = 12.8;

/// Upper bound on the rows read from a chart's local table.
const ODS_MAX_CHART_ROWS: usize = 100_000;

/// Limits that keep a small hostile package from exhausting memory: series per
/// chart, cached values per chart (series x rows) and charts per sheet.
const ODS_MAX_CHART_SERIES: usize = 255;
const ODS_MAX_CHART_VALUES: usize = 1_000_000;
const ODS_MAX_CHARTS_PER_SHEET: usize = 64;

/// The sheet area `read_ods` imports cells from; a chart anchor outside it is
/// clamped to it, so an import never asks the writer for millions of rows.
const ODS_MAX_READ_ROW: u32 = 100_000;
const ODS_MAX_READ_COLUMN: u32 = 1_000;

/// A chart written as the `Object N/` sub-document of an ODS package.
struct OdsChartObject {
    name: String,
    content: String,
}

/// Why a sheet chart cannot be written to ODS, if it cannot. The writer skips
/// such a chart; the compatibility report names it before the save.
pub(crate) fn ods_chart_problem(chart: &ChartData) -> Option<String> {
    if !matches!(chart.kind.as_str(), "column" | "bar" | "line" | "pie" | "area" | "scatter" | "doughnut") {
        return Some(format!("the chart type \"{}\" is not exportable", chart.kind));
    }
    if chart.series.is_empty() {
        return Some("the chart has no data series".into());
    }
    if !chart.categories.trim().is_empty() && crate::address::parse_range(&chart.categories).is_none() {
        return Some("the category range could not be read".into());
    }
    chart
        .series
        .iter()
        .find(|series| crate::address::parse_range(&series.range).is_none())
        .map(|series| format!("the range for series \"{}\" could not be read", series.name))
}

/// A sheet name as it appears in an ODF cell address; names that are not plain
/// words are quoted with `'` doubled inside.
fn ods_sheet_ref(name: &str) -> String {
    if !name.is_empty() && name.chars().all(|ch| ch.is_alphanumeric() || ch == '_') {
        name.to_string()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

/// `A2:A5` on `sheet` as an absolute ODF cell range address
/// (`Sheet1.$A$2:Sheet1.$A$5`); a single cell stays a single address.
fn ods_range_address(sheet: &str, range: &str) -> Option<String> {
    let range = range.trim();
    if range.is_empty() {
        return None;
    }
    let ((start_row, start_col), (end_row, end_col)) = crate::address::parse_range(range)?;
    let sheet = ods_sheet_ref(sheet);
    let start = format!("{sheet}.${}${}", crate::address::column_name(start_col), start_row + 1);
    if (start_row, start_col) == (end_row, end_col) {
        return Some(start);
    }
    Some(format!("{start}:{sheet}.${}${}", crate::address::column_name(end_col), end_row + 1))
}

/// The first range of an ODF cell range address as a sheet-local `A2:A5`
/// (sheet names and `$` dropped, like the XLSX importer does).
fn ods_range_to_ours(address: &str) -> String {
    let mut parts = vec![String::new()];
    let mut quoted = false;
    for ch in address.trim().chars() {
        match ch {
            '\'' => {
                quoted = !quoted;
                parts.last_mut().unwrap().push(ch);
            }
            ':' if !quoted => parts.push(String::new()),
            ch if ch.is_whitespace() && !quoted => break,
            ch => parts.last_mut().unwrap().push(ch),
        }
    }
    parts
        .iter()
        .map(|part| part.rsplit('.').next().unwrap_or(part).replace('$', ""))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(":")
}

/// A model colour as the `#rrggbb` ODF requires, or `None`.
fn ods_color(color: &str) -> Option<String> {
    let hex = color.trim().strip_prefix('#')?;
    (hex.len() == 6 && hex.chars().all(|ch| ch.is_ascii_hexdigit())).then(|| format!("#{hex}"))
}

fn ods_cell_label(value: &CellValue) -> String {
    match value {
        CellValue::Empty => String::new(),
        CellValue::Number(number) => format!("{number}"),
        CellValue::Bool(flag) => if *flag { "TRUE" } else { "FALSE" }.into(),
        CellValue::Text(text) | CellValue::Error(text) => text.clone(),
    }
}

fn ods_cell_number(value: &CellValue) -> Option<f64> {
    match value {
        CellValue::Number(number) => Some(*number),
        CellValue::Bool(flag) => Some(if *flag { 1.0 } else { 0.0 }),
        CellValue::Text(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
    .filter(|number| number.is_finite())
}

/// The `table:table` LibreOffice keeps inside every chart object: a header row
/// with the series names, then one row per category with the cached values.
/// The model caches are written when present; otherwise the values come from
/// the sheet cells so the object renders on its own.
fn ods_chart_local_table(chart: &ChartData, cells: &std::collections::BTreeMap<String, Cell>) -> String {
    let cell_values = |range: &str| -> Vec<CellValue> {
        crate::address::expand_range(range, ODS_MAX_CHART_ROWS)
            .iter()
            .map(|address| cells.get(address).map(|cell| cell.value.clone()).unwrap_or_default())
            .collect()
    };
    let categories: Vec<String> = if chart.categories_cache.is_empty() {
        cell_values(&chart.categories).iter().map(ods_cell_label).collect()
    } else {
        chart.categories_cache.clone()
    };
    let series: Vec<Vec<Option<f64>>> = chart
        .series
        .iter()
        .enumerate()
        .map(|(index, series)| match chart.series_values_cache.get(index).filter(|values| !values.is_empty()) {
            Some(values) => values.iter().map(|value| Some(*value).filter(|value| value.is_finite())).collect(),
            None => cell_values(&series.range).iter().map(ods_cell_number).collect(),
        })
        .collect();
    let rows = series.iter().map(Vec::len).chain(std::iter::once(categories.len())).max().unwrap_or(0);
    let mut out = format!(
        "<table:table table:name=\"local-table\"><table:table-header-columns><table:table-column/></table:table-header-columns><table:table-columns><table:table-column table:number-columns-repeated=\"{}\"/></table:table-columns><table:table-header-rows><table:table-row><table:table-cell><text:p/></table:table-cell>",
        chart.series.len().max(1)
    );
    for entry in &chart.series {
        out.push_str(&format!(
            "<table:table-cell office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>",
            crate::xml::escape_text(&entry.name)
        ));
    }
    out.push_str("</table:table-row></table:table-header-rows><table:table-rows>");
    for row in 0..rows {
        let label = categories.get(row).map(String::as_str).unwrap_or("");
        // The first column of a scatter chart holds X values, which are numbers.
        match label.trim().parse::<f64>().ok().filter(|number| chart.kind == "scatter" && number.is_finite()) {
            Some(number) => out.push_str(&format!(
                "<table:table-row><table:table-cell office:value-type=\"float\" office:value=\"{number}\"><text:p>{}</text:p></table:table-cell>",
                crate::xml::escape_text(label)
            )),
            None => out.push_str(&format!(
                "<table:table-row><table:table-cell office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>",
                crate::xml::escape_text(label)
            )),
        }
        for values in &series {
            match values.get(row).copied().flatten() {
                Some(value) => out.push_str(&format!(
                    "<table:table-cell office:value-type=\"float\" office:value=\"{value}\"><text:p>{value}</text:p></table:table-cell>"
                )),
                None => out.push_str("<table:table-cell><text:p/></table:table-cell>"),
            }
        }
        out.push_str("</table:table-row>");
    }
    out.push_str("</table:table-rows></table:table>");
    out
}

/// `Object N/content.xml` for one sheet chart, following LibreOffice's layout:
/// `chart:chart` with title, legend and plot area (axes, series pointing at the
/// sheet ranges) plus the local table with the cached values. A column chart
/// is `chart:bar`; a bar chart is the same class with `chart:vertical`.
fn ods_chart_content(
    placement: &ChartPlacement,
    sheet_name: &str,
    cells: &std::collections::BTreeMap<String, Cell>,
) -> String {
    let chart = &placement.chart;
    let kind = chart.kind.as_str();
    let pie = matches!(kind, "pie" | "doughnut");
    let scatter = kind == "scatter";
    // LibreOffice's donut is the `chart:ring` class. ODF has no hole size, so a
    // custom `hole_size` stays in the .oswk and XLSX files only.
    let class = match kind {
        "line" => "chart:line",
        "pie" => "chart:circle",
        "doughnut" => "chart:ring",
        "area" => "chart:area",
        "scatter" => "chart:scatter",
        _ => "chart:bar",
    };
    let (scatter_lines, scatter_markers, scatter_smooth) = crate::xlsx::scatter_flavour(chart.scatter_style.as_deref());
    let mut plot_properties = String::new();
    if matches!(kind, "column" | "bar") {
        plot_properties.push_str(&format!(" chart:vertical=\"{}\"", kind == "bar"));
    }
    if chart.stacked && !pie && !scatter {
        plot_properties.push_str(" chart:stacked=\"true\"");
    }
    if scatter && scatter_smooth {
        plot_properties.push_str(" chart:interpolation=\"cubic-spline\"");
    }
    let mut styles = format!(
        "<style:style style:name=\"chPlot\" style:family=\"chart\"><style:chart-properties{plot_properties}/></style:style>"
    );
    // A scatter chart reads its X values from the first series' domain.
    let domain = if scatter { ods_range_address(sheet_name, &chart.categories) } else { None };
    let mut series_xml = String::new();
    for (index, series) in chart.series.iter().enumerate() {
        let style_name = format!("chS{}", index + 1);
        let mut chart_properties = String::new();
        if chart.show_labels {
            chart_properties.push_str(" chart:data-label-number=\"value\"");
        }
        if scatter {
            let symbol = if scatter_markers { "automatic" } else { "none" };
            chart_properties.push_str(&format!(" chart:symbol-type=\"{symbol}\""));
        }
        let color = series.color.as_deref().and_then(ods_color);
        let stroke = if scatter {
            format!(" draw:stroke=\"{}\"", if scatter_lines { "solid" } else { "none" })
        } else {
            String::new()
        };
        let graphic = match color {
            Some(color) => format!(
                "<style:graphic-properties{stroke} draw:fill=\"solid\" draw:fill-color=\"{color}\" svg:stroke-color=\"{color}\"/>"
            ),
            None if scatter => format!("<style:graphic-properties{stroke}/>"),
            None => String::new(),
        };
        styles.push_str(&format!(
            "<style:style style:name=\"{style_name}\" style:family=\"chart\"><style:chart-properties{chart_properties}/>{graphic}</style:style>"
        ));
        // LibreOffice stores a literal series name as a quoted string literal.
        let domain_xml = domain
            .as_deref()
            .map(|range| format!("<chart:domain table:cell-range-address=\"{}\"/>", escape(range)))
            .unwrap_or_default();
        let series_head = format!(
            "<chart:series chart:style-name=\"{style_name}\" chart:class=\"{class}\" chart:values-cell-range-address=\"{}\" loext:label-string=\"{}\"",
            escape(&ods_range_address(sheet_name, &series.range).unwrap_or_default()),
            escape(&format!("\"{}\"", series.name.replace('"', "\"\"")))
        );
        if domain_xml.is_empty() {
            series_xml.push_str(&format!("{series_head}/>"));
        } else {
            series_xml.push_str(&format!("{series_head}>{domain_xml}</chart:series>"));
        }
    }
    let categories = ods_range_address(sheet_name, &chart.categories);
    let categories_xml = categories
        .as_deref()
        .filter(|_| !scatter)
        .map(|range| format!("<chart:categories table:cell-range-address=\"{}\"/>", escape(range)))
        .unwrap_or_default();
    // Like the XLSX export, a pie chart has no axes worth a title.
    let axis_title = |text: &str| {
        if text.is_empty() || pie {
            String::new()
        } else {
            format!("<chart:title><text:p>{}</text:p></chart:title>", crate::xml::escape_text(text))
        }
    };
    let grid = if pie { "" } else { "<chart:grid chart:class=\"major\"/>" };
    // LibreOffice hides the tick labels of an axis that has no style.
    let axis_style = if pie {
        ""
    } else {
        styles.push_str(
            "<style:style style:name=\"chAxis\" style:family=\"chart\"><style:chart-properties chart:display-label=\"true\"/></style:style>",
        );
        " chart:style-name=\"chAxis\""
    };
    let axes = format!(
        "<chart:axis chart:dimension=\"x\" chart:name=\"primary-x\"{axis_style}>{}{categories_xml}</chart:axis><chart:axis chart:dimension=\"y\" chart:name=\"primary-y\"{axis_style}>{}{grid}</chart:axis>",
        axis_title(&chart.x_title),
        axis_title(&chart.y_title)
    );
    let ranges: Vec<String> = categories
        .into_iter()
        .chain(chart.series.iter().filter_map(|series| ods_range_address(sheet_name, &series.range)))
        .collect();
    let title = if chart.title.is_empty() {
        String::new()
    } else {
        format!("<chart:title><text:p>{}</text:p></chart:title>", crate::xml::escape_text(&chart.title))
    };
    let legend = if chart.legend { "<chart:legend chart:legend-position=\"bottom\"/>" } else { "" };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NS} {CHART_NS} office:version=\"1.2\"><office:automatic-styles>{styles}</office:automatic-styles><office:body><office:chart><chart:chart svg:width=\"{}\" svg:height=\"{}\" chart:class=\"{class}\">{title}{legend}<chart:plot-area chart:style-name=\"chPlot\" table:cell-range-address=\"{}\">{axes}{series_xml}<chart:wall/><chart:floor/></chart:plot-area>{}</chart:chart></office:chart></office:body></office:document-content>",
        cm(placement.width_px.max(64.0) * 0.75),
        cm(placement.height_px.max(64.0) * 0.75),
        escape(&ranges.join(" ")),
        ods_chart_local_table(chart, cells)
    )
}

/// The chart frames of one sheet keyed by their anchor cell (zero-based row,
/// column). Each frame is anchored to its cell, so its position is the offset
/// inside that cell; the chart itself goes to `objects` as `Object N`.
fn ods_chart_frames(
    sheet: &Sheet,
    cells: &std::collections::BTreeMap<String, Cell>,
    objects: &mut Vec<OdsChartObject>,
) -> HashMap<(u32, u32), String> {
    let mut frames: HashMap<(u32, u32), String> = HashMap::new();
    for (index, placement) in sheet.charts.iter().enumerate() {
        if ods_chart_problem(&placement.chart).is_some() {
            continue;
        }
        let name = format!("Object {}", objects.len() + 1);
        let ranges: Vec<String> = std::iter::once(&placement.chart.categories)
            .chain(placement.chart.series.iter().map(|series| &series.range))
            .filter_map(|range| ods_range_address(&sheet.name, range))
            .collect();
        let anchor = crate::address::parse(&placement.anchor).unwrap_or((0, 0));
        frames.entry(anchor).or_default().push_str(&format!(
            "<draw:frame draw:z-index=\"{index}\" draw:name=\"{}\" svg:width=\"{}\" svg:height=\"{}\" svg:x=\"0cm\" svg:y=\"0cm\"><draw:object draw:notify-on-update-of-ranges=\"{}\" xlink:href=\"./{name}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/></draw:frame>",
            escape(if placement.id.is_empty() { &name } else { &placement.id }),
            cm(placement.width_px.max(64.0) * 0.75),
            cm(placement.height_px.max(64.0) * 0.75),
            escape(&ranges.join(" "))
        ));
        objects.push(OdsChartObject { content: ods_chart_content(placement, &sheet.name, cells), name });
    }
    frames
}

/// The sheet cells plus the computed output of the sheet's editor pivot
/// tables, which ODS (like the XLSX export) gets as plain values. A cell that
/// already holds a value or formula wins over the pivot output.
fn ods_cells_with_pivots<'a>(
    workbook: &Workbook,
    sheet: &'a Sheet,
) -> std::borrow::Cow<'a, std::collections::BTreeMap<String, Cell>> {
    let mut cells = std::borrow::Cow::Borrowed(&sheet.cells);
    for (address, value) in crate::pivot::materialize(workbook, sheet) {
        let taken =
            sheet.get(&address).map(|cell| cell.formula.is_some() || cell.value != CellValue::Empty).unwrap_or(false);
        if !taken {
            cells.to_mut().entry(address).or_default().value = value;
        }
    }
    cells
}

// ---------------------------------------------------------------------------
// ODS conditional formatting (LibreOffice's `calcext` extension)
// ---------------------------------------------------------------------------

const CALCEXT_NS: &str = "xmlns:calcext=\"urn:org:documentfoundation:names:experimental:calc:xmlns:calcext:1.0\"";
/// The namespace behind the `of:` in `of:=SUM(...)`; LibreOffice reads a formula
/// whose prefix is not declared as `Err:510`.
const OF_NS: &str = "xmlns:of=\"urn:oasis:names:tc:opendocument:xmlns:of:1.2\"";

/// Rules of a sheet that cannot be written to ODS, as the reason each was left
/// out. Mirrors `ods_chart_problem` so the compatibility report can name them.
pub(crate) fn ods_conditional_problem(rule: &CondRule) -> Option<String> {
    let known = |threshold: &CondThreshold| crate::xlsx::CFVO_KINDS.contains(&threshold.kind.as_str());
    match rule.kind.as_str() {
        "colorScale" if !(2..=3).contains(&rule.thresholds.len()) || !rule.thresholds.iter().all(known) => {
            Some("a color scale needs two or three valid colour stops".into())
        }
        "iconSet" if rule.icon_set.as_deref().map(crate::xlsx::icon_set_count).is_some_and(|count| count.is_none()) => {
            Some("the icon set is unknown".into())
        }
        "expression" if rule.formula.as_deref().map(str::trim).unwrap_or("").is_empty() => {
            Some("the formula rule has no formula".into())
        }
        "greater" | "less" | "equal" | "between" | "text" | "textContains" | "duplicate" | "duplicates" | "top"
        | "bottom" | "expression" | "colorScale" | "dataBar" | "iconSet" => {
            rule.range.trim().is_empty().then(|| "the rule has no range".to_string())
        }
        other => Some(format!("the rule type \"{other}\" is not exportable")),
    }
}

/// `A1:B5 D2` on `sheet` as the space separated cell range list ODF wants
/// (`Sheet1.A1:Sheet1.B5 Sheet1.D2`); parts that are not ranges are dropped.
fn ods_target_ranges(sheet: &str, range: &str) -> String {
    let sheet = ods_sheet_ref(sheet);
    range
        .split(|ch: char| ch.is_whitespace() || ch == ',')
        .filter_map(|part| {
            let ((start_row, start_col), (end_row, end_col)) = crate::address::parse_range(part)?;
            let start = format!("{sheet}.{}", crate::address::format(start_row, start_col));
            Some(if (start_row, start_col) == (end_row, end_col) {
                start
            } else {
                format!("{start}:{sheet}.{}", crate::address::format(end_row, end_col))
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether `chars[index..]` starts a cell reference the way a rule formula
/// writes it (`A1`, `$B$2`, `A1:C5`, `Sheet2!A1`, `'My Sheet'!A1:B2`), and if so
/// the number of characters it spans and its ODF spelling (`[.A1]`,
/// `[.A1:.C5]`, `[$Sheet2.A1]`).
fn ods_reference_at(chars: &[char], index: usize) -> Option<(usize, String)> {
    let cell_at = |start: usize| -> Option<(usize, String)> {
        let mut cursor = start;
        let mut text = String::new();
        if chars.get(cursor) == Some(&'$') {
            text.push('$');
            cursor += 1;
        }
        let letters = chars[cursor..].iter().take_while(|ch| ch.is_ascii_alphabetic()).count();
        if !(1..=3).contains(&letters) {
            return None;
        }
        text.extend(chars[cursor..cursor + letters].iter().map(char::to_ascii_uppercase));
        cursor += letters;
        if chars.get(cursor) == Some(&'$') {
            text.push('$');
            cursor += 1;
        }
        let digits = chars[cursor..].iter().take_while(|ch| ch.is_ascii_digit()).count();
        if digits == 0 {
            return None;
        }
        text.extend(&chars[cursor..cursor + digits]);
        Some((cursor + digits - start, text))
    };
    // An optional sheet prefix: `Name!` or `'Quoted name'!`.
    let mut cursor = index;
    let mut sheet = None;
    if chars.get(cursor) == Some(&'\'') {
        let mut end = cursor + 1;
        let mut name = String::new();
        while end < chars.len() {
            match (chars[end], chars.get(end + 1)) {
                ('\'', Some('\'')) => {
                    name.push('\'');
                    end += 2;
                }
                ('\'', _) => break,
                (ch, _) => {
                    name.push(ch);
                    end += 1;
                }
            }
        }
        if chars.get(end) != Some(&'\'') || chars.get(end + 1) != Some(&'!') {
            return None;
        }
        sheet = Some(ods_sheet_ref(&name));
        cursor = end + 2;
    } else {
        let word = chars[cursor..].iter().take_while(|ch| ch.is_alphanumeric() || **ch == '_').count();
        if word > 0 && chars.get(cursor + word) == Some(&'!') {
            sheet = Some(chars[cursor..cursor + word].iter().collect());
            cursor += word + 1;
        }
    }
    let (first_len, first) = cell_at(cursor)?;
    cursor += first_len;
    let mut reference = first;
    if chars.get(cursor) == Some(&':') {
        if let Some((second_len, second)) = cell_at(cursor + 1) {
            reference = format!("{reference}:.{second}");
            cursor += 1 + second_len;
        }
    }
    // `LOG10(` and `Rate1` are not references.
    if chars.get(cursor).is_some_and(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '(' | '.')) {
        return None;
    }
    let text = match sheet {
        Some(sheet) => format!("[${sheet}.{reference}]"),
        None => format!("[.{reference}]"),
    };
    Some((cursor - index, text))
}

/// A rule formula in the editor's spelling (`AND($A2>20,B2<4)`) as ODF writes
/// it in conditions: bracketed references and `;` between arguments.
fn formula_to_ods(formula: &str) -> String {
    let chars: Vec<char> = formula.chars().collect();
    let mut out = String::new();
    let (mut index, mut quoted) = (0usize, false);
    while index < chars.len() {
        let ch = chars[index];
        if ch == '"' {
            quoted = !quoted;
        }
        if quoted || ch == '"' {
            out.push(ch);
            index += 1;
            continue;
        }
        if ch == ',' {
            out.push(';');
            index += 1;
            continue;
        }
        let after_word = index > 0 && (chars[index - 1].is_alphanumeric() || matches!(chars[index - 1], '_' | '.'));
        if !after_word {
            if let Some((length, text)) = ods_reference_at(&chars, index) {
                out.push_str(&text);
                index += length;
                continue;
            }
        }
        out.push(ch);
        index += 1;
    }
    out
}

/// The inverse of [`formula_to_ods`]: `[.$D1]` -> `$D1`, `[$Data.$B1]` ->
/// `Data!$B1`, `;` -> `,`.
fn formula_from_ods(formula: &str) -> String {
    let chars: Vec<char> = formula.chars().collect();
    let mut out = String::new();
    let (mut index, mut quoted) = (0usize, false);
    while index < chars.len() {
        let ch = chars[index];
        if ch == '"' {
            quoted = !quoted;
        }
        if quoted || ch == '"' {
            out.push(ch);
            index += 1;
            continue;
        }
        match ch {
            ';' => out.push(','),
            '[' => {
                let end = chars[index..].iter().position(|ch| *ch == ']').map_or(chars.len(), |offset| index + offset);
                let inner: String = chars[index + 1..end.min(chars.len())].iter().collect();
                out.push_str(&ods_reference_to_ours(&inner));
                index = end + 1;
                continue;
            }
            _ => out.push(ch),
        }
        index += 1;
    }
    out
}

/// `.A1:.B2`, `$Data.$B1` or `$'My Sheet'.A1:.B2` to the editor's spelling.
fn ods_reference_to_ours(inner: &str) -> String {
    // Split at ':' and '.' outside quoted sheet names.
    let mut parts: Vec<(String, String)> = vec![(String::new(), String::new())];
    let (mut quoted, mut after_dot) = (false, false);
    for ch in inner.chars() {
        match ch {
            '\'' => {
                quoted = !quoted;
                let part = parts.last_mut().unwrap();
                if after_dot {
                    part.1.push(ch)
                } else {
                    part.0.push(ch)
                }
            }
            ':' if !quoted => {
                parts.push((String::new(), String::new()));
                after_dot = false;
            }
            '.' if !quoted && !after_dot => after_dot = true,
            ch => {
                let part = parts.last_mut().unwrap();
                if after_dot {
                    part.1.push(ch)
                } else {
                    part.0.push(ch)
                }
            }
        }
    }
    let sheet = parts[0].0.trim_start_matches('$').to_string();
    let cells: Vec<&str> = parts.iter().map(|(_, cell)| cell.as_str()).collect();
    let range = cells.join(":");
    if sheet.is_empty() {
        range
    } else {
        format!("{sheet}!{range}")
    }
}

/// A rule operand as a condition literal: numbers stay, text is quoted.
fn ods_condition_literal(value: &str) -> String {
    let value = value.trim();
    if value.parse::<f64>().is_ok() || value.starts_with('"') {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('"', "\"\""))
    }
}

/// The `calcext:value` of a highlight rule, in the grammar LibreOffice writes:
/// an operator and operand (`>5`), `between(1,9)`, `contains-text("x")`,
/// `top-elements(3)` or `formula-is(...)`.
fn ods_condition_value(rule: &CondRule) -> Option<String> {
    let first = rule.values.first().map(String::as_str).unwrap_or("0");
    let second = rule.values.get(1).map(String::as_str).unwrap_or("0");
    Some(match rule.kind.as_str() {
        "greater" => format!(">{}", ods_condition_literal(first)),
        "less" => format!("<{}", ods_condition_literal(first)),
        "equal" => format!("={}", ods_condition_literal(first)),
        "between" => format!("between({},{})", ods_condition_literal(first), ods_condition_literal(second)),
        "text" | "textContains" => format!("contains-text(\"{}\")", first.replace('"', "\"\"")),
        "duplicate" | "duplicates" => "duplicate".to_string(),
        "top" => format!("top-elements({})", rule.top_n.unwrap_or(10)),
        "bottom" => format!("bottom-elements({})", rule.top_n.unwrap_or(10)),
        "expression" => {
            let formula = rule.formula.as_deref()?.trim().trim_start_matches('=').trim();
            format!("formula-is({})", formula_to_ods(formula))
        }
        _ => return None,
    })
}

/// A threshold type in the words of `calcext:type`.
fn ods_threshold_type(kind: &str) -> &'static str {
    match kind {
        "min" => "minimum",
        "max" => "maximum",
        "percent" => "percent",
        "percentile" => "percentile",
        "formula" => "formula",
        _ => "number",
    }
}

/// One threshold entry. An icon set's first entry also says whether the cell
/// value stays visible, where LibreOffice reads it.
fn ods_entry(tag: &str, threshold: &CondThreshold, hide_value: bool) -> String {
    let value = if matches!(threshold.kind.as_str(), "min" | "max") { "0" } else { threshold.value.trim() };
    let color = threshold
        .color
        .as_deref()
        .and_then(ods_color)
        .map(|color| format!(" calcext:color=\"{color}\""))
        .unwrap_or_default();
    let show = if hide_value { " calcext:show-value=\"false\"" } else { "" };
    format!(
        "<calcext:{tag}{show} calcext:value=\"{}\" calcext:type=\"{}\"{color}/>",
        escape(if value.is_empty() { "0" } else { value }),
        ods_threshold_type(&threshold.kind)
    )
}

/// The `calcext:conditional-formats` block of one sheet. Highlight looks are
/// collected into `look_styles` (named table-cell styles for `styles.xml`).
fn ods_conditional_formats(sheet: &Sheet, look_styles: &mut Vec<(String, String)>) -> String {
    let mut out = String::new();
    for rule in &sheet.conditional {
        if ods_conditional_problem(rule).is_some() {
            continue;
        }
        let target = ods_target_ranges(&sheet.name, &rule.range);
        if target.is_empty() {
            continue;
        }
        let base =
            format!("{}.{}", ods_sheet_ref(&sheet.name), crate::xlsx::anchor_of(&rule.range).trim_start_matches('$'));
        let body = match rule.kind.as_str() {
            "colorScale" => {
                let entries: String =
                    rule.thresholds.iter().map(|stop| ods_entry("color-scale-entry", stop, false)).collect();
                format!("<calcext:color-scale>{entries}</calcext:color-scale>")
            }
            "dataBar" => {
                let color = rule.fill.as_deref().and_then(ods_color).unwrap_or_else(|| "#638EC6".into());
                let (low, high) = match rule.thresholds.as_slice() {
                    [low, high] => {
                        (ods_entry("formatting-entry", low, false), ods_entry("formatting-entry", high, false))
                    }
                    _ => (
                        "<calcext:formatting-entry calcext:value=\"0\" calcext:type=\"auto-minimum\"/>".to_string(),
                        "<calcext:formatting-entry calcext:value=\"0\" calcext:type=\"auto-maximum\"/>".to_string(),
                    ),
                };
                format!(
                    "<calcext:data-bar calcext:positive-color=\"{color}\" calcext:gradient=\"true\" calcext:axis-position=\"automatic\" calcext:show-value=\"{}\" calcext:axis-color=\"#000000\" calcext:negative-color=\"#ff0000\" calcext:min-length=\"0\" calcext:max-length=\"100\">{low}{high}</calcext:data-bar>",
                    !rule.hide_value
                )
            }
            "iconSet" => {
                let name = rule
                    .icon_set
                    .as_deref()
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .unwrap_or("3TrafficLights1");
                let count = crate::xlsx::icon_set_count(name).unwrap_or(3);
                let thresholds = if rule.thresholds.len() == count {
                    rule.thresholds.clone()
                } else {
                    crate::xlsx::default_icon_thresholds(count)
                };
                let entries: String = thresholds
                    .iter()
                    .enumerate()
                    .map(|(position, entry)| ods_entry("formatting-entry", entry, rule.hide_value && position == 0))
                    .collect();
                // LibreOffice has no reversed icon sets; the flag is ours, kept
                // so the editor's own files round-trip.
                let reverse = if rule.reverse_icons { " calcext:reverse=\"true\"" } else { "" };
                format!(
                    "<calcext:icon-set calcext:icon-set-type=\"{}\"{reverse}>{entries}</calcext:icon-set>",
                    escape(name)
                )
            }
            _ => {
                let Some(value) = ods_condition_value(rule) else { continue };
                // The look is a named cell style; a rule with none gets the
                // shared highlight, as in the XLSX export.
                let fill = rule.fill.as_deref().and_then(ods_color);
                let color = rule.color.as_deref().and_then(ods_color);
                let fill = if fill.is_none() && color.is_none() && !rule.bold && !rule.italic {
                    ods_color(crate::xlsx::CONDITIONAL_FILL)
                } else {
                    fill
                };
                let mut text = String::new();
                if let Some(color) = &color {
                    text.push_str(&format!(" fo:color=\"{color}\""));
                }
                if rule.bold {
                    text.push_str(" fo:font-weight=\"bold\"");
                }
                if rule.italic {
                    text.push_str(" fo:font-style=\"italic\"");
                }
                let cell = fill.map(|fill| format!(" fo:background-color=\"{fill}\"")).unwrap_or_default();
                let style_xml = format!("<style:text-properties{text}/><style:table-cell-properties{cell}/>");
                let position = match look_styles.iter().position(|(_, existing)| existing == &style_xml) {
                    Some(position) => position,
                    None => {
                        look_styles.push((format!("OmniCF{}", look_styles.len() + 1), style_xml));
                        look_styles.len() - 1
                    }
                };
                format!(
                    "<calcext:condition calcext:apply-style-name=\"{}\" calcext:value=\"{}\" calcext:base-cell-address=\"{}\"/>",
                    look_styles[position].0,
                    escape(&value),
                    escape(&base)
                )
            }
        };
        out.push_str(&format!(
            "<calcext:conditional-format calcext:target-range-address=\"{}\">{body}</calcext:conditional-format>",
            escape(&target)
        ));
    }
    if out.is_empty() {
        out
    } else {
        format!("<calcext:conditional-formats>{out}</calcext:conditional-formats>")
    }
}

/// The indexes whose stored size is 0: the rows or columns the editor hides.
fn ods_hidden(sizes: &BTreeMap<u32, f64>) -> BTreeSet<u32> {
    sizes.iter().filter(|(_, size)| **size <= 0.0).map(|(index, _)| *index).collect()
}

pub fn write_ods(workbook: &Workbook) -> OfficeResult<Vec<u8>> {
    let mut styles = AutoStyles::default();
    let mut body = String::new();
    let mut chart_objects: Vec<OdsChartObject> = Vec::new();
    // Highlight looks of conditional-formatting rules, shared by every sheet.
    let mut look_styles: Vec<(String, String)> = Vec::new();
    for sheet in &workbook.sheets {
        body.push_str(&format!("<table:table table:name=\"{}\">", escape(&sheet.name)));
        let cells = ods_cells_with_pivots(workbook, sheet);
        let frames = ods_chart_frames(sheet, &cells, &mut chart_objects);
        let mut max_col = 0u32;
        for address in cells.keys() {
            if let Some((_, column)) = crate::address::parse(address) {
                max_col = max_col.max(column);
            }
        }
        for (_, column) in frames.keys() {
            max_col = max_col.max(*column);
        }
        // The editor hides a column or a row by storing size 0; ODF says so with
        // `table:visibility="collapse"`. A hidden column past the last cell is
        // still written. (`filter` is for rows an AutoFilter hid and needs the
        // filter's database range, which this writer does not export.)
        let hidden_columns = ods_hidden(&sheet.col_widths);
        let hidden_rows = ods_hidden(&sheet.row_heights);
        let last_column = hidden_columns.last().map_or(max_col, |last| max_col.max(*last)).min(200);
        for column in 0..=last_column {
            let width = sheet.col_widths.get(&column).copied().unwrap_or(90.0);
            let collapse = if hidden_columns.contains(&column) { " table:visibility=\"collapse\"" } else { "" };
            body.push_str(&format!(
                "<table:table-column table:style-name=\"co{}\"{collapse} style:column-width=\"{}\"/>",
                column,
                cm(width * 0.75)
            ));
        }
        // LibreOffice writes the sheet's conditional formats between the
        // columns and the rows.
        body.push_str(&ods_conditional_formats(sheet, &mut look_styles));
        // Only rows holding a cell or a chart are written one by one; the gaps
        // between them become repeated empty rows, so a far-away anchor costs
        // one element instead of a row per index.
        let mut occupied: BTreeSet<u32> =
            cells.keys().filter_map(|address| crate::address::parse(address).map(|(row, _)| row)).collect();
        occupied.extend(frames.keys().map(|(row, _)| *row));
        occupied.extend(hidden_rows.iter().copied());
        occupied.insert(0);
        let mut next_row = 0u32;
        for row in occupied {
            if row > next_row {
                body.push_str(&format!(
                    "<table:table-row table:number-rows-repeated=\"{}\"><table:table-cell table:number-columns-repeated=\"{}\"/></table:table-row>",
                    row - next_row,
                    max_col + 1
                ));
            }
            next_row = row + 1;
            let mut row_output = String::new();
            let mut column = 0u32;
            let mut empty_run = 0u32;
            while column <= max_col {
                let address = crate::address::format(row, column);
                let cell = cells.get(&address).filter(|cell| !cell.is_empty());
                let frame = frames.get(&(row, column)).map(String::as_str);
                if cell.is_none() && frame.is_none() {
                    empty_run += 1;
                    column += 1;
                    continue;
                }
                if empty_run > 0 {
                    row_output
                        .push_str(&format!("<table:table-cell table:number-columns-repeated=\"{}\"/>", empty_run));
                    empty_run = 0;
                }
                // Cell-anchored charts sit inside their anchor cell, before its text.
                let frame = frame.unwrap_or("");
                let Some(cell) = cell else {
                    row_output.push_str(&format!("<table:table-cell>{frame}</table:table-cell>"));
                    column += 1;
                    continue;
                };
                let style_name = styles.cell(cell_style_xml(&cell.style));
                let mut attributes = format!(" table:style-name=\"{style_name}\"");
                if let Some(merge) = sheet.merges.iter().find(|merge| merge.start == address) {
                    if let (Some((start_row, start_col)), Some((_, end_col))) =
                        (crate::address::parse(&merge.start), crate::address::parse(&merge.end))
                    {
                        let _ = start_row;
                        attributes.push_str(&format!(" table:number-columns-spanned=\"{}\"", end_col - start_col + 1));
                    }
                }
                let label = match &cell.value {
                    CellValue::Empty => String::new(),
                    CellValue::Number(number) => format!("{number}"),
                    CellValue::Bool(value) => if *value { "TRUE" } else { "FALSE" }.to_string(),
                    CellValue::Text(text) | CellValue::Error(text) => text.clone(),
                };
                // A hyperlink is a `text:a` around the cell text. A cell with no
                // text shows the link's display text (or its target) instead, since
                // ODF has no link without a label. Targets outside the allowed
                // schemes are not written.
                let link_target = cell.link.as_deref().and_then(safe_link_target);
                let paragraph = match &link_target {
                    Some(target) => {
                        let shown = if label.is_empty() {
                            cell.link_display.clone().filter(|text| !text.is_empty()).unwrap_or_else(|| target.clone())
                        } else {
                            label
                        };
                        let tip = cell
                            .link_tooltip
                            .as_deref()
                            .filter(|text| !text.is_empty())
                            .map(|text| format!(" office:title=\"{}\"", escape(text)))
                            .unwrap_or_default();
                        format!(
                            "<text:p><text:a xlink:type=\"simple\" xlink:href=\"{}\"{tip}>{}</text:a></text:p>",
                            escape(&ods_href(target)),
                            crate::xml::escape_text(&shown)
                        )
                    }
                    None if label.is_empty() => "<text:p/>".to_string(),
                    None => format!("<text:p>{}</text:p>", crate::xml::escape_text(&label)),
                };
                let annotation = cell
                    .comment
                    .as_deref()
                    .filter(|text| !text.trim().is_empty())
                    .map(|text| ods_annotation(text, cell.comment_author.as_deref(), cell.comment_visible))
                    .unwrap_or_default();
                let value_xml = format!("{annotation}{paragraph}");
                let value_type = match &cell.value {
                    CellValue::Number(_) => "float",
                    CellValue::Bool(_) => "boolean",
                    _ => "string",
                };
                // LibreOffice marks an error cell as a string with its own flag, which is
                // what tells an error from text that merely looks like one.
                let error_flag =
                    if matches!(cell.value, CellValue::Error(_)) { " calcext:value-type=\"error\"" } else { "" };
                let formula = cell
                    .formula
                    .as_deref()
                    .map(|formula| format!(" table:formula=\"{}\"", escape(&our_formula_to_odf(formula))))
                    .unwrap_or_default();
                let value_attr = match &cell.value {
                    CellValue::Number(number) => format!(" office:value=\"{number}\""),
                    CellValue::Bool(value) => {
                        format!(" office:boolean-value=\"{}\"", if *value { "true" } else { "false" })
                    }
                    // LibreOffice takes a cell's text from `office:string-value` when
                    // it is there and then never sees the `text:a` link, so a linked
                    // cell carries its text in the paragraph only.
                    CellValue::Text(text) if link_target.is_none() => {
                        format!(" office:string-value=\"{}\"", escape(text))
                    }
                    _ => String::new(),
                };
                row_output.push_str(&format!("<table:table-cell office:value-type=\"{value_type}\"{error_flag}{value_attr}{attributes}{formula}>{frame}{value_xml}</table:table-cell>"));
                column += 1;
            }
            if empty_run > 0 {
                row_output.push_str(&format!("<table:table-cell table:number-columns-repeated=\"{}\"/>", empty_run));
            }
            let collapse = if hidden_rows.contains(&row) { " table:visibility=\"collapse\"" } else { "" };
            body.push_str(&format!("<table:table-row{collapse}>{row_output}</table:table-row>"));
        }
        body.push_str("</table:table>");
    }
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NS} {CALCEXT_NS} {OF_NS} office:version=\"1.2\">{}{}<office:body><office:spreadsheet>{body}</office:spreadsheet></office:body></office:document-content>",
        styles.xml(),
        "<office:styles/>"
    );
    // The looks a conditional format applies are named cell styles.
    let look_styles_xml: String = look_styles
        .iter()
        .map(|(name, xml)| {
            format!("<style:style style:name=\"{name}\" style:family=\"table-cell\">{xml}</style:style>")
        })
        .collect();
    // Every chart object is a sub-document with its own manifest entries.
    let mut object_entries = String::new();
    for object in &chart_objects {
        let name = escape(&object.name);
        object_entries.push_str(&format!(
            "<manifest:file-entry manifest:full-path=\"{name}/\" manifest:version=\"1.2\" manifest:media-type=\"application/vnd.oasis.opendocument.chart\"/><manifest:file-entry manifest:full-path=\"{name}/content.xml\" manifest:media-type=\"text/xml\"/><manifest:file-entry manifest:full-path=\"{name}/styles.xml\" manifest:media-type=\"text/xml\"/>"
        ));
    }
    let manifest = manifest_for("application/vnd.oasis.opendocument.spreadsheet")
        .replace("</manifest:manifest>", &format!("{object_entries}</manifest:manifest>"));
    let mut zip = ZipWriter::new();
    zip.add_text("mimetype", "application/vnd.oasis.opendocument.spreadsheet");
    zip.add_text("META-INF/manifest.xml", &manifest);
    zip.add_text("content.xml", &content);
    zip.add_text(
        "styles.xml",
        &if look_styles_xml.is_empty() {
            format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NS} office:version=\"1.2\"/>")
        } else {
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NS} office:version=\"1.2\"><office:styles>{look_styles_xml}</office:styles></office:document-styles>"
            )
        },
    );
    zip.add_text("meta.xml", &meta_xml(&workbook.title, "OmniOffice", None));
    for object in &chart_objects {
        zip.add_text(&format!("{}/content.xml", object.name), &object.content);
        zip.add_text(
            &format!("{}/styles.xml", object.name),
            &format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NS} {CHART_NS} office:version=\"1.2\"><office:styles/></office:document-styles>"
            ),
        );
    }
    Ok(zip.finish())
}

pub fn write_ods_file(path: &Path, workbook: &Workbook) -> OfficeResult<()> {
    crate::io::write_atomic(path, &write_ods(workbook)?)
}

/// A chart's local table: the series names from the header row and, per body
/// row, the category label followed by one value per series.
struct OdsLocalTable {
    header: Vec<String>,
    rows: Vec<Vec<(String, Option<f64>)>>,
}

fn read_ods_table_row(row: &XmlNode) -> Vec<(String, Option<f64>)> {
    let mut cells = Vec::new();
    for cell in row.children.iter().filter(|cell| matches!(cell.local_name(), "table-cell" | "covered-table-cell")) {
        let repeat = cell
            .attr_any_ns("number-columns-repeated")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(1)
            .clamp(1, 256);
        let mut paragraphs = Vec::new();
        cell.find_all("p", &mut paragraphs);
        let text = paragraphs.iter().map(|node| node.deep_text()).collect::<Vec<_>>().join("\n");
        let number = cell
            .attr_any_ns("value")
            .and_then(|value| value.trim().parse::<f64>().ok())
            .or_else(|| text.trim().parse::<f64>().ok())
            .filter(|value| value.is_finite());
        for _ in 0..repeat {
            cells.push((text.clone(), number));
        }
        if cells.len() > 1_024 {
            break;
        }
    }
    cells
}

fn read_ods_local_table(table: &XmlNode) -> OdsLocalTable {
    let mut header = Vec::new();
    let mut rows = Vec::new();
    for child in &table.children {
        match child.local_name() {
            "table-header-rows" => {
                if let Some(row) = child.child("table-row") {
                    header = read_ods_table_row(row).into_iter().map(|(text, _)| text).collect();
                }
            }
            "table-rows" => rows.extend(child.children_named("table-row").map(read_ods_table_row)),
            "table-row" => rows.push(read_ods_table_row(child)),
            _ => {}
        }
    }
    rows.truncate(ODS_MAX_CHART_ROWS);
    OdsLocalTable { header, rows }
}

/// The `style:<properties>` child of the automatic style `name`.
fn ods_style_child<'a>(
    styles: &HashMap<&str, &'a XmlNode>,
    name: Option<&str>,
    properties: &str,
) -> Option<&'a XmlNode> {
    styles.get(name?).copied()?.child(properties)
}

fn odf_paragraph_text(node: &XmlNode) -> String {
    let mut paragraphs = Vec::new();
    node.find_all("p", &mut paragraphs);
    paragraphs.iter().map(|paragraph| paragraph.deep_text()).collect::<Vec<_>>().join("\n")
}

/// Reads the chart object a sheet `draw:frame` embeds (`Object N/content.xml`)
/// back into a placement. Frames holding anything else are skipped; caches
/// come from the object's local table, the inverse of the export.
fn read_ods_chart(
    reader: &ZipReader,
    frame: &XmlNode,
    anchor: String,
    sheet: &Sheet,
    seen_objects: &mut HashSet<String>,
    warnings: &mut Vec<String>,
) -> Option<ChartPlacement> {
    let object = frame.child("object")?;
    let href = object.attr_any_ns("href")?.trim().trim_start_matches("./").trim_end_matches('/').to_string();
    if href.is_empty() {
        return None;
    }
    // Each object is read once: frames repeating one href would otherwise each
    // hold a full copy of its caches.
    if !seen_objects.insert(href.clone()) {
        warnings.push(format!("The embedded object \"{href}\" is shown more than once; only the first copy was kept."));
        return None;
    }
    let Some(root) = reader.read_text(&format!("{href}/content.xml")).ok().and_then(|text| parse_xml(&text).ok())
    else {
        warnings.push(format!("The embedded object \"{href}\" could not be read and was skipped."));
        return None;
    };
    let mut nodes = Vec::new();
    root.find_all("chart", &mut nodes);
    let Some(chart) = nodes.into_iter().find(|node| node.attr_any_ns("class").is_some()) else {
        warnings.push(format!("The embedded object \"{href}\" is not a chart and was skipped."));
        return None;
    };
    let mut style_nodes = Vec::new();
    root.find_all("style", &mut style_nodes);
    let styles: HashMap<&str, &XmlNode> =
        style_nodes.into_iter().filter_map(|node| node.attr("style:name").map(|name| (name, node))).collect();
    let plot = chart.child("plot-area");
    let plot_properties =
        ods_style_child(&styles, plot.and_then(|plot| plot.attr_any_ns("style-name")), "chart-properties");
    let plot_flag = |name: &str| plot_properties.and_then(|node| node.attr_any_ns(name)) == Some("true");
    let class = chart.attr_any_ns("class").unwrap_or("").rsplit(':').next().unwrap_or("");
    let kind = match class {
        "bar" if plot_flag("vertical") => "bar",
        "bar" => "column",
        "line" => "line",
        "circle" => "pie",
        "ring" => "doughnut",
        "area" => "area",
        "scatter" => "scatter",
        other => {
            let mapped = match other {
                "filled-radar" => "area",
                "bubble" | "stock" | "radar" => "line",
                _ => "column",
            };
            warnings.push(format!("A \"{other}\" chart was imported as a {mapped} chart."));
            mapped
        }
    };
    let mut x_title = String::new();
    let mut y_title = String::new();
    let mut categories = String::new();
    for axis in plot.map(|plot| plot.children_of("axis")).unwrap_or_default() {
        let title = axis.child("title").map(odf_paragraph_text).unwrap_or_default();
        match axis.attr_any_ns("dimension") {
            Some("x") => {
                x_title = title;
                if let Some(range) = axis.child("categories").and_then(|node| node.attr_any_ns("cell-range-address")) {
                    categories = ods_range_to_ours(range);
                }
            }
            Some("y") => y_title = title,
            _ => {}
        }
    }
    let local = chart.child("table").map(read_ods_local_table);
    let labels_on = |node: Option<&XmlNode>| {
        node.and_then(|node| node.attr_any_ns("data-label-number")).map(|value| value != "none").unwrap_or(false)
    };
    let mut show_labels = labels_on(plot_properties);
    let mut series = Vec::new();
    let mut series_values_cache = Vec::new();
    let mut series_nodes = plot.map(|plot| plot.children_of("series")).unwrap_or_default();
    if series_nodes.len() > ODS_MAX_CHART_SERIES {
        warnings
            .push(format!("A chart with {} series was cut to its first {ODS_MAX_CHART_SERIES}.", series_nodes.len()));
        series_nodes.truncate(ODS_MAX_CHART_SERIES);
    }
    // Cached values are a convenience (the ranges stay): drop them rather than
    // allocate series x rows values past the limit.
    let cache_rows = local.as_ref().map_or(0, |table| table.rows.len());
    let keep_caches = cache_rows.saturating_mul(series_nodes.len()) <= ODS_MAX_CHART_VALUES;
    if !keep_caches {
        warnings.push("A chart's cached values were too large and were left out; its cell ranges are kept.".into());
    }
    let mut lines_hidden = true;
    let mut markers_hidden = true;
    for (index, node) in series_nodes.into_iter().enumerate() {
        let style_name = node.attr_any_ns("style-name");
        show_labels |= labels_on(ods_style_child(&styles, style_name, "chart-properties"));
        let graphic = ods_style_child(&styles, style_name, "graphic-properties");
        if kind == "scatter" {
            // A scatter series draws a line unless its stroke is `none` and
            // shows markers unless its symbol type is `none`.
            lines_hidden &= graphic.and_then(|graphic| graphic.attr_any_ns("stroke")) == Some("none");
            markers_hidden &= ods_style_child(&styles, style_name, "chart-properties")
                .and_then(|properties| properties.attr_any_ns("symbol-type"))
                == Some("none");
            // The X values are the series' domain; the first one is the chart's.
            if categories.is_empty() {
                if let Some(range) = node.child("domain").and_then(|domain| domain.attr_any_ns("cell-range-address")) {
                    categories = ods_range_to_ours(range);
                }
            }
        }
        let color = graphic
            .and_then(|graphic| {
                let stroke = graphic.attr_any_ns("stroke-color");
                let fill = graphic.attr_any_ns("fill-color");
                if kind == "line" {
                    stroke.or(fill)
                } else {
                    fill.or(stroke)
                }
            })
            .and_then(ods_color)
            .map(|color| color.to_ascii_uppercase());
        let name = node
            .attr_any_ns("label-string")
            .map(|literal| {
                let literal = literal.trim();
                match literal.strip_prefix('"').and_then(|inner| inner.strip_suffix('"')) {
                    Some(inner) => inner.replace("\"\"", "\""),
                    None => literal.to_string(),
                }
            })
            .or_else(|| {
                local.as_ref().and_then(|table| table.header.get(index + 1)).filter(|name| !name.is_empty()).cloned()
            })
            .or_else(|| {
                let address = ods_range_to_ours(node.attr_any_ns("label-cell-address")?);
                sheet.get(address.split(':').next()?).map(|cell| ods_cell_label(&cell.value))
            })
            .unwrap_or_else(|| format!("Series {}", index + 1));
        let range = node.attr_any_ns("values-cell-range-address").map(ods_range_to_ours).unwrap_or_default();
        let values: Vec<f64> = local
            .as_ref()
            .filter(|_| keep_caches)
            .filter(|table| table.rows.iter().any(|row| row.len() > index + 1))
            .map(|table| {
                table.rows.iter().map(|row| row.get(index + 1).and_then(|cell| cell.1).unwrap_or(0.0)).collect()
            })
            .unwrap_or_default();
        series_values_cache.push(values);
        series.push(ChartSeries { name, range, color });
    }
    // Empty caches are dropped, as the XLSX importer does, so a chart that only
    // carries ranges stays range-only.
    if series_values_cache.iter().all(Vec::is_empty) {
        series_values_cache.clear();
    }
    let mut categories_cache: Vec<String> = local
        .as_ref()
        .filter(|_| keep_caches)
        .map(|table| table.rows.iter().map(|row| row.first().map(|cell| cell.0.clone()).unwrap_or_default()).collect())
        .unwrap_or_default();
    if categories_cache.iter().all(String::is_empty) {
        categories_cache.clear();
    }
    let size = |attribute: &str, fallback: f64| {
        frame
            .attr_any_ns(attribute)
            .or_else(|| chart.attr_any_ns(attribute))
            .and_then(parse_cm)
            .map(|pt| pt / 0.75)
            .unwrap_or(fallback)
    };
    Some(ChartPlacement {
        id: frame
            .attr("draw:name")
            .filter(|name| !name.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        chart: ChartData {
            kind: kind.to_string(),
            title: chart.child("title").map(odf_paragraph_text).unwrap_or_default(),
            categories,
            series,
            legend: chart.child("legend").is_some(),
            x_title,
            y_title,
            stacked: plot_flag("stacked") || plot_flag("percentage"),
            show_labels,
            categories_cache,
            series_values_cache,
            hole_size: None,
            scatter_style: (kind == "scatter" && !lines_hidden).then(|| {
                let smooth = plot_properties.and_then(|node| node.attr_any_ns("interpolation")) == Some("cubic-spline");
                match (markers_hidden, smooth) {
                    (false, false) => "lineMarker",
                    (true, false) => "line",
                    (false, true) => "smoothMarker",
                    (true, true) => "smooth",
                }
                .to_string()
            }),
        },
        anchor,
        width_px: size("width", 480.0),
        height_px: size("height", 288.0),
    })
}

/// Anchor cell for a chart placed on the sheet (`table:shapes`) rather than in
/// a cell, estimated from its position with LibreOffice's default cell size.
fn ods_shape_anchor(frame: &XmlNode) -> String {
    let x = frame.attr_any_ns("x").and_then(parse_cm).unwrap_or(0.0).max(0.0);
    let y = frame.attr_any_ns("y").and_then(parse_cm).unwrap_or(0.0).max(0.0);
    crate::address::format(
        ((y / ODS_DEFAULT_ROW_PT).floor() as u32).min(ODS_MAX_READ_ROW),
        ((x / ODS_DEFAULT_COLUMN_PT).floor() as u32).min(ODS_MAX_READ_COLUMN),
    )
}

/// True when a cell carries a value, a formula or text. The empty padding
/// LibreOffice writes past the data is not content.
fn ods_cell_has_content(cell: &XmlNode) -> bool {
    if ["formula", "value", "boolean-value", "date-value"].iter().any(|name| cell.attr_any_ns(name).is_some()) {
        return true;
    }
    let mut paragraphs = Vec::new();
    cell.find_all("p", &mut paragraphs);
    paragraphs.iter().any(|node| !node.deep_text().is_empty())
}

/// A cell note as `office:annotation`, the first child of its cell.
fn ods_annotation(text: &str, author: Option<&str>, visible: bool) -> String {
    let paragraphs: String =
        text.lines().map(|line| format!("<text:p>{}</text:p>", crate::xml::escape_text(line))).collect();
    let creator = author
        .filter(|author| !author.trim().is_empty())
        .map(|author| format!("<dc:creator>{}</dc:creator>", crate::xml::escape_text(author.trim())))
        .unwrap_or_default();
    format!("<office:annotation office:display=\"{visible}\">{creator}{paragraphs}</office:annotation>")
}

/// An internal link in ODS spells the sheet separator `.` (`#Sheet2.A1`,
/// `#'My Sheet'.A1`); the model, like XLSX, uses `!`.
fn swap_sheet_separator(reference: &str, from: char, to: char) -> String {
    let mut quoted = false;
    let mut last = None;
    for (index, ch) in reference.char_indices() {
        match ch {
            '\'' => quoted = !quoted,
            ch if ch == from && !quoted => last = Some(index),
            _ => {}
        }
    }
    match last {
        Some(index) => format!("{}{to}{}", &reference[..index], &reference[index + 1..]),
        None => reference.to_string(),
    }
}

/// The `xlink:href` for a model link target.
fn ods_href(target: &str) -> String {
    match target.strip_prefix('#') {
        Some(reference) => format!("#{}", swap_sheet_separator(reference, '!', '.')),
        None => target.to_string(),
    }
}

/// A cell's `office:annotation` as (text, author, shown permanently).
fn read_ods_annotation(annotation: &XmlNode) -> Option<(String, Option<String>, bool)> {
    let text = annotation.children_of("p").iter().map(|node| node.deep_text()).collect::<Vec<_>>().join("\n");
    if text.trim().is_empty() {
        return None;
    }
    let author = annotation
        .child("creator")
        .map(|creator| creator.deep_text().trim().to_string())
        .filter(|author| !author.is_empty());
    Some((text, author, annotation.attr_any_ns("display") == Some("true")))
}

/// The hyperlink of a cell: (target, screen tip) of the first `text:a` in its
/// paragraphs. A target that is not http, https, mailto or internal is counted in
/// `dropped` and ignored.
fn read_ods_cell_link(cell: &XmlNode, dropped: &mut usize) -> Option<(String, Option<String>)> {
    let mut anchors = Vec::new();
    for paragraph in cell.children_of("p") {
        paragraph.find_all("a", &mut anchors);
    }
    let anchor = anchors.into_iter().find(|anchor| anchor.attr_any_ns("href").is_some())?;
    let href = anchor.attr_any_ns("href")?.trim();
    let target = match href.strip_prefix('#') {
        Some(reference) => format!("#{}", swap_sheet_separator(reference, '.', '!')),
        None => href.to_string(),
    };
    let Some(target) = safe_link_target(&target) else {
        *dropped += 1;
        return None;
    };
    let tooltip = anchor.attr_any_ns("title").map(str::to_string).filter(|title| !title.is_empty());
    Some((target, tooltip))
}

/// Conditional-format entries kept per sheet; a hostile file cannot make the
/// importer build an unbounded rule list.
const ODS_MAX_CONDITIONAL_RULES: usize = 5_000;

/// What a table-cell style paints: fill, font colour, bold, italic.
type OdsLook = (Option<String>, Option<String>, bool, bool);

/// The looks of every table-cell style in `roots` (content.xml's automatic
/// styles and styles.xml), by style name.
fn ods_cell_looks(roots: &[&XmlNode]) -> HashMap<String, OdsLook> {
    let mut looks = HashMap::new();
    for root in roots {
        let mut nodes = Vec::new();
        root.find_all("style", &mut nodes);
        for node in nodes {
            if node.attr("style:family") != Some("table-cell") {
                continue;
            }
            let Some(name) = node.attr("style:name") else { continue };
            let fill = node
                .child("table-cell-properties")
                .and_then(|properties| properties.attr_any_ns("background-color"))
                .and_then(ods_color)
                .map(|color| color.to_ascii_uppercase());
            let text = node.child("text-properties");
            let color = text
                .and_then(|text| text.attr_any_ns("color"))
                .and_then(ods_color)
                .map(|color| color.to_ascii_uppercase());
            let bold = text.and_then(|text| text.attr_any_ns("font-weight")) == Some("bold");
            let italic = text.and_then(|text| text.attr_any_ns("font-style")) == Some("italic");
            // LibreOffice escapes odd characters in `style:name` (`ConditionalStyle_5f_1`)
            // and refers to the style by its display name.
            if let Some(display) = node.attr("style:display-name") {
                looks.insert(display.to_string(), (fill.clone(), color.clone(), bold, italic));
            }
            looks.insert(name.to_string(), (fill, color, bold, italic));
        }
    }
    looks
}

/// Splits an ODF cell range list on whitespace outside quoted sheet names and
/// maps each range to the sheet-local form (`Sheet1.A1:Sheet1.B5` -> `A1:B5`).
fn ods_ranges_to_ours(addresses: &str) -> String {
    let mut parts = vec![String::new()];
    let mut quoted = false;
    for ch in addresses.trim().chars() {
        match ch {
            '\'' => {
                quoted = !quoted;
                parts.last_mut().unwrap().push(ch);
            }
            ch if ch.is_whitespace() && !quoted => parts.push(String::new()),
            ch => parts.last_mut().unwrap().push(ch),
        }
    }
    parts
        .iter()
        .filter(|part| !part.is_empty())
        .map(|part| {
            let range = ods_range_to_ours(part);
            // LibreOffice spells a single cell as `Sheet1.A1:Sheet1.A1`.
            match range.split_once(':') {
                Some((start, end)) if start == end => start.to_string(),
                _ => range,
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Splits condition arguments on `,` or `;` outside string literals and nested
/// parentheses.
fn split_ods_args(inner: &str) -> Vec<String> {
    let mut args = vec![String::new()];
    let (mut quoted, mut depth) = (false, 0i32);
    for ch in inner.chars() {
        match ch {
            '"' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => depth -= 1,
            ',' | ';' if !quoted && depth == 0 => {
                args.push(String::new());
                continue;
            }
            _ => {}
        }
        args.last_mut().unwrap().push(ch);
    }
    args.into_iter().map(|arg| arg.trim().to_string()).collect()
}

/// `"a ""b"""` -> `a "b"`; text without quotes is returned as written.
fn unquote_ods(text: &str) -> String {
    let text = text.trim();
    match text.strip_prefix('"').and_then(|inner| inner.strip_suffix('"')) {
        Some(inner) => inner.replace("\"\"", "\""),
        None => text.to_string(),
    }
}

/// A `calcext:value` condition as a model rule (kind, operands and formula
/// only). Conditions that are only formulas in the editor's vocabulary
/// (`>=`, begins-with, unique, above-average, ...) become `expression` rules.
fn ods_condition_rule(value: &str, anchor: &str, all: &str) -> Option<CondRule> {
    let value = value.trim();
    let mut rule = CondRule::default();
    let call = |name: &str| value.strip_prefix(name).and_then(|rest| rest.strip_suffix(')'));
    let mut expression = |formula: String| {
        rule.kind = "expression".into();
        rule.formula = Some(formula);
    };
    // LibreOffice writes `>5`, `between(1,9)` and `formula-is(...)`; the ODF
    // style-map spellings (`cell-content()>5`, `cell-content-is-between(1,9)`,
    // `is-true-formula(...)`) come from other producers and are read as well.
    let operand_form = value.strip_prefix("cell-content()").unwrap_or(value);
    if let Some(inner) = call("formula-is(").or_else(|| call("is-true-formula(")) {
        expression(formula_from_ods(inner.trim()));
    } else if let Some(inner) = call("between(").or_else(|| call("cell-content-is-between(")) {
        let args = split_ods_args(inner);
        let [low, high] = args.as_slice() else { return None };
        rule.kind = "between".into();
        rule.values = vec![low.clone(), high.clone()];
    } else if let Some(inner) = call("not-between(").or_else(|| call("cell-content-is-not-between(")) {
        let args = split_ods_args(inner);
        let [low, high] = args.as_slice() else { return None };
        expression(format!("OR({anchor}<{low},{anchor}>{high})"));
    } else if let Some(operator) =
        [">=", "<=", "!=", "<>", ">", "<", "="].into_iter().find(|operator| operand_form.starts_with(operator))
    {
        let operand = operand_form[operator.len()..].trim().to_string();
        match operator {
            ">" | "<" | "=" => {
                rule.kind = match operator {
                    ">" => "greater",
                    "<" => "less",
                    _ => "equal",
                }
                .into();
                rule.values = vec![operand];
            }
            "!=" | "<>" => expression(format!("{anchor}<>{operand}")),
            _ => expression(format!("{anchor}{operator}{operand}")),
        }
    } else if let Some(inner) = call("contains-text(") {
        rule.kind = "textContains".into();
        rule.values = vec![unquote_ods(inner)];
    } else if let Some(inner) = call("not-contains-text(") {
        let text = unquote_ods(inner).replace('"', "\"\"");
        expression(format!("ISERROR(SEARCH(\"{text}\",{anchor}))"));
    } else if let Some(inner) = call("begins-with(") {
        let text = unquote_ods(inner).replace('"', "\"\"");
        expression(format!("LEFT({anchor},LEN(\"{text}\"))=\"{text}\""));
    } else if let Some(inner) = call("ends-with(") {
        let text = unquote_ods(inner).replace('"', "\"\"");
        expression(format!("RIGHT({anchor},LEN(\"{text}\"))=\"{text}\""));
    } else if matches!(value, "duplicate" | "is-duplicate") {
        rule.kind = "duplicate".into();
    } else if matches!(value, "unique" | "is-unique") {
        expression(format!("COUNTIF({all},{anchor})=1"));
    } else if let Some(inner) = call("top-elements(") {
        rule.kind = "top".into();
        rule.top_n = Some(inner.trim().parse().ok()?);
    } else if let Some(inner) = call("bottom-elements(") {
        rule.kind = "bottom".into();
        rule.top_n = Some(inner.trim().parse().ok()?);
    } else if let Some(inner) = call("top-percent(") {
        let rank: u32 = inner.trim().parse().ok()?;
        expression(format!("{anchor}>=PERCENTILE({all},1-{rank}/100)"));
    } else if let Some(inner) = call("bottom-percent(") {
        let rank: u32 = inner.trim().parse().ok()?;
        expression(format!("{anchor}<=PERCENTILE({all},{rank}/100)"));
    } else if let Some(operator) = match value {
        "above-average" => Some(">"),
        "above-equal-average" => Some(">="),
        "below-average" => Some("<"),
        "below-equal-average" => Some("<="),
        _ => None,
    } {
        expression(format!("{anchor}{operator}AVERAGE({all})"));
    } else if value == "is-error" {
        expression(format!("ISERROR({anchor})"));
    } else if value == "is-no-error" {
        expression(format!("NOT(ISERROR({anchor}))"));
    } else {
        return None;
    }
    Some(rule)
}

/// One `calcext:formatting-entry` / `color-scale-entry` as a threshold.
fn ods_threshold(node: &XmlNode) -> Option<CondThreshold> {
    let kind = match node.attr_any_ns("type")? {
        "minimum" | "auto-minimum" => "min",
        "maximum" | "auto-maximum" => "max",
        "value" | "number" => "num",
        "percent" => "percent",
        "percentile" => "percentile",
        "formula" => "formula",
        _ => return None,
    };
    let value = if matches!(kind, "min" | "max") {
        String::new()
    } else {
        node.attr_any_ns("value").unwrap_or("").trim().to_string()
    };
    let color = node.attr_any_ns("color").and_then(ods_color).map(|color| color.to_ascii_uppercase());
    Some(CondThreshold { kind: kind.to_string(), value, color })
}

/// Reads the `calcext:conditional-formats` of one sheet; returns how many
/// entries the model could not hold.
fn read_ods_conditional_formats(formats: &XmlNode, sheet: &mut Sheet, looks: &HashMap<String, OdsLook>) -> usize {
    let mut unsupported = 0usize;
    for format in formats.children_of("conditional-format") {
        let range = ods_ranges_to_ours(format.attr_any_ns("target-range-address").unwrap_or(""));
        if range.is_empty() {
            continue;
        }
        let all = crate::xlsx::absolute_area(&range);
        for entry in &format.children {
            if sheet.conditional.len() >= ODS_MAX_CONDITIONAL_RULES {
                return unsupported + 1;
            }
            let model = match entry.local_name() {
                "condition" => {
                    let base = entry.attr_any_ns("base-cell-address").map(ods_range_to_ours).unwrap_or_default();
                    let anchor = base.split(':').next().filter(|cell| !cell.is_empty()).map(str::to_string);
                    let anchor = anchor.unwrap_or_else(|| crate::xlsx::anchor_of(&range));
                    let rule = ods_condition_rule(entry.attr_any_ns("value").unwrap_or(""), &anchor, &all);
                    rule.map(|mut rule| {
                        if let Some((fill, color, bold, italic)) =
                            entry.attr_any_ns("apply-style-name").and_then(|name| looks.get(name))
                        {
                            rule.fill = fill.clone();
                            rule.color = color.clone();
                            rule.bold = *bold;
                            rule.italic = *italic;
                        }
                        rule
                    })
                }
                "color-scale" => {
                    let stops: Option<Vec<CondThreshold>> =
                        entry.children_of("color-scale-entry").into_iter().map(ods_threshold).collect();
                    stops.filter(|stops| (2..=3).contains(&stops.len())).map(|thresholds| CondRule {
                        kind: "colorScale".into(),
                        thresholds,
                        ..Default::default()
                    })
                }
                "data-bar" => {
                    let bounds: Option<Vec<CondThreshold>> =
                        entry.children_of("formatting-entry").into_iter().map(ods_threshold).collect();
                    bounds.map(|bounds| {
                        // The automatic bounds are the default; only explicit ones are kept.
                        let thresholds = match bounds.as_slice() {
                            [low, high] if !(low.kind == "min" && high.kind == "max") => bounds.clone(),
                            _ => Vec::new(),
                        };
                        CondRule {
                            kind: "dataBar".into(),
                            fill: entry
                                .attr_any_ns("positive-color")
                                .and_then(ods_color)
                                .map(|color| color.to_ascii_uppercase()),
                            hide_value: entry.attr_any_ns("show-value") == Some("false"),
                            thresholds,
                            ..Default::default()
                        }
                    })
                }
                "icon-set" => {
                    let name = entry.attr_any_ns("icon-set-type").unwrap_or("3TrafficLights1").trim().to_string();
                    let thresholds: Option<Vec<CondThreshold>> =
                        entry.children_of("formatting-entry").into_iter().map(ods_threshold).collect();
                    crate::xlsx::icon_set_count(&name).zip(thresholds).filter(|(count, list)| list.len() == *count).map(
                        |(_, thresholds)| CondRule {
                            kind: "iconSet".into(),
                            icon_set: Some(name),
                            reverse_icons: entry.attr_any_ns("reverse") == Some("true"),
                            // LibreOffice says "hide the value" on the first entry.
                            hide_value: entry.attr_any_ns("show-value") == Some("false")
                                || entry
                                    .children_of("formatting-entry")
                                    .iter()
                                    .any(|threshold| threshold.attr_any_ns("show-value") == Some("false")),
                            thresholds,
                            ..Default::default()
                        },
                    )
                }
                _ => None,
            };
            match model {
                Some(mut rule) => {
                    rule.id = format!("cf{}", sheet.conditional.len() + 1);
                    rule.range = range.clone();
                    sheet.conditional.push(rule);
                }
                None => unsupported += 1,
            }
        }
    }
    unsupported
}

/// Whether an ODF row or column element is hidden: `collapse` is a hand-hidden
/// one, `filter` one an AutoFilter hid; both show as size 0 in the editor.
fn ods_collapsed(node: &XmlNode) -> bool {
    matches!(node.attr_any_ns("visibility"), Some("collapse" | "filter"))
}

/// `table:display="false"` on a row or column group: the group is folded.
fn ods_group_folded(group: &XmlNode) -> bool {
    group.attr_any_ns("display") == Some("false")
}

/// The rows of a table in document order with whether each is hidden. Rows may
/// sit in `table-header-rows`, `table-rows` and nested `table-row-group`s; a
/// folded group hides every row it holds.
fn collect_ods_rows<'a>(node: &'a XmlNode, hidden: bool, out: &mut Vec<(&'a XmlNode, bool)>) {
    for child in &node.children {
        match child.local_name() {
            "table-row" => out.push((child, hidden || ods_collapsed(child))),
            "table-rows" | "table-header-rows" => collect_ods_rows(child, hidden, out),
            "table-row-group" => collect_ods_rows(child, hidden || ods_group_folded(child), out),
            _ => {}
        }
    }
}

/// Runs of hidden columns as (first column, count), from the column elements of
/// a table, which may sit in `table-columns`, `table-header-columns` and groups.
fn ods_hidden_column_runs(table: &XmlNode) -> Vec<(u32, u32)> {
    fn walk(node: &XmlNode, hidden: bool, column: &mut u32, runs: &mut Vec<(u32, u32)>) {
        for child in &node.children {
            match child.local_name() {
                "table-column" => {
                    let repeat = child
                        .attr_any_ns("number-columns-repeated")
                        .and_then(|value| value.parse::<u32>().ok())
                        .unwrap_or(1)
                        .max(1);
                    if hidden || ods_collapsed(child) {
                        runs.push((*column, repeat));
                    }
                    *column = column.saturating_add(repeat);
                }
                "table-columns" | "table-header-columns" => walk(child, hidden, column, runs),
                "table-column-group" => walk(child, hidden || ods_group_folded(child), column, runs),
                _ => {}
            }
        }
    }
    let mut runs = Vec::new();
    walk(table, false, &mut 0, &mut runs);
    runs
}

pub fn read_ods(bytes: &[u8]) -> OfficeResult<SheetRead> {
    let reader = ZipReader::open(bytes.to_vec())?;
    if !reader.contains("content.xml") {
        return Err(OfficeError::corrupt("The package does not contain content.xml."));
    }
    let text = reader.read_text("content.xml")?;
    let root = parse_xml(&text)?;
    let mut workbook = Workbook::new_blank("Imported spreadsheet");
    workbook.sheets.clear();
    let mut warnings = Vec::new();
    let mut tables = Vec::new();
    root.find_all("table", &mut tables);
    let mut seen_objects = HashSet::new();
    // The named and automatic cell styles conditional formats point at.
    let styles_root = reader.read_text("styles.xml").ok().and_then(|text| parse_xml(&text).ok());
    let looks = ods_cell_looks(&[&root, styles_root.as_ref().unwrap_or(&root)]);
    let mut unsupported_formats = 0usize;
    let mut dropped_links = 0usize;
    for table in tables {
        // `table:name`: a plain `attr("name")` never matched the prefixed
        // attribute, so every imported sheet used to be called "Sheet".
        let name = table.attr_any_ns("name").unwrap_or("Sheet").to_string();
        let mut sheet = Sheet::new(&name);
        // Frames anchored to a cell, with that cell; charts are read once the
        // cells are in so a series label cell can be resolved.
        let mut frames: Vec<(String, &XmlNode)> = Vec::new();
        let mut row = 0u32;
        let mut last_row = 0u32;
        let mut cut_off = false;
        // Runs of hidden rows as (first row, count), applied once the grid is known.
        let mut hidden_row_runs: Vec<(u32, u32)> = Vec::new();
        let mut table_rows = Vec::new();
        collect_ods_rows(table, false, &mut table_rows);
        for (row_node, row_hidden) in table_rows {
            // The repeat only moves the position (a repeated row's cells are
            // set once), so it is taken as written: capping it shifted every
            // cell after a long empty gap up to the wrong row.
            let repeat_rows =
                row_node.attr_any_ns("number-rows-repeated").and_then(|value| value.parse::<u32>().ok()).unwrap_or(1);
            if row_hidden {
                hidden_row_runs.push((row, repeat_rows.max(1)));
            }
            let mut column = 0u32;
            for cell in row_node.children_named("table-cell") {
                let repeat = cell
                    .attr_any_ns("number-columns-repeated")
                    .and_then(|value| value.parse::<u32>().ok())
                    .unwrap_or(1)
                    .min(1024);
                // Past the imported area only charts are still picked up, so a
                // chart anchored far down or right is not lost on reopening.
                if row > ODS_MAX_READ_ROW || column > ODS_MAX_READ_COLUMN {
                    cut_off |= ods_cell_has_content(cell);
                    for frame in cell.children_named("frame") {
                        let anchor = crate::address::format(row.min(ODS_MAX_READ_ROW), column.min(ODS_MAX_READ_COLUMN));
                        frames.push((anchor, frame));
                    }
                    column = column.saturating_add(repeat);
                    continue;
                }
                let value_type = cell.attr_any_ns("value-type").unwrap_or("string").to_string();
                let formula = cell.attr_any_ns("formula").map(odf_formula_to_ours);
                let value = match value_type.as_str() {
                    "float" | "currency" | "percentage" => cell
                        .attr_any_ns("value")
                        .and_then(|value| value.parse::<f64>().ok())
                        .map(CellValue::Number)
                        .unwrap_or(CellValue::Empty),
                    "boolean" => cell
                        .attr_any_ns("boolean-value")
                        .map(|value| CellValue::Bool(value == "true"))
                        .unwrap_or(CellValue::Empty),
                    "date" => cell
                        .attr_any_ns("date-value")
                        .map(|value| CellValue::Text(value.to_string()))
                        .unwrap_or(CellValue::Empty),
                    _ => {
                        // Only the cell's own paragraphs: a note's text lives in
                        // `office:annotation`, which is not part of the value.
                        let text =
                            cell.children_of("p").iter().map(|node| node.deep_text()).collect::<Vec<_>>().join("\n");
                        if text.is_empty() {
                            CellValue::Empty
                        } else if cell.attr("calcext:value-type") == Some("error")
                            || (formula.is_some() && text.starts_with('#'))
                        {
                            CellValue::Error(text)
                        } else {
                            CellValue::Text(text)
                        }
                    }
                };
                let note = cell.child("annotation").and_then(read_ods_annotation);
                let link = read_ods_cell_link(cell, &mut dropped_links);
                if !matches!(value, CellValue::Empty) || formula.is_some() || note.is_some() || link.is_some() {
                    last_row = last_row.max(row);
                    for offset in 0..repeat.min(64) {
                        let address = crate::address::format(row, column + offset);
                        let mut model = Cell { value: value.clone(), formula: formula.clone(), ..Default::default() };
                        if let Some((target, tooltip)) = &link {
                            model.link = Some(target.clone());
                            model.link_tooltip = tooltip.clone();
                        }
                        // Only the first cell of a repeated run keeps the note.
                        if let (Some((text, author, visible)), 0) = (&note, offset) {
                            model.comment = Some(text.clone());
                            model.comment_author = author.clone();
                            model.comment_visible = *visible;
                        }
                        sheet.set(&address, model);
                    }
                }
                for frame in cell.children_named("frame") {
                    last_row = last_row.max(row);
                    frames.push((crate::address::format(row, column), frame));
                }
                column = column.saturating_add(repeat);
            }
            row = row.saturating_add(repeat_rows.max(1));
        }
        for shapes in table.children_named("shapes") {
            frames.extend(shapes.children_named("frame").map(|frame| (ods_shape_anchor(frame), frame)));
        }
        for formats in table.children_named("conditional-formats") {
            unsupported_formats += read_ods_conditional_formats(formats, &mut sheet, &looks);
        }
        for (anchor, frame) in frames {
            if sheet.charts.len() >= ODS_MAX_CHARTS_PER_SHEET {
                warnings.push(format!(
                    "Sheet \"{name}\" has more than {ODS_MAX_CHARTS_PER_SHEET} charts; the rest were skipped."
                ));
                break;
            }
            if let Some(placement) = read_ods_chart(&reader, frame, anchor, &sheet, &mut seen_objects, &mut warnings) {
                sheet.charts.push(placement);
            }
        }
        sheet.row_count = (last_row.min(ODS_MAX_READ_ROW) + 51).max(200);
        sheet.col_count = 26;
        // A hidden row or column is size 0 in the sparse tables. Only the
        // sheet's own grid is recorded: LibreOffice hides the unused rest of a
        // sheet with one run a million rows long.
        for (first, count) in hidden_row_runs {
            for hidden in first..first.saturating_add(count).min(sheet.row_count) {
                sheet.row_heights.insert(hidden, 0.0);
            }
        }
        let content_columns = sheet
            .cells
            .keys()
            .filter_map(|address| crate::address::parse(address).map(|(_, column)| column + 1))
            .max()
            .unwrap_or(0);
        let grid_columns = sheet.col_count.max(content_columns).min(ODS_MAX_READ_COLUMN + 1);
        for (first, count) in ods_hidden_column_runs(table) {
            for hidden in first..first.saturating_add(count).min(grid_columns) {
                sheet.col_widths.insert(hidden, 0.0);
            }
        }
        if cut_off {
            warnings.push(crate::xlsx::import_limit_warning(&name, ODS_MAX_READ_ROW, ODS_MAX_READ_COLUMN));
        }
        workbook.sheets.push(sheet);
    }
    if workbook.sheets.is_empty() {
        workbook.sheets.push(Sheet::new("Sheet1"));
    }
    if unsupported_formats > 0 {
        warnings.push(format!(
            "{unsupported_formats} conditional formatting rule(s) use types this editor cannot import and were dropped."
        ));
    }
    if dropped_links > 0 {
        warnings.push(format!(
            "{dropped_links} hyperlink(s) were dropped: only http, https, mailto and internal references are allowed."
        ));
    }
    warnings.push("Cell formatting from ODS files is imported with limited support.".into());
    Ok(SheetRead { workbook, warnings })
}

pub fn read_ods_file(path: &Path) -> OfficeResult<SheetRead> {
    let bytes = crate::io::read_bytes(path)?;
    let mut result = read_ods(&bytes)?;
    result.workbook.title = crate::io::file_stem(path);
    Ok(result)
}

// ---------------------------------------------------------------------------
// ODP
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DeckRead {
    pub deck: Deck,
    pub warnings: Vec<String>,
}

/// Namespaces the slide timing (ODF 1.2 SMIL animations) needs on top of [`NS`].
const ANIM_NS: &str = concat!(
    "xmlns:anim=\"urn:oasis:names:tc:opendocument:xmlns:animation:1.0\" ",
    "xmlns:smil=\"urn:oasis:names:tc:opendocument:xmlns:smil-compatible:1.0\""
);

/// True when `value` is usable as an `xml:id` (an NCName).
fn is_ncname(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_alphanumeric() || matches!(ch, '.' | '-' | '_'))
}

/// `xml:id`/`draw:id` allocation for ODP shapes, which animations target.
/// Ids must be unique in the whole content.xml: a model id that is an NCName
/// and still free is kept, so a re-import restores it; anything else (a UUID
/// starting with a digit, an id repeated on another slide) gets `idN`.
#[derive(Default)]
struct OdpShapeIds {
    used: std::collections::HashSet<String>,
    /// Model object id -> written id, for the slide being written.
    slide: HashMap<String, String>,
    /// Model ids whose planned id has already been handed out on this slide.
    written: std::collections::HashSet<String>,
    next: usize,
}

impl OdpShapeIds {
    /// A fresh id that is unique in the whole content.xml, preferring an NCName
    /// model id that is still free.
    fn unique(&mut self, object: &SlideObject) -> String {
        let id = if is_ncname(&object.id) && !self.used.contains(&object.id) {
            object.id.clone()
        } else {
            loop {
                self.next += 1;
                let candidate = format!("id{}", self.next);
                if !self.used.contains(&candidate) {
                    break candidate;
                }
            }
        };
        self.used.insert(id.clone());
        id
    }

    /// Plans the id of one shape without consuming it, so a connector written
    /// before its target already knows the target's id. The first object with a
    /// given model id owns the mapping.
    fn plan(&mut self, object: &SlideObject) -> String {
        let id = self.unique(object);
        if !object.id.is_empty() {
            self.slide.entry(object.id.clone()).or_insert_with(|| id.clone());
        }
        id
    }

    /// The id to write for `object`: its planned id when there is one, otherwise
    /// a fresh one. A repeated model id falls back to a fresh id so every shape
    /// stays addressable.
    fn assign(&mut self, object: &SlideObject) -> String {
        if !object.id.is_empty() {
            if let Some(planned) = self.slide.get(&object.id).cloned() {
                if self.written.insert(object.id.clone()) {
                    return planned;
                }
            }
        }
        let id = self.unique(object);
        if !object.id.is_empty() {
            self.slide.entry(object.id.clone()).or_insert_with(|| id.clone());
        }
        id
    }

    /// Starts a new slide; the planned and written maps are per page.
    fn reset_slide(&mut self) {
        self.slide.clear();
        self.written.clear();
    }
}

/// The editor's animation effects and the LibreOffice presets they are written
/// as (LibreOffice's `simpress/effects.xml`): kind, effect, preset id and
/// preset sub-type. Grow and shrink share a preset and differ by scale.
const ODP_PRESETS: [(&str, &str, &str, &str); 11] = [
    ("entrance", "appear", "ooo-entrance-appear", ""),
    ("entrance", "fade", "ooo-entrance-fade-in", ""),
    ("entrance", "flyIn", "ooo-entrance-fly-in", "from-bottom"),
    ("entrance", "zoom", "ooo-entrance-zoom", "in"),
    ("emphasis", "pulse", "ooo-emphasis-flash-bulb", ""),
    ("emphasis", "spin", "ooo-emphasis-spin", ""),
    ("emphasis", "grow", "ooo-emphasis-grow-and-shrink", ""),
    ("emphasis", "shrink", "ooo-emphasis-grow-and-shrink", ""),
    ("exit", "disappear", "ooo-exit-disappear", ""),
    ("exit", "fadeOut", "ooo-exit-fade-out", ""),
    ("exit", "flyOut", "ooo-exit-fly-out", "to-top"),
];

/// The editor effect closest to a foreign effect or preset name (a PPTX filter
/// such as `wipe(down)`, or a LibreOffice preset without an editor twin).
fn closest_odp_effect(kind: &str, name: &str) -> &'static str {
    let name = name.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|word| name.contains(word));
    let flies = has(&["fly", "peek", "rise", "ascend", "descend", "float", "glide", "sling", "credits", "sink"]);
    match kind {
        "entrance" if flies => "flyIn",
        "entrance" if has(&["zoom", "magnify", "expand", "grow", "stretch", "compress", "unfold"]) => "zoom",
        "entrance" if name.is_empty() || has(&["appear", "flash"]) => "appear",
        "entrance" => "fade",
        "exit" if flies => "flyOut",
        "exit" if has(&["disappear", "flash"]) => "disappear",
        "exit" => "fadeOut",
        _ if has(&["shrink", "compress"]) => "shrink",
        _ if has(&["grow", "magnify", "zoom", "scale", "size", "expand"]) => "grow",
        _ if has(&["spin", "rotate", "teeter", "swivel", "wave"]) => "spin",
        _ => "pulse",
    }
}

/// The editor effect an animation is written as, and whether that is exact.
fn odp_export_effect(kind: &str, effect: &str) -> (&'static str, bool) {
    let exact = match (kind, effect) {
        ("entrance", "fadeIn") => Some("fade"),
        // PPTX imports name the exit fade `fade`.
        ("exit", "fade") => Some("fadeOut"),
        _ => ODP_PRESETS.iter().find(|preset| preset.0 == kind && preset.1 == effect).map(|preset| preset.1),
    };
    match exact {
        Some(effect) => (effect, true),
        None => (closest_odp_effect(kind, effect), false),
    }
}

/// A SMIL clock value in seconds (`0.5s`).
fn smil_time(ms: u64) -> String {
    format!("{}s", ms as f64 / 1000.0)
}

/// Seconds from a SMIL clock value (`0.5s`, `500ms`, `2`, `00:00:01.5`);
/// `None` for event values such as `next` or `indefinite`.
fn parse_smil_seconds(value: &str) -> Option<f64> {
    let value = value.trim();
    let seconds = if let Some(ms) = value.strip_suffix("ms") {
        ms.trim().parse::<f64>().ok()? / 1000.0
    } else if let Some(minutes) = value.strip_suffix("min") {
        minutes.trim().parse::<f64>().ok()? * 60.0
    } else if let Some(hours) = value.strip_suffix('h') {
        hours.trim().parse::<f64>().ok()? * 3600.0
    } else if let Some(seconds) = value.strip_suffix('s') {
        seconds.trim().parse::<f64>().ok()?
    } else if value.contains(':') {
        let mut total = 0.0;
        for part in value.split(':') {
            total = total * 60.0 + part.trim().parse::<f64>().ok()?;
        }
        total
    } else {
        value.parse::<f64>().ok()?
    };
    seconds.is_finite().then_some(seconds.max(0.0))
}

fn seconds_to_ms(seconds: f64) -> u32 {
    (seconds * 1000.0).round().clamp(0.0, u32::MAX as f64) as u32
}

/// The animation nodes of one effect, following the LibreOffice preset of the
/// same name with its durations scaled to `duration_ms`.
fn odp_effect_nodes(effect: &str, target: &str, duration_ms: u64) -> String {
    let target = escape(target);
    let dur = smil_time(duration_ms);
    let visibility = |visible: bool, begin_ms: u64, dur: &str| {
        format!(
            "<anim:set smil:begin=\"{}\" smil:dur=\"{dur}\" smil:fill=\"hold\" smil:targetElement=\"{target}\" smil:attributeName=\"visibility\" smil:to=\"{}\"/>",
            smil_time(begin_ms),
            if visible { "visible" } else { "hidden" }
        )
    };
    let show = visibility(true, 0, "0.001s");
    let hide_at_end = visibility(false, duration_ms.saturating_sub(1), "0.001s");
    let animate = |attribute: &str, values: &str| {
        format!(
            "<anim:animate smil:dur=\"{dur}\" smil:fill=\"hold\" smil:targetElement=\"{target}\" smil:attributeName=\"{attribute}\" smil:values=\"{values}\" smil:keyTimes=\"0;1\"/>"
        )
    };
    let fade = |mode: &str| {
        format!(
            "<anim:transitionFilter smil:dur=\"{dur}\" smil:targetElement=\"{target}\" smil:type=\"fade\" smil:subtype=\"crossfade\"{mode}/>"
        )
    };
    let transform = |kind: &str, by: &str| {
        format!(
            "<anim:animateTransform smil:dur=\"{dur}\" smil:fill=\"hold\" smil:targetElement=\"{target}\" smil:by=\"{by}\" svg:type=\"{kind}\"/>"
        )
    };
    match effect {
        "fade" => format!("{show}{}", fade("")),
        "flyIn" => format!("{show}{}{}", animate("x", "x;x"), animate("y", "1+height/2;y")),
        "zoom" => format!("{show}{}{}", animate("width", "0;width"), animate("height", "0;height")),
        "disappear" => visibility(false, 0, &dur),
        "fadeOut" => format!("{}{hide_at_end}", fade(" smil:mode=\"out\"")),
        "flyOut" => format!("{}{}{hide_at_end}", animate("x", "x;x"), animate("y", "y;0-height/2")),
        "spin" => transform("rotate", "360"),
        "grow" => transform("scale", "1.5,1.5"),
        "shrink" => transform("scale", "0.5,0.5"),
        "pulse" => format!(
            "<anim:transitionFilter smil:dur=\"{dur}\" smil:targetElement=\"{target}\" smil:keySplines=\"0,0;0.2,0.5;0.8,0.5;1,0\" smil:type=\"fade\" smil:subtype=\"crossfade\" smil:mode=\"out\"/><anim:animateTransform smil:dur=\"{}\" smil:fill=\"hold\" smil:autoReverse=\"true\" smil:targetElement=\"{target}\" smil:by=\"1.05,1.05\" svg:type=\"scale\"/>",
            smil_time(duration_ms / 2)
        ),
        // "appear": visible for the whole effect so its duration survives.
        _ => visibility(true, 0, &dur),
    }
}

/// One "after previous" step of a click group in the main sequence.
struct OdpTimingStep {
    begin_ms: u64,
    end_ms: u64,
    effects: String,
}

/// The slide's main sequence as ODF SMIL timing, the structure LibreOffice
/// writes: timing root -> main sequence -> one `anim:par` per click -> one per
/// "after previous" step -> one per effect. The effect `anim:par` carries the
/// trigger (`presentation:node-type`), class, preset and delay; its children
/// carry the duration and the target shape.
fn odp_timing_xml(slide: &Slide, ids: &HashMap<String, String>, warnings: &mut Vec<String>) -> String {
    let mut animations: Vec<&Animation> = slide.animations.iter().collect();
    animations.sort_by_key(|animation| animation.order);
    let mut clicks: Vec<(&str, Vec<OdpTimingStep>)> = Vec::new();
    for animation in animations {
        let Some(target) = ids.get(&animation.object_id) else {
            warnings.push(format!(
                "An animation targeting \"{}\" was not written because the object is not on the exported slide.",
                animation.object_id
            ));
            continue;
        };
        if !matches!(animation.kind.as_str(), "entrance" | "exit" | "emphasis") {
            warnings.push(format!(
                "An animation of kind \"{}\" was kept in the native .oswk file and not written to the ODP timing.",
                animation.kind
            ));
            continue;
        }
        let (effect, exact) = odp_export_effect(&animation.kind, &animation.effect);
        let Some(&(_, _, preset_id, sub_type)) =
            ODP_PRESETS.iter().find(|preset| preset.0 == animation.kind && preset.1 == effect)
        else {
            continue;
        };
        if !exact {
            warnings.push(format!(
                "The {} effect \"{}\" has no LibreOffice preset and was written as \"{preset_id}\".",
                animation.kind, animation.effect
            ));
        }
        let node_type = match animation.trigger.as_str() {
            "withPrevious" => "with-previous",
            "afterPrevious" => "after-previous",
            _ => "on-click",
        };
        let first_step = || vec![OdpTimingStep { begin_ms: 0, end_ms: 0, effects: String::new() }];
        match (node_type, clicks.last_mut()) {
            ("on-click", _) => clicks.push(("indefinite", first_step())),
            // Effects before the first click start with the slide.
            (_, None) => clicks.push(("0s", first_step())),
            ("after-previous", Some((_, steps))) => {
                let begin_ms = steps.last().map(|step| step.end_ms).unwrap_or(0);
                steps.push(OdpTimingStep { begin_ms, end_ms: begin_ms, effects: String::new() });
            }
            _ => {}
        }
        let Some(step) = clicks.last_mut().and_then(|(_, steps)| steps.last_mut()) else { continue };
        let duration = u64::from(animation.duration_ms);
        let delay = u64::from(animation.delay_ms);
        let sub_type =
            if sub_type.is_empty() { String::new() } else { format!(" presentation:preset-sub-type=\"{sub_type}\"") };
        step.effects.push_str(&format!(
            "<anim:par smil:begin=\"{}\" smil:fill=\"hold\" presentation:node-type=\"{node_type}\" presentation:preset-class=\"{}\" presentation:preset-id=\"{preset_id}\"{sub_type}>{}</anim:par>",
            smil_time(delay),
            animation.kind,
            odp_effect_nodes(effect, target, duration)
        ));
        step.end_ms = step.end_ms.max(step.begin_ms + delay + duration);
    }
    if clicks.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "<anim:par presentation:node-type=\"timing-root\"><anim:seq presentation:node-type=\"main-sequence\">",
    );
    for (begin, steps) in clicks {
        out.push_str(&format!("<anim:par smil:begin=\"{begin}\">"));
        for step in steps {
            out.push_str(&format!("<anim:par smil:begin=\"{}\">{}</anim:par>", smil_time(step.begin_ms), step.effects));
        }
        out.push_str("</anim:par>");
    }
    out.push_str("</anim:seq></anim:par>");
    out
}

/// Writes a `draw:text-box`'s paragraphs the way the ODT writer writes body
/// text: automatic paragraph and text styles shared through `AutoStyles`,
/// bullets collected into `text:list` elements, and `Standard` for a plain
/// paragraph. Runs that no longer add up to `paragraph.text` fall back to the
/// paragraph text itself, so an edit can never lose content.
fn write_odp_text_frame(writer: &mut XmlWriter, text: &TextFrame, styles: &mut AutoStyles) {
    let mut index = 0usize;
    while index < text.paragraphs.len() {
        let paragraph = &text.paragraphs[index];
        if !paragraph.bullet {
            write_odp_paragraph(writer, paragraph, styles);
            index += 1;
            continue;
        }
        // A run of consecutive bullets becomes one list, nested by level.
        let start = index;
        while index < text.paragraphs.len() && text.paragraphs[index].bullet {
            index += 1;
        }
        write_odp_list(writer, &text.paragraphs[start..index], styles, 0);
    }
    if text.paragraphs.is_empty() {
        writer.raw("<text:p/>");
    }
}

/// One `text:list` of a bullet run at `level`; items deeper than `level` become
/// nested lists inside their item, so the reader can recover the level.
fn write_odp_list(writer: &mut XmlWriter, items: &[TextParagraph], styles: &mut AutoStyles, level: u32) {
    writer.raw("<text:list text:style-name=\"LB\">");
    let mut index = 0usize;
    while index < items.len() {
        writer.raw("<text:list-item>");
        write_odp_paragraph(writer, &items[index], styles);
        let mut end = index + 1;
        while end < items.len() && items[end].level > level {
            end += 1;
        }
        if end > index + 1 {
            write_odp_list(writer, &items[index + 1..end], styles, level + 1);
        }
        writer.raw("</text:list-item>");
        index = end;
    }
    writer.raw("</text:list>");
}

fn write_odp_paragraph(writer: &mut XmlWriter, paragraph: &TextParagraph, styles: &mut AutoStyles) {
    let props = ParaProps {
        align: paragraph.align.clone(),
        list: paragraph.bullet.then(|| ListInfo {
            kind: "bullet".into(),
            level: paragraph.level,
            start: 1,
            marker: "•".into(),
        }),
        ..Default::default()
    };
    let properties = paragraph_style_xml(&props, false);
    let name = if properties == "<style:paragraph-properties/>" {
        "Standard".to_string()
    } else {
        styles.paragraph(properties)
    };
    writer.raw(&format!("<text:p text:style-name=\"{name}\">"));
    let runs_match = paragraph.runs.iter().map(|run| run.text.as_str()).collect::<String>() == paragraph.text;
    if paragraph.runs.is_empty() || !runs_match {
        let fallback = Run {
            text: paragraph.text.clone(),
            bold: paragraph.bold,
            italic: paragraph.italic,
            underline: paragraph.underline,
            size_pt: paragraph.size_pt,
            color: paragraph.color.clone(),
            lang: paragraph.lang.clone(),
            ..Default::default()
        };
        write_runs(writer, &[fallback], styles, &NoteContext::default());
    } else {
        write_runs(writer, &paragraph.runs, styles, &NoteContext::default());
    }
    writer.raw("</text:p>");
}

/// One shape of a slide as `draw:*` XML. `charts` maps a chart object's model
/// id to the `Object N` sub-document the writer stored for it.
fn slide_object_xml(
    object: &SlideObject,
    ids: &mut OdpShapeIds,
    styles: &mut AutoStyles,
    charts: &HashMap<String, String>,
) -> String {
    let id = ids.assign(object);
    let id_attrs = format!(" draw:id=\"{0}\" xml:id=\"{0}\"", escape(&id));
    if object.kind == "group" {
        // Children keep their absolute page coordinates; draw:g has no box of
        // its own, which matches the editor's group = bounding box of children.
        let mut children: Vec<&SlideObject> = object.children.iter().collect();
        children.sort_by_key(|child| child.z);
        let inner: String = children.into_iter().map(|child| slide_object_xml(child, ids, styles, charts)).collect();
        return format!(
            "<draw:g draw:name=\"{}\"{id_attrs} draw:z-index=\"{}\">{inner}</draw:g>",
            escape(&object.name),
            object.z
        );
    }
    let frame_start = format!("<draw:frame draw:name=\"{}\"{id_attrs} text:anchor-type=\"page\"", escape(&object.name));
    let style = object.style.clone().unwrap_or_default();
    let mut inner = String::new();
    match object.kind.as_str() {
        "image" => {
            if let Some(image) = &object.image {
                // `fo:clip` trims the rendered frame; the fractions are of the
                // rendered size, spelled in cm with the same precision as every
                // other length in this writer.
                let clip = image
                    .crop
                    .as_ref()
                    .filter(|crop| crop.left > 0.0 || crop.top > 0.0 || crop.right > 0.0 || crop.bottom > 0.0)
                    .map(|crop| {
                        format!(
                            " fo:clip=\"rect({} {} {} {})\"",
                            cm(crop.top * object.h.max(1.0)),
                            cm(crop.right * object.w.max(1.0)),
                            cm(crop.bottom * object.h.max(1.0)),
                            cm(crop.left * object.w.max(1.0))
                        )
                    })
                    .unwrap_or_default();
                inner.push_str(&format!("<draw:image xlink:href=\"Pictures/{}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"{clip}/>", escape(&image.name)));
            }
        }
        "line" | "arrow" => {
            let line = object.line.clone().unwrap_or_default();
            let (x1, y1, x2, y2) = (object.x, object.y, object.x + line.x2, object.y + line.y2);
            if line.begin_object.is_some() || line.end_object.is_some() {
                // A glued line is a real connector: LibreOffice needs the
                // written shape ids (planned before writing) and glue points.
                let mut glue = String::new();
                if let Some(target) = line.begin_object.as_deref().and_then(|target| ids.slide.get(target)) {
                    glue.push_str(&format!(
                        " draw:start-shape=\"{}\" draw:start-glue-point=\"{}\"",
                        escape(target),
                        line.begin_site
                    ));
                }
                if let Some(target) = line.end_object.as_deref().and_then(|target| ids.slide.get(target)) {
                    glue.push_str(&format!(
                        " draw:end-shape=\"{}\" draw:end-glue-point=\"{}\"",
                        escape(target),
                        line.end_site
                    ));
                }
                let stroke = object.style.as_ref().and_then(|style| style.stroke.clone());
                let width = object.style.as_ref().map(|style| style.stroke_width_pt).unwrap_or(2.0).max(0.5);
                let properties = format!(
                    "<style:graphic-properties draw:stroke=\"solid\" svg:stroke-width=\"{}\"{} draw:fill=\"none\"/>",
                    cm(width),
                    stroke.map(|color| format!(" svg:stroke-color=\"{}\"", escape(&color))).unwrap_or_default()
                );
                let style_name = styles.graphic(properties);
                inner.push_str(&format!(
                    "<draw:connector svg:x1=\"{}\" svg:y1=\"{}\" svg:x2=\"{}\" svg:y2=\"{}\"{glue} draw:style-name=\"{style_name}\"/>",
                    cm(x1),
                    cm(y1),
                    cm(x2),
                    cm(y2)
                ));
            } else {
                inner.push_str(&format!(
                    "<draw:line svg:x1=\"{}\" svg:y1=\"{}\" svg:x2=\"{}\" svg:y2=\"{}\" draw:style-name=\"gr1\"/>",
                    cm(x1),
                    cm(y1),
                    cm(x2),
                    cm(y2)
                ));
            }
        }
        "table" => {
            if let Some(table) = &object.table {
                let mut writer = XmlWriter::new();
                let mut styles = AutoStyles::default();
                let mut media = Media::default();
                write_table(&mut writer, table, &mut styles, &mut media, &NoteContext::default());
                inner.push_str(&writer.finish());
            }
        }
        "chart" => match charts.get(&object.id) {
            Some(name) => inner.push_str(&format!(
                "<draw:object xlink:href=\"./{}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/>",
                escape(name)
            )),
            None => {
                // A chart this writer cannot export keeps the old title
                // placeholder so nothing is dropped silently.
                if let Some(chart) = &object.chart {
                    let title =
                        if chart.title.trim().is_empty() { format!("{} chart", chart.kind) } else { chart.title.clone() };
                    inner.push_str(&format!(
                        "<draw:text-box><text:p text:style-name=\"Standard\">{}</text:p></draw:text-box>",
                        crate::xml::escape_text(&title)
                    ));
                }
            }
        },
        _ => {
            if let Some(text) = &object.text {
                let mut writer = XmlWriter::new();
                writer.raw("<draw:text-box>");
                write_odp_text_frame(&mut writer, text, styles);
                writer.raw("</draw:text-box>");
                inner.push_str(&writer.finish());
            }
        }
    }
    let shape = match object.kind.as_str() {
        "ellipse" => "draw:ellipse",
        "line" | "arrow" => return format!("{frame_start} draw:z-index=\"{}\">{inner}</draw:frame>", object.z),
        _ => "draw:frame",
    };
    if shape == "draw:frame" {
        let fill = style
            .fill
            .as_deref()
            .map(|fill| format!(" draw:fill=\"solid\" draw:fill-color=\"{}\"", escape(fill)))
            .unwrap_or_else(|| " draw:fill=\"none\"".into());
        format!(
            "{frame_start} svg:x=\"{}\" svg:y=\"{}\" svg:width=\"{}\" svg:height=\"{}\" draw:z-index=\"{}\"{}>{}<draw:glue-points/><draw:enhanced-geometry/></draw:frame>",
            cm(object.x),
            cm(object.y),
            cm(object.w.max(4.0)),
            cm(object.h.max(4.0)),
            object.z,
            fill,
            inner
        )
        .replace("<draw:glue-points/><draw:enhanced-geometry/>", "")
    } else {
        format!(
            "{frame_start} svg:x=\"{}\" svg:y=\"{}\" svg:width=\"{}\" svg:height=\"{}\" draw:z-index=\"{}\">{}</draw:frame>",
            cm(object.x),
            cm(object.y),
            cm(object.w.max(4.0)),
            cm(object.h.max(4.0)),
            object.z,
            inner
        )
    }
}

#[derive(Debug, Clone)]
pub struct DeckWrite {
    pub bytes: Vec<u8>,
    pub warnings: Vec<String>,
}

/// Assigns ids to the shapes of one slide in the order `slide_object_xml`
/// visits them, so a connector can name a shape written later in the z order.
fn plan_odp_ids(objects: &[SlideObject], ids: &mut OdpShapeIds) {
    let mut ordered: Vec<&SlideObject> = objects.iter().collect();
    ordered.sort_by_key(|object| object.z);
    for object in ordered {
        if !object.id.is_empty() {
            ids.plan(object);
        }
        plan_odp_ids(&object.children, ids);
    }
}

/// The chart objects anywhere on a slide, in the order the writer visits them.
fn collect_odp_charts<'a>(objects: &'a [SlideObject], charts: &mut Vec<&'a SlideObject>) {
    let mut ordered: Vec<&SlideObject> = objects.iter().collect();
    ordered.sort_by_key(|object| object.z);
    for object in ordered {
        if object.chart.is_some() {
            charts.push(object);
        }
        collect_odp_charts(&object.children, charts);
    }
}

fn collect_odp_pictures(objects: &[SlideObject], pictures: &mut Vec<(String, Vec<u8>)>) {
    for object in objects {
        if let Some(image) = &object.image {
            if !image.is_empty() && !pictures.iter().any(|(name, _)| name == &image.name) {
                pictures.push((image.name.clone(), image.bytes()));
            }
        }
        collect_odp_pictures(&object.children, pictures);
    }
}

pub fn write_odp_package(deck: &Deck) -> OfficeResult<DeckWrite> {
    let mut warnings = Vec::new();
    let bytes = write_odp_bytes(deck, &mut warnings)?;
    let mut seen = std::collections::HashSet::new();
    warnings.retain(|warning| seen.insert(warning.clone()));
    Ok(DeckWrite { bytes, warnings })
}

pub fn write_odp(deck: &Deck) -> OfficeResult<Vec<u8>> {
    Ok(write_odp_package(deck)?.bytes)
}

/// The `text:list` style the ODP content declares for the bullet lists the
/// writer emits (`LB`, the same name the ODT writer uses).
fn odp_list_style_xml() -> String {
    let mut out = String::from("<text:list-style style:name=\"LB\">");
    for level in 1..=9 {
        out.push_str(&format!("<text:list-level-style-bullet text:level=\"{level}\" text:bullet-char=\"•\"><style:list-level-properties text:space-before=\"{}cm\" text:min-label-width=\"0.6cm\"/></text:list-level-style-bullet>", (level as f64 - 1.0) * 0.6));
    }
    out.push_str("</text:list-style>");
    out
}

/// One placeholder frame of the ODF master page for the footer, date and slide
/// number; `body` is already escaped (or a field element).
fn odp_master_frame(class: &str, x: f64, y: f64, w: f64, h: f64, body: &str) -> String {
    format!(
        "<draw:frame presentation:class=\"{class}\" svg:x=\"{}\" svg:y=\"{}\" svg:width=\"{}\" svg:height=\"{}\"><draw:text-box><text:p>{body}</text:p></draw:text-box></draw:frame>",
        cm(x),
        cm(y),
        cm(w),
        cm(h)
    )
}

/// The placeholder frames of the default master page for an enabled footer,
/// laid out along the bottom of the slide like the PPTX writer lays them out.
fn odp_footer_shapes(deck: &Deck, footer: &SlideFooter) -> String {
    let width = deck.size.width_pt;
    let bottom = (deck.size.height_pt - 34.0).max(0.0);
    let mut out = String::new();
    if footer.show_date {
        out.push_str(&odp_master_frame(
            "date-time",
            width * 0.06,
            bottom,
            width * 0.25,
            24.0,
            &crate::xml::escape_text(&footer.date_text),
        ));
    }
    if footer.show_text {
        out.push_str(&odp_master_frame(
            "footer",
            width * 0.35,
            bottom,
            width * 0.30,
            24.0,
            &crate::xml::escape_text(&footer.text),
        ));
    }
    if footer.show_slide_number {
        out.push_str(&odp_master_frame(
            "page-number",
            width * 0.88,
            bottom,
            width * 0.06,
            24.0,
            "<text:page-number>1</text:page-number>",
        ));
    }
    out
}

fn write_odp_bytes(deck: &Deck, warnings: &mut Vec<String>) -> OfficeResult<Vec<u8>> {
    let mut body = String::new();
    let mut pictures: Vec<(String, Vec<u8>)> = Vec::new();
    let mut ids = OdpShapeIds::default();
    // Charts become `Object N` sub-documents numbered across the deck.
    let mut chart_objects: Vec<OdsChartObject> = Vec::new();
    let mut auto = AutoStyles::default();
    for (index, slide) in deck.slides.iter().enumerate() {
        let style_attr = if slide.hidden { " draw:style-name=\"dp-hidden\"" } else { "" };
        body.push_str(&format!(
            "<draw:page draw:name=\"Slide{}\" draw:master-page-name=\"Default\"{style_attr}>",
            index + 1
        ));
        ids.reset_slide();
        plan_odp_ids(&slide.objects, &mut ids);
        collect_odp_pictures(&slide.objects, &mut pictures);
        let mut chart_map: HashMap<String, String> = HashMap::new();
        let mut charts = Vec::new();
        collect_odp_charts(&slide.objects, &mut charts);
        for chart_object in charts {
            let Some(chart) = &chart_object.chart else { continue };
            match ods_chart_problem(chart) {
                Some(reason) => warnings.push(format!("Chart data is kept in the native .oswk file: {reason}.")),
                None => {
                    let name = format!("Object {}", chart_objects.len() + 1);
                    let placement = ChartPlacement {
                        id: chart_object.id.clone(),
                        chart: chart.clone(),
                        anchor: String::new(),
                        width_px: chart_object.w.max(48.0) / 0.75,
                        height_px: chart_object.h.max(48.0) / 0.75,
                    };
                    chart_objects.push(OdsChartObject {
                        content: ods_chart_content(&placement, "Sheet1", &BTreeMap::new()),
                        name: name.clone(),
                    });
                    if !chart_object.id.is_empty() {
                        chart_map.insert(chart_object.id.clone(), name);
                    }
                }
            }
        }
        let mut objects: Vec<&SlideObject> = slide.objects.iter().collect();
        objects.sort_by_key(|object| object.z);
        for object in objects {
            body.push_str(&slide_object_xml(object, &mut ids, &mut auto, &chart_map));
        }
        // draw:page content order: shapes, then the timing root, then notes.
        body.push_str(&odp_timing_xml(slide, &ids.slide, warnings));
        if !slide.notes.is_empty() {
            body.push_str(&format!("<presentation:notes><draw:frame presentation:class=\"notes\"><draw:text-box><text:p>{}</text:p></draw:text-box></draw:frame></presentation:notes>", crate::xml::escape_text(&slide.notes)));
        }
        body.push_str("</draw:page>");
    }
    let page_layout = format!(
        "<style:page-layout style:name=\"pl1\"><style:page-layout-properties fo:page-width=\"{}\" fo:page-height=\"{}\"/></style:page-layout>",
        cm(deck.size.width_pt),
        cm(deck.size.height_pt)
    );
    // A hidden slide is a drawing-page style with `visibility="hidden"`
    // (LibreOffice's convention); visible-only output stays exactly as before.
    let hidden_style = if deck.slides.iter().any(|slide| slide.hidden) {
        "<style:style style:name=\"dp-hidden\" style:family=\"drawing-page\"><style:drawing-page-properties presentation:visibility=\"hidden\"/></style:style>"
    } else {
        ""
    };
    let automatic = auto.xml();
    let automatic =
        automatic.trim_start_matches("<office:automatic-styles>").trim_end_matches("</office:automatic-styles>");
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NS} {ANIM_NS} office:version=\"1.2\"><office:styles>{}</office:styles><office:automatic-styles>{page_layout}{hidden_style}{automatic}</office:automatic-styles><office:body><office:presentation>{body}</office:presentation></office:body></office:document-content>",
        odp_list_style_xml()
    );
    let background = match deck.theme.as_str() {
        "dark" => "#0F172A",
        "business" => "#F8FAFC",
        "education" => "#FEFCE8",
        "modern" => "#FFFFFF",
        _ => "#FFFFFF",
    };
    let mut drawing_props = format!("draw:fill=\"solid\" draw:fill-color=\"{background}\"");
    let mut master_shapes = String::new();
    if let Some(footer) = deck.footer.as_ref().filter(|footer| footer.enabled) {
        if footer.show_text {
            drawing_props.push_str(" presentation:display-footer=\"true\"");
        }
        if footer.show_slide_number {
            drawing_props.push_str(" presentation:display-page-number=\"true\"");
        }
        if footer.show_date {
            drawing_props.push_str(" presentation:display-date-time=\"true\"");
        }
        master_shapes = odp_footer_shapes(deck, footer);
    }
    let styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NS} office:version=\"1.2\"><office:styles/><office:automatic-styles>{page_layout}</office:automatic-styles><office:master-styles><style:master-page style:name=\"Default\" style:page-layout-name=\"pl1\"><style:drawing-page-properties {drawing_props}/>{master_shapes}</style:master-page></office:master-styles></office:document-styles>"
    );
    // Every chart object is a sub-document with its own manifest entries.
    let mut object_entries = String::new();
    for object in &chart_objects {
        let name = escape(&object.name);
        object_entries.push_str(&format!(
            "<manifest:file-entry manifest:full-path=\"{name}/\" manifest:version=\"1.2\" manifest:media-type=\"application/vnd.oasis.opendocument.chart\"/><manifest:file-entry manifest:full-path=\"{name}/content.xml\" manifest:media-type=\"text/xml\"/><manifest:file-entry manifest:full-path=\"{name}/styles.xml\" manifest:media-type=\"text/xml\"/>"
        ));
    }
    let manifest = manifest_for("application/vnd.oasis.opendocument.presentation")
        .replace("</manifest:manifest>", &format!("{object_entries}</manifest:manifest>"));
    let mut zip = ZipWriter::new();
    zip.add_text("mimetype", "application/vnd.oasis.opendocument.presentation");
    zip.add_text("META-INF/manifest.xml", &manifest);
    zip.add_text("content.xml", &content);
    zip.add_text("styles.xml", &styles);
    zip.add_text("meta.xml", &meta_xml(&deck.title, "OmniOffice", None));
    for object in &chart_objects {
        zip.add_text(&format!("{}/content.xml", object.name), &object.content);
        zip.add_text(
            &format!("{}/styles.xml", object.name),
            &format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NS} {CHART_NS} office:version=\"1.2\"><office:styles/></office:document-styles>"
            ),
        );
    }
    for (name, data) in &pictures {
        zip.add(&format!("Pictures/{name}"), data);
    }
    Ok(zip.finish())
}

pub fn write_odp_file(path: &Path, deck: &Deck) -> OfficeResult<()> {
    crate::io::write_atomic(path, &write_odp(deck)?)
}

/// Read-side state of one ODP package: named styles, crops declared by graphic
/// styles and the embedded objects already read.
#[derive(Default)]
struct OdpImport {
    styles: ReadStyles,
    clips: HashMap<String, String>,
    seen_objects: HashSet<String>,
}

/// `style:graphic-properties/fo:clip` by style name, for images that carry
/// their crop on a graphic style instead of the `draw:image` element.
fn odp_graphic_clips(roots: &[&XmlNode]) -> HashMap<String, String> {
    let mut clips = HashMap::new();
    for root in roots {
        let mut styles = Vec::new();
        root.find_all("style", &mut styles);
        for style in styles {
            let Some(name) = style.attr_any_ns("name") else { continue };
            let mut properties = Vec::new();
            style.find_all("graphic-properties", &mut properties);
            if let Some(clip) = properties.iter().find_map(|node| node.attr_any_ns("clip")) {
                clips.insert(name.to_string(), clip.to_string());
            }
        }
    }
    clips
}

/// The crop of an ODP image, read from `fo:clip="rect(top right bottom left)"`
/// on the `draw:image`, the frame or its graphic style. The lengths are the
/// trims of the rendered frame and become fractions of the frame size. Clip
/// lengths round-trip through the writer's three-decimal cm spelling, so the
/// fractions are accurate to roughly a thousandth of the frame size; the tests
/// allow 0.01.
fn read_odp_crop(image: &XmlNode, frame: &XmlNode, import: &OdpImport, w: f64, h: f64) -> Option<ImageCrop> {
    let clip = image
        .attr_any_ns("clip")
        .or_else(|| frame.attr_any_ns("clip"))
        .map(str::to_string)
        .or_else(|| frame.attr_any_ns("style-name").and_then(|name| import.clips.get(name)).cloned())?;
    let lengths = clip.trim().strip_prefix("rect(")?.strip_suffix(')')?;
    let lengths: Option<Vec<f64>> = lengths
        .split(|ch: char| ch.is_whitespace() || ch == ',')
        .filter(|part| !part.is_empty())
        .map(parse_cm)
        .collect();
    let lengths = lengths?;
    let [top, right, bottom, left] = lengths.as_slice() else { return None };
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let crop = ImageCrop {
        left: (left / w).abs().clamp(0.0, 0.95),
        top: (top / h).abs().clamp(0.0, 0.95),
        right: (right / w).abs().clamp(0.0, 0.95),
        bottom: (bottom / h).abs().clamp(0.0, 0.95),
    };
    (crop.left > 0.0 || crop.top > 0.0 || crop.right > 0.0 || crop.bottom > 0.0).then_some(crop)
}

/// A `draw:line` or `draw:connector` shape: geometry from the svg endpoints and
/// the glue references (`draw:start-shape` and friends) of a connector.
fn read_odp_line(node: &XmlNode) -> Option<SlideObject> {
    let x1 = node.attr_any_ns("x1").and_then(parse_cm)?;
    let y1 = node.attr_any_ns("y1").and_then(parse_cm)?;
    let x2 = node.attr_any_ns("x2").and_then(parse_cm).unwrap_or(x1);
    let y2 = node.attr_any_ns("y2").and_then(parse_cm).unwrap_or(y1);
    let mut object = SlideObject::new("line", x1, y1, (x2 - x1).abs(), (y2 - y1).abs());
    object.line = Some(LineSpec {
        x2: x2 - x1,
        y2: y2 - y1,
        begin_arrow: node.attr_any_ns("marker-start").is_some(),
        end_arrow: node.attr_any_ns("marker-end").is_some(),
        dash: String::new(),
        begin_object: node.attr_any_ns("start-shape").map(str::to_string),
        end_object: node.attr_any_ns("end-shape").map(str::to_string),
        begin_site: node.attr_any_ns("start-glue-point").and_then(|value| value.parse::<u32>().ok()).unwrap_or(0),
        end_site: node.attr_any_ns("end-glue-point").and_then(|value| value.parse::<u32>().ok()).unwrap_or(0),
    });
    Some(object)
}

/// One `text:p` of a shape: runs with named styles resolved, alignment from
/// the paragraph style, and paragraph-level formatting aggregated from the
/// runs like the PPTX import does.
fn read_odp_paragraph(paragraph: &XmlNode, level: u32, bullet: bool, styles: &ReadStyles) -> TextParagraph {
    let mut runs = Vec::new();
    let mut notes = NoteReadState::default();
    if !paragraph.text.is_empty() {
        runs.push(Run { text: paragraph.text.clone(), ..Default::default() });
    }
    for child in &paragraph.children {
        node_text_runs(child, &mut runs, &mut notes, styles);
    }
    let props = paragraph_props(paragraph, styles);
    let mut model = TextParagraph { level, bullet, align: props.align, ..Default::default() };
    for run in &runs {
        model.text.push_str(&run.text);
        model.bold |= run.bold;
        model.italic |= run.italic;
        model.underline |= run.underline;
        if model.size_pt.is_none() {
            model.size_pt = run.size_pt;
        }
        if model.color.is_none() {
            model.color = run.color.clone();
        }
    }
    model.lang = runs.iter().find_map(|run| run.lang.clone());
    model.runs = runs;
    model
}

/// The paragraphs of a `draw:text-box`: plain paragraphs and lists, the latter
/// nested so the nesting depth is the bullet level.
fn read_odp_paragraphs(text_box: &XmlNode, styles: &ReadStyles) -> Vec<TextParagraph> {
    fn walk(node: &XmlNode, depth: u32, styles: &ReadStyles, out: &mut Vec<TextParagraph>) {
        for child in &node.children {
            match child.local_name() {
                "p" | "h" => out.push(read_odp_paragraph(child, depth.saturating_sub(1), depth > 0, styles)),
                "list" => {
                    for item in child.children_named("list-item") {
                        walk(item, depth + 1, styles, out);
                    }
                }
                _ => {}
            }
        }
    }
    let mut paragraphs = Vec::new();
    walk(text_box, 0, styles, &mut paragraphs);
    if paragraphs.is_empty() {
        paragraphs.push(TextParagraph::default());
    }
    paragraphs
}

/// One `draw:frame` of a slide or group: picture, embedded chart, text box,
/// table, connector, or a plain shape kept as a rectangle. Frames without a
/// size (lines) are skipped.
fn read_odp_frame(
    frame: &XmlNode,
    zip: &ZipReader,
    import: &mut OdpImport,
    warnings: &mut Vec<String>,
) -> Option<SlideObject> {
    let x = frame.attr_any_ns("x").and_then(parse_cm).unwrap_or(40.0);
    let y = frame.attr_any_ns("y").and_then(parse_cm).unwrap_or(40.0);
    let w = frame.attr_any_ns("width").and_then(parse_cm).unwrap_or(320.0);
    let h = frame.attr_any_ns("height").and_then(parse_cm).unwrap_or(180.0);
    let mut images = Vec::new();
    frame.find_all("image", &mut images);
    if let Some(image_node) = images.first() {
        if let Some(href) = image_node.attr_any_ns("href") {
            let path = href.trim_start_matches("./");
            if let Ok(data) = zip.read(path) {
                let name = path.rsplit('/').next().unwrap_or("image.png").to_string();
                let mut object = SlideObject::new("image", x, y, w, h);
                let mut image = ImageData::from_bytes(&name, &data);
                image.crop = read_odp_crop(image_node, frame, import, w, h);
                object.image = Some(image);
                return Some(object);
            }
        }
    }
    // An embedded object is a chart when its sub-document parses as one; an
    // unrecognized one falls through to the rectangle fallback below.
    if frame.child("object").is_some() {
        let sheet = Sheet::new("Sheet1");
        if let Some(placement) = read_ods_chart(zip, frame, String::new(), &sheet, &mut import.seen_objects, warnings) {
            let mut object = SlideObject::new("chart", x, y, w, h);
            object.chart = Some(placement.chart);
            return Some(object);
        }
    }
    if let Some(line) = frame
        .children_of("connector")
        .into_iter()
        .next()
        .or_else(|| frame.children_of("line").into_iter().next())
        .and_then(read_odp_line)
    {
        return Some(line);
    }
    let mut text_boxes = Vec::new();
    frame.find_all("text-box", &mut text_boxes);
    if let Some(text_box) = text_boxes.first() {
        let paragraphs = read_odp_paragraphs(text_box, &import.styles);
        let mut object = SlideObject::new("text", x, y, w, h);
        if paragraphs.iter().any(|paragraph| !paragraph.text.trim().is_empty()) {
            object.text = Some(TextFrame { paragraphs, ..Default::default() });
        }
        return Some(object);
    }
    let mut tables = Vec::new();
    frame.find_all("table", &mut tables);
    if let Some(table) = tables.first() {
        let mut rows = Vec::new();
        for row_node in table.children_named("table-row") {
            let mut cells = Vec::new();
            for cell in row_node.children_named("table-cell") {
                let mut inner = Vec::new();
                cell.find_all("p", &mut inner);
                let cell_text = inner.iter().map(|node| node.deep_text()).collect::<Vec<_>>().join("\n");
                cells.push(TableCell { blocks: vec![Block::paragraph(&cell_text)], ..Default::default() });
            }
            rows.push(TableRow { cells, ..Default::default() });
        }
        let mut object = SlideObject::new("table", x, y, w, h);
        object.table = Some(TableData { rows, ..Default::default() });
        return Some(object);
    }
    // Shapes we cannot map precisely are preserved as rectangles.
    if frame.attr_any_ns("width").is_some() {
        let fill = match frame.attr_any_ns("fill") {
            Some("none") => None,
            _ => Some(frame.attr_any_ns("fill-color").unwrap_or("#E2E8F0").to_string()),
        };
        let mut object = SlideObject::new("rect", x, y, w, h);
        object.style = Some(ShapeStyle { fill, ..Default::default() });
        warnings.push("Some shapes were imported as simple rectangles.".into());
        return Some(object);
    }
    None
}

/// The shapes directly below `parent` (a `draw:page` or a `draw:g`) in
/// document order. A `draw:g` becomes a `group` object whose box is the
/// bounding box of its children, as the editor sizes groups; children keep
/// their absolute coordinates. Shape ids come from `xml:id`/`draw:id` so the
/// slide timing can find its targets.
fn read_odp_shapes(
    parent: &XmlNode,
    zip: &ZipReader,
    import: &mut OdpImport,
    warnings: &mut Vec<String>,
) -> Vec<SlideObject> {
    let mut objects = Vec::new();
    for node in &parent.children {
        let object = match node.local_name() {
            "frame" => read_odp_frame(node, zip, import, warnings),
            // A line or connector may be a direct child of the page as well as
            // wrapped in a frame, as some producers write it.
            "line" | "connector" => read_odp_line(node),
            "g" => {
                let children = read_odp_shapes(node, zip, import, warnings);
                if children.is_empty() {
                    warnings.push("A group without importable shapes was skipped.".into());
                    None
                } else {
                    let left = children.iter().map(|child| child.x).fold(f64::INFINITY, f64::min);
                    let top = children.iter().map(|child| child.y).fold(f64::INFINITY, f64::min);
                    let right =
                        children.iter().map(|child| child.x + child.w.max(0.0)).fold(f64::NEG_INFINITY, f64::max);
                    let bottom =
                        children.iter().map(|child| child.y + child.h.max(0.0)).fold(f64::NEG_INFINITY, f64::max);
                    let mut group =
                        SlideObject::new("group", left, top, (right - left).max(1.0), (bottom - top).max(1.0));
                    group.children = children;
                    Some(group)
                }
            }
            _ => None,
        };
        let Some(mut object) = object else { continue };
        object.z = objects.len() as i32 + 1;
        if let Some(id) = node.attr("xml:id").or_else(|| node.attr("draw:id")).filter(|id| !id.trim().is_empty()) {
            object.id = id.to_string();
        }
        object.name = node.attr("draw:name").unwrap_or_default().to_string();
        objects.push(object);
    }
    objects
}

fn collect_odp_ids<'a>(objects: &'a [SlideObject], out: &mut std::collections::HashSet<&'a str>) {
    for object in objects {
        out.insert(object.id.as_str());
        collect_odp_ids(&object.children, out);
    }
}

/// The effect `anim:par` nodes below a sequence, in document order. Effects are
/// the nodes that carry a trigger or a preset class; the click and "after
/// previous" containers around them are walked through.
fn collect_odp_effects<'a>(node: &'a XmlNode, out: &mut Vec<&'a XmlNode>) {
    for child in &node.children {
        if !matches!(child.local_name(), "par" | "seq" | "iterate") {
            continue;
        }
        let trigger = matches!(child.attr_any_ns("node-type"), Some("on-click" | "with-previous" | "after-previous"));
        if trigger || child.attr_any_ns("preset-class").is_some() {
            out.push(child);
        } else {
            collect_odp_effects(child, out);
        }
    }
}

/// Reads the slide's main sequence back into animations: trigger from the
/// effect's node type, class and preset to kind and editor effect (a preset
/// without an editor twin maps to the closest effect with a warning), delay
/// from its begin and duration from its longest child.
fn read_odp_timing(page: &XmlNode, objects: &[SlideObject], warnings: &mut Vec<String>) -> Vec<Animation> {
    let Some(root) = page.children.iter().find(|child| child.local_name() == "par") else { return Vec::new() };
    let mut effects = Vec::new();
    for sequence in root.children_named("seq") {
        match sequence.attr_any_ns("node-type") {
            Some("main-sequence") => collect_odp_effects(sequence, &mut effects),
            Some("interactive-sequence") => {
                warnings.push("Animations triggered by clicking a shape were not imported.".into());
            }
            _ => {}
        }
    }
    let mut ids = std::collections::HashSet::new();
    collect_odp_ids(objects, &mut ids);
    let mut animations = Vec::new();
    for effect in effects {
        let mut nodes = Vec::new();
        effect.walk(&mut nodes);
        let nodes = &nodes[1..];
        let preset = effect.attr_any_ns("preset-id").unwrap_or("");
        let kind = match effect.attr_any_ns("preset-class") {
            Some(kind @ ("entrance" | "exit" | "emphasis")) => kind,
            Some(other) => {
                warnings.push(format!("A \"{other}\" animation has no editor equivalent and was skipped."));
                continue;
            }
            None if preset.starts_with("ooo-entrance-") => "entrance",
            None if preset.starts_with("ooo-exit-") => "exit",
            None => "emphasis",
        };
        let Some(target) = nodes.iter().find_map(|node| node.attr_any_ns("targetElement")) else {
            warnings.push("An animation without a target object was skipped.".into());
            continue;
        };
        if !ids.contains(target) {
            warnings.push("An animation whose target shape was not imported was skipped.".into());
            continue;
        }
        let scale = nodes
            .iter()
            .find(|node| node.local_name() == "animateTransform" && node.attr_any_ns("type") == Some("scale"))
            .and_then(|node| node.attr_any_ns("by").or_else(|| node.attr_any_ns("to")))
            .and_then(|value| value.split(',').next()?.trim().parse::<f64>().ok());
        let exact = ODP_PRESETS.iter().find(|entry| {
            entry.0 == kind
                && entry.2 == preset
                && match entry.1 {
                    "grow" => scale.map(|scale| scale >= 1.0).unwrap_or(true),
                    "shrink" => scale.map(|scale| scale < 1.0).unwrap_or(false),
                    _ => true,
                }
        });
        let effect_name = match exact {
            Some(entry) => entry.1,
            None => {
                // A custom effect has no preset; its transition type is the best hint.
                let hint = if preset.is_empty() {
                    nodes
                        .iter()
                        .find_map(|node| node.attr_any_ns("type").filter(|_| node.local_name() == "transitionFilter"))
                        .unwrap_or("")
                } else {
                    preset
                };
                let closest = closest_odp_effect(kind, hint.trim_start_matches(&format!("ooo-{kind}-")));
                let label = if preset.is_empty() { "without a preset" } else { preset };
                warnings.push(format!(
                    "The {kind} effect \"{label}\" was imported as the closest editor effect \"{closest}\"."
                ));
                closest
            }
        };
        let trigger = match effect.attr_any_ns("node-type") {
            Some("with-previous") => "withPrevious",
            Some("after-previous") => "afterPrevious",
            _ => "onClick",
        };
        let duration = nodes
            .iter()
            .filter_map(|node| {
                let duration = node.attr_any_ns("dur").and_then(parse_smil_seconds)?;
                Some(node.attr_any_ns("begin").and_then(parse_smil_seconds).unwrap_or(0.0) + duration)
            })
            .fold(0.0, f64::max);
        animations.push(Animation {
            id: uuid::Uuid::new_v4().to_string(),
            object_id: target.to_string(),
            kind: kind.to_string(),
            effect: effect_name.to_string(),
            trigger: trigger.to_string(),
            duration_ms: seconds_to_ms(duration),
            delay_ms: effect.attr_any_ns("begin").and_then(parse_smil_seconds).map(seconds_to_ms).unwrap_or(0),
            order: animations.len() as u32,
        });
    }
    animations
}

/// The names of the drawing-page styles that mark a slide hidden
/// (`presentation:visibility="hidden"` on their properties), LibreOffice's
/// convention for a hidden slide.
fn odp_hidden_page_styles(root: &XmlNode) -> HashSet<String> {
    let mut hidden = HashSet::new();
    let mut styles = Vec::new();
    root.find_all("style", &mut styles);
    for style in styles {
        if style.attr_any_ns("family") != Some("drawing-page") {
            continue;
        }
        let Some(name) = style.attr_any_ns("name") else { continue };
        let mut properties = Vec::new();
        style.find_all("drawing-page-properties", &mut properties);
        if properties.iter().any(|node| node.attr_any_ns("visibility") == Some("hidden")) {
            hidden.insert(name.to_string());
        }
    }
    hidden
}

/// The deck footer settings a master page declares: the
/// `presentation:display-*` flags on its drawing-page properties, plus the
/// placeholder frames (`presentation:class`) carrying the footer and date
/// text. `None` when the master page says nothing about a footer.
fn read_odp_footer(root: &XmlNode) -> Option<SlideFooter> {
    let mut footer = SlideFooter::default();
    let mut found = false;
    let mut properties = Vec::new();
    root.find_all("drawing-page-properties", &mut properties);
    for node in properties {
        if node.attr_any_ns("display-footer") == Some("true") {
            footer.show_text = true;
            found = true;
        }
        if node.attr_any_ns("display-page-number") == Some("true") {
            footer.show_slide_number = true;
            found = true;
        }
        if node.attr_any_ns("display-date-time") == Some("true") {
            footer.show_date = true;
            found = true;
        }
    }
    let mut frames = Vec::new();
    root.find_all("frame", &mut frames);
    for frame in frames {
        match frame.attr_any_ns("class") {
            Some("footer") => {
                footer.show_text = true;
                found = true;
                let text = frame.deep_text().trim().to_string();
                if footer.text.is_empty() {
                    footer.text = text;
                }
            }
            Some("page-number") => {
                footer.show_slide_number = true;
                found = true;
            }
            Some("date-time") => {
                footer.show_date = true;
                found = true;
                let text = frame.deep_text().trim().to_string();
                if footer.date_text.is_empty() {
                    footer.date_text = text;
                }
            }
            _ => {}
        }
    }
    found.then(|| {
        footer.enabled = true;
        footer
    })
}

pub fn read_odp(bytes: &[u8]) -> OfficeResult<DeckRead> {
    let reader = ZipReader::open(bytes.to_vec())?;
    if !reader.contains("content.xml") {
        return Err(OfficeError::corrupt("The package does not contain content.xml."));
    }
    let text = reader.read_text("content.xml")?;
    // Trailing text after an inline element is wrapped, so run order survives
    // the parse exactly like the ODT reader's parts.
    let root = parse_xml(&wrap_trailing_text(&text))?;
    let mut deck = Deck::new_blank("Imported presentation");
    deck.slides.clear();
    let mut warnings = Vec::new();
    // Slide size and master page (footer flags) from styles.xml when available.
    let styles_root =
        reader.read_text("styles.xml").ok().and_then(|styles| parse_xml(&wrap_trailing_text(&styles)).ok());
    if let Some(styles_root) = &styles_root {
        let mut layouts = Vec::new();
        styles_root.find_all("page-layout-properties", &mut layouts);
        if let Some(properties) = layouts.first() {
            if let Some(width) = properties.attr_any_ns("page-width").and_then(parse_cm) {
                deck.size.width_pt = width;
            }
            if let Some(height) = properties.attr_any_ns("page-height").and_then(parse_cm) {
                deck.size.height_pt = height;
            }
        }
        if let Some(footer) = read_odp_footer(styles_root) {
            deck.footer = Some(footer);
        }
    }
    let mut import = OdpImport::default();
    collect_styles(&root, &mut import.styles);
    let mut roots = vec![&root];
    if let Some(styles_root) = &styles_root {
        collect_styles(styles_root, &mut import.styles);
        roots.push(styles_root);
    }
    import.clips = odp_graphic_clips(&roots);
    let hidden_pages = odp_hidden_page_styles(&root);
    let mut pages = Vec::new();
    root.find_all("page", &mut pages);
    for page in pages {
        let objects = read_odp_shapes(page, &reader, &mut import, &mut warnings);
        let animations = read_odp_timing(page, &objects, &mut warnings);
        let hidden = page.attr_any_ns("visibility") == Some("hidden")
            || page.attr_any_ns("style-name").map(|name| hidden_pages.contains(name)).unwrap_or(false);
        let mut slide = Slide { objects, animations, hidden, ..Default::default() };
        let mut notes = Vec::new();
        page.find_all("notes", &mut notes);
        if let Some(notes) = notes.first() {
            slide.notes = notes.deep_text().trim().to_string();
        }
        deck.slides.push(slide);
    }
    if deck.slides.is_empty() {
        deck.slides.push(Slide::default());
    }
    warnings.sort();
    warnings.dedup();
    Ok(DeckRead { deck, warnings })
}

pub fn read_odp_file(path: &Path) -> OfficeResult<DeckRead> {
    let bytes = crate::io::read_bytes(path)?;
    let mut result = read_odp(&bytes)?;
    result.deck.title = crate::io::file_stem(path);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_document() -> TextDocument {
        let mut document = TextDocument::new_blank("ODT sample");
        document.blocks = vec![
            Block::heading("Başlık", 1),
            Block::Paragraph {
                props: ParaProps::default(),
                runs: vec![
                    Run { text: "düz ".into(), ..Default::default() },
                    Run { text: "kalın".into(), bold: true, ..Default::default() },
                ],
            },
            Block::Paragraph {
                props: ParaProps {
                    list: Some(ListInfo { kind: "bullet".into(), level: 0, start: 1, marker: "•".into() }),
                    ..Default::default()
                },
                runs: vec![Run { text: "madde".into(), ..Default::default() }],
            },
            Block::Table { table: TableData::simple(2, 2, 400.0) },
        ];
        document.footer = vec![Block::paragraph("Alt bilgi")];
        document
    }

    fn sample_png() -> Vec<u8> {
        let mut buffer = image::RgbaImage::new(4, 4);
        for pixel in buffer.pixels_mut() {
            *pixel = image::Rgba([200, 30, 60, 255]);
        }
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(buffer).write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    fn package_with(content: &str, styles: Option<&str>) -> Vec<u8> {
        let mut zip = ZipWriter::new();
        zip.add_text("mimetype", "application/vnd.oasis.opendocument.text");
        zip.add_text("content.xml", content);
        if let Some(styles) = styles {
            zip.add_text("styles.xml", styles);
        }
        zip.finish()
    }

    fn first_table(document: &TextDocument) -> &TableData {
        document
            .blocks
            .iter()
            .find_map(|block| match block {
                Block::Table { table } => Some(table),
                _ => None,
            })
            .expect("the document must contain a table")
    }

    #[test]
    fn odt_roundtrip() {
        let bytes = write_odt(&sample_document()).unwrap();
        let read = read_odt(&bytes).unwrap();
        let text = read.document.plain_text();
        assert!(text.contains("Başlık"), "text was {text}");
        assert!(text.contains("kalın"));
        assert!(text.contains("madde"));
        assert!(read.document.footer.iter().map(Block::plain_text).any(|text| text.contains("Alt bilgi")));
    }

    #[test]
    fn odt_mimetype_first_and_stored() {
        let bytes = write_odt(&sample_document()).unwrap();
        assert_eq!(&bytes[30..38], b"mimetype");
        assert_eq!(u16::from_le_bytes([bytes[8], bytes[9]]), 0);
    }

    #[test]
    fn odt_notes_roundtrip() {
        let mut document = TextDocument::new_blank("Notes");
        document.footnotes = vec![
            Footnote {
                id: "fn-a".into(),
                runs: vec![Run { text: "Birinci not".into(), bold: true, ..Default::default() }],
                marker: String::new(),
            },
            Footnote {
                id: "fn-b".into(),
                runs: vec![Run { text: "Tablo notu".into(), ..Default::default() }],
                marker: String::new(),
            },
        ];
        document.endnotes = vec![Footnote {
            id: "en-a".into(),
            runs: vec![Run { text: "Son not".into(), ..Default::default() }],
            marker: String::new(),
        }];
        let mut table = TableData::simple(1, 1, 300.0);
        table.rows[0].cells[0].blocks = vec![Block::Paragraph {
            props: ParaProps::default(),
            runs: vec![
                Run { text: "hucre".into(), ..Default::default() },
                Run { footnote: Some("fn-b".into()), ..Default::default() },
            ],
        }];
        document.blocks = vec![
            Block::Paragraph {
                props: ParaProps::default(),
                runs: vec![
                    Run { text: "govde".into(), ..Default::default() },
                    Run { footnote: Some("fn-a".into()), ..Default::default() },
                    Run { text: " son".into(), ..Default::default() },
                    Run { endnote: Some("en-a".into()), ..Default::default() },
                ],
            },
            Block::Table { table },
        ];

        let bytes = write_odt(&document).unwrap();
        let reader = ZipReader::open(bytes.clone()).unwrap();
        let content = reader.read_text("content.xml").unwrap();
        assert!(content.contains("text:note-class=\"footnote\""), "content: {content}");
        assert!(content.contains("text:note-class=\"endnote\""));
        assert!(content.contains("Birinci not"));
        assert!(content.contains("Tablo notu"));
        assert!(content.contains("Son not"));

        let read = read_odt(&bytes).unwrap();
        assert_eq!(read.document.footnotes.len(), 2, "warnings: {:?}", read.warnings);
        assert_eq!(read.document.endnotes.len(), 1);
        let order = read.document.footnote_order();
        assert_eq!(order.len(), 2);
        assert_eq!(read.document.footnote_number(&order[0]), Some(1));
        assert_eq!(read.document.footnote_number(&order[1]), Some(2));
        let first = read.document.footnotes.iter().find(|note| note.id == order[0]).unwrap();
        assert!(first.runs.iter().any(|run| run.text.contains("Birinci not")));
        let second = read.document.footnotes.iter().find(|note| note.id == order[1]).unwrap();
        assert!(second.runs.iter().any(|run| run.text.contains("Tablo notu")));
        assert!(read.document.endnotes[0].runs.iter().any(|run| run.text.contains("Son not")));

        // Note bodies must not be folded into the paragraph text.
        let body = read.document.blocks.iter().map(Block::plain_text).collect::<Vec<_>>().join("\n");
        assert!(!body.contains("Birinci not"), "note body leaked: {body}");
        assert!(!body.contains("Tablo notu"), "table note body leaked: {body}");
        assert!(body.contains("govde"));
        assert!(body.contains("hucre"));

        let paragraph = match &read.document.blocks[0] {
            Block::Paragraph { runs, .. } => runs,
            _ => panic!("first block should be a paragraph"),
        };
        assert!(paragraph.iter().any(|run| run.footnote.is_some()));
        assert!(paragraph.iter().any(|run| run.endnote.is_some()));
        let cell = match &read.document.blocks[1] {
            Block::Table { table } => &table.rows[0].cells[0],
            _ => panic!("second block should be a table"),
        };
        let cell_ref = cell.blocks.iter().find_map(|block| match block {
            Block::Paragraph { runs, .. } => runs.iter().find(|run| run.footnote.is_some()),
            _ => None,
        });
        assert!(cell_ref.is_some(), "the table cell footnote reference was lost");
    }

    #[test]
    fn odt_table_spans_roundtrip() {
        let mut document = TextDocument::new_blank("Spans");
        let mut table = TableData::simple(3, 3, 450.0);
        table.rows[0].cells = vec![
            TableCell { blocks: vec![Block::paragraph("Span")], colspan: 2, rowspan: 2, ..Default::default() },
            TableCell { blocks: vec![Block::paragraph("B")], ..Default::default() },
        ];
        table.rows[1].cells = vec![TableCell { blocks: vec![Block::paragraph("C")], ..Default::default() }];
        document.blocks = vec![Block::Table { table }];

        let bytes = write_odt(&document).unwrap();
        let content = ZipReader::open(bytes.clone()).unwrap().read_text("content.xml").unwrap();
        assert!(content.contains("table:number-columns-spanned=\"2\""), "content: {content}");
        assert!(content.contains("table:number-rows-spanned=\"2\""), "content: {content}");
        // The two grid positions below the origin are covered placeholders, and
        // the table still has three grid columns even though no row lists three
        // origin cells.
        assert_eq!(content.matches("<table:covered-table-cell/>").count(), 2, "content: {content}");
        assert_eq!(content.matches("<table:table-column ").count(), 3, "content: {content}");

        let read = read_odt(&bytes).unwrap();
        let table = first_table(&read.document);
        assert_eq!(table.rows.len(), 3, "warnings: {:?}", read.warnings);
        assert_eq!(table.rows[0].cells.len(), 2);
        assert_eq!(table.rows[0].cells[0].colspan, 2);
        assert_eq!(table.rows[0].cells[0].rowspan, 2);
        assert_eq!(table.rows[0].cells[0].blocks[0].plain_text(), "Span");
        assert_eq!(table.rows[0].cells[1].blocks[0].plain_text(), "B");
        assert_eq!(table.rows[1].cells.len(), 1);
        assert_eq!(table.rows[1].cells[0].blocks[0].plain_text(), "C");
        assert_eq!(table.rows[2].cells.len(), 3);
        assert_eq!(table.column_widths_pt.len(), 3);
        assert!((table.column_widths_pt[0] - 150.0).abs() < 0.1, "widths: {:?}", table.column_widths_pt);
    }

    #[test]
    fn odt_table_import_absorbs_repeated_covered_cells() {
        let content = concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
            "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" ",
            "xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" office:version=\"1.2\">",
            "<office:body><office:text><table:table>",
            "<table:table-column table:number-columns-repeated=\"3\"/>",
            // A covered cell with no origin at all is malformed; it is skipped
            // instead of panicking and A/B start at the first grid columns.
            "<table:table-row><table:covered-table-cell/>",
            "<table:table-cell office:value-type=\"string\"><text:p>A</text:p></table:table-cell>",
            "<table:table-cell office:value-type=\"string\"><text:p>B</text:p></table:table-cell></table:table-row>",
            "<table:table-row>",
            "<table:table-cell office:value-type=\"string\" table:number-columns-spanned=\"2\" ",
            "table:number-rows-spanned=\"2\"><text:p>Span</text:p></table:table-cell>",
            "<table:table-cell office:value-type=\"string\"><text:p>C</text:p></table:table-cell>",
            "</table:table-row>",
            // The repeated covered cells absorb into the declared rowspan,
            // which stays 2 because number-rows-spanned already counts them.
            "<table:table-row><table:covered-table-cell table:number-columns-repeated=\"2\"/>",
            "<table:table-cell office:value-type=\"string\"><text:p>D</text:p></table:table-cell></table:table-row>",
            "<table:table-row><table:table-cell office:value-type=\"string\"><text:p>E</text:p></table:table-cell>",
            "<table:table-cell office:value-type=\"string\"><text:p>F</text:p></table:table-cell>",
            "<table:table-cell office:value-type=\"string\"><text:p>G</text:p></table:table-cell></table:table-row>",
            "</table:table></office:text></office:body></office:document-content>"
        );
        let read = read_odt(&package_with(content, None)).unwrap();
        let table = first_table(&read.document);
        assert_eq!(table.rows.len(), 4, "warnings: {:?}", read.warnings);
        assert_eq!(table.rows[0].cells.len(), 2);
        assert_eq!(table.rows[0].cells[0].blocks[0].plain_text(), "A");
        assert_eq!(table.rows[0].cells[1].blocks[0].plain_text(), "B");
        assert_eq!(table.rows[1].cells.len(), 2);
        assert_eq!(table.rows[1].cells[0].colspan, 2);
        assert_eq!(table.rows[1].cells[0].rowspan, 2);
        assert_eq!(table.rows[2].cells.len(), 1);
        assert_eq!(table.rows[2].cells[0].blocks[0].plain_text(), "D");
        assert_eq!(table.rows[3].cells.len(), 3);
        assert_eq!(table.column_widths_pt.len(), 3);
    }

    #[test]
    fn odt_image_wrapping_roundtrip() {
        let mut document = TextDocument::new_blank("Wrap");
        document.blocks = vec![
            Block::Image {
                image: ImageData::from_bytes("a.png", &sample_png()),
                width_pt: 120.0,
                height_pt: 90.0,
                align: "right".into(),
                caption: String::new(),
                wrap: "square".into(),
            },
            Block::Image {
                image: ImageData::from_bytes("b.png", &sample_png()),
                width_pt: 100.0,
                height_pt: 80.0,
                align: "left".into(),
                caption: String::new(),
                wrap: "topBottom".into(),
            },
            Block::Image {
                image: ImageData::from_bytes("c.png", &sample_png()),
                width_pt: 80.0,
                height_pt: 60.0,
                align: "center".into(),
                caption: String::new(),
                wrap: "inline".into(),
            },
        ];
        let bytes = write_odt(&document).unwrap();
        let content = ZipReader::open(bytes.clone()).unwrap().read_text("content.xml").unwrap();
        assert!(content.contains("style:family=\"graphic\""), "content: {content}");
        assert!(content.contains("style:wrap=\"parallel\""), "content: {content}");
        assert!(content.contains("style:wrap=\"none\""), "content: {content}");
        // Only the wrapped frames reference a graphic style; the inline one
        // keeps the plain paragraph-anchored frame.
        assert_eq!(content.matches("draw:style-name").count(), 2, "content: {content}");

        let read = read_odt(&bytes).unwrap();
        let images: Vec<(&String, f64, f64, &String)> = read
            .document
            .blocks
            .iter()
            .filter_map(|block| match block {
                Block::Image { image, width_pt, height_pt, wrap, .. } => {
                    Some((&image.name, *width_pt, *height_pt, wrap))
                }
                _ => None,
            })
            .collect();
        assert_eq!(images.len(), 3, "warnings: {:?}", read.warnings);
        assert_eq!(images[0].3, "square");
        assert_eq!(images[1].3, "topBottom");
        assert_eq!(images[2].3, "inline");
        assert!((images[0].1 - 120.0).abs() < 0.1, "widths: {images:?}");
        assert!((images[0].2 - 90.0).abs() < 0.1, "heights: {images:?}");
        assert_eq!(images[0].0, "image1.png");
    }

    #[test]
    fn odt_tab_stops_roundtrip() {
        let mut document = TextDocument::new_blank("Tabs");
        document.blocks = vec![Block::Paragraph {
            props: ParaProps {
                align: "right".into(),
                tabs: vec![
                    TabStop { pos_pt: 72.0, align: "left".into() },
                    TabStop { pos_pt: 144.0, align: "center".into() },
                    TabStop { pos_pt: 216.0, align: "right".into() },
                    TabStop { pos_pt: 288.0, align: "decimal".into() },
                ],
                ..Default::default()
            },
            runs: vec![Run { text: "a\tb".into(), ..Default::default() }],
        }];
        let bytes = write_odt(&document).unwrap();
        let content = ZipReader::open(bytes.clone()).unwrap().read_text("content.xml").unwrap();
        assert!(content.contains("style:position=\"2.540cm\""), "content: {content}");
        assert!(content.contains("style:type=\"char\" style:char=\".\""), "content: {content}");
        assert!(content.contains("style:type=\"center\""), "content: {content}");

        let read = read_odt(&bytes).unwrap();
        let props = match &read.document.blocks[0] {
            Block::Paragraph { props, .. } => props,
            _ => panic!("first block should be a paragraph"),
        };
        assert_eq!(props.align, "right");
        assert_eq!(props.tabs.len(), 4, "tabs: {:?}", props.tabs);
        assert!((props.tabs[0].pos_pt - 72.0).abs() < 0.01, "tabs: {:?}", props.tabs);
        assert!((props.tabs[1].pos_pt - 144.0).abs() < 0.01, "tabs: {:?}", props.tabs);
        assert_eq!(props.tabs[1].align, "center");
        assert_eq!(props.tabs[3].align, "decimal");
    }

    #[test]
    fn odt_named_paragraph_style_and_text_style_import() {
        let styles = concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<office:document-styles xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
            "xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" ",
            "xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" office:version=\"1.2\">",
            "<office:styles><style:style style:name=\"Fancy\" style:family=\"paragraph\">",
            "<style:paragraph-properties fo:text-align=\"center\" fo:margin-left=\"1.27cm\" ",
            "fo:break-before=\"page\"><style:tab-stops>",
            "<style:tab-stop style:position=\"2.54cm\" style:type=\"left\"/>",
            "</style:tab-stops></style:paragraph-properties></style:style>",
            "<style:style style:name=\"Bold\" style:family=\"text\">",
            "<style:text-properties fo:font-weight=\"bold\" fo:color=\"#ff0000\"/>",
            "</style:style></office:styles></office:document-styles>"
        );
        let content = concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
            "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" office:version=\"1.2\">",
            "<office:body><office:text><text:p text:style-name=\"Fancy\">",
            "plain <text:span text:style-name=\"Bold\">kalın</text:span></text:p>",
            "</office:text></office:body></office:document-content>"
        );
        let read = read_odt(&package_with(content, Some(styles))).unwrap();
        let props = match &read.document.blocks[0] {
            Block::Paragraph { props, runs } => {
                let bold = runs.iter().find(|run| run.text == "kalın").expect("the span run");
                assert!(bold.bold, "the named text style must resolve bold: {runs:?}");
                assert_eq!(bold.color.as_deref(), Some("#FF0000"));
                props
            }
            _ => panic!("first block should be a paragraph"),
        };
        assert_eq!(props.align, "center");
        assert!(props.page_break_before);
        assert!((props.indent_left_pt - 36.0).abs() < 0.1, "indent: {}", props.indent_left_pt);
        assert_eq!(props.tabs.len(), 1);
        assert!((props.tabs[0].pos_pt - 72.0).abs() < 0.01);
    }

    #[test]
    fn odt_watermark_roundtrip() {
        let mut document = TextDocument::new_blank("WM");
        document.blocks = vec![Block::paragraph("Body")];
        document.watermark = Some(Watermark {
            text: "GİZLİ <taslak> & \"x\"".into(),
            color: Some("#FF0000".into()),
            opacity: 0.25,
            rotation: -45.0,
            font_pt: 48.0,
            bold: false,
        });
        let bytes = write_odt(&document).unwrap();
        let reader = ZipReader::open(bytes.clone()).unwrap();
        let styles = reader.read_text("styles.xml").unwrap();
        assert!(styles.contains("style:wrap=\"run-through\" style:run-through=\"background\""), "styles: {styles}");
        assert!(styles.contains("draw:opacity=\"0.25\""), "styles: {styles}");
        assert!(styles.contains("draw:transform=\"rotate(-45)\""), "styles: {styles}");
        assert!(styles.contains("fo:color=\"#FF0000\""), "styles: {styles}");
        assert!(styles.contains("<style:header>"), "styles: {styles}");
        let meta = reader.read_text("meta.xml").unwrap();
        assert!(
            meta.contains("meta:name=\"OSAK:Watermark:Text\" meta:value=\"GİZLİ &lt;taslak&gt; &amp; &quot;x&quot;\""),
            "meta: {meta}"
        );
        assert!(meta.contains("OSAK:Watermark:Opacity"), "meta: {meta}");
        assert!(meta.contains("OSAK:Watermark:Bold"), "meta: {meta}");

        let read = read_odt(&bytes).unwrap();
        assert_eq!(read.document.watermark, document.watermark, "warnings: {:?}", read.warnings);
        // The visible frame is not imported as header content.
        assert!(read.document.header.is_empty(), "header: {:?}", read.document.header);
        assert!(read.document.plain_text().contains("Body"));
        assert!(!read.document.plain_text().contains("GİZLİ"));
    }

    #[test]
    fn odt_watermark_color_none_roundtrips_and_defaults_visibly() {
        let mut document = TextDocument::new_blank("WM");
        document.watermark = Some(Watermark { color: None, ..Watermark::default() });
        let bytes = write_odt(&document).unwrap();
        let styles = ZipReader::open(bytes.clone()).unwrap().read_text("styles.xml").unwrap();
        // The frame still needs a visible color even when the model has none.
        assert!(styles.contains("fo:color=\"#C0C0C0\""), "styles: {styles}");
        let read = read_odt(&bytes).unwrap();
        let watermark = read.document.watermark.expect("watermark missing");
        assert_eq!(watermark.color, None);
        assert_eq!(watermark.text, "TASLAK");
        assert!(watermark.bold);
    }

    #[test]
    fn odt_watermark_falls_back_to_a_run_through_frame() {
        let styles = concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<office:document-styles xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
            "xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" ",
            "xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" ",
            "xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" ",
            "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" office:version=\"1.2\">",
            "<office:automatic-styles>",
            "<style:style style:name=\"WMG\" style:family=\"graphic\"><style:graphic-properties ",
            "style:wrap=\"run-through\" style:run-through=\"background\" draw:opacity=\"0.3\"/></style:style>",
            "<style:style style:name=\"WMT\" style:family=\"text\"><style:text-properties ",
            "fo:color=\"#FF0000\" fo:font-size=\"36pt\" fo:font-weight=\"bold\"/></style:style>",
            "</office:automatic-styles><office:master-styles>",
            "<style:master-page style:name=\"Standard\"><style:header>",
            "<draw:frame draw:name=\"Watermark\" draw:style-name=\"WMG\" draw:transform=\"rotate(-45)\">",
            "<draw:text-box><text:p><text:span text:style-name=\"WMT\">GIZLI</text:span></text:p>",
            "</draw:text-box></draw:frame></style:header></style:master-page></office:master-styles>",
            "</office:document-styles>"
        );
        let content = concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
            "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" office:version=\"1.2\">",
            "<office:body><office:text><text:p>Body</text:p></office:text></office:body>",
            "</office:document-content>"
        );
        let read = read_odt(&package_with(content, Some(styles))).unwrap();
        let watermark = read.document.watermark.expect("watermark missing");
        assert_eq!(watermark.text, "GIZLI");
        assert_eq!(watermark.color.as_deref(), Some("#FF0000"));
        assert!((watermark.opacity - 0.3).abs() < 1e-9);
        assert!((watermark.rotation + 45.0).abs() < 1e-9);
        assert!((watermark.font_pt - 36.0).abs() < 1e-9);
        assert!(watermark.bold);
        assert!(read.document.header.is_empty(), "header: {:?}", read.document.header);
    }

    #[test]
    fn odt_without_a_watermark_has_none() {
        let read = read_odt(&write_odt(&sample_document()).unwrap()).unwrap();
        assert!(read.document.watermark.is_none());
    }

    #[test]
    fn odt_hand_written_note_fixture_parses() {
        let content = concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
            "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" office:version=\"1.2\">",
            "<office:body><office:text><text:p>Merhaba",
            "<text:note text:id=\"ftn1\" text:note-class=\"footnote\"><text:note-citation>1</text:note-citation>",
            "<text:note-body><text:p>El yazisi dipnot</text:p></text:note-body></text:note>",
            " dunya<text:note text:id=\"edn1\" text:note-class=\"endnote\"><text:note-citation>i</text:note-citation>",
            "<text:note-body><text:p>El yazisi sonnot</text:p></text:note-body></text:note>",
            "</text:p></office:text></office:body></office:document-content>"
        );
        let mut zip = ZipWriter::new();
        zip.add_text("mimetype", "application/vnd.oasis.opendocument.text");
        zip.add_text("content.xml", content);
        let read = read_odt(&zip.finish()).unwrap();
        assert_eq!(read.document.footnotes.len(), 1);
        assert_eq!(read.document.endnotes.len(), 1);
        assert_eq!(read.document.footnotes[0].id, "ftn1");
        assert_eq!(read.document.endnotes[0].id, "edn1");
        assert!(read.document.footnotes[0].runs.iter().any(|run| run.text == "El yazisi dipnot"));
        assert!(read.document.endnotes[0].runs.iter().any(|run| run.text == "El yazisi sonnot"));
        let paragraph = match &read.document.blocks[0] {
            Block::Paragraph { runs, .. } => runs,
            _ => panic!("first block should be a paragraph"),
        };
        let text: String = paragraph.iter().map(|run| run.text.as_str()).collect();
        assert_eq!(text, "Merhaba dunya", "note bodies leaked into the paragraph: {text}");
        assert!(paragraph.iter().any(|run| run.footnote.as_deref() == Some("ftn1")));
        assert!(paragraph.iter().any(|run| run.endnote.as_deref() == Some("edn1")));
        // A citation that is not the automatic number is preserved as a marker.
        assert_eq!(read.document.footnotes[0].marker, "");
        assert_eq!(read.document.endnotes[0].marker, "i");
    }

    #[test]
    fn ods_roundtrip_with_formulas() {
        let mut workbook = Workbook::new_blank("ODS");
        let sheet = &mut workbook.sheets[0];
        sheet.set("A1", Cell { value: CellValue::Number(10.0), ..Default::default() });
        sheet.set("A2", Cell { value: CellValue::Number(20.0), ..Default::default() });
        sheet.set(
            "A3",
            Cell { value: CellValue::Number(30.0), formula: Some("=SUM(A1:A2)".into()), ..Default::default() },
        );
        sheet.set("B1", Cell { value: CellValue::Text("metin".into()), ..Default::default() });
        let bytes = write_ods(&workbook).unwrap();
        let read = read_ods(&bytes).unwrap();
        let sheet = &read.workbook.sheets[0];
        assert_eq!(sheet.get("A1").map(|cell| cell.value.clone()), Some(CellValue::Number(10.0)));
        assert_eq!(sheet.get("B1").map(|cell| cell.value.clone()), Some(CellValue::Text("metin".into())));
        assert_eq!(sheet.get("A3").and_then(|cell| cell.formula.clone()).as_deref(), Some("=SUM(A1:A2)"));
    }

    #[test]
    fn ods_compresses_empty_cells() {
        let mut workbook = Workbook::new_blank("Big");
        workbook.sheets[0].set("A1", Cell { value: CellValue::Text("x".into()), ..Default::default() });
        workbook.sheets[0].set("T500", Cell { value: CellValue::Number(1.0), ..Default::default() });
        let bytes = write_ods(&workbook).unwrap();
        assert!(bytes.len() < 20_000, "package was {} bytes", bytes.len());
    }

    #[test]
    fn odp_roundtrip() {
        let mut deck = Deck::new_blank("ODP");
        let mut slide = Slide::default();
        let mut text = SlideObject::new("text", 60.0, 60.0, 400.0, 120.0);
        text.text = Some(TextFrame {
            paragraphs: vec![TextParagraph { text: "Slayt metni".into(), ..Default::default() }],
            ..Default::default()
        });
        let mut rect = SlideObject::new("rect", 40.0, 240.0, 200.0, 80.0);
        rect.style = Some(ShapeStyle { fill: Some("#1D4ED8".into()), ..Default::default() });
        slide.objects = vec![text, rect];
        slide.notes = "konuşmacı notu".into();
        deck.slides = vec![slide];
        let bytes = write_odp(&deck).unwrap();
        let read = read_odp(&bytes).unwrap();
        assert_eq!(read.deck.slides.len(), 1);
        let texts: Vec<String> = read.deck.slides[0]
            .objects
            .iter()
            .filter_map(|object| object.text.as_ref().map(TextFrame::plain))
            .collect();
        assert!(texts.iter().any(|text| text.contains("Slayt metni")));
        assert!(read.deck.slides[0].notes.contains("konuşmacı"));
    }

    #[test]
    fn odp_rich_text_runs_bullets_and_alignment_round_trip() {
        let mut deck = Deck::new_blank("Rich");
        let mut text = SlideObject::new("text", 60.0, 60.0, 500.0, 300.0);
        text.text = Some(TextFrame {
            paragraphs: vec![
                TextParagraph {
                    text: "Center".into(),
                    align: "center".into(),
                    runs: vec![Run { text: "Center".into(), ..Default::default() }],
                    ..Default::default()
                },
                TextParagraph {
                    text: "Bold red 24".into(),
                    runs: vec![
                        Run { text: "Bold".into(), bold: true, ..Default::default() },
                        Run { text: " red ".into(), color: Some("#FF0000".into()), ..Default::default() },
                        Run { text: "24".into(), size_pt: Some(24.0), ..Default::default() },
                    ],
                    ..Default::default()
                },
                TextParagraph { text: "Top".into(), bullet: true, ..Default::default() },
                TextParagraph { text: "Nested".into(), bullet: true, level: 1, ..Default::default() },
            ],
            ..Default::default()
        });
        deck.slides = vec![Slide { objects: vec![text], ..Default::default() }];

        let bytes = write_odp(&deck).unwrap();
        let content = ZipReader::open(bytes.clone()).unwrap().read_text("content.xml").unwrap();
        assert!(content.contains("<text:span "), "content: {content}");
        assert!(content.contains("<text:list text:style-name=\"LB\">"), "content: {content}");
        assert_eq!(content.matches("<text:list-item>").count(), 2, "content: {content}");
        assert!(content.contains("fo:text-align=\"center\""), "content: {content}");
        assert!(content.contains("<text:list-style style:name=\"LB\">"), "content: {content}");

        let read = read_odp(&bytes).unwrap();
        let frame = read.deck.slides[0].objects[0].text.clone().expect("text frame");
        let paragraphs = &frame.paragraphs;
        assert_eq!(paragraphs.len(), 4, "warnings: {:?}", read.warnings);
        assert_eq!(paragraphs[0].text, "Center");
        assert_eq!(paragraphs[0].align, "center");
        assert_eq!(paragraphs[1].text, "Bold red 24");
        let bold = paragraphs[1].runs.iter().find(|run| run.text == "Bold").expect("bold run");
        assert!(bold.bold);
        let red = paragraphs[1].runs.iter().find(|run| run.text.contains("red")).expect("red run");
        assert_eq!(red.color.as_deref(), Some("#FF0000"));
        let sized = paragraphs[1].runs.iter().find(|run| run.text == "24").expect("sized run");
        assert_eq!(sized.size_pt, Some(24.0));
        assert!(paragraphs[2].bullet && paragraphs[2].level == 0, "{:?}", paragraphs[2]);
        assert!(paragraphs[3].bullet && paragraphs[3].level == 1, "{:?}", paragraphs[3]);

        // A second cycle is stable.
        let again = read_odp(&write_odp(&read.deck).unwrap()).unwrap();
        assert_eq!(again.deck.slides[0].objects[0].text, read.deck.slides[0].objects[0].text);
    }

    #[test]
    fn odp_hidden_slides_round_trip() {
        let mut deck = Deck::new_blank("Hidden");
        deck.slides = vec![Slide::default(), Slide { hidden: true, ..Default::default() }];
        let bytes = write_odp(&deck).unwrap();
        let content = ZipReader::open(bytes.clone()).unwrap().read_text("content.xml").unwrap();
        assert!(content.contains("presentation:visibility=\"hidden\""), "content: {content}");
        assert_eq!(
            content.matches("draw:style-name=\"dp-hidden\"").count(),
            1,
            "only the hidden slide references the style: {content}"
        );

        let read = read_odp(&bytes).unwrap();
        assert!(!read.deck.slides[0].hidden);
        assert!(read.deck.slides[1].hidden);

        // A deck without hidden slides does not declare the style at all.
        let visible = write_odp(&Deck::new_blank("Visible")).unwrap();
        let content = ZipReader::open(visible).unwrap().read_text("content.xml").unwrap();
        assert!(!content.contains("dp-hidden"), "content: {content}");
    }

    fn odp_chart_deck() -> Deck {
        let mut column = SlideObject::new("chart", 60.0, 60.0, 480.0, 288.0);
        column.id = "chart-column".into();
        column.chart = Some(ChartData {
            kind: "column".into(),
            title: "Sales".into(),
            categories: "A2:A4".into(),
            series: vec![ChartSeries { name: "North".into(), range: "B2:B4".into(), color: Some("#1D4ED8".into()) }],
            legend: true,
            x_title: "Quarter".into(),
            y_title: "Units".into(),
            show_labels: true,
            categories_cache: vec!["Q1".into(), "Q2".into(), "Q3".into()],
            series_values_cache: vec![vec![10.0, 20.5, 31.0]],
            ..Default::default()
        });
        let mut pie = SlideObject::new("chart", 60.0, 380.0, 320.0, 240.0);
        pie.id = "chart-pie".into();
        pie.chart = Some(ChartData {
            kind: "pie".into(),
            title: "Share".into(),
            categories: "A2:A4".into(),
            series: vec![ChartSeries { name: "North".into(), range: "B2:B4".into(), color: None }],
            categories_cache: vec!["Q1".into(), "Q2".into(), "Q3".into()],
            series_values_cache: vec![vec![10.0, 20.5, 31.0]],
            ..Default::default()
        });
        let mut deck = Deck::new_blank("Charts");
        deck.slides = vec![Slide { objects: vec![column, pie], ..Default::default() }];
        deck
    }

    #[test]
    fn odp_charts_are_written_as_objects_and_round_trip() {
        let deck = odp_chart_deck();
        let write = write_odp_package(&deck).unwrap();
        assert!(write.warnings.is_empty(), "supported charts need no warning: {:?}", write.warnings);
        let reader = ZipReader::open(write.bytes.clone()).unwrap();
        let manifest = reader.read_text("META-INF/manifest.xml").unwrap();
        assert!(manifest.contains(
            "manifest:full-path=\"Object 1/\" manifest:version=\"1.2\" manifest:media-type=\"application/vnd.oasis.opendocument.chart\""
        ), "manifest: {manifest}");
        for object in ["Object 1", "Object 2"] {
            for part in ["content.xml", "styles.xml"] {
                let path = format!("{object}/{part}");
                assert!(manifest.contains(&format!("manifest:full-path=\"{path}\"")), "{path} is missing");
                assert!(reader.contains(&path), "{path} is present");
            }
        }
        let content = reader.read_text("content.xml").unwrap();
        assert!(content.contains("xlink:href=\"./Object 1\""));
        assert!(content.contains("xlink:href=\"./Object 2\""));
        let chart = reader.read_text("Object 1/content.xml").unwrap();
        assert!(chart.contains("chart:class=\"chart:bar\""));
        assert!(chart.contains("<table:table table:name=\"local-table\">"));
        assert!(reader.read_text("Object 2/content.xml").unwrap().contains("chart:class=\"chart:circle\""));

        let read = read_odp(&write.bytes).unwrap();
        let objects = &read.deck.slides[0].objects;
        let column = objects.iter().find(|object| object.id == "chart-column").expect("column chart");
        assert_eq!(column.kind, "chart");
        assert_eq!(column.chart.as_ref(), deck.slides[0].objects[0].chart.as_ref());
        let pie = objects.iter().find(|object| object.id == "chart-pie").expect("pie chart");
        assert_eq!(pie.chart.as_ref(), deck.slides[0].objects[1].chart.as_ref());

        // A second cycle keeps the charts unchanged.
        let again = read_odp(&write_odp(&read.deck).unwrap()).unwrap();
        let charts: Vec<&ChartData> =
            again.deck.slides[0].objects.iter().filter_map(|object| object.chart.as_ref()).collect();
        assert_eq!(charts, objects.iter().filter_map(|object| object.chart.as_ref()).collect::<Vec<_>>());
    }

    #[test]
    fn odp_unsupported_charts_warn_and_keep_a_placeholder() {
        let mut deck = odp_chart_deck();
        deck.slides[0].objects[1].chart.as_mut().unwrap().kind = "radar".into();
        let write = write_odp_package(&deck).unwrap();
        assert!(
            write
                .warnings
                .iter()
                .any(|warning| warning.contains("Chart data is kept in the native .oswk file")
                    && warning.contains("radar")),
            "warnings: {:?}",
            write.warnings
        );
        let content = ZipReader::open(write.bytes.clone()).unwrap().read_text("content.xml").unwrap();
        assert!(!content.contains("xlink:href=\"./Object 2\""));
        assert!(content.contains("Share"), "the unsupported chart keeps its title placeholder: {content}");
        // The placeholder comes back as a text shape; the chart object itself
        // stays in the native .oswk file, which is what the warning says.
        let read = read_odp(&write.bytes).unwrap();
        let placeholder = read.deck.slides[0]
            .objects
            .iter()
            .find(|object| object.text.as_ref().map(TextFrame::plain).as_deref() == Some("Share"))
            .expect("chart placeholder");
        assert!(placeholder.chart.is_none());
    }

    #[test]
    fn odp_connectors_round_trip_with_glue_points() {
        let mut deck = Deck::new_blank("Connectors");
        let mut source = SlideObject::new("rect", 100.0, 100.0, 120.0, 80.0);
        source.id = "from-rect".into();
        source.z = 1;
        let mut connector = SlideObject::new("line", 220.0, 140.0, 180.0, 160.0);
        connector.id = "conn".into();
        connector.z = 2;
        connector.line = Some(LineSpec {
            x2: 180.0,
            y2: 160.0,
            begin_object: Some("from-rect".into()),
            end_object: Some("to-rect".into()),
            begin_site: 3,
            end_site: 1,
            ..Default::default()
        });
        let mut target = SlideObject::new("rect", 400.0, 300.0, 120.0, 80.0);
        target.id = "to-rect".into();
        target.z = 3;
        deck.slides = vec![Slide { objects: vec![source, connector, target], ..Default::default() }];

        let bytes = write_odp(&deck).unwrap();
        let content = ZipReader::open(bytes.clone()).unwrap().read_text("content.xml").unwrap();
        assert!(content.contains("<draw:connector "), "content: {content}");
        assert!(content.contains("draw:start-shape=\"from-rect\" draw:start-glue-point=\"3\""), "content: {content}");
        assert!(content.contains("draw:end-shape=\"to-rect\" draw:end-glue-point=\"1\""), "content: {content}");

        let read = read_odp(&bytes).unwrap();
        let connector = read.deck.slides[0].objects.iter().find(|object| object.id == "conn").expect("connector");
        assert_eq!(connector.kind, "line");
        let line = connector.line.clone().expect("line spec");
        assert_eq!(line.begin_object.as_deref(), Some("from-rect"));
        assert_eq!(line.end_object.as_deref(), Some("to-rect"));
        assert_eq!(line.begin_site, 3);
        assert_eq!(line.end_site, 1);
        assert!((connector.x - 220.0).abs() < 0.05 && (connector.y - 140.0).abs() < 0.05, "{connector:?}");
        assert!((line.x2 - 180.0).abs() < 0.05 && (line.y2 - 160.0).abs() < 0.05, "{line:?}");
    }

    #[test]
    fn odp_image_crop_round_trips_within_tolerance() {
        let mut deck = Deck::new_blank("Crop");
        let mut object = SlideObject::new("image", 60.0, 60.0, 400.0, 300.0);
        let mut image = ImageData::from_bytes("crop.png", &sample_png());
        image.crop = Some(ImageCrop { left: 0.1, top: 0.2, right: 0.05, bottom: 0.15 });
        object.image = Some(image);
        deck.slides = vec![Slide { objects: vec![object], ..Default::default() }];

        let bytes = write_odp(&deck).unwrap();
        let content = ZipReader::open(bytes.clone()).unwrap().read_text("content.xml").unwrap();
        assert!(content.contains("fo:clip=\"rect("), "content: {content}");

        let read = read_odp(&bytes).unwrap();
        let crop = read.deck.slides[0].objects[0].image.as_ref().unwrap().crop.clone().expect("crop");
        assert!((crop.left - 0.1).abs() < 0.01, "{crop:?}");
        assert!((crop.top - 0.2).abs() < 0.01, "{crop:?}");
        assert!((crop.right - 0.05).abs() < 0.01, "{crop:?}");
        assert!((crop.bottom - 0.15).abs() < 0.01, "{crop:?}");

        // An uncropped image writes no clip at all.
        let mut plain = deck.clone();
        plain.slides[0].objects[0].image.as_mut().unwrap().crop = None;
        let content = ZipReader::open(write_odp(&plain).unwrap()).unwrap().read_text("content.xml").unwrap();
        assert!(!content.contains("fo:clip"), "content: {content}");
    }

    #[test]
    fn odp_footer_slide_number_and_date_round_trip() {
        let mut deck = Deck::new_blank("Footer");
        deck.footer = Some(SlideFooter {
            enabled: true,
            text: "Alt bilgi".into(),
            show_text: true,
            show_slide_number: true,
            show_date: true,
            date_text: "2026-10-10".into(),
        });
        let bytes = write_odp(&deck).unwrap();
        let styles = ZipReader::open(bytes.clone()).unwrap().read_text("styles.xml").unwrap();
        assert!(styles.contains("presentation:display-footer=\"true\""), "styles: {styles}");
        assert!(styles.contains("presentation:display-page-number=\"true\""), "styles: {styles}");
        assert!(styles.contains("presentation:display-date-time=\"true\""), "styles: {styles}");
        assert!(styles.contains("presentation:class=\"footer\""), "styles: {styles}");
        assert!(styles.contains("presentation:class=\"page-number\""), "styles: {styles}");
        assert!(styles.contains("<text:page-number>"), "styles: {styles}");
        assert!(styles.contains("2026-10-10"), "styles: {styles}");

        let read = read_odp(&bytes).unwrap();
        let footer = read.deck.footer.clone().expect("footer");
        assert!(footer.enabled && footer.show_text && footer.show_slide_number && footer.show_date);
        assert_eq!(footer.text, "Alt bilgi");
        assert_eq!(footer.date_text, "2026-10-10");

        // A second cycle keeps the footer.
        assert_eq!(read_odp(&write_odp(&read.deck).unwrap()).unwrap().deck.footer, read.deck.footer);

        // A deck without a footer reads back without one.
        let plain = read_odp(&write_odp(&Deck::new_blank("No footer")).unwrap()).unwrap();
        assert!(plain.deck.footer.is_none());
    }

    #[test]
    fn malformed_inputs_are_errors() {
        assert!(read_odt(b"").is_err());
        assert!(read_ods(&[0u8; 16]).is_err());
        assert!(read_odp(&[1u8; 32]).is_err());
    }

    #[test]
    fn formula_conversion_both_ways() {
        assert_eq!(odf_formula_to_ours("of:=SUM([.A1:.A2])"), "=SUM(A1:A2)");
        assert_eq!(our_formula_to_odf("=SUM(A1:B2)+1"), "of:=SUM([.A1:.B2])+1");
        assert_eq!(odf_formula_to_ours("of:=[.A1]*2"), "=A1*2");
    }

    #[test]
    fn condition_formulas_convert_references_and_separators() {
        let ours = "AND($D1>4,MOD($D1,2)=0,Data!$B1<3,'My Sheet'!A1:B2<>\"a,b A1\")";
        let odf = "AND([.$D1]>4;MOD([.$D1];2)=0;[$Data.$B1]<3;[$'My Sheet'.A1:.B2]<>\"a,b A1\")";
        assert_eq!(formula_to_ods(ours), odf);
        assert_eq!(formula_from_ods(odf), ours);
        // Function names with digits and names that end in digits are not references.
        assert_eq!(formula_to_ods("LOG10(Rate1)+SUM(A1:A3)"), "LOG10(Rate1)+SUM([.A1:.A3])");
        assert_eq!(formula_from_ods("LOG10(Rate1)+SUM([.A1:.A3])"), "LOG10(Rate1)+SUM(A1:A3)");
        assert_eq!(formula_to_ods("1.5*A1"), "1.5*[.A1]");
    }

    #[test]
    fn condition_arguments_split_outside_quotes_and_parentheses() {
        assert_eq!(split_ods_args("1,9"), ["1", "9"]);
        assert_eq!(split_ods_args("\"a,b\";MAX(1;2)"), ["\"a,b\"", "MAX(1;2)"]);
        assert_eq!(unquote_ods("\"say \"\"hi\"\"\""), "say \"hi\"");
    }
}

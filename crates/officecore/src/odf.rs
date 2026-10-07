//! OpenDocument Format support: ODT (Writer), ODS (Calc) and ODP (Impress).
//!
//! Written directly as ODF 1.2 packages so LibreOffice, OpenOffice and
//! OnlyOffice open the results. Import is tolerant: unknown constructs are
//! skipped with warnings, and nothing is ever executed from the package.

use crate::error::{OfficeError, OfficeResult};
use crate::model::*;
use crate::xml::{parse_xml, XmlNode, XmlWriter};
use crate::zip::{ZipReader, ZipWriter};
use std::collections::{BTreeSet, HashMap, HashSet};
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
    paragraph_keys: HashMap<String, String>,
    text_keys: HashMap<String, String>,
    cell_keys: HashMap<String, String>,
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
    format!("<style:paragraph-properties{properties}/>")
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
            Block::Image { image, width_pt, height_pt, .. } => {
                let Some(name) = media.add(image) else {
                    index += 1;
                    continue;
                };
                writer.raw(&format!(
                    "<text:p text:style-name=\"{}\"><draw:frame draw:name=\"{}\" text:anchor-type=\"paragraph\" svg:width=\"{}\" svg:height=\"{}\"><draw:image xlink:href=\"Pictures/{}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/></draw:frame></text:p>",
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

fn write_table(
    writer: &mut XmlWriter,
    table: &TableData,
    styles: &mut AutoStyles,
    media: &mut Media,
    notes: &NoteContext,
) {
    let columns = table.rows.iter().map(|row| row.cells.len()).max().unwrap_or(1).max(1);
    writer.raw("<table:table>");
    for index in 0..columns {
        let width = table.column_widths_pt.get(index).copied().unwrap_or(90.0);
        writer.raw(&format!(
            "<table:table-column table:style-name=\"co{}\" style:column-width=\"{}\"/>",
            index,
            cm(width)
        ));
    }
    for row in &table.rows {
        writer.raw("<table:table-row>");
        for cell in &row.cells {
            writer.raw(&format!(
                "<table:table-cell office:value-type=\"string\"{}>",
                if cell.colspan > 1 {
                    format!(" table:number-columns-spanned=\"{}\"", cell.colspan)
                } else {
                    String::new()
                }
            ));
            if cell.blocks.is_empty() {
                writer.raw("<text:p/>");
            } else {
                write_blocks(writer, &cell.blocks, styles, media, notes, 0);
            }
            writer.raw("</table:table-cell>");
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
    let mut header_footer = String::new();
    let mut media = Media::default();
    let notes = NoteContext::for_document(document);
    if !document.header.is_empty() {
        let mut writer = XmlWriter::new();
        let mut styles = AutoStyles::default();
        write_blocks(&mut writer, &document.header, &mut styles, &mut media, &notes, 0);
        header_footer.push_str(&format!("<style:header>{}</style:header>", writer.finish()));
    }
    if !document.footer.is_empty() {
        let mut writer = XmlWriter::new();
        let mut styles = AutoStyles::default();
        write_blocks(&mut writer, &document.footer, &mut styles, &mut media, &notes, 0);
        header_footer.push_str(&format!("<style:footer>{}</style:footer>", writer.finish()));
    }
    format!(
        "<office:master-styles>{layout}<style:master-page style:name=\"Standard\" style:page-layout-name=\"pm1\">{header_footer}</style:master-page></office:master-styles>"
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

fn meta_xml(title: &str, generator: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-meta {NS} office:version=\"1.2\"><office:meta><meta:generator>{}</meta:generator><dc:title>{}</dc:title></office:meta></office:document-meta>",
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
    zip.add_text("meta.xml", &meta_xml(&document.title, "OmniOffice"));
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

fn read_span_style(node: &XmlNode) -> Run {
    let mut run = Run::default();
    let mut properties = Vec::new();
    node.find_all("style:text-properties", &mut properties);
    let Some(properties) = properties.first() else { return run };
    if properties.attr_any_ns("font-weight") == Some("bold") {
        run.bold = true;
    }
    if properties.attr_any_ns("font-style") == Some("italic") {
        run.italic = true;
    }
    if properties.attr_any_ns("font-family").is_some() {
        run.font = properties.attr_any_ns("font-family").map(str::to_string);
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
        } else if position.starts_with("sub") {
            run.subscript = true;
        }
    }
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
fn annotation_paragraph_text(paragraph: &XmlNode) -> String {
    let mut runs = Vec::new();
    let mut scratch = NoteReadState::default();
    for inner in &paragraph.children {
        node_text_runs(inner, &mut runs, &mut scratch);
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
fn read_annotation(node: &XmlNode, runs: &mut Vec<Run>, notes: &mut NoteReadState) {
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
        let text = annotation_paragraph_text(paragraph);
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
fn read_note(node: &XmlNode, runs: &mut Vec<Run>, notes: &mut NoteReadState) {
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
                node_text_runs(inner, &mut body_runs, notes);
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

fn node_text_runs(node: &XmlNode, runs: &mut Vec<Run>, notes: &mut NoteReadState) {
    match node.local_name() {
        "span" => {
            let base = read_span_style(node);
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
                    "note" => read_note(child, runs, notes),
                    "annotation" | "annotation-end" => node_text_runs(child, runs, notes),
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
                node_text_runs(child, &mut inner, notes);
                for mut run in inner {
                    if run.link.is_none() {
                        run.link = link.clone();
                    }
                    runs.push(run);
                }
            }
        }
        "note" => read_note(node, runs, notes),
        // Comments must not reach the catch-all below, which would fold their
        // author, date and text into the paragraph.
        "annotation" => read_annotation(node, runs, notes),
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
                node_text_runs(child, runs, notes);
            }
        }
    }
}

fn paragraph_props(node: &XmlNode, style_map: &HashMap<String, ParaProps>) -> ParaProps {
    let props = if let Some(name) = node.attr("style-name").or_else(|| node.attr_any_ns("style-name")) {
        style_map.get(name).cloned().unwrap_or_default()
    } else {
        ParaProps::default()
    };
    props
}

fn read_blocks(
    node: &XmlNode,
    style_map: &HashMap<String, ParaProps>,
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
                for inner in &child.children {
                    node_text_runs(inner, &mut runs, notes);
                }
                notes.anchor_paragraph(&mut runs);
                let mut props = paragraph_props(child, style_map);
                if child.local_name() == "h" {
                    let level =
                        child.attr_any_ns("outline-level").and_then(|value| value.parse::<u32>().ok()).unwrap_or(1);
                    props.style = format!("Heading{level}");
                }
                blocks.push(Block::Paragraph { props, runs });
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
                                node_text_runs(part, &mut runs, notes);
                            }
                            notes.anchor_paragraph(&mut runs);
                            let mut props = paragraph_props(inner, style_map);
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
                let mut rows = Vec::new();
                for row_node in child.children_named("table-row") {
                    let mut row = TableRow::default();
                    for cell_node in row_node.children_named("table-cell") {
                        let mut cell = TableCell::default();
                        if let Some(span) =
                            cell_node.attr_any_ns("number-columns-spanned").and_then(|value| value.parse::<u32>().ok())
                        {
                            cell.colspan = span.max(1);
                        }
                        let mut nested_warnings = Vec::new();
                        cell.blocks = read_blocks(cell_node, style_map, reader, &mut nested_warnings, notes);
                        if cell.blocks.is_empty() {
                            cell.blocks.push(Block::paragraph(""));
                        }
                        row.cells.push(cell);
                    }
                    if row.cells.is_empty() {
                        continue;
                    }
                    if widths.is_empty() {
                        widths = vec![90.0; row.cells.len()];
                    }
                    rows.push(row);
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
                blocks.extend(read_blocks(child, style_map, reader, warnings, notes));
            }
            // Not valid ODF outside a paragraph: keep the comment, unanchored.
            "annotation" => read_annotation(child, &mut Vec::new(), notes),
            _ => {
                if child.local_name() == "frame" {
                    let mut images = Vec::new();
                    child.find_all("image", &mut images);
                    if let Some(image_node) = images.first() {
                        if let Some(href) = image_node.attr("href") {
                            let width = child.attr("width").and_then(parse_cm).unwrap_or(320.0);
                            let height = child.attr("height").and_then(parse_cm).unwrap_or(200.0);
                            let path = href.trim_start_matches("./");
                            match reader.read(path) {
                                Ok(data) => {
                                    let name = path.rsplit('/').next().unwrap_or("image.png").to_string();
                                    blocks.push(Block::Image {
                                        image: ImageData::from_bytes(&name, &data),
                                        width_pt: width,
                                        height_pt: height,
                                        align: "center".into(),
                                        caption: String::new(),
                                    });
                                }
                                Err(_) => warnings.push("An embedded image could not be read from the package.".into()),
                            }
                        }
                    } else {
                        warnings.push("Text boxes and embedded objects are imported as plain content.".into());
                        let mut inner = Vec::new();
                        for part in &child.children {
                            if part.local_name() == "text-box" {
                                inner.extend(read_blocks(part, style_map, reader, warnings, notes));
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

fn collect_styles(root: &XmlNode, style_map: &mut HashMap<String, ParaProps>) {
    let mut styles = Vec::new();
    root.find_all("style", &mut styles);
    for style in styles {
        let Some(name) = style.attr("name") else { continue };
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
        }
        style_map.insert(name.to_string(), props);
    }
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
    let mut style_map = HashMap::new();
    if let Ok(text) = reader.read_text("styles.xml") {
        if let Ok(root) = parse_xml(&text) {
            collect_styles(&root, &mut style_map);
        }
    }
    let text = reader.read_text("content.xml")?;
    let root = parse_xml(&wrap_trailing_text(&text))?;
    collect_styles(&root, &mut style_map);
    let mut document = TextDocument::new_blank("Imported document");
    let mut note_state = NoteReadState::default();
    note_state.start_comment_part(&root);
    let container = root.child("body").and_then(|body| body.child("text")).unwrap_or(&root);
    document.blocks = read_blocks(container, &style_map, &reader, &mut warnings, &mut note_state);
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
                document.header = read_blocks(header, &style_map, &reader, &mut warnings, &mut note_state);
            }
            let mut footers = Vec::new();
            root.find_all("footer", &mut footers);
            if let Some(footer) = footers.first() {
                document.footer = read_blocks(footer, &style_map, &reader, &mut warnings, &mut note_state);
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
    let trimmed = formula.trim().trim_start_matches("of:=").trim_start_matches("oooc:=").trim_start_matches("msoxl:=");
    let mut out = String::new();
    let chars: Vec<char> = trimmed.chars().collect();
    let mut index = 0usize;
    while index < chars.len() {
        if chars[index] == '[' {
            let mut end = index;
            let mut content = String::new();
            while end + 1 < chars.len() && chars[end + 1] != ']' {
                end += 1;
                content.push(chars[end]);
            }
            index = (end + 1).min(chars.len() - 1);
            let content = content.trim_start_matches('.').replace('.', "");
            out.push_str(&content);
        } else if chars[index] == ';' {
            out.push(',');
        } else {
            out.push(chars[index]);
        }
        index += 1;
    }
    format!("={out}")
}

fn our_formula_to_odf(formula: &str) -> String {
    let trimmed = formula.trim().trim_start_matches('=');
    let mut out = String::new();
    let chars: Vec<char> = trimmed.chars().collect();
    let mut index = 0usize;
    while index < chars.len() {
        let ch = chars[index];
        if ch == ';' {
            out.push(';');
            index += 1;
            continue;
        }
        let is_ref_start = ch.is_ascii_alphabetic()
            && index + 1 < chars.len()
            && chars[index + 1..].iter().take_while(|c| c.is_ascii_alphanumeric() || **c == '$').count() > 0
            && chars[index + 1..]
                .iter()
                .take_while(|c| c.is_ascii_alphanumeric() || **c == '$')
                .any(|c| c.is_ascii_digit());
        if is_ref_start {
            let mut end = index;
            while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '$') {
                end += 1;
            }
            let reference: String = chars[index..end].iter().collect();
            if end < chars.len() && chars[end] == ':' {
                let mut end2 = end + 1;
                while end2 < chars.len() && (chars[end2].is_ascii_alphanumeric() || chars[end2] == '$') {
                    end2 += 1;
                }
                let second: String = chars[end + 1..end2].iter().collect();
                out.push_str(&format!("[.{reference}:.{second}]"));
                index = end2;
            } else {
                out.push_str(&format!("[.{reference}]"));
                index = end;
            }
        } else {
            out.push(ch);
            index += 1;
        }
    }
    format!("of:={out}")
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

pub fn write_ods(workbook: &Workbook) -> OfficeResult<Vec<u8>> {
    let mut styles = AutoStyles::default();
    let mut body = String::new();
    let mut chart_objects: Vec<OdsChartObject> = Vec::new();
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
        for column in 0..=max_col.min(200) {
            let width = sheet.col_widths.get(&column).copied().unwrap_or(90.0);
            body.push_str(&format!(
                "<table:table-column table:style-name=\"co{}\" style:column-width=\"{}\"/>",
                column,
                cm(width * 0.75)
            ));
        }
        // Only rows holding a cell or a chart are written one by one; the gaps
        // between them become repeated empty rows, so a far-away anchor costs
        // one element instead of a row per index.
        let mut occupied: BTreeSet<u32> =
            cells.keys().filter_map(|address| crate::address::parse(address).map(|(row, _)| row)).collect();
        occupied.extend(frames.keys().map(|(row, _)| *row));
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
                let value_xml = match &cell.value {
                    CellValue::Empty => "<text:p/>".to_string(),
                    CellValue::Number(number) => format!("<text:p>{number}</text:p>"),
                    CellValue::Bool(value) => format!("<text:p>{}</text:p>", if *value { "TRUE" } else { "FALSE" }),
                    CellValue::Text(text) => format!("<text:p>{}</text:p>", crate::xml::escape_text(text)),
                    CellValue::Error(error) => format!("<text:p>{}</text:p>", crate::xml::escape_text(error)),
                };
                let value_type = match &cell.value {
                    CellValue::Number(_) => "float",
                    CellValue::Bool(_) => "boolean",
                    _ => "string",
                };
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
                    CellValue::Text(text) => format!(" office:string-value=\"{}\"", escape(text)),
                    _ => String::new(),
                };
                row_output.push_str(&format!("<table:table-cell office:value-type=\"{value_type}\"{value_attr}{attributes}{formula}>{frame}{value_xml}</table:table-cell>"));
                column += 1;
            }
            if empty_run > 0 {
                row_output.push_str(&format!("<table:table-cell table:number-columns-repeated=\"{}\"/>", empty_run));
            }
            body.push_str(&format!("<table:table-row>{row_output}</table:table-row>"));
        }
        body.push_str("</table:table>");
    }
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NS} office:version=\"1.2\">{}{}<office:body><office:spreadsheet>{body}</office:spreadsheet></office:body></office:document-content>",
        styles.xml(),
        "<office:styles/>"
    );
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
        &format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NS} office:version=\"1.2\"/>"),
    );
    zip.add_text("meta.xml", &meta_xml(&workbook.title, "OmniOffice"));
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
        for row_node in table.children_named("table-row") {
            // The repeat only moves the position (a repeated row's cells are
            // set once), so it is taken as written: capping it shifted every
            // cell after a long empty gap up to the wrong row.
            let repeat_rows =
                row_node.attr_any_ns("number-rows-repeated").and_then(|value| value.parse::<u32>().ok()).unwrap_or(1);
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
                        let text = {
                            let mut paragraphs = Vec::new();
                            cell.find_all("p", &mut paragraphs);
                            paragraphs.iter().map(|node| node.deep_text()).collect::<Vec<_>>().join("\n")
                        };
                        if text.is_empty() {
                            CellValue::Empty
                        } else if formula.is_some() && text.starts_with('#') {
                            CellValue::Error(text)
                        } else {
                            CellValue::Text(text)
                        }
                    }
                };
                if !matches!(value, CellValue::Empty) || formula.is_some() {
                    last_row = last_row.max(row);
                    for offset in 0..repeat.min(64) {
                        let address = crate::address::format(row, column + offset);
                        sheet.set(
                            &address,
                            Cell { value: value.clone(), formula: formula.clone(), ..Default::default() },
                        );
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
        workbook.sheets.push(sheet);
    }
    if workbook.sheets.is_empty() {
        workbook.sheets.push(Sheet::new("Sheet1"));
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
    next: usize,
}

impl OdpShapeIds {
    fn assign(&mut self, object: &SlideObject) -> String {
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
        if !object.id.is_empty() {
            self.slide.insert(object.id.clone(), id.clone());
        }
        id
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

fn slide_object_xml(object: &SlideObject, ids: &mut OdpShapeIds) -> String {
    let id = ids.assign(object);
    let id_attrs = format!(" draw:id=\"{0}\" xml:id=\"{0}\"", escape(&id));
    if object.kind == "group" {
        // Children keep their absolute page coordinates; draw:g has no box of
        // its own, which matches the editor's group = bounding box of children.
        let mut children: Vec<&SlideObject> = object.children.iter().collect();
        children.sort_by_key(|child| child.z);
        let inner: String = children.into_iter().map(|child| slide_object_xml(child, ids)).collect();
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
                inner.push_str(&format!("<draw:image xlink:href=\"Pictures/{}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/>", escape(&image.name)));
            }
        }
        "line" | "arrow" => {
            let line = object.line.clone().unwrap_or_default();
            inner.push_str(&format!(
                "<draw:line svg:x1=\"{}\" svg:y1=\"{}\" svg:x2=\"{}\" svg:y2=\"{}\" draw:style-name=\"gr1\"/>",
                cm(object.x),
                cm(object.y),
                cm(object.x + line.x2),
                cm(object.y + line.y2)
            ));
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
        "chart" => {
            if let Some(chart) = &object.chart {
                let title =
                    if chart.title.trim().is_empty() { format!("{} chart", chart.kind) } else { chart.title.clone() };
                inner.push_str(&format!(
                    "<draw:text-box><text:p text:style-name=\"Standard\">{}</text:p></draw:text-box>",
                    crate::xml::escape_text(&title)
                ));
            }
        }
        _ => {
            if let Some(text) = &object.text {
                let mut writer = XmlWriter::new();
                writer.raw("<draw:text-box>");
                for paragraph in &text.paragraphs {
                    let level = paragraph.level;
                    writer.raw(&format!(
                        "<text:p text:style-name=\"Standard\">{}</text:p>",
                        crate::xml::escape_text(&paragraph.text)
                    ));
                    let _ = level;
                }
                if text.paragraphs.is_empty() {
                    writer.raw("<text:p/>");
                }
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

/// Charts anywhere on the slide, including inside groups.
fn count_odp_charts(objects: &[SlideObject]) -> usize {
    objects.iter().map(|object| usize::from(object.chart.is_some()) + count_odp_charts(&object.children)).sum()
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
    let charts: usize = deck.slides.iter().map(|slide| count_odp_charts(&slide.objects)).sum();
    if charts > 0 {
        warnings.push("Chart data is kept in the native .oswk file; ODP gets drawn placeholder shapes.".into());
    }
    let mut seen = std::collections::HashSet::new();
    warnings.retain(|warning| seen.insert(warning.clone()));
    Ok(DeckWrite { bytes, warnings })
}

pub fn write_odp(deck: &Deck) -> OfficeResult<Vec<u8>> {
    Ok(write_odp_package(deck)?.bytes)
}

fn write_odp_bytes(deck: &Deck, warnings: &mut Vec<String>) -> OfficeResult<Vec<u8>> {
    let mut body = String::new();
    let mut pictures: Vec<(String, Vec<u8>)> = Vec::new();
    let mut ids = OdpShapeIds::default();
    for (index, slide) in deck.slides.iter().enumerate() {
        body.push_str(&format!("<draw:page draw:name=\"Slide{}\" draw:master-page-name=\"Default\">", index + 1));
        ids.slide.clear();
        collect_odp_pictures(&slide.objects, &mut pictures);
        let mut objects: Vec<&SlideObject> = slide.objects.iter().collect();
        objects.sort_by_key(|object| object.z);
        for object in objects {
            body.push_str(&slide_object_xml(object, &mut ids));
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
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NS} {ANIM_NS} office:version=\"1.2\"><office:automatic-styles>{page_layout}</office:automatic-styles><office:body><office:presentation>{body}</office:presentation></office:body></office:document-content>"
    );
    let background = match deck.theme.as_str() {
        "dark" => "#0F172A",
        "business" => "#F8FAFC",
        "education" => "#FEFCE8",
        "modern" => "#FFFFFF",
        _ => "#FFFFFF",
    };
    let styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NS} office:version=\"1.2\"><office:styles/><office:automatic-styles>{page_layout}</office:automatic-styles><office:master-styles><style:master-page style:name=\"Default\" style:page-layout-name=\"pl1\"><style:drawing-page-properties draw:fill=\"solid\" draw:fill-color=\"{background}\"/></style:master-page></office:master-styles></office:document-styles>"
    );
    let mut zip = ZipWriter::new();
    zip.add_text("mimetype", "application/vnd.oasis.opendocument.presentation");
    zip.add_text("META-INF/manifest.xml", &manifest_for("application/vnd.oasis.opendocument.presentation"));
    zip.add_text("content.xml", &content);
    zip.add_text("styles.xml", &styles);
    zip.add_text("meta.xml", &meta_xml(&deck.title, "OmniOffice"));
    for (name, data) in &pictures {
        zip.add(&format!("Pictures/{name}"), data);
    }
    Ok(zip.finish())
}

pub fn write_odp_file(path: &Path, deck: &Deck) -> OfficeResult<()> {
    crate::io::write_atomic(path, &write_odp(deck)?)
}

/// One `draw:frame` of a slide or group: picture, text box, table, or a plain
/// shape kept as a rectangle. Frames without a size (lines) are skipped.
fn read_odp_frame(frame: &XmlNode, reader: &ZipReader, warnings: &mut Vec<String>) -> Option<SlideObject> {
    let x = frame.attr_any_ns("x").and_then(parse_cm).unwrap_or(40.0);
    let y = frame.attr_any_ns("y").and_then(parse_cm).unwrap_or(40.0);
    let w = frame.attr_any_ns("width").and_then(parse_cm).unwrap_or(320.0);
    let h = frame.attr_any_ns("height").and_then(parse_cm).unwrap_or(180.0);
    let mut images = Vec::new();
    frame.find_all("image", &mut images);
    if let Some(image_node) = images.first() {
        if let Some(href) = image_node.attr_any_ns("href") {
            let path = href.trim_start_matches("./");
            if let Ok(data) = reader.read(path) {
                let name = path.rsplit('/').next().unwrap_or("image.png").to_string();
                let mut object = SlideObject::new("image", x, y, w, h);
                object.image = Some(ImageData::from_bytes(&name, &data));
                return Some(object);
            }
        }
    }
    let mut text_boxes = Vec::new();
    frame.find_all("text-box", &mut text_boxes);
    if let Some(text_box) = text_boxes.first() {
        let mut paragraphs = Vec::new();
        let mut paragraph_nodes = Vec::new();
        text_box.find_all("p", &mut paragraph_nodes);
        for paragraph in &paragraph_nodes {
            paragraphs.push(TextParagraph { text: paragraph.deep_text(), ..Default::default() });
        }
        if paragraph_nodes.is_empty() {
            paragraphs.push(TextParagraph::default());
        }
        let mut object = SlideObject::new("text", x, y, w, h);
        object.text = Some(TextFrame { paragraphs, ..Default::default() });
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
fn read_odp_shapes(parent: &XmlNode, reader: &ZipReader, warnings: &mut Vec<String>) -> Vec<SlideObject> {
    let mut objects = Vec::new();
    for node in &parent.children {
        let object = match node.local_name() {
            "frame" => read_odp_frame(node, reader, warnings),
            "g" => {
                let children = read_odp_shapes(node, reader, warnings);
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

pub fn read_odp(bytes: &[u8]) -> OfficeResult<DeckRead> {
    let reader = ZipReader::open(bytes.to_vec())?;
    if !reader.contains("content.xml") {
        return Err(OfficeError::corrupt("The package does not contain content.xml."));
    }
    let text = reader.read_text("content.xml")?;
    let root = parse_xml(&text)?;
    let mut deck = Deck::new_blank("Imported presentation");
    deck.slides.clear();
    let mut warnings = Vec::new();
    // Slide size from styles.xml when available.
    if let Ok(styles) = reader.read_text("styles.xml") {
        if let Ok(styles_root) = parse_xml(&styles) {
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
        }
    }
    let mut pages = Vec::new();
    root.find_all("page", &mut pages);
    for page in pages {
        let objects = read_odp_shapes(page, &reader, &mut warnings);
        let animations = read_odp_timing(page, &objects, &mut warnings);
        let mut slide = Slide { objects, animations, ..Default::default() };
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
}

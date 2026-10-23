//! ODT writer: package parts, styles, notes, tables and media. The reader and
//! the ODS/ODP halves stay in `odf.rs`; shared helpers are `pub(crate)`.

use super::NS;
use crate::error::OfficeResult;
use crate::model::*;
use crate::xml::XmlWriter;
use crate::zip::ZipWriter;
use std::collections::HashMap;
use std::path::Path;

pub(crate) fn cm(points: f64) -> String {
    format!("{:.3}cm", points * crate::model::PT_TO_MM / 10.0)
}

pub(crate) fn parse_cm(value: &str) -> Option<f64> {
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

pub(crate) fn escape(value: &str) -> String {
    crate::xml::escape_attr(value)
}

#[derive(Default)]
pub(crate) struct AutoStyles {
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
    pub(crate) fn paragraph(&mut self, xml: String) -> String {
        if let Some(name) = self.paragraph_keys.get(&xml) {
            return name.clone();
        }
        let name = format!("P{}", self.paragraphs.len() + 1);
        self.paragraphs.push((name.clone(), xml.clone()));
        self.paragraph_keys.insert(xml, name.clone());
        name
    }

    pub(crate) fn text(&mut self, xml: String) -> String {
        if let Some(name) = self.text_keys.get(&xml) {
            return name.clone();
        }
        let name = format!("T{}", self.texts.len() + 1);
        self.texts.push((name.clone(), xml.clone()));
        self.text_keys.insert(xml, name.clone());
        name
    }

    pub(crate) fn cell(&mut self, xml: String) -> String {
        if let Some(name) = self.cell_keys.get(&xml) {
            return name.clone();
        }
        let name = format!("C{}", self.cells.len() + 1);
        self.cells.push((name.clone(), xml.clone()));
        self.cell_keys.insert(xml, name.clone());
        name
    }

    pub(crate) fn graphic(&mut self, xml: String) -> String {
        if let Some(name) = self.graphic_keys.get(&xml) {
            return name.clone();
        }
        let name = format!("G{}", self.graphics.len() + 1);
        self.graphics.push((name.clone(), xml.clone()));
        self.graphic_keys.insert(xml, name.clone());
        name
    }

    pub(crate) fn xml(&self) -> String {
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
pub(crate) struct Media {
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
pub(crate) struct NoteContext {
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

pub(crate) fn paragraph_style_xml(props: &ParaProps, page_break: bool) -> String {
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

pub(crate) fn write_runs(writer: &mut XmlWriter, runs: &[Run], styles: &mut AutoStyles, notes: &NoteContext) {
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

pub(crate) fn write_table(
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

/// Media type for a package entry. Chart sub-documents are directories; every
/// other entry is a file whose type follows its extension.
fn entry_media_type(name: &str) -> &'static str {
    if name.ends_with('/') {
        return "application/vnd.oasis.opendocument.chart";
    }
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else {
        "text/xml"
    }
}

/// Builds the package manifest from the entries actually written.
///
/// LibreOffice refuses a package whose manifest and contents disagree (a
/// dangling entry or an unlisted file is "source file could not be loaded"),
/// so the manifest is generated from the same list the zip is filled with
/// instead of a static template. `mimetype` and the manifest itself are the
/// only entries a manifest never lists.
pub(crate) fn manifest_for(mime: &str, files: &[String]) -> String {
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\" manifest:version=\"1.2\"><manifest:file-entry manifest:full-path=\"/\" manifest:media-type=\"{mime}\"/>"
    );
    for file in files {
        let name = escape(file);
        if file.ends_with('/') {
            out.push_str(&format!(
                "<manifest:file-entry manifest:full-path=\"{name}\" manifest:version=\"1.2\" manifest:media-type=\"{}\"/>",
                entry_media_type(file)
            ));
        } else {
            out.push_str(&format!(
                "<manifest:file-entry manifest:full-path=\"{name}\" manifest:media-type=\"{}\"/>",
                entry_media_type(file)
            ));
        }
    }
    out.push_str("</manifest:manifest>");
    out
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

pub(crate) fn meta_xml(title: &str, generator: &str, watermark: Option<&Watermark>) -> String {
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
    let mut files =
        vec!["content.xml".to_string(), "styles.xml".to_string(), "meta.xml".to_string(), "settings.xml".to_string()];
    for (name, _) in &media.items {
        files.push(format!("Pictures/{name}"));
    }
    let mut zip = ZipWriter::new();
    zip.add_text("mimetype", "application/vnd.oasis.opendocument.text");
    zip.add_text("META-INF/manifest.xml", &manifest_for("application/vnd.oasis.opendocument.text", &files));
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

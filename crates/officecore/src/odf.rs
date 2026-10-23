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
mod text_write;
pub(crate) use text_write::{
    cm, escape, manifest_for, meta_xml, paragraph_style_xml, parse_cm, write_runs, write_table, AutoStyles, Media,
    NoteContext,
};
pub use text_write::{write_odt, write_odt_file, TextRead};

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
    let mut files = vec!["content.xml".to_string(), "styles.xml".to_string(), "meta.xml".to_string()];
    for object in &chart_objects {
        files.push(format!("{}/", object.name));
        files.push(format!("{}/content.xml", object.name));
        files.push(format!("{}/styles.xml", object.name));
    }
    let manifest = manifest_for("application/vnd.oasis.opendocument.spreadsheet", &files);
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
    let mut files = vec!["content.xml".to_string(), "styles.xml".to_string(), "meta.xml".to_string()];
    for object in &chart_objects {
        files.push(format!("{}/", object.name));
        files.push(format!("{}/content.xml", object.name));
        files.push(format!("{}/styles.xml", object.name));
    }
    for (name, _) in &pictures {
        files.push(format!("Pictures/{name}"));
    }
    let manifest = manifest_for("application/vnd.oasis.opendocument.presentation", &files);
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
#[path = "odf_tests.rs"]
mod tests;

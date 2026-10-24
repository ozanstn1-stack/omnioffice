//! ODT (Writer) import: content, styles, notes, frames and watermarks.

use super::*;
use crate::error::{OfficeError, OfficeResult};
use crate::xml::{parse_xml, XmlNode};
use std::collections::HashMap;
use std::path::Path;

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
pub(super) struct NoteReadState {
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

pub(super) fn node_text_runs(node: &XmlNode, runs: &mut Vec<Run>, notes: &mut NoteReadState, styles: &ReadStyles) {
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

pub(super) fn paragraph_props(node: &XmlNode, styles: &ReadStyles) -> ParaProps {
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

pub(super) fn collect_styles(root: &XmlNode, styles: &mut ReadStyles) {
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
pub(super) fn wrap_trailing_text(xml: &str) -> String {
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

//! UYAP `.udf` documents.
//!
//! UDF ("UYAP Doküman Formatı") is the document format of the Turkish judicial
//! system. It is a ZIP container whose `content.xml` carries the text; the
//! format is closed, so this module implements the structure that public
//! documentation and real samples agree on:
//!
//! ```text
//! <template>
//!   <properties>  page size, margins, document info
//!   <elements>    an ordered list of <paragraph>, <table>, <image>, ...
//!     <paragraph Alignment="0" LineSpacing="1.15" ...>
//!       <content startOffset="0" length="5" Bold="true" Size="12">text</content>
//!     </table>    <row> <cell colspan rowspan> <paragraph>...
//!   <headers>/<footers>
//! ```
//!
//! Formatting is stored per `<content>` segment (there is no inheritance), and
//! `startOffset`/`length` are the segment's position in the document's plain
//! text. We read the per-segment attributes and keep the offsets consistent on
//! export.
//!
//! Anything we do not model (an unknown element, an embedded image, a base64
//! RTF fragment) is **reported in the warnings** and, for whole elements, kept
//! verbatim in [`TextDocument::preserved_udf`] so an export writes it back
//! instead of dropping it. No code was copied from any third-party project:
//! the layout comes from the public structure of the format and from the files
//! this module itself writes.

use crate::error::{OfficeError, OfficeResult};
use crate::io::{read_bytes, write_atomic};
use crate::model::{
    Block, ListInfo, PageSetup, ParaProps, PreservedUdf, Run, TableCell, TableData, TableRow, TextDocument,
};
use crate::odf::TextRead;
use crate::xml::{escape_attr, escape_text, parse_xml, XmlNode};
use crate::zip::{decode_utf8, ZipReader, ZipWriter};
use base64::Engine;
use std::path::Path;

/// Largest `content.xml` we parse (a UDF is a few hundred KB at most).
const MAX_CONTENT_XML: u64 = 64 * 1024 * 1024;

/// UDF alignment is numeric: 0 left, 1 centre, 2 right, 3 justified.
fn alignment_to_name(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "0" | "left" => Some("left"),
        "1" | "center" | "centre" => Some("center"),
        "2" | "right" => Some("right"),
        "3" | "justify" | "justified" | "both" => Some("justify"),
        _ => None,
    }
}

fn alignment_to_number(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "center" | "centre" => "1",
        "right" => "2",
        "justify" | "justified" | "both" => "3",
        _ => "0",
    }
}

/// `R,G,B` (UDF's colour form) to `#RRGGBB`, or `#RRGGBB`/`RRGGBB` verbatim.
fn color_from_udf(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let parts: Vec<&str> = trimmed.split(',').map(str::trim).collect();
    if parts.len() == 3 {
        let mut out = String::from("#");
        for part in parts {
            let number: u32 = part.parse().ok()?;
            if number > 255 {
                return None;
            }
            out.push_str(&format!("{number:02X}"));
        }
        return Some(out);
    }
    crate::io::normalize_hex(trimmed)
}

/// `#RRGGBB` to UDF's `R,G,B`, so a colour survives a round trip.
fn color_to_udf(value: &str) -> Option<String> {
    let hex = crate::io::normalize_hex(value)?;
    let bare = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&bare[0..2], 16).ok()?;
    let g = u8::from_str_radix(&bare[2..4], 16).ok()?;
    let b = u8::from_str_radix(&bare[4..6], 16).ok()?;
    Some(format!("{r},{g},{b}"))
}

/// Attribute lookup that ignores the namespace prefix **and** the case, because
/// UDF versions disagree on `Alignment` vs `alignment`.
fn attr_ci<'a>(node: &'a XmlNode, name: &str) -> Option<&'a str> {
    let wanted = name.to_ascii_lowercase();
    if let Some(value) = node.attrs.get(name) {
        return Some(value.as_str());
    }
    node.attrs
        .iter()
        .find(|(key, _)| !key.starts_with("xmlns") && key.to_ascii_lowercase() == wanted)
        .map(|(_, value)| value.as_str())
}

fn attr_bool(node: &XmlNode, name: &str) -> Option<bool> {
    let value = attr_ci(node, name)?.trim().to_ascii_lowercase();
    match value.as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn attr_f64(node: &XmlNode, name: &str) -> Option<f64> {
    attr_ci(node, name)?.trim().parse().ok()
}

/// Serializes a parsed node back to XML, so an unknown element is written back
/// exactly as it was read.
fn node_to_xml(node: &XmlNode) -> String {
    let mut out = String::new();
    out.push('<');
    out.push_str(&node.name);
    for (key, value) in &node.attrs {
        out.push(' ');
        out.push_str(key);
        out.push_str("=\"");
        out.push_str(&escape_attr(value));
        out.push('"');
    }
    if node.children.is_empty() && node.text.is_empty() {
        out.push_str("/>");
        return out;
    }
    out.push('>');
    out.push_str(&escape_text(&node.text));
    for child in &node.children {
        out.push_str(&node_to_xml(child));
    }
    out.push_str("</");
    out.push_str(&node.name);
    out.push('>');
    out
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

/// The plain text of an embedded RTF fragment: control words, groups and the
/// `\uN` escapes are turned into text, everything else is dropped. This is the
/// same "good enough" extraction the RTF importer's fallback uses; a fragment
/// never carries structure we could map to runs anyway.
fn rtf_to_text(rtf: &str) -> String {
    let mut out = String::new();
    let mut chars = rtf.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '{' | '}' => {}
            '\\' => {
                match chars.peek().copied() {
                    Some('\'') => {
                        // \'hh : one byte in the current code page; collect two
                        // hex digits and push the byte as Latin-1.
                        chars.next();
                        let mut hex = String::new();
                        for _ in 0..2 {
                            if let Some(digit) = chars.next() {
                                if digit.is_ascii_hexdigit() {
                                    hex.push(digit);
                                }
                            }
                        }
                        if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                            out.push(byte as char);
                        }
                    }
                    Some('u') => {
                        // \uN? : a Unicode code point, followed by one fallback
                        // character that must be skipped.
                        chars.next();
                        let mut digits = String::new();
                        if let Some('-') = chars.peek().copied() {
                            digits.push('-');
                            chars.next();
                        }
                        while let Some(digit) = chars.peek().copied() {
                            if digit.is_ascii_digit() {
                                digits.push(digit);
                                chars.next();
                            } else {
                                break;
                            }
                        }
                        if let Ok(code) = digits.parse::<i32>() {
                            let code = if code < 0 { code + 65536 } else { code };
                            if let Some(value) = char::from_u32(code as u32) {
                                out.push(value);
                            }
                        }
                        // Skip the single fallback character.
                        if chars.peek().is_some() && chars.peek() != Some(&' ') {
                            chars.next();
                        }
                    }
                    _ => {
                        // A control word: letters then an optional number, ended
                        // by a space (consumed) or a non-letter.
                        let mut word = String::new();
                        while let Some(letter) = chars.peek().copied() {
                            if letter.is_ascii_alphabetic() {
                                word.push(letter);
                                chars.next();
                            } else {
                                break;
                            }
                        }
                        let mut number = String::new();
                        if let Some('-') = chars.peek().copied() {
                            number.push('-');
                            chars.next();
                        }
                        while let Some(digit) = chars.peek().copied() {
                            if digit.is_ascii_digit() {
                                number.push(digit);
                                chars.next();
                            } else {
                                break;
                            }
                        }
                        if chars.peek() == Some(&' ') {
                            chars.next();
                        }
                        let code = number.parse::<i32>().unwrap_or(0);
                        match word.as_str() {
                            "par" | "line" => out.push('\n'),
                            "tab" => out.push('\t'),
                            // Escaped literals.
                            "{" | "}" | "\\" => out.push_str(&word),
                            "emdash" => out.push('\u{2014}'),
                            "endash" => out.push('\u{2013}'),
                            "lquote" => out.push('\u{2018}'),
                            "rquote" => out.push('\u{2019}'),
                            "ldblquote" => out.push('\u{201C}'),
                            "rdblquote" => out.push('\u{201D}'),
                            "bullet" => out.push('\u{2022}'),
                            "u" => {
                                let code = if code < 0 { code + 65536 } else { code };
                                if let Some(value) = char::from_u32(code as u32) {
                                    out.push(value);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            '\r' | '\n' => {}
            other => out.push(other),
        }
    }
    out
}

/// One `<content>` segment -> a run.
fn read_content_segment(node: &XmlNode, warnings: &mut Vec<String>) -> Option<Run> {
    let kind = attr_ci(node, "type").unwrap_or("text").trim().to_ascii_lowercase();
    let text = if kind == "rtf" {
        let encoded = node.deep_text();
        let trimmed = encoded.trim();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(trimmed.as_bytes())
            .ok()
            .and_then(|bytes| decode_utf8(&bytes, "rtf fragment").ok());
        match decoded {
            Some(rtf) => {
                warnings.push("An embedded RTF fragment was imported as plain text.".into());
                rtf_to_text(&rtf)
            }
            None => {
                warnings.push("An embedded RTF fragment could not be decoded and was skipped.".into());
                return None;
            }
        }
    } else {
        node.deep_text()
    };
    let mut run = Run { text, ..Default::default() };
    run.bold = attr_bool(node, "Bold").unwrap_or(false);
    run.italic = attr_bool(node, "Italic").unwrap_or(false);
    run.underline = attr_bool(node, "Underline").unwrap_or(false);
    run.strike = attr_bool(node, "Strikethrough").unwrap_or(false);
    run.font = attr_ci(node, "Family").map(str::to_string).filter(|value| !value.trim().is_empty());
    run.size_pt = attr_f64(node, "Size").filter(|value| *value > 0.0);
    run.color = attr_ci(node, "Foreground").and_then(color_from_udf);
    run.highlight = attr_ci(node, "Background").and_then(color_from_udf);
    match attr_ci(node, "Offset").map(|value| value.trim().to_ascii_lowercase()) {
        Some(value) if value == "superscript" || value == "super" => run.superscript = true,
        Some(value) if value == "subscript" || value == "sub" => run.subscript = true,
        _ => {}
    }
    Some(run)
}

/// `<list>`/`<listInfo>` child of a paragraph, when present.
fn read_list(node: &XmlNode) -> Option<ListInfo> {
    let list = node.children.iter().find(|child| child.local_name() == "list" || child.local_name() == "listInfo")?;
    let kind = attr_ci(list, "kind").unwrap_or("bullet").trim().to_ascii_lowercase();
    let kind = if kind == "number" || kind == "ordered" { "number" } else { "bullet" };
    Some(ListInfo {
        kind: kind.to_string(),
        level: attr_ci(list, "level").and_then(|value| value.trim().parse().ok()).unwrap_or(0),
        start: attr_ci(list, "start").and_then(|value| value.trim().parse().ok()).unwrap_or(1),
        marker: attr_ci(list, "marker").unwrap_or(if kind == "number" { "1." } else { "•" }).to_string(),
    })
}

fn read_paragraph(node: &XmlNode, warnings: &mut Vec<String>) -> Block {
    let mut props = ParaProps::default();
    if let Some(style) = attr_ci(node, "Style").map(str::trim).filter(|value| !value.is_empty()) {
        props.style = style.to_string();
    }
    if let Some(align) = attr_ci(node, "Alignment").and_then(alignment_to_name) {
        props.align = align.to_string();
    }
    if let Some(value) = attr_f64(node, "LineSpacing").filter(|value| *value > 0.0) {
        props.line_spacing = value;
    }
    if let Some(value) = attr_f64(node, "SpaceBefore") {
        props.space_before_pt = value;
    }
    if let Some(value) = attr_f64(node, "SpaceAfter") {
        props.space_after_pt = value;
    }
    if let Some(value) = attr_f64(node, "LeftIndent") {
        props.indent_left_pt = value;
    }
    if let Some(value) = attr_f64(node, "RightIndent") {
        props.indent_right_pt = value;
    }
    if let Some(value) = attr_f64(node, "FirstLineIndent") {
        props.first_line_pt = value;
    }
    if let Some(value) = attr_bool(node, "PageBreakBefore") {
        props.page_break_before = value;
    }
    props.list = read_list(node);
    // Tab stops: `<tabStops><tabStop position alignment/></tabStops>`.
    if let Some(stops) = node.children.iter().find(|child| child.local_name() == "tabStops") {
        for stop in stops.children.iter().filter(|child| child.local_name() == "tabStop") {
            if let Some(pos) = attr_f64(stop, "position") {
                let align = attr_ci(stop, "alignment").unwrap_or("left").trim().to_ascii_lowercase();
                let align = match align.as_str() {
                    "center" | "centre" => "center",
                    "right" => "right",
                    "decimal" => "decimal",
                    _ => "left",
                };
                props.tabs.push(crate::model::TabStop { pos_pt: pos, align: align.to_string() });
            }
        }
    }
    let mut runs: Vec<Run> = Vec::new();
    for child in &node.children {
        match child.local_name() {
            "content" | "text" | "span" | "run" => {
                if let Some(run) = read_content_segment(child, warnings) {
                    runs.push(run);
                }
            }
            "tabStops" | "list" | "listInfo" => {}
            // An unknown inline element: keep its text so nothing is lost, and
            // say so once.
            other => {
                let text = child.deep_text();
                if !text.is_empty() {
                    runs.push(Run { text, ..Default::default() });
                }
                warnings.push(format!("An unrecognised UDF element <{other}> was read as plain text."));
            }
        }
    }
    // Direct text on the paragraph itself (some writers skip <content>).
    if runs.is_empty() && !node.text.trim().is_empty() {
        runs.push(Run { text: node.text.clone(), ..Default::default() });
    }
    if runs.is_empty() {
        runs.push(Run::default());
    }
    Block::Paragraph { props, runs }
}

fn read_table(node: &XmlNode, warnings: &mut Vec<String>, preserved: &mut Vec<PreservedUdf>) -> Block {
    let mut table = TableData {
        borders: attr_bool(node, "Border").unwrap_or(true),
        border_color: attr_ci(node, "BorderColor").and_then(color_from_udf).unwrap_or_default(),
        align: attr_ci(node, "Align").and_then(alignment_to_name).unwrap_or("left").to_string(),
        ..Default::default()
    };
    for row_node in node.children.iter().filter(|child| child.local_name() == "row") {
        let mut row = TableRow {
            header: attr_bool(row_node, "Header").unwrap_or(false),
            height_pt: attr_f64(row_node, "Height").filter(|value| *value > 0.0),
            ..Default::default()
        };
        for cell_node in row_node.children.iter().filter(|child| child.local_name() == "cell") {
            let mut cell = TableCell {
                colspan: attr_f64(cell_node, "Colspan").map(|value| value.max(1.0) as u32).unwrap_or(1),
                rowspan: attr_f64(cell_node, "Rowspan").map(|value| value.max(1.0) as u32).unwrap_or(1),
                background: attr_ci(cell_node, "Background").and_then(color_from_udf),
                align: attr_ci(cell_node, "Align").and_then(alignment_to_name).unwrap_or("left").to_string(),
                valign: attr_ci(cell_node, "Valign")
                    .or_else(|| attr_ci(cell_node, "VerticalAlignment"))
                    .map(|value| value.trim().to_ascii_lowercase())
                    .unwrap_or_default(),
                width_pt: attr_f64(cell_node, "Width").filter(|value| *value > 0.0),
                ..Default::default()
            };
            cell.blocks = read_blocks(&cell_node.children, warnings, preserved);
            if cell.blocks.is_empty() {
                cell.blocks = vec![Block::paragraph("")];
            }
            row.cells.push(cell);
        }
        table.rows.push(row);
    }
    if table.rows.is_empty() {
        table = TableData::simple(1, 1, 460.0);
    }
    let columns = table.rows.iter().map(|row| row.cells.len()).max().unwrap_or(1).max(1);
    table.column_widths_pt = vec![460.0 / columns as f64; columns];
    Block::Table { table }
}

/// Elements we model directly; everything else is preserved verbatim.
const KNOWN_ELEMENTS: &[&str] =
    &["paragraph", "p", "table", "tabStops", "list", "listInfo", "content", "text", "span", "run"];

fn read_blocks(children: &[XmlNode], warnings: &mut Vec<String>, preserved: &mut Vec<PreservedUdf>) -> Vec<Block> {
    let mut blocks = Vec::new();
    for child in children {
        match child.local_name() {
            "paragraph" | "p" => blocks.push(read_paragraph(child, warnings)),
            "table" => blocks.push(read_table(child, warnings, preserved)),
            // Whitespace and containers we descend into without a warning.
            "#text" => {}
            _ if KNOWN_ELEMENTS.contains(&child.local_name()) => {}
            other => {
                if other == "image" || other == "picture" || other == "object" {
                    warnings.push("An embedded image in the UDF was kept but is not shown in the editor.".into());
                } else {
                    warnings.push(format!("An unrecognised UDF element <{other}> was kept as it is."));
                }
                preserved.push(PreservedUdf { name: other.to_string(), xml: node_to_xml(child) });
            }
        }
    }
    blocks
}

fn read_page_setup(node: &XmlNode) -> PageSetup {
    let mut page = PageSetup::default();
    let Some(format) = node.children.iter().find(|child| child.local_name() == "pageFormat") else {
        return page;
    };
    if let Some(width) = attr_f64(format, "width").filter(|value| *value > 0.0) {
        page.width_pt = width;
    }
    if let Some(height) = attr_f64(format, "height").filter(|value| *value > 0.0) {
        page.height_pt = height;
    }
    if let Some(value) = attr_f64(format, "marginTop") {
        page.margin_top_pt = value;
    }
    if let Some(value) = attr_f64(format, "marginRight") {
        page.margin_right_pt = value;
    }
    if let Some(value) = attr_f64(format, "marginBottom") {
        page.margin_bottom_pt = value;
    }
    if let Some(value) = attr_f64(format, "marginLeft") {
        page.margin_left_pt = value;
    }
    if let Some(orientation) = attr_ci(format, "orientation").map(|value| value.trim().to_ascii_lowercase()) {
        page.orientation = if orientation.starts_with("land") { "landscape".into() } else { "portrait".into() };
    }
    page
}

/// Reads a UDF package into a Writer document.
pub fn read_udf(bytes: &[u8]) -> OfficeResult<TextRead> {
    let reader = ZipReader::open(bytes.to_vec())?;
    if !reader.contains("content.xml") {
        return Err(OfficeError::corrupt("The UDF package has no content.xml."));
    }
    let xml = reader.read_text("content.xml")?;
    if xml.len() as u64 > MAX_CONTENT_XML {
        return Err(OfficeError::corrupt("The UDF content.xml is larger than the supported limit."));
    }
    let root = parse_xml(&xml)?;
    let template = root
        .children
        .iter()
        .find(|child| matches!(child.local_name(), "template" | "document" | "root"))
        .unwrap_or(&root);
    let mut warnings = Vec::new();
    let mut preserved = Vec::new();
    let mut document = TextDocument::new_blank("Untitled document");
    document.blocks.clear();
    if let Some(properties) = template.children.iter().find(|child| child.local_name() == "properties") {
        document.page = read_page_setup(properties);
        if let Some(info) = properties.children.iter().find(|child| child.local_name() == "documentInfo") {
            if let Some(title) = attr_ci(info, "title").map(str::trim).filter(|value| !value.is_empty()) {
                document.title = title.to_string();
            }
            if let Some(author) = attr_ci(info, "author").map(str::trim).filter(|value| !value.is_empty()) {
                document.metadata.author = author.to_string();
            }
        }
    }
    let elements = template.children.iter().find(|child| child.local_name() == "elements");
    let content_children: &[XmlNode] = match elements {
        Some(elements) => &elements.children,
        None => &template.children,
    };
    document.blocks = read_blocks(content_children, &mut warnings, &mut preserved);
    if let Some(headers) = template.children.iter().find(|child| child.local_name() == "headers") {
        for header in headers.children.iter().filter(|child| child.local_name() == "header") {
            document.header = read_blocks(&header.children, &mut warnings, &mut preserved);
        }
    }
    if let Some(footers) = template.children.iter().find(|child| child.local_name() == "footers") {
        for footer in footers.children.iter().filter(|child| child.local_name() == "footer") {
            document.footer = read_blocks(&footer.children, &mut warnings, &mut preserved);
        }
    }
    if document.blocks.is_empty() {
        document.blocks = vec![Block::paragraph("")];
    }
    document.preserved_udf = preserved;
    warnings.sort();
    warnings.dedup();
    Ok(TextRead { document, warnings })
}

/// Reads a `.udf` file from disk.
pub fn read_udf_file(path: &Path) -> OfficeResult<TextRead> {
    let bytes = read_bytes(path)?;
    read_udf(&bytes)
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

fn write_runs(writer: &mut String, runs: &[Run], cursor: &mut usize) {
    for run in runs {
        if run.text.is_empty() {
            continue;
        }
        let length = run.text.chars().count();
        writer.push_str("<content");
        writer.push_str(&format!(" startOffset=\"{}\" length=\"{}\"", *cursor, length));
        if let Some(font) = &run.font {
            writer.push_str(&format!(" Family=\"{}\"", escape_attr(font)));
        }
        if let Some(size) = run.size_pt {
            writer.push_str(&format!(" Size=\"{}\"", trim_number(size)));
        }
        writer.push_str(&format!(" Bold=\"{}\"", run.bold));
        writer.push_str(&format!(" Italic=\"{}\"", run.italic));
        writer.push_str(&format!(" Underline=\"{}\"", run.underline));
        writer.push_str(&format!(" Strikethrough=\"{}\"", run.strike));
        if let Some(color) = run.color.as_deref().and_then(color_to_udf) {
            writer.push_str(&format!(" Foreground=\"{color}\""));
        }
        if let Some(highlight) = run.highlight.as_deref().and_then(color_to_udf) {
            writer.push_str(&format!(" Background=\"{highlight}\""));
        }
        if run.superscript {
            writer.push_str(" Offset=\"Superscript\"");
        } else if run.subscript {
            writer.push_str(" Offset=\"Subscript\"");
        }
        writer.push('>');
        writer.push_str(&escape_text(&run.text));
        writer.push_str("</content>");
        *cursor += length;
    }
}

/// Trims a float to its shortest exact form ("12" not "12.0").
fn trim_number(value: f64) -> String {
    let mut text = format!("{value}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    text
}

fn write_paragraph(writer: &mut String, props: &ParaProps, runs: &[Run], cursor: &mut usize) {
    writer.push_str("<paragraph");
    if !props.style.is_empty() && props.style != "Normal" {
        writer.push_str(&format!(" Style=\"{}\"", escape_attr(&props.style)));
    }
    writer.push_str(&format!(" Alignment=\"{}\"", alignment_to_number(&props.align)));
    writer.push_str(&format!(" LineSpacing=\"{}\"", trim_number(props.line_spacing)));
    writer.push_str(&format!(" SpaceBefore=\"{}\"", trim_number(props.space_before_pt)));
    writer.push_str(&format!(" SpaceAfter=\"{}\"", trim_number(props.space_after_pt)));
    writer.push_str(&format!(" LeftIndent=\"{}\"", trim_number(props.indent_left_pt)));
    writer.push_str(&format!(" RightIndent=\"{}\"", trim_number(props.indent_right_pt)));
    writer.push_str(&format!(" FirstLineIndent=\"{}\"", trim_number(props.first_line_pt)));
    if props.page_break_before {
        writer.push_str(" PageBreakBefore=\"true\"");
    }
    writer.push('>');
    if let Some(list) = &props.list {
        writer.push_str(&format!(
            "<list kind=\"{}\" level=\"{}\" start=\"{}\" marker=\"{}\"/>",
            escape_attr(&list.kind),
            list.level,
            list.start,
            escape_attr(&list.marker)
        ));
    }
    if !props.tabs.is_empty() {
        writer.push_str("<tabStops>");
        for tab in &props.tabs {
            writer.push_str(&format!(
                "<tabStop position=\"{}\" alignment=\"{}\"/>",
                trim_number(tab.pos_pt),
                escape_attr(&tab.align)
            ));
        }
        writer.push_str("</tabStops>");
    }
    write_runs(writer, runs, cursor);
    writer.push_str("</paragraph>");
    *cursor += 1; // the paragraph break counts as one plain-text character
}

fn write_table(writer: &mut String, table: &TableData, cursor: &mut usize) {
    writer.push_str(&format!(
        "<table Border=\"{}\" BorderColor=\"{}\" Align=\"{}\">",
        table.borders,
        escape_attr(&table.border_color),
        alignment_to_number(&table.align)
    ));
    for row in &table.rows {
        writer.push_str("<row");
        if let Some(height) = row.height_pt.filter(|value| *value > 0.0) {
            writer.push_str(&format!(" Height=\"{}\"", trim_number(height)));
        }
        if row.header {
            writer.push_str(" Header=\"true\"");
        }
        writer.push('>');
        for cell in &row.cells {
            writer.push_str(&format!("<cell Colspan=\"{}\" Rowspan=\"{}\"", cell.colspan.max(1), cell.rowspan.max(1)));
            if let Some(background) = cell.background.as_deref().and_then(color_to_udf) {
                writer.push_str(&format!(" Background=\"{background}\""));
            }
            writer.push_str(&format!(" Align=\"{}\"", alignment_to_number(&cell.align)));
            if !cell.valign.is_empty() {
                writer.push_str(&format!(" Valign=\"{}\"", escape_attr(&cell.valign)));
            }
            if let Some(width) = cell.width_pt.filter(|value| *value > 0.0) {
                writer.push_str(&format!(" Width=\"{}\"", trim_number(width)));
            }
            writer.push('>');
            for block in &cell.blocks {
                write_block(writer, block, cursor);
            }
            writer.push_str("</cell>");
        }
        writer.push_str("</row>");
    }
    writer.push_str("</table>");
}

fn write_block(writer: &mut String, block: &Block, cursor: &mut usize) {
    match block {
        Block::Paragraph { props, runs } => write_paragraph(writer, props, runs, cursor),
        Block::Table { table } => write_table(writer, table, cursor),
        // A UDF has no model for these; they are reported by `write_udf`.
        _ => {}
    }
}

/// Writes a Writer document as a UDF package.
pub fn write_udf(document: &TextDocument) -> OfficeResult<Vec<u8>> {
    let mut elements = String::new();
    let mut cursor = 0usize;
    for block in &document.blocks {
        write_block(&mut elements, block, &mut cursor);
    }
    // Unknown elements read from a file are written back verbatim.
    for preserved in &document.preserved_udf {
        elements.push_str(&preserved.xml);
    }
    let mut headers = String::new();
    for block in &document.header {
        write_block(&mut headers, block, &mut cursor);
    }
    let mut footers = String::new();
    for block in &document.footer {
        write_block(&mut footers, block, &mut cursor);
    }
    let page = &document.page;
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <template format_id=\"1.8\" producer=\"OmniOffice {version}\">\
         <properties>\
         <pageFormat width=\"{width}\" height=\"{height}\" orientation=\"{orientation}\" \
         marginTop=\"{mt}\" marginRight=\"{mr}\" marginBottom=\"{mb}\" marginLeft=\"{ml}\"/>\
         <documentInfo title=\"{title}\" documentType=\"BELGE\" createdBy=\"OmniOffice\" createdDate=\"{date}\" author=\"{author}\"/>\
         </properties>\
         <elements>{elements}</elements>\
         {headers_section}{footers_section}\
         </template>",
        version = env!("CARGO_PKG_VERSION"),
        width = trim_number(page.width_pt),
        height = trim_number(page.height_pt),
        orientation = escape_attr(&page.orientation),
        mt = trim_number(page.margin_top_pt),
        mr = trim_number(page.margin_right_pt),
        mb = trim_number(page.margin_bottom_pt),
        ml = trim_number(page.margin_left_pt),
        title = escape_attr(&document.title),
        date = escape_attr(&document.metadata.created),
        author = escape_attr(&document.metadata.author),
        headers_section = if headers.is_empty() {
            String::new()
        } else {
            format!("<headers><header type=\"default\">{headers}</header></headers>")
        },
        footers_section = if footers.is_empty() {
            String::new()
        } else {
            format!("<footers><footer type=\"default\">{footers}</footer></footers>")
        },
    );
    let mut zip = ZipWriter::new();
    zip.add_text("content.xml", &content);
    Ok(zip.finish())
}

/// Writes a `.udf` file to disk (atomic).
pub fn write_udf_file(path: &Path, document: &TextDocument) -> OfficeResult<()> {
    let bytes = write_udf(document)?;
    write_atomic(path, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ListInfo, TabStop};

    fn document_with(blocks: Vec<Block>) -> TextDocument {
        let mut document = TextDocument::new_blank("Test");
        document.blocks = blocks;
        document
    }

    #[test]
    fn round_trips_paragraphs_and_character_formatting() {
        let document = document_with(vec![
            Block::Paragraph {
                props: ParaProps { align: "justify".into(), first_line_pt: 36.0, ..Default::default() },
                runs: vec![
                    Run { text: "Merhaba ".into(), ..Default::default() },
                    Run {
                        text: "dünya".into(),
                        bold: true,
                        italic: true,
                        underline: true,
                        size_pt: Some(14.0),
                        color: Some("#FF0000".into()),
                        ..Default::default()
                    },
                ],
            },
            Block::Paragraph {
                props: ParaProps { align: "center".into(), style: "Heading1".into(), ..Default::default() },
                runs: vec![Run { text: "Başlık".into(), ..Default::default() }],
            },
        ]);
        let bytes = write_udf(&document).unwrap();
        let read = read_udf(&bytes).unwrap();
        assert_eq!(read.document.blocks.len(), 2);
        match &read.document.blocks[0] {
            Block::Paragraph { props, runs } => {
                assert_eq!(props.align, "justify");
                assert_eq!(props.first_line_pt, 36.0);
                assert_eq!(runs.len(), 2);
                assert_eq!(runs[0].text, "Merhaba ");
                assert!(runs[1].bold && runs[1].italic && runs[1].underline);
                assert_eq!(runs[1].size_pt, Some(14.0));
                assert_eq!(runs[1].color.as_deref(), Some("#FF0000"));
            }
            other => panic!("expected a paragraph, got {other:?}"),
        }
        match &read.document.blocks[1] {
            Block::Paragraph { props, runs } => {
                assert_eq!(props.align, "center");
                assert_eq!(props.style, "Heading1");
                assert_eq!(runs[0].text, "Başlık");
            }
            other => panic!("expected a paragraph, got {other:?}"),
        }
        assert!(read.warnings.is_empty(), "warnings: {:?}", read.warnings);
    }

    #[test]
    fn round_trips_a_table_with_merged_cells() {
        let mut table = TableData::simple(2, 2, 400.0);
        table.rows[0].header = true;
        table.rows[0].cells[0].colspan = 2;
        table.rows[0].cells[0].blocks = vec![Block::paragraph("Başlık")];
        table.rows[1].cells[0].blocks = vec![Block::paragraph("A")];
        table.rows[1].cells[1].blocks = vec![Block::paragraph("B")];
        let document = document_with(vec![Block::Table { table }]);
        let bytes = write_udf(&document).unwrap();
        let read = read_udf(&bytes).unwrap();
        match &read.document.blocks[0] {
            Block::Table { table } => {
                assert_eq!(table.rows.len(), 2);
                assert!(table.rows[0].header);
                assert_eq!(table.rows[0].cells[0].colspan, 2);
                assert_eq!(table.rows[0].cells[0].blocks[0].plain_text(), "Başlık");
                assert_eq!(table.rows[1].cells[1].blocks[0].plain_text(), "B");
            }
            other => panic!("expected a table, got {other:?}"),
        }
    }

    #[test]
    fn round_trips_lists_tabs_and_page_setup() {
        let mut document = document_with(vec![Block::Paragraph {
            props: ParaProps {
                list: Some(ListInfo { kind: "number".into(), level: 0, start: 1, marker: "1.".into() }),
                tabs: vec![TabStop { pos_pt: 85.0, align: "right".into() }],
                ..Default::default()
            },
            runs: vec![Run { text: "Madde".into(), ..Default::default() }],
        }]);
        document.page.margin_left_pt = 70.0;
        document.page.orientation = "landscape".into();
        document.title = "Dilekçe".into();
        let bytes = write_udf(&document).unwrap();
        let read = read_udf(&bytes).unwrap();
        assert_eq!(read.document.title, "Dilekçe");
        assert_eq!(read.document.page.margin_left_pt, 70.0);
        assert_eq!(read.document.page.orientation, "landscape");
        match &read.document.blocks[0] {
            Block::Paragraph { props, .. } => {
                assert_eq!(props.list.as_ref().map(|list| list.kind.as_str()), Some("number"));
                assert_eq!(props.tabs.len(), 1);
                assert_eq!(props.tabs[0].pos_pt, 85.0);
                assert_eq!(props.tabs[0].align, "right");
            }
            other => panic!("expected a paragraph, got {other:?}"),
        }
    }

    #[test]
    fn keeps_an_unknown_element_and_warns() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<template>
  <properties><pageFormat width="595" height="842"/></properties>
  <elements>
    <paragraph Alignment="0"><content>Metin</content></paragraph>
    <ozelEtiket id="7"><paragraph><content>İçerik</content></paragraph></ozelEtiket>
  </elements>
</template>"#;
        let mut zip = ZipWriter::new();
        zip.add_text("content.xml", xml);
        let read = read_udf(&zip.finish()).unwrap();
        assert!(
            read.warnings.iter().any(|warning| warning.contains("ozelEtiket")),
            "the unknown element must be reported: {:?}",
            read.warnings
        );
        assert_eq!(read.document.preserved_udf.len(), 1);
        assert_eq!(read.document.preserved_udf[0].name, "ozelEtiket");
        // The element's own content is kept verbatim, so nothing inside it is lost.
        assert!(read.document.preserved_udf[0].xml.contains("İçerik"), "xml: {}", read.document.preserved_udf[0].xml);
        // And it is written back verbatim on export.
        let bytes = write_udf(&read.document).unwrap();
        let again = read_udf(&bytes).unwrap();
        assert_eq!(again.document.preserved_udf.len(), 1);
        assert!(again.document.preserved_udf[0].xml.contains("ozelEtiket"));
    }

    #[test]
    fn reads_an_rtf_fragment_as_plain_text() {
        let rtf = r"{\rtf1\ansi\deff0 Bu bir \b ornek\b0 paragraf\par}";
        let encoded = base64::engine::general_purpose::STANDARD.encode(rtf.as_bytes());
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<template><elements><paragraph><content type="rtf" encoding="base64">{encoded}</content></paragraph></elements></template>"#
        );
        let mut zip = ZipWriter::new();
        zip.add_text("content.xml", &xml);
        let read = read_udf(&zip.finish()).unwrap();
        assert!(read.warnings.iter().any(|warning| warning.contains("RTF fragment")), "warnings: {:?}", read.warnings);
        let text = read.document.plain_text();
        assert!(text.contains("Bu bir"), "text: {text}");
        assert!(text.contains("ornek"), "text: {text}");
    }

    #[test]
    fn reads_a_package_without_an_elements_wrapper() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<template><paragraph Alignment="2"><content>Sağa yaslı</content></paragraph></template>"#;
        let mut zip = ZipWriter::new();
        zip.add_text("content.xml", xml);
        let read = read_udf(&zip.finish()).unwrap();
        match &read.document.blocks[0] {
            Block::Paragraph { props, runs } => {
                assert_eq!(props.align, "right");
                assert_eq!(runs[0].text, "Sağa yaslı");
            }
            other => panic!("expected a paragraph, got {other:?}"),
        }
    }

    #[test]
    fn rejects_a_package_without_content_xml() {
        let mut zip = ZipWriter::new();
        zip.add_text("other.txt", "not a document");
        let error = read_udf(&zip.finish()).unwrap_err();
        assert!(error.to_string().contains("content.xml"), "{error}");
    }

    #[test]
    fn reads_numeric_and_named_alignment_and_rgb_colours() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<template><elements><paragraph Alignment="1"><content Foreground="255,0,0" Background="0,255,0" Bold="true">Renkli</content></paragraph></elements></template>"#;
        let mut zip = ZipWriter::new();
        zip.add_text("content.xml", xml);
        let read = read_udf(&zip.finish()).unwrap();
        match &read.document.blocks[0] {
            Block::Paragraph { props, runs } => {
                assert_eq!(props.align, "center");
                assert_eq!(runs[0].color.as_deref(), Some("#FF0000"));
                assert_eq!(runs[0].highlight.as_deref(), Some("#00FF00"));
                assert!(runs[0].bold);
            }
            other => panic!("expected a paragraph, got {other:?}"),
        }
    }
}

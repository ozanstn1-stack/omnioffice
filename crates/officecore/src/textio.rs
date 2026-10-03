//! Plain text, Markdown and HTML representations of Writer documents.
//!
//! These conversions are used for TXT export, quick previews and printing.
//! They are intentionally lossy (text formats cannot carry page layout) and
//! never the primary save format.

use crate::model::*;
use crate::xml::escape_text;

fn run_markdown(run: &Run) -> String {
    if run.text.is_empty() {
        return String::new();
    }
    let mut text = run.text.clone();
    if run.bold {
        text = format!("**{text}**");
    }
    if run.italic {
        text = format!("*{text}*");
    }
    if run.strike {
        text = format!("~~{text}~~");
    }
    if let Some(url) = &run.link {
        text = format!("[{text}]({url})");
    }
    text
}

fn table_to_markdown(table: &TableData) -> String {
    let mut out = String::new();
    for (index, row) in table.rows.iter().enumerate() {
        let cells: Vec<String> = row
            .cells
            .iter()
            .map(|cell| cell.blocks.iter().map(block_to_markdown).collect::<Vec<_>>().join(" ").replace('\n', " "))
            .collect();
        out.push_str("| ");
        out.push_str(&cells.join(" | "));
        out.push_str(" |\n");
        if index == 0 {
            out.push('|');
            for _ in &cells {
                out.push_str(" --- |");
            }
            out.push('\n');
        }
    }
    out
}

fn block_to_markdown(block: &Block) -> String {
    match block {
        Block::Paragraph { props, runs } => {
            let text: String = runs.iter().map(run_markdown).collect();
            if let Some(list) = &props.list {
                let indent = "  ".repeat(list.level.min(6) as usize);
                let marker = if list.kind == "number" { "1." } else { "-" };
                format!("{indent}{marker} {text}")
            } else {
                match props.style.as_str() {
                    "Title" => format!("# {text}"),
                    "Subtitle" => format!("## {text}"),
                    style if style.starts_with("Heading") => {
                        let level = style.trim_start_matches("Heading").parse::<usize>().unwrap_or(1).clamp(1, 6);
                        format!("{} {}", "#".repeat(level), text)
                    }
                    "Quote" => format!("> {text}"),
                    _ => text,
                }
            }
        }
        Block::Table { table } => table_to_markdown(table),
        Block::Image { caption, image, .. } => {
            let alt = if caption.is_empty() { image.alt.clone() } else { caption.clone() };
            format!("![{alt}](embedded-image.{})", image.extension())
        }
        Block::PageBreak => "\n---\n".into(),
        Block::SectionBreak { .. } => "\n---\n".into(),
        Block::Rule => "---".into(),
        Block::Toc { entries } => entries
            .iter()
            .map(|entry| {
                let indent = "  ".repeat(entry.level.saturating_sub(1) as usize);
                if entry.page > 0 {
                    format!("{indent}- {} .... {}", entry.text, entry.page)
                } else {
                    format!("{indent}- {}", entry.text)
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// Notes in reference order as `(number, text)` pairs, for the formats that
/// cannot carry real note objects (Markdown, HTML, plain text).
fn document_notes(document: &TextDocument) -> Vec<(usize, String)> {
    let mut notes = Vec::new();
    for id in document.footnote_order() {
        if let Some(note) = document.footnotes.iter().find(|note| note.id == id) {
            let text = note.runs.iter().map(|run| run.text.as_str()).collect::<Vec<_>>().join("");
            notes.push((notes.len() + 1, text));
        }
    }
    for id in document.endnote_order() {
        if let Some(note) = document.endnotes.iter().find(|note| note.id == id) {
            let text = note.runs.iter().map(|run| run.text.as_str()).collect::<Vec<_>>().join("");
            notes.push((notes.len() + 1, text));
        }
    }
    notes
}

pub fn document_to_markdown(document: &TextDocument) -> String {
    let body = document.blocks.iter().map(block_to_markdown).collect::<Vec<_>>().join("\n\n");
    let notes = document_notes(document);
    if notes.is_empty() {
        return body;
    }
    let list = notes.iter().map(|(number, text)| format!("{number}. {text}")).collect::<Vec<_>>().join("\n");
    format!("{body}\n\n---\n\n{list}\n")
}

pub fn document_to_text(document: &TextDocument) -> String {
    let mut out = String::new();
    for block in &document.blocks {
        match block {
            Block::Paragraph { runs, .. } => {
                out.push_str(&runs.iter().map(|run| run.text.as_str()).collect::<Vec<_>>().join(""));
                out.push('\n');
            }
            Block::Table { table } => {
                for row in &table.rows {
                    let cells: Vec<String> = row
                        .cells
                        .iter()
                        .map(|cell| cell.blocks.iter().map(Block::plain_text).collect::<Vec<_>>().join(" "))
                        .collect();
                    out.push_str(&cells.join("\t"));
                    out.push('\n');
                }
            }
            Block::PageBreak => out.push('\n'),
            _ => {}
        }
    }
    while out.ends_with("\n\n\n") {
        out.pop();
    }
    let notes = document_notes(document);
    if !notes.is_empty() {
        out.push_str("\n\nNotes\n");
        for (number, text) in notes {
            out.push_str(&format!("{number}. {text}\n"));
        }
    }
    out.trim_end().to_string()
}

fn run_html(run: &Run) -> String {
    if run.text.is_empty() {
        return String::new();
    }
    let mut text = escape_text(&run.text).replace('\n', "<br/>");
    if run.bold {
        text = format!("<strong>{text}</strong>");
    }
    if run.italic {
        text = format!("<em>{text}</em>");
    }
    if run.underline {
        text = format!("<u>{text}</u>");
    }
    if run.strike {
        text = format!("<s>{text}</s>");
    }
    let mut styles = Vec::new();
    if let Some(color) = &run.color {
        styles.push(format!("color:{}", safe_color(color)));
    }
    if let Some(highlight) = &run.highlight {
        styles.push(format!("background:{}", safe_color(highlight)));
    }
    if let Some(font) = &run.font {
        styles.push(format!("font-family:'{}'", escape_text(font).replace('\'', "")));
    }
    if let Some(size) = run.size_pt {
        styles.push(format!("font-size:{size}pt"));
    }
    if run.superscript {
        styles.push("vertical-align:super;font-size:75%".into());
    }
    if run.subscript {
        styles.push("vertical-align:sub;font-size:75%".into());
    }
    if !styles.is_empty() {
        text = format!("<span style=\"{}\">{text}</span>", styles.join(";"));
    }
    if let Some(url) = &run.link {
        text = format!("<a href=\"{}\" target=\"_blank\" rel=\"noreferrer\">{text}</a>", escape_text(url));
    }
    text
}

fn safe_color(value: &str) -> String {
    let cleaned: String = value.chars().filter(|ch| ch.is_ascii_hexdigit() || *ch == '#').collect();
    if cleaned.starts_with('#') && (cleaned.len() == 4 || cleaned.len() == 7) {
        cleaned
    } else {
        "#000000".into()
    }
}

fn block_html(block: &Block) -> String {
    match block {
        Block::Paragraph { props, runs } => {
            let content: String = runs.iter().map(run_html).collect();
            let content = if content.is_empty() { "<br/>".to_string() } else { content };
            let tag = match props.style.as_str() {
                "Title" => "h1",
                "Subtitle" => "h2",
                style if style.starts_with("Heading") => {
                    let level = style.trim_start_matches("Heading").parse::<usize>().unwrap_or(1).clamp(1, 6);
                    match level {
                        1 => "h1",
                        2 => "h2",
                        3 => "h3",
                        4 => "h4",
                        5 => "h5",
                        _ => "h6",
                    }
                }
                _ => "p",
            };
            let mut styles = Vec::new();
            if props.align != "left" {
                styles.push(format!("text-align:{}", props.align));
            }
            if props.indent_left_pt > 0.0 {
                styles.push(format!("margin-left:{}pt", props.indent_left_pt));
            }
            if props.space_before_pt > 0.0 {
                styles.push(format!("margin-top:{}pt", props.space_before_pt));
            }
            if props.space_after_pt > 0.0 {
                styles.push(format!("margin-bottom:{}pt", props.space_after_pt));
            }
            if props.line_spacing > 0.0 {
                styles.push(format!("line-height:{}", props.line_spacing));
            }
            let style_attr = if styles.is_empty() { String::new() } else { format!(" style=\"{}\"", styles.join(";")) };
            if let Some(list) = &props.list {
                let kind = if list.kind == "number" { "ol" } else { "ul" };
                let indent = if list.level > 0 {
                    format!(" style=\"margin-left:{}px\"", list.level * 24)
                } else {
                    String::new()
                };
                return format!("<{kind}{indent}><li>{content}</li></{kind}>");
            }
            format!("<{tag}{style_attr}>{content}</{tag}>")
        }
        Block::Table { table } => {
            let mut out = String::from("<table>");
            for row in &table.rows {
                out.push_str("<tr>");
                for cell in &row.cells {
                    let span = if cell.colspan > 1 { format!(" colspan=\"{}\"", cell.colspan) } else { String::new() };
                    let mut styles = Vec::new();
                    if let Some(background) = &cell.background {
                        styles.push(format!("background:{}", safe_color(background)));
                    }
                    if cell.align != "left" && !cell.align.is_empty() {
                        styles.push(format!("text-align:{}", cell.align));
                    }
                    let style_attr =
                        if styles.is_empty() { String::new() } else { format!(" style=\"{}\"", styles.join(";")) };
                    let content: String = cell.blocks.iter().map(block_html).collect();
                    out.push_str(&format!("<td{span}{style_attr}>{content}</td>"));
                }
                out.push_str("</tr>");
            }
            out.push_str("</table>");
            out
        }
        Block::Image { image, width_pt, height_pt, caption, .. } => {
            let _ = crate::io::normalize_hex("#000000");
            format!(
                "<figure><img src=\"data:{};base64,{}\" style=\"width:{}pt;height:{}pt;object-fit:contain\" alt=\"{}\"/>{}</figure>",
                escape_text(&image.mime),
                image.data_base64,
                width_pt.max(20.0),
                height_pt.max(20.0),
                escape_text(&image.alt),
                if caption.is_empty() { String::new() } else { format!("<figcaption>{}</figcaption>", escape_text(caption)) }
            )
        }
        Block::PageBreak => "<div class=\"page-break\"></div>".into(),
        Block::SectionBreak { .. } => "<div class=\"page-break\"></div>".into(),
        Block::Rule => "<hr/>".into(),
        Block::Toc { entries } => {
            let items = entries
                .iter()
                .map(|entry| {
                    format!(
                        "<li class=\"toc-level-{}\">{}{}</li>",
                        entry.level.clamp(1, 6),
                        escape_text(&entry.text),
                        if entry.page > 0 {
                            format!(" <span class=\"toc-page\">{}</span>", entry.page)
                        } else {
                            String::new()
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join("");
            format!("<nav class=\"toc\"><ul>{items}</ul></nav>")
        }
    }
}

/// Standalone HTML used for previews and the print pipeline.
pub fn document_to_html(document: &TextDocument) -> String {
    let mut body: String = document.blocks.iter().map(block_html).collect();
    let notes = document_notes(document);
    if !notes.is_empty() {
        let items = notes
            .iter()
            .map(|(number, text)| format!("<li value=\"{number}\">{}</li>", escape_text(text)))
            .collect::<Vec<_>>()
            .join("");
        body.push_str(&format!("<section class=\"notes\"><h2>Notes</h2><ol>{items}</ol></section>"));
    }
    let header = if document.header.is_empty() {
        String::new()
    } else {
        format!("<header>{}</header>", document.header.iter().map(block_html).collect::<String>())
    };
    let footer = if document.footer.is_empty() {
        String::new()
    } else {
        format!("<footer>{}</footer>", document.footer.iter().map(block_html).collect::<String>())
    };
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"/><title>{}</title><style>{}</style></head><body><article>{header}{body}{footer}</article></body></html>",
        escape_text(&document.title),
        HTML_STYLE
    )
}

const HTML_STYLE: &str = "body{margin:0;background:#e9edf2;font-family:'Segoe UI',system-ui,sans-serif;color:#1f2328}article{background:#fff;max-width:760px;margin:24px auto;padding:56px;box-shadow:0 2px 14px rgba(15,23,42,.14)}h1{font-size:26pt;margin:0 0 8pt}h2{font-size:18pt}h3{font-size:14pt}p{margin:0 0 8pt}table{border-collapse:collapse;margin:8pt 0;width:100%}td{border:1px solid #94a3b8;padding:5pt 7pt;vertical-align:top}img{max-width:100%}figure{margin:10pt 0;text-align:center}figcaption{font-size:9pt;color:#64748b}.page-break{page-break-after:always;border-top:1px dashed #cbd5e1;margin:18pt 0}header,footer{color:#64748b;font-size:9.5pt;margin-bottom:12pt}@media print{body{background:#fff}article{box-shadow:none;margin:0;padding:0;max-width:none}}";

/// Builds a Writer document from plain text (one paragraph per line).
pub fn text_to_document(text: &str, title: &str) -> TextDocument {
    let mut document = TextDocument::new_blank(title);
    let mut blocks: Vec<Block> = text
        .lines()
        .map(|line| Block::Paragraph {
            props: ParaProps::default(),
            runs: vec![Run { text: line.to_string(), ..Default::default() }],
        })
        .collect();
    if blocks.is_empty() {
        blocks.push(Block::paragraph(""));
    }
    document.blocks = blocks;
    document
}

/// Plain text of a spreadsheet sheet (used by previews and search).
pub fn sheet_to_text(sheet: &Sheet) -> String {
    let mut out = String::new();
    for (address, cell) in &sheet.cells {
        let value = match &cell.value {
            CellValue::Empty => continue,
            CellValue::Number(number) => number.to_string(),
            CellValue::Text(text) => text.clone(),
            CellValue::Bool(value) => value.to_string(),
            CellValue::Error(error) => error.clone(),
        };
        out.push_str(address);
        out.push('\t');
        out.push_str(&value);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> TextDocument {
        let mut document = TextDocument::new_blank("Doc");
        document.blocks = vec![
            Block::heading("Title here", 1),
            Block::Paragraph {
                props: ParaProps::default(),
                runs: vec![
                    Run { text: "plain ".into(), ..Default::default() },
                    Run { text: "bold".into(), bold: true, ..Default::default() },
                ],
            },
            Block::Paragraph {
                props: ParaProps {
                    list: Some(ListInfo { kind: "bullet".into(), level: 0, start: 1, marker: "•".into() }),
                    ..Default::default()
                },
                runs: vec![Run { text: "item".into(), ..Default::default() }],
            },
            Block::Table { table: TableData::simple(2, 2, 400.0) },
        ];
        document
    }

    #[test]
    fn markdown_output() {
        let markdown = document_to_markdown(&document());
        assert!(markdown.contains("# Title here"));
        assert!(markdown.contains("**bold**"));
        assert!(markdown.contains("- item"));
        assert!(markdown.contains("| "));
    }

    #[test]
    fn text_output() {
        let text = document_to_text(&document());
        assert!(text.contains("Title here"));
        assert!(text.contains("plain bold"));
    }

    #[test]
    fn html_is_standalone_and_escaped() {
        let mut doc = TextDocument::new_blank("x");
        doc.blocks = vec![Block::paragraph("<script>alert(1)</script>")];
        let html = document_to_html(&doc);
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(!html.contains("<script>alert"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn text_to_document_lines() {
        let document = text_to_document("one\ntwo\nthree", "T");
        assert_eq!(document.blocks.len(), 3);
        assert_eq!(document.plain_text(), "one\ntwo\nthree");
    }
}

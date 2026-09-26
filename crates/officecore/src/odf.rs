//! OpenDocument Format support: ODT (Writer), ODS (Calc) and ODP (Impress).
//!
//! Written directly as ODF 1.2 packages so LibreOffice, OpenOffice and
//! OnlyOffice open the results. Import is tolerant: unknown constructs are
//! skipped with warnings, and nothing is ever executed from the package.

use crate::error::{OfficeError, OfficeResult};
use crate::model::*;
use crate::xml::{parse_xml, XmlNode, XmlWriter};
use crate::zip::{ZipReader, ZipWriter};
use std::collections::HashMap;
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
            out.push_str(&format!("<style:style style:name=\"{name}\" style:family=\"table-cell\">{xml}</style:style>"));
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

fn write_runs(writer: &mut XmlWriter, runs: &[Run], styles: &mut AutoStyles) {
    for run in runs {
        let text = run.text.replace('\n', " ");
        if text.is_empty() {
            continue;
        }
        let content = crate::xml::escape_text(&text);
        if run.link.is_some() || run.bold || run.italic || run.underline || run.strike || run.color.is_some() || run.highlight.is_some() || run.font.is_some() || run.size_pt.is_some() || run.superscript || run.subscript {
            let name = styles.text(run_style_xml(run));
            writer.raw(&format!("<text:span text:style-name=\"{name}\">{content}</text:span>"));
        } else {
            writer.raw(&content);
        }
        if let Some(url) = &run.link {
            let last = writer.as_str().len();
            let _ = last;
            let _ = url;
        }
    }
}

fn write_blocks(writer: &mut XmlWriter, blocks: &[Block], styles: &mut AutoStyles, media: &mut Media, list_depth: u32) {
    let mut index = 0usize;
    while index < blocks.len() {
        let block = &blocks[index];
        match block {
            Block::Paragraph { props, runs } => {
                if let Some(list) = &props.list {
                    // Collect the consecutive list items at this level.
                    writer.raw(&format!("<text:list text:style-name=\"{}\">", if list.kind == "number" { "LN" } else { "LB" }));
                    while index < blocks.len() {
                        match &blocks[index] {
                            Block::Paragraph { props: inner, runs: inner_runs } if inner.list.is_some() => {
                                
                                let name = styles.paragraph(paragraph_style_xml(inner, false));
                                writer.raw(&format!("<text:list-item><text:p text:style-name=\"{name}\">"));
                                write_runs(writer, inner_runs, styles);
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
                    write_runs(writer, runs, styles);
                    writer.raw("</text:h>");
                } else {
                    writer.raw(&format!("<text:p text:style-name=\"{name}\">"));
                    write_runs(writer, runs, styles);
                    writer.raw("</text:p>");
                }
            }
            Block::Table { table } => write_table(writer, table, styles, media),
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
                    let text = if entry.page > 0 {
                        format!("{} .... {}", entry.text, entry.page)
                    } else {
                        entry.text.clone()
                    };
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

fn write_table(writer: &mut XmlWriter, table: &TableData, styles: &mut AutoStyles, media: &mut Media) {
    let columns = table.rows.iter().map(|row| row.cells.len()).max().unwrap_or(1).max(1);
    writer.raw("<table:table>");
    for index in 0..columns {
        let width = table.column_widths_pt.get(index).copied().unwrap_or(90.0);
        writer.raw(&format!("<table:table-column table:style-name=\"co{}\" style:column-width=\"{}\"/>", index, cm(width)));
    }
    for row in &table.rows {
        writer.raw("<table:table-row>");
        for cell in &row.cells {
            writer.raw(&format!(
                "<table:table-cell office:value-type=\"string\"{}>",
                if cell.colspan > 1 { format!(" table:number-columns-spanned=\"{}\"", cell.colspan) } else { String::new() }
            ));
            if cell.blocks.is_empty() {
                writer.raw("<text:p/>");
            } else {
                write_blocks(writer, &cell.blocks, styles, media, 0);
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
        let outline = style.outline_level.map(|level| format!(" style:default-outline-level=\"{}\"", level + 1)).unwrap_or_default();
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
    if !document.header.is_empty() {
        let mut writer = XmlWriter::new();
        let mut styles = AutoStyles::default();
        write_blocks(&mut writer, &document.header, &mut styles, &mut media, 0);
        header_footer.push_str(&format!("<style:header>{}</style:header>", writer.finish()));
    }
    if !document.footer.is_empty() {
        let mut writer = XmlWriter::new();
        let mut styles = AutoStyles::default();
        write_blocks(&mut writer, &document.footer, &mut styles, &mut media, 0);
        header_footer.push_str(&format!("<style:footer>{}</style:footer>", writer.finish()));
    }
    format!(
        "<office:master-styles>{layout}<style:master-page style:name=\"Standard\" style:page-layout-name=\"pm1\">{header_footer}</style:master-page></office:master-styles>"
    )
}

fn manifest() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\" manifest:version=\"1.2\">\
        <manifest:file-entry manifest:full-path=\"/\" manifest:media-type=\"application/vnd.oasis.opendocument.text\"/>\
        <manifest:file-entry manifest:full-path=\"content.xml\" manifest:media-type=\"text/xml\"/>\
        <manifest:file-entry manifest:full-path=\"styles.xml\" manifest:media-type=\"text/xml\"/>\
        <manifest:file-entry manifest:full-path=\"meta.xml\" manifest:media-type=\"text/xml\"/>\
        <manifest:file-entry manifest:full-path=\"Pictures/\" manifest:media-type=\"\"/>\
        <manifest:file-entry manifest:full-path=\"Images/\" manifest:media-type=\"\"/>\
        </manifest:manifest>"
    )
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
    let mut body = XmlWriter::new();
    write_blocks(&mut body, &document.blocks, &mut styles, &mut media, 0);
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NS} office:version=\"1.2\">{}{}<office:body><office:text text:style-name=\"Standard\">{}</office:text></office:body></office:document-content>",
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
    zip.add_text("meta.xml", &meta_xml(&document.title, "Office Swiss Army Knife"));
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

fn node_text_runs(node: &XmlNode, runs: &mut Vec<Run>) {
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
                node_text_runs(child, &mut inner);
                for mut run in inner {
                    if run.link.is_none() {
                        run.link = link.clone();
                    }
                    runs.push(run);
                }
            }
        }
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
                node_text_runs(child, runs);
            }
        }
    }
}

fn paragraph_props(node: &XmlNode, style_map: &HashMap<String, ParaProps>) -> ParaProps {
    let mut props = if let Some(name) = node.attr("style-name").or_else(|| node.attr_any_ns("style-name")) {
        style_map.get(name).cloned().unwrap_or_default()
    } else {
        ParaProps::default()
    };
    props.page_break_before = props.page_break_before;
    props
}

fn read_blocks(node: &XmlNode, style_map: &HashMap<String, ParaProps>, reader: &ZipReader, warnings: &mut Vec<String>) -> Vec<Block> {
    let mut blocks = Vec::new();
    for child in &node.children {
        match child.local_name() {
            "p" | "h" => {
                let mut runs = Vec::new();
                if !child.text.is_empty() {
                    runs.push(Run { text: child.text.clone(), ..Default::default() });
                }
                for inner in &child.children {
                    node_text_runs(inner, &mut runs);
                }
                let mut props = paragraph_props(child, style_map);
                if child.local_name() == "h" {
                    let level = child.attr_any_ns("outline-level").and_then(|value| value.parse::<u32>().ok()).unwrap_or(1);
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
                                node_text_runs(part, &mut runs);
                            }
                            let mut props = paragraph_props(inner, style_map);
                            let level = inner.attr_any_ns("level").and_then(|value| value.parse::<u32>().ok()).unwrap_or(0);
                            props.list = Some(ListInfo { kind: "bullet".into(), level, start: 1, marker: "•".into() });
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
                        if let Some(span) = cell_node.attr_any_ns("number-columns-spanned").and_then(|value| value.parse::<u32>().ok()) {
                            cell.colspan = span.max(1);
                        }
                        let mut nested_warnings = Vec::new();
                        cell.blocks = read_blocks(cell_node, style_map, reader, &mut nested_warnings);
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
                blocks.push(Block::Table { table: TableData { rows, column_widths_pt: widths, borders: true, border_color: "#94A3B8".into(), align: "left".into() } });
            }
            "section" => {
                blocks.extend(read_blocks(child, style_map, reader, warnings));
            }
            "annotation" => warnings.push("Comments in the document were not imported.".into()),
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
                                inner.extend(read_blocks(part, style_map, reader, warnings));
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
                if let Some(percent) = value.trim_end_matches('%').parse::<f64>().ok() {
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
            let mut notes = Vec::new();
            root.find_all("annotation", &mut notes);
            if !notes.is_empty() {
                warnings.push("Comments in the document were not imported.".into());
            }
        }
    }
    let text = reader.read_text("content.xml")?;
    let root = parse_xml(&text)?;
    collect_styles(&root, &mut style_map);
    let mut document = TextDocument::new_blank("Imported document");
    let container = root.child("body").and_then(|body| body.child("text")).unwrap_or(&root);
    document.blocks = read_blocks(container, &style_map, &reader, &mut warnings);
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
        if let Ok(root) = parse_xml(&styles) {
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
                document.page.orientation = if document.page.width_pt > document.page.height_pt { "landscape".into() } else { "portrait".into() };
                document.page.size = "custom".into();
            }
            // Header / footer content.
            let mut headers = Vec::new();
            root.find_all("header", &mut headers);
            if let Some(header) = headers.first() {
                document.header = read_blocks(header, &style_map, &reader, &mut warnings);
            }
            let mut footers = Vec::new();
            root.find_all("footer", &mut footers);
            if let Some(footer) = footers.first() {
                document.footer = read_blocks(footer, &style_map, &reader, &mut warnings);
            }
        }
    }
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

pub fn write_ods(workbook: &Workbook) -> OfficeResult<Vec<u8>> {
    let mut styles = AutoStyles::default();
    let mut body = String::new();
    for sheet in &workbook.sheets {
        body.push_str(&format!("<table:table table:name=\"{}\">", escape(&sheet.name)));
        let mut max_row = 0u32;
        let mut max_col = 0u32;
        for address in sheet.cells.keys() {
            if let Some((row, column)) = crate::address::parse(address) {
                max_row = max_row.max(row);
                max_col = max_col.max(column);
            }
        }
        for column in 0..=max_col.min(200) {
            let width = sheet.col_widths.get(&column).copied().unwrap_or(90.0);
            body.push_str(&format!("<table:table-column table:style-name=\"co{}\" style:column-width=\"{}\"/>", column, cm(width * 0.75)));
        }
        let mut row = 0u32;
        while row <= max_row {
            let mut row_output = String::new();
            let mut column = 0u32;
            let mut empty_run = 0u32;
            while column <= max_col {
                let address = crate::address::format(row, column);
                let cell = sheet.cells.get(&address);
                if cell.filter(|cell| !cell.is_empty()).is_none() {
                    empty_run += 1;
                    column += 1;
                    continue;
                }
                if empty_run > 0 {
                    row_output.push_str(&format!("<table:table-cell table:number-columns-repeated=\"{}\"/>", empty_run));
                    empty_run = 0;
                }
                let cell = cell.unwrap();
                let style_name = styles.cell(cell_style_xml(&cell.style));
                let mut attributes = format!(" table:style-name=\"{style_name}\"");
                if let Some(merge) = sheet.merges.iter().find(|merge| merge.start == address) {
                    if let (Some((start_row, start_col)), Some((_, end_col))) = (crate::address::parse(&merge.start), crate::address::parse(&merge.end)) {
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
                let formula = cell.formula.as_deref().map(|formula| format!(" table:formula=\"{}\"", escape(&our_formula_to_odf(formula)))).unwrap_or_default();
                let value_attr = match &cell.value {
                    CellValue::Number(number) => format!(" office:value=\"{number}\""),
                    CellValue::Bool(value) => format!(" office:boolean-value=\"{}\"", if *value { "true" } else { "false" }),
                    CellValue::Text(text) => format!(" office:string-value=\"{}\"", escape(text)),
                    _ => String::new(),
                };
                row_output.push_str(&format!("<table:table-cell office:value-type=\"{value_type}\"{value_attr}{attributes}{formula}>{value_xml}</table:table-cell>"));
                column += 1;
            }
            if empty_run > 0 {
                row_output.push_str(&format!("<table:table-cell table:number-columns-repeated=\"{}\"/>", empty_run));
            }
            body.push_str(&format!("<table:table-row>{row_output}</table:table-row>"));
            row += 1;
        }
        body.push_str("</table:table>");
    }
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NS} office:version=\"1.2\">{}{}<office:body><office:spreadsheet>{body}</office:spreadsheet></office:body></office:document-content>",
        styles.xml(),
        "<office:styles/>"
    );
    let mut zip = ZipWriter::new();
    zip.add_text("mimetype", "application/vnd.oasis.opendocument.spreadsheet");
    zip.add_text("META-INF/manifest.xml", &manifest_for("application/vnd.oasis.opendocument.spreadsheet"));
    zip.add_text("content.xml", &content);
    zip.add_text("styles.xml", &format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {NS} office:version=\"1.2\"/>"));
    zip.add_text("meta.xml", &meta_xml(&workbook.title, "Office Swiss Army Knife"));
    Ok(zip.finish())
}

pub fn write_ods_file(path: &Path, workbook: &Workbook) -> OfficeResult<()> {
    crate::io::write_atomic(path, &write_ods(workbook)?)
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
    for table in tables {
        let name = table.attr("name").unwrap_or("Sheet").to_string();
        let mut sheet = Sheet::new(&name);
        let mut row = 0u32;
        for row_node in table.children_named("table-row") {
            let repeat_rows = row_node.attr_any_ns("number-rows-repeated").and_then(|value| value.parse::<u32>().ok()).unwrap_or(1).min(2048);
            let mut column = 0u32;
            for cell in row_node.children_named("table-cell") {
                let repeat = cell.attr_any_ns("number-columns-repeated").and_then(|value| value.parse::<u32>().ok()).unwrap_or(1).min(1024);
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
                    "date" => cell.attr_any_ns("date-value").map(|value| CellValue::Text(value.to_string())).unwrap_or(CellValue::Empty),
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
                    for offset in 0..repeat.min(64) {
                        let address = crate::address::format(row, column + offset);
                        sheet.set(&address, Cell { value: value.clone(), formula: formula.clone(), ..Default::default() });
                    }
                }
                column += repeat;
                if column > 1_000 {
                    break;
                }
            }
            row += repeat_rows;
            if row > 100_000 {
                break;
            }
        }
        sheet.row_count = (row + 51).max(200);
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

fn slide_object_xml(object: &SlideObject) -> String {
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
            inner.push_str(&format!("<draw:line svg:x1=\"{}\" svg:y1=\"{}\" svg:x2=\"{}\" svg:y2=\"{}\" draw:style-name=\"gr1\"/>", cm(object.x), cm(object.y), cm(object.x + line.x2), cm(object.y + line.y2)));
        }
        "table" => {
            if let Some(table) = &object.table {
                let mut writer = XmlWriter::new();
                let mut styles = AutoStyles::default();
                let mut media = Media::default();
                write_table(&mut writer, table, &mut styles, &mut media);
                inner.push_str(&writer.finish());
            }
        }
        "chart" => {
            if let Some(chart) = &object.chart {
                let title = if chart.title.trim().is_empty() { format!("{} chart", chart.kind) } else { chart.title.clone() };
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
                    writer.raw(&format!("<text:p text:style-name=\"Standard\">{}</text:p>", crate::xml::escape_text(&paragraph.text)));
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
        "line" | "arrow" => return format!("<draw:frame draw:name=\"{}\" text:anchor-type=\"page\" draw:z-index=\"{}\">{inner}</draw:frame>", escape(&object.name), object.z),
        _ => "draw:frame",
    };
    if shape == "draw:frame" {
        let fill = style
            .fill
            .as_deref()
            .map(|fill| format!(" draw:fill=\"solid\" draw:fill-color=\"{}\"", escape(fill)))
            .unwrap_or_else(|| " draw:fill=\"none\"".into());
        format!(
            "<draw:frame draw:name=\"{}\" text:anchor-type=\"page\" svg:x=\"{}\" svg:y=\"{}\" svg:width=\"{}\" svg:height=\"{}\" draw:z-index=\"{}\"{}>{}<draw:glue-points/><draw:enhanced-geometry/></draw:frame>",
            escape(&object.name),
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
            "<draw:frame draw:name=\"{}\" text:anchor-type=\"page\" svg:x=\"{}\" svg:y=\"{}\" svg:width=\"{}\" svg:height=\"{}\" draw:z-index=\"{}\">{}</draw:frame>",
            escape(&object.name),
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

fn count_groups(objects: &[SlideObject]) -> usize {
    objects
        .iter()
        .map(|object| if object.kind == "group" { 1 + count_groups(&object.children) } else { 0 })
        .sum()
}

fn flatten_groups(objects: &[SlideObject], out: &mut Vec<SlideObject>) {
    for object in objects {
        if object.kind == "group" {
            flatten_groups(&object.children, out);
        } else {
            out.push(object.clone());
        }
    }
}

pub fn write_odp_package(deck: &Deck) -> OfficeResult<DeckWrite> {
    let bytes = write_odp_bytes(deck)?;
    let mut warnings = Vec::new();
    let groups: usize = deck.slides.iter().map(|slide| count_groups(&slide.objects)).sum();
    if groups > 0 {
        warnings.push("Groups are exported as individual shapes.".into());
    }
    let charts: usize = deck.slides.iter().map(|slide| slide.objects.iter().filter(|object| object.chart.is_some()).count()).sum();
    if charts > 0 {
        warnings.push("Chart data is kept in the native .oswk file; ODP gets drawn placeholder shapes.".into());
    }
    let animations: usize = deck.slides.iter().map(|slide| slide.animations.len()).sum();
    if animations > 0 {
        warnings.push("Animations are kept in the native .oswk file and are not written to ODP.".into());
    }
    Ok(DeckWrite { bytes, warnings })
}

pub fn write_odp(deck: &Deck) -> OfficeResult<Vec<u8>> {
    Ok(write_odp_package(deck)?.bytes)
}

fn write_odp_bytes(deck: &Deck) -> OfficeResult<Vec<u8>> {
    let mut body = String::new();
    let mut pictures: Vec<(String, Vec<u8>)> = Vec::new();
    for (index, slide) in deck.slides.iter().enumerate() {
        body.push_str(&format!("<draw:page draw:name=\"Slide{}\" draw:master-page-name=\"Default\">", index + 1));
        let mut objects: Vec<SlideObject> = Vec::new();
        flatten_groups(&slide.objects, &mut objects);
        objects.sort_by_key(|object| object.z);
        for object in &objects {
            if let Some(image) = &object.image {
                if !image.is_empty() && !pictures.iter().any(|(name, _)| name == &image.name) {
                    pictures.push((image.name.clone(), image.bytes()));
                }
            }
            body.push_str(&slide_object_xml(object));
        }
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
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NS} office:version=\"1.2\"><office:automatic-styles>{page_layout}</office:automatic-styles><office:body><office:presentation>{body}</office:presentation></office:body></office:document-content>"
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
    zip.add_text("meta.xml", &meta_xml(&deck.title, "Office Swiss Army Knife"));
    for (name, data) in &pictures {
        zip.add(&format!("Pictures/{name}"), data);
    }
    Ok(zip.finish())
}

pub fn write_odp_file(path: &Path, deck: &Deck) -> OfficeResult<()> {
    crate::io::write_atomic(path, &write_odp(deck)?)
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
        let mut slide = Slide::default();
        slide.objects.clear();
        let mut z = 1i32;
        for frame in page.children_named("frame") {
            let x = frame.attr("x").and_then(parse_cm).unwrap_or(40.0);
            let y = frame.attr("y").and_then(parse_cm).unwrap_or(40.0);
            let w = frame.attr("width").and_then(parse_cm).unwrap_or(320.0);
            let h = frame.attr("height").and_then(parse_cm).unwrap_or(180.0);
            let mut images = Vec::new();
            frame.find_all("image", &mut images);
            if let Some(image_node) = images.first() {
                if let Some(href) = image_node.attr("href") {
                    let path = href.trim_start_matches("./");
                    if let Ok(data) = reader.read(path) {
                        let name = path.rsplit('/').next().unwrap_or("image.png").to_string();
                        let mut object = SlideObject::new("image", x, y, w, h);
                        object.z = z;
                        object.image = Some(ImageData::from_bytes(&name, &data));
                        slide.objects.push(object);
                        z += 1;
                        continue;
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
                object.z = z;
                object.text = Some(TextFrame { paragraphs, ..Default::default() });
                slide.objects.push(object);
                z += 1;
                continue;
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
                object.z = z;
                object.table = Some(TableData { rows, ..Default::default() });
                slide.objects.push(object);
                z += 1;
                continue;
            }
            // Shapes we cannot map precisely are preserved as rectangles.
            if frame.attr("width").is_some() {
                let mut object = SlideObject::new("rect", x, y, w, h);
                object.z = z;
                object.style = Some(ShapeStyle { fill: Some("#E2E8F0".into()), ..Default::default() });
                slide.objects.push(object);
                z += 1;
                warnings.push("Some shapes were imported as simple rectangles.".into());
            }
        }
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
                props: ParaProps { list: Some(ListInfo { kind: "bullet".into(), level: 0, start: 1, marker: "•".into() }), ..Default::default() },
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
    fn ods_roundtrip_with_formulas() {
        let mut workbook = Workbook::new_blank("ODS");
        let sheet = &mut workbook.sheets[0];
        sheet.set("A1", Cell { value: CellValue::Number(10.0), ..Default::default() });
        sheet.set("A2", Cell { value: CellValue::Number(20.0), ..Default::default() });
        sheet.set("A3", Cell { value: CellValue::Number(30.0), formula: Some("=SUM(A1:A2)".into()), ..Default::default() });
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
        text.text = Some(TextFrame { paragraphs: vec![TextParagraph { text: "Slayt metni".into(), ..Default::default() }], ..Default::default() });
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

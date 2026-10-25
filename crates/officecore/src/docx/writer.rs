//! DOCX writer: package parts, styles, numbering, notes and the document body.
//! The reader lives in `reader`; the public entry points are re-exported by `mod.rs`.

use super::*;

const NS_DECL: &str = concat!(
    "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" ",
    "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" ",
    "xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" ",
    "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" ",
    "xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\" ",
    "xmlns:v=\"urn:schemas-microsoft-com:vml\" ",
    "xmlns:o=\"urn:schemas-microsoft-com:office:office\" ",
    "xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\""
);

fn twips(points: f64) -> i64 {
    (points * 20.0).round() as i64
}

fn half_points(points: f64) -> i64 {
    (points * 2.0).round() as i64
}

fn emu(points: f64) -> i64 {
    (points * 12700.0).round() as i64
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Relationships {
    next_id: usize,
    entries: Vec<(String, String, bool)>,
}

impl Relationships {
    fn new(start: usize) -> Self {
        Self { next_id: start, entries: Vec::new() }
    }

    fn add(&mut self, kind: &str, target: &str, external: bool) -> String {
        let id = format!("rId{}", self.next_id);
        self.next_id += 1;
        self.entries.push((id.clone(), format!("{kind}|{target}"), external));
        id
    }

    fn xml(&self) -> String {
        let mut writer = XmlWriter::new();
        writer.declaration();
        writer.open("Relationships", &[("xmlns", "http://schemas.openxmlformats.org/package/2006/relationships")]);
        for (id, payload, external) in &self.entries {
            let (kind, target) = payload.split_once('|').unwrap_or(("", ""));
            let mut attrs: Vec<(&str, &str)> = vec![("Id", id), ("Type", kind), ("Target", target)];
            if *external {
                attrs.push(("TargetMode", "External"));
            }
            writer.empty("Relationship", &attrs);
        }
        writer.close("Relationships");
        writer.finish()
    }
}

const REL_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const REL_HYPERLINK: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";

#[derive(Default)]
struct Media {
    entries: Vec<(String, Vec<u8>)>,
    index: usize,
}

impl Media {
    fn add(&mut self, image: &ImageData) -> Option<String> {
        if image.is_empty() {
            return None;
        }
        self.index += 1;
        let path = format!("media/image{}.{}", self.index, image.extension());
        self.entries.push((path.clone(), image.bytes()));
        Some(path)
    }
}

const CONTENT_TYPE_HEADER: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml";
const CONTENT_TYPE_FOOTER: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml";

fn content_types(
    header_parts: &[String],
    footer_parts: &[String],
    footnotes: bool,
    endnotes: bool,
    comments: bool,
) -> String {
    let mut writer = XmlWriter::new();
    writer.declaration();
    writer.open("Types", &[("xmlns", "http://schemas.openxmlformats.org/package/2006/content-types")]);
    writer.empty(
        "Default",
        &[("Extension", "rels"), ("ContentType", "application/vnd.openxmlformats-package.relationships+xml")],
    );
    writer.empty("Default", &[("Extension", "xml"), ("ContentType", "application/xml")]);
    for (extension, mime) in [
        ("png", "image/png"),
        ("jpg", "image/jpeg"),
        ("jpeg", "image/jpeg"),
        ("gif", "image/gif"),
        ("bmp", "image/bmp"),
        ("webp", "image/webp"),
        ("tiff", "image/tiff"),
    ] {
        writer.empty("Default", &[("Extension", extension), ("ContentType", mime)]);
    }
    let mut overrides: Vec<(String, &str)> = vec![
        (
            "/word/document.xml".into(),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        ),
        ("/word/styles.xml".into(), "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"),
        ("/word/numbering.xml".into(), "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"),
        ("/word/settings.xml".into(), "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"),
        ("/docProps/core.xml".into(), "application/vnd.openxmlformats-package.core-properties+xml"),
        ("/docProps/app.xml".into(), "application/vnd.openxmlformats-officedocument.extended-properties+xml"),
    ];
    for part in header_parts {
        overrides.push((format!("/word/{part}"), CONTENT_TYPE_HEADER));
    }
    for part in footer_parts {
        overrides.push((format!("/word/{part}"), CONTENT_TYPE_FOOTER));
    }
    if footnotes {
        overrides.push((
            "/word/footnotes.xml".into(),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml",
        ));
    }
    if endnotes {
        overrides.push((
            "/word/endnotes.xml".into(),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml",
        ));
    }
    if comments {
        overrides.push((
            "/word/comments.xml".into(),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
        ));
    }
    for (part, mime) in overrides {
        writer.empty("Override", &[("PartName", &part), ("ContentType", mime)]);
    }
    writer.close("Types");
    writer.finish()
}

fn root_relationships() -> String {
    let mut writer = XmlWriter::new();
    writer.declaration();
    writer.open("Relationships", &[("xmlns", "http://schemas.openxmlformats.org/package/2006/relationships")]);
    writer.empty(
        "Relationship",
        &[
            ("Id", "rId1"),
            ("Type", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument"),
            ("Target", "word/document.xml"),
        ],
    );
    writer.empty(
        "Relationship",
        &[
            ("Id", "rId2"),
            ("Type", "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties"),
            ("Target", "docProps/core.xml"),
        ],
    );
    writer.empty(
        "Relationship",
        &[
            ("Id", "rId3"),
            ("Type", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties"),
            ("Target", "docProps/app.xml"),
        ],
    );
    writer.close("Relationships");
    writer.finish()
}

fn core_properties(document: &TextDocument) -> String {
    let created = if document.metadata.created.is_empty() {
        "1970-01-01T00:00:00Z".to_string()
    } else {
        document.metadata.created.clone()
    };
    let modified =
        if document.metadata.modified.is_empty() { created.clone() } else { document.metadata.modified.clone() };
    let mut writer = XmlWriter::new();
    writer.declaration();
    writer.open(
        "cp:coreProperties",
        &[
            ("xmlns:cp", "http://schemas.openxmlformats.org/package/2006/metadata/core-properties"),
            ("xmlns:dc", "http://purl.org/dc/elements/1.1/"),
            ("xmlns:dcterms", "http://purl.org/dc/terms/"),
            ("xmlns:dcmitype", "http://purl.org/dc/dcmitype/"),
            ("xmlns:xsi", "http://www.w3.org/2001/XMLSchema-instance"),
        ],
    );
    writer.open("dc:title", &[]).text(&document.title).close("dc:title");
    writer.open("dc:creator", &[]).text(&document.metadata.author).close("dc:creator");
    writer.open("dc:subject", &[]).text(&document.metadata.subject).close("dc:subject");
    writer.open("cp:keywords", &[]).text(&document.metadata.keywords).close("cp:keywords");
    writer.open("cp:lastModifiedBy", &[]).text(&document.metadata.last_modified_by).close("cp:lastModifiedBy");
    writer.open("dcterms:created", &[("xsi:type", "dcterms:W3CDTF")]).text(&created).close("dcterms:created");
    writer.open("dcterms:modified", &[("xsi:type", "dcterms:W3CDTF")]).text(&modified).close("dcterms:modified");
    writer.close("cp:coreProperties");
    writer.finish()
}

fn app_properties(document: &TextDocument) -> String {
    let words = document.word_count();
    let mut writer = XmlWriter::new();
    writer.declaration();
    writer.open(
        "Properties",
        &[
            ("xmlns", "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"),
            ("xmlns:vt", "http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"),
        ],
    );
    writer.open("Application", &[]).text("OmniOffice").close("Application");
    writer.open("AppVersion", &[]).text("2.0000").close("AppVersion");
    writer.open("Words", &[]).text(&words.to_string()).close("Words");
    writer.open("Paragraphs", &[]).text(&document.blocks.len().to_string()).close("Paragraphs");
    writer.close("Properties");
    writer.finish()
}

fn write_paragraph_properties(writer: &mut XmlWriter, props: &ParaProps, sect: Option<&str>, num_id: Option<u32>) {
    let mut children: Vec<String> = Vec::new();
    if !props.style.is_empty() && props.style != "Normal" {
        children.push(format!("<w:pStyle w:val=\"{}\"/>", crate::xml::escape_attr(&props.style)));
    }
    if props.keep_with_next {
        children.push("<w:keepNext/>".into());
    }
    if props.keep_together {
        children.push("<w:keepLines/>".into());
    }
    if props.page_break_before {
        children.push("<w:pageBreakBefore/>".into());
    }
    match props.align.as_str() {
        "center" => children.push("<w:jc w:val=\"center\"/>".into()),
        "right" => children.push("<w:jc w:val=\"right\"/>".into()),
        "justify" => children.push("<w:jc w:val=\"both\"/>".into()),
        _ => {}
    }
    if props.space_before_pt != 0.0 || props.space_after_pt != 0.0 {
        children.push(format!(
            "<w:spacing w:before=\"{}\" w:after=\"{}\"/>",
            twips(props.space_before_pt.max(0.0)),
            twips(props.space_after_pt.max(0.0))
        ));
    }
    if props.line_spacing > 0.0 {
        children.push(format!(
            "<w:spacing w:line=\"{}\" w:lineRule=\"auto\"/>",
            (props.line_spacing * 240.0).round() as i64
        ));
    }
    if props.indent_left_pt != 0.0 || props.indent_right_pt != 0.0 || props.first_line_pt != 0.0 {
        let mut attrs = String::new();
        if props.indent_left_pt != 0.0 {
            attrs.push_str(&format!(" w:left=\"{}\"", twips(props.indent_left_pt)));
        }
        if props.indent_right_pt != 0.0 {
            attrs.push_str(&format!(" w:right=\"{}\"", twips(props.indent_right_pt)));
        }
        if props.first_line_pt > 0.0 {
            attrs.push_str(&format!(" w:firstLine=\"{}\"", twips(props.first_line_pt)));
        } else if props.first_line_pt < 0.0 {
            attrs.push_str(&format!(" w:hanging=\"{}\"", twips(-props.first_line_pt)));
        }
        children.push(format!("<w:ind{attrs}/>"));
    }
    let tabs: Vec<String> = props
        .tabs
        .iter()
        .filter_map(|tab| match tab.align.as_str() {
            align @ ("left" | "center" | "right" | "decimal") => {
                Some(format!("<w:tab w:val=\"{align}\" w:pos=\"{}\"/>", twips(tab.pos_pt)))
            }
            _ => None,
        })
        .collect();
    if !tabs.is_empty() {
        children.push(format!("<w:tabs>{}</w:tabs>", tabs.concat()));
    }
    if let Some(list) = &props.list {
        // Bullets share one numbering; every numbered run gets its own
        // `w:num` so Word restarts it at the list's start.
        let resolved = if list.kind == "number" { num_id.unwrap_or(2) } else { 1 };
        children.push(format!(
            "<w:numPr><w:ilvl w:val=\"{}\"/><w:numId w:val=\"{}\"/></w:numPr>",
            list.level.min(8),
            resolved
        ));
    }
    if let Some(sect) = sect {
        children.push(sect.to_string());
    }
    if !children.is_empty() {
        writer.raw("<w:pPr>");
        for child in children {
            writer.raw(&child);
        }
        writer.raw("</w:pPr>");
    }
}

const HIGHLIGHTS: [&str; 16] = [
    "yellow",
    "green",
    "cyan",
    "magenta",
    "blue",
    "red",
    "darkBlue",
    "darkGreen",
    "darkRed",
    "darkYellow",
    "gray",
    "lightGray",
    "black",
    "white",
    "darkGray",
    "none",
];

fn highlight_value(color: &str) -> String {
    let normalized = color.to_ascii_lowercase();
    let simple = normalized.trim_start_matches('#');
    for name in HIGHLIGHTS {
        if name.to_ascii_lowercase() == simple {
            return name.to_string();
        }
    }
    // Map hex colours onto the closest Word highlight bucket.
    match simple {
        "ffff00" => "yellow".into(),
        "00ff00" => "green".into(),
        "00ffff" => "cyan".into(),
        "ff00ff" => "magenta".into(),
        "0000ff" => "blue".into(),
        "ff0000" => "red".into(),
        "" => "none".into(),
        _ => "yellow".into(),
    }
}

/// Shared writer state: relationships, media, note/comment/revision id maps.
#[derive(Default)]
struct DocxState {
    rels: Relationships,
    media: Media,
    id_counter: usize,
    footnote_ids: HashMap<String, u32>,
    endnote_ids: HashMap<String, u32>,
    comment_ids: HashMap<String, u32>,
    revision_ids: HashMap<String, u32>,
    revision_seq: u32,
    /// `w:numId` of the numbered-list run each paragraph belongs to, keyed by
    /// the address of its properties (see [`plan_list_numbering`]).
    list_num_ids: HashMap<usize, u32>,
}

impl DocxState {
    fn new() -> Self {
        Self { rels: Relationships::new(4), ..Default::default() }
    }

    /// Numeric id for a revision, allocated once and reused.
    fn revision_id(&mut self, id: &str) -> u32 {
        if let Some(existing) = self.revision_ids.get(id) {
            return *existing;
        }
        self.revision_seq += 1;
        let value = self.revision_seq;
        self.revision_ids.insert(id.to_string(), value);
        value
    }

    fn note_id(&mut self, key: &str, id: &str) -> u32 {
        let map = if key == "endnote" { &mut self.endnote_ids } else { &mut self.footnote_ids };
        if let Some(existing) = map.get(id) {
            return *existing;
        }
        let value = map.len() as u32 + 1;
        map.insert(id.to_string(), value);
        value
    }

    fn comment_id(&mut self, id: &str) -> Option<u32> {
        self.comment_ids.get(id).copied()
    }
}

fn write_run_properties(writer: &mut XmlWriter, run: &Run, extra: Option<&str>) {
    let mut children: Vec<String> = Vec::new();
    if let Some(font) = &run.font {
        let escaped = crate::xml::escape_attr(font);
        children.push(format!("<w:rFonts w:ascii=\"{escaped}\" w:hAnsi=\"{escaped}\" w:cs=\"{escaped}\"/>"));
    }
    if run.bold {
        children.push("<w:b/>".into());
    }
    if run.italic {
        children.push("<w:i/>".into());
    }
    if run.underline {
        children.push("<w:u w:val=\"single\"/>".into());
    }
    if run.strike {
        children.push("<w:strike/>".into());
    }
    if let Some(color) = run.color.as_deref().and_then(bare_hex) {
        children.push(format!("<w:color w:val=\"{color}\"/>"));
    }
    if let Some(highlight) = &run.highlight {
        children.push(format!("<w:highlight w:val=\"{}\"/>", highlight_value(highlight)));
    }
    if let Some(size) = run.size_pt {
        children.push(format!("<w:sz w:val=\"{}\"/><w:szCs w:val=\"{}\"/>", half_points(size), half_points(size)));
    }
    if run.superscript {
        children.push("<w:vertAlign w:val=\"superscript\"/>".into());
    } else if run.subscript {
        children.push("<w:vertAlign w:val=\"subscript\"/>".into());
    }
    if let Some(extra) = extra {
        children.push(extra.to_string());
    }
    if !children.is_empty() {
        writer.raw("<w:rPr>");
        for child in children {
            writer.raw(&child);
        }
        writer.raw("</w:rPr>");
    }
}

/// The XML of a `w:rPrChange` carrying the formatting before a tracked change.
fn format_revision_xml(state: &mut DocxState, revision: &RevisionMark) -> Option<String> {
    let original = revision.original.as_ref()?;
    let id = state.revision_id(&revision.id);
    let mut properties = Run { text: String::new(), ..Default::default() };
    properties.apply_format(original);
    let mut inner = XmlWriter::new();
    write_run_properties(&mut inner, &properties, None);
    Some(format!(
        "<w:rPrChange w:id=\"{id}\" w:author=\"{}\" w:date=\"{}\">{}</w:rPrChange>",
        crate::xml::escape_attr(&revision.author),
        crate::xml::escape_attr(if revision.date.is_empty() { "1970-01-01T00:00:00Z" } else { &revision.date }),
        inner.finish()
    ))
}

fn write_text_run(state: &mut DocxState, writer: &mut XmlWriter, run: &Run) {
    let revision = run.revision.clone();
    match revision.as_ref().map(|mark| mark.kind.as_str()) {
        Some("delete") => {
            let id = state.revision_id(&revision.as_ref().unwrap().id);
            writer.raw(&format!(
                "<w:del w:id=\"{id}\" w:author=\"{}\" w:date=\"{}\">",
                crate::xml::escape_attr(&revision.as_ref().unwrap().author),
                crate::xml::escape_attr(if revision.as_ref().unwrap().date.is_empty() {
                    "1970-01-01T00:00:00Z"
                } else {
                    &revision.as_ref().unwrap().date
                })
            ));
            write_run_content(state, writer, run, true);
            writer.raw("</w:del>");
        }
        Some("insert") => {
            let id = state.revision_id(&revision.as_ref().unwrap().id);
            writer.raw(&format!(
                "<w:ins w:id=\"{id}\" w:author=\"{}\" w:date=\"{}\">",
                crate::xml::escape_attr(&revision.as_ref().unwrap().author),
                crate::xml::escape_attr(if revision.as_ref().unwrap().date.is_empty() {
                    "1970-01-01T00:00:00Z"
                } else {
                    &revision.as_ref().unwrap().date
                })
            ));
            write_run_content(state, writer, run, false);
            writer.raw("</w:ins>");
        }
        _ => write_run_content(state, writer, run, false),
    }
}

fn write_run_content(state: &mut DocxState, writer: &mut XmlWriter, run: &Run, deleted: bool) {
    if let Some(id) = &run.footnote {
        if let Some(number) = state.footnote_ids.get(id).copied() {
            writer
                .raw("<w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/><w:vertAlign w:val=\"superscript\"/></w:rPr>");
            writer.raw(&format!("<w:footnoteReference w:id=\"{number}\"/>"));
            writer.raw("</w:r>");
            return;
        }
    }
    if let Some(id) = &run.endnote {
        if let Some(number) = state.endnote_ids.get(id).copied() {
            writer
                .raw("<w:r><w:rPr><w:rStyle w:val=\"EndnoteReference\"/><w:vertAlign w:val=\"superscript\"/></w:rPr>");
            writer.raw(&format!("<w:endnoteReference w:id=\"{number}\"/>"));
            writer.raw("</w:r>");
            return;
        }
    }
    if let Some(field) = &run.field {
        write_field_run(state, writer, run, field);
        return;
    }
    // `{{page}}` / `{{pages}}` become real PAGE/NUMPAGES fields so Word and
    // LibreOffice keep them up to date instead of showing a static number.
    if run.text.contains("{{page}}") || run.text.contains("{{pages}}") {
        let mut remaining = run.text.as_str();
        while !remaining.is_empty() {
            let next_page = remaining.find("{{page}}");
            let next_pages = remaining.find("{{pages}}");
            let (index, token) = match (next_page, next_pages) {
                (Some(a), Some(b)) => {
                    if a < b {
                        (a, "{{page}}")
                    } else {
                        (b, "{{pages}}")
                    }
                }
                (Some(a), None) => (a, "{{page}}"),
                (None, Some(b)) => (b, "{{pages}}"),
                (None, None) => {
                    write_plain_run(state, writer, run, remaining, deleted);
                    break;
                }
            };
            if index > 0 {
                write_plain_run(state, writer, run, &remaining[..index], deleted);
            }
            let mut field_run = run.clone();
            field_run.field = Some(FieldRef {
                kind: if token == "{{pages}}" { "pages".into() } else { "page".into() },
                target: String::new(),
                cached: "1".into(),
            });
            write_field_run(state, writer, &field_run, field_run.field.as_ref().unwrap());
            remaining = &remaining[index + token.len()..];
        }
        return;
    }
    write_plain_run(state, writer, run, &run.text, deleted);
}

fn write_plain_run(state: &mut DocxState, writer: &mut XmlWriter, run: &Run, text: &str, deleted: bool) {
    if text.is_empty() {
        return;
    }
    let extra = if !deleted {
        run.revision.as_ref().filter(|mark| mark.kind == "format").and_then(|mark| format_revision_xml(state, mark))
    } else {
        None
    };
    writer.raw("<w:r>");
    write_run_properties(writer, run, extra.as_deref());
    let mut parts = text.split('\n').peekable();
    while let Some(part) = parts.next() {
        if deleted {
            writer.raw("<w:delText xml:space=\"preserve\">");
        } else {
            writer.raw("<w:t xml:space=\"preserve\">");
        }
        writer.text(part);
        writer.raw(if deleted { "</w:delText>" } else { "</w:t>" });
        if parts.peek().is_some() {
            writer.raw("<w:br/>");
        }
    }
    writer.raw("</w:r>");
}

fn field_instruction(field: &FieldRef) -> String {
    match field.kind.as_str() {
        "page" => "PAGE".into(),
        "pages" => "NUMPAGES".into(),
        "date" => "DATE".into(),
        "time" => "TIME".into(),
        "title" => "TITLE".into(),
        "author" => "AUTHOR".into(),
        "ref" | "bookmark" | "figure" | "table" => format!("REF {} \\h", field.target),
        "refPage" => format!("PAGEREF {} \\h", field.target),
        "footnote" => format!("NOTEREF {} \\f", field.target),
        other => other.to_uppercase(),
    }
}

fn write_field_run(state: &mut DocxState, writer: &mut XmlWriter, run: &Run, field: &FieldRef) {
    let instruction = field_instruction(field);
    let cached = if field.cached.is_empty() { "1" } else { field.cached.as_str() };
    for payload in [
        "<w:fldChar w:fldCharType=\"begin\"/>".to_string(),
        format!("<w:instrText xml:space=\"preserve\"> {instruction} </w:instrText>"),
        "<w:fldChar w:fldCharType=\"separate\"/>".to_string(),
    ] {
        writer.raw("<w:r>");
        write_run_properties(writer, run, None);
        writer.raw(&payload);
        writer.raw("</w:r>");
    }
    let mut result = run.clone();
    result.field = None;
    result.text = cached.to_string();
    write_run_content(state, writer, &result, false);
    writer.raw("<w:r>");
    write_run_properties(writer, run, None);
    writer.raw("<w:fldChar w:fldCharType=\"end\"/>");
    writer.raw("</w:r>");
}

fn write_paragraph(state: &mut DocxState, writer: &mut XmlWriter, props: &ParaProps, runs: &[Run], sect: Option<&str>) {
    let num_id = state.list_num_ids.get(&(props as *const ParaProps as usize)).copied();
    writer.raw("<w:p>");
    write_paragraph_properties(writer, props, sect, num_id);
    for run in runs {
        let comment = run.comment.as_deref().and_then(|id| state.comment_id(id));
        if let Some(comment) = comment {
            writer.raw(&format!("<w:commentRangeStart w:id=\"{comment}\"/>"));
        }
        if let Some(url) = &run.link {
            let id = state.rels.add(REL_HYPERLINK, url, true);
            writer.raw(&format!("<w:hyperlink r:id=\"{id}\">"));
            write_text_run(state, writer, run);
            writer.raw("</w:hyperlink>");
        } else {
            write_text_run(state, writer, run);
        }
        if let Some(comment) = comment {
            writer.raw(&format!("<w:commentRangeEnd w:id=\"{comment}\"/>"));
            writer.raw(&format!("<w:r><w:rPr><w:rStyle w:val=\"CommentReference\"/></w:rPr><w:commentReference w:id=\"{comment}\"/></w:r>"));
        }
    }
    writer.raw("</w:p>");
}

fn write_image(
    state: &mut DocxState,
    writer: &mut XmlWriter,
    image: &ImageData,
    width_pt: f64,
    height_pt: f64,
    align: &str,
    wrap: &str,
) {
    let Some(target) = state.media.add(image) else {
        return;
    };
    let id = state.rels.add(REL_IMAGE, &target, false);
    state.id_counter += 1;
    let id_counter = state.id_counter;
    let name = if image.name.is_empty() { format!("Image {}", id_counter) } else { image.name.clone() };
    let width = if width_pt > 1.0 { width_pt } else { 320.0 };
    let height = if height_pt > 1.0 { height_pt } else { 200.0 };
    writer.raw("<w:p><w:r><w:drawing>");
    let anchored = wrap == "square" || wrap == "topBottom";
    if anchored {
        let position = match align {
            "left" => "left",
            "right" => "right",
            _ => "center",
        };
        writer.raw(&format!(
            "<wp:anchor distT=\"0\" distB=\"0\" distL=\"114300\" distR=\"114300\" simplePos=\"0\" relativeHeight=\"{id_counter}\" behindDoc=\"0\" locked=\"0\" layoutInCell=\"1\" allowOverlap=\"1\">"
        ));
        writer.raw("<wp:simplePos x=\"0\" y=\"0\"/>");
        writer.raw(&format!("<wp:positionH relativeFrom=\"column\"><wp:align>{position}</wp:align></wp:positionH>"));
        writer.raw("<wp:positionV relativeFrom=\"paragraph\"><wp:align>top</wp:align></wp:positionV>");
        writer.raw(&format!("<wp:extent cx=\"{}\" cy=\"{}\"/>", emu(width), emu(height)));
        writer.raw("<wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/>");
        writer.raw(if wrap == "topBottom" {
            "<wp:wrapTopAndBottom/>"
        } else {
            "<wp:wrapSquare wrapText=\"bothSides\"/>"
        });
    } else {
        writer.raw(&format!(
            "<wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{}\" cy=\"{}\"/>",
            emu(width),
            emu(height)
        ));
    }
    writer.raw(&format!(
        "<wp:docPr id=\"{}\" name=\"{}\" descr=\"{}\"/>",
        id_counter,
        crate::xml::escape_attr(&name),
        crate::xml::escape_attr(&image.alt)
    ));
    writer.raw("<wp:cNvGraphicFramePr><a:graphicFrameLocks xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" noChangeAspect=\"1\"/></wp:cNvGraphicFramePr>");
    writer.raw("<a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">");
    writer.raw("<a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">");
    writer.raw("<pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">");
    writer.raw(&format!(
        "<pic:nvPicPr><pic:cNvPr id=\"{id_counter}\" name=\"{}\"/><pic:cNvPicPr/></pic:nvPicPr>",
        crate::xml::escape_attr(&name)
    ));
    writer
        .raw(&format!("<pic:blipFill><a:blip r:embed=\"{id}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>",));
    writer.raw(&format!(
        "<pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr>",
        emu(width),
        emu(height)
    ));
    let closer = if anchored { "</wp:anchor>" } else { "</wp:inline>" };
    writer.raw(&format!("</pic:pic></a:graphicData></a:graphic>{closer}</w:drawing></w:r></w:p>"));
}

/// How many grid columns a table spans, counting colspans and the columns a
/// rowspan keeps covered in the rows below it.
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

/// A `w:tc` that continues a vertical merge from the row above: no content,
/// only the merge marker (and the width of the grid column it covers).
fn write_merged_continuation(writer: &mut XmlWriter, width_pt: Option<f64>) {
    writer.raw("<w:tc><w:tcPr>");
    if let Some(width) = width_pt {
        writer.raw(&format!("<w:tcW w:w=\"{}\" w:type=\"dxa\"/>", twips(width)));
    }
    writer.raw("<w:vMerge/></w:tcPr><w:p/></w:tc>");
}

fn write_table(state: &mut DocxState, writer: &mut XmlWriter, table: &TableData, depth: usize) {
    if depth > 4 {
        return;
    }
    let grid_columns = table_grid_columns(table).max(table.column_widths_pt.len()).max(1);
    let mut grid_widths = table.column_widths_pt.clone();
    let fallback =
        if grid_widths.is_empty() { 156.0 } else { grid_widths.iter().sum::<f64>() / grid_widths.len() as f64 };
    while grid_widths.len() < grid_columns {
        grid_widths.push(fallback);
    }
    grid_widths.truncate(grid_columns);
    writer.raw("<w:tbl>");
    writer.raw("<w:tblPr>");
    let total: f64 = grid_widths.iter().sum::<f64>().max(100.0);
    writer.raw(&format!("<w:tblW w:w=\"{}\" w:type=\"dxa\"/>", twips(total)));
    if table.borders {
        let color = bare_hex(&table.border_color).unwrap_or_else(|| "94A3B8".into());
        writer.raw(&format!(
            "<w:tblBorders><w:top w:val=\"single\" w:sz=\"4\" w:color=\"{color}\"/><w:left w:val=\"single\" w:sz=\"4\" w:color=\"{color}\"/><w:bottom w:val=\"single\" w:sz=\"4\" w:color=\"{color}\"/><w:right w:val=\"single\" w:sz=\"4\" w:color=\"{color}\"/><w:insideH w:val=\"single\" w:sz=\"4\" w:color=\"{color}\"/><w:insideV w:val=\"single\" w:sz=\"4\" w:color=\"{color}\"/></w:tblBorders>"
        ));
    }
    match table.align.as_str() {
        "center" => {
            writer.raw("<w:jc w:val=\"center\"/>");
        }
        "right" => {
            writer.raw("<w:jc w:val=\"right\"/>");
        }
        _ => {}
    }
    writer.raw("</w:tblPr>");
    writer.raw("<w:tblGrid>");
    for width in &grid_widths {
        writer.raw(&format!("<w:gridCol w:w=\"{}\"/>", twips(*width)));
    }
    writer.raw("</w:tblGrid>");
    // Rows the model stores contain only the cells that start in them; every
    // grid column a rowspan keeps covered gets an explicit continuation cell.
    let mut open: Vec<usize> = Vec::new();
    for row in &table.rows {
        // `open` counts the rows still covered after the current one.
        for remaining in open.iter_mut() {
            *remaining = remaining.saturating_sub(1);
        }
        writer.raw("<w:tr>");
        if row.header {
            writer.raw("<w:trPr><w:tblHeader/></w:trPr>");
        }
        if let Some(height) = row.height_pt {
            writer.raw(&format!("<w:trPr><w:trHeight w:val=\"{}\"/></w:trPr>", twips(height)));
        }
        let mut column = 0usize;
        for cell in &row.cells {
            while column < open.len() && open[column] > 0 {
                write_merged_continuation(writer, grid_widths.get(column).copied());
                column += 1;
            }
            let span = cell.colspan.max(1) as usize;
            while open.len() < column + span {
                open.push(0);
            }
            writer.raw("<w:tc><w:tcPr>");
            if let Some(width) = cell.width_pt {
                writer.raw(&format!("<w:tcW w:w=\"{}\" w:type=\"dxa\"/>", twips(width)));
            }
            if cell.colspan > 1 {
                writer.raw(&format!("<w:gridSpan w:val=\"{}\"/>", cell.colspan));
            }
            if cell.rowspan > 1 {
                writer.raw("<w:vMerge w:val=\"restart\"/>");
            }
            if let Some(background) = cell.background.as_deref().and_then(bare_hex) {
                writer.raw(&format!("<w:shd w:val=\"clear\" w:fill=\"{background}\"/>"));
            }
            match cell.valign.as_str() {
                "center" => {
                    writer.raw("<w:vAlign w:val=\"center\"/>");
                }
                "bottom" => {
                    writer.raw("<w:vAlign w:val=\"bottom\"/>");
                }
                _ => {}
            }
            writer.raw("</w:tcPr>");
            if cell.blocks.is_empty() {
                write_paragraph(state, writer, &ParaProps::default(), &[], None);
            } else {
                for block in &cell.blocks {
                    write_block(state, writer, block, depth + 1);
                }
            }
            writer.raw("</w:tc>");
            if cell.rowspan > 1 {
                for slot in open.iter_mut().skip(column).take(span) {
                    *slot = cell.rowspan as usize;
                }
            }
            column += span;
        }
        while column < open.len() && open[column] > 0 {
            write_merged_continuation(writer, grid_widths.get(column).copied());
            column += 1;
        }
        writer.raw("</w:tr>");
    }
    writer.raw("</w:tbl>");
    // Word requires a paragraph after a table.
    writer.raw("<w:p/>");
}

fn write_block(state: &mut DocxState, writer: &mut XmlWriter, block: &Block, depth: usize) {
    match block {
        Block::Paragraph { props, runs } => write_paragraph(state, writer, props, runs, None),
        Block::Table { table } => write_table(state, writer, table, depth),
        Block::Image { image, width_pt, height_pt, align, wrap, .. } => {
            write_image(state, writer, image, *width_pt, *height_pt, align, wrap)
        }
        Block::PageBreak => {
            writer.raw("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>");
        }
        Block::Rule => {
            writer.raw("<w:p><w:pPr><w:pBdr><w:bottom w:val=\"single\" w:sz=\"6\" w:space=\"1\" w:color=\"94A3B8\"/></w:pBdr></w:pPr></w:p>");
        }
        Block::Toc { entries } => {
            // Static lines with the page numbers from the last update in the
            // editor; Word will not refresh them, which the export warns about.
            for entry in entries {
                let (props, runs) = crate::layout::toc_entry_line(entry);
                write_paragraph(state, writer, &props, &runs, None);
            }
        }
        // Section breaks are emitted by `write_docx`, which owns the section
        // plan; a break inside a table cell degrades to a page break.
        Block::SectionBreak { .. } => {
            writer.raw("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>");
        }
    }
}

/// The canonical Word 2007 VML text watermark: the `_x0000_t136` shapetype and
/// a shape whose `v:textpath` carries the text, as a default header part
/// stores it. VML's `style` rotation is in degrees, so the model value is
/// written as-is.
fn watermark_paragraph(watermark: &Watermark) -> String {
    let color =
        watermark.color.as_deref().and_then(bare_hex).map(|hex| format!("#{hex}")).unwrap_or_else(|| "#C0C0C0".into());
    let opacity = watermark.opacity.clamp(0.0, 1.0);
    let weight = if watermark.bold { "bold" } else { "normal" };
    let text = crate::xml::escape_attr(&watermark.text);
    format!(
        concat!(
            "<w:p><w:r><w:pict>",
            "<v:shapetype id=\"_x0000_t136\" coordsize=\"21600,21600\" o:spt=\"136\" adj=\"10800\" path=\"m@7,l@8,m@5,21600l@6,21600e\">",
            "<v:formulas>",
            "<v:f eqn=\"sum #0 0 10800\"/><v:f eqn=\"prod #0 2 1\"/><v:f eqn=\"sum 21600 0 @1\"/>",
            "<v:f eqn=\"sum 0 0 @2\"/><v:f eqn=\"sum 21600 0 @3\"/><v:f eqn=\"if @0 @3 0\"/>",
            "<v:f eqn=\"if @0 21600 @1\"/><v:f eqn=\"if @0 0 @2\"/><v:f eqn=\"if @0 @4 21600\"/>",
            "<v:f eqn=\"mid @5 @6\"/><v:f eqn=\"mid @8 @5\"/><v:f eqn=\"mid @7 @8\"/>",
            "<v:f eqn=\"mid @6 @7\"/><v:f eqn=\"sum @6 0 @5\"/>",
            "</v:formulas>",
            "<v:path textpathok=\"t\" o:connecttype=\"custom\" o:connectlocs=\"@9,0;@10,10800;@11,21600;@12,10800\" o:connectangles=\"270,180,90,0\"/>",
            "<v:textpath on=\"t\" fitshape=\"t\"/>",
            "<v:handles><v:h position=\"#0,bottomRight\" xrange=\"6629,14971\"/></v:handles>",
            "</v:shapetype>",
            "<v:shape id=\"OsakWatermark\" o:spid=\"_x0000_s2049\" type=\"#_x0000_t136\" ",
            "style=\"position:absolute;margin-left:0;margin-top:0;width:415pt;height:207.5pt;rotation:{};z-index:-251658752;mso-position-horizontal:center;mso-position-horizontal-relative:margin;mso-position-vertical:center;mso-position-vertical-relative:margin\" ",
            "fillcolor=\"{}\" stroked=\"f\"><v:fill opacity=\"{}\"/>",
            "<v:textpath style=\"font-family:&quot;Calibri&quot;;font-size:{}pt;font-weight:{}\" string=\"{}\"/>",
            "</v:shape></w:pict></w:r></w:p>"
        ),
        watermark.rotation, color, opacity, watermark.font_pt, weight, text
    )
}

fn write_head_foot(state: &mut DocxState, blocks: &[Block], is_header: bool, watermark: Option<&Watermark>) -> String {
    let mut writer = XmlWriter::new();
    writer.declaration();
    writer.raw(&format!("<w:{} {NS_DECL}>", if is_header { "hdr" } else { "ftr" }));
    if blocks.is_empty() && !(is_header && watermark.is_some()) {
        writer.raw("<w:p/>");
    }
    for block in blocks {
        write_block(state, &mut writer, block, 0);
    }
    if let Some(watermark) = watermark.filter(|_| is_header) {
        writer.raw(&watermark_paragraph(watermark));
    }
    writer.raw(if is_header { "</w:hdr>" } else { "</w:ftr>" });
    writer.finish()
}

/// The `w:sectPr` payload for one section, registering header/footer parts.
///
/// `watermark` is only set for the first section: its default header part then
/// carries the VML text watermark, which is where Word looks for it.
fn section_properties_xml(
    state: &mut DocxState,
    section: &SectionProps,
    parts: &mut DocxParts,
    watermark: Option<&Watermark>,
) -> String {
    let mut out = String::new();
    let mut add_part = |state: &mut DocxState,
                        parts: &mut DocxParts,
                        blocks: &[Block],
                        is_header: bool,
                        kind: &str|
     -> Option<String> {
        let watermark = if is_header && kind == "default" { watermark } else { None };
        if blocks.is_empty() && watermark.is_none() {
            return None;
        }
        let index = if is_header { parts.headers.len() } else { parts.footers.len() } + 1;
        let name = format!("{}{}.xml", if is_header { "header" } else { "footer" }, index);
        let xml = write_head_foot(state, blocks, is_header, watermark);
        let relationship = state.rels.add(
            if is_header {
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header"
            } else {
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer"
            },
            &name,
            false,
        );
        if is_header {
            parts.headers.push(name.clone());
        } else {
            parts.footers.push(name.clone());
        }
        parts.part_xml.push((name.clone(), xml));
        let tag = if is_header { "headerReference" } else { "footerReference" };
        out.push_str(&format!("<w:{tag} w:type=\"{kind}\" r:id=\"{relationship}\"/>"));
        Some(name)
    };
    add_part(state, parts, &section.header, true, "default");
    add_part(state, parts, &section.first_header, true, "first");
    add_part(state, parts, &section.even_header, true, "even");
    add_part(state, parts, &section.footer, false, "default");
    add_part(state, parts, &section.first_footer, false, "first");
    add_part(state, parts, &section.even_footer, false, "even");
    if section.different_first_page {
        out.push_str("<w:titlePg/>");
    }
    if section.different_odd_even {
        out.push_str("<w:evenAndOddHeaders/>");
    }
    match section.start.as_str() {
        "continuous" => out.push_str("<w:type w:val=\"continuous\"/>"),
        "oddPage" => out.push_str("<w:type w:val=\"oddPage\"/>"),
        "evenPage" => out.push_str("<w:type w:val=\"evenPage\"/>"),
        _ => {}
    }
    out.push_str(&format!(
        "<w:pgSz w:w=\"{}\" w:h=\"{}\"{} />",
        twips(section.page.width_pt),
        twips(section.page.height_pt),
        if section.page.orientation == "landscape" { " w:orient=\"landscape\"" } else { "" }
    ));
    out.push_str(&format!(
        "<w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\" w:header=\"{}\" w:footer=\"{}\" w:gutter=\"0\"/>",
        twips(section.page.margin_top_pt),
        twips(section.page.margin_right_pt),
        twips(section.page.margin_bottom_pt),
        twips(section.page.margin_left_pt),
        twips(section.page.header_distance_pt),
        twips(section.page.footer_distance_pt)
    ));
    let columns = section.columns.max(section.page.columns.max(1));
    if columns > 1 {
        out.push_str(&format!("<w:cols w:num=\"{}\" w:space=\"{}\"/>", columns, twips(section.page.column_spacing_pt)));
    } else {
        out.push_str("<w:cols w:space=\"708\"/>");
    }
    out
}

/// Header/footer parts produced while planning the sections.
#[derive(Default)]
struct DocxParts {
    headers: Vec<String>,
    footers: Vec<String>,
    part_xml: Vec<(String, String)>,
    even_odd: bool,
}

fn write_styles(document: &TextDocument) -> String {
    let mut writer = XmlWriter::new();
    writer.declaration();
    writer.raw(&format!("<w:styles {NS_DECL}>"));
    let normal = document.styles.iter().find(|style| style.id == "Normal").cloned().unwrap_or_default();
    writer.raw("<w:docDefaults><w:rPrDefault><w:rPr>");
    if let Some(font) = &normal.font {
        let escaped = crate::xml::escape_attr(font);
        writer.raw(&format!("<w:rFonts w:ascii=\"{escaped}\" w:hAnsi=\"{escaped}\" w:cs=\"{escaped}\"/>"));
    }
    writer.raw(&format!("<w:sz w:val=\"{}\"/>", half_points(normal.size_pt.unwrap_or(11.0))));
    writer.raw("</w:rPr></w:rPrDefault>");
    writer.raw(&format!(
        "<w:pPrDefault><w:pPr><w:spacing w:after=\"{}\" w:line=\"{}\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault>",
        twips(8.0),
        (1.15 * 240.0) as i64
    ));
    writer.raw("</w:docDefaults>");
    for style in &document.styles {
        let is_default = style.id == "Normal";
        writer.raw(&format!(
            "<w:style w:type=\"paragraph\" w:styleId=\"{}\"{}>",
            crate::xml::escape_attr(&style.id),
            if is_default { " w:default=\"1\"" } else { "" }
        ));
        writer.raw(&format!("<w:name w:val=\"{}\"/>", crate::xml::escape_attr(&style.name)));
        if let Some(based) = &style.based_on {
            writer.raw(&format!("<w:basedOn w:val=\"{}\"/>", crate::xml::escape_attr(based)));
        }
        if let Some(next) = &style.next {
            writer.raw(&format!("<w:next w:val=\"{}\"/>", crate::xml::escape_attr(next)));
        }
        writer.raw("<w:qFormat/>");
        writer.raw("<w:pPr>");
        if style.keep_with_next == Some(true) {
            writer.raw("<w:keepNext/>");
        }
        if style.page_break_before == Some(true) {
            writer.raw("<w:pageBreakBefore/>");
        }
        if let Some(align) = &style.align {
            let value = match align.as_str() {
                "center" => "center",
                "right" => "right",
                "justify" => "both",
                _ => "left",
            };
            writer.raw(&format!("<w:jc w:val=\"{value}\"/>"));
        }
        if style.space_before_pt.is_some() || style.space_after_pt.is_some() {
            writer.raw(&format!(
                "<w:spacing w:before=\"{}\" w:after=\"{}\"/>",
                twips(style.space_before_pt.unwrap_or(0.0)),
                twips(style.space_after_pt.unwrap_or(0.0))
            ));
        }
        if let Some(spacing) = style.line_spacing {
            writer.raw(&format!("<w:spacing w:line=\"{}\" w:lineRule=\"auto\"/>", (spacing * 240.0).round() as i64));
        }
        if style.indent_left_pt.is_some() || style.indent_right_pt.is_some() || style.first_line_pt.is_some() {
            let mut attrs = String::new();
            if let Some(value) = style.indent_left_pt {
                attrs.push_str(&format!(" w:left=\"{}\"", twips(value)));
            }
            if let Some(value) = style.indent_right_pt {
                attrs.push_str(&format!(" w:right=\"{}\"", twips(value)));
            }
            if let Some(value) = style.first_line_pt {
                let attribute = if value < 0.0 { "w:hanging" } else { "w:firstLine" };
                attrs.push_str(&format!(" {attribute}=\"{}\"", twips(value.abs())));
            }
            writer.raw(&format!("<w:ind{attrs}/>"));
        }
        if let Some(level) = style.outline_level {
            writer.raw(&format!("<w:outlineLvl w:val=\"{}\"/>", level.min(8)));
        }
        writer.raw("</w:pPr><w:rPr>");
        if let Some(font) = &style.font {
            let escaped = crate::xml::escape_attr(font);
            writer.raw(&format!("<w:rFonts w:ascii=\"{escaped}\" w:hAnsi=\"{escaped}\" w:cs=\"{escaped}\"/>"));
        }
        if style.bold == Some(true) {
            writer.raw("<w:b/>");
        }
        if style.italic == Some(true) {
            writer.raw("<w:i/>");
        }
        if style.underline == Some(true) {
            writer.raw("<w:u w:val=\"single\"/>");
        }
        if let Some(color) = style.color.as_deref().and_then(bare_hex) {
            writer.raw(&format!("<w:color w:val=\"{color}\"/>"));
        }
        if let Some(size) = style.size_pt {
            writer.raw(&format!("<w:sz w:val=\"{}\"/>", half_points(size)));
        }
        writer.raw("</w:rPr></w:style>");
    }
    // Character and paragraph styles Word needs for notes and comments.
    writer.raw("<w:style w:type=\"character\" w:styleId=\"FootnoteReference\"><w:name w:val=\"footnote reference\"/><w:uiPriority w:val=\"99\"/><w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr></w:style>");
    writer.raw("<w:style w:type=\"character\" w:styleId=\"EndnoteReference\"><w:name w:val=\"endnote reference\"/><w:uiPriority w:val=\"99\"/><w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr></w:style>");
    writer.raw("<w:style w:type=\"paragraph\" w:styleId=\"FootnoteText\"><w:name w:val=\"footnote text\"/><w:uiPriority w:val=\"99\"/><w:pPr><w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/></w:pPr><w:rPr><w:sz w:val=\"18\"/></w:rPr></w:style>");
    writer.raw("<w:style w:type=\"paragraph\" w:styleId=\"EndnoteText\"><w:name w:val=\"endnote text\"/><w:uiPriority w:val=\"99\"/><w:pPr><w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/></w:pPr><w:rPr><w:sz w:val=\"18\"/></w:rPr></w:style>");
    writer.raw("<w:style w:type=\"character\" w:styleId=\"CommentReference\"><w:name w:val=\"annotation reference\"/><w:uiPriority w:val=\"99\"/><w:rPr><w:sz w:val=\"16\"/></w:rPr></w:style>");
    writer.raw("<w:style w:type=\"paragraph\" w:styleId=\"CommentText\"><w:name w:val=\"annotation text\"/><w:uiPriority w:val=\"99\"/><w:pPr><w:spacing w:after=\"0\"/></w:pPr></w:style>");
    writer.raw("</w:styles>");
    writer.finish()
}

fn write_numbering(runs: &[u32]) -> String {
    let mut writer = XmlWriter::new();
    writer.declaration();
    writer.raw(&format!("<w:numbering {NS_DECL}>"));
    writer.raw("<w:abstractNum w:abstractNumId=\"0\">");
    for level in 0..9u32 {
        let marker = match level % 3 {
            0 => "•",
            1 => "○",
            _ => "▪",
        };
        writer.raw(&format!(
            "<w:lvl w:ilvl=\"{level}\"><w:start w:val=\"1\"/><w:numFmt w:val=\"bullet\"/><w:lvlText w:val=\"{marker}\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"{}\" w:hanging=\"360\"/></w:pPr><w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\"/></w:rPr></w:lvl>",
            (level + 1) * 720
        ));
    }
    writer.raw("</w:abstractNum>");
    writer.raw("<w:abstractNum w:abstractNumId=\"1\">");
    for level in 0..9u32 {
        let format = ["decimal", "lowerLetter", "lowerRoman"][(level % 3) as usize];
        writer.raw(&format!(
            "<w:lvl w:ilvl=\"{level}\"><w:start w:val=\"1\"/><w:numFmt w:val=\"{format}\"/><w:lvlText w:val=\"%{}.\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"{}\" w:hanging=\"360\"/></w:pPr></w:lvl>",
            level + 1,
            (level + 1) * 720
        ));
    }
    writer.raw("</w:abstractNum>");
    writer.raw("<w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num>");
    writer.raw("<w:num w:numId=\"2\"><w:abstractNumId w:val=\"1\"/></w:num>");
    // One `w:num` per numbered-list run, each restarting level 0 at the
    // list's own start so a list never continues the previous one.
    for (index, start) in runs.iter().enumerate() {
        writer.raw(&format!(
            "<w:num w:numId=\"{}\"><w:abstractNumId w:val=\"1\"/><w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"{}\"/></w:lvlOverride></w:num>",
            3 + index as u32,
            start
        ));
    }
    writer.raw("</w:numbering>");
    writer.finish()
}

/// Plans the numbered-list runs of the document body.
///
/// A run is a contiguous sequence of numbered paragraphs; any other paragraph
/// or block ends it, and a numbered paragraph whose `start` differs from the
/// run's starts a new run. Every run gets its own numbering id, so Word
/// restarts it at the list's start. The walk descends into table cells so a
/// list inside a table participates in the surrounding sequence. Returns each
/// run's start (in order; run `index` uses `numId` `3 + index`) and the id of
/// each paragraph, keyed by its address.
fn plan_list_numbering(document: &TextDocument) -> (Vec<u32>, HashMap<usize, u32>) {
    let mut sequence: Vec<Option<&ParaProps>> = Vec::new();
    collect_list_properties(&document.blocks, &mut sequence);
    let mut starts: Vec<u32> = Vec::new();
    let mut ids: HashMap<usize, u32> = HashMap::new();
    let mut active: Option<usize> = None;
    for props in sequence.into_iter().flatten() {
        let Some(list) = props.list.as_ref().filter(|list| list.kind == "number") else {
            active = None;
            continue;
        };
        let start = list.start.max(1);
        let index = match active {
            Some(index) if starts[index] == start => index,
            _ => {
                starts.push(start);
                active = Some(starts.len() - 1);
                starts.len() - 1
            }
        };
        ids.insert(props as *const ParaProps as usize, 3 + index as u32);
    }
    (starts, ids)
}

/// Collects every paragraph in document order; non-paragraph blocks push
/// `None` so they break a list run.
fn collect_list_properties<'a>(blocks: &'a [Block], out: &mut Vec<Option<&'a ParaProps>>) {
    for block in blocks {
        match block {
            Block::Paragraph { props, .. } => out.push(Some(props)),
            Block::Table { table } => {
                for row in &table.rows {
                    for cell in &row.cells {
                        collect_list_properties(&cell.blocks, out);
                    }
                }
            }
            _ => out.push(None),
        }
    }
}

fn write_settings(even_and_odd_headers: bool, track_revisions: bool) -> String {
    let mut writer = XmlWriter::new();
    writer.declaration();
    writer.raw(&format!("<w:settings {NS_DECL}>"));
    writer.raw("<w:zoom w:percent=\"100\"/><w:defaultTabStop w:val=\"708\"/><w:compat/>");
    if even_and_odd_headers {
        writer.raw("<w:evenAndOddHeaders/>");
    }
    if track_revisions {
        writer.raw("<w:trackRevisions/>");
    }
    writer.raw("</w:settings>");
    writer.finish()
}

/// The `word/footnotes.xml` or `word/endnotes.xml` part, when notes exist.
fn write_notes(state: &mut DocxState, document: &TextDocument, endnotes: bool) -> Option<String> {
    let ids = if endnotes { state.endnote_ids.clone() } else { state.footnote_ids.clone() };
    if ids.is_empty() {
        return None;
    }
    let (notes, item, root, text_style, reference_style) = if endnotes {
        (&document.endnotes, "endnote", "w:endnotes", "EndnoteText", "EndnoteReference")
    } else {
        (&document.footnotes, "footnote", "w:footnotes", "FootnoteText", "FootnoteReference")
    };
    let mut writer = XmlWriter::new();
    writer.declaration();
    writer.raw(&format!("<{root} {NS_DECL}>"));
    writer.raw(&format!(
        "<w:{item} w:type=\"separator\" w:id=\"-1\"><w:p><w:pPr><w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/></w:pPr><w:r><w:separator/></w:r></w:p></w:{item}>"
    ));
    writer.raw(&format!(
        "<w:{item} w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:pPr><w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/></w:pPr><w:r><w:continuationSeparator/></w:r></w:p></w:{item}>"
    ));
    for note in notes {
        let Some(number) = ids.get(&note.id).copied() else { continue };
        writer.raw(&format!("<w:{item} w:id=\"{number}\">"));
        writer.raw(&format!("<w:p><w:pPr><w:pStyle w:val=\"{text_style}\"/></w:pPr>"));
        writer.raw("<w:r><w:rPr>");
        writer.raw(&format!("<w:rStyle w:val=\"{reference_style}\"/><w:vertAlign w:val=\"superscript\"/>"));
        writer.raw("</w:rPr>");
        if note.marker.is_empty() {
            writer.raw(&format!("<w:{item}Ref/>"));
        } else {
            writer.raw(&format!("<w:t xml:space=\"preserve\">{}</w:t>", crate::xml::escape_text(&note.marker)));
        }
        writer.raw("</w:r><w:r><w:t xml:space=\"preserve\"> </w:t></w:r>");
        for run in &note.runs {
            if run.text.is_empty() {
                continue;
            }
            let mut body = run.clone();
            body.footnote = None;
            body.endnote = None;
            body.field = None;
            write_text_run(state, &mut writer, &body);
        }
        writer.raw("</w:p>");
        writer.raw(&format!("</w:{item}>"));
    }
    writer.raw(&format!("</{root}>"));
    Some(writer.finish())
}

/// The `word/comments.xml` part, when the document has comments.
fn write_comments(state: &mut DocxState, document: &TextDocument) -> Option<String> {
    if state.comment_ids.is_empty() || document.comments.is_empty() {
        return None;
    }
    let mut writer = XmlWriter::new();
    writer.declaration();
    writer.raw(&format!("<w:comments {NS_DECL}>"));
    for comment in &document.comments {
        let Some(number) = state.comment_ids.get(&comment.id).copied() else { continue };
        let date = if comment.created.is_empty() { "1970-01-01T00:00:00Z" } else { comment.created.as_str() };
        writer.raw(&format!(
            "<w:comment w:id=\"{number}\" w:author=\"{}\" w:date=\"{}\" w:initials=\"{}\">",
            crate::xml::escape_attr(&comment.author),
            crate::xml::escape_attr(date),
            crate::xml::escape_attr(&initials(&comment.author))
        ));
        writer.raw("<w:p><w:pPr><w:pStyle w:val=\"CommentText\"/></w:pPr><w:r><w:annotationRef/></w:r>");
        writer.raw(&format!(
            "<w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
            crate::xml::escape_text(&comment.text)
        ));
        for reply in &comment.replies {
            writer.raw("<w:p><w:pPr><w:pStyle w:val=\"CommentText\"/></w:pPr>");
            writer.raw(&format!(
                "<w:r><w:t xml:space=\"preserve\">Re: {}: {}</w:t></w:r></w:p>",
                crate::xml::escape_text(&reply.author),
                crate::xml::escape_text(&reply.text)
            ));
        }
        writer.raw("</w:comment>");
    }
    writer.raw("</w:comments>");
    Some(writer.finish())
}

fn initials(author: &str) -> String {
    author.split_whitespace().filter_map(|word| word.chars().next()).take(3).collect::<String>().to_uppercase()
}

pub fn write_docx(document: &TextDocument) -> OfficeResult<Vec<u8>> {
    let mut state = DocxState::new();
    let (list_starts, list_num_ids) = plan_list_numbering(document);
    state.list_num_ids = list_num_ids;

    // Comments are exported only when they exist in the model; the id map also
    // decides which `w:commentReference` runs are written.
    for comment in &document.comments {
        let number = state.comment_ids.len() as u32 + 1;
        state.comment_ids.insert(comment.id.clone(), number);
    }
    // Footnote/endnote numbers follow reference order, so numbering stays
    // correct when a note is inserted in the middle.
    for id in document.footnote_order() {
        state.note_id("footnote", &id);
    }
    for id in document.endnote_order() {
        state.note_id("endnote", &id);
    }

    // Plan the sections: the document-level page/header/footer fields are the
    // first section; each SectionBreak starts the next one.
    let sections = document.all_sections();
    let mut parts = DocxParts::default();
    let mut section_xml: Vec<String> = Vec::new();
    for (index, section) in sections.iter().enumerate() {
        let watermark = if index == 0 { document.watermark.as_ref() } else { None };
        section_xml.push(section_properties_xml(&mut state, section, &mut parts, watermark));
    }
    parts.even_odd = sections.iter().any(|section| section.different_odd_even)
        || sections.iter().any(|section| !section.even_header.is_empty() || !section.even_footer.is_empty());

    let mut body = XmlWriter::new();
    let mut current = 0usize;
    for block in &document.blocks {
        if matches!(block, Block::SectionBreak { .. }) {
            // Word stores the properties of the section that just ended in the
            // last paragraph of that section.
            let sect = section_xml.get(current).cloned().unwrap_or_default();
            write_paragraph(
                &mut state,
                &mut body,
                &ParaProps::default(),
                &[],
                Some(&format!("<w:sectPr>{sect}</w:sectPr>")),
            );
            current += 1;
            continue;
        }
        write_block(&mut state, &mut body, block, 0);
    }
    let last = section_xml.last().cloned().unwrap_or_default();
    body.raw(&format!("<w:sectPr>{last}</w:sectPr>"));

    let document_xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:document {NS_DECL}><w:body>{}</w:body></w:document>", body.finish());

    let styles_xml = write_styles(document);
    let numbering_xml = write_numbering(&list_starts);
    let settings_xml = write_settings(parts.even_odd, document.track_changes);

    // Notes and comments parts are written after the body so every referenced
    // note and comment has been assigned its number.
    let footnotes_xml = write_notes(&mut state, document, false);
    let endnotes_xml = write_notes(&mut state, document, true);
    let comments_xml = write_comments(&mut state, document);

    if let Some(xml) = &footnotes_xml {
        state.rels.add(
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes",
            "footnotes.xml",
            false,
        );
        let _ = xml;
    }
    if let Some(xml) = &endnotes_xml {
        state.rels.add(
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes",
            "endnotes.xml",
            false,
        );
        let _ = xml;
    }
    if let Some(xml) = &comments_xml {
        state.rels.add(
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments",
            "comments.xml",
            false,
        );
        let _ = xml;
    }

    // document.xml.rels: styles/numbering/settings first (rId1..3), then the rest.
    let mut entries = vec![
        (
            "rId1".to_string(),
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles|styles.xml".to_string(),
            false,
        ),
        (
            "rId2".to_string(),
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering|numbering.xml".to_string(),
            false,
        ),
        (
            "rId3".to_string(),
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings|settings.xml".to_string(),
            false,
        ),
    ];
    entries.extend(state.rels.entries.iter().cloned());
    let mut rels_writer = Relationships::new(0);
    rels_writer.entries = entries;
    let document_rels = rels_writer.xml();

    let mut zip = ZipWriter::new();
    zip.add_text(
        "[Content_Types].xml",
        &content_types(
            &parts.headers,
            &parts.footers,
            footnotes_xml.is_some(),
            endnotes_xml.is_some(),
            comments_xml.is_some(),
        ),
    );
    zip.add_text("_rels/.rels", &root_relationships());
    zip.add_text("docProps/core.xml", &core_properties(document));
    zip.add_text("docProps/app.xml", &app_properties(document));
    zip.add_text("word/document.xml", &document_xml);
    zip.add_text("word/_rels/document.xml.rels", &document_rels);
    zip.add_text("word/styles.xml", &styles_xml);
    zip.add_text("word/numbering.xml", &numbering_xml);
    zip.add_text("word/settings.xml", &settings_xml);
    // Header/footer parts in the order they were planned and referenced.
    for (name, xml) in &parts.part_xml {
        zip.add_text(&format!("word/{name}"), xml);
    }
    if let Some(xml) = footnotes_xml {
        zip.add_text("word/footnotes.xml", &xml);
    }
    if let Some(xml) = endnotes_xml {
        zip.add_text("word/endnotes.xml", &xml);
    }
    if let Some(xml) = comments_xml {
        zip.add_text("word/comments.xml", &xml);
    }
    for (name, data) in &state.media.entries {
        zip.add(&format!("word/{name}"), data);
    }
    Ok(zip.finish())
}

pub fn write_docx_file(path: &Path, document: &TextDocument) -> OfficeResult<()> {
    let bytes = write_docx(document)?;
    crate::io::write_atomic(path, &bytes)
}

//! DOCX package metadata, content-type manifest, and relationship helpers.

use super::*;

pub(super) const CONTENT_TYPE_HEADER: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml";
pub(super) const CONTENT_TYPE_FOOTER: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml";
const CONTENT_TYPE_XLSX: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

#[derive(Clone)]
pub(super) struct RelLink {
    pub(super) target: String,
    pub(super) external: bool,
}

pub(super) fn read_relationships(reader: &ZipReader, part: &str) -> HashMap<String, RelLink> {
    let (dir, file) = match part.rsplit_once('/') {
        Some((dir, file)) => (format!("{dir}/"), file.to_string()),
        None => (String::new(), part.to_string()),
    };
    let rels_name = format!("{dir}_rels/{file}.rels");
    let mut map = HashMap::new();
    let Ok(text) = reader.read_text(&rels_name) else {
        return map;
    };
    let Ok(root) = parse_xml(&text) else {
        return map;
    };
    for node in root.children_named("Relationship") {
        let Some(id) = node.attr("Id") else { continue };
        let Some(target) = node.attr("Target") else { continue };
        let external = node.attr("TargetMode").map(|value| value.eq_ignore_ascii_case("external")).unwrap_or(false);
        map.insert(id.to_string(), RelLink { target: target.to_string(), external });
    }
    map
}

pub(super) fn resolve_media_path(target: &str) -> String {
    if target.starts_with('/') {
        return target.trim_start_matches('/').to_string();
    }
    if target.contains("://") {
        return String::new();
    }
    format!("word/{}", target.trim_start_matches("./"))
}

pub(super) fn content_types(
    header_parts: &[String],
    footer_parts: &[String],
    footnotes: bool,
    endnotes: bool,
    comments: bool,
    chart_count: usize,
    has_embeddings: bool,
) -> String {
    let mut writer = XmlWriter::new();
    writer.declaration();
    writer.open("Types", &[("xmlns", "http://schemas.openxmlformats.org/package/2006/content-types")]);
    writer.empty(
        "Default",
        &[("Extension", "rels"), ("ContentType", "application/vnd.openxmlformats-package.relationships+xml")],
    );
    writer.empty("Default", &[("Extension", "xml"), ("ContentType", "application/xml")]);
    if has_embeddings {
        writer.empty("Default", &[("Extension", "xlsx"), ("ContentType", CONTENT_TYPE_XLSX)]);
    }
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
    for index in 1..=chart_count {
        overrides.push((
            format!("/word/charts/chart{index}.xml"),
            "application/vnd.openxmlformats-officedocument.drawingml.chart+xml",
        ));
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

pub(super) fn root_relationships() -> String {
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
            ("Type", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/metadata/core-properties"),
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

pub(super) fn core_properties(document: &TextDocument) -> String {
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

pub(super) fn app_properties(document: &TextDocument) -> String {
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

//! PDF inspector and accessibility checker.
//!
//! The Info screen used to report nine facts. A document that is going to be
//! archived, printed or published needs a lot more: what fonts and images it
//! embeds, whether it carries JavaScript or attachments, whether the text layer
//! is tagged, and what a screen reader would trip over. This module answers
//! those questions from the file structure, without adding a dependency.
//!
//! Everything here is read-only. Findings carry a severity and a plain
//! explanation, because "this document fails WCAG" without saying which check
//! failed and why is not something anyone can act on.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::docutil;
use crate::error::{PdfError, PdfResult};

/// How serious a finding is for an accessible or archival document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    /// Worth knowing; the document still works.
    Info,
    /// Should be fixed before publishing.
    Warning,
    /// Blocks conformance (PDF/UA, PDF/A).
    Error,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub severity: Severity,
    /// Stable identifier, e.g. `a11y.missing-title`.
    pub code: String,
    /// Short title shown in the list.
    pub title: String,
    /// What to do about it.
    pub detail: String,
}

/// One embedded font.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontInfo {
    pub name: String,
    pub subtype: String,
    /// True for a Type 0 composite font, which needs a CID descendant.
    pub composite: bool,
    /// True when the font program itself is in the file.
    pub embedded: bool,
    pub embedded_formats: Vec<String>,
}

/// One image XObject, aggregated across the document.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub color_space: String,
    pub bits_per_component: u8,
    /// `DCTDecode` (JPEG), `FlateDecode` (raw), `JPXDecode` (JPEG 2000)...
    pub filter: String,
    pub occurrences: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorSpaceInfo {
    pub name: String,
    pub components: u32,
    pub occurrences: u32,
    pub device_dependent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationInfo {
    pub page: u32,
    pub subtype: String,
    /// The annotation's `/Contents` text, which often holds a note.
    pub contents: String,
    /// True when the annotation is not displayed in the page.
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormFieldInfo {
    /// Fully qualified name, e.g. `address.postcode`.
    pub name: String,
    /// `Tx` (text), `Btn` (button), `Ch` (choice)...
    pub kind: String,
    pub field_type: String,
    pub read_only: bool,
    pub required: bool,
    /// True when the field has no `/TU` tooltip or alternate name.
    pub missing_label: bool,
    /// The options of a choice field.
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineEntry {
    pub title: String,
    /// Page the entry points at, 1-based.
    pub page: u32,
    pub depth: u32,
}

/// The full structural report for a document.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentInspection {
    pub path: String,
    pub file_size_bytes: u64,
    pub pdf_version: String,
    pub page_count: u32,
    pub encrypted: bool,
    pub linearized: bool,
    pub object_count: u32,
    pub has_struct_tree: bool,
    pub marked: bool,
    pub struct_tree: bool,
    pub language: String,
    pub viewer_preferences: String,
    pub fonts: Vec<FontInfo>,
    pub images: Vec<ImageInfo>,
    pub color_spaces: Vec<ColorSpaceInfo>,
    pub annotations: Vec<AnnotationInfo>,
    pub form_fields: Vec<FormFieldInfo>,
    pub outline: Vec<OutlineEntry>,
    pub embedded_files: Vec<String>,
    pub has_javascript: bool,
    pub javascript_entries: Vec<String>,
    pub has_open_action: bool,
    pub has_acro_form: bool,
    pub attachment_count: u32,
    pub total_image_pixels: u64,
    pub findings: Vec<Finding>,
    /// `pdf/ua` when the blocking checks pass, otherwise the first failure.
    pub accessibility_conformance: String,
    /// The /Info /Title; the UI falls back to the file name when empty.
    pub title_override: String,
    pub subject_override: String,
    pub author_override: String,
    pub producer: String,
    pub creator: String,
}

/// Inspects a document's structure. Read-only and never mutates the file.
pub fn inspect_document(path: &Path, password: Option<&str>) -> PdfResult<DocumentInspection> {
    let doc = docutil::load_document(path, password)?;
    let mut out = DocumentInspection {
        path: path.to_string_lossy().to_string(),
        file_size_bytes: std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0),
        pdf_version: if doc.is_encrypted() { "unknown".into() } else { doc.version.clone() },
        encrypted: doc.is_encrypted(),
        linearized: doc.trailer.get(b"Root").is_ok()
            && matches!(doc.get_object(root_id(&doc)?), Ok(Object::Dictionary(catalog)) if catalog.has_type(b"Catalog") && catalog.get(b"Linearized").is_ok()),
        object_count: doc.objects.len() as u32,
        ..Default::default()
    };

    let catalog = match root_id(&doc) {
        Ok(id) => match doc.get_dictionary(id) {
            Ok(dictionary) => dictionary.clone(),
            Err(_) => Dictionary::new(),
        },
        Err(_) => Dictionary::new(),
    };

    out.struct_tree = catalog.get(b"StructTreeRoot").is_ok();
    out.marked = catalog.get(b"MarkInfo").is_ok();
    out.language = text_of(&catalog, b"Lang").unwrap_or_default();
    if let Some(Object::Dictionary(preferences)) = deref(&doc, catalog.get(b"ViewerPreferences").ok()) {
        out.viewer_preferences =
            preferences.iter().map(|(key, _)| String::from_utf8_lossy(key).to_string()).collect::<Vec<_>>().join(", ");
    }
    out.has_open_action = catalog.get(b"OpenAction").is_ok();
    out.has_javascript = catalog.get(b"JavaScript").is_ok() || catalog.get(b"AA").is_ok();
    if let Ok(Object::Name(kind)) = catalog.get(b"JavaScript") {
        out.javascript_entries.push(String::from_utf8_lossy(kind).to_string());
    }
    if let Some(Object::Dictionary(names)) = deref(&doc, catalog.get(b"Names").ok()) {
        collect_name_tree(&doc, names, b"JavaScript", &mut out.javascript_entries);
        let mut files: Vec<String> = Vec::new();
        collect_name_tree(&doc, names, b"EmbeddedFiles", &mut files);
        out.embedded_files = files;
    }
    if let Ok(info_id) = doc.trailer.get(b"Info").and_then(|value| value.as_reference()) {
        if let Ok(Object::Dictionary(info)) = doc.get_object(info_id) {
            out.title_override = text_of(info, b"Title").unwrap_or_default();
            out.subject_override = text_of(info, b"Subject").unwrap_or_default();
            out.author_override = text_of(info, b"Author").unwrap_or_default();
            out.producer = text_of(info, b"Producer").unwrap_or_default();
            out.creator = text_of(info, b"Creator").unwrap_or_default();
        }
    }
    out.has_acro_form = catalog.get(b"AcroForm").is_ok();
    out.outline = read_outline(&doc, &catalog)?;

    let pages = doc.get_pages();
    out.page_count = pages.len() as u32;
    let mut fonts: BTreeMap<Vec<u8>, Dictionary> = BTreeMap::new();
    for (number, page_id) in &pages {
        collect_fonts(&doc, *page_id, &mut fonts);
        collect_annotations(&doc, *page_id, *number, &mut out.annotations);
        collect_images(&doc, *page_id, &mut out.images, &mut out.color_spaces);
    }
    for (name, font) in fonts {
        if let Some(info) = describe_font(&doc, &name, &font) {
            out.fonts.push(info);
        }
    }
    out.images.sort_by_key(|entry| (entry.width, entry.height, entry.filter.clone()));
    out.total_image_pixels = out
        .images
        .iter()
        .map(|entry| u64::from(entry.width) * u64::from(entry.height) * u64::from(entry.occurrences))
        .sum();
    out.attachment_count = out.embedded_files.len() as u32;
    out.form_fields = read_form_fields(&doc, &catalog);

    evaluate_findings(&mut out);
    Ok(out)
}

fn root_id(doc: &Document) -> PdfResult<ObjectId> {
    doc.trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|error| PdfError::InvalidPdf(format!("catalog: {error}")))
}

/// Resolves a dictionary entry that may be inline or behind a reference.
fn deref<'a>(doc: &'a Document, value: Option<&'a Object>) -> Option<&'a Object> {
    match value? {
        Object::Reference(id) => doc.get_object(*id).ok(),
        other => Some(other),
    }
}

fn text_of(dictionary: &Dictionary, key: &[u8]) -> Option<String> {
    docutil::pdf_text_value(dictionary.get(key).ok()?)
}

/// Walks a `/Names` subtree collecting the string value of each name.
fn collect_name_tree(doc: &Document, names: &Dictionary, key: &[u8], out: &mut Vec<String>) {
    let tree = match deref(doc, names.get(key).ok()).cloned() {
        Some(Object::Dictionary(tree)) => tree,
        _ => return,
    };
    if let Some(Object::Array(leaves)) = deref(doc, tree.get(b"Names").ok()) {
        let mut index = 0;
        while index + 1 < leaves.len() {
            if let Some(value) = leaves.get(index + 1).and_then(|value| deref(doc, Some(value))) {
                let name = match value {
                    Object::String(bytes, _) => String::from_utf8_lossy(bytes).to_string(),
                    Object::Dictionary(file) => file
                        .get(b"UF")
                        .ok()
                        .or_else(|| file.get(b"F").ok())
                        .and_then(docutil::pdf_text_value)
                        .unwrap_or_default(),
                    _ => String::new(),
                };
                if !name.is_empty() {
                    out.push(name);
                }
            }
            index += 2;
        }
    }
    if let Some(Object::Array(kids)) = deref(doc, tree.get(b"Kids").ok()) {
        for kid in kids {
            if let Some(Object::Dictionary(child)) = deref(doc, Some(kid)) {
                collect_name_tree(doc, child, key, out);
            }
        }
    }
}

/// Reads the document outline (bookmarks) with a bounded depth.
pub fn read_outline(doc: &Document, catalog: &Dictionary) -> PdfResult<Vec<OutlineEntry>> {
    let root = match deref(doc, catalog.get(b"Outlines").ok()).cloned() {
        Some(Object::Dictionary(root)) => root,
        _ => return Ok(Vec::new()),
    };
    let mut out: Vec<OutlineEntry> = Vec::new();
    let mut node = match root.get(b"First") {
        Ok(Object::Reference(id)) => Some(*id),
        _ => None,
    };
    let mut depth = 0u32;
    // 10k entries and 16 levels: an outline that needs more is a loop, not a
    // document the user wants to browse.
    while let Some(current) = node {
        if out.len() >= 10_000 || depth > 16 {
            break;
        }
        let entry = match doc.get_object(current) {
            Ok(Object::Dictionary(entry)) => entry,
            _ => break,
        };
        out.push(OutlineEntry {
            title: text_of(entry, b"Title").unwrap_or_default(),
            page: entry.get(b"Dest").ok().and_then(|value| destination_page(doc, value)).unwrap_or(0),
            depth,
        });
        node = match entry.get(b"Next") {
            Ok(Object::Reference(id)) => Some(*id),
            _ => None,
        };
        if let Ok(Object::Reference(first)) = entry.get(b"First") {
            let mut child = Some(*first);
            let child_depth = depth + 1;
            while let Some(current_child) = child {
                if out.len() >= 10_000 || child_depth > 16 {
                    break;
                }
                let child_entry = match doc.get_object(current_child) {
                    Ok(Object::Dictionary(value)) => value,
                    _ => break,
                };
                out.push(OutlineEntry {
                    title: text_of(child_entry, b"Title").unwrap_or_default(),
                    page: child_entry.get(b"Dest").ok().and_then(|value| destination_page(doc, value)).unwrap_or(0),
                    depth: child_depth,
                });
                child = match child_entry.get(b"Next") {
                    Ok(Object::Reference(id)) => Some(*id),
                    _ => None,
                };
            }
            depth = 0;
        }
    }
    Ok(out)
}

fn destination_page(doc: &Document, value: &Object) -> Option<u32> {
    let array = match value {
        Object::Array(items) => items.clone(),
        Object::Dictionary(entry) => match entry.get(b"D").ok() {
            Some(Object::Array(items)) => items.clone(),
            _ => return None,
        },
        _ => return None,
    };
    // ISO 32000-1 destinations put the page reference first (`[page /XYZ ...]`);
    // some producers keep a leading `null`, so a second lookup covers them.
    let page_id = [0usize, 1].into_iter().find_map(|index| array.get(index)?.as_reference().ok())?;
    for (number, candidate) in doc.get_pages() {
        if candidate == page_id {
            return Some(number);
        }
    }
    None
}

/// Rewrites the document outline (bookmarks) as an incremental update.
///
/// `entries` are in reading order; each 0-based `depth` nests the entry one
/// level below the nearest preceding entry with a smaller depth (a depth jump
/// is clamped to one level). Pages are 1-based; an entry outside the document
/// is rejected with `PdfError::InvalidInput`. Every byte of `pdf` is preserved,
/// so a document that is already signed keeps its signatures.
///
/// An empty `entries` removes `/Outlines` and `/PageMode` from the catalog.
pub fn write_outline(pdf: &[u8], entries: &[OutlineEntry]) -> PdfResult<Vec<u8>> {
    let mut doc = Document::load_mem(pdf).map_err(|error| PdfError::from_lopdf(error, None))?;
    if doc.is_encrypted() || doc.was_encrypted() {
        return Err(PdfError::PasswordRequired);
    }
    let catalog_id = root_id(&doc)?;
    let pages = doc.get_pages();
    let total = pages.len() as u32;
    for entry in entries {
        if entry.page == 0 || entry.page > total {
            return Err(PdfError::InvalidInput(format!(
                "outline entry \"{}\" points at page {}, outside the 1-{} range",
                entry.title, entry.page, total
            )));
        }
    }
    if entries.is_empty() {
        let catalog = doc.get_object_mut(catalog_id)?.as_dict_mut()?;
        catalog.remove(b"Outlines");
        catalog.remove(b"PageMode");
        return crate::incremental::apply_difference(pdf, &doc);
    }

    // Build the item tree with an explicit parent chain. The outline root is
    // created first so top-level items have a `/Parent` to point at.
    let outlines_id = doc.add_object(Object::Dictionary(Dictionary::new()));
    // (declared depth, effective depth, id): a depth jump is clamped to one
    // level below the nearest preceding entry with a smaller depth.
    let mut stack: Vec<(u32, u32, ObjectId)> = Vec::new();
    let mut first_child: HashMap<ObjectId, ObjectId> = HashMap::new();
    let mut last_child: HashMap<ObjectId, ObjectId> = HashMap::new();
    let mut descendant_count: HashMap<ObjectId, u32> = HashMap::new();
    for entry in entries {
        while matches!(stack.last(), Some((declared, _, _)) if *declared >= entry.depth) {
            stack.pop();
        }
        let depth = stack.last().map_or(0, |(_, effective, _)| effective + 1);
        let parent = stack.last().map(|(_, _, id)| *id).unwrap_or(outlines_id);
        let page_id = pages.get(&entry.page).copied().ok_or(PdfError::RangeOutOfBounds)?;
        let previous = last_child.get(&parent).copied();
        let mut item = Dictionary::new();
        item.set("Title", docutil::pdf_text_object(&entry.title));
        item.set("Parent", Object::Reference(parent));
        item.set(
            "Dest",
            Object::Array(vec![
                Object::Reference(page_id),
                Object::Name(b"XYZ".to_vec()),
                Object::Null,
                Object::Null,
                Object::Null,
            ]),
        );
        if let Some(previous_id) = previous {
            item.set("Prev", Object::Reference(previous_id));
        }
        let item_id = doc.add_object(Object::Dictionary(item));
        if let Some(previous_id) = previous {
            doc.get_object_mut(previous_id)?.as_dict_mut()?.set("Next", Object::Reference(item_id));
        }
        first_child.entry(parent).or_insert(item_id);
        last_child.insert(parent, item_id);
        // Every ancestor's visible count includes the new item.
        *descendant_count.entry(outlines_id).or_insert(0) += 1;
        for (_, _, ancestor) in &stack {
            *descendant_count.entry(*ancestor).or_insert(0) += 1;
        }
        stack.push((entry.depth, depth, item_id));
    }
    // A parent (the root or an item with children) owns the First/Last links
    // and a Count of its visible descendants.
    let parents: Vec<ObjectId> = last_child.keys().copied().collect();
    for parent in parents {
        let count = descendant_count.get(&parent).copied().unwrap_or(0) as i64;
        let dictionary = doc.get_object_mut(parent)?.as_dict_mut()?;
        dictionary.set("First", Object::Reference(first_child[&parent]));
        dictionary.set("Last", Object::Reference(last_child[&parent]));
        dictionary.set("Count", Object::Integer(count));
    }
    doc.get_object_mut(outlines_id)?.as_dict_mut()?.set("Type", Object::Name(b"Outlines".to_vec()));
    {
        let catalog = doc.get_object_mut(catalog_id)?.as_dict_mut()?;
        catalog.set("Outlines", Object::Reference(outlines_id));
        catalog.set("PageMode", Object::Name(b"UseOutlines".to_vec()));
    }
    crate::incremental::apply_difference(pdf, &doc)
}

/// The font-program keys a reader needs in order to render without
/// substituting, and the format each one implies.
const FONT_PROGRAMS: [(&[u8], &str); 3] =
    [(b"FontFile", "TrueType"), (b"FontFile2", "TrueType"), (b"FontFile3", "OpenType")];

/// Collects every distinct font name in the document, with its dictionary.
fn collect_fonts(doc: &Document, page_id: ObjectId, out: &mut BTreeMap<Vec<u8>, Dictionary>) {
    if let Ok(fonts) = doc.get_page_fonts(page_id) {
        for (name, font) in fonts {
            out.insert(name.to_vec(), font.clone());
        }
    }
}

fn describe_font(doc: &Document, name: &[u8], font: &Dictionary) -> Option<FontInfo> {
    let subtype = font
        .get(b"Subtype")
        .ok()
        .and_then(|value| value.as_name().ok())
        .map(|value| String::from_utf8_lossy(value).to_string())
        .unwrap_or_default();
    let mut embedded_formats: Vec<String> = Vec::new();
    // A Type0 font has no descriptor of its own; its program lives on the
    // descendant CIDFont.
    let descendant = match deref(doc, font.get(b"DescendantFonts").ok()) {
        Some(Object::Array(items)) if subtype == "Type0" => match deref(doc, items.first()) {
            Some(Object::Dictionary(descendant)) => Some(descendant),
            _ => None,
        },
        _ => None,
    };
    let owner = descendant.unwrap_or(font);
    if let Some(Object::Dictionary(descriptor)) = deref(doc, owner.get(b"FontDescriptor").ok()) {
        for (key, format) in FONT_PROGRAMS {
            if descriptor.get(key).is_ok() {
                embedded_formats.push(format.to_string());
            }
        }
    }
    Some(FontInfo {
        name: String::from_utf8_lossy(name).to_string(),
        composite: subtype == "Type0",
        embedded: !embedded_formats.is_empty(),
        subtype,
        embedded_formats,
    })
}

fn collect_annotations(doc: &Document, page_id: ObjectId, page: u32, out: &mut Vec<AnnotationInfo>) {
    let items = match doc.get_page_annotations(page_id) {
        Ok(items) => items,
        Err(_) => return,
    };
    for entry in items {
        out.push(AnnotationInfo {
            page,
            subtype: entry
                .get(b"Subtype")
                .ok()
                .and_then(|value| value.as_name().ok())
                .map(|value| String::from_utf8_lossy(value).to_string())
                .unwrap_or_default(),
            contents: text_of(entry, b"Contents").unwrap_or_default(),
            // Bit 2 of /F is the Hidden flag.
            hidden: entry.get(b"F").ok().and_then(|value| value.as_i64().ok()).unwrap_or(0) & 2 != 0,
        });
    }
}

fn collect_images(doc: &Document, page_id: ObjectId, images: &mut Vec<ImageInfo>, spaces: &mut Vec<ColorSpaceInfo>) {
    for id in docutil::page_xobjects(doc, page_id) {
        let stream = match doc.get_object(id) {
            Ok(Object::Stream(stream)) => stream,
            _ => continue,
        };
        if stream.dict.get(b"Subtype").ok().and_then(|value| value.as_name().ok()) != Some(b"Image") {
            continue;
        }
        let width = stream.dict.get(b"Width").ok().and_then(|value| value.as_i64().ok()).unwrap_or(0) as u32;
        let height = stream.dict.get(b"Height").ok().and_then(|value| value.as_i64().ok()).unwrap_or(0) as u32;
        let filter = stream
            .dict
            .get(b"Filter")
            .ok()
            .and_then(|value| value.as_name().ok())
            .map(|value| String::from_utf8_lossy(value).to_string())
            .unwrap_or_else(|| "none".into());
        let space = describe_color_space(stream.dict.get(b"ColorSpace").ok());
        let bits = stream.dict.get(b"BitsPerComponent").ok().and_then(|value| value.as_i64().ok()).unwrap_or(8) as u8;
        let entry = ImageInfo {
            width,
            height,
            color_space: space.name.clone(),
            bits_per_component: bits,
            filter,
            occurrences: 1,
        };
        match images.iter_mut().find(|candidate| {
            candidate.width == entry.width
                && candidate.height == entry.height
                && candidate.color_space == entry.color_space
                && candidate.filter == entry.filter
        }) {
            Some(existing) => existing.occurrences += 1,
            None => images.push(entry),
        }
        match spaces.iter_mut().find(|candidate| candidate.name == space.name) {
            Some(existing) => existing.occurrences += 1,
            None => spaces.push(ColorSpaceInfo {
                name: space.name,
                components: space.components,
                occurrences: 1,
                device_dependent: space.device_dependent,
            }),
        }
    }
}

struct SpaceDescription {
    name: String,
    components: u32,
    device_dependent: bool,
}

fn describe_color_space(value: Option<&Object>) -> SpaceDescription {
    let name = match value {
        Some(Object::Name(name)) => String::from_utf8_lossy(name).to_string(),
        Some(Object::Array(items)) => items
            .first()
            .and_then(|value| value.as_name().ok())
            .map(|value| String::from_utf8_lossy(value).to_string())
            .unwrap_or_else(|| "Indexed".into()),
        _ => "unknown".into(),
    };
    let components = match name.as_str() {
        "DeviceGray" | "CalGray" | "G" | "Indexed" => 1,
        "DeviceRGB" | "CalRGB" | "Lab" | "RGB" => 3,
        "DeviceCMYK" | "CMYK" => 4,
        _ => 3,
    };
    let device_dependent = name.starts_with("Device") || name.starts_with("Cal");
    SpaceDescription { name, components, device_dependent }
}

/// Flattens the AcroForm field tree, honouring `/Kids` and parent names.
fn read_form_fields(doc: &Document, catalog: &Dictionary) -> Vec<FormFieldInfo> {
    let acro = match deref(doc, catalog.get(b"AcroForm").ok()).cloned() {
        Some(Object::Dictionary(acro)) => acro,
        _ => return Vec::new(),
    };
    let mut out: Vec<FormFieldInfo> = Vec::new();
    let mut queue: Vec<(Dictionary, String)> = Vec::new();
    if let Some(Object::Array(fields)) = deref(doc, acro.get(b"Fields").ok()).cloned() {
        for field in &fields {
            if let Some(Object::Dictionary(dictionary)) = deref(doc, Some(field)).cloned() {
                queue.push((dictionary, String::new()));
            }
        }
    }
    // A field tree can be deep; 5000 fields and 32 levels is far past any real form.
    let mut guard = 0;
    while let Some((dictionary, prefix)) = queue.pop() {
        guard += 1;
        if guard > 5_000 || out.len() > 5_000 {
            break;
        }
        let partial = text_of(&dictionary, b"T").unwrap_or_default();
        let name = if prefix.is_empty() {
            partial.clone()
        } else if partial.is_empty() {
            prefix.clone()
        } else {
            format!("{prefix}.{partial}")
        };
        let field_type = dictionary
            .get(b"FT")
            .ok()
            .and_then(|value| value.as_name().ok())
            .map(|value| String::from_utf8_lossy(value).to_string())
            .unwrap_or_default();
        if matches!(dictionary.get(b"Kids"), Ok(Object::Array(_))) {
            if let Ok(Object::Array(kids)) = dictionary.get(b"Kids") {
                for kid in kids {
                    if let Some(Object::Dictionary(child)) = deref(doc, Some(kid)).cloned() {
                        queue.push((child, name.clone()));
                    }
                }
            }
            continue;
        }
        let options = match deref(doc, dictionary.get(b"Opt").ok()).cloned() {
            Some(Object::Array(items)) => items
                .iter()
                .filter_map(|item| match item {
                    Object::String(bytes, _) => Some(String::from_utf8_lossy(bytes).to_string()),
                    // A choice entry is [export value, display text].
                    Object::Array(pair) => pair.first().and_then(|value| match value {
                        Object::String(bytes, _) => Some(String::from_utf8_lossy(bytes).to_string()),
                        _ => None,
                    }),
                    _ => None,
                })
                .filter(|value| !value.is_empty())
                .collect(),
            _ => Vec::new(),
        };
        let flags = dictionary.get(b"Ff").ok().and_then(|value| value.as_i64().ok()).unwrap_or(0);
        out.push(FormFieldInfo {
            name,
            missing_label: text_of(&dictionary, b"TU").unwrap_or_default().is_empty(),
            kind: dictionary
                .get(b"S")
                .ok()
                .and_then(|value| value.as_name().ok())
                .map(|value| String::from_utf8_lossy(value).to_string())
                .unwrap_or_default(),
            field_type,
            // Bit 1 is ReadOnly, bit 2 is Required.
            read_only: flags & 1 != 0,
            required: flags & 2 != 0,
            options,
        });
    }
    out
}

/// Runs the accessibility and archival checks over an inspection result.
pub fn evaluate_findings(inspection: &mut DocumentInspection) {
    let mut findings: Vec<Finding> = Vec::new();
    let mut add = |severity: Severity, code: &str, title: &str, detail: &str| {
        findings.push(Finding {
            severity,
            code: code.to_string(),
            title: title.to_string(),
            detail: detail.to_string(),
        });
    };

    let title = inspection.title();
    if title.trim().is_empty() {
        add(
            Severity::Error,
            "a11y.missing-title",
            "No document title",
            "Set a title in the document properties so a screen reader announces it.",
        );
    }
    if inspection.language.trim().is_empty() {
        add(
            Severity::Error,
            "a11y.missing-language",
            "No document language",
            "Set the document language so a screen reader picks the right voice.",
        );
    }
    if !inspection.struct_tree {
        add(
            Severity::Error,
            "a11y.untagged",
            "Not a tagged PDF",
            "The document has no structure tree, so a screen reader cannot tell headings from body text. Export it tagged.",
        );
    }
    if !inspection.marked && inspection.struct_tree {
        add(
            Severity::Warning,
            "a11y.not-marked",
            "Not marked as a figure document",
            "The structure tree exists but /MarkInfo is missing.",
        );
    }
    if !inspection.outline.is_empty() {
        let mut previous: i32 = -1;
        let mut skipped = false;
        for entry in &inspection.outline {
            let level = entry.depth as i32;
            if previous >= 0 && level > previous + 1 {
                skipped = true;
                break;
            }
            previous = level;
        }
        if skipped {
            add(
                Severity::Warning,
                "a11y.heading-skip",
                "A bookmark level is skipped",
                "The outline jumps a level, which breaks keyboard navigation.",
            );
        }
    }
    for field in &inspection.form_fields {
        if field.missing_label {
            add(
                Severity::Error,
                "a11y.form-label",
                "A form field has no label",
                &format!("The field \"{}\" has no alternate name, so a screen reader announces nothing.", field.name),
            );
        }
    }
    if inspection.has_javascript {
        add(
            Severity::Warning,
            "security.javascript",
            "The document contains JavaScript",
            "Scripts can run when the file is opened. Remove them unless the document genuinely needs them.",
        );
    }
    if !inspection.embedded_files.is_empty() {
        add(
            Severity::Info,
            "security.attachments",
            "The document carries attachments",
            &format!(
                "{} embedded file(s) travel with the PDF and are not visible on the page.",
                inspection.embedded_files.len()
            ),
        );
    }
    let unembedded: Vec<&FontInfo> = inspection.fonts.iter().filter(|font| !font.embedded).collect();
    if !unembedded.is_empty() {
        add(
            Severity::Error,
            "pdfa.unembedded-font",
            "Some fonts are not embedded",
            &format!(
                "{} font(s) rely on the reader's substitutes, so the document renders differently elsewhere and fails PDF/A.",
                unembedded.len()
            ),
        );
    }
    if inspection.encrypted {
        add(
            Severity::Warning,
            "pdfa.encrypted",
            "The document is encrypted",
            "PDF/A forbids encryption. Remove the password before archiving.",
        );
    }
    let annotations: usize = inspection.annotations.iter().filter(|entry| entry.subtype != "Link").count();
    if annotations > 0 {
        add(
            Severity::Info,
            "a11y.annotations",
            "The document has annotations",
            &format!("{annotations} non-link annotation(s). Link text should say where it leads."),
        );
    }
    findings.sort_by(|a, b| b.severity.cmp(&a.severity).then(a.code.cmp(&b.code)));
    let blocking = findings.iter().any(|finding| finding.severity == Severity::Error);
    inspection.accessibility_conformance = if blocking { "fails".into() } else { "pdf/ua".into() };
    inspection.findings = findings;
}

impl DocumentInspection {
    /// The `/Info` title, falling back to the file name.
    pub fn title(&self) -> String {
        if self.title_override.is_empty() {
            Path::new(&self.path).file_stem().map(|stem| stem.to_string_lossy().to_string()).unwrap_or_default()
        } else {
            self.title_override.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> DocumentInspection {
        DocumentInspection { path: "invoice.pdf".into(), ..Default::default() }
    }

    #[test]
    fn a_file_name_stands_in_for_a_missing_title() {
        // A screen reader announces the file name, so that is a usable title and
        // must not be reported as a failure.
        let mut inspection = base();
        inspection.language = "en".into();
        inspection.struct_tree = true;
        evaluate_findings(&mut inspection);
        let codes: Vec<&str> = inspection.findings.iter().map(|entry| entry.code.as_str()).collect();
        assert!(!codes.contains(&"a11y.missing-title"), "unexpected findings: {codes:?}");
        assert_eq!(inspection.title(), "invoice");
    }

    #[test]
    fn no_title_and_no_usable_name_is_an_error() {
        let mut inspection = DocumentInspection { path: String::new(), ..Default::default() };
        inspection.language = "en".into();
        inspection.struct_tree = true;
        evaluate_findings(&mut inspection);
        let codes: Vec<&str> = inspection.findings.iter().map(|entry| entry.code.as_str()).collect();
        assert!(codes.contains(&"a11y.missing-title"), "unexpected findings: {codes:?}");
        assert!(inspection.findings.iter().any(|entry| entry.severity == Severity::Error));
        assert_eq!(inspection.accessibility_conformance, "fails");
    }

    #[test]
    fn a_missing_language_is_always_an_error() {
        let mut tagged = base();
        tagged.language = String::new();
        tagged.struct_tree = true;
        evaluate_findings(&mut tagged);
        let codes: Vec<&str> = tagged.findings.iter().map(|entry| entry.code.as_str()).collect();
        assert!(codes.contains(&"a11y.missing-language"), "unexpected findings: {codes:?}");
        // A structure tree was supplied, so untagged must not also fire.
        assert!(!codes.contains(&"a11y.untagged"), "unexpected findings: {codes:?}");

        let mut untagged = base();
        untagged.language = String::new();
        evaluate_findings(&mut untagged);
        let codes: Vec<&str> = untagged.findings.iter().map(|entry| entry.code.as_str()).collect();
        assert!(codes.contains(&"a11y.untagged"), "unexpected findings: {codes:?}");
    }

    #[test]
    fn a_fully_tagged_document_with_a_title_passes() {
        let mut inspection = base();
        inspection.title_override = "Invoice 2024".into();
        inspection.language = "en-GB".into();
        inspection.struct_tree = true;
        inspection.marked = true;
        evaluate_findings(&mut inspection);
        assert_eq!(inspection.accessibility_conformance, "pdf/ua");
        assert!(!inspection.findings.iter().any(|entry| entry.severity == Severity::Error));
    }

    #[test]
    fn an_unembedded_font_is_an_error() {
        let mut inspection = base();
        inspection.title_override = "Report".into();
        inspection.language = "en".into();
        inspection.struct_tree = true;
        inspection.fonts.push(FontInfo {
            name: "Helvetica".into(),
            subtype: "Type1".into(),
            composite: false,
            embedded: false,
            embedded_formats: Vec::new(),
        });
        evaluate_findings(&mut inspection);
        assert!(inspection.findings.iter().any(|entry| entry.code == "pdfa.unembedded-font"));
        assert_eq!(inspection.accessibility_conformance, "fails");
    }

    #[test]
    fn a_form_field_without_a_label_is_an_error() {
        let mut inspection = base();
        inspection.title_override = "Form".into();
        inspection.language = "en".into();
        inspection.struct_tree = true;
        inspection.form_fields.push(FormFieldInfo {
            name: "address.postcode".into(),
            kind: "Tx".into(),
            field_type: "Tx".into(),
            read_only: false,
            required: true,
            missing_label: true,
            options: Vec::new(),
        });
        evaluate_findings(&mut inspection);
        let finding = inspection
            .findings
            .iter()
            .find(|entry| entry.code == "a11y.form-label")
            .expect("the missing label must be reported");
        assert!(finding.detail.contains("address.postcode"));
    }

    #[test]
    fn a_skipped_bookmark_level_is_a_warning_not_an_error() {
        let mut inspection = base();
        inspection.title_override = "Report".into();
        inspection.language = "en".into();
        inspection.struct_tree = true;
        inspection.outline = vec![
            OutlineEntry { title: "One".into(), page: 1, depth: 0 },
            OutlineEntry { title: "Three".into(), page: 2, depth: 2 },
        ];
        evaluate_findings(&mut inspection);
        let finding = inspection
            .findings
            .iter()
            .find(|entry| entry.code == "a11y.heading-skip")
            .expect("the skipped level must be reported");
        assert_eq!(finding.severity, Severity::Warning);
        assert_eq!(inspection.accessibility_conformance, "pdf/ua");
    }

    #[test]
    fn findings_are_ordered_most_severe_first() {
        let mut inspection = base();
        inspection.has_javascript = true;
        evaluate_findings(&mut inspection);
        let severities: Vec<Severity> = inspection.findings.iter().map(|entry| entry.severity).collect();
        let mut sorted = severities.clone();
        sorted.sort_by(|a, b| b.cmp(a));
        assert_eq!(severities, sorted);
    }

    #[test]
    fn color_space_names_map_to_component_counts() {
        assert_eq!(describe_color_space(Some(&Object::Name(b"DeviceRGB".to_vec()))).components, 3);
        assert_eq!(describe_color_space(Some(&Object::Name(b"DeviceCMYK".to_vec()))).components, 4);
        assert_eq!(describe_color_space(Some(&Object::Name(b"DeviceGray".to_vec()))).components, 1);
        assert!(describe_color_space(Some(&Object::Name(b"DeviceRGB".to_vec()))).device_dependent);
        assert!(!describe_color_space(Some(&Object::Name(b"ICCBased".to_vec()))).device_dependent);
    }

    #[test]
    fn an_unknown_colour_space_does_not_panic() {
        let description = describe_color_space(None);
        assert_eq!(description.name, "unknown");
    }

    #[test]
    fn severity_orders_from_info_to_error() {
        assert!(Severity::Error > Severity::Warning);
        assert!(Severity::Warning > Severity::Info);
    }
}

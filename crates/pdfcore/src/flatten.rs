//! Annotation and form flattening.
//!
//! Flattening bakes an annotation's appearance into the page content so the
//! result no longer depends on the viewer rendering widgets, and then removes
//! the annotation (and, for form fields, the field dictionary) from the file.
//!
//! For every annotation with an `/AP /N` appearance stream the module appends
//! `q <matrix> cm /<name> Do Q` to the page, where the matrix maps the stream's
//! `/BBox` (after its own `/Matrix`) onto the annotation `/Rect`, and registers
//! the stream as a page XObject. Widgets are also removed from the AcroForm
//! field tree; when that leaves `/Fields` empty the `/AcroForm` entry is gone.
//!
//! With `appearances` enabled and `annotations`/`forms` disabled, annotations
//! are kept but text fields without an appearance get a simple generated one.
//! Anything that cannot be flattened (no appearance, no way to generate one)
//! is reported as a warning instead of being silently dropped.

use std::collections::BTreeSet;
use std::path::Path;

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};

use crate::docutil::{self, add_resource_entry, append_page_content, pdf_text_value, Matrix};
use crate::error::PdfResult;
use crate::progress::{CancelToken, ProgressCallback, ProgressReporter};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlattenOptions {
    /// Burn every annotation that has an appearance and remove it.
    pub annotations: bool,
    /// Burn form fields/widgets and remove them from the AcroForm.
    pub forms: bool,
    /// Generate a simple appearance for text fields that have none.
    pub appearances: bool,
}

impl Default for FlattenOptions {
    fn default() -> Self {
        Self {
            annotations: true,
            forms: true,
            appearances: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlattenReport {
    pub annotations_flattened: u32,
    pub fields_flattened: u32,
    pub pages_touched: u32,
    pub warnings: Vec<String>,
}

enum AnnotsStorage {
    Inline,
    Array(ObjectId),
}

fn resolve_dict(doc: &Document, value: Option<&Object>) -> Option<Dictionary> {
    match value? {
        Object::Reference(id) => doc.get_dictionary(*id).ok().cloned(),
        Object::Dictionary(dict) => Some(dict.clone()),
        _ => None,
    }
}

fn resolve_array(doc: &Document, value: Option<&Object>) -> Option<Vec<Object>> {
    match value? {
        Object::Reference(id) => match doc.get_object(*id).ok()? {
            Object::Array(items) => Some(items.clone()),
            _ => None,
        },
        Object::Array(items) => Some(items.clone()),
        _ => None,
    }
}

fn read_box(dict: &Dictionary, key: &[u8]) -> Option<[f64; 4]> {
    let items = dict.get(key).ok()?.as_array().ok()?;
    if items.len() < 4 {
        return None;
    }
    let mut out = [0.0; 4];
    for (index, item) in items.iter().take(4).enumerate() {
        out[index] = docutil::object_to_f64(item)?;
    }
    Some(out)
}

fn rect_of(doc: &Document, annot: &Dictionary) -> Option<[f64; 4]> {
    match annot.get(b"Rect").ok()? {
        Object::Array(items) => {
            if items.len() < 4 {
                return None;
            }
            let mut out = [0.0; 4];
            for (index, item) in items.iter().take(4).enumerate() {
                out[index] = docutil::object_to_f64(item)?;
            }
            Some(out)
        }
        Object::Reference(id) => {
            let items = match doc.get_object(*id).ok()? {
                Object::Array(items) => items.clone(),
                _ => return None,
            };
            if items.len() < 4 {
                return None;
            }
            let mut out = [0.0; 4];
            for (index, item) in items.iter().take(4).enumerate() {
                out[index] = docutil::object_to_f64(item)?;
            }
            Some(out)
        }
        _ => None,
    }
}

fn read_matrix(dict: &Dictionary) -> Matrix {
    let items = match dict.get(b"Matrix").ok().and_then(|value| value.as_array().ok()) {
        Some(items) if items.len() >= 6 => items,
        _ => return Matrix::IDENTITY,
    };
    let mut values = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    for (index, item) in items.iter().take(6).enumerate() {
        values[index] = docutil::object_to_f64(item).unwrap_or(if index == 0 || index == 3 { 1.0 } else { 0.0 });
    }
    Matrix(values)
}

/// The stream the viewer would show for an annotation, following `/AP /N`
/// through both the reference and the state-dictionary forms.
fn appearance_stream_id(doc: &Document, annot: &Dictionary) -> Option<ObjectId> {
    let ap = resolve_dict(doc, annot.get(b"AP").ok())?;
    let normal = ap.get(b"N").ok()?;
    match normal {
        Object::Reference(id) => match doc.get_object(*id).ok()? {
            Object::Stream(_) => Some(*id),
            Object::Dictionary(states) => select_state(doc, annot, states),
            _ => None,
        },
        Object::Dictionary(states) => select_state(doc, annot, states),
        _ => None,
    }
}

fn select_state(doc: &Document, annot: &Dictionary, states: &Dictionary) -> Option<ObjectId> {
    let state = annot.get(b"AS").ok().and_then(|value| value.as_name().ok());
    let chosen = state
        .and_then(|name| states.get(name).ok())
        .or_else(|| states.iter().next().map(|(_, value)| value))?;
    let id = chosen.as_reference().ok()?;
    match doc.get_object(id).ok()? {
        Object::Stream(_) => Some(id),
        _ => None,
    }
}

/// Walks a field's `/Parent` chain for an inheritable key such as `/FT`.
fn inherited_name(doc: &Document, id: ObjectId, key: &[u8]) -> Option<Vec<u8>> {
    let mut current = Some(id);
    for _ in 0..32 {
        let id = current?;
        let dict = doc.get_dictionary(id).ok()?;
        if let Some(name) = dict.get(key).ok().and_then(|value| value.as_name().ok()) {
            return Some(name.to_vec());
        }
        current = dict.get(b"Parent").ok().and_then(|value| value.as_reference().ok());
    }
    None
}

fn generated_appearance(doc: &mut Document, annot_id: ObjectId, annot: &Dictionary) -> Option<ObjectId> {
    let field_type = inherited_name(doc, annot_id, b"FT")?;
    if field_type != b"Tx" {
        return None;
    }
    let text = annot
        .get(b"V")
        .ok()
        .and_then(|value| match value {
            Object::Reference(id) => doc.get_object(*id).ok().and_then(pdf_text_value),
            other => pdf_text_value(other),
        })
        .unwrap_or_default();
    let rect = rect_of(doc, annot)?;
    let width = (rect[2] - rect[0]).abs().max(1.0);
    let height = (rect[3] - rect[1]).abs().max(1.0);
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    }));
    let content = format!(
        "q\nBT\n/Helv 10 Tf\n2 2 Td\n({}) Tj\nET\nQ\n",
        docutil::escape_pdf_literal(&text)
    );
    let stream_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1i64,
            "BBox" => vec![
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(width as f32),
                Object::Real(height as f32),
            ],
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! {
                    "Helv" => Object::Reference(font),
                }),
            }),
        },
        content.into_bytes(),
    )));
    let target = doc.get_object_mut(annot_id).ok()?.as_dict_mut().ok()?;
    target.set(
        "AP",
        Object::Dictionary(dictionary! {
            "N" => Object::Reference(stream_id),
        }),
    );
    Some(stream_id)
}

fn appearance_matrix(doc: &Document, appearance_id: ObjectId, rect: [f64; 4]) -> Matrix {
    let (bbox, matrix) = match doc.get_object(appearance_id) {
        Ok(Object::Stream(stream)) => (
            read_box(&stream.dict, b"BBox").unwrap_or([0.0, 0.0, 1.0, 1.0]),
            read_matrix(&stream.dict),
        ),
        _ => return Matrix::IDENTITY,
    };
    let corners = [
        matrix.apply(bbox[0], bbox[1]),
        matrix.apply(bbox[2], bbox[1]),
        matrix.apply(bbox[0], bbox[3]),
        matrix.apply(bbox[2], bbox[3]),
    ];
    let min_x = corners.iter().map(|corner| corner.0).fold(f64::MAX, f64::min);
    let max_x = corners.iter().map(|corner| corner.0).fold(f64::MIN, f64::max);
    let min_y = corners.iter().map(|corner| corner.1).fold(f64::MAX, f64::min);
    let max_y = corners.iter().map(|corner| corner.1).fold(f64::MIN, f64::max);
    let width = (rect[2] - rect[0]).abs().max(0.0001);
    let height = (rect[3] - rect[1]).abs().max(0.0001);
    let sx = if max_x - min_x > 1e-9 { width / (max_x - min_x) } else { 1.0 };
    let sy = if max_y - min_y > 1e-9 { height / (max_y - min_y) } else { 1.0 };
    Matrix::translate(rect[0].min(rect[2]), rect[1].min(rect[3]))
        .mul(Matrix::scale(sx, sy))
        .mul(Matrix::translate(-min_x, -min_y))
        .mul(matrix)
}

fn burn_annotation(
    doc: &mut Document,
    page_id: ObjectId,
    annot: &Dictionary,
    appearance_id: ObjectId,
) -> PdfResult<()> {
    let rect = rect_of(doc, annot).unwrap_or([0.0, 0.0, 1.0, 1.0]);
    let matrix = appearance_matrix(doc, appearance_id, rect);
    let name = format!("FA{}", appearance_id.0);
    add_resource_entry(doc, page_id, b"XObject", &name, Object::Reference(appearance_id))?;
    let snippet = format!("q\n{}\n/{name} Do\nQ\n", matrix.to_cm());
    append_page_content(doc, page_id, snippet.into_bytes())
}

fn annotation_label(annot: &Dictionary) -> String {
    annot
        .get(b"Subtype")
        .ok()
        .and_then(|value| value.as_name().ok())
        .map(|value| String::from_utf8_lossy(value).to_string())
        .unwrap_or_else(|| "annotation".into())
}

fn annots_storage(doc: &Document, page_id: ObjectId) -> Option<(AnnotsStorage, Vec<Object>)> {
    let page = doc.get_dictionary(page_id).ok()?;
    match page.get(b"Annots").ok()? {
        Object::Array(items) => Some((AnnotsStorage::Inline, items.clone())),
        Object::Reference(id) => match doc.get_object(*id).ok()? {
            Object::Array(items) => Some((AnnotsStorage::Array(*id), items.clone())),
            _ => None,
        },
        _ => None,
    }
}

fn retain_annots(items: &mut Vec<Object>, remove_ids: &BTreeSet<ObjectId>, remove_indices: &BTreeSet<usize>) {
    let mut index = 0;
    items.retain(|entry| {
        let remove = match entry {
            Object::Reference(id) => remove_ids.contains(id),
            _ => remove_indices.contains(&index),
        };
        index += 1;
        !remove
    });
}

fn filter_annots(
    doc: &mut Document,
    page_id: ObjectId,
    storage: &AnnotsStorage,
    remove_ids: &BTreeSet<ObjectId>,
    remove_indices: &BTreeSet<usize>,
) -> PdfResult<()> {
    match storage {
        AnnotsStorage::Inline => {
            let items = doc
                .get_object_mut(page_id)?
                .as_dict_mut()?
                .get_mut(b"Annots")?
                .as_array_mut()?;
            retain_annots(items, remove_ids, remove_indices);
        }
        AnnotsStorage::Array(id) => {
            let items = doc.get_object_mut(*id)?.as_array_mut()?;
            retain_annots(items, remove_ids, remove_indices);
        }
    }
    Ok(())
}

fn collect_field_ids(doc: &Document) -> BTreeSet<ObjectId> {
    let mut out = BTreeSet::new();
    let catalog = match doc.catalog() {
        Ok(catalog) => catalog,
        Err(_) => return out,
    };
    let acro = match resolve_dict(doc, catalog.get(b"AcroForm").ok()) {
        Some(acro) => acro,
        None => return out,
    };
    let fields = match resolve_array(doc, acro.get(b"Fields").ok()) {
        Some(fields) => fields,
        None => return out,
    };
    for field in fields {
        if let Ok(id) = field.as_reference() {
            walk_field(doc, id, &mut out, 0);
        }
    }
    out
}

fn walk_field(doc: &Document, id: ObjectId, out: &mut BTreeSet<ObjectId>, depth: usize) {
    if depth > 32 || !out.insert(id) {
        return;
    }
    let dict = match doc.get_dictionary(id) {
        Ok(dict) => dict.clone(),
        Err(_) => return,
    };
    if let Some(kids) = resolve_array(doc, dict.get(b"Kids").ok()) {
        for kid in kids {
            if let Ok(kid_id) = kid.as_reference() {
                walk_field(doc, kid_id, out, depth + 1);
            }
        }
    }
}

fn remove_field_references(doc: &mut Document, remove: &BTreeSet<ObjectId>) {
    for object in doc.objects.values_mut() {
        let dict = match object {
            Object::Dictionary(dict) => dict,
            Object::Stream(stream) => &mut stream.dict,
            _ => continue,
        };
        for key in [b"Fields".as_slice(), b"Kids"] {
            if let Ok(Object::Array(items)) = dict.get_mut(key) {
                items.retain(|entry| match entry.as_reference() {
                    Ok(id) => !remove.contains(&id),
                    Err(_) => true,
                });
            }
        }
    }
}

/// Drops field nodes that were left with an empty `/Kids` array, so a fully
/// flattened form does not keep a skeleton tree behind.
fn prune_empty_field_nodes(doc: &mut Document) {
    for _ in 0..4 {
        let mut empty: BTreeSet<ObjectId> = BTreeSet::new();
        for (id, object) in &doc.objects {
            let dict = match object {
                Object::Dictionary(dict) => dict,
                Object::Stream(stream) => &stream.dict,
                _ => continue,
            };
            if !dict.has(b"FT") {
                continue;
            }
            if let Some(kids) = resolve_array(doc, dict.get(b"Kids").ok()) {
                if kids.is_empty() {
                    empty.insert(*id);
                }
            }
        }
        if empty.is_empty() {
            return;
        }
        remove_field_references(doc, &empty);
    }
}

fn finish_acroform(doc: &mut Document) {
    let catalog_id = match doc.trailer.get(b"Root").and_then(|value| value.as_reference()) {
        Ok(id) => id,
        Err(_) => return,
    };
    let acro_value = doc
        .get_dictionary(catalog_id)
        .ok()
        .and_then(|catalog| catalog.get(b"AcroForm").ok())
        .cloned();
    let empty = match &acro_value {
        Some(Object::Reference(id)) => doc
            .get_dictionary(*id)
            .ok()
            .map(|acro| {
                resolve_array(doc, acro.get(b"Fields").ok())
                    .map(|fields| fields.is_empty())
                    .unwrap_or(true)
            })
            .unwrap_or(true),
        Some(Object::Dictionary(acro)) => resolve_array(doc, acro.get(b"Fields").ok())
            .map(|fields| fields.is_empty())
            .unwrap_or(true),
        _ => return,
    };
    if empty {
        if let Some(Object::Reference(id)) = acro_value {
            doc.objects.remove(&id);
        }
        if let Ok(catalog) = doc.get_dictionary_mut(catalog_id) {
            catalog.remove(b"AcroForm");
        }
        return;
    }
    match acro_value {
        Some(Object::Reference(id)) => {
            if let Ok(Object::Dictionary(acro)) = doc.get_object_mut(id) {
                acro.remove(b"NeedAppearances");
            }
        }
        Some(Object::Dictionary(_)) => {
            if let Ok(catalog) = doc.get_dictionary_mut(catalog_id) {
                if let Ok(Object::Dictionary(acro)) = catalog.get_mut(b"AcroForm") {
                    acro.remove(b"NeedAppearances");
                }
            }
        }
        _ => {}
    }
}

/// Flattens annotations, forms and appearances of `input` into `output`.
pub fn flatten_pdf(
    input: &Path,
    output: &Path,
    options: &FlattenOptions,
    progress: &ProgressCallback,
    cancel: &CancelToken,
) -> PdfResult<FlattenReport> {
    cancel.check()?;
    let mut doc = docutil::load_document(input, None)?;
    let reporter = ProgressReporter::new(progress);
    let field_ids = collect_field_ids(&doc);
    let pages: Vec<(u32, ObjectId)> = doc.get_pages().into_iter().collect();
    let total = pages.len() as u64;
    let mut report = FlattenReport::default();
    let mut flattened_fields: BTreeSet<ObjectId> = BTreeSet::new();
    let mut touched_pages: BTreeSet<u32> = BTreeSet::new();

    for (index, (page_number, page_id)) in pages.iter().enumerate() {
        cancel.check()?;
        reporter.emit_step("flatten.page", index as u64, total);
        let (storage, entries) = match annots_storage(&doc, *page_id) {
            Some(value) => value,
            None => continue,
        };
        let mut remove_ids: BTreeSet<ObjectId> = BTreeSet::new();
        let mut remove_indices: BTreeSet<usize> = BTreeSet::new();
        let mut burned_here = 0u32;

        for (entry_index, entry) in entries.iter().enumerate() {
            let annot_id = entry.as_reference().ok();
            let annot = match entry {
                Object::Reference(id) => match doc.get_dictionary(*id) {
                    Ok(dict) => dict.clone(),
                    Err(_) => continue,
                },
                Object::Dictionary(dict) => dict.clone(),
                _ => continue,
            };
            let is_field = annot_id
                .map(|id| field_ids.contains(&id))
                .unwrap_or(false);
            let flatten = options.annotations || (options.forms && is_field);

            if flatten {
                let mut appearance = appearance_stream_id(&doc, &annot);
                if appearance.is_none() && is_field {
                    if let Some(id) = annot_id {
                        appearance = generated_appearance(&mut doc, id, &annot);
                    }
                }
                match appearance {
                    Some(appearance_id) => {
                        burn_annotation(&mut doc, *page_id, &annot, appearance_id)?;
                        match annot_id {
                            Some(id) => {
                                remove_ids.insert(id);
                                if is_field {
                                    flattened_fields.insert(id);
                                }
                            }
                            None => {
                                remove_indices.insert(entry_index);
                            }
                        }
                        report.annotations_flattened += 1;
                        burned_here += 1;
                    }
                    None => report.warnings.push(format!(
                        "Page {page_number}: the {} annotation has no appearance stream and was left in place.",
                        annotation_label(&annot)
                    )),
                }
            } else if options.appearances && is_field && appearance_stream_id(&doc, &annot).is_none() {
                let generated = annot_id.and_then(|id| generated_appearance(&mut doc, id, &annot));
                if generated.is_none() {
                    report.warnings.push(format!(
                        "Page {page_number}: a form field without an appearance could not be given one."
                    ));
                }
            }
        }

        if !remove_ids.is_empty() || !remove_indices.is_empty() {
            filter_annots(&mut doc, *page_id, &storage, &remove_ids, &remove_indices)?;
        }
        if burned_here > 0 {
            touched_pages.insert(*page_number);
        }
    }

    if !flattened_fields.is_empty() {
        remove_field_references(&mut doc, &flattened_fields);
        prune_empty_field_nodes(&mut doc);
    }
    finish_acroform(&mut doc);

    report.fields_flattened = flattened_fields.len() as u32;
    report.pages_touched = touched_pages.len() as u32;
    cancel.check()?;
    docutil::save_document(&mut doc, output, true)?;
    Ok(report)
}
